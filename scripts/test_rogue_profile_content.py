"""The Rogue-only package: one locked build and Rogue's mechanics only.

It shares the rogue-exploration catalog by copy. Divergence is deliberate per
tuning milestone, so any catalog difference must be listed here.
"""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue"
STARTER = ROOT / "scenarios/rogue-exploration"


def load(path):
    return tomllib.loads(path.read_text(encoding="utf-8"))


def ranks(build):
    counts = {skill: 1 for skill in build["starting_skills"]}
    for row in build["hit_dice"] + build["growth_schedule"]:
        for purchase in row.get("purchases", []):
            if purchase["selection"]["kind"] == "skill_rank":
                skill = purchase["selection"]["skill"]
                counts[skill] = counts.get(skill, 0) + 1
    return counts


class RogueProfileContent(unittest.TestCase):
    def setUp(self):
        self.manifest = load(PACKAGE / "scenario.toml")
        self.rules = load(PACKAGE / "rules.toml")

    def test_one_preset_with_a_locked_scheduled_build(self):
        [character] = self.manifest["characters"]
        self.assertEqual(self.manifest["default_character"], character["id"])
        build = character["creature"]
        self.assertEqual(build["restoration"], "authored")
        self.assertEqual(build["future_growth"], {"kind": "authored"})
        self.assertEqual([row["source"] for row in build["hit_dice"]], ["racial", "warrior"])
        schedule = build["growth_schedule"]
        # Rogue's experience table has 20 thresholds after level 1.
        self.assertEqual(len(schedule), 20)
        self.assertTrue(all(row["source"] == "warrior" for row in schedule))
        self.assertTrue(all(len(row["purchases"]) <= 1 for row in schedule))
        self.assertTrue(all(count <= 5 for count in ranks(build).values()), ranks(build))

    def test_capabilities_disable_owner_choices_and_corpses(self):
        self.assertEqual(self.manifest["capabilities"], {
            "npc_stair_traversal": False, "owner_advancement": False, "corpses": False})

    def test_rogue_starting_kit_has_no_bandages(self):
        [character] = self.manifest["characters"]
        kit = {row["archetype"]: row["quantity"] for row in character["loadout"]}
        self.assertEqual(kit, {"rogue.mace": 1, "rogue.ring_mail": 1, "rogue.shortbow": 1,
                               "rogue.arrow": 32, "rogue.ration": 1})
        self.assertTrue(set(kit) <= set(character["known_identities"]))

    def test_no_bandage_stock_or_universal_care(self):
        stages = self.manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
        self.assertNotIn("rogue.bandages", [stage["id"] for stage in stages])
        for pool in self.manifest["floor_activation"]["carried_item_pools"].values():
            self.assertFalse([row for row in pool["choices"] if "bandage" in row["archetype"]])
        [defaults] = [row["value"] for row in self.rules["overrides"]
                      if row["path"] == ["constants", "default_abilities"]]
        self.assertNotIn("bind_wounds", defaults)

    def test_only_listed_sources_differ_from_the_starter_package(self):
        for name in ["catalog/actors.toml", "catalog/bodies.toml", "catalog/creatures.toml",
                     "catalog/items.toml"]:
            self.assertEqual((PACKAGE / name).read_bytes(), (STARTER / name).read_bytes(), name)
        starter = load(STARTER / "rules.toml")
        changed = [row["path"] for row, old in zip(self.rules["overrides"], starter["overrides"])
                   if row != old]
        self.assertEqual(changed, [["constants", "default_abilities"]])
        self.assertEqual(len(self.rules["overrides"]), len(starter["overrides"]))


if __name__ == "__main__":
    unittest.main()
