"""Complete coverage and save-accounting oracle for native saved exploration."""
import copy
import json
import unittest
from performance_driver import SPEC
from saved_exploration_driver import validate


class SavedExplorationReport(unittest.TestCase):
    def result(self):
        spec = json.loads(SPEC.read_text())
        profile = dict(version=1, network_events=1, apply_ms=1, draw_ms=1,
                       native_ms=1, capture_ms=0, previous_report_ms=0, turn_interval_ms=1)
        return dict(trace_version=spec["version"], seed=spec["seed"], regions=8,
                    checkpoint_interval=64, actions=77, disclosed_cells=620,
                    checkpoint_sequence=64, checkpoint_bytes=1000000, tail_records=13,
                    restart_equal=True, continued_after_restart=True,
                    samples=[dict(index=i, label=s["label"], action=s["action"],
                                  request_to_presentation_ms=1, profile=profile.copy())
                             for i,s in enumerate(spec["traversal"]*7)])

    def test_complete_exploration(self):
        validate(self.result())

    def test_incomplete_or_invalid_exploration_cannot_pass(self):
        result = self.result()
        for key, value in (("checkpoint_sequence",0), ("checkpoint_bytes",67108865),
                           ("tail_records",64), ("disclosed_cells",0),
                           ("restart_equal",False), ("continued_after_restart",False),
                           ("actions",78), ("trace_version",99)):
            with self.assertRaises(AssertionError):
                validate({**result,key:value})
        for samples in (result["samples"][:-1], result["samples"][::-1]):
            with self.assertRaises(AssertionError):
                validate({**result,"samples":samples})
        for value in (float("nan"), float("inf"), -1, True):
            broken = copy.deepcopy(result)
            broken["samples"][0]["request_to_presentation_ms"] = value
            with self.assertRaises(AssertionError):
                validate(broken)
