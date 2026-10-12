"""Explore generated floors through actual clients using disclosed cells only."""
import json
import shutil
import subprocess
import unittest

from process_harness import ProcessTestCase, SPECTATOR_TOKEN, SUFFIX, explore_to_stairs, underfoot


class RogueExplorationProcess(ProcessTestCase):
    def test_extreme_room_entry_is_a_structured_authoring_error(self):
        from build_explicit_grid_groups import build
        root = self.directory / "invalid-entry"
        build(root)
        region = root / "regions/1.toml"
        region.write_text(region.read_text(encoding="utf-8").replace(
            "start = [13, 3, 0]", "start = [-2147483648, 3, 0]"), encoding="utf-8")
        result = subprocess.run([self.bin / ("tor-scenario" + SUFFIX),
                                 "validate", root], capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("Room entries", json.loads(result.stderr)["error"]["message"])

    def test_authored_entry_and_group_stair_round_trip(self):
        from build_explicit_grid_groups import build
        root = self.directory / "mixed-entry"
        build(root)
        manifest = root / "scenario.toml"
        text = manifest.read_text(encoding="utf-8").replace('anchor = "1/start"', 'anchor = "235/entry"')
        text += '\n[stair_pairs.entrance]\nupper = "235/entry"\nlower = "1/start"\n'
        manifest.write_text(text, encoding="utf-8", newline="\n")
        (root / "regions" / "235.toml").write_text("id=235\nname='Entry'\nsize=[3,3,2]\nchamber=true\n[anchors]\nentry=[1,1,0]\n", encoding="utf-8", newline="\n")
        self.server("--allow-unvalidated", scenario=root, separate_stderr=True)
        player, welcome = self.client()
        origin = underfoot(welcome["state"])["key"]
        down = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNone(down["error"])
        self.assertTrue(underfoot(down["state"])["stairs_up"])
        up = self.act(player, {"type": "move", "direction": "up"})
        self.assertIsNone(up["error"])
        self.assertEqual(underfoot(up["state"])["key"], origin)

    def test_headless_exploration_stair_round_trip_resume_and_spectator(self):
        server = self.server(scenario="rogue-exploration")
        player, welcome = self.client()
        self.assert_private_floor_metadata(welcome["state"])
        departure = explore_to_stairs(self, player, welcome["state"])
        origin = underfoot(departure)["key"]
        arrived = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNone(arrived["error"])
        self.assertTrue(underfoot(arrived["state"])["stairs_up"])
        observer, watched = self.client(token=SPECTATOR_TOKEN, observe=True)
        self.assertEqual(watched["state"], arrived["state"])
        self.assert_private_floor_metadata(watched["state"])
        self.flush_save()
        player.stop()
        observer.stop()
        server.stop()
        # Keep the exact closed startup input, including SQLite sidecars, even
        # if a failed restore changes the working save before it advertises a
        # listener. Failed-test retention includes this independent reproduction.
        resume_input = self.directory / "resume-input"
        resume_input.mkdir()
        for path in self.directory.glob(self.save.name + "*"):
            if path.is_file():
                shutil.copy2(path, resume_input / path.name)
        (resume_input / "expected-state.json").write_text(
            json.dumps(arrived["state"], ensure_ascii=False, indent=2), encoding="utf-8")
        self.server()
        player, resumed = self.client()
        self.assertEqual(resumed["state"], arrived["state"])
        self.assert_private_floor_metadata(resumed["state"])
        returned = self.act(player, {"type": "move", "direction": "up"})
        self.assertIsNone(returned["error"])
        self.assertEqual(underfoot(returned["state"])["key"], origin)

    def assert_private_floor_metadata(self, state):
        encoded = json.dumps(state)
        # Generation and named destinations are backend structure. Neither a
        # player nor a spectator receives the untouched final floor or its
        # generation manifest while exploring the first two floors.
        for hidden in ("Depth 26", "floor-26", "234/up", "234/down",
                       "generation_groups", "named_anchors", "named_stairs",
                       '"region"', '"recipe"'):
            self.assertNotIn(hidden, encoded)
        for cell in state["observation"]["visible_cells"]:
            self.assertEqual(set(cell["position"]), {"x", "y", "z"})

    def test_adventure_uses_generated_floor_stairs(self):
        self.server(scenario="rogue-exploration")
        player, welcome = self.client()
        explore_to_stairs(self, player, welcome["state"])
        player.stop()
        adventure, welcome = self.adventure()
        self.assertIn("Stairs lead down.", welcome)
        down = self.say(adventure, "down")
        self.assertTrue(down.startswith("You go down.\n"), down)
        self.assertIn("Stairs lead up.", down)
        self.assertTrue(self.say(adventure, "up").startswith("You go up.\n"))


class RogueExplorationAsciiProcess(ProcessTestCase):
    graphical = True

    def test_native_generated_floor_stair_round_trip(self):
        self.server(scenario="rogue-exploration")
        player, welcome = self.client()
        departure = explore_to_stairs(self, player, welcome["state"])
        player.stop()
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        initial = self.ascii_frame(window, lambda frame: frame.get("state") and not frame["busy"])
        origin = underfoot(initial["state"])["key"]
        tick = int(departure["observation"]["tick"])
        key = self.native_keys(window)
        for name, delta in [("period", 100), ("comma", 200)]:
            key("Shift_L", True)
            key(name, True)
            frame = self.ascii_frame(window, lambda frame: frame.get("state")
                and frame["state"]["observation"]["tick"] == str(tick + delta) and not frame["busy"])
            key(name, False)
            key("Shift_L", False)
            if delta == 100:
                self.assertTrue(underfoot(frame["state"])["stairs_up"])
            else:
                self.assertEqual(underfoot(frame["state"])["key"], origin)


if __name__ == "__main__":
    unittest.main()
