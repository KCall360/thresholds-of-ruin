"""Opt-in magic mapping source binding; no stock/kit or pre-existing label edits."""
from pathlib import Path
import tomllib
import unittest

PRODUCTION = Path(__file__).resolve().parents[1] / "scenarios/rogue-exploration"

def source(name):
    return tomllib.loads((PRODUCTION / name).read_text(encoding="utf-8"))

class RogueMappingContent(unittest.TestCase):
    def test_mapping_is_sole_on_use_readable_operation(self):
        self.assertEqual(source("catalog/items.toml")["items"].get("rogue.magic_mapping"), {
            "name": "scroll of magic mapping", "identity": "rogue.magic_mapping", "asset": "item.scroll",
            "magical": True, "class": "scroll", "stackable": True, "appearance_pool": "rogue.scrolls",
            "readable": {"discovery": "on_use", "effects": [{"kind": "map_topology"}]}})

    def test_mapping_and_food_scroll_labels_extend_exact_existing_capacity(self):
        catalog = source("catalog/items.toml")
        self.assertEqual(catalog["appearance_pools"]["rogue.scrolls"]["appearances"], [
            "scroll titled VEL MOR", "scroll titled ELAM TOR", "scroll titled NAR SHEN", "scroll titled HETH ARA",
            "scroll titled TAL ION", "scroll titled UR SEK", "scroll titled KEL RAN", "scroll titled AN DOR",
            "scroll titled IV MEM", "scroll titled OS THAL", "scroll titled VEL TOR", "scroll titled SAH REN",
            "scroll titled OR VETH", "scroll titled NEM AL", "scroll titled TIR OS", "scroll titled KHA MER",
            "scroll titled LOR NEV", "scroll titled MIR VEL"])
        self.assertEqual(sum(item.get("appearance_pool") == "rogue.scrolls" for item in catalog["items"].values()), 18)

    def test_live_stock_and_starter_kit_are_unchanged(self):
        manifest = source("scenario.toml")
        self.assertNotIn("rogue.magic_mapping", repr(manifest["floor_activation"]))
        for character in manifest["characters"]:
            self.assertNotIn("rogue.magic_mapping", character["known_identities"])
            self.assertNotIn("rogue.magic_mapping", [row["archetype"] for row in character["loadout"]])

if __name__ == "__main__":
    unittest.main()
