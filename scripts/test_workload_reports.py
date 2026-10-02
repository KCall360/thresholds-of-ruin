"""The benchmark workload validators: complete matrices pass, broken samples are rejected."""
import copy
import unittest

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

    def test_failures_are_value_errors_not_assertions(self):
        # `python -O` removes assert statements; validation must still run.
        self.assertTrue(issubclass(InvalidWorkload, ValueError))
        self.assertFalse(issubclass(InvalidWorkload, AssertionError))


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
        for mutate in [lambda r:r.pop(),lambda r:r[0]['command_ms'].pop(),lambda r:r[0]['decision_ms'].__setitem__(0,float('nan')),lambda r:r[0]['phase_totals_ms'].pop('simulation')]:
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
        for mutate in [lambda r: r.pop(), lambda r: r[0].update(workload_version=2),
                       lambda r: r[0].update(transfer_ms=[float('nan')]*20),
                       lambda r: r[0].update(stack_candidates=0)]:
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

    def test_missing_samples_nonfinite_times_and_false_activity_fail(self):
        for mutate in [lambda r: r.pop(), lambda r: r[0].update(sample=99),
                       lambda r: r[0].update(save_ms=float('nan')),
                       lambda r: r[0].update(physics_steps=0 if r[0]['falling'] else 1)]:
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

    def test_client_trace_rejects_missing_and_nonfinite_measurements(self):
        rows = [dict(sample=i, label="rename" if i%2==0 else "wait", places=2,
                     request_to_ack_ms=1., request_to_ready_ms=2., request_to_presentation_ms=3.)
                for i in range(SPEC["samples"])]
        result = dict(version=1, extra_rooms=0, final_places=[{}, {}], samples=rows)
        validate_client(result)
        rows[0]["request_to_ack_ms"] = float("nan")
        with self.assertRaises(ValueError):
            validate_client(result)
        result["samples"] = rows[1:]
        with self.assertRaises(ValueError):
            validate_client(result)


if __name__ == "__main__":
    unittest.main()
