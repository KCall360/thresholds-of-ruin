"""Curated recipes preserve budgets, ownership, and mirrored encounter identity."""
from collections import Counter
import tomllib
import unittest
from arena_matrix import FAMILIES, HD_LEVELS, encounter, recipe, toml_source


class ArenaMatrixTests(unittest.TestCase):
    def test_all_curated_encounters_roundtrip_and_mirror_without_changing_builds(self):
        count = 0
        for family in FAMILIES:
            for hd in HD_LEVELS:
                if hd == 1 and family in ("mixed", "warrior_mage_dip", "mage_warrior_dip"):
                    with self.assertRaises(ValueError):
                        recipe(family, hd)
                    continue
                for candidate in (False, True):
                    with self.subTest(family=family, hd=hd, candidate=candidate):
                        forward, left = encounter(family, hd, candidate, False, "current-test-ruleset")
                        mirrored, right = encounter(family, hd, candidate, True, "current-test-ruleset")
                        self.assertEqual(forward, mirrored)
                        for data in (forward, left, right):
                            self.assertEqual(tomllib.loads(toml_source(data)), data)
                        self.assertEqual(left["anchors"]["start"][0] + right["anchors"]["start"][0], 8)
                        self.assertEqual(left["actors"][0]["at"][0] + right["actors"][0]["at"][0], 8)
                        self.assertEqual(left["actors"][0]["id"], right["actors"][0]["id"])
                        self.assertEqual(left.get("walls"), right.get("walls"))
                        self.assertEqual(forward["arena"]["participants"], [1, 2])
                        for build in (forward["characters"][0]["creature"],
                                      forward["archetypes"]["opponent"]["creature"]):
                            entries = build["hit_dice"]
                            self.assertEqual(len(entries), hd)
                            self.assertEqual(sum(build["attributes"].values()), 10)
                            self.assertEqual(len({entry["talent"] for entry in entries}), hd)
                            ranks = Counter()
                            attributes = build["attributes"].copy()
                            for ordinal, entry in enumerate(entries, 1):
                                self.assertEqual(len(entry["training"]),
                                                 1 if entry["source"] == "racial" else 2)
                                ranks.update(entry["training"])
                                if "attribute" in entry:
                                    self.assertEqual(ordinal % 4, 0)
                                    attributes[entry["attribute"]] += 1
                            self.assertLessEqual(max(ranks.values()), 5)
                            self.assertLessEqual(max(attributes.values()), 5)
                        count += 2
        self.assertEqual(count, 268)

    def test_source_families_and_bindings_are_explicit(self):
        self.assertEqual({entry["source"] for entry in recipe("racial", 16)[0]["hit_dice"]}, {"racial"})
        for family in ["mixed", "warrior_mage_dip", "mage_warrior_dip"]:
            self.assertEqual({entry["source"] for entry in recipe(family, 2)[0]["hit_dice"]}, {"warrior", "mage"})
        for binding in ["intellect", "willpower", "awareness", "presence"]:
            self.assertEqual(recipe("mage_" + binding, 4)[0]["binding"], binding)
        self.assertEqual(recipe("speed_light", 4)[1], "light_weaponry")
        for args in [("unknown", 1), ("racial", 3), ("racial", 0), ("racial", True), ("racial", 1.0)]:
            with self.assertRaises(ValueError):
                recipe(*args)

    def test_partitioned_terrain_blocks_every_cross_arena_cell(self):
        for mirrored in (False, True):
            _, region = encounter("occluded_terrain", 16, True, mirrored, "current-test-ruleset")
            self.assertEqual({tuple(cell) for cell in region["walls"]},
                             {(4, y, z) for y in range(5) for z in range(2)})
            self.assertNotEqual(region["anchors"]["start"][0], 4)
            self.assertNotEqual(region["actors"][0]["at"][0], 4)
