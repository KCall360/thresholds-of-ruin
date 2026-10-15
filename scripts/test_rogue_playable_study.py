"""The playable study keeps exact kits, points and an explicit AI proxy policy."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch

from rogue_roster_study import (
    DEFAULT_RULES, PLAYABLE_AI, PLAYABLE_AI_NAME, PRODUCTION, ROOT, create_study, encounter_sources,
    run_manifest, source_cases, study_encounter,
)


class RoguePlayableStudyTests(unittest.TestCase):
    def test_playable_modes_bound_the_eight_recipe_candidates_to_real_hd_one(self):
        for reference in ("playable-unspent", "playable-first-purchase"):
            inventory = run_manifest(case_ids=["hobgoblin-hd1", "zombie-hd1", "quagga-hd1"],
                                     reference=reference)
            self.assertEqual(inventory["reference"], reference)
            self.assertEqual(inventory["required_cases"], 8)
            self.assertEqual(inventory["required_reference_hd"], [1])
            self.assertEqual(inventory["expected_runs"], 24)
            self.assertEqual(inventory["seeds"], ["0", "1"])
            self.assertEqual(inventory["battle_evidence"], "not_run")
            self.assertEqual(inventory["calibration"], "not_assessed")
            self.assertEqual(inventory["faction"], "delvers")
            self.assertEqual(inventory["arms"], {"baseline": "warrior", "candidate": "mage"})
            self.assertEqual(set(row["species"] for row in
                                 source_cases(encounter_sources(), reference)), {
                "rogue.emu", "rogue.snake", "rogue.hobgoblin", "rogue.zombie",
                "rogue.quagga", "rogue.ice_monster", "rogue.black_unicorn", "rogue.vampire"})
        for reference, cases in (("unknown", None), ("playable-unspent", ["centaur-hd1"]),
                                 ("playable-first-purchase", ["quagga-hd6"])):
            with self.assertRaises(ValueError):
                run_manifest(case_ids=cases, reference=reference)

    def test_vampire_study_retains_eight_racial_rows_and_source_disposition_without_stock_claims(self):
        source = encounter_sources()
        before = copy.deepcopy(source)
        for reference in ("playable-unspent", "playable-first-purchase"):
            inventory = run_manifest(case_ids=["vampire-hd1"], reference=reference)
            self.assertEqual(inventory["cases"], [{"id": "vampire-hd1", "species": "rogue.vampire",
                "reference_hd": 1, "foe_hd": 8, "plan": "vampire-hd1/plan.json"}])
            self.assertEqual(inventory["production_activation"], "not_selected")
            self.assertIn("birth stock", " ".join(inventory["limitations"]))
            case = next(row for row in source_cases(source, reference) if row["id"] == "vampire-hd1")
            for arm in ("baseline", "candidate"):
                manifest, region = study_encounter(case, arm, "forward", source, reference=reference)
                self.assertEqual(region["actors"][0]["archetype"], "rogue.vampire")
                self.assertEqual(region["actors"][0]["ai"], "rogue_hostile")
                self.assertNotIn("floor_activation", manifest)
                self.assertNotIn("creatures", manifest)
        self.assertEqual(source, before)

    def test_actual_selected_ids_kit_attributes_and_unspent_build_are_not_arena_substitutes(self):
        source = encounter_sources()
        original = copy.deepcopy(source)
        case = next(row for row in source_cases(source, "playable-unspent")
                    if row["species"] == "rogue.hobgoblin")
        for arm, selected in (("baseline", 1), ("candidate", 2)):
            manifest, region = study_encounter(case, arm, "forward", source,
                                              reference="playable-unspent")
            expected = copy.deepcopy(source["manifest"]["characters"][selected - 1])
            # New reconstruction: only geometry and explicit study control differ.
            expected.update(anchor="1/start", unselected="ai", ai=PLAYABLE_AI_NAME)
            self.assertEqual(manifest["characters"], [expected])
            self.assertEqual(manifest["default_character"], selected)
            self.assertEqual(manifest["arena"]["participants"], [selected, 3])
            self.assertEqual(region["actors"], [{"id": 3, "at": [7, 2, 0],
                "archetype": case["species"], "controller": "ai", "ai": "rogue_hostile"}])
            self.assertEqual(manifest["modules"], source["manifest"]["modules"])
            self.assertEqual(manifest["ai_profiles"], {**source["manifest"]["ai_profiles"],
                                                       PLAYABLE_AI_NAME: PLAYABLE_AI})
            self.assertEqual(manifest["factions"], source["manifest"]["factions"])
            self.assertNotIn("creatures", manifest)
            self.assertNotIn("archetypes", manifest)
            self.assertEqual(expected["creature"]["hit_dice"][0]["purchases"], [])
            self.assertEqual(expected["creature"]["nutrition"], {"kind": "required"})
        self.assertEqual(source, original)

    def test_first_purchase_spends_one_point_and_does_not_invent_a_default_spell(self):
        source = encounter_sources()
        case = source_cases(source, "playable-first-purchase")[0]
        choices = {
            "baseline": {"kind": "talent", "talent": "rattling_blow"},
            "candidate": {"kind": "talent_form", "talent": "evocation_initiate",
                          "form": "spark_lance_cold"},
        }
        for arm, choice in choices.items():
            fresh, _ = study_encounter(case, arm, "forward", source, reference="playable-unspent")
            purchased, _ = study_encounter(case, arm, "forward", source,
                                           reference="playable-first-purchase")
            expected = copy.deepcopy(fresh)
            expected["id"] = purchased["id"]
            expected["characters"][0]["creature"]["hit_dice"][0]["purchases"] = [
                {"pool": "advancement", "selection": choice}]
            self.assertEqual(purchased, expected)
        for reference, spent in (("playable-unspent", 0), ("playable-first-purchase", 1)):
            inventory = run_manifest(reference=reference)
            for build in inventory["reference_builds"].values():
                self.assertEqual(build["points"], {"earned": 1, "spent": spent, "unspent": 1 - spent})
                self.assertEqual(build["growth_evidence"], "authored_hd_one_not_earned_route")
                self.assertEqual(build["ai_profile"], PLAYABLE_AI)
                self.assertEqual(len(build["loadout"]), 4)
                self.assertEqual(build["item_definitions"], source["items"])

    def test_mirrors_preserve_actual_actor_identity_and_every_nonspatial_operand(self):
        source = encounter_sources()
        for reference in ("playable-unspent", "playable-first-purchase"):
            for case in source_cases(source, reference):
                for arm in ("baseline", "candidate"):
                    manifest, region = study_encounter(case, arm, "forward", source, reference=reference)
                    mirrored_manifest, mirrored = study_encounter(case, arm, "mirrored", source,
                                                                   reference=reference)
                    self.assertEqual(manifest, mirrored_manifest)
                    expected = copy.deepcopy(region)
                    expected["anchors"]["start"][0] = 8 - expected["anchors"]["start"][0]
                    expected["actors"][0]["at"][0] = 8 - expected["actors"][0]["at"][0]
                    self.assertEqual(mirrored, expected)

    def test_generated_playable_packages_retain_items_and_exact_per_class_receipts(self):
        imported = tomllib.loads((ROOT / PRODUCTION / "rules.toml").read_text())["import"]
        dependencies = {imported: hashlib.sha256(
            (ROOT / DEFAULT_RULES).read_bytes().replace(b"\r\n", b"\n")).hexdigest()}
        with tempfile.TemporaryDirectory() as directory:
            compiler = Path(directory) / "compiler"
            compiler.write_bytes(b"explicit test compiler")
            output = Path(directory) / "study"
            with patch("rogue_roster_study.resolve_with_dependencies",
                       return_value=({"id": "test-rules"}, dependencies)), patch(
                    "rogue_roster_study.subprocess.run",
                    return_value=subprocess.CompletedProcess([], 0, "", "")) as launched:
                result = create_study(output, compiler, case_ids=["hobgoblin-hd1"],
                                      reference="playable-first-purchase")
            self.assertEqual(launched.call_count, 4)
            self.assertEqual(result["validated_packages"], 4)
            for arm, selected in (("baseline", 1), ("candidate", 2)):
                for orientation in ("forward", "mirrored"):
                    package = output / "hobgoblin-hd1" / (arm + "-" + orientation)
                    self.assertEqual((package / "catalog/items.toml").read_bytes(),
                                     (ROOT / PRODUCTION / "catalog/items.toml").read_bytes())
                    manifest = tomllib.loads((package / "scenario.toml").read_text())
                    self.assertEqual(manifest["default_character"], selected)
                    self.assertEqual(manifest["arena"]["participants"], [selected, 3])
            plan = json.loads((output / "hobgoblin-hd1/plan.json").read_text())
            self.assertEqual(plan["faction"], "delvers")
            self.assertFalse((output / "partial.json").exists())


if __name__ == "__main__":
    unittest.main()
