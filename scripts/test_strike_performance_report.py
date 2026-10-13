"""Strict ordered-strike workload identity, measurement and recovery contracts."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import unittest

import perf_compare
import strike_performance_report as report
from workload_report import InvalidWorkload


def rows():
    result = []
    for strikes, riders in ((1, 0), (3, 2), (16, 8)):
        for actors in (2, 256):
            for regions in (16, 256):
                for mode in ("checkpoint", "tail"):
                    for sample, seed in enumerate((42, 43, 44)):
                        commands = 1 + (strikes * 100 + 149) // 150
                        prefix = 1 + int(strikes > 1)
                        row = dict(workload="ordered-strikes", version=1, strikes=strikes,
                                   riders=riders, actors=actors, regions=regions, mode=mode,
                                   sample=sample, seed=seed, loaded_actors=actors,
                                   loaded_regions=regions, setup_commands=actors,
                                   setup_checkpoint_sequence=actors,
                                   checkpoint_interval=1 if mode == "checkpoint" else 4096,
                                   prefix_completed=int(strikes > 1), command_count=commands,
                                   prefix_commands=prefix, continuation_commands=commands-prefix,
                                   primary_hits=strikes, target_health_before=1_000_000,
                                   target_health_after=1_000_000-strikes,
                                   command_ms=[1.] * commands, sequence_ms=float(commands),
                                   admission_start_ms=1., advancement_ms=float(commands-1))
                        for field in ("setup_ms", "plan_encoding_ms", "checkpoint_encoding_ms",
                                      "save_ms", "resume_ms", "final_save_ms", "final_resume_ms",
                                      "comparison_projection_ms", "simulation_ms", "perception_ms",
                                      "navigation_ms", "revision_ms", "checkpoint_capture_ms"):
                            row[field] = 1.
                        for field in ("plan_json_bytes", "checkpoint_json_bytes", "saved_bytes",
                                      "final_saved_bytes", "disclosed_bytes"):
                            row[field] = 100
                        for field in ("ability_plans", "game_clones", "preparation_candidates",
                                      "actor_table_copies", "observations", "scenes", "body_cells",
                                      "actor_candidates", "health_derivations", "rider_resolutions",
                                      "revision_comparisons", "actors_observed"):
                            row[field] = 0
                        row["ability_plans"] = 1
                        row["strike_resolutions"] = strikes
                        for stage, count in (("prefix", prefix), ("final", commands)):
                            row[f"{stage}_records_loaded"] = actors + count
                            row[f"{stage}_checkpoint_sequence"] = actors + (count if mode == "checkpoint" else 0)
                            row[f"{stage}_records_replayed"] = 0 if mode == "checkpoint" else count
                            row[f"{stage}_state_equal"] = True
                            row[f"{stage}_journal_equal"] = True
                        result.append(row)
    return result


class StrikeReports(unittest.TestCase):
    def test_full_matrix_keeps_modes_dimensions_seeds_and_interval_counts(self):
        measured = rows()
        self.assertEqual(len(measured), 72)
        summarized = report.summarize(measured)
        self.assertEqual(len(summarized), 24)
        for case in summarized:
            self.assertEqual(case["sequence_ms"]["n"], 3)
            self.assertEqual(case["sequence_ms"]["p95"], case["sequence_ms"]["maximum"])
            self.assertEqual(case["seeds"], [42, 43, 44])
            self.assertIn("small n", case["caution"])
            self.assertEqual(case["command_ms"]["n"], 3 * (1 + (case["strikes"] * 100 + 149) // 150))

    def test_missing_duplicate_extra_and_malformed_case_or_seed_are_rejected(self):
        for mutate in (lambda r: r.pop(), lambda r: r.append(copy.deepcopy(r[0])),
                       lambda r: r[0].update(seed=43), lambda r: r[0].update(sample=3),
                       lambda r: r[0].update(strikes=2), lambda r: r[0].update(actors=2.0),
                       lambda r: r[0].update(strikes=[]), lambda r: r[0].update(mode={}),
                       lambda r: r[0].update(regions=True), lambda r: r[0].update(version=True),
                       lambda r: r[0].update(mode="replay"), lambda r: r[0].update(workload="combat")):
            changed = rows(); mutate(changed)
            with self.subTest(row=changed[0]), self.assertRaises(InvalidWorkload):
                report.summarize(changed)

    def test_all_intervals_reject_boolean_negative_and_nonfinite_values(self):
        for field, value in rows()[0].items():
            if not field.endswith("_ms"):
                continue
            for invalid in (True, -1, float("nan"), float("inf"), "1"):
                changed = rows()
                changed[0][field] = [invalid] * len(value) if isinstance(value, list) else invalid
                with self.subTest(field=field, invalid=invalid), self.assertRaises(InvalidWorkload):
                    report.summarize(changed)

    def test_counter_and_byte_types_and_bounds_are_enforced(self):
        for field, value in rows()[0].items():
            if type(value) is not int:
                continue
            for invalid in (True, -1, 0.5):
                changed = rows(); changed[0][field] = invalid
                with self.subTest(field=field, invalid=invalid), self.assertRaises(InvalidWorkload):
                    report.summarize(changed)
        for field in ("plan_json_bytes", "checkpoint_json_bytes", "saved_bytes", "final_saved_bytes", "disclosed_bytes"):
            changed = rows(); changed[0][field] = 0
            with self.subTest(field=field), self.assertRaises(InvalidWorkload):
                report.summarize(changed)

    def test_missing_and_private_or_unknown_fields_are_rejected(self):
        for field in rows()[0]:
            changed = rows(); del changed[0][field]
            with self.subTest(field=field), self.assertRaises(InvalidWorkload):
                report.summarize(changed)
        for field in ("plan", "frozen_operands", "strikes_data", "unexpected"):
            changed = rows(); changed[0][field] = {"secret": 123}
            with self.subTest(field=field), self.assertRaises(InvalidWorkload):
                report.summarize(changed)

    def test_command_primary_rider_and_dimension_claims_match_measurements(self):
        for update in ({"strike_resolutions": 0}, {"rider_resolutions": 1}, {"primary_hits": 2},
                       {"loaded_actors": 3}, {"loaded_regions": 17}, {"prefix_completed": 1},
                       {"command_count": 1}, {"command_ms": []}, {"sequence_ms": 20.},
                       {"admission_start_ms": 2.}, {"advancement_ms": 2.},
                       {"prefix_commands": 2}, {"continuation_commands": 2}):
            changed = rows(); changed[0].update(update)
            with self.subTest(update=update), self.assertRaises(InvalidWorkload):
                report.summarize(changed)
        changed = rows()
        row = next(r for r in changed if r["strikes"] == 16)
        row["rider_resolutions"] = 129
        with self.assertRaises(InvalidWorkload): report.summarize(changed)

    def test_recovery_requires_real_checkpoint_and_exact_bounded_tail(self):
        for mode in ("checkpoint", "tail"):
            for field, value in (("setup_checkpoint_sequence", 0), ("checkpoint_interval", 0),
                                 ("prefix_checkpoint_sequence", 0), ("final_checkpoint_sequence", 0),
                                 ("prefix_records_replayed", 99), ("final_records_replayed", 99),
                                 ("prefix_records_loaded", 99), ("final_records_loaded", 99),
                                 ("prefix_state_equal", False), ("final_state_equal", 1),
                                 ("prefix_journal_equal", False), ("final_journal_equal", None)):
                changed = rows(); next(r for r in changed if r["mode"] == mode)[field] = value
                with self.subTest(mode=mode, field=field), self.assertRaises(InvalidWorkload):
                    report.summarize(changed)

    def test_smoke_selection_is_explicit_and_does_not_relax_the_default(self):
        smoke = [r for r in rows() if (r["strikes"], r["riders"], r["actors"], r["regions"]) == (3, 2, 2, 16)]
        self.assertEqual(len(report.summarize(smoke, smoke=True)), 2)
        for measured, kwargs in ((smoke, {}), (rows(), {"smoke": True}), (smoke[:-1], {"smoke": True})):
            with self.assertRaises(InvalidWorkload): report.summarize(measured, **kwargs)

    def test_real_optimized_interpreter_preserves_validation(self):
        for valid in (True, False):
            measured = rows()
            if not valid: measured[0]["save_ms"] = True
            child = subprocess.run([sys.executable, "-O", "-c",
                "import json,sys; import strike_performance_report as r; r.summarize(json.load(sys.stdin))"],
                input=json.dumps(measured), text=True, capture_output=True,
                cwd=Path(__file__).parent, timeout=15)
            self.assertEqual(child.returncode == 0, valid, child.stderr)
            if not valid: self.assertIn("InvalidWorkload", child.stderr)

    def test_registry_retains_full_identity_and_flat_measurements(self):
        spec = perf_compare.WORKLOADS["strikes"]
        self.assertEqual(spec.example, "strike_bench")
        self.assertEqual(spec.validator, "scripts/strike_performance_report.py")
        self.assertEqual(perf_compare.parse_cases(["strikes"], 1)[0].validator_args(["--smoke"]), ["--smoke"])
        self.assertEqual(spec.group(rows()[0]), "s1-q0-a2-r16-checkpoint")
        timings, counts, version = perf_compare.extract_rows([rows()[0]], spec)
        group = "s1-q0-a2-r16-checkpoint"
        self.assertEqual(version, 1)
        self.assertEqual(timings[group]["command_ms"], [1., 1.])
        self.assertEqual(counts[group]["strike_resolutions"], 1)
        for field in ("strikes", "riders", "actors", "regions", "sample", "seed", "version"):
            self.assertNotIn(field, counts[group])


if __name__ == "__main__":
    unittest.main()
