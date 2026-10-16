"""New reconstruction tests; retained playable tests remain separately identified."""
import copy
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch

from arena_evaluation import digest, participant_contract
from arena_matrix import recipe, toml_source
from rogue_roster_study import (
    AI_SOURCES, DEFAULT_RULES, INPUTS, PLAYABLE_AI, PLAYABLE_AI_NAME, PRODUCTION,
    REFERENCES, ROOT, create_study, encounter_sources, run_manifest,
    source_cases, study_encounter,
)


class RogueRosterStudyTests(unittest.TestCase):
    def dependencies(self):
        imported = tomllib.loads((ROOT / PRODUCTION / "rules.toml").read_text())["import"]
        return {imported: hashlib.sha256((ROOT / DEFAULT_RULES).read_bytes().replace(b"\r\n", b"\n")).hexdigest()}

    def test_diagnostics_are_ten_fixed_foes_at_three_reference_bands_not_playable_starts(self):
        source = encounter_sources()
        before = copy.deepcopy(source)
        cases = source_cases(source)
        self.assertEqual(len(cases), 30)
        self.assertEqual({row["reference_hd"] for row in cases}, {1, 6, 11})
        for case in cases:
            for arm, family in (("baseline", "warrior_mixed"), ("candidate", "mage_mixed")):
                manifest, region = study_encounter(case, arm, "forward", source)
                self.assertEqual(manifest["characters"][0]["creature"], recipe(family, case["reference_hd"])[0])
                self.assertEqual(manifest["modules"], ["catalog/creatures.toml", "catalog/actors.toml"])
                self.assertEqual(set(manifest["creatures"]["species"]), {"subject"})
                self.assertNotIn("archetypes", manifest)
                self.assertEqual(region["actors"], [{"id": 2, "at": [7, 2, 0],
                    "archetype": case["species"], "controller": "ai", "ai": "rogue_hostile"}])
                self.assertEqual(manifest["factions"], {"blue": ["dungeon_foes"], "dungeon_foes": ["blue"]})
                self.assertEqual(tomllib.loads(toml_source(manifest)), manifest)
        self.assertEqual(source, before)

    def test_proxy_values_match_existing_authoritative_general_policy_not_default(self):
        self.assertEqual(PLAYABLE_AI, {"care": True, "equipment": True, "loot": True, "flee_percent": 25})
        for relative in AI_SOURCES:
            text = (ROOT / relative).read_text()
            body = text.split("pub fn general() -> Self {", 1)[1].split("}", 1)[0]
            fields = dict(re.findall(r"\b(care|equipment|loot|flee_percent): (true|false|[0-9]+)", body))
            self.assertEqual({key: json.loads(value) for key, value in fields.items()}, PLAYABLE_AI)
        source = encounter_sources()
        self.assertNotIn(PLAYABLE_AI_NAME, source["manifest"]["ai_profiles"])
        self.assertTrue(all(row["unselected"] == "omit" for row in source["manifest"]["characters"]))
        policy = run_manifest(reference="playable-unspent")["controller_policy"]
        self.assertEqual(policy["provenance"], "AiProfile::general()")
        self.assertFalse(policy["human_route_evidence"])
        self.assertEqual(policy["opening_action"], "none")
        self.assertEqual(policy["species_disposition"], "unchanged")
        self.assertIn("no contact is not combat survival", " ".join(
            run_manifest(reference="playable-unspent")["limitations"]))

    def test_snapshot_reader_returns_exact_authored_sources_without_inserting_a_fake_normal_profile(self):
        snapshot = {name: (ROOT / name).read_bytes() for name in INPUTS}
        self.assertEqual(encounter_sources(snapshot), encounter_sources())
        self.assertEqual(encounter_sources(snapshot)["manifest"],
                         tomllib.loads(snapshot[PRODUCTION + "scenario.toml"].decode()))

    def test_case_and_seed_inventory_exposes_omissions_and_freezes_timeout_budget(self):
        full = run_manifest()
        self.assertEqual((full["required_cases"], full["package_count"], full["expected_runs"]), (30, 120, 240))
        self.assertEqual(full["missing_cases"], [])
        small = run_manifest(["42"], 0.5, ["hobgoblin-hd1"], reference="playable-unspent")
        self.assertEqual((small["required_cases"], len(small["missing_cases"])), (10, 9))
        self.assertEqual((small["expected_runs"], small["maximum_replay_timeout_seconds"]), (4, 2))
        self.assertTrue(small["pilot"])
        self.assertFalse(small["complete_design"])
        self.assertEqual(small["battle_evidence"], "not_run")
        self.assertEqual(small["production_activation"], "not_selected")

    def test_invalid_inputs_reject_without_materialization(self):
        for values in ([], ["0", "0"], ["01"], [1], ["1"] * 1001):
            with self.subTest(seeds=values[:2]), self.assertRaises(ValueError):
                run_manifest(values)
        for timeout in (0, -1, True, float("nan"), float("inf"), 3601):
            with self.subTest(timeout=timeout), self.assertRaises(ValueError):
                run_manifest(timeout=timeout)
        for ids in ([], ["unknown"], ["emu-hd1", "emu-hd1"]):
            with self.assertRaises(ValueError):
                run_manifest(case_ids=ids)
        source = encounter_sources()
        case = source_cases(source)[0]
        for value, arm, orientation in ((dict(case, reference_hd=3), "baseline", "forward"),
                                         (case, "warrior", "forward"), (case, "baseline", "backward")):
            with self.assertRaises(ValueError):
                study_encounter(value, arm, orientation, source)

    def test_generated_plans_declare_actual_per_arm_rosters_for_run_finish_load_replay(self):
        for reference in REFERENCES:
            with self.subTest(reference=reference), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                compiler = root / "compiler"
                compiler.write_bytes(b"unit compiler")
                output = root / "study"
                with patch("rogue_roster_study.resolve_with_dependencies", return_value=({"id": "test"}, self.dependencies())), \
                        patch("rogue_roster_study.subprocess.run", return_value=subprocess.CompletedProcess([], 0, "", "")):
                    result = create_study(output, compiler, ["0"], case_ids=["hobgoblin-hd1"], reference=reference)
                plan = json.loads((output / "hobgoblin-hd1/plan.json").read_text())
                playable = reference != "arena-diagnostic"
                expected = {arm: [{"actor": str(selected if playable else 1),
                                   "faction": "delvers" if playable else "blue"},
                                  {"actor": "3" if playable else "2", "faction": "dungeon_foes"}]
                            for arm, selected in (("baseline", 1), ("candidate", 2))}
                self.assertEqual(plan["participant_rosters"], expected)
                participant_contract(plan)
                self.assertEqual(result["source_inputs"], {name: digest(ROOT / name) for name in INPUTS})
                self.assertEqual(result["compiler_sha256"], digest(compiler))
                for name in INPUTS:
                    self.assertEqual((output / "sources" / name).read_bytes(), (ROOT / name).read_bytes())
                self.assertEqual(result["validated_packages"], 4)
                self.assertEqual(json.loads((output / "study.json").read_text()), result)
                for name, hashes in result["cases"][0]["package_inputs"].items():
                    self.assertTrue(hashes)
                    self.assertIn("catalog/actors.toml", hashes)
                with self.assertRaises(FileExistsError):
                    create_study(output, compiler, case_ids=["hobgoblin-hd1"], reference=reference)

    def test_failed_stale_and_timed_out_compilers_keep_partial_inventory_and_source_snapshot(self):
        for failure in ("reject", "stale", "timeout"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                compiler = root / "compiler"
                compiler.write_bytes(b"unit compiler")
                output = root / "study"
                action = ({"side_effect": subprocess.TimeoutExpired("compiler", 30)} if failure == "timeout" else
                          {"return_value": subprocess.CompletedProcess([], 2, "", "reject fixture")})
                with patch("rogue_roster_study.resolve_with_dependencies",
                           return_value=({"id": "test"}, {} if failure == "stale" else self.dependencies())), \
                        patch("rogue_roster_study.subprocess.run", **action) as launched:
                    with self.assertRaises((ValueError, subprocess.TimeoutExpired)):
                        create_study(output, compiler, case_ids=["emu-hd1"], reference="playable-unspent")
                self.assertEqual(launched.call_count, 0 if failure == "stale" else 1)
                partial = json.loads((output / "partial.json").read_text())
                self.assertEqual(partial["generation_status"], "failed")
                self.assertEqual(partial["validated_packages"], 0)
                self.assertTrue(partial["failure"]["message"])
                self.assertFalse((output / "study.json").exists())
                self.assertEqual((output / "sources" / PRODUCTION / "scenario.toml").read_bytes(),
                                 (ROOT / PRODUCTION / "scenario.toml").read_bytes())

    def test_midgeneration_failure_keeps_exact_success_count_and_pending_package(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            compiler = root / "compiler"
            compiler.write_bytes(b"unit compiler")
            output = root / "study"
            with patch("rogue_roster_study.resolve_with_dependencies", return_value=({"id": "test"}, self.dependencies())), \
                    patch("rogue_roster_study.subprocess.run", side_effect=[subprocess.CompletedProcess([], 0, "", ""),
                         subprocess.CompletedProcess([], 2, "", "rejected mirror")]):
                with self.assertRaisesRegex(ValueError, "rejected mirror"):
                    create_study(output, compiler, case_ids=["emu-hd1"])
            partial = json.loads((output / "partial.json").read_text())
            self.assertEqual(partial["validated_packages"], 1)
            self.assertEqual(partial["pending_package"], "emu-hd1/baseline-mirrored")
            self.assertFalse((output / "emu-hd1/plan.json").exists())
            self.assertFalse((output / "study.json").exists())

    def test_source_or_compiler_drift_never_publishes_a_validated_inventory(self):
        for changed in (DEFAULT_RULES, "scripts/arena_recipes.json", AI_SOURCES[0], "compiler"):
            with self.subTest(changed=changed), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                # A real directory/.. alias covers canonicalization without
                # relying on Windows 8.3 names or permission to create symlinks.
                compiler_parent = root / "compiler-parent"
                compiler_parent.mkdir()
                compiler = compiler_parent / ".." / "compiler"
                compiler.write_bytes(b"unit compiler")
                resolved_compiler = compiler.resolve(strict=True)
                self.assertNotEqual(compiler, resolved_compiler)
                self.assertTrue(compiler.samefile(resolved_compiler))
                output = root / "study"
                calls = 0
                def fingerprint(path):
                    nonlocal calls
                    if Path(path) == resolved_compiler:
                        calls += 1
                        if changed == "compiler" and calls > 1:
                            return "drift"
                    return "drift" if path == ROOT / changed else digest(path)
                with patch("rogue_roster_study.resolve_with_dependencies", return_value=({"id": "test"}, self.dependencies())), \
                        patch("rogue_roster_study.subprocess.run", return_value=subprocess.CompletedProcess([], 0, "", "")), \
                        patch("rogue_roster_study.digest", side_effect=fingerprint):
                    with self.assertRaisesRegex(ValueError, "source or compiler changed"):
                        create_study(output, compiler, case_ids=["emu-hd1"])
                self.assertEqual(calls, 2)
                self.assertFalse((output / "study.json").exists())
                partial = json.loads((output / "partial.json").read_text())
                self.assertEqual(partial["generation_status"], "failed")
                self.assertEqual(partial["validated_packages"], 4)

    def test_changed_starter_hd_rejects_instead_of_silently_fabricating_a_playable_build(self):
        source = encounter_sources()
        source["manifest"]["characters"][1]["creature"]["hit_dice"].append({"source": "mage", "purchases": []})
        case = source_cases(source, "playable-unspent")[0]
        with self.assertRaisesRegex(ValueError, "actual unspent one-HD"):
            study_encounter(case, "candidate", "forward", source, reference="playable-unspent")


if __name__ == "__main__":
    unittest.main()
