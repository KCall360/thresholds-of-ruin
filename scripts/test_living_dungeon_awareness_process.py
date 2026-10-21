"""Automatic worn Stealth, active Aggravation and paid location-only alerts.

A lit ordinary arena makes AI movement observable to the actual player. Large
explicit Stealth operands guarantee concealment without making the PC invisible.
The pulse fixture overrides only cadence probability to 1/1; production 1/3
selection and protected sleep/Xeroc boundaries remain simulation-test owners.
"""
import copy
import json
import shutil
import subprocess
import tomllib
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT, settle_rogue_action
from status_condition_package import checkpoint


def view(frame):
    return frame["state"]["observation"]


class LivingDungeonAwarenessProcesses(ProcessTestCase):
    def package(self):
        package = self.directory / "living-awareness"
        shutil.copytree(ROOT / "scenarios/tests/magic-items", package)
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        catalog = tomllib.loads((ROOT / "scenarios/rogue-exploration/catalog/items.toml").read_text(encoding="utf-8"))
        archetypes = {}
        for name, identity in [("stealth", "rogue.ring_stealth"),
                               ("aggravation", "rogue.ring_aggravation"),
                               ("alert", "rogue.aggravate_monster")]:
            item = copy.deepcopy(catalog["items"][identity])
            for field in ("asset", "appearance_pool", "equipment_generation"):
                item.pop(field, None)
            archetypes[name] = item
        pulse = next(modifier["op"]["value"] for modifier in archetypes["aggravation"]["equipment"]["modifiers"]
                     if modifier["op"]["kind"] == "perception")
        pulse["chance_denominator"] = 1
        manifest.update(id="living-awareness-process", archetypes=archetypes, appearance_pools={},
                        factions={"player": ["watcher"], "watcher": ["player"]},
                        ai_profiles={"watcher": {"idle": "wait", "doors": False, "care": False,
                            "equipment": False, "loot": False, "gold": False, "flee_percent": 0}})
        species = manifest["creatures"]["species"]["figure"]
        species["behavior"] = {"disposition": "mean", "idle": "wait", "memory_ticks": 300}
        species["modifiers"] = [{"scope": "always", "op": {
            "kind": "add", "stat": {"kind": "health"}, "value": 1000}}]
        sneaky = copy.deepcopy(species)
        # Species base attributes have a cap of five. The ordinary modifier
        # fold supplies the deterministic effective operand, as in the existing
        # theft process fixture, without invalidating the creature definition.
        sneaky["modifiers"].append({"scope": "always", "op": {
            "kind": "set", "stat": {"kind": "attribute", "attribute": "speed"},
            "value": 100}})
        manifest["creatures"]["species"]["sneaky"] = sneaky
        character = manifest["characters"][0]
        character["turn_ticks"] = 100
        character["creature"].update(species="sneaky", faction="player")
        character["body"] = {"cells": [[0, 0, 0], [0, 0, 1]], "eye": [0, 0, 1], "mass": 80}
        character["loadout"] = [{"archetype": "stealth", "quantity": 1, "equipped_slot": 2},
                                {"archetype": "aggravation", "quantity": 1},
                                {"archetype": "alert", "quantity": 2}]
        character["known_identities"] = ["rogue.ring_stealth", "rogue.ring_aggravation", "rogue.aggravate_monster"]
        target = copy.deepcopy(character["creature"])
        target.update(species="figure", name="patient watcher", faction="watcher")
        path.write_text(toml_source(manifest), encoding="utf-8")
        region_path = package / "regions/1.toml"
        region = tomllib.loads(region_path.read_text(encoding="utf-8"))
        # Every point is within ordinary sight. No darkness or wall can explain
        # the watcher's initial failure to acquire the automatically sneaking PC.
        region.update(size=[11, 7, 2], anchors={"start": [2, 3, 0]}, items=[],
                      actors=[{"id": 2, "at": [8, 3, 0], "turn_ticks": 100,
                               "controller": "ai", "ai": "watcher", "creature": target,
                               "body": copy.deepcopy(character["body"])}])
        region_path.write_text(toml_source(region), encoding="utf-8")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return package

    def snapshot(self, player):
        frame = self.request(player, {"type": "snapshot"})
        self.assertIsNone(frame["error"])
        self.assertFalse(frame["state"]["wizard_game"])
        return frame

    def execute(self, player, action):
        frame = self.act(player, action)
        self.assertIsNone(frame["error"])
        return settle_rogue_action(self, player, frame)

    def watcher(self, frame):
        rows = [actor for actor in view(frame)["visible_actors"] if actor["name"] == "patient watcher"]
        # A tall body is disclosed once per visible cell, not once per actor.
        self.assertEqual(len({row["id"] for row in rows}), 1,
                         "the actual owner must physically see the named lit watcher")
        anchors = [row for row in rows if row["position"]["z"] == 0]
        self.assertEqual(len(anchors), 1)
        return anchors[0]

    def distance(self, frame):
        position = self.watcher(frame)["position"]
        return abs(position["x"]) + abs(position["y"])

    def item(self, frame, identity):
        rows = [item for item in view(frame)["inventory"] if item["name"] == identity]
        self.assertEqual(len(rows), 1)
        return rows[0]

    def assert_anonymous_alert(self, saved):
        watcher = saved["actors"]["2"]
        alert = watcher["awareness"]["alert"]
        self.assertIsNotNone(alert)
        self.assertEqual(set(alert), {"location", "expires"})
        self.assertIsNone(watcher["behavior"]["engagement"], "a location pulse grants no actor target")
        contacts = watcher["awareness"]["contacts"]
        self.assertTrue(any(row["subject"] == 1 and not row["detected"] for row in contacts))
        return alert

    def test_worn_stealth_periodic_snapshot_alert_and_paid_scroll_survive_reopen(self):
        package = self.package()
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=42)
        player, initial = self.client()
        # Validate the live derived operand, not merely the authored input:
        # minimum Stealth total103 exceeds this untrained Awareness0 watcher's
        # maximum Perception roll18, even with Aggravation's one disadvantage.
        self.assertEqual(view(initial)["combat"]["own_stats"]["attributes"]["speed"], 100)
        initial_distance = self.distance(initial)
        initial_hp = view(initial)["combat"]["hp"]
        for _ in range(3):
            quiet = self.execute(player, {"type": "wait"})
            self.assertEqual(self.distance(quiet), initial_distance,
                             "automatic worn Stealth must prevent ordinary acquisition")
            self.assertEqual(view(quiet)["combat"]["hp"], initial_hp)
        saved = checkpoint(self, player)
        self.assertTrue(any(row["subject"] == 1 and not row["detected"]
                            for row in saved["actors"]["2"]["awareness"]["contacts"]))
        for _ in range(5):
            query = self.snapshot(player)
            self.assertEqual(self.distance(query), initial_distance)
            self.assertEqual(view(query)["tick"], view(quiet)["tick"])
        self.assertEqual(checkpoint(self, player), saved, "read-only queries cannot retry an opposed check")
        ring = self.item(quiet, "ring of aggravation")["id"]
        current = self.execute(player, {"type": "equip", "item": ring, "slot": 3})
        current = self.execute(player, {"type": "wait"})
        phased = checkpoint(self, player)
        owner = phased["actors"]["1"]
        phase = owner["awareness"]["elapsed"] + phased["tick"] - owner["active_since"]
        self.assertGreater(phase, 0)
        self.assertLess(phase, 500, "restart must occur before the first authored pulse")
        self.assertIsNone(phased["actors"]["2"]["awareness"]["alert"])
        player.stop()
        server.stop(save=False)
        package.rename(self.directory / "retained-awareness-source")
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        player, current = self.client()
        self.assertEqual(checkpoint(self, player), phased)
        pulse_at = phased["tick"] + 500 - phase
        for _ in range(8):
            current = self.execute(player, {"type": "wait"})
            if int(view(current)["tick"]) >= pulse_at:
                break
            self.assertEqual(self.distance(current), initial_distance)
            self.assertIsNone(checkpoint(self, player)["actors"]["2"]["awareness"]["alert"])
        self.assertGreaterEqual(int(view(current)["tick"]), pulse_at)
        pulsed = checkpoint(self, player)
        alert = self.assert_anonymous_alert(pulsed)
        self.assertEqual(alert["location"]["position"], {"x": 2, "y": 3, "z": 0})
        # Give the autonomous listener one decision boundary to investigate.
        current = self.execute(player, {"type": "wait"})
        self.assertLess(self.distance(current), initial_distance,
                        "location-only alert must produce visible investigation")
        self.assertEqual(view(current)["combat"]["hp"], initial_hp)
        ring = self.item(current, "ring of aggravation")["id"]
        current = self.execute(player, {"type": "unequip", "item": ring})
        frozen = checkpoint(self, player)["actors"]["1"]["awareness"]["elapsed"]
        current = self.execute(player, {"type": "move", "direction": "north"})
        moved = checkpoint(self, player)
        still_alert = moved["actors"]["2"]["awareness"]["alert"]
        if still_alert is not None:
            self.assertEqual(still_alert["location"], alert["location"], "alert must not track wearer")
        # Leave a real spatial gap from the stale point. If we stood just one
        # square away, the old investigation could already end adjacent and a
        # fresh alert could not demonstrate further public approach.
        for _ in range(2):
            current = self.execute(player, {"type": "move", "direction": "north"})
        for _ in range(5):
            current = self.execute(player, {"type": "wait"})
        quiet_again = checkpoint(self, player)
        self.assertEqual(quiet_again["actors"]["1"]["awareness"]["elapsed"], frozen,
                         "unworn active pulse clock must not accrue catch-up work")
        self.assertIsNone(quiet_again["actors"]["2"]["awareness"]["alert"])
        page = self.item(current, "scroll of aggravate monster")
        before_tick = int(view(current)["tick"])
        before_distance = self.distance(current)
        current = self.execute(player, {"type": "read", "item": page["id"]})
        self.assertGreater(int(view(current)["tick"]), before_tick)
        self.assertEqual(self.item(current, "scroll of aggravate monster")["quantity"], "1")
        self.assert_anonymous_alert(checkpoint(self, player))
        for _ in range(2):
            current = self.execute(player, {"type": "wait"})
        self.assertLess(self.distance(current), before_distance,
                        "paid Aggravate scroll must restart investigation of the new point")
        self.assertEqual(view(current)["combat"]["hp"], initial_hp)
        # The same physical visibility without the worn automatic stance must
        # permit ordinary acquisition. Count a real public committed attack,
        # not damage (the deliberately high Speed can make that attack miss).
        stealth = self.item(current, "ring of stealth")["id"]
        cursor = len(player.transcript)
        current = self.execute(player, {"type": "unequip", "item": stealth})
        attacked = False
        for _ in range(12):
            caster = self.watcher(current)["id"]
            # Settling an action consumes intermediate public strike snapshots
            # before returning a later ready snapshot, whose events can be empty.
            # Inspect every actual server observation consumed since unequip rather
            # than mistaking the final frame for the entire causal action.
            for line in player.transcript[cursor:]:
                if not line.startswith("{"):
                    continue
                message = json.loads(line).get("message")
                if not message:
                    continue
                combat = None
                if message["type"] == "snapshot":
                    combat = message["snapshot"]["state"]["observation"].get("combat")
                elif message["type"] == "update":
                    body = message["update"]["body"]
                    if body["type"] == "observation":
                        combat = body["state"]["observation"].get("combat")
                    elif body["type"] == "observation_delta":
                        combat = body["state"].get("combat")
                events = combat["events"] if combat else []
                attacked |= any(event["type"] == "ability" and event["caster"] == caster
                                for event in events)
            cursor = len(player.transcript)
            if attacked:
                break
            current = self.execute(player, {"type": "wait"})
        self.assertTrue(attacked, "removing continual Stealth restores ordinary hostile acquisition")


if __name__ == "__main__":
    unittest.main()
