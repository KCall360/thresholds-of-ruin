"""The benchmark workload validators: complete matrices pass, broken samples are rejected."""
import copy
import unittest
import json
from pathlib import Path
import subprocess
import sys
import tempfile

import client_performance_report
import combat_performance_report
import item_performance_report
import physics_performance_report
import place_performance_report
from client_performance_report import validate_presentation_profile
from physics_performance_report import CASES
from place_performance_report import SPEC, METRICS, validate_client
from workload_report import InvalidWorkload, percentiles


class SharedHelpers(unittest.TestCase):
    def test_percentiles_use_nearest_rank_and_reject_empty_samples(self):
        self.assertEqual(percentiles([3, 1, 2, 4]), dict(n=4, p50=2, p95=4, maximum=4))
        with self.assertRaises(InvalidWorkload):
            percentiles([])

    def test_real_optimized_interpreter_rejects_each_workload(self):
        cases = [
            ("combat_performance_report", "summarize", CombatReports().rows(), "save_ms"),
            ("item_performance_report", "summarize", ItemReports().rows(), "construction_ms"),
            ("physics_performance_report", "summarize", PhysicsReports().rows(), "resume_ms"),
            ("client_performance_report", "validate", ClientPerformanceReport().rows(), "apply_ms"),
            ("place_performance_report", "validate", PlacePerformanceReport().rows(), "command_ms"),
        ]
        for module, entry, rows, field in cases:
            with self.subTest(module=module):
                for valid in (True, False):
                    if not valid:
                        next(row for row in rows if field in row)[field] = True
                    child = subprocess.run([sys.executable, "-O", "-c",
                        f"import json,sys; from {module} import {entry}; {entry}(json.load(sys.stdin))"],
                        input=json.dumps(rows), text=True, capture_output=True,
                        cwd=Path(__file__).parent, timeout=15)
                    self.assertEqual(child.returncode == 0, valid, child.stderr)
                    if not valid:
                        self.assertRegex(child.stderr, r"(?:InvalidWorkload|ValueError):")

    def test_matrix_versions_counts_and_sample_ids_reject_boolean_aliases(self):
        for validator, rows, fields in [
                (combat_performance_report, CombatReports().rows(), ("version", "sample", "navigation_refreshes")),
                (physics_performance_report, PhysicsReports().rows(), ("version", "sample", "physics_steps")),
                (item_performance_report, ItemReports().rows(), ("workload_version", "sample"))]:
            for field in fields:
                broken = copy.deepcopy(rows)
                broken[0][field] = bool(broken[0][field])
                with self.subTest(validator=validator.__name__, field=field), self.assertRaises(InvalidWorkload):
                    validator.summarize(broken)


class ExplicitSmokeSelection(unittest.TestCase):
    def test_selected_scope_rejects_boolean_aliases_and_malformed_pairs(self):
        for validator, rows, values in [
                (combat_performance_report.summarize,
                 [row for row in CombatReports().rows() if (row["actors"], row["history"]) == (2, 0)],
                 ((2, False), (2.0, 0), [2, 0], (2,), "2,0")),
                (item_performance_report.summarize,
                 [row for row in ItemReports().rows() if (row["items"], row["identities"]) == (16, 8)],
                 ((16.0, 8), (16, 8.0), [16, 8], (16,), "16,8"))]:
            for selected in values:
                with self.subTest(validator=validator.__module__, selected=selected), self.assertRaises(ValueError):
                    validator(rows, selected_case=selected)
        for selected in (False, 0.0, "0", []):
            with self.subTest(selected=selected), self.assertRaises(ValueError):
                place_performance_report.validate(
                    [row for row in PlacePerformanceReport().rows() if row["extra_rooms"] == 0],
                    selected_rooms=selected)

    def test_smoke_requires_one_complete_known_case_and_never_relaxes_full_default(self):
        for validator, rows, selected, matches, unknown in [
            (combat_performance_report.summarize, CombatReports().rows(), {"selected_case":(2, 0)},
             lambda row: (row["actors"], row["history"]) == (2, 0), {"selected_case":(99, 0)}),
            (item_performance_report.summarize, ItemReports().rows(), {"selected_case":(16, 8)},
             lambda row: (row["items"], row["identities"]) == (16, 8), {"selected_case":(99, 0)}),
            (place_performance_report.validate, PlacePerformanceReport().rows(), {"selected_rooms":0},
             lambda row: row["extra_rooms"] == 0, {"selected_rooms":99}),
        ]:
            focused = [row for row in rows if matches(row)]
            with self.subTest(validator=validator.__module__):
                self.assertTrue(validator(focused, **selected))
                with self.assertRaises(ValueError):
                    validator(focused)
                with self.assertRaises(ValueError):
                    validator(focused[:-1], **selected)
                with self.assertRaises(ValueError):
                    validator(focused, **unknown)
                with self.assertRaises(ValueError):
                    validator(rows, **selected)


class CombatReports(unittest.TestCase):
    def rows(self):
        return [dict(workload='combat',version=1,actors=actors,history=history,sample=sample,
                     command_ms=[1.]*64,decision_ms=[.1]*64,client_apply_ms=[.2]*32,client_draw_ms=[.3]*32,
                     phase_totals_ms={k:1. for k in ['simulation','perception','navigation','revision','checkpoint_capture']},navigation_refreshes=0,
                     save_ms=2.,resume_ms=3.,saved_bytes=100,disclosed_bytes=20,scenes=10,body_cells=10)
                for actors in [2,8] for history in [0,1000] for sample in range(3)]
    def test_complete_matrix_summarizes_measured_samples(self):
        result=combat_performance_report.summarize(self.rows())
        self.assertEqual(len(result),4)
        self.assertEqual(result[0]['command_ms']['n'],192)
        self.assertEqual(result[0]['command_ms']['p95'],1.)
    def test_incomplete_nonfinite_and_wrong_count_reports_are_rejected(self):
        rows=self.rows()
        for name, mutate in [
                ("missing sample", lambda r:r.pop()),
                ("missing command timing", lambda r:r[0]['command_ms'].pop()),
                ("nonfinite decision", lambda r:r[0]['decision_ms'].__setitem__(0,float('nan'))),
                ("missing simulation phase", lambda r:r[0]['phase_totals_ms'].pop('simulation'))]:
            with self.subTest(corruption=name):
                changed=copy.deepcopy(rows);mutate(changed)
                with self.assertRaises(InvalidWorkload):combat_performance_report.summarize(changed)


class ItemReports(unittest.TestCase):
    def rows(self):
        return [dict(workload_version=1, items=n, identities=k, sample=i, transfers=20,
            construction_ms=1., knowledge_ms=.1, transfer_ms=[.1]*20, save_ms=2., resume_ms=3.,
            client_apply_ms=[.1]*20, client_render_ms=[.1]*20,
            disclosed_bytes=100, saved_bytes=1000, observations=40, scenes=40,
            item_candidates=100, stack_candidates=100, knowledge_checks=100)
            for n,k in [(16,8),(1000,256)] for i in range(20)]

    def test_complete_workload_and_sample_counts(self):
        self.assertEqual(item_performance_report.summarize(self.rows())[0]['transfer_ms']['n'], 400)

    def test_incomplete_invalid_or_nonfinite_reports_fail(self):
        for name, mutate in [("missing sample", lambda r: r.pop()),
                             ("unknown workload", lambda r: r[0].update(workload_version=2)),
                             ("nonfinite transfer", lambda r: r[0].update(transfer_ms=[float('nan')]*20)),
                             ("no stack candidates", lambda r: r[0].update(stack_candidates=0))]:
            with self.subTest(corruption=name):
                rows = copy.deepcopy(self.rows()); mutate(rows)
                with self.assertRaises(InvalidWorkload): item_performance_report.summarize(rows)


class PhysicsReports(unittest.TestCase):
    def rows(self):
        return [dict(workload='physics', version=1, actors=a, items=i, cells=c, falling=f,
                     sample=s, command_ms=[1.]*(a*8), client_apply_ms=[.1]*(a*8),
                     client_draw_ms=[.2]*(a*8), save_ms=1., resume_ms=1.,
                     saved_bytes=1000, disclosed_bytes=100, physics_steps=100 if f else 0,
                     body_cells=100, scenes=10)
                for a, i, c, f in CASES for s in range(3)]

    def test_complete_matrix_preserves_counts(self):
        report = physics_performance_report.summarize(self.rows())
        self.assertEqual(len(report), 8)
        self.assertEqual(report[0]['command_ms']['n'], 24)

    def smoke_rows(self):
        return [row for row in self.rows() if row['sample'] == 0]

    def test_explicit_smoke_keeps_every_shape_and_full_default_requires_three_samples(self):
        rows = self.smoke_rows()
        report = physics_performance_report.summarize(rows, smoke=True)
        self.assertEqual(len(report), 8)
        self.assertEqual({(r['actors'], r['items'], r['cells'], r['falling']) for r in report}, CASES)
        for row in report:
            self.assertEqual(row['samples'], 1)
            self.assertEqual(row['command_ms']['n'], row['actors'] * 8)
            self.assertEqual(row['client_apply_ms']['n'], row['actors'] * 8)
            self.assertEqual(row['client_draw_ms']['n'], row['actors'] * 8)
            self.assertEqual(row['save_ms']['n'], 1)
            self.assertEqual(row['resume_ms']['n'], 1)
        with self.assertRaises(InvalidWorkload):
            physics_performance_report.summarize(rows)
        with self.assertRaises(InvalidWorkload):
            physics_performance_report.summarize(self.rows(), smoke=True)

    def test_smoke_selection_requires_an_explicit_boolean(self):
        for value in (None, 0, 1, 'true', [], {}):
            with self.subTest(value=value), self.assertRaises(InvalidWorkload):
                physics_performance_report.summarize(self.smoke_rows(), smoke=value)

    def test_smoke_rejects_missing_duplicate_extra_and_misidentified_shapes(self):
        rows = self.smoke_rows()
        for broken in (rows[:-1], rows + [copy.deepcopy(rows[0])],
                       rows + [row for row in self.rows() if row['sample'] == 1]):
            with self.subTest(rows=len(broken)), self.assertRaises(InvalidWorkload):
                physics_performance_report.summarize(broken, smoke=True)
        for field, value in (('sample', 1), ('sample', False), ('actors', 99),
                             ('actors', True), ('falling', 0), ('version', 2)):
            broken = self.smoke_rows()
            broken[0][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(InvalidWorkload):
                physics_performance_report.summarize(broken, smoke=True)

    def test_smoke_preserves_each_shapes_action_draw_activity_and_duration_checks(self):
        mutations = [
            ('missing command', lambda r: r['command_ms'].pop()),
            ('no client application', lambda r: r.update(client_apply_ms=[])),
            ('missing draw', lambda r: r['client_draw_ms'].pop()),
            ('false activity', lambda r: r.update(physics_steps=0 if r['falling'] else 1)),
            ('invalid save', lambda r: r.update(save_ms=float('nan'))),
            ('invalid resume', lambda r: r.update(resume_ms=-1)),
            ('no saved bytes', lambda r: r.update(saved_bytes=0)),
            ('no disclosed bytes', lambda r: r.update(disclosed_bytes=0)),
            ('no body cells', lambda r: r.update(body_cells=0)),
        ]
        for index in range(8):
            for name, mutate in mutations:
                rows = self.smoke_rows()
                mutate(rows[index])
                with self.subTest(shape=index, corruption=name), self.assertRaises(InvalidWorkload):
                    physics_performance_report.summarize(rows, smoke=True)

    def test_smoke_cli_is_explicit_and_cannot_relax_the_default(self):
        script = Path(__file__).with_name('physics_performance_report.py')
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'physics.jsonl'
            for rows, flags, valid in ((self.rows(), [], True),
                                      (self.smoke_rows(), [], False),
                                      (self.smoke_rows(), ['--smoke'], True),
                                      (self.rows(), ['--smoke'], False),
                                      (self.rows(), ['--unknown'], False)):
                source.write_text(''.join(json.dumps(row) + '\n' for row in rows), encoding='utf-8')
                child = subprocess.run([sys.executable, '-O', str(script), str(source), *flags],
                                       text=True, capture_output=True, timeout=15)
                with self.subTest(rows=len(rows), flags=flags):
                    self.assertEqual(child.returncode == 0, valid, child.stderr)
                    if valid:
                        self.assertEqual(len(json.loads(child.stdout)), 8)

    def test_missing_samples_nonfinite_times_and_false_activity_fail(self):
        for name, mutate in [("missing sample", lambda r: r.pop()),
                             ("wrong sample identity", lambda r: r[0].update(sample=99)),
                             ("nonfinite save", lambda r: r[0].update(save_ms=float('nan'))),
                             ("wrong physics activity", lambda r: r[0].update(physics_steps=0 if r[0]['falling'] else 1))]:
            with self.subTest(corruption=name):
                rows = self.rows()
                mutate(rows)
                with self.assertRaises(InvalidWorkload):
                    physics_performance_report.summarize(rows)

    def test_only_changed_observer_states_are_applied_and_drawn(self):
        rows = self.rows()
        for row in rows:
            if row['actors'] == 8:
                row['client_apply_ms'] = [.1] * 16
                row['client_draw_ms'] = [.2] * 16
        report = physics_performance_report.summarize(rows)
        self.assertTrue(all(r['client_apply_ms']['n'] == 48
                            for r in report if r['actors'] == 8))
        rows[0]['client_draw_ms'].pop()
        with self.assertRaises(InvalidWorkload):
            physics_performance_report.summarize(rows)


    def test_optional_physics_profiles_validate_counts_and_phase_durations(self):
        rows = self.rows()
        profile = {key: {"secs": 0, "nanos": 100} for key in
                   ("authoritative_total", "perception", "navigation_refresh", "simulation_transition")}
        profile.update(actors_observed=0, perception_calls=0, scene_calls=0)
        for row in rows:
            row["profiles"] = [copy.deepcopy(profile) for _ in row["command_ms"]]
        physics_performance_report.summarize(rows)
        for mutate in (lambda rs: rs[0]["profiles"].pop(),
                       lambda rs: rs[0]["profiles"][0]["perception"].update(nanos=-1),
                       lambda rs: rs[0]["profiles"][0].update(scene_calls=-1)):
            broken = copy.deepcopy(rows)
            mutate(broken)
            with self.assertRaises(InvalidWorkload):
                physics_performance_report.summarize(broken)

    def test_optional_checkpoint_diagnostics_reject_invalid_size_and_time(self):
        rows = self.rows()
        for row in rows:
            row["checkpoint_profile"] = dict(bytes=123, ms=.2, status={})
        physics_performance_report.summarize(rows)
        for changes in (dict(bytes=0), dict(bytes=True), dict(ms=float('nan')),
                        dict(ms=-1), dict(status=[])):
            broken = copy.deepcopy(rows)
            broken[0]["checkpoint_profile"].update(changes)
            with self.assertRaises(InvalidWorkload):
                physics_performance_report.summarize(broken)

    def test_optional_commit_timing_is_a_bounded_part_of_the_successful_batch(self):
        rows = self.rows()
        for row in rows:
            row["checkpoint_profile"] = dict(bytes=123, ms=.2,
                status=dict(last_commit_ms=10, last_batch_ms=20))
        physics_performance_report.summarize(rows)
        for status in (dict(last_commit_ms=-1, last_batch_ms=20),
                       dict(last_commit_ms=True, last_batch_ms=20),
                       dict(last_commit_ms=21, last_batch_ms=20),
                       dict(last_commit_ms=10), dict(last_commit_ms=10, last_batch_ms=float('nan'))):
            broken = copy.deepcopy(rows)
            broken[0]["checkpoint_profile"]["status"] = status
            with self.assertRaises(InvalidWorkload):
                physics_performance_report.summarize(broken)


class SceneProjectionCounts(unittest.TestCase):
    def workloads(self):
        return [(combat_performance_report, CombatReports().rows()),
                (item_performance_report, ItemReports().rows()),
                (physics_performance_report, PhysicsReports().rows())]

    def test_cached_projections_preserve_zero_counts_and_measured_disclosure(self):
        for validator, rows in self.workloads():
            with self.subTest(workload=validator.__name__):
                for row in rows:
                    row['scenes'] = 0
                summary = validator.summarize(rows)
                self.assertTrue(all(row['scenes'] == dict(min=0, max=0) for row in summary))
                self.assertTrue(all(row['disclosed_bytes']['min'] > 0 for row in summary))

    def test_scene_counts_reject_booleans_negative_and_noninteger_values(self):
        for validator, rows in self.workloads():
            for value in (True, False, -1, 0.0, 1.5, '0', None, float('inf')):
                with self.subTest(workload=validator.__name__, value=value):
                    broken = copy.deepcopy(rows)
                    broken[0]['scenes'] = value
                    with self.assertRaises(InvalidWorkload):
                        validator.summarize(broken)

    def test_cached_projections_still_require_actual_disclosure(self):
        for validator, rows in self.workloads():
            fields = ['disclosed_bytes']
            if validator is item_performance_report:
                fields.append('observations')
            for field in fields:
                with self.subTest(workload=validator.__name__, field=field):
                    broken = copy.deepcopy(rows)
                    for row in broken:
                        row['scenes'] = 0
                    broken[0][field] = 0
                    with self.assertRaisesRegex(InvalidWorkload, f'{field} must be a positive count'):
                        validator.summarize(broken)


class ClientPerformanceReport(unittest.TestCase):
    def test_native_profile_rejects_unbounded_or_invalid_work(self):
        profile = dict(version=1, network_events=16, apply_ms=2, draw_ms=1,
                       native_ms=16, capture_ms=0, previous_report_ms=1, turn_interval_ms=17)
        validate_presentation_profile(profile)
        validate_presentation_profile({**profile, "previous_report_encode_ms":.2, "previous_report_write_ms":.8})
        for extra in ({"previous_report_encode_ms":.2},
                      {"previous_report_encode_ms":.2, "previous_report_write_ms":float("nan")}):
            with self.assertRaises(ValueError):
                validate_presentation_profile({**profile, **extra})
        for key, value in (("network_events",17),("network_events",True),("version",2),
                           ("native_ms",float("inf")),("capture_ms",-1)):
            with self.assertRaises(ValueError):
                validate_presentation_profile({**profile, key:value})

    def rows(self):
        return [dict(version=1, cells=c, burst=b, sample=s, memory=c, chart=min(c,4096), apply_ms=1., render_ms=2.)
                for c in (64,20956) for b in (1,64) for s in range(20)]

    def test_exact_coverage_and_distributions(self):
        summary = client_performance_report.validate(self.rows())
        self.assertEqual(len(summary), 8)
        self.assertTrue(all(s["n"] == 20 for s in summary.values()))

    def test_narration_workload_requires_explicit_version_and_semantic_coverage(self):
        rows = self.rows()
        for row in rows:
            row.update(version=2, narration_count=1 if row['burst'] == 1 and row['sample'] == 0 else 2)
        self.assertEqual(len(client_performance_report.validate(rows, version=2)), 8)
        with self.assertRaises(ValueError): client_performance_report.validate(rows)
        rows[0]['narration_count'] = 0
        with self.assertRaises(ValueError): client_performance_report.validate(rows, version=2)

    def test_rejects_missing_reordered_invalid_or_unbounded_samples(self):
        rows = self.rows()
        for broken in (rows[:-1], rows[::-1], rows+rows[:1]):
            with self.assertRaises(ValueError): client_performance_report.validate(broken)
        for key, value in (("version",2),("memory",0),("chart",4097),("apply_ms",float("nan")),
                           ("render_ms",-1),("apply_ms",True)):
            broken = copy.deepcopy(rows)
            broken[0][key] = value
            with self.assertRaises(ValueError): client_performance_report.validate(broken)


class PlacePerformanceReport(unittest.TestCase):
    def rows(self):
        rows = []
        for rooms in SPEC["extra_rooms"]:
            count = 2 + rooms * len(SPEC["hints"])
            rows.extend(dict(version=1, kind="discovery", extra_rooms=rooms, room=i,
                             places=2+(i+1)*len(SPEC["hints"])) for i in range(rooms))
            rows.extend(dict(version=1, kind="sample", extra_rooms=rooms, places=count,
                             sample=i, label="rename" if i % 2 == 0 else "wait", records=1,
                             navigation_refreshes=0, wire_bytes=100, **{m: 1.0 for m in METRICS})
                        for i in range(SPEC["samples"]))
            rows.append(dict(version=1, kind="recovery", extra_rooms=rooms, places=count, exact=True, checkpoint_bytes=100))
        return rows

    def test_complete_trace_and_missing_duplicate_or_invalid_samples(self):
        rows = self.rows()
        self.assertEqual(len(place_performance_report.validate(rows)), 6)
        for broken in (rows[:-1], rows + [rows[-1]], rows[1:]):
            with self.assertRaises(ValueError):
                place_performance_report.validate(broken)
        for field, value in (("places", 999), ("records", 2), ("command_ms", float("nan")), ("navigation_refreshes", 1)):
            bad = copy.deepcopy(rows)
            bad[0][field] = value
            with self.assertRaises(ValueError):
                place_performance_report.validate(bad)

    def test_place_measurements_reject_booleans_missing_and_invalid_types(self):
        for field in METRICS:
            for value in (True, float("nan"), float("inf"), -1, "1", None):
                rows = self.rows()
                sample = next(row for row in rows if row["kind"] == "sample")
                sample[field] = value
                with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                    place_performance_report.validate(rows)
            rows = self.rows()
            next(row for row in rows if row["kind"] == "sample").pop(field)
            with self.subTest(field=field, missing=True), self.assertRaises(ValueError):
                place_performance_report.validate(rows)

    def test_client_trace_rejects_missing_and_nonfinite_measurements(self):
        rows = [dict(sample=i, label="rename" if i%2==0 else "wait", places=2,
                     request_to_ack_ms=1., request_to_ready_ms=2., request_to_presentation_ms=3.)
                for i in range(SPEC["samples"])]
        result = dict(version=1, extra_rooms=0, final_places=[{}, {}], samples=rows)
        validate_client(result)
        for value in (float("nan"), float("inf"), -1, True, None, "1"):
            rows[0]["request_to_ack_ms"] = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                validate_client(result)
        result["samples"] = rows[1:]
        with self.assertRaises(ValueError):
            validate_client(result)


class ObservationOwnershipReport(unittest.TestCase):
    def rows(self):
        return [dict(diagnostic='observation_ownership', version=1, cells=c, readers=n,
                     method=m, sample=i, state_bytes=c*128, retained_objects=n if m == 'owned_clone' else 1,
                     distinct_state_serialized_bytes=c*128*(n if m == 'owned_clone' else 1),
                     clone_ms=.2 if m == 'owned_clone' else .001, verified=True)
                for c in (64, 4096, 20956) for n in (1, 8, 32) for i in range(100)
                for m in (('owned_clone', 'shared_handle') if i % 2 == 0 else ('shared_handle', 'owned_clone'))]

    def test_complete_matrix_reports_cloning_and_distinct_content(self):
        result = client_performance_report.validate_ownership(self.rows())
        self.assertEqual(len(result), 18)
        owned = result['cells-4096-readers-32-owned_clone']
        shared = result['cells-4096-readers-32-shared_handle']
        self.assertEqual(owned['n'], 100)
        self.assertEqual(shared['retained_objects'], 1)
        self.assertEqual(owned['distinct_state_serialized_bytes'], shared['distinct_state_serialized_bytes'] * 32)
        self.assertEqual(shared['p95_ms'], .001)

    def test_missing_duplicate_unverified_and_unstable_payloads_are_rejected(self):
        for mutate in (lambda r: r.pop(), lambda r: r.append(r[0]),
                       lambda r: r.reverse(), lambda r: r[0].update(verified=False),
                       lambda r: r[1].update(state_bytes=r[1]['state_bytes'] + 1),
                       lambda r: r[201].update(retained_objects=7),
                       lambda r: r[0].update(distinct_state_serialized_bytes=0)):
            rows = self.rows(); mutate(rows)
            with self.assertRaises(ValueError):
                client_performance_report.validate_ownership(rows)

    def test_invalid_numeric_types_versions_and_nonfinite_times_are_rejected(self):
        for field, value in (('version', True), ('version', 2), ('readers', True),
                             ('sample', False), ('clone_ms', float('nan')),
                             ('clone_ms', float('inf')), ('clone_ms', -1),
                             ('clone_ms', True), ('state_bytes', False),
                             ('diagnostic', 'other'), ('unexpected', 1)):
            rows = self.rows(); rows[0][field] = value
            with self.assertRaises(ValueError):
                client_performance_report.validate_ownership(rows)


if __name__ == "__main__":
    unittest.main()
