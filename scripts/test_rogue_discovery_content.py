"""Provisional discovery authoring contracts; runtime tests own actual discovery.

These source checks do not establish numerical balance, LOS, contact or recovery.
"""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((PACKAGE / name).read_text(encoding="utf-8"))


class RogueDiscoveryContent(unittest.TestCase):
    def test_independent_feature_settings_do_not_replace_existing_geometry_recipe(self):
        recipe = source("scenario.toml")["generation_recipes"]["exploration"]
        self.assertEqual({key: value for key, value in recipe.items()
                          if key not in ("secret_entrances", "hidden_traps")}, {
            "kind": "grid_rooms", "version": 1, "room_width": [4, 24],
            "room_height": [3, 5], "extra_connections": [0, 2],
            "lighting": {"darkness_roll": 10, "darkness_start": 1}})
        self.assertEqual(recipe["secret_entrances"], {
            "chance_per_mille": 250, "maximum": 2, "dc": 12})
        self.assertEqual(recipe["hidden_traps"], {
            "count": [1, 2], "dc": 12, "rust_slot": "body_armor", "rust_amount": 1,
            "sleep_condition": "rogue.sleep", "sleep_duration": 200})

    def test_sleeping_gas_reuses_source_owned_timed_incapacity_and_pause(self):
        recipe = source("scenario.toml")["generation_recipes"]["exploration"]["hidden_traps"]
        rows = source("rules.toml")["overrides"]
        condition = next(row["value"] for row in rows
                         if row["path"] == ["conditions", recipe["sleep_condition"]])
        self.assertEqual(condition["lifecycle"], "timed")
        self.assertEqual(condition["restrictions"], ["voluntary_actions"])
        self.assertEqual(condition["preparation"], "pause")
        self.assertEqual(condition["timing"], {
            "kind": "actor_active", "on_expiry": {"kind": "remove"}})
        rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml")
                              .read_text(encoding="utf-8"))
        self.assertIn(recipe["rust_slot"], {slot["id"] for slot in rules["equipment_slots"]})


if __name__ == "__main__":
    unittest.main()
