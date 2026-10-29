"""Production spear: ordinary expertise, frozen payload, durable physical recovery."""
import copy
import json
import tomllib
import unittest

from fruit_catalog_package import remove_sources
from process_harness import ProcessTestCase, SPECTATOR_TOKEN, resource
from rogue_spear_package import PRODUCTION, fixture, prepare
from status_condition_package import checkpoint, observation
from stream_relay import StreamRelay


def spear(frame):
    return next(row for row in observation(frame)["inventory"] if row["name"] == "spear")


class RogueSpearFixtureTests(unittest.TestCase):
    def test_fixture_retains_ordinary_kit_and_purchases_with_one_production_spear(self):
        production = tomllib.loads((PRODUCTION / "scenario.toml").read_text(encoding="utf-8"))
        manifest, region = fixture()
        selected = copy.deepcopy(production["characters"][0])
        selected["anchor"] = "1/start"
        selected["known_identities"].append("rogue.spear")
        selected["loadout"].append({"archetype": "rogue.spear", "quantity": 1})
        self.assertEqual(manifest["characters"], [selected])
        self.assertEqual(manifest["modules"], production["modules"])
        self.assertEqual(region["actors"], [{"id": 3, "at": [2, 2, 0],
                                             "archetype": "rogue.emu", "controller": "external"}])
        self.assertNotIn("floor_activation", manifest)
        self.assertNotIn("archetypes", manifest)


class RogueSpearProcesses(ProcessTestCase):
    def snapshot(self):
        return self.request(self.probe, {"type": "snapshot"})

    def settle(self):
        for _ in range(16):
            current = self.snapshot()
            if observation(current)["ready"] and not current["intentions"]:
                return current
            peer = self.request(self.peer, {"type": "snapshot"})
            self.assertTrue(observation(peer)["ready"], "external target must advance pending work")
            self.assertIsNone(self.act(self.peer, {"type": "wait"})["error"])
        self.fail("Spear action did not settle within bounded ordinary target waits")

    def text(self):
        relay = StreamRelay(self.address, on_server_message=self.replies.append)
        self.addCleanup(relay.close)
        upstream = self.address
        self.address = relay.address
        try:
            return self.text_client()[0]
        finally:
            self.address = upstream

    def reply(self, kind, start):
        replies = [row for row in self.replies[start:] if row["type"] == kind]
        self.assertEqual(len(replies), 1, (kind, replies))
        return replies[0]

    def choose(self, text, item, direction):
        start = len(self.replies)
        self.assertIn("spear", text.command("throw"))
        choices = self.reply("projectile_choices", start)["page"]["choices"]
        self.assertEqual(choices, [{"kind": "throw", "source": {
            "kind": "carried", "item": item, "socket": 10}}])
        self.assertIn("Which direction?", text.command("1"))
        start = len(self.replies)
        self.assertIn("Use this exact weapon action?", text.command(direction))
        terms = self.reply("ability_weapon_preview", start)["preview"]
        self.assertEqual(terms["ability"], "rogue.throw")
        self.assertEqual(terms["invocation"]["aim"]["delivery"], choices[0])
        self.assertEqual(terms["invocation"]["aim"]["direction"], direction)
        self.assertEqual(sum(cost["start"] + cost["resolution"]
                             for cost in terms["cost"].values() if cost), 2)
        return terms

    def test_text_spear_expertise_pending_source_free_reopen_and_exact_recoverable_hit(self):
        package = prepare(self)
        game = self.server("--checkpoint-interval", 1, scenario=package, seed=42)
        self.probe, initial = self.client(observe=True)
        self.peer, target = self.client(actor=3)
        target_health = int(observation(target)["combat"]["hp"])
        self.assertFalse(initial["state"]["wizard_game"])
        self.replies = []
        text = self.text()
        start = len(self.replies)
        self.assertIn("No choices", text.command("throw"))
        self.assertEqual(self.reply("projectile_choices", start)["page"]["choices"], [])
        text.command("cancel")
        self.assertEqual(self.snapshot()["state"], initial["state"])
        self.assertIn("Starting skills", text.command("advancement"))
        self.assertIn("Confirm", text.command("advancement purchase 2 advancement expertise rogue.spear"))
        self.assertNotIn("Server error", text.command("advancement confirm"))
        learned = self.settle()
        self.assertIn({"expertise": {"id": "rogue.spear", "name": "Spear"},
                       "status": "active", "source_count": 1},
                      observation(learned)["combat"]["own_stats"]["expertises"])
        item = spear(learned)["id"]
        self.choose(text, item, "east")
        text.command("no")
        self.assertEqual(self.snapshot()["state"], learned["state"])
        terms = self.choose(text, item, "east")
        confirmed = text.command("yes")
        self.assertNotIn("Server error", confirmed)
        self.assertNotIn("Action: Failed.", confirmed)
        started = self.snapshot()
        self.assertEqual(spear(started)["quantity"], "1")
        focus = terms["cost"]["focus"]
        self.assertEqual(resource(started, "focus")["reserved"], focus["resolution"])
        self.assertEqual(resource(started, "focus")["balance"],
                         resource(learned, "focus")["balance"] - focus["start"])
        spectator, watched = self.client(token=SPECTATOR_TOKEN, observe=True)
        public = json.dumps([started, watched, self.replies])
        for field in ('"thrown_payload"', '"throw_carried"', '"allocation"'):
            self.assertNotIn(field, public)
        spectator.stop()
        saved = checkpoint(self, self.probe)
        pending = saved["actors"]["1"]["pending"]
        self.assertIsNotNone(pending)
        plan = pending["plan"]
        self.assertEqual(plan["kind"], "projectile")
        self.assertEqual(plan["source"]["kind"], "throw_carried")
        frozen = plan["source"]["allocation"]["target"]
        self.assertTrue(frozen["expert"])
        self.assertIsNone(frozen["spec"]["launcher"])
        self.assertEqual(frozen["spec"]["thrown_payload"]["recovery"], "recoverable")
        self.assertEqual(frozen["spec"]["thrown_payload"]["damage"]["components"][0]["amount"],
                         {"type": "rolled", "count": 1, "sides": 6, "bonus": 0})
        source = str(plan["source"]["item"])
        self.assertEqual(saved["items"][source]["quantity"], 1)
        self.assertEqual(sum(hold["quantity"] for hold in saved["items"][source]["holds"]), 1)
        condition = saved["items"][source]["condition"]
        text.stop()
        paused = self.snapshot()
        if not any(row["phase"] == "paused" for row in paused["intentions"]):
            paused = self.frame(self.probe, lambda frame: any(
                row["phase"] == "paused" for row in frame["intentions"]))
        saved_paused = checkpoint(self, self.probe)
        retained = saved_paused["actors"]["1"]["pending"]
        self.assertFalse(retained["active"])
        for field in ("plan", "hold", "origin", "intention", "progress", "item_payment", "remaining"):
            self.assertEqual(retained[field], pending[field], field)
        self.assertEqual(saved_paused["items"], saved["items"])
        self.assertEqual(saved_paused["rules_rng"], saved["rules_rng"])
        self.probe.stop(); self.peer.stop(); game.stop(save=False)
        remove_sources(self, package)
        game = self.server("--checkpoint-interval", 1, seed=None)
        self.probe, reopened = self.client(observe=True)
        self.peer, _ = self.client(actor=3)
        self.assertEqual(reopened["state"], paused["state"])
        self.assertEqual(reopened["intentions"], paused["intentions"])
        self.assertEqual(checkpoint(self, self.probe), saved_paused)
        owner, _ = self.client()
        self.assertEqual(checkpoint(self, self.probe), saved_paused, "control must not resume paid work")
        self.assertIsNone(self.command(owner, {"type": "resume_intention"})["error"])
        thrown = self.settle()
        owner.stop()
        self.assertFalse(any(row["id"] == item for row in observation(thrown)["inventory"]))
        landed, = [row for row in observation(thrown)["ground_items"] if row["item"]["id"] == item]
        self.assertEqual(landed["item"]["quantity"], "1")
        self.assertLess(int(observation(self.request(self.peer, {"type": "snapshot"}))["combat"]["hp"]),
                        target_health)
        self.assertEqual(resource(thrown, "focus")["reserved"], 0)
        settled = checkpoint(self, self.probe)
        spear_rows = [(key, row) for key, row in settled["items"].items()
                      if row["spec"]["identity"] == "rogue.spear"]
        self.assertEqual([key for key, _ in spear_rows], [source])
        self.assertEqual(settled["items"][source]["quantity"], 1)
        self.assertEqual(settled["items"][source]["condition"], condition)
        self.assertEqual(settled["items"][source]["holds"], [])
        # Recover the exact durable unit by vacating its actual impact cell.
        text = self.text()
        self.assertIsNone(self.command(self.peer, {"type": "act", "action": {
            "type": "move", "direction": "north"}})["error"])
        self.assertNotIn("Server error", text.command("wait"))
        self.settle()
        self.assertNotIn("Server error", text.command("east"))
        at_landing = self.settle()
        self.assertTrue(next(row for row in observation(at_landing)["ground_items"]
                             if row["item"]["id"] == item)["reachable"])
        self.assertIn("You pick up ", text.command(f"take #{item}"))
        final = self.settle()
        self.assertEqual(spear(final)["id"], item)
        self.assertEqual(spear(final)["quantity"], "1")
        saved_final = checkpoint(self, self.probe)
        self.assertEqual(saved_final["items"][source]["condition"], condition)
        text.stop(); self.probe.stop(); self.peer.stop(); game.stop()
        self.server("--checkpoint-interval", 1, seed=None)
        self.probe, reopened = self.client(observe=True)
        self.assertEqual(reopened["state"], final["state"])
        self.assertEqual(checkpoint(self, self.probe), saved_final)


if __name__ == "__main__":
    unittest.main()
