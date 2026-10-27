"""Independent opt-in treasure-room authoring policy; never a live balance change."""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue-exploration"


class RogueTreasureContent(unittest.TestCase):
    def test_example_is_inert_and_describes_one_bounded_shared_room(self):
        path = PACKAGE / "examples/treasure-stage.toml"
        fragment = tomllib.loads(path.read_text(encoding="utf-8"))
        self.assertEqual(fragment, {"floor_activation": {"profiles": {"rogue.low_depth": {
            "stages": [{"id": "rogue.treasure_room", "version": 1, "kind": "room_cluster",
                "chance": {"numerator": 1, "denominator": 20}, "item_count": [2, 9],
                "actor_count": [4, 11], "quantity": 1, "actor_depth_offset": 1,
                "ai": "rogue_hostile", "depth_growth": {"kind": "racial_after", "depth": 26},
                "avoid_protagonist_sight": False,
                "item_bands": [{"depth": {"kind": "open", "minimum": 1}, "choices": [
                    {"archetype": "rogue." + name, "weight": 1}
                    for name in ("gold", "healing", "fruit", "dart", "shuriken")]}],
                "actor_bands": [
                    {"depth": {"kind": "finite", "minimum": 2, "maximum": 5},
                     "choices": [{"archetype": "rogue.hobgoblin", "weight": 1}]},
                    {"depth": {"kind": "open", "minimum": 6},
                     "choices": [{"archetype": "rogue.orc", "weight": 1}]}]}]}}}})
        manifest_text = (PACKAGE / "scenario.toml").read_text(encoding="utf-8")
        manifest = tomllib.loads(manifest_text)
        self.assertNotIn("treasure", manifest_text)
        self.assertEqual(manifest["floor_activation"]["families"], {"dungeon": "rogue.low_depth"})
        self.assertEqual(set(manifest["floor_activation"]["profiles"]), {"rogue.low_depth"})
        self.assertNotIn("room_cluster", {stage["kind"] for stage in
            manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]})
        self.assertNotIn("examples/treasure-stage.toml", str(manifest.get("modules", [])))
        items = tomllib.loads((PACKAGE / "catalog/items.toml").read_text(encoding="utf-8"))["items"]
        actors = tomllib.loads((PACKAGE / "catalog/actors.toml").read_text(encoding="utf-8"))["actors"]
        stage = fragment["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]
        for band in stage["item_bands"]:
            for choice in band["choices"]:
                self.assertIn(choice["archetype"], items)
        for band in stage["actor_bands"]:
            for choice in band["choices"]:
                self.assertIn(choice["archetype"], actors)


if __name__ == "__main__":
    unittest.main()
