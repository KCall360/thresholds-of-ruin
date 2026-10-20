"""Explore generated floors through actual clients using disclosed cells only."""
import json
import shutil
import sqlite3
import subprocess
import tomllib
import unittest
from contextlib import closing

from process_harness import DIRECTIONS, ProcessTestCase, ROOT, SPECTATOR_TOKEN, SUFFIX, explore_to_stairs, underfoot


class RogueBundledPolicy(unittest.TestCase):
    def test_bundled_dungeon_has_an_open_compact_descriptor_without_enumerated_floors(self):
        package = ROOT / "scenarios/rogue-exploration"
        manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
        self.assertIn("procedural_worlds", manifest.keys())
        self.assertEqual(set(manifest["procedural_worlds"]), {"dungeon"})
        family = manifest["procedural_worlds"]["dungeon"]
        self.assertEqual(family["kind"], "grid_chain")
        self.assertEqual(family["version"], 1)
        self.assertEqual(family["grid"], [3, 3])
        self.assertEqual(family["size"], [26, 7, 2])
        self.assertEqual(family["domain"], {"kind": "open", "max_depth": 1_000_000})
        self.assertEqual(manifest["characters"][0]["anchor"], "dungeon/1/start")
        self.assertNotIn("generation_groups", manifest)
        self.assertNotIn("stair_pairs", manifest)
        self.assertNotIn("objectives", manifest)
        self.assertFalse((package / "regions").exists())
        index = json.loads((package / "index.json").read_text(encoding="utf-8"))
        self.assertEqual(index["regions"], [])
        certificate = json.loads((package / "validation.json").read_text(encoding="utf-8"))
        self.assertEqual(certificate["regions"], 0)

    def test_geometry_fixture_preserves_the_population_free_workload_explicitly(self):
        actual = tomllib.loads((ROOT / "scenarios/rogue-exploration/scenario.toml").read_text(encoding="utf-8"))
        geometry = tomllib.loads((ROOT / "scenarios/tests/rogue-geometry/scenario.toml").read_text(encoding="utf-8"))
        self.assertEqual(geometry["procedural_worlds"], actual["procedural_worlds"])
        # This fixture owns feature-free geometry traversal, not concealed
        # entrances or pressure traps. Keep the unchanged geometry/lighting
        # recipe without silently adding gameplay obstacles to that workload.
        features = ("secret_entrances", "hidden_traps")
        self.assertEqual(geometry["generation_recipes"], {
            name: {key: value for key, value in recipe.items() if key not in features}
            for name, recipe in actual["generation_recipes"].items()
        })
        for recipe in geometry["generation_recipes"].values():
            for feature in features:
                self.assertNotIn(feature, recipe)
        self.assertNotIn("floor_activation", geometry)
        self.assertNotIn("capabilities", geometry)
        self.assertEqual(geometry["characters"], [{"id": 1, "anchor": "dungeon/1/start", "turn_ticks": 100}])


class RogueExplorationProcess(ProcessTestCase):
    def geometry_copy(self, name):
        package = self.directory / name
        shutil.copytree(ROOT / "scenarios/tests/rogue-geometry", package,
                        ignore=shutil.ignore_patterns(".compiled"))
        return package

    def compiler(self, *arguments):
        result = subprocess.run([self.bin / ("tor-scenario" + SUFFIX), *map(str, arguments)],
                                capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def test_compiler_keeps_future_depth_queries_bounded_without_enumerating_the_dungeon(self):
        # These are offline authoring queries, never a client navigation oracle.
        package = self.geometry_copy("compact-authoring")
        certificate = self.compiler("validate", package)
        self.assertEqual(certificate["regions"], 0)
        index = (package / "index.json").read_bytes()
        self.assertEqual(json.loads(index)["regions"], [])
        self.assertFalse((package / "regions").exists())
        manifest = self.compiler("inspect", package)["manifest"]
        family = manifest["procedural_worlds"]["dungeon"]
        self.assertEqual(family["domain"], {"kind": "open", "max_depth": 1_000_000})
        slots = family["grid"][0] * family["grid"][1]
        plans = {}
        for depth in [27, 1_000_000, 101, 27]:
            with self.subTest(depth=depth):
                region = family["base_region"] + (depth - 1) * slots + 4
                plan = self.compiler("horizon", package, region, 1)
                self.assertIn(region, plan["required"])
                self.assertEqual(plan["activate"], plan["required"])
                self.assertEqual(plan["deactivate"], [])
                self.assertEqual(plan["expanded_regions"], 1)
                self.assertGreaterEqual(plan["examined_links"], 4)
                self.assertLessEqual(plan["examined_links"], 6)
                self.assertLessEqual(len(plan["required"]), 7)
                if depth in plans:
                    self.assertEqual(plan, plans[depth])
                plans[depth] = plan
        self.assertEqual((package / "index.json").read_bytes(), index)
        self.assertFalse((package / "regions").exists())

    def test_seeded_initial_disclosure_is_repeatable_varied_and_safe_to_wait_on(self):
        disclosures = []
        for run, seed in enumerate([0, 1, 42, 0]):
            with self.subTest(seed=seed, run=run):
                self.save = self.directory / f"seed-{run}.db"
                server = self.server(scenario="tests/rogue-geometry", seed=seed)
                player, welcome = self.client()
                self.assert_safe_entry(welcome["state"])
                self.assert_private_floor_metadata(welcome["state"])
                # Opaque cell identities have independent fresh-save privacy
                # salts. Compare only the geometry already disclosed to us.
                cells = [{key: value for key, value in cell.items() if key != "key"}
                         for cell in welcome["state"]["observation"]["visible_cells"]]
                disclosures.append(json.dumps(cells, sort_keys=True))
                waited = self.act(player, {"type": "wait"})
                self.assertIsNone(waited["error"])
                self.assertEqual(underfoot(waited["state"])["key"],
                                 underfoot(welcome["state"])["key"])
                self.assert_safe_entry(waited["state"])
                player.stop()
                server.stop()
        self.assertEqual(len(disclosures), 4, "Every seeded session must complete")
        self.assertEqual(disclosures[0], disclosures[3])
        self.assertGreater(len(set(disclosures[:3])), 1)

    def assert_safe_entry(self, state):
        cell = underfoot(state)
        self.assertFalse(cell["wall"])
        self.assertFalse(cell["stairs_up"])
        self.assertFalse(cell["stairs_down"])
        self.assertIsNone(state["observation"].get("combat"))

    def test_short_journal_only_exploration_resumes_without_source_files(self):
        package = self.geometry_copy("journal-only-dungeon")
        server = self.server("--checkpoint-interval", 0, scenario=package)
        player, current = self.client()
        self.assert_safe_entry(current["state"])
        for _ in range(4):
            cells = current["state"]["observation"]["visible_cells"]
            direction = next((name for dx, dy, name in DIRECTIONS if any(
                cell["position"] == {"x": dx, "y": dy, "z": 0} and not cell["wall"]
                for cell in cells)), None)
            self.assertIsNotNone(direction, "Entry room must disclose a walkable neighbor")
            previous = underfoot(current["state"])["key"]
            current = self.act(player, {"type": "move", "direction": direction})
            self.assertIsNone(current["error"])
            self.assertNotEqual(underfoot(current["state"])["key"], previous)
        before = self.request(player, {"type": "snapshot"})
        self.flush_save()
        player.stop()
        server.stop()
        with closing(sqlite3.connect(self.save)) as db:
            self.assertEqual(db.execute("SELECT count(*) FROM checkpoint").fetchone()[0], 0)
            self.assertGreater(db.execute("SELECT max(sequence) FROM journal").fetchone()[0], 0)
        shutil.rmtree(package)
        server = self.server("--checkpoint-interval", 0, seed=None)
        player, resumed = self.client()
        self.assertEqual(resumed["state"], before["state"])
        self.assertEqual(resumed["history"], before["history"])
        self.assert_private_floor_metadata(resumed["state"])
        continued = self.act(player, {"type": "wait"})
        self.assertIsNone(continued["error"])
        self.assertGreater(int(continued["state"]["observation"]["tick"]),
                           int(resumed["state"]["observation"]["tick"]))
        self.flush_save()
        player.stop()
        server.stop()
        with closing(sqlite3.connect(self.save)) as db:
            self.assertEqual(db.execute("SELECT count(*) FROM checkpoint").fetchone()[0], 0)

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
        package = self.geometry_copy("source-free-dungeon")
        # Natural disclosed exploration can take hundreds of moves. Exercise
        # checkpoint recovery here; the short test above covers journal-only.
        server = self.server("--checkpoint-interval", 64, scenario=package)
        player, welcome = self.client(omit_memory=True, compact_background=True)
        self.assert_safe_entry(welcome["state"])
        self.assert_private_floor_metadata(welcome["state"])
        departure = explore_to_stairs(self, player, welcome["state"])
        origin = underfoot(departure)["key"]
        arrived = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNone(arrived["error"])
        self.assertTrue(underfoot(arrived["state"])["stairs_up"])
        observer, watched = self.client(token=SPECTATOR_TOKEN, observe=True, omit_memory=True, compact_background=True)
        self.assertEqual(watched["state"], arrived["state"])
        self.assert_private_floor_metadata(watched["state"])
        self.flush_save()
        player.stop()
        observer.stop()
        server.stop()
        with closing(sqlite3.connect(self.save)) as db:
            checkpoint = db.execute("SELECT sequence FROM checkpoint").fetchone()
            self.assertIsNotNone(checkpoint)
            self.assertGreater(checkpoint[0], 0)
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
        shutil.rmtree(package)
        self.server("--checkpoint-interval", 64, seed=None)
        player, resumed = self.client(omit_memory=True, compact_background=True)
        self.assertEqual(resumed["state"], arrived["state"])
        self.assert_private_floor_metadata(resumed["state"])
        observer, watched = self.client(token=SPECTATOR_TOKEN, observe=True, omit_memory=True, compact_background=True)
        self.assertEqual(watched["state"], resumed["state"])
        returned = self.act(player, {"type": "move", "direction": "up"})
        self.assertIsNone(returned["error"])
        self.assertEqual(underfoot(returned["state"])["key"], origin)
        self.assertEqual(returned["state"]["observation"]["visible_cells"],
                         departure["observation"]["visible_cells"])
        revisited = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNone(revisited["error"])
        self.assertEqual(underfoot(revisited["state"])["key"], underfoot(arrived["state"])["key"])
        # Discover the next floor after source removal, then revisit both
        # paired endpoints. No wizard, hidden topology, or fixed cell route.
        second_exit = explore_to_stairs(self, player, revisited["state"])
        third = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNone(third["error"])
        self.assertTrue(underfoot(third["state"])["stairs_up"])
        self.assert_private_floor_metadata(third["state"])
        back = self.act(player, {"type": "move", "direction": "up"})
        self.assertIsNone(back["error"])
        self.assertEqual(underfoot(back["state"])["key"], underfoot(second_exit)["key"])
        second_entry = explore_to_stairs(self, player, back["state"], direction="up")
        self.assertEqual(underfoot(second_entry)["key"], underfoot(arrived["state"])["key"])
        home = self.act(player, {"type": "move", "direction": "up"})
        self.assertIsNone(home["error"])
        self.assertEqual(underfoot(home["state"])["key"], origin)

    def assert_private_floor_metadata(self, state):
        encoded = json.dumps(state)
        # Logical families, undisclosed destinations, operational limits and
        # generation inputs remain backend-only regardless of dungeon depth.
        for hidden in ("dungeon/", "dungeon depth ", "procedural_worlds",
                       "generation_groups", "generation_recipes", "named_anchors", "named_stairs",
                       '"base_region"', '"max_depth"', '"domain"', '"region"', '"recipe"'):
            self.assertNotIn(hidden, encoded)
        for cell in state["observation"]["visible_cells"]:
            self.assertEqual(set(cell["position"]), {"x", "y", "z"})

    def test_adventure_uses_generated_floor_stairs(self):
        self.server(scenario="tests/rogue-geometry")
        player, welcome = self.client(omit_memory=True, compact_background=True)
        explore_to_stairs(self, player, welcome["state"])
        player.stop()
        adventure, welcome = self.adventure()
        self.assertRegex(welcome, r"(?i)\bstairs (?:lead )?down\b")
        down = self.say(adventure, "down")
        self.assertTrue(down.startswith("You go down.\n"), down)
        self.assertRegex(down, r"(?i)\bstairs (?:lead )?up\b")
        self.assertTrue(self.say(adventure, "up").startswith("You go up.\n"))


class RogueExplorationAsciiProcess(ProcessTestCase):
    graphical = True

    def test_native_generated_floor_stair_round_trip(self):
        self.server(scenario="tests/rogue-geometry")
        player, welcome = self.client(omit_memory=True)
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
