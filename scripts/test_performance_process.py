"""The benchmark trace driven through real headless clients and ASCII frames."""
import json
import os
from pathlib import Path
import unittest
import test_text_process as support
from performance_driver import run_demo


class PerformanceProcesses(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        support.TextProcesses.setUpClass.__func__(cls)

    def test_mixed_trace_is_presented_and_verified_for_complete_cycles(self):
        directory = support.ProcessTestDirectory()
        self.addCleanup(directory.cleanup)
        output = Path(directory.name) / "demo"
        regions = int(os.environ.get("TOR_PERFORMANCE_REGIONS", "8"))
        cycles = int(os.environ.get("TOR_PERFORMANCE_CYCLES", "1"))
        result = run_demo(self.bin, output, regions=regions, actors=1, cycles=cycles, pace=0, stay_open=False)
        self.assertEqual(result["cycles"], cycles)
        self.assertEqual(result["spectator_role"], "spectator")
        self.assertGreater(result["presented_revision"], result["initial_revision"])
        self.assertGreater(result["client_memory_end"], result["client_memory_start"])
        labels = {s["label"] for s in result["samples"]}
        self.assertTrue({"cross_region_boundary", "cross_boundary_with_los_change",
            "open_or_close_door", "change_elevation", "move_near_obstacle"} <= labels)
        self.assertTrue(all(s["request_to_ack_ms"] >= 0 for s in result["samples"] if s["expected"] != "blocked"))
        self.assertTrue(all(s["request_to_presentation_ms"] >= 0 for s in result["samples"] if s["expected"] != "blocked"))
        self.assertTrue((output / "game.json").exists())
        self.assertEqual(json.loads((output / "result.json").read_text())["cycles"], cycles)

    def test_eight_real_clients_follow_scheduled_turns_and_visibility_changes(self):
        directory = support.ProcessTestDirectory()
        self.addCleanup(directory.cleanup)
        result = run_demo(self.bin, Path(directory.name)/"multi", regions=8, actors=8, cycles=1, pace=0)
        self.assertEqual({s["actor"] for s in result["samples"]}, set(range(1,9)))
        self.assertIn("multi_actor_visibility_change", {s["label"] for s in result["samples"]})


if __name__ == "__main__":
    unittest.main()
