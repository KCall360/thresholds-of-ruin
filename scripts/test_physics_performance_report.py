import unittest
from physics_performance_report import CASES, summarize


class PhysicsReports(unittest.TestCase):
    def rows(self):
        return [dict(workload='physics', version=1, actors=a, items=i, cells=c, falling=f,
                     sample=s, command_ms=[1.]*(a*8), client_apply_ms=[.1]*(a*8),
                     client_draw_ms=[.2]*(a*8), save_ms=1., resume_ms=1.,
                     saved_bytes=1000, disclosed_bytes=100, physics_steps=100 if f else 0,
                     body_cells=100, scenes=10)
                for a, i, c, f in CASES for s in range(3)]

    def test_complete_matrix_preserves_counts(self):
        report = summarize(self.rows())
        self.assertEqual(len(report), 8)
        self.assertEqual(report[0]['command_ms']['n'], 24)

    def test_missing_samples_nonfinite_times_and_false_activity_fail(self):
        for mutate in [lambda r: r.pop(), lambda r: r[0].update(sample=99),
                       lambda r: r[0].update(save_ms=float('nan')),
                       lambda r: r[0].update(physics_steps=0 if r[0]['falling'] else 1)]:
            rows = self.rows()
            mutate(rows)
            with self.assertRaises(AssertionError):
                summarize(rows)

    def test_only_changed_observer_states_are_applied_and_drawn(self):
        rows = self.rows()
        for row in rows:
            if row['actors'] == 8:
                row['client_apply_ms'] = [.1] * 16
                row['client_draw_ms'] = [.2] * 16
        report = summarize(rows)
        self.assertTrue(all(r['client_apply_ms']['n'] == 48
                            for r in report if r['actors'] == 8))
        rows[0]['client_draw_ms'].pop()
        with self.assertRaises(AssertionError):
            summarize(rows)
