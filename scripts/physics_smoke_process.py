"""Explicit eight-shape physics correctness smoke; never default test discovery.

Run with scripts/test_runner.py physics_smoke_process.PhysicsCorrectnessSmoke.
The ordinary test_performance_process suite retains three samples per shape.
"""
import unittest

from performance_process_support import assert_physics_report, benchmark
from process_harness import ProcessTestCase


class PhysicsCorrectnessSmoke(ProcessTestCase):
    @classmethod
    def setUpClass(cls):
        # This standalone example is the only executable the smoke launches.
        # benchmark() builds and resolves its exact profile-specific artifact.
        # Keep ProcessTestCase's per-test directories and failure retention.
        pass

    def test_all_shapes_preserve_actions_draw_flush_and_resume(self):
        rows = benchmark(self, 'physics_bench', ['--profile', '--smoke'], 600)
        assert_physics_report(self, rows, smoke=True)


if __name__ == '__main__':
    unittest.main()
