"""Independent authored living-dungeon contracts; never invokes a compiler."""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1] / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((ROOT / name).read_text(encoding="utf-8"))


class RogueLivingDungeonContent(unittest.TestCase):
    def test_wandering_is_separate_from_initial_population_and_excludes_six_species(self):
        policy = source("scenario.toml")["floor_activation"]["wandering"]
        self.assertEqual(policy, {
            "stage_id": "rogue.eight_species_foes",
            "excluded_wandering_content": ["rogue.dragon", "rogue.ice_monster", "rogue.leprechaun",
                                           "rogue.nymph", "rogue.venus_flytrap", "rogue.xeroc"],
            "initial_delay": [6700, 7300], "retry_ticks": 400,
            "chance": {"numerator": 1, "denominator": 6}, "living_cap": 8})

    def test_ring_edges_are_perception_capabilities_not_global_check_penalties(self):
        items = source("catalog/items.toml")["items"]
        self.assertEqual(items["rogue.ring_stealth"]["equipment"]["modifiers"], [
            {"scope": "always", "op": {"kind": "add", "stat": {"kind": "nutrition_rate_thirtieths"}, "value": 30}},
            {"scope": "always", "op": {"kind": "perception", "value": {"kind": "continual_stealth", "edge": 1}}}])
        self.assertEqual(items["rogue.ring_aggravation"]["equipment"]["modifiers"], [
            {"scope": "always", "op": {"kind": "perception", "value": {
                "kind": "aggravation", "interval": 500, "chance_denominator": 3, "stealth_disadvantage": 1}}}])
        self.assertEqual(items["rogue.ring_aggravation"]["equipment_generation"], {"kind": "cursed"})

    def test_five_control_scrolls_are_finite_and_scare_has_no_check_operand(self):
        items, manifest = source("catalog/items.toml")["items"], source("scenario.toml")
        expected = {
            "aggravate_monster": {"kind": "aggravate"},
            "create_monster": {"kind": "create_monster"},
            "hold_monster": {"kind": "hold_nearby", "condition": "rogue.held", "duration": 1000, "radius": 2},
            "monster_confusion": {"kind": "arm_confusion", "condition": "rogue.potion_confusion", "duration": 2000},
            "scare_monster": {"kind": "nothing"},
        }
        for name, control in expected.items():
            item = items["rogue." + name]
            self.assertEqual(item["readable"]["control"], control)
            self.assertTrue(item["stackable"])
            self.assertEqual(item["appearance_pool"], "rogue.scrolls")
            for character in manifest["characters"]:
                self.assertNotIn("rogue." + name, character["known_identities"])
                self.assertNotIn("rogue." + name, [v["archetype"] for v in character["loadout"]])
        self.assertEqual(items["rogue.scare_monster"]["ground_scare"], {
            "condition": "rogue.scared", "duration": 1000})
        stage = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][12]
        self.assertEqual(stage, {"id": "rogue.control_scrolls", "version": 1, "kind": "items",
            "count": [1, 1], "quantity": 1, "bands": [{"depth": {"kind": "open", "minimum": 1},
                "choices": [{"archetype": "rogue." + name, "weight": 1} for name in expected]}]})
