"""Occupied ground Scare drives ordinary autonomous retreat and finite recovery.

The assertions use the real player's disclosed geometry, not a private AI state
flag. Save facts only supplement the public movement/restart witness. Runtime
qualification is intentionally separate from source syntax checks.
"""
import copy
import shutil
import subprocess
import tomllib
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT, settle_rogue_action


def view(frame):
    return frame["state"]["observation"]


class GroundScareControlProcesses(ProcessTestCase):
    def package(self):
        package = self.directory / "occupied-scare"
        shutil.copytree(ROOT / "scenarios/tests/magic-items", package)
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        manifest["id"] = "occupied-scare-process"
        manifest["archetypes"] = {"ward": {
            "name": "ground ward", "identity": "fixture.ground_ward", "class": "scroll",
            "stackable": True, "readable": {"control": {"kind": "nothing"}, "discovery": "never"},
            "ground_scare": {"condition": "fixture.flee", "duration": 1000}}}
        manifest["appearance_pools"] = {}
        manifest["factions"] = {"neutral": ["foe"], "foe": ["neutral"]}
        manifest["ai_profiles"] = {"pursuer": {
            "idle": "wait", "doors": False, "care": False, "equipment": False,
            "loot": False, "gold": False, "flee_percent": 0}}
        species = manifest["creatures"]["species"]["figure"]
        species["behavior"] = {"disposition": "mean", "idle": "wait", "memory_ticks": 10000}
        species["modifiers"] = [{"scope": "always", "op": {
            "kind": "add", "stat": {"kind": "health"}, "value": 1000}}]
        character = manifest["characters"][0]
        character["turn_ticks"] = 100
        character["body"] = {"cells": [[0, 0, 0], [0, 0, 1]], "eye": [0, 0, 1], "mass": 80}
        target = copy.deepcopy(character["creature"])
        target.update(name="ward pursuer", faction="foe")
        path.write_text(toml_source(manifest), encoding="utf-8")
        region_path = package / "regions/1.toml"
        region = tomllib.loads(region_path.read_text(encoding="utf-8"))
        # The entire lit 9x7 arena remains within the ordinary 16-cell sight
        # extent from both authored owner positions (no vision cheats/observer).
        region.update(size=[9, 7, 2], anchors={"start": [3, 3, 0]},
                      items=[{"id": 10, "at": [3, 3, 0], "archetype": "ward", "quantity": 1}],
                      actors=[{"id": 2, "at": [4, 3, 0], "controller": "ai", "ai": "pursuer",
                               "body": copy.deepcopy(character["body"]), "creature": target}])
        region_path.write_text(toml_source(region), encoding="utf-8")
        with (package / "rules.toml").open("a", encoding="utf-8") as output:
            output.write('''
[[overrides]]
op = "add"
path = ["conditions", "fixture.flee"]
value = { id = "fixture.flee", family = "fixture.flee", name = "Frightened", lifecycle = "timed", modifiers = [], stacking = { kind = "keep" }, source_scoped = false, descriptors = ["mind_affecting"], capacity = "reject", timing = { kind = "actor_active", on_expiry = { kind = "remove" } }, restrictions = [], preparation = "preserve" }
''')
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return package

    def snapshot(self, player):
        result = self.request(player, {"type": "snapshot"})
        self.assertIsNone(result["error"])
        self.assertFalse(result["state"]["wizard_game"])
        return result

    def execute(self, player, action):
        result = self.act(player, action)
        self.assertIsNone(result["error"])
        return settle_rogue_action(self, player, result)

    def pursuer(self, frame):
        actors = [row for row in view(frame)["visible_actors"] if row["name"] == "ward pursuer"]
        # Public geometry contains one occurrence per disclosed body cell.
        # This two-cell body has one identity and exactly one ground anchor.
        self.assertEqual(len({row["id"] for row in actors}), 1,
                         "open lit fixture must disclose the named retreating actor")
        anchors = [row for row in actors if row["position"]["z"] == 0]
        self.assertEqual(len(anchors), 1)
        return anchors[0]

    def distance(self, frame):
        position = self.pursuer(frame)["position"]
        # Ordinary melee/ward adjacency includes diagonal neighboring cells.
        return max(abs(position["x"]), abs(position["y"]))

    def test_occupied_ground_scroll_causes_retreat_then_expires_across_restart(self):
        package = self.package()
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=42)
        player, initial = self.client()
        self.assertEqual(self.distance(initial), 1)
        self.assertTrue(any(row["item"]["name"] == "ground ward"
                            for row in view(initial)["ground_items"]))
        retreated = self.execute(player, {"type": "wait"})
        self.assertGreater(self.distance(retreated), 1,
                           "occupied ward must make the hostile retreat rather than attack/approach")
        self.assertLess(int(view(retreated)["tick"]), 1000)
        # Leave the ward through a normal move, so expiry can be distinguished
        # from a new approach triggering another occupied ward.
        departed = self.execute(player, {"type": "move", "direction": "north"})
        self.assertLess(int(view(departed)["tick"]), 1000)
        for _ in range(4):
            query = self.snapshot(player)
            self.assertEqual(view(query)["tick"], view(departed)["tick"])
            self.assertEqual(self.pursuer(query)["position"], self.pursuer(departed)["position"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        expected_tick = view(departed)["tick"]
        expected_position = self.pursuer(departed)["position"]
        player.stop()
        server.stop(save=False)
        package.rename(self.directory / "retained-occupied-scare-source")
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        player, restored = self.client()
        self.assertEqual(view(restored)["tick"], expected_tick)
        self.assertEqual(self.pursuer(restored)["position"], expected_position)
        self.assertEqual(view(restored)["position"], view(departed)["position"])
        # With no newly occupied ward and no attacks by the owner, the authored
        # finite fear clock is the only reason to reverse retreat into pursuit.
        before_expiry = [self.distance(restored)]
        after_expiry = []
        current = restored
        for _ in range(24):
            current = self.execute(player, {"type": "wait"})
            tick = int(view(current)["tick"])
            if tick < 1000:
                before_expiry.append(self.distance(current))
            else:
                after_expiry.append(self.distance(current))
            if tick >= 2000 and after_expiry and min(after_expiry) <= 1:
                break
        self.assertTrue(after_expiry, "the ordinary action sequence must cross the authored expiry")
        self.assertGreater(max(before_expiry), 1)
        self.assertLess(min(after_expiry), max(before_expiry),
                        "expired finite fear must permit renewed pursuit, not permanent fleeing")
        self.assertLessEqual(min(after_expiry), 1)


if __name__ == "__main__":
    unittest.main()
