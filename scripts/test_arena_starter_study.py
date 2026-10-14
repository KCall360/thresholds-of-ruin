"""The starter study is distinct from the STR-heavy diagnostic matrix."""
from collections import Counter
import unittest

from arena_starter_study import study_cases


class StarterStudyTests(unittest.TestCase):
    def test_all_nine_allocation_pairs_at_each_required_band_have_two_class_relative_views(self):
        cases = study_cases()
        self.assertEqual(len(cases), 54,
                         "starter study requires 27 matchups and both class-relative reference views")
        expected = {(warrior, mage, hd, evaluated)
                    for warrior in ("warrior_rank_heavy", "warrior_talent_heavy", "warrior_mixed")
                    for mage in ("mage_rank_heavy", "mage_talent_heavy", "mage_mixed")
                    for hd in (1, 6, 11) for evaluated in ("warrior", "mage")}
        self.assertEqual({(case["warrior"], case["mage"], case["hit_dice"], case["evaluated_class"])
                          for case in cases}, expected)
        self.assertEqual(len({case["id"] for case in cases}), 54)
        self.assertEqual(Counter(case["hit_dice"] for case in cases), {1: 18, 6: 18, 11: 18})
        for case in cases:
            self.assertEqual(case["reference"], case["evaluated_class"] + "_mixed")
            self.assertEqual(case["candidate"], case[case["evaluated_class"]])
            other = "mage" if case["evaluated_class"] == "warrior" else "warrior"
            self.assertEqual(case["opponent"], case[other])

    def test_only_evaluated_paid_build_changes_between_reference_and_candidate(self):
        import copy
        import tomllib
        from arena_matrix import recipe, toml_source
        from arena_starter_study import study_encounter
        for case in study_cases():
            with self.subTest(case=case["id"]):
                baseline, left = study_encounter(case, "baseline", "forward", "rules.toml")
                candidate, right = study_encounter(case, "candidate", "forward", "rules.toml")
                self.assertEqual(left, right)
                self.assertEqual(candidate["characters"][0]["creature"], recipe(case["candidate"], case["hit_dice"])[0])
                self.assertEqual(baseline["characters"][0]["creature"], recipe(case["reference"], case["hit_dice"])[0])
                self.assertEqual(baseline["archetypes"], candidate["archetypes"])
                self.assertEqual(baseline["creatures"], candidate["creatures"])
                for variant in (baseline, candidate):
                    self.assertEqual(tomllib.loads(toml_source(variant)), variant)
                    self.assertEqual(variant["arena"], {"participants": [1, 2], "control": "all_ai",
                                                       "ticks": 20000, "actions": 2000})
                compared = copy.deepcopy(candidate)
                compared["characters"][0]["creature"] = baseline["characters"][0]["creature"]
                self.assertEqual(compared, baseline)
                self.assertEqual(candidate["creatures"]["species"]["subject"],
                                 candidate["creatures"]["species"]["opponent"])

    def test_mirroring_preserves_content_and_swaps_only_positions(self):
        import copy
        from arena_starter_study import study_encounter
        for case in study_cases():
            for variant in ("baseline", "candidate"):
                forward, left = study_encounter(case, variant, "forward", "rules.toml")
                mirrored, right = study_encounter(case, variant, "mirrored", "rules.toml")
                self.assertEqual(forward, mirrored)
                reflected = copy.deepcopy(left)
                reflected["anchors"]["start"][0] = 8 - reflected["anchors"]["start"][0]
                reflected["actors"][0]["at"][0] = 8 - reflected["actors"][0]["at"][0]
                self.assertEqual(reflected, right)

    def test_run_manifest_bounds_runs_without_treating_repeated_baselines_as_samples(self):
        from arena_starter_study import FORMAT, run_manifest
        manifest = run_manifest()
        self.assertEqual(manifest["format"], FORMAT)
        self.assertNotEqual(FORMAT, "tor-arena-matrix-v1")
        self.assertEqual(manifest["seeds"], list(map(str, range(32))))
        self.assertTrue(manifest["complete_design"])
        self.assertFalse(manifest["pilot"])
        self.assertEqual(manifest["planned_seed_count"], 32)
        self.assertEqual(manifest["battle_evidence"], "not_run")
        self.assertEqual(manifest["calibration"], "not_assessed")
        self.assertEqual((manifest["selected_matchups"], manifest["selected_class_relative_comparisons"]), (27, 54))
        self.assertEqual((manifest["package_count"], manifest["expected_runs"]), (216, 6912))
        self.assertEqual(manifest["maximum_run_timeout_seconds"], 6912 * 30)
        self.assertEqual(manifest["maximum_replay_timeout_seconds"], 6912 * 30)
        self.assertIn("seed within a matchup", manifest["sampling_unit"])
        self.assertIn("reused mixed-reference baselines", manifest["nonindependent"])
        self.assertIn("forward and mirrored orientations", manifest["nonindependent"])
        self.assertTrue(manifest["evidence_requirements"]["pending_instrumentation"])
        self.assertTrue(manifest["evidence_requirements"]["ordinary_process"])
        self.assertIn("net balance is spending", manifest["evidence_requirements"]["not_inferred"])

    def test_explicit_pilot_keeps_required_coverage_and_missing_comparisons_visible(self):
        from arena_starter_study import run_manifest
        chosen = study_cases()[0]
        manifest = run_manifest(["42"], 0.5, [chosen["id"]])
        self.assertFalse(manifest["complete_design"])
        self.assertTrue(manifest["pilot"])
        self.assertEqual(manifest["required_bands"], [1, 6, 11])
        self.assertEqual(manifest["required_class_relative_comparisons"], 54)
        self.assertEqual(manifest["selected_class_relative_comparisons"], 1)
        self.assertEqual(manifest["expected_runs"], 4)
        self.assertEqual(manifest["maximum_run_timeout_seconds"], 2)
        self.assertEqual(manifest["cases"][0]["plan"], chosen["id"] + "/plan.json")

    def test_one_seed_complete_layout_remains_an_unrun_pilot(self):
        from arena_starter_study import run_manifest
        manifest = run_manifest(["42"])
        self.assertTrue(manifest["complete_design"])
        self.assertTrue(manifest["pilot"])
        self.assertEqual(manifest["planned_seed_count"], 1)
        self.assertEqual(manifest["default_seed_count"], 32)
        self.assertEqual(manifest["battle_evidence"], "not_run")
        self.assertEqual(manifest["calibration"], "not_assessed")

    def test_all_termination_classes_pair_only_complete_elimination_seeds(self):
        from arena_evaluation import summarize
        observations = []
        for value, outcome in enumerate(("elimination", "draw", "action_limit", "tick_limit",
                                         "stalemate", "failure", "missing")):
            for variant in ("baseline", "candidate"):
                for orientation in ("forward", "mirrored"):
                    if outcome == "missing" and variant == "candidate" and orientation == "mirrored":
                        continue
                    winner = "blue" if variant == "candidate" else "red"
                    termination = {"type": "elimination", "winner": winner}
                    observation = {"seed": str(value), "variant": variant, "orientation": orientation,
                                   "status": "ok", "report": {"termination": termination}}
                    if outcome == "draw":
                        termination["winner"] = None
                    elif outcome == "failure" and variant == "candidate" and orientation == "mirrored":
                        observation = {"seed": str(value), "variant": variant, "orientation": orientation,
                                       "status": "failure", "message": "Evaluation timeout"}
                    elif outcome in ("action_limit", "tick_limit", "stalemate"):
                        termination.clear()
                        termination["type"] = outcome
                    observations.append(observation)
        summary = summarize(observations, "blue")
        self.assertEqual(summary["paired_seeds"], 2)
        self.assertEqual(summary["excluded_seeds"], ["2", "3", "4", "5", "6"])
        self.assertEqual(summary["mean_candidate_win_difference"], 0.5)
        self.assertEqual(summary["terminations"]["baseline"],
                         {"elimination": 8, "action_limit": 2, "tick_limit": 2, "stalemate": 2})
        self.assertEqual(summary["terminations"]["candidate"],
                         {"elimination": 6, "action_limit": 2, "tick_limit": 2, "stalemate": 2, "failure": 1})

    def test_authored_purchase_concentration_keeps_free_ranks_out_of_the_paid_denominator(self):
        from arena_starter_study import allocation_spend
        portfolios = allocation_spend()
        self.assertEqual(len(portfolios), 18)
        self.assertEqual(len({(row["family"], row["hit_dice"]) for row in portfolios}), 18)
        for row in portfolios:
            self.assertEqual(row["shared_points_spent"], row["hit_dice"])
            self.assertEqual(row["paid_ranks"] + row["paid_talents"], row["shared_points_spent"])
            self.assertEqual(len(row["free_starting_ranks"]), 2)
            self.assertEqual(len(row["selections"]), row["shared_points_spent"])
        rank_controls = [row for row in portfolios if row["family"].endswith("rank_heavy")
                         and row["hit_dice"] == 1]
        self.assertEqual(len(rank_controls), 2)
        self.assertTrue(all(row["paid_talents"] == 0 for row in rank_controls))
        mage = next(row for row in portfolios if row["family"] == "mage_talent_heavy" and row["hit_dice"] == 6)
        self.assertEqual({selection["form"] for selection in mage["selections"] if selection["kind"] == "talent_form"},
                         {"spark_lance_fire", "falter", "shell_impact"})

    def test_unknown_cases_invalid_seeds_and_unbounded_timeouts_reject(self):
        from arena_starter_study import run_manifest, study_encounter
        case = study_cases()[0]
        for seeds in ([], ["42", "42"], ["01"], [42], ["1"] * 1001):
            with self.subTest(seeds=seeds[:2]), self.assertRaises(ValueError):
                run_manifest(seeds)
        for timeout in (0, -1, True, float("inf"), float("nan"), 3601):
            with self.subTest(timeout=timeout), self.assertRaises(ValueError):
                run_manifest(timeout=timeout)
        for ids in ([], ["unknown"], [case["id"], case["id"]]):
            with self.subTest(ids=ids), self.assertRaises(ValueError):
                run_manifest(case_ids=ids)
        for selected, variant, orientation in ((dict(case, hit_dice=2), "candidate", "forward"),
                                               (case, "reference", "forward"),
                                               (case, "candidate", "backward")):
            with self.assertRaises(ValueError):
                study_encounter(selected, variant, orientation, "rules.toml")

    def test_generator_keeps_partial_diagnostics_on_compiler_failure_or_timeout(self):
        import subprocess
        import tempfile
        from pathlib import Path
        from unittest.mock import patch
        from arena_starter_study import create_study
        case_id = study_cases()[0]["id"]
        for failure in (subprocess.CompletedProcess([], 1, "", "authored package rejected"),
                        subprocess.TimeoutExpired("compiler", 30)):
            with self.subTest(failure=type(failure).__name__), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                compiler = root / "compiler"
                compiler.write_bytes(b"test compiler identity")
                rules = root / "rules.toml"
                rules.write_text('id = "test-rules"\n', encoding="utf-8")
                output = root / "study"
                behavior = {"side_effect": failure} if isinstance(failure, Exception) else {"return_value": failure}
                with patch("arena_starter_study.resolve_with_dependencies", return_value=({"id": "test-rules"}, {})), \
                        patch("arena_starter_study.subprocess.run", **behavior) as launched:
                    with self.assertRaises((ValueError, subprocess.TimeoutExpired)):
                        create_study(output, compiler, rules, ["42"], case_ids=[case_id])
                self.assertEqual(launched.call_count, 1)
                self.assertEqual(launched.call_args.kwargs["timeout"], 30)
                self.assertTrue((output / "partial.json").is_file())
                self.assertFalse((output / "study.json").exists())
                self.assertEqual(rules.read_text(encoding="utf-8"), 'id = "test-rules"\n')

    def test_generator_fingerprints_inputs_and_rejects_midgeneration_source_drift(self):
        import json
        import subprocess
        import tempfile
        from pathlib import Path
        from unittest.mock import patch
        from arena_starter_study import create_study
        case_id = study_cases()[0]["id"]
        for drift in (False, True):
            with self.subTest(drift=drift), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                compiler = root / "compiler"
                compiler.write_bytes(b"test compiler identity")
                rules = root / "rules.toml"
                original = 'id = "test-rules"\n'
                rules.write_text(original, encoding="utf-8")
                output = root / "study"
                def validate(*args, **kwargs):
                    if drift:
                        rules.write_text(original + "# changed\n", encoding="utf-8")
                    return subprocess.CompletedProcess(args[0], 0, "", "")
                with patch("arena_starter_study.resolve_with_dependencies", return_value=({"id": "test-rules"}, {})), \
                        patch("arena_starter_study.subprocess.run", side_effect=validate):
                    if drift:
                        with self.assertRaisesRegex(ValueError, "source or compiler changed"):
                            create_study(output, compiler, rules, ["42"], case_ids=[case_id])
                        self.assertFalse((output / "study.json").exists())
                    else:
                        inventory = create_study(output, compiler, rules, ["42"], case_ids=[case_id])
                        self.assertEqual(inventory["generation_status"], "validated")
                        self.assertEqual(inventory["validated_packages"], 4)
                        self.assertEqual(len(inventory["cases"][0]["package_inputs"]), 4)
                        self.assertIn("arena_recipes.json", inventory["generator_inputs"])
                        self.assertEqual(inventory, json.loads((output / "study.json").read_text(encoding="utf-8")))
                        self.assertFalse((output / "partial.json").exists())
                        for materialized in output.rglob("rules.toml"):
                            self.assertEqual(materialized.read_text(encoding="utf-8"), original)
                        with self.assertRaises(FileExistsError):
                            create_study(output, compiler, rules, ["42"], case_ids=[case_id])

    def test_partial_inventory_survives_resolver_failure_and_midcase_interruption(self):
        import json
        import subprocess
        import tempfile
        from pathlib import Path
        from unittest.mock import patch
        from arena_starter_study import create_study
        case_id = study_cases()[0]["id"]
        for resolving in (True, False):
            with self.subTest(resolving=resolving), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                compiler = root / "compiler"
                compiler.write_bytes(b"test compiler identity")
                rules = root / "rules.toml"
                rules.write_text('id = "test-rules"\n', encoding="utf-8")
                output = root / "study"
                resolution = ({"side_effect": ValueError("Cannot run rules resolver: timeout")}
                              if resolving else {"return_value": ({"id": "test-rules"}, {})})
                validations = [subprocess.CompletedProcess([], 0, "", ""), KeyboardInterrupt()]
                with patch("arena_starter_study.resolve_with_dependencies", **resolution), \
                        patch("arena_starter_study.subprocess.run", side_effect=validations) as launched:
                    with self.assertRaises(ValueError if resolving else KeyboardInterrupt):
                        create_study(output, compiler, rules, ["42"], case_ids=[case_id])
                self.assertFalse((output / "study.json").exists())
                retained = json.loads((output / "partial.json").read_text(encoding="utf-8"))
                self.assertEqual(retained["generation_status"], "resolving_rules" if resolving else "preparing")
                self.assertEqual(retained["validated_packages"], 0 if resolving else 1)
                self.assertEqual(launched.call_count, 0 if resolving else 2)
                if not resolving:
                    self.assertEqual(set(retained["cases"][0]["package_inputs"]), {"baseline-forward"})
                self.assertEqual(retained["battle_evidence"], "not_run")
                self.assertTrue(retained["pilot"])
