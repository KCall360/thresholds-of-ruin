"""Ordinary scare-scroll quantities, curse removal and retained save authority.

This owns the new pickup lifecycle end to end. Existing magic-item process tests
retain ordinary read/identify payment, spectator knowledge, retry and source-free
pending-plan witnesses; this does not replace them. Simulation owns fear AI,
immunity, timing and stack provenance permutations.
"""
import copy
import json
import shutil
import subprocess
import tomllib
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT, SPECTATOR_TOKEN
from status_condition_package import checkpoint


def observation(frame):
    return frame["state"]["observation"]


class LivingDungeonProcesses(ProcessTestCase):
    def package(self):
        package = self.directory / "living-dungeon"
        shutil.copytree(ROOT / "scenarios/tests/magic-items", package)
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        manifest["archetypes"] = {
            "scare": {"name": "ward page", "identity": "fixture.ward_page",
                      "class": "scroll", "stackable": True,
                      "readable": {"control": {"kind": "nothing"}, "discovery": "never"},
                      "ground_scare": {"condition": "fixture.scared", "duration": 1000}},
            "uncurse": {"name": "cleansing page", "identity": "fixture.cleansing_page",
                        "class": "scroll", "stackable": True,
                        "readable": {"discovery": "never", "effects": [
                            {"kind": "alter_equipment", "selection": {"kind": "all"},
                             "alteration": "remove_curse"}]}},
        }
        manifest["appearance_pools"] = {}
        path.write_text(toml_source(manifest), encoding="utf-8")
        region_path = package / "regions/1.toml"
        region = tomllib.loads(region_path.read_text(encoding="utf-8"))
        region["items"] = [{"id": 10, "at": [1, 2, 0], "archetype": "scare", "quantity": 3},
                           {"id": 20, "at": [1, 2, 0], "archetype": "uncurse", "quantity": 1}]
        region_path.write_text(toml_source(region), encoding="utf-8")
        with (package / "rules.toml").open("a", encoding="utf-8") as output:
            output.write('''
[[overrides]]
op = "add"
path = ["conditions", "fixture.scared"]
value = { id = "fixture.scared", family = "fixture.scared", name = "Scared", lifecycle = "timed", modifiers = [], stacking = { kind = "keep" }, source_scoped = false, descriptors = [], capacity = "reject", timing = { kind = "actor_active", on_expiry = { kind = "remove" } }, restrictions = [], preparation = "preserve" }
''')
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return package

    def snapshot(self, client):
        frame = self.request(client, {"type": "snapshot"})
        self.assertIsNone(frame["error"])
        return frame

    def paid(self, client, action):
        before = self.snapshot(client)
        self.assertIsNone(self.act(client, action)["error"])
        after = self.snapshot(client)
        self.assertTrue(observation(after)["ready"])
        self.assertFalse(after["intentions"])
        self.assertGreater(int(observation(after)["tick"]), int(observation(before)["tick"]))
        self.assertFalse(after["state"]["wizard_game"])
        return after

    def item(self, frame, name, *, ground=False, quantity=None):
        values = ([row["item"] for row in observation(frame)["ground_items"]]
                  if ground else observation(frame)["inventory"])
        matches = [row for row in values if row["name"] == name
                   and (quantity is None or row["quantity"] == str(quantity))]
        self.assertEqual(len(matches), 1, matches)
        return matches[0]["id"]

    def test_adventure_cursed_pickup_narrates_crumble_not_transfer(self):
        self.server("--checkpoint-interval", 1, scenario=self.package())
        probe, initial = self.client(observe=True)
        text, _ = self.adventure()
        # Adventure resolves disclosed nouns/ordinals, not script-mode opaque
        # IDs. Take the whole original stack so the dropped cursed unit is the
        # only ground referent named ward page; no ambiguous stack selection.
        self.assertIn("pick up", self.say(text, "take 3 ward pages").lower())
        self.item(self.snapshot(probe), "ward page", quantity=3)
        self.assertIn("drop", self.say(text, "drop 1 ward page").lower())
        self.item(self.snapshot(probe), "ward page", ground=True, quantity=1)
        prose = self.say(text, "take 1 ward page").lower()
        self.assertIn("crumbles", prose)
        self.assertNotIn("pick up", prose)
        final = self.snapshot(probe)
        self.item(final, "ward page", quantity=2)
        self.assertFalse(any(row["item"]["name"] == "ward page"
                             for row in observation(final)["ground_items"]))

    def control_package(self, *, resistant_can_be_hit=False, resistant_spiritual=1000):
        """Two human peers pin check operands without AI or wizard mutations."""
        from copy import deepcopy
        package = self.package()
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        manifest["factions"] = {"reader": ["subjects"], "subjects": ["reader"]}
        for name, mode, condition in [("binding page", "hold_nearby", "fixture.held"),
                                       ("bewildering page", "arm_confusion", "fixture.confused")]:
            control = {"kind": mode, "condition": condition, "duration": 2000}
            if mode == "hold_nearby":
                control["radius"] = 2
            manifest["archetypes"][name.replace(" ", "_")] = {"name": name, "class": "scroll", "stackable": True,
                                            "readable": {"control": control, "discovery": "never"}}
        species = manifest["creatures"]["species"]["figure"]
        species["behavior"]["memory_ticks"] = 10000
        species["melee"].update(bonus=100, wind_up=25, recovery=25)
        species["melee"]["strikes"][0]["on_success"][0]["damage"]["components"][0]["amount"]["amount"] = 1
        species["modifiers"].append({"scope": "always", "op": {
            "kind": "add", "stat": {"kind": "health"}, "value": 1000}})
        reader = deepcopy(species)
        # Species base attributes obey attribute_cap=5; ordinary permanent
        # modifiers may raise derived attributes up to derived_attribute_cap.
        # This preserves a guaranteed Lore success against the low defense,
        # and guaranteed failure against the separately authored +1000 defense.
        reader["modifiers"].append({"scope": "always", "op": {
            "kind": "add", "stat": {"kind": "attribute", "attribute": "intellect"}, "value": 100}})
        resistant = deepcopy(species)
        resistant["modifiers"].extend({"scope": "always", "op": {
            "kind": "add", "stat": {"kind": "defense", "defense": defense}, "value": value}}
            for defense, value in [("spiritual", resistant_spiritual),
                                    ("physical", 0 if resistant_can_be_hit else 1000)])
        manifest["creatures"]["species"].update(reader=reader, resistant=resistant)
        character = manifest["characters"][0]
        character["creature"].update(species="reader", faction="reader")
        character["turn_ticks"] = 25
        # Eye above the ground owns the Hold geometry regression.
        character["body"] = {"cells": [[0, 0, 0], [0, 0, 1]], "eye": [0, 0, 1], "mass": 80}
        path.write_text(toml_source(manifest), encoding="utf-8")
        region_path = package / "regions/1.toml"
        region = tomllib.loads(region_path.read_text(encoding="utf-8"))
        region["actors"] = []
        for identity, name, species_id, at in [(2, "willing subject", "figure", [2, 2, 0]),
                                               (3, "resistant subject", "resistant", [1, 3, 0])]:
            creature = deepcopy(character["creature"])
            creature.update(name=name, species=species_id, faction="subjects")
            region["actors"].append({"id": identity, "at": at, "turn_ticks": 25,
                                      "controller": "external", "body": character["body"],
                                      "creature": creature})
        region["items"] = [{"id": identity, "at": [1, 2, 0], "carried_by": 1,
                            "archetype": name.replace(" ", "_"), "quantity": 1}
                           for identity, name in [(30, "binding page"), (31, "bewildering page")]]
        region_path.write_text(toml_source(region), encoding="utf-8")
        conditions = []
        for identity, held in [("fixture.held", True), ("fixture.confused", False)]:
            conditions.append({"op": "add", "path": ["conditions", identity], "value": {
                "id": identity, "family": identity, "name": identity, "lifecycle": "timed",
                "modifiers": [] if held else [{"scope": "always", "op": {"kind": "locomotion",
                    "value": {"kind": "confused", "chance": {"numerator": 4, "denominator": 5}}}}],
                "stacking": {"kind": "keep" if held else "extend"}, "source_scoped": False,
                "descriptors": ["mind_affecting"], "capacity": "reject",
                "timing": {"kind": "actor_active", "on_expiry": {"kind": "remove"}},
                "restrictions": ["voluntary_actions"] if held else [],
                "preparation": "pause" if held else "preserve"}})
        rule_path = package / "rules.toml"
        rules = tomllib.loads(rule_path.read_text(encoding="utf-8"))
        rules["overrides"].extend(conditions)
        rule_path.write_text(toml_source(rules), encoding="utf-8")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return package

    def finish_control(self, owner, peers):
        for _ in range(80):
            current = self.snapshot(owner)
            view = observation(current)
            if (view["ready"] and not current["intentions"]
                    and view["combat"]["preparation_remaining"] is None
                    and int(view["combat"]["recovery_remaining"]) == 0):
                return current
            for peer in peers:
                frame = self.snapshot(peer)
                if frame["readiness"]["admission"] and observation(frame)["ready"]:
                    self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
        self.fail("control fixture failed to reach its ordinary decision boundary")

    def control_action(self, owner, peers, action):
        self.assertIsNone(self.act(owner, action)["error"])
        return self.finish_control(owner, peers)

    def control_attack(self, owner, peers, name):
        frame = self.snapshot(owner)
        target = next(row["id"] for row in observation(frame)["visible_actors"] if row["name"] == name)
        return self.control_action(owner, peers, {"type": "use_ability", "id": "basic_melee",
            "invocation": {"aim": {"kind": "actor_target", "target": target},
                           "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}})

    def shown_condition(self, client, identity):
        return next((row for row in observation(self.snapshot(client))["combat"]["own_stats"]["conditions"]
                     if row["condition"]["id"] == identity), None)

    def test_confusion_charge_restart_miss_retains_then_landed_hit_delivers(self):
        package = self.control_package()
        server = self.server("--checkpoint-interval", 1, scenario=package)
        owner, initial = self.client()
        easy, _ = self.client(actor=2)
        hard, _ = self.client(actor=3)
        page = self.item(initial, "bewildering page")
        self.control_action(owner, [easy, hard], {"type": "read", "item": page})
        armed = checkpoint(self, owner)["actors"]["1"]["control"]["confusion"]
        self.assertIsNotNone(armed)
        for client in [owner, easy, hard]:
            client.stop()
        server.stop(save=False)
        package.rename(self.directory / "retained-control-source")
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        owner, _ = self.client()
        easy, _ = self.client(actor=2)
        hard, _ = self.client(actor=3)
        self.assertEqual(checkpoint(self, owner)["actors"]["1"]["control"]["confusion"], armed)
        hp = observation(self.snapshot(hard))["combat"]["hp"]
        self.control_attack(owner, [easy, hard], "resistant subject")
        self.assertEqual(observation(self.snapshot(hard))["combat"]["hp"], hp, "guaranteed physical miss")
        self.assertEqual(checkpoint(self, owner)["actors"]["1"]["control"]["confusion"], armed)
        self.assertIsNone(self.shown_condition(hard, "fixture.confused"))
        hp = int(observation(self.snapshot(easy))["combat"]["hp"])
        self.control_attack(owner, [easy, hard], "willing subject")
        self.assertLess(int(observation(self.snapshot(easy))["combat"]["hp"]), hp)
        self.assertIsNotNone(self.shown_condition(easy, "fixture.confused"))
        self.assertIsNone(checkpoint(self, owner)["actors"]["1"]["control"]["confusion"])

    def test_confusion_resisted_hit_spends_charge_and_hold_checks_and_release_are_public(self):
        self.server("--checkpoint-interval", 1, scenario=self.control_package(resistant_can_be_hit=True))
        owner, initial = self.client()
        easy, _ = self.client(actor=2)
        hard, _ = self.client(actor=3)
        self.control_action(owner, [easy, hard], {"type": "read", "item": self.item(initial, "bewildering page")})
        hp = int(observation(self.snapshot(hard))["combat"]["hp"])
        self.control_attack(owner, [easy, hard], "resistant subject")
        self.assertLess(int(observation(self.snapshot(hard))["combat"]["hp"]), hp)
        self.assertIsNone(self.shown_condition(hard, "fixture.confused"), "Spiritual defense resists delivery")
        self.assertIsNone(checkpoint(self, owner)["actors"]["1"]["control"]["confusion"])
        self.control_attack(owner, [easy, hard], "willing subject")
        self.assertIsNone(self.shown_condition(easy, "fixture.confused"), "resisted charge is not banked")
        # Both visible enemies now have an ordinary earned engagement with reader.
        page = self.item(self.snapshot(owner), "binding page")
        self.control_action(owner, [easy, hard], {"type": "read", "item": page})
        self.assertIsNotNone(self.shown_condition(easy, "fixture.held"))
        self.assertIsNone(self.shown_condition(hard, "fixture.held"))
        # Admission advertises an available queue slot, not permission for
        # every action. The active condition must reject voluntary work itself.
        before_denial = self.snapshot(easy)
        denied = self.act(easy, {"type": "wait"})
        self.assertIsNotNone(denied["error"])
        after_denial = self.snapshot(easy)
        self.assertEqual(after_denial["state"], before_denial["state"])
        self.assertEqual(after_denial["intentions"], before_denial["intentions"])
        self.assertIsNotNone(self.shown_condition(easy, "fixture.held"))
        self.control_attack(owner, [easy, hard], "willing subject")
        self.assertIsNone(self.shown_condition(easy, "fixture.held"), "committed attack releases Hold")
        self.assertFalse(self.snapshot(owner)["state"]["wizard_game"])

    def test_hold_committed_physical_miss_releases_without_damage(self):
        self.server("--checkpoint-interval", 1, scenario=self.control_package(resistant_spiritual=0))
        owner, _ = self.client()
        easy, _ = self.client(actor=2)
        hard, _ = self.client(actor=3)
        hp = observation(self.snapshot(hard))["combat"]["hp"]
        self.control_attack(owner, [easy, hard], "resistant subject")
        self.assertEqual(observation(self.snapshot(hard))["combat"]["hp"], hp)
        page = self.item(self.snapshot(owner), "binding page")
        self.control_action(owner, [easy, hard], {"type": "read", "item": page})
        self.assertIsNotNone(self.shown_condition(hard, "fixture.held"))
        self.assertIsNone(self.shown_condition(easy, "fixture.held"), "unengaged neighbor is not held")
        self.control_attack(owner, [easy, hard], "resistant subject")
        self.assertEqual(observation(self.snapshot(hard))["combat"]["hp"], hp)
        self.assertIsNone(self.shown_condition(hard, "fixture.held"), "the committed miss releases Hold")

    def test_paid_scare_pickup_curse_crumble_remove_curse_and_restart(self):
        package = self.package()
        server = self.server("--checkpoint-interval", 1, scenario=package)
        owner, initial = self.client()
        source = self.item(initial, "ward page", ground=True)
        taken = self.paid(owner, {"type": "take", "item": source, "quantity": "2"})
        ward = self.item(taken, "ward page", quantity=2)
        saved = checkpoint(self, owner)
        self.assertFalse(saved["items"]["10"]["condition"]["cursed"])
        self.assertEqual(saved["items"]["10"]["quantity"], 1)
        cursed = [item for item in saved["items"].values() if item["condition"]["cursed"]]
        self.assertEqual(len(cursed), 1)
        self.assertEqual(cursed[0]["quantity"], 2)
        self.assertEqual(cursed[0]["location"], {"Carried": 1})
        owner.stop()
        server.stop(save=False)
        # Retain the source as evidence but make its original path unavailable.
        package.rename(self.directory / "retained-source")
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        owner, restored = self.client()
        self.assertEqual(checkpoint(self, owner), saved)
        ward = self.item(restored, "ward page", quantity=2)
        dropped = self.paid(owner, {"type": "drop", "item": ward, "quantity": "1"})
        # Both ground stacks have quantity one; the cursed stack must not merge
        # with the uncursed original. Distinguish using the original opaque ID.
        grounds = [row["item"] for row in observation(dropped)["ground_items"]
                   if row["item"]["name"] == "ward page"]
        self.assertEqual(len(grounds), 2)
        cursed_ground, = [row["id"] for row in grounds if row["id"] != source]
        crumbled = self.paid(owner, {"type": "take", "item": cursed_ground, "quantity": "1"})
        self.item(crumbled, "ward page", quantity=1)
        self.item(crumbled, "ward page", ground=True, quantity=1)
        before_retry = checkpoint(self, owner)
        rejected = self.act(owner, {"type": "take", "item": cursed_ground, "quantity": "1"})
        self.assertIsNotNone(rejected["error"])
        self.assertEqual(checkpoint(self, owner), before_retry,
                         "a stale crumble retry cannot pay twice or transfer a replacement")
        cleansing = self.item(crumbled, "cleansing page", ground=True)
        picked = self.paid(owner, {"type": "take", "item": cleansing, "quantity": None})
        cleansing = self.item(picked, "cleansing page")
        cleared = self.paid(owner, {"type": "read", "item": cleansing})
        uncursed = checkpoint(self, owner)
        self.assertTrue(all(not item["condition"]["cursed"] for item in uncursed["items"].values()))
        ward = self.item(cleared, "ward page", quantity=1)
        dropped = self.paid(owner, {"type": "drop", "item": ward, "quantity": None})
        source = self.item(dropped, "ward page", ground=True, quantity=2)
        taken = self.paid(owner, {"type": "take", "item": source, "quantity": None})
        ward = self.item(taken, "ward page", quantity=2)
        recursed = checkpoint(self, owner)
        self.assertEqual(sum(item["quantity"] for item in recursed["items"].values()
                             if item["condition"]["cursed"]), 2)
        spectator, hidden = self.client(SPECTATOR_TOKEN, observe=True)
        self.assertTrue(all(row["known_cursed"] is None
                            for row in observation(hidden)["interactions"]["inventory"]))
        spectator.stop()
        dropped = self.paid(owner, {"type": "drop", "item": ward, "quantity": None})
        ground = self.item(dropped, "ward page", ground=True, quantity=2)
        final = self.paid(owner, {"type": "take", "item": ground, "quantity": "1"})
        self.assertFalse(observation(final)["inventory"])
        self.item(final, "ward page", ground=True, quantity=1)
        # Retain the exact current-writer message for wire recording. The
        # recorder runs this whole acceptance case, including all cleanup;
        # neither this test nor the recorder manufactures a server envelope.
        snapshots = [json.loads(line).get("message") for line in owner.transcript
                     if line.startswith("{")]
        snapshots = [message for message in snapshots if message and message["type"] == "snapshot"]
        self.crumble_sample = copy.deepcopy(snapshots[-1])
        self.assertEqual(self.crumble_sample["snapshot"]["state"], final["state"])
        events = [entry["content"].get("event")
                  for entry in self.crumble_sample["snapshot"]["history"]["entries"]
                  if entry["content"]["type"] == "action"]
        self.assertIn({"type": "crumbled", "item": ground, "quantity": "1"}, events)


if __name__ == "__main__":
    unittest.main()
