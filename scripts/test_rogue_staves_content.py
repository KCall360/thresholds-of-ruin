"""Independent physical-form and unchanged effect-family authoring contracts."""
from pathlib import Path
import tomllib
import unittest

from rogue_staves_package import METALS, WOODS, expected_forms

PRODUCTION = Path(__file__).resolve().parents[1] / "scenarios/rogue-exploration"
EFFECTS = {
    "fire": "fire", "cold": "cold", "lightning": "lightning",
    "invisibility": "invisibility", "haste": "haste monster", "slow": "slow monster",
    "teleport_away": "teleport away", "teleport_to": "teleport to", "nothing": "nothing",
    "light": "light", "drain_life": "drain life", "cancellation": "cancellation",
    "magic_missile": "magic missile", "polymorph": "polymorph",
}


def source(name):
    return tomllib.loads((PRODUCTION / name).read_text(encoding="utf-8"))


class RogueStavesContent(unittest.TestCase):
    def test_all_fourteen_effects_share_identity_and_explicit_wand_staff_physical_forms(self):
        catalog = source("catalog/items.toml")
        devices = {identity: item for identity, item in catalog["items"].items() if "device" in item}
        self.assertEqual(set(devices), {"rogue.wand_" + effect for effect in EFFECTS})
        for effect, name in EFFECTS.items():
            identity = "rogue.wand_" + effect
            item = devices[identity]
            with self.subTest(effect=effect):
                self.assertEqual(item.get("forms"), expected_forms(name))
                self.assertEqual(item["identity"], identity)
                self.assertEqual(item["class"], "wand")
                self.assertEqual(item["device"]["ability"], identity)
                self.assertEqual(item["device"]["initial_charges"],
                                 {"minimum": 10, "maximum": 19} if effect == "light"
                                 else {"minimum": 3, "maximum": 7})
                self.assertFalse(item["stackable"])
                self.assertNotIn("equipment_generation", item)
                self.assertNotIn("equipment", item)
                self.assertNotIn("projectile", item)
                self.assertEqual(item["appearance_pool"], "rogue.wands")

    def test_separate_metal_and_wood_pools_cover_every_possible_effect_assignment(self):
        pools = source("catalog/items.toml")["appearance_pools"]
        self.assertEqual(pools["rogue.wands"], {"asset": "item.wand",
                                              "appearances": [metal + " wand" for metal in METALS]})
        self.assertEqual(pools["rogue.staves"], {"asset": "item.wand",
                                               "appearances": [wood + " staff" for wood in WOODS]})
        self.assertEqual(len(set(METALS)), len(EFFECTS))
        self.assertEqual(len(set(WOODS)), len(EFFECTS))


if __name__ == "__main__":
    unittest.main()
