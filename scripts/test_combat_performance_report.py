import copy
import unittest
from combat_performance_report import summarize

class CombatReportTests(unittest.TestCase):
    def rows(self):
        return [dict(workload='combat',version=1,actors=actors,history=history,sample=sample,
                     command_ms=[1.]*64,decision_ms=[.1]*64,client_apply_ms=[.2]*32,client_draw_ms=[.3]*32,
                     phase_totals_ms={k:1. for k in ['simulation','perception','navigation','revision','checkpoint_capture']},navigation_refreshes=0,
                     save_ms=2.,resume_ms=3.,saved_bytes=100,disclosed_bytes=20,scenes=10,body_cells=10)
                for actors in [2,8] for history in [0,1000] for sample in range(3)]
    def test_complete_matrix_summarizes_measured_samples(self):
        result=summarize(self.rows())
        self.assertEqual(len(result),4)
        self.assertEqual(result[0]['command_ms']['n'],192)
        self.assertEqual(result[0]['command_ms']['p95'],1.)
    def test_incomplete_nonfinite_and_wrong_count_reports_are_rejected(self):
        rows=self.rows()
        for mutate in [lambda r:r.pop(),lambda r:r[0]['command_ms'].pop(),lambda r:r[0]['decision_ms'].__setitem__(0,float('nan')),lambda r:r[0]['phase_totals_ms'].pop('simulation')]:
            changed=copy.deepcopy(rows);mutate(changed)
            with self.assertRaises(AssertionError):summarize(changed)

if __name__=='__main__':unittest.main()
