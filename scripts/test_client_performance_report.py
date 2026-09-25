import copy
import unittest
from client_performance_report import validate, validate_presentation_profile


class ClientPerformanceReport(unittest.TestCase):
    def test_native_profile_rejects_unbounded_or_invalid_work(self):
        profile = dict(version=1, network_events=16, apply_ms=2, draw_ms=1,
                       native_ms=16, capture_ms=0, previous_report_ms=1, turn_interval_ms=17)
        validate_presentation_profile(profile)
        for key, value in (("network_events",17),("network_events",True),("version",2),
                           ("native_ms",float("inf")),("capture_ms",-1)):
            with self.assertRaises(ValueError):
                validate_presentation_profile({**profile, key:value})

    def rows(self):
        return [dict(version=1, cells=c, burst=b, sample=s, memory=c, chart=min(c,4096), apply_ms=1., render_ms=2.)
                for c in (64,20956) for b in (1,64) for s in range(20)]

    def test_exact_coverage_and_distributions(self):
        summary = validate(self.rows())
        self.assertEqual(len(summary), 8)
        self.assertTrue(all(s["n"] == 20 for s in summary.values()))

    def test_rejects_missing_reordered_invalid_or_unbounded_samples(self):
        rows = self.rows()
        for broken in (rows[:-1], rows[::-1], rows+rows[:1]):
            with self.assertRaises(ValueError): validate(broken)
        for key, value in (("version",2),("memory",0),("chart",4097),("apply_ms",float("nan")),
                           ("render_ms",-1),("apply_ms",True)):
            broken = copy.deepcopy(rows)
            broken[0][key] = value
            with self.assertRaises(ValueError): validate(broken)
