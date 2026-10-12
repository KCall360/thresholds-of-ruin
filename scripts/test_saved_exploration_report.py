"""Complete coverage and save-accounting oracle for native saved exploration."""
import copy
import json
import unittest
from workload_report import InvalidWorkload
import subprocess
import sys
from pathlib import Path
from performance_driver import SPEC
from saved_exploration_driver import validate


class SavedExplorationReport(unittest.TestCase):
    def result(self, regions=8):
        spec = json.loads(SPEC.read_text())
        profile = dict(version=1, network_events=1, apply_ms=1, draw_ms=1,
                       native_ms=1, capture_ms=0, previous_report_ms=0, turn_interval_ms=1)
        return dict(trace_version=spec["version"], seed=spec["seed"], regions=regions,
                    checkpoint_interval=64, actions=11 * (regions - 1), disclosed_cells={8:1260, 256:41436}[regions],
                    checkpoint_sequence=(22 * (regions - 1) // 64) * 64, checkpoint_bytes=1000000,
                    tail_records=22 * (regions - 1) % 64, journal_records=22 * (regions - 1),
                    restart_to_frame_ms=1., restart_input_ms=1.,
                    restart_equal=True, continued_after_restart=True,
                    samples=[dict(index=i, label=s["label"], action=s["action"],
                                  request_to_presentation_ms=1, profile=profile.copy())
                             for i,s in enumerate(spec["traversal"]*(regions-1))])

    def test_complete_exploration(self):
        for regions in (8, 256):
            with self.subTest(regions=regions):
                validate(self.result(regions))

    def test_incomplete_or_invalid_exploration_cannot_pass(self):
        result = self.result()
        for key, value in (("checkpoint_sequence",0), ("checkpoint_bytes",16 * 1024 * 1024),
                           ("journal_records",77), ("journal_records",155),
                           ("tail_records",64), ("disclosed_cells",0),
                           ("disclosed_cells",1243), ("disclosed_cells",1259), ("disclosed_cells",1261),
                           ("restart_equal",False), ("continued_after_restart",False),
                           ("actions",78), ("trace_version",99)):
            with self.assertRaises(InvalidWorkload):
                validate({**result,key:value})
        for samples in (result["samples"][:-1], result["samples"][::-1]):
            with self.assertRaises(InvalidWorkload):
                validate({**result,"samples":samples})
        for value in (float("nan"), float("inf"), -1, True):
            broken = copy.deepcopy(result)
            broken["samples"][0]["request_to_presentation_ms"] = value
            with self.assertRaises(InvalidWorkload):
                validate(broken)


    def test_checkpoint_size_has_an_independent_strict_sixteen_mib_boundary(self):
        for regions in (8, 256):
            result = self.result(regions)
            validate({**result, "checkpoint_bytes": 16 * 1024 * 1024 - 1})
            for value in (16 * 1024 * 1024, 16 * 1024 * 1024 + 1, 0, -1, True, 1.5, "1", None):
                with self.subTest(regions=regions, value=value), self.assertRaises(InvalidWorkload):
                    validate({**result, "checkpoint_bytes": value})

    def test_actual_optimized_interpreter_rejects_invalid_saved_evidence(self):
        result = self.result()
        for valid in (True, False):
            if not valid:
                result["disclosed_cells"] -= 1
            child = subprocess.run([sys.executable, "-O", "-c",
                "import json,sys; from saved_exploration_driver import validate; validate(json.load(sys.stdin))"],
                input=json.dumps(result), text=True, capture_output=True, cwd=Path(__file__).parent, timeout=15)
            self.assertEqual(child.returncode == 0, valid, child.stderr)
            if not valid:
                self.assertIn("InvalidWorkload: Incomplete disclosed", child.stderr)
