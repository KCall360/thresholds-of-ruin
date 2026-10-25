"""Opt-in permanent growth doses preserve existing stock and restoration."""
from pathlib import Path
import tomllib
import unittest

PRODUCTION = Path(__file__).resolve().parents[1] / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((PRODUCTION / name).read_text(encoding="utf-8"))


class RogueGrowthContent(unittest.TestCase):
    def assert_dose(self, kind, name):
        self.assertEqual(source("catalog/items.toml")["items"]["rogue." + kind], {
            "name": "potion of " + name, "identity": "rogue." + kind, "asset": "item.potion",
            "magical": True, "class": "potion", "stackable": True,
            "appearance_pool": "rogue.potions", "consumable": {
                "discovery": "on_use", "effects": [{"kind": kind}]}})

    def test_gain_strength_is_dedicated_on_use_dose(self):
        self.assert_dose("gain_strength", "gain strength")

    def test_raise_level_is_dedicated_on_use_dose(self):
        self.assert_dose("raise_level", "raise level")

    def test_growth_adds_two_distinct_potion_identities_and_cosmetics(self):
        catalog = source("catalog/items.toml")
        self.assertEqual({key for key, item in catalog["items"].items()
                          if item.get("appearance_pool") == "rogue.potions"}, {
            "rogue.restore_strength", "rogue.healing", "rogue.extra_healing", "rogue.confusion",
            "rogue.blindness", "rogue.poison", "rogue.see_invisible", "rogue.levitation", "rogue.haste",
            "rogue.gain_strength", "rogue.raise_level", "rogue.hallucination", "rogue.monster_detection", "rogue.magic_detection"})
        self.assertEqual(catalog["appearance_pools"]["rogue.potions"]["appearances"], [
            "amber potion", "smoky potion", "violet potion", "azure potion", "crimson potion",
            "silver potion", "pearl potion", "teal potion", "ochre potion", "indigo potion", "copper potion", "chartreuse potion", "saffron potion", "cerulean potion"])

    def test_existing_restoration_and_live_stock_are_unchanged(self):
        manifest = source("scenario.toml")
        for kind in ("rogue.gain_strength", "rogue.raise_level"):
            self.assertNotIn(kind, repr(manifest["floor_activation"]))
            for character in manifest["characters"]:
                self.assertNotIn(kind, character["known_identities"])
                self.assertNotIn(kind, [row["archetype"] for row in character["loadout"]])
        stage = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][7]
        self.assertEqual(stage, {"id": "rogue.magic_devices", "version": 1, "kind": "items",
            "count": [1, 1], "quantity": 1, "bands": [{"depth": {"kind": "open", "minimum": 1},
                "choices": [{"archetype": "rogue.wand_" + kind, "weight": 1} for kind in
                    ("fire", "cold", "lightning", "invisibility", "haste", "slow", "teleport_away", "teleport_to")]}]})
        self.assertEqual(source("catalog/items.toml")["items"]["rogue.restore_strength"]["consumable"], {
            "discovery": "self_effect", "effects": [{"kind": "restore_attribute", "attribute": "strength",
                "amount": {"kind": "fixed", "amount": 1000}}]})


if __name__ == "__main__":
    unittest.main()
