"""Opt-in Haste dose source; existing finite stock and other policies stay fixed."""
from pathlib import Path
import tomllib
import unittest

PRODUCTION = Path(__file__).resolve().parents[1] / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((PRODUCTION / name).read_text(encoding="utf-8"))


class RogueHasteContent(unittest.TestCase):
    def test_haste_potion_uses_shared_condition_and_dedicated_rest_on_use(self):
        self.assertEqual(source("catalog/items.toml")["items"]["rogue.haste"], {
            "name": "potion of haste", "identity": "rogue.haste", "asset": "item.potion",
            "magical": True, "class": "potion", "stackable": True,
            "appearance_pool": "rogue.potions", "consumable": {
                "discovery": "on_use", "effects": [{"kind": "haste_dose",
                    "condition": "rogue.hasted", "rest": "rogue.haste_exhaustion"}]}})

    def test_haste_is_ninth_distinct_potion_identity_without_replacing_old_cosmetics(self):
        catalog = source("catalog/items.toml")
        self.assertEqual({name for name, item in catalog["items"].items()
                          if item.get("appearance_pool") == "rogue.potions"}, {
            "rogue.restore_strength", "rogue.healing", "rogue.extra_healing", "rogue.confusion",
            "rogue.blindness", "rogue.poison", "rogue.see_invisible", "rogue.levitation", "rogue.haste"})
        self.assertEqual(catalog["appearance_pools"]["rogue.potions"]["appearances"], [
            "amber potion", "smoky potion", "violet potion", "azure potion", "crimson potion",
            "silver potion", "pearl potion", "teal potion", "ochre potion"])

    def test_existing_live_stock_and_starter_kits_do_not_gain_haste_potion(self):
        manifest = source("scenario.toml")
        self.assertNotIn("rogue.haste'", repr(manifest["floor_activation"]))
        stage = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][7]
        self.assertEqual(stage, {"id": "rogue.magic_devices", "version": 1, "kind": "items",
            "count": [1, 1], "quantity": 1, "bands": [{"depth": {"kind": "open", "minimum": 1},
                "choices": [{"archetype": "rogue.wand_" + kind, "weight": 1} for kind in
                    ("fire", "cold", "lightning", "invisibility", "haste", "slow", "teleport_away", "teleport_to")]}]})
        for character in manifest["characters"]:
            self.assertNotIn("rogue.haste", character["known_identities"])
            self.assertNotIn("rogue.haste", [row["archetype"] for row in character["loadout"]])


if __name__ == "__main__":
    unittest.main()
