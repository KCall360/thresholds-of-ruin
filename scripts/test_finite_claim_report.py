"""Independent finite-claim report fixtures and malformed-report regressions."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import finite_claim_report as report
from workload_report import InvalidWorkload


# These are the published workload cases, deliberately not imported from the validator.
def specifications():
    cases = []
    for access in ("carried", "ground"):
        for costs, fragments in ((1, 1), (1, 32), (8, 8), (8, 32)):
            cases.append((f"{access}-c{costs}-f{fragments}", access, costs, fragments, 1, 1, fragments, 1))
    cases.extend((
        ("last-unit-2", "ground", 1, 1, 2, 1, 1, 1),
        ("shared-stack-27", "ground", 1, 1, 27, 27, 27, 27),
        ("shared-stacks-64", "ground", 1, 1, 64, 64, 64, 8),
    ))
    return cases


def rows():
    result = []
    for case, access, costs, fragments, owners, successes, units, maximum_holds in specifications():
        for background in (0, 64):
            for mode in ("checkpoint", "tail"):
                for sample in range(3):
                    actors = owners + 1 + background
                    advances = 1 + owners - successes
                    prefix = 2 * owners
                    commands = prefix + advances
                    row = dict(
                        workload="finite-claims", version=1, case=case, background_actors=background,
                        mode=mode, sample=sample, seed=42 + sample, access=access,
                        costs=costs, fragments_per_use=fragments, competing_owners=owners,
                        loaded_actors=actors, loaded_regions=16, setup_records=actors,
                        setup_checkpoint_sequence=actors, checkpoint_interval=1 if mode == "checkpoint" else 4096,
                        admitted_uses=owners, successful_starts=successes, failed_starts=owners-successes,
                        successful_resolutions=successes, consumed_units=units,
                        prefix_held_fragments=successes * fragments, prefix_claim_owners=successes,
                        prefix_max_item_holds=maximum_holds, prefix_held_units=units,
                        final_held_fragments=0, final_claim_owners=0, final_max_item_holds=0, final_held_units=0,
                        item_hold_limit=64, fragment_limit=32, cost_limit=8,
                        physical_units_before=units, physical_units_at_prefix=units, physical_units_after=0,
                        prefix_records=prefix, final_records=commands, advancement_commands=advances,
                        admission_ms=[1.] * owners, start_ms=[2.] * owners, advancement_ms=[3.] * advances,
                        measured_ms=float(3 * owners + 3 * advances),
                        plan_count=successes, plan_max_json_bytes=100, plan_total_json_bytes=100 * successes,
                        prefix_checkpoint_json_bytes=1000, final_checkpoint_json_bytes=2000,
                        prefix_sqlite_file_bytes=4096, final_sqlite_file_bytes=8192,
                    )
                    for phase in ("admission", "start", "settlement"):
                        for counter in ("game_clones", "scenes", "ordinary_scenes", "observations",
                                        "ability_plans", "preparation_candidates", "strike_resolutions",
                                        "scene_membership_lookups", "scene_membership_cells", "scene_cells_copied"):
                            row[f"{phase}_{counter}"] = 0
                    row["start_ability_plans"] = successes
                    row["settlement_strike_resolutions"] = successes
                    for field in ("setup_ms", "diagnostic_ms", "plan_encoding_ms", "prefix_checkpoint_encoding_ms",
                                  "final_checkpoint_encoding_ms", "prefix_save_ms", "prefix_resume_ms",
                                  "final_save_ms", "final_resume_ms", "comparison_ms"):
                        row[field] = 1.
                    for stage, count in (("prefix", prefix), ("final", commands)):
                        row[f"{stage}_records_loaded"] = actors + count
                        row[f"{stage}_checkpoint_sequence"] = actors + (count if mode == "checkpoint" else 0)
                        row[f"{stage}_records_replayed"] = 0 if mode == "checkpoint" else count
                        row[f"{stage}_state_equal"] = True
                        row[f"{stage}_journal_equal"] = True
                        row[f"{stage}_claims_equal"] = True
                    result.append(row)
    return result


class FiniteClaimReports(unittest.TestCase):
    def test_complete_matrix_preserves_actual_work_and_separate_byte_units(self):
        measured = rows()
        self.assertEqual(len(measured), 132)
        summarized = report.summarize(measured)
        self.assertEqual(len(summarized), 44)
        for case in summarized:
            self.assertEqual(case["measured_ms"]["n"], 3)
            self.assertEqual(case["measured_ms"]["p95"], case["measured_ms"]["maximum"])
            self.assertEqual(case["seeds"], [42, 43, 44])
            self.assertIn("small n", case["caution"])
            self.assertIn("prefix_checkpoint_json_bytes", case)
            self.assertIn("prefix_sqlite_file_bytes", case)
        crowded = next(case for case in summarized if case["case"] == "shared-stacks-64")
        self.assertEqual(crowded["prefix_claim_owners"], {"min": 64, "max": 64})
        self.assertEqual(crowded["prefix_max_item_holds"], {"min": 8, "max": 8})

    def rejected(self, update=None, *, delete=None, value=...):
        measured = rows()
        if update is not None:
            measured[0].update(update)
        if delete is not None:
            del measured[0][delete]
        if value is not ...:
            measured[0] = value
        with self.assertRaises(InvalidWorkload):
            report.summarize(measured)

    def test_missing_duplicate_and_extra_cases_never_form_a_complete_matrix(self):
        for mutate in (lambda r: r.pop(), lambda r: r.append(copy.deepcopy(r[0])),
                       lambda r: r[0].update(case="invented"), lambda r: r[0].update(sample=3),
                       lambda r: r[0].update(seed=43), lambda r: r[0].update(mode="replay")):
            measured = rows(); mutate(measured)
            with self.subTest(mutate=mutate), self.assertRaises(InvalidWorkload):
                report.summarize(measured)

    def test_schema_is_closed_and_types_are_exact_before_case_hashing(self):
        for field in rows()[0]:
            with self.subTest(missing=field):
                self.rejected(delete=field)
        for update in ({"plan": {}}, {"held_items": []}, {"unexpected": 1}, {"version": True},
                       {"case": []}, {"mode": {}}, {"background_actors": 0.0},
                       {"workload": "ordered-strikes"}, {"access": False}):
            with self.subTest(update=update): self.rejected(update)
        for value in ([], [1], "sample", 2, True, None):
            with self.subTest(value=value): self.rejected(value=value)

    def test_every_integer_field_rejects_boolean_fraction_negative_and_unbounded_values(self):
        for field, value in rows()[0].items():
            if type(value) is not int:
                continue
            for invalid in (True, -1, 0.5, 2**64):
                with self.subTest(field=field, invalid=invalid): self.rejected({field: invalid})

    def test_all_intervals_reject_boolean_negative_nonfinite_and_text_values(self):
        for field, value in rows()[0].items():
            if not field.endswith("_ms"):
                continue
            for invalid in (True, -1, float("nan"), float("inf"), "1"):
                replacement = [invalid] * len(value) if isinstance(value, list) else invalid
                with self.subTest(field=field, invalid=invalid): self.rejected({field: replacement})
        for field in ("admission_ms", "start_ms", "advancement_ms"):
            for invalid in ([], 1., None, [1., 1.], {"duration": 1.}):
                with self.subTest(field=field, invalid=invalid): self.rejected({field: invalid})
        self.rejected({"measured_ms": 999.})

    def test_fixed_case_contract_requires_actual_reservations_settlement_and_dimensions(self):
        for update in ({"costs": 9}, {"fragments_per_use": 33}, {"item_hold_limit": 65},
                       {"fragment_limit": 64}, {"cost_limit": 9}, {"loaded_regions": 256},
                       {"loaded_actors": 1}, {"competing_owners": 64}, {"admitted_uses": 0},
                       {"successful_starts": 0}, {"failed_starts": 1}, {"successful_resolutions": 0},
                       {"consumed_units": 0}, {"physical_units_before": 0}, {"physical_units_at_prefix": 0},
                       {"physical_units_after": 1}, {"prefix_held_fragments": 0},
                       {"prefix_claim_owners": 0}, {"prefix_max_item_holds": 64},
                       {"prefix_held_units": 0}, {"final_held_fragments": 1}, {"final_claim_owners": 1},
                       {"final_max_item_holds": 1}, {"final_held_units": 1}, {"plan_count": 0},
                       {"settlement_strike_resolutions": 0}, {"admission_strike_resolutions": 1},
                       {"start_strike_resolutions": 1}):
            with self.subTest(update=update): self.rejected(update)

    def test_bounded_logical_sizes_and_real_file_lengths_are_independent(self):
        for field in ("plan_max_json_bytes", "plan_total_json_bytes", "prefix_checkpoint_json_bytes",
                      "final_checkpoint_json_bytes", "prefix_sqlite_file_bytes", "final_sqlite_file_bytes"):
            with self.subTest(field=field): self.rejected({field: 0})
        self.rejected({"plan_max_json_bytes": 16_385})
        self.rejected({"plan_total_json_bytes": 99})
        self.rejected({"prefix_checkpoint_json_bytes": 64 * 1024 * 1024 + 1})
        changed = rows()
        changed[0]["prefix_sqlite_file_bytes"] += 4096
        self.assertEqual(len(report.summarize(changed)), 44)

    def test_recovery_requires_real_nonzero_checkpoint_exact_tail_and_all_equality_checks(self):
        for update in ({"setup_records": 0}, {"setup_checkpoint_sequence": 0},
                       {"checkpoint_interval": 0}, {"prefix_records": 0}, {"final_records": 0}):
            with self.subTest(update=update): self.rejected(update)
        for mode in ("checkpoint", "tail"):
            for stage in ("prefix", "final"):
                for suffix, value in (("records_loaded", 999), ("checkpoint_sequence", 0),
                                      ("records_replayed", 999), ("state_equal", False),
                                      ("journal_equal", 1), ("claims_equal", None)):
                    measured = rows()
                    next(row for row in measured if row["mode"] == mode)[f"{stage}_{suffix}"] = value
                    with self.subTest(mode=mode, stage=stage, suffix=suffix), self.assertRaises(InvalidWorkload):
                        report.summarize(measured)

    def test_smoke_is_complete_explicit_and_never_accepted_as_full(self):
        smoke = [r for r in rows() if r["case"] in ("ground-c8-f32", "last-unit-2") and r["background_actors"] == 0]
        self.assertEqual(len(report.summarize(smoke, smoke=True)), 4)
        for measured, flag in ((smoke, False), (rows(), True), (smoke[:-1], True)):
            with self.assertRaises(InvalidWorkload): report.summarize(measured, smoke=flag)
        with self.assertRaises(InvalidWorkload): report.summarize(smoke, smoke=1)

    def test_strict_json_lines_rejects_skipped_garbage_duplicate_keys_and_empty_input(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "samples.jsonl"
            for text in ("", "not-json\n", '{}\nignored\n', '{"version":1,"version":2}\n', '[1]\n'):
                path.write_text(text, encoding="utf-8")
                with self.subTest(text=text), self.assertRaises(InvalidWorkload):
                    report.read_rows(path)
            path.write_text("\n".join(json.dumps(row) for row in rows()), encoding="utf-8")
            self.assertEqual(report.read_rows(path), rows())

    def test_repeatability_reports_physical_deltas_but_rejects_changed_logical_work(self):
        first, second = rows(), rows()
        second[0]["setup_ms"] = 123.
        second[0]["prefix_sqlite_file_bytes"] += 4096
        comparison = report.compare(first, second)
        self.assertTrue(comparison["logical_equal"])
        self.assertEqual(comparison["physical_difference_rows"], 1)
        self.assertEqual(comparison["physical_byte_deltas"]["prefix_sqlite_file_bytes"], [4096])
        second[0]["start_game_clones"] += 1
        with self.assertRaisesRegex(InvalidWorkload, "start_game_clones"):
            report.compare(first, second)

    def test_real_optimized_interpreter_preserves_validation(self):
        for valid in (True, False):
            measured = rows()
            if not valid: measured[0]["prefix_save_ms"] = True
            child = subprocess.run([sys.executable, "-O", "-c",
                                    "import json,sys; import finite_claim_report as r; r.summarize(json.load(sys.stdin))"],
                                   input=json.dumps(measured), text=True, capture_output=True,
                                   cwd=Path(__file__).parent, timeout=30)
            self.assertEqual(child.returncode == 0, valid, child.stderr)
            if not valid: self.assertIn("InvalidWorkload", child.stderr)


if __name__ == "__main__":
    unittest.main()
