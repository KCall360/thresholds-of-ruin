"""Shared grid authoring stays independent of bundled scenario policy."""
from pathlib import Path
import tempfile
import tomllib
import unittest

from grid_scenario_authoring import build_explicit, build_compact

ROOT = Path(__file__).resolve().parents[1]


class GridScenarioAuthoring(unittest.TestCase):
    def test_legacy_rogue_output_is_unchanged(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            build_explicit(root, "rogue-exploration", 26)
            expected = ROOT / "scenarios/rogue-exploration"
            paths = [Path("scenario.toml"), Path("rules.toml"),
                     *[Path("regions") / f"{n}.toml" for n in range(1, 235)]]
            for path in paths:
                self.assertEqual((root / path).read_bytes(), (expected / path).read_bytes(), str(path))

    def test_explicit_fixture_has_three_floors_and_terminal_boundary(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            build_explicit(root, "explicit-grid-groups", 3)
            manifest = tomllib.loads((root / "scenario.toml").read_text())
            self.assertEqual(len(manifest["generation_groups"]), 3)
            self.assertEqual(len(list((root / "regions").glob("*.toml"))), 27)
            self.assertEqual(len(manifest["stair_pairs"]), 2)
            last = tomllib.loads((root / "regions/27.toml").read_text())
            self.assertEqual(last["generate"]["stair_anchors"], [])

    def test_compact_fixture_needs_no_bundled_sources_or_region_stubs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            build_compact(root, 'kind="open"\nmax_depth=1000000')
            manifest = tomllib.loads((root / "scenario.toml").read_text())
            self.assertNotIn("generation_groups", manifest)
            self.assertNotIn("stair_pairs", manifest)
            self.assertEqual(list(manifest["procedural_worlds"]), ["caves"])
            self.assertFalse((root / "regions").exists())
