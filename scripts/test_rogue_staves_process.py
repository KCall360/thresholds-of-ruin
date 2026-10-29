"""Actual Text staff/wand physics, charge ownership and source-free paid work."""
import copy
import json
import tomllib
import unittest

from fruit_catalog_package import remove_sources
from process_harness import ProcessTestCase, SPECTATOR_TOKEN
from rogue_staves_package import ROOT, PRODUCTION, fixture, prepare
from status_condition_package import checkpoint, observation


def inventory(frame, item):
    return next(row for row in observation(frame)["inventory"] if row["id"] == item)


def interaction(frame, item):
    return next(row for row in observation(frame)["interactions"]["inventory"] if row["item"] == item)


class RogueStavesFixtureTests(unittest.TestCase):
    def test_fixture_changes_only_explicit_actor_geometry_and_finite_stock(self):
        manifest, region = fixture()
        base = tomllib.loads((ROOT / "scenarios/tests/magic-items/scenario.toml").read_text())
        production = tomllib.loads((PRODUCTION / "scenario.toml").read_text())
        owner = copy.deepcopy(base["characters"][0])
        owner["turn_ticks"] = 100
        self.assertEqual(manifest["characters"], [owner])
        self.assertEqual(manifest["creatures"]["species"]["figure"],
                         base["creatures"]["species"]["figure"])
        self.assertEqual(manifest["assets"], production["assets"])
        self.assertEqual(manifest["themes"], production["themes"])
        self.assertEqual(manifest["modules"], ["catalog/items.toml", "catalog/creatures.toml",
                                              "catalog/bodies.toml"])
        self.assertNotIn("archetypes", manifest)
        self.assertNotIn("appearance_pools", manifest)
        self.assertNotIn("floor_activation", manifest)
        self.assertEqual(region["items"], [
            {"id": 20, "at": [1, 2, 0], "archetype": "rogue.identify_wand", "quantity": 2},
            {"id": 30, "at": [1, 2, 0], "archetype": "rogue.wand_nothing"},
            {"id": 31, "at": [1, 2, 0], "archetype": "rogue.wand_nothing"},
            {"id": 32, "at": [1, 2, 0], "archetype": "rogue.wand_fire"}])


class RogueStavesProcesses(ProcessTestCase):
    def snapshot(self, client=None):
        frame = self.request(client or self.probe, {"type": "snapshot"})
        self.assertIsNone(frame["error"])
        return frame

    def settle(self):
        for _ in range(24):
            frame = self.snapshot()
            combat = observation(frame)["combat"]
            if (observation(frame)["ready"] and not frame["intentions"]
                    and combat["preparation_remaining"] is None and int(combat["recovery_remaining"]) == 0):
                return frame
            peer = self.snapshot(self.peer)
            if observation(peer)["ready"] and peer["readiness"]["admission"]:
                self.assertIsNone(self.act(self.peer, {"type": "wait"})["error"])
        self.fail("Magical implement action did not settle within bounded ordinary peer waits")

    def completed(self, text, command):
        output = text.command(command)
        self.assertNotIn("Server error", output)
        self.assertNotIn("Action: Failed.", output)
        return self.settle()

    def test_text_staff_physical_hit_charge_exhaustion_and_pending_source_free_reopen(self):
        package = prepare(self)
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=4)
        self.probe, initial = self.client(observe=True)
        self.peer, _ = self.client(actor=3)
        text, _ = self.text_client()
        # Fixed seed4 vector, independently evaluated once from the public hash
        # recipe and full14-effect catalog: Nothing→walnut staff, Fire→nickel wand.
        self.assertEqual(sorted(row["item"]["name"] for row in observation(initial)["ground_items"]
                                if row["item"]["class"] == "wand"),
                         ["nickel wand", "walnut staff", "walnut staff"])
        for ground in observation(initial)["ground_items"]:
            self.completed(text, "take #" + ground["item"]["id"])
        carried = self.snapshot()
        staffs = [row for row in observation(carried)["inventory"] if row["name"] == "walnut staff"]
        self.assertEqual(len(staffs), 2)
        selected, twin = [row["id"] for row in staffs]
        wand, = [row["id"] for row in observation(carried)["inventory"] if row["name"] == "nickel wand"]
        scroll, = [row["id"] for row in observation(carried)["inventory"] if row["class"] == "scroll"]
        for item in (selected, twin, wand):
            self.assertFalse(inventory(carried, item)["identified"])
            self.assertIsNone(interaction(carried, item)["charges"])
            self.assertIsNone(interaction(carried, item)["known_equipment"])
        initial_saved = checkpoint(self, self.probe)
        counts = {key: initial_saved["items"][key]["charges"] for key in ("30", "31", "32")}
        self.assertTrue(all(3 <= count <= 7 for count in counts.values()))
        wielded = self.completed(text, "wield #" + selected)
        self.assertEqual(interaction(wielded, selected)["equipped_slot"], 0)
        before = checkpoint(self, self.probe)
        self.assertEqual({key: before["items"][key]["charges"] for key in counts}, counts)
        target_health = int(observation(self.snapshot(self.peer))["combat"]["hp"])
        output = text.command("attack practice target")
        self.assertNotIn("Action: Failed.", output)
        started = self.snapshot()
        saved = checkpoint(self, self.probe)
        pending = saved["actors"]["1"]["pending"]
        self.assertIsNotNone(pending)
        plan = pending["plan"]
        self.assertEqual(plan["kind"], "ability")
        self.assertEqual(plan["ability"], "basic_melee")
        self.assertEqual(plan["weapon"]["kind"], "equipped")
        self.assertEqual(plan["weapon"]["socket"], 0)
        self.assertFalse(plan["weapon"]["secondary"])
        source = str(plan["weapon"]["item"])
        self.assertIn(source, ("30", "31"))
        self.assertEqual(saved["items"][source]["spec"]["equipment"]["attack"]["damage"]["components"][0]["amount"],
                         {"type": "rolled", "count": 2, "sides": 3, "bonus": 0})
        self.assertEqual({key: saved["items"][key]["charges"] for key in counts}, counts)
        self.assertFalse(inventory(started, selected)["identified"])
        spectator, watched = self.client(SPECTATOR_TOKEN, observe=True)
        for frame in (started, watched):
            public = json.dumps(frame)
            for secret in ('"forms"', '"rogue.wand_nothing"', '"initial_charges"'):
                self.assertNotIn(secret, public)
            self.assertIsNone(interaction(frame, selected)["charges"])
        spectator.stop(save=False)
        text.stop(save=False)
        paused = self.snapshot()
        if not any(row["phase"] == "paused" for row in paused["intentions"]):
            paused = self.frame(self.probe, lambda frame: any(
                row["phase"] == "paused" for row in frame["intentions"]))
        frozen = checkpoint(self, self.probe)
        self.assertFalse(frozen["actors"]["1"]["pending"]["active"])
        for field in ("plan", "hold", "origin", "intention", "progress", "item_payment", "remaining"):
            self.assertEqual(frozen["actors"]["1"]["pending"][field], pending[field], field)
        self.assertEqual(frozen["items"], saved["items"])
        self.assertEqual(frozen["rules_rng"], saved["rules_rng"])
        self.probe.stop(save=False); self.peer.stop(save=False); server.stop(save=False)
        remove_sources(self, package)
        self.server("--checkpoint-interval", 1, seed=None)
        self.probe, reopened = self.client(observe=True)
        self.peer, _ = self.client(actor=3)
        self.assertDisclosedStateEqual(reopened["state"], paused["state"])
        self.assertEqual(reopened["intentions"], paused["intentions"])
        self.assertEqual(checkpoint(self, self.probe), frozen)
        owner, _ = self.client()
        self.assertEqual(checkpoint(self, self.probe), frozen, "control cannot resume paid work")
        self.assertIsNone(self.command(owner, {"type": "resume_intention"})["error"])
        hit = self.settle()
        owner.stop(save=False)
        self.assertLess(int(observation(self.snapshot(self.peer))["combat"]["hp"]), target_health)
        after_hit = checkpoint(self, self.probe)
        self.assertEqual({key: after_hit["items"][key]["charges"] for key in counts}, counts)
        self.assertFalse(inventory(hit, selected)["identified"])
        self.assertIsNone(interaction(hit, selected)["charges"])
        text, _ = self.text_client()
        self.completed(text, "read #" + scroll)
        self.assertIsNotNone(observation(self.snapshot())["interactions"]["identification"])
        self.assertNotIn("Server error", text.command("identify #" + selected))
        known = self.snapshot()
        for item in (selected, twin):
            self.assertEqual(inventory(known, item)["name"], "staff of nothing")
            self.assertTrue(inventory(known, item)["identified"])
        self.assertEqual(interaction(known, selected)["charges"], counts[source])
        self.assertIsNone(interaction(known, twin)["charges"], "type discovery cannot invent sibling charges")
        spectator, watched = self.client(SPECTATOR_TOKEN, observe=True)
        self.assertIsNone(interaction(watched, selected)["charges"])
        self.assertIsNone(interaction(watched, selected)["known_equipment"])
        spectator.stop(save=False)
        for remaining in range(counts[source] - 1, -1, -1):
            before_zap = checkpoint(self, self.probe)
            exhausted = self.completed(text, "zap #" + selected)
            after_zap = checkpoint(self, self.probe)
            self.assertEqual(interaction(exhausted, selected)["charges"], remaining)
            self.assertEqual(after_zap["items"][source]["charges"], remaining)
            self.assertEqual(after_zap["rules_rng"], before_zap["rules_rng"], "Nothing must draw no effect RNG")
            for other in counts:
                if other != source:
                    self.assertEqual(after_zap["items"][other]["charges"], counts[other])
        self.assertTrue(interaction(exhausted, selected)["zappable"])
        zero = checkpoint(self, self.probe)
        self.completed(text, "zap #" + selected)
        empty_zap = checkpoint(self, self.probe)
        self.assertGreater(empty_zap["tick"], zero["tick"], "empty devices retain ordinary paid Zap")
        self.assertEqual(empty_zap["rules_rng"], zero["rules_rng"])
        self.assertEqual(empty_zap["items"], zero["items"])
        target_health = int(observation(self.snapshot(self.peer))["combat"]["hp"])
        empty_hit = self.completed(text, "attack practice target")
        self.assertLess(int(observation(self.snapshot(self.peer))["combat"]["hp"]), target_health)
        self.assertEqual(interaction(empty_hit, selected)["charges"], 0)
        self.assertEqual(checkpoint(self, self.probe)["items"][source]["charges"], 0)
        self.completed(text, "remove #" + selected)
        wand_wielded = self.completed(text, "wield #" + wand)
        self.assertEqual(interaction(wand_wielded, wand)["equipped_slot"], 0)
        before_wand = checkpoint(self, self.probe)
        target_health = int(observation(self.snapshot(self.peer))["combat"]["hp"])
        wand_hit = self.completed(text, "attack practice target")
        self.assertEqual(int(observation(self.snapshot(self.peer))["combat"]["hp"]), target_health - 1)
        final = checkpoint(self, self.probe)
        self.assertEqual(final["items"]["32"]["charges"], before_wand["items"]["32"]["charges"])
        self.assertFalse(inventory(wand_hit, wand)["identified"])
        self.assertIsNone(interaction(wand_hit, wand)["charges"])
        self.assertFalse(wand_hit["state"]["wizard_game"])
        # The real Adventure parser/rendering path, not just headless DTOs or
        # --script output, must use disclosed physical nouns and category labels.
        text.stop(save=False)
        adventure, _ = self.adventure()
        listed = self.say(adventure, "inventory")
        self.assertIn("staff of nothing", listed)
        self.assertIn("nickel wand", listed)
        staff_description = self.say(adventure, "examine first staff")
        self.assertIn("is a staff.", staff_description)
        self.assertNotIn("is a wand.", staff_description)
        # The identified scroll legitimately also contains the noun "wand";
        # answer the actual ambiguity rather than hiding it from the fixture.
        clarification = self.say(adventure, "examine wand")
        self.assertIn("Which do you mean", clarification)
        self.assertIn("scroll of identify ring or wand", clarification)
        self.assertIn("nickel wand", clarification)
        wand_description = self.say(adventure, "nickel wand")
        self.assertIn("nickel wand", wand_description)
        self.assertIn("is a wand.", wand_description)
        self.assertNotIn("staff of nothing", wand_description)
        self.say(adventure, "all skips staves")
        staff_options = self.say(adventure, "options")
        self.assertIn("wands and staves", staff_options)
        self.say(adventure, "all skips wands")
        self.assertEqual(self.say(adventure, "options"), staff_options)
        self.assertEqual(checkpoint(self, self.probe), final,
                         "inventory, examination and local settings cannot advance simulation")


if __name__ == "__main__":
    unittest.main()
