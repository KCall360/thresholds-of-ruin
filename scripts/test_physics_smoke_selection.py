"""Explicit physics smoke selection preserves the exhaustive process contract."""
import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import perf_compare
import performance_process_support as support
import physics_smoke_process
from physics_performance_report import CASES
from process_harness import ROOT, SUFFIX


class PhysicsSmokeSelection(unittest.TestCase):
    def rows(self, *, smoke):
        return [dict(workload='physics', version=1, actors=a, items=i, cells=c, falling=f,
                     sample=s, command_ms=[1.]*(a*8), client_apply_ms=[.1]*(a*8),
                     client_draw_ms=[.2]*(a*8), save_ms=1., resume_ms=1.,
                     saved_bytes=1000, disclosed_bytes=100, physics_steps=100 if f else 0,
                     body_cells=100, scenes=10,
                     checkpoint_profile=dict(bytes=123, ms=.2, status=dict(
                         accepted_sequence=10, durable_sequence=10, pending_bytes=0,
                         saving=False, error=None, checkpoints=1,
                         last_commit_ms=1, last_batch_ms=2)))
                for a, i, c, f in CASES for s in range(1 if smoke else 3)]

    def test_full_and_smoke_preserve_every_completed_durable_status_assertion(self):
        mutations = [dict(durable_sequence=9), dict(pending_bytes=1), dict(saving=True),
                     dict(error='disk failure'), dict(checkpoints=0), dict(last_commit_ms=-1),
                     dict(last_commit_ms=3)]
        for smoke in (False, True):
            rows = self.rows(smoke=smoke)
            support.assert_physics_report(self, rows, smoke=smoke)
            for index in range(len(rows)):
                for mutation in mutations:
                    broken = copy.deepcopy(rows)
                    broken[index]['checkpoint_profile']['status'].update(mutation)
                    with self.subTest(smoke=smoke, row=index, mutation=mutation), self.assertRaises((AssertionError, ValueError)):
                        support.assert_physics_report(self, broken, smoke=smoke)

    def test_discovery_keeps_exhaustive_test_and_smoke_requires_its_exact_module(self):
        def ids(suite):
            for test in suite:
                if isinstance(test, unittest.TestSuite):
                    yield from ids(test)
                else:
                    yield test.id()

        loader = unittest.TestLoader()
        discovered = set(ids(loader.discover(str(ROOT / 'scripts'), pattern='test_*process.py')))
        full_id = 'test_performance_process.PerformanceProcesses.test_physics_benchmark_reports_completed_durable_commit_timings'
        smoke_id = 'physics_smoke_process.PhysicsCorrectnessSmoke.test_all_shapes_preserve_actions_draw_flush_and_resume'
        self.assertIn(full_id, discovered)
        self.assertNotIn(smoke_id, discovered)
        self.assertEqual(list(ids(loader.loadTestsFromName(smoke_id))), [smoke_id])
        self.assertFalse(loader.errors, loader.errors)

    def test_explicit_and_exhaustive_methods_choose_different_exact_sample_matrices(self):
        loader = unittest.TestLoader()
        full = next(iter(loader.loadTestsFromName(
            'test_performance_process.PerformanceProcesses.test_physics_benchmark_reports_completed_durable_commit_timings')))
        with patch.object(full, 'benchmark', return_value=self.rows(smoke=False)) as run_full:
            full.test_physics_benchmark_reports_completed_durable_commit_timings()
        run_full.assert_called_once_with('physics_bench', ['--profile'], 600)
        smoke = physics_smoke_process.PhysicsCorrectnessSmoke(
            'test_all_shapes_preserve_actions_draw_flush_and_resume')
        with patch.object(physics_smoke_process, 'benchmark', return_value=self.rows(smoke=True)) as run_smoke:
            smoke.test_all_shapes_preserve_actions_draw_flush_and_resume()
        run_smoke.assert_called_once_with(smoke, 'physics_bench', ['--profile', '--smoke'], 600)

    def test_performance_comparison_does_not_select_or_validate_smoke(self):
        unit = perf_compare.parse_cases(['physics'], cycles=5)[0]
        self.assertEqual(unit.args, ())
        self.assertEqual(unit.validator_args(['--smoke']), [])
        self.assertNotIn('--smoke', unit.spec.validator_flags)


class BenchmarkProcessSupport(unittest.TestCase):
    def test_launcher_preserves_profile_paths_arguments_timeouts_and_json_output(self):
        for profile in ('debug', 'release'):
            with self.subTest(profile=profile), tempfile.TemporaryDirectory() as directory:
                case = SimpleNamespace(directory=Path(directory), bin=Path(directory) / profile)
                calls = []

                def run(command, **kwargs):
                    calls.append((command, kwargs))
                    if len(calls) == 2:
                        kwargs['stdout'].write(json.dumps({'sample': 0}) + '\n')

                with patch.dict(os.environ, {'TOR_TEST_PROFILE': profile}), patch.object(support.subprocess, 'run', side_effect=run):
                    rows = support.benchmark(case, 'physics_bench', ['--profile', '--smoke'], 123)
                self.assertEqual(rows, [{'sample': 0}])
                build = ['cargo', 'build', '-p', 'tor-server', '--example', 'physics_bench', '--locked']
                if profile == 'release':
                    build.append('--release')
                self.assertEqual(calls[0][0], build)
                self.assertEqual(calls[1][0], [case.bin / 'examples' / ('physics_bench' + SUFFIX), '--profile', '--smoke'])
                for index, (_, kwargs) in enumerate(calls):
                    self.assertEqual(kwargs['cwd'], ROOT)
                    self.assertEqual(kwargs['timeout'], 600 if index == 0 else 123)
                    self.assertTrue(kwargs['check'])
                self.assertIs(calls[0][1]['stdout'], calls[0][1]['stderr'])
                self.assertEqual(Path(calls[0][1]['stdout'].name), case.directory / 'physics_bench-build.log')
                self.assertEqual(Path(calls[1][1]['stdout'].name), case.directory / 'physics_bench.jsonl')
                self.assertEqual(Path(calls[1][1]['stderr'].name), case.directory / 'physics_bench-stderr.log')

    def test_launcher_propagates_build_run_and_report_failures(self):
        for failure_at in (1, 2):
            with self.subTest(failure_at=failure_at), tempfile.TemporaryDirectory() as directory:
                case = SimpleNamespace(directory=Path(directory), bin=Path(directory))
                outcomes = [None] * (failure_at - 1) + [subprocess.CalledProcessError(3, ['cargo'])]
                with patch.object(support.subprocess, 'run', side_effect=outcomes), self.assertRaises(subprocess.CalledProcessError):
                    support.benchmark(case, 'physics_bench', ['--smoke'], 180)
        with tempfile.TemporaryDirectory() as directory:
            case = SimpleNamespace(directory=Path(directory), bin=Path(directory))

            def malformed(command, **kwargs):
                if command[0] != 'cargo':
                    kwargs['stdout'].write('not json\n')

            with patch.object(support.subprocess, 'run', side_effect=malformed), self.assertRaises(json.JSONDecodeError):
                support.benchmark(case, 'physics_bench', ['--smoke'], 180)


if __name__ == '__main__':
    unittest.main()
