import copy
import unittest
from item_performance_report import summarize


class ItemReports(unittest.TestCase):
    def rows(self):
        return [dict(workload_version=1, items=n, identities=k, sample=i, transfers=20,
            construction_ms=1., knowledge_ms=.1, transfer_ms=[.1]*20, save_ms=2., resume_ms=3.,
            client_apply_ms=[.1]*20, client_render_ms=[.1]*20,
            disclosed_bytes=100, saved_bytes=1000, observations=40, scenes=40,
            item_candidates=100, stack_candidates=100, knowledge_checks=100)
            for n,k in [(16,8),(1000,256)] for i in range(20)]

    def test_complete_workload_and_sample_counts(self):
        self.assertEqual(summarize(self.rows())[0]['transfer_ms']['n'], 400)

    def test_incomplete_invalid_or_nonfinite_reports_fail(self):
        for mutate in [lambda r: r.pop(), lambda r: r[0].update(workload_version=2),
                       lambda r: r[0].update(transfer_ms=[float('nan')]*20),
                       lambda r: r[0].update(stack_candidates=0)]:
            rows = copy.deepcopy(self.rows()); mutate(rows)
            with self.assertRaises(AssertionError): summarize(rows)
