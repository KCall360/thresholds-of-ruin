"""Curated recipes preserve budgets, ownership, and mirrored encounter identity."""
from collections import Counter
from pathlib import Path
import tempfile
import tomllib
import unittest
from arena_matrix import FAMILIES, HD_LEVELS, create_matrix, encounter, recipe, toml_source


class ArenaMatrixTests(unittest.TestCase):
    def test_generated_encounter_uses_current_scenario_format(self):
        manifest, _ = encounter("warrior_rank_heavy", 1, False, False, "current-test-ruleset")
        self.assertEqual(manifest["format"], 16)

    def test_named_starter_allocations_cover_one_six_and_eleven_with_exact_paid_forms(self):
        names = {"warrior_rank_heavy", "warrior_talent_heavy", "warrior_mixed",
                 "mage_rank_heavy", "mage_talent_heavy", "mage_mixed"}
        self.assertTrue(names.issubset(FAMILIES))
        self.assertTrue({1, 6, 11}.issubset(HD_LEVELS))
        for name in sorted(names):
            for hd in (1, 6, 11):
                with self.subTest(name=name, hd=hd):
                    build, _ = recipe(name, hd)
                    self.assertEqual(len(build["hit_dice"]), hd)
                    self.assertEqual({row["source"] for row in build["hit_dice"]}, {name.split("_")[0]})
                    self.assertEqual(sum(len(row["purchases"]) for row in build["hit_dice"]), hd)
                    self.assertEqual(build["attributes"], {"strength": 2, "speed": 2, "intellect": 2,
                                                         "willpower": 2, "awareness": 1, "presence": 1})
        fire = recipe("mage_talent_heavy", 1)[0]["hit_dice"][0]["purchases"][0]["selection"]
        cold = recipe("mage_mixed", 1)[0]["hit_dice"][0]["purchases"][0]["selection"]
        self.assertEqual(fire, {"kind": "talent_form", "talent": "evocation_initiate", "form": "spark_lance_fire"})
        self.assertEqual(cold, {"kind": "talent_form", "talent": "evocation_initiate", "form": "spark_lance_cold"})
        heavy = recipe("mage_talent_heavy", 6)[0]["hit_dice"]
        self.assertEqual(heavy[1]["purchases"][0]["selection"],
                         {"kind": "talent_form", "talent": "binding_initiate", "form": "falter"})
        self.assertEqual(heavy[2]["purchases"][0]["selection"],
                         {"kind": "talent_form", "talent": "warding_initiate", "form": "shell_impact"})
        for name in ("warrior_talent_heavy", "warrior_mixed"):
            selected = recipe(name, 11)[0]["hit_dice"][-1]["purchases"][0]["selection"]
            self.assertEqual(selected, {"kind": "talent", "talent": "open_the_guard"})

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
                            self.assertEqual(build["restoration"], "authored")
                            self.assertEqual(len(set(build["starting_skills"])), 2)
                            ranks = Counter(build["starting_skills"])
                            talents = []
                            attributes = build["attributes"].copy()
                            for ordinal, entry in enumerate(entries, 1):
                                self.assertEqual(set(entry) - {"attribute"}, {"source", "purchases"})
                                self.assertEqual(len(entry["purchases"]), 1)
                                purchase = entry["purchases"][0]
                                self.assertEqual(purchase["pool"], "advancement")
                                selection = purchase["selection"]
                                if selection["kind"] == "skill_rank":
                                    ranks.update([selection["skill"]])
                                else:
                                    self.assertIn(selection["kind"], ("talent", "talent_form"))
                                    talents.append(selection["talent"])
                                if "attribute" in entry:
                                    self.assertEqual(ordinal % 4, 0)
                                    attributes[entry["attribute"]] += 1
                            self.assertEqual(sum(ranks.values()) + len(talents), hd + 2)
                            self.assertEqual(len(set(talents)), len(talents))
                            self.assertLessEqual(max(ranks.values()), 5)
                            self.assertLessEqual(max(attributes.values()), 5)
                        count += 2
        expected = sum(not (hd == 1 and family in ("mixed", "warrior_mage_dip", "mage_warrior_dip"))
                       for family in FAMILIES for hd in HD_LEVELS)
        self.assertEqual(count, expected * 4)

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

    def test_rank_purchases_compete_with_talents_and_keep_required_starting_skills(self):
        for family in FAMILIES:
            build, style = recipe(family, 16)
            selections = [entry["purchases"][0]["selection"] for entry in build["hit_dice"]]
            with self.subTest(family=family):
                self.assertTrue(any(selection["kind"] == "skill_rank" for selection in selections))
                if family == "racial":
                    self.assertTrue(all(selection["kind"] == "skill_rank" for selection in selections))
                else:
                    self.assertTrue(any(selection["kind"] in ("talent", "talent_form") for selection in selections))
                if family in ("mixed", "warrior_mage_dip", "mage_warrior_dip"):
                    self.assertEqual(set(build["starting_skills"]), {style, "spellcasting"})
                elif family.startswith("mage_") or family in ("binding_control", "sustain"):
                    self.assertEqual(set(build["starting_skills"]), {"spellcasting", "heavy_weaponry"})
                else:
                    self.assertIn(style, build["starting_skills"])
        mixed = recipe("mixed", 4)[0]["hit_dice"]
        self.assertEqual(mixed[2]["purchases"][0]["selection"],
                         {"kind": "talent", "talent": "measured_cut"})
        self.assertEqual(mixed[3]["purchases"][0]["selection"], {"kind": "talent_form", "talent": "binding_initiate", "form": "falter"})
        sustain = recipe("sustain", 8)[0]["hit_dice"]
        self.assertEqual({entry["purchases"][0]["selection"]["talent"] for entry in sustain
                          if "talent" in entry["purchases"][0]["selection"]}, {"evocation_initiate"})

    def test_partitioned_terrain_blocks_every_cross_arena_cell(self):
        for mirrored in (False, True):
            _, region = encounter("occluded_terrain", 16, True, mirrored, "current-test-ruleset")
            self.assertEqual({tuple(cell) for cell in region["walls"]},
                             {(4, y, z) for y in range(5) for z in range(2)})
            self.assertNotEqual(region["anchors"]["start"][0], 4)
            self.assertNotEqual(region["actors"][0]["at"][0], 4)


class BundledRulesTests(unittest.TestCase):
    def test_imported_rules_are_materialized_with_dependency_identity(self):
        from ruleset_sources import load_rules, DEFAULT_COMPILER
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "import.toml"
            source.write_text('import = "interactions-v39"\noverrides = [{op = "set", path = ["resources", "mana", "recovery_period"], value = 897}]\n', encoding="utf-8")
            expected = load_rules(source)
            result = create_matrix(root / "matrix", DEFAULT_COMPILER, str(source), ["42"],
                                   families=["strength_heavy"], levels=[1])
            for path in (root / "matrix").rglob("rules.toml"):
                actual = tomllib.loads(path.read_text())
                self.assertEqual(actual, expected)
                self.assertNotIn("import", actual)
            self.assertEqual(set(result["rules_dependencies"]), {"interactions-v39"})

    def test_generated_packages_bundle_the_explicit_rules_source(self):
        from ruleset_sources import DEFAULT_COMPILER, DEFAULT_RULESET
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "chosen-rules.toml"
            source.write_bytes(DEFAULT_RULESET.read_bytes())
            create_matrix(root / "matrix", DEFAULT_COMPILER, str(source), ["42"],
                          families=["strength_heavy"], levels=[1])
            for path in (root / "matrix").rglob("rules.toml"):
                self.assertEqual(path.read_bytes(), source.read_bytes())
