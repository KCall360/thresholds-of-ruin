"""Real opt-in standalone throwing stacks: exact source, payment and recovery.

No tests import another test module. Shared dagger/arrow owners retain their
existing expectations; this owner reaches the newly supported carried stack.
"""
import copy
import json
import tomllib
import unittest

from fruit_catalog_package import remove_sources
from process_harness import ProcessTestCase, SPECTATOR_TOKEN, non_graphical, resource
from rogue_throwing_package import MISSILES, PRODUCTION, fixture, prepare
from status_condition_package import checkpoint, observation
from stream_relay import StreamRelay


def inventory(frame, name):
    return next(row for row in observation(frame)["inventory"] if row["name"] == name)


def saved_items(saved, identity):
    return [(identity_id, row) for identity_id, row in saved["items"].items()
            if row["spec"]["identity"] == identity]


class RogueThrowingFixtureTests(unittest.TestCase):
    def test_fixture_keeps_ordinary_kit_and_expertise_with_three_units_of_each_new_weapon(self):
        production = tomllib.loads((PRODUCTION / "scenario.toml").read_text(encoding="utf-8"))
        manifest, region = fixture()
        selected = copy.deepcopy(production["characters"][0])
        selected["anchor"] = "1/start"
        selected["known_identities"] += ["rogue.dart", "rogue.shuriken"]
        selected["loadout"] += [{"archetype": "rogue.dart", "quantity": 3},
                                {"archetype": "rogue.shuriken", "quantity": 3}]
        self.assertEqual(manifest["characters"], [selected])
        self.assertEqual(manifest["modules"], production["modules"])
        self.assertEqual(region["actors"], [{"id": 3, "at": [2, 2, 0],
                                             "archetype": "rogue.emu", "controller": "external"}])
        self.assertNotIn("floor_activation", manifest)
        self.assertNotIn("archetypes", manifest)
        self.assertEqual(MISSILES, ("rogue.dart", "rogue.shuriken"))


class RogueThrowingProcesses(ProcessTestCase):
    graphical = True

    def setup_encounter(self):
        self.package = prepare(self)
        self.game = self.server("--checkpoint-interval", 1, scenario=self.package, seed=42)
        self.probe, first = self.client(observe=True)
        self.peer, target = self.client(actor=3)
        self.target_health = int(observation(target)["combat"]["hp"])
        self.replies = []
        return first

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
        self.fail("Throw did not settle within the bounded ordinary peer waits")

    def await_throw_started(self, name, previous_quantity):
        # Native/Text command acknowledgements need not mean remote execution.
        def started_or_released(frame):
            return (inventory(frame, name)["quantity"] == str(previous_quantity - 1)
                    or any(row["phase"] == "started" for row in frame["intentions"]))
        current = self.snapshot()
        return current if started_or_released(current) else self.frame(self.probe, started_or_released)

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
        rows = [row for row in self.replies[start:] if row["type"] == kind]
        self.assertEqual(len(rows), 1, (kind, rows))
        return rows[0]

    def learn(self, text):
        self.assertIn("Starting skills", text.command("advancement"))
        self.assertIn("Confirm", text.command("advancement purchase 2 advancement expertise rogue.knife"))
        self.assertNotIn("Server error", text.command("advancement confirm"))
        learned = self.settle()
        self.assertIn({"expertise": {"id": "rogue.knife", "name": "Knife"},
                       "status": "active", "source_count": 1},
                      observation(learned)["combat"]["own_stats"]["expertises"])
        return learned

    def choose(self, text, name, direction):
        start = len(self.replies)
        self.assertIn(name, text.command("throw"))
        choices = self.reply("projectile_choices", start)["page"]["choices"]
        item = inventory(self.snapshot(), name)
        matches = [(index, choice) for index, choice in enumerate(choices)
                   if choice["source"]["item"] == item["id"]]
        self.assertEqual(len(matches), 1, choices)
        index, choice = matches[0]
        self.assertEqual(choice, {"kind": "throw", "source": {
            "kind": "carried", "item": item["id"], "socket": 10}})
        self.assertIn("Which direction?", text.command(str(index + 1)))
        start = len(self.replies)
        self.assertIn("Use this exact weapon action?", text.command(direction))
        preview = self.reply("ability_weapon_preview", start)["preview"]
        self.assertEqual(preview["ability"], "rogue.throw")
        self.assertEqual(preview["invocation"]["aim"]["delivery"], choice)
        self.assertEqual(preview["invocation"]["aim"]["direction"], direction)
        self.assertGreater(sum(cost["start"] + cost["resolution"]
                               for cost in preview["cost"].values() if cost), 0)
        return preview

    @non_graphical
    def test_text_carried_dart_and_shuriken_preserve_exact_pending_payload_after_source_deletion(self):
        initial = self.setup_encounter()
        self.assertFalse(initial["state"]["wizard_game"])
        owner, _ = self.client()
        refused = self.act(owner, {"type": "equip", "item": inventory(initial, "dart")["id"], "slot": 10})
        self.assertIsNotNone(refused["error"], "a whole multi-unit stack cannot be wielded")
        self.assertEqual(self.snapshot()["state"], initial["state"])
        owner.stop()
        text = self.text()
        start = len(self.replies)
        self.assertIn("No choices", text.command("throw"))
        self.assertEqual(self.reply("projectile_choices", start)["page"]["choices"], [])
        text.command("cancel")
        learned = self.learn(text)
        self.choose(text, "dart", "east")
        self.assertNotIn("Server error", text.command("no"))
        self.assertEqual(self.snapshot()["state"], learned["state"])
        terms = self.choose(text, "dart", "east")
        self.assertNotIn("Server error", text.command("yes"))
        started = self.snapshot()
        self.assertEqual(inventory(started, "dart")["quantity"], "3")
        focus = terms["cost"]["focus"]
        self.assertEqual(resource(started, "focus")["reserved"], focus["resolution"])
        self.assertEqual(resource(started, "focus")["balance"],
                         resource(learned, "focus")["balance"] - focus["start"])
        spectator, watched = self.client(token=SPECTATOR_TOKEN, observe=True)
        public = json.dumps([started, watched, self.replies])
        for private_field in ('"thrown_payload"', '"throw_carried"', '"allocation"'):
            self.assertNotIn(private_field, public)
        spectator.stop()
        saved = checkpoint(self, self.probe)
        pending = saved["actors"]["1"]["pending"]
        self.assertIsNotNone(pending)
        plan = pending["plan"]
        self.assertEqual(plan["kind"], "projectile")
        self.assertEqual(plan["source"]["kind"], "throw_carried")
        frozen = plan["source"]["allocation"]["target"]
        self.assertTrue(frozen["expert"])
        payload = frozen["spec"]["thrown_payload"]
        self.assertEqual(payload["recovery"], "consumed_on_hit")
        self.assertIsNone(frozen["spec"]["launcher"])
        self.assertEqual(payload["damage"]["components"][0]["amount"],
                         {"type": "rolled", "count": 1, "sides": 3, "bonus": 0})
        source_id = str(plan["source"]["item"])
        self.assertEqual(saved["items"][source_id]["quantity"], 3)
        self.assertEqual(sum(row["quantity"] for row in saved["items"][source_id]["holds"]), 1)
        # Ordinary owner disconnect queues a pause, not an implicit resume.
        # Capture that authoritative boundary before shutdown/restart.
        text.stop()
        paused = self.snapshot()
        if not any(row["phase"] == "paused" for row in paused["intentions"]):
            paused = self.frame(self.probe, lambda frame: any(
                row["phase"] == "paused" for row in frame["intentions"]))
        paused_saved = checkpoint(self, self.probe)
        retained = paused_saved["actors"]["1"]["pending"]
        self.assertFalse(retained["active"])
        for field in ("plan", "hold", "origin", "intention", "progress", "item_payment", "remaining"):
            self.assertEqual(retained[field], pending[field], field)
        self.assertEqual(paused_saved["items"], saved["items"])
        self.assertEqual(paused_saved["rules_rng"], saved["rules_rng"])
        saved = paused_saved
        self.probe.stop(); self.peer.stop(); self.game.stop(save=False)
        remove_sources(self, self.package)
        self.game = self.server("--checkpoint-interval", 1, seed=None)
        self.probe, restored = self.client(observe=True)
        self.peer, _ = self.client(actor=3)
        self.assertEqual(restored["state"], paused["state"])
        self.assertEqual(restored["intentions"], paused["intentions"])
        self.assertEqual(checkpoint(self, self.probe), saved)
        owner, _ = self.client()
        self.assertEqual(checkpoint(self, self.probe), saved, "acquiring control must not resume")
        resumed = self.command(owner, {"type": "resume_intention"})
        self.assertIsNone(resumed["error"])
        if not observation(resumed)["combat"]["preparation_active"]:
            resumed = self.frame(owner, lambda frame: observation(frame)["combat"]["preparation_active"]
                and any(row["phase"] == "started" and row["intention"] == paused["intentions"][0]["intention"]
                        for row in frame["intentions"]))
        self.assertTrue(observation(resumed)["combat"]["preparation_active"])
        self.assertTrue(any(row["phase"] == "started"
                            and row["intention"] == paused["intentions"][0]["intention"]
                            for row in resumed["intentions"]))
        thrown = self.settle()
        owner.stop()
        text = self.text()
        self.assertEqual(inventory(thrown, "dart")["quantity"], "2")
        self.assertEqual(resource(thrown, "focus")["reserved"], 0)
        target = self.request(self.peer, {"type": "snapshot"})
        self.assertLess(int(observation(target)["combat"]["hp"]), self.target_health)
        hit = checkpoint(self, self.probe)
        self.assertEqual(sum(row["quantity"] for _, row in saved_items(hit, "rogue.dart")), 2)
        self.assertFalse(any(row["item"]["name"] == "dart" for row in observation(thrown)["ground_items"]))
        self.assertEqual(hit["items"][source_id]["holds"], [])
        shuriken_terms = self.choose(text, "shuriken", "west")
        self.assertNotIn("Server error", text.command("yes"))
        pending_shuriken = checkpoint(self, self.probe)["actors"]["1"]["pending"]["plan"]
        shuriken_payload = pending_shuriken["source"]["allocation"]["target"]["spec"]["thrown_payload"]
        self.assertEqual(shuriken_payload["damage"]["components"][0]["amount"],
                         {"type": "rolled", "count": 2, "sides": 4, "bonus": 0})
        missed = self.settle()
        self.assertEqual(inventory(missed, "shuriken")["quantity"], "2")
        self.assertEqual(int(observation(self.request(self.peer, {"type": "snapshot"}))["combat"]["hp"]),
                         int(observation(target)["combat"]["hp"]))
        ground, = [row for row in observation(missed)["ground_items"] if row["item"]["name"] == "shuriken"]
        self.assertEqual(ground["item"]["quantity"], "1")
        self.assertNotEqual(ground["item"]["id"], inventory(missed, "shuriken")["id"])
        final = checkpoint(self, self.probe)
        self.assertEqual(sum(row["quantity"] for _, row in saved_items(final, "rogue.shuriken")), 3)
        self.assertTrue(all(not row["holds"] for _, row in saved_items(final, "rogue.shuriken")))
        # The ordinary Warrior has four Focus, not an unlimited fixture pool.
        # Let actual actor-active recovery fund the third two-Focus throw.
        required = sum(shuriken_terms["cost"]["focus"][key] for key in ("start", "resolution"))
        self.assertEqual(required, 2)
        for _ in range(8):
            if resource(self.snapshot(), "focus")["available"] >= required:
                break
            self.assertNotIn("Server error", text.command("wait"))
            self.settle()
        self.assertGreaterEqual(resource(self.snapshot(), "focus")["available"], required)
        self.assertEqual(inventory(self.snapshot(), "shuriken")["quantity"], "2")
        target_before_shuriken = self.request(self.peer, {"type": "snapshot"})
        self.assertEqual(self.choose(text, "shuriken", "east")["cost"], shuriken_terms["cost"])
        confirmed = text.command("yes")
        self.assertNotIn("Server error", confirmed)
        self.assertNotIn("Action: Failed.", confirmed)
        self.await_throw_started("shuriken", 2)
        shuriken_hit = self.settle()
        self.assertEqual(inventory(shuriken_hit, "shuriken")["quantity"], "1")
        self.assertLess(int(observation(self.request(self.peer, {"type": "snapshot"}))["combat"]["hp"]),
                        int(observation(target_before_shuriken)["combat"]["hp"]))
        self.assertLess(int(observation(self.request(self.peer, {"type": "snapshot"}))["combat"]["hp"]),
                        self.target_health)
        final = checkpoint(self, self.probe)
        self.assertEqual(sum(row["quantity"] for _, row in saved_items(final, "rogue.shuriken")), 2)
        self.assertEqual(sum(int(row["item"]["quantity"]) for row in observation(shuriken_hit)["ground_items"]
                             if row["item"]["name"] == "shuriken"), 1)
        for _ in range(2):
            self.assertEqual(self.snapshot()["state"], shuriken_hit["state"])
        self.assertEqual(checkpoint(self, self.probe), final)
        text.stop(); self.probe.stop(); self.peer.stop(); self.game.stop()
        self.server("--checkpoint-interval", 1, seed=None)
        self.probe, reopened = self.client(observe=True)
        self.assertEqual(reopened["state"], shuriken_hit["state"])
        self.assertEqual(checkpoint(self, self.probe), final)


    def test_native_carried_stack_picker_cancels_without_holding_and_confirms_one_unit(self):
        self.setup_encounter()
        setup = self.text()
        self.learn(setup)
        setup.stop()
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        initial = self.ascii_frame(window, lambda frame: frame["state"] is not None and not frame["busy"])
        self.assertTrue(initial["window_open"])
        native = self.native_keys(window)
        before = checkpoint(self, self.probe)
        dart = inventory(self.snapshot(), "dart")["id"]

        def press(name, key):
            return self.native_press(window, native, name, key)

        def choose():
            menu = press("z", "abilities")
            index = [row["id"] for row in menu["ability_choices"]].index("rogue.throw")
            for _ in range(index):
                press("Down", "down")
            sources = press("Return", "enter")
            rows = sources["projectile_selection"]["page"]["choices"]
            self.assertTrue(any("dart" in row for row in sources["projectile_rows"]))
            self.assertTrue(any("shuriken" in row for row in sources["projectile_rows"]))
            index = next(index for index, row in enumerate(rows) if row["source"]["item"] == dart)
            self.assertEqual(rows[index], {"kind": "throw", "source": {
                "kind": "carried", "item": dart, "socket": 10}})
            for _ in range(index):
                press("Down", "down")
            press("Return", "enter")
            preview = press("Right", "right")
            self.assertEqual(preview["weapon_preview"]["preview"]["ability"], "rogue.throw")
            return preview

        choose()
        cancelled = press("Escape", "escape")
        self.assertIsNone(cancelled["weapon_preview"])
        self.assertEqual(checkpoint(self, self.probe), before)
        choose()
        self.assertTrue(press("Return", "enter")["window_open"])
        self.await_throw_started("dart", 3)
        final = self.settle()
        self.assertEqual(inventory(final, "dart")["quantity"], "2")
        saved = checkpoint(self, self.probe)
        self.assertEqual(sum(row["quantity"] for _, row in saved_items(saved, "rogue.dart")), 2)
        self.assertTrue(all(not row["holds"] for _, row in saved_items(saved, "rogue.dart")))


if __name__ == "__main__":
    unittest.main()
