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

import parallel_tests
import perf_compare
import performance_process_support as support
import physics_smoke_process
from physics_performance_report import CASES
from process_harness import ROOT, SUFFIX, ProcessTestCase


class PhysicsSmokeSelection(unittest.TestCase):
    def test_parser_coverage_uses_existing_server_test_target(self):
        root = Path(__file__).resolve().parents[1]
        workflow = (root / '.github/workflows/ci.yml').read_text()
        self.assertNotIn('cargo test -p tor-server --example physics_bench', workflow)
        self.assertIn('python scripts/verify.py full --ci-profile ${{ matrix.profile }} --ci-partition ${{ matrix.partition }}', workflow)
        support = root / 'crates/server/example_support/physics_options.rs'
        library = (root / 'crates/server/src/lib.rs').read_text()
        example = (root / 'crates/server/examples/physics_bench.rs').read_text()
        self.assertIn('#[cfg(test)]\n#[path = "../example_support/physics_options.rs"]\nmod physics_options;', library)
        self.assertIn('#[path = "../example_support/physics_options.rs"]\nmod physics_options;', example)
        self.assertEqual(support.read_text().count('#[test]'), 3)

    def test_existing_runner_loads_and_schedules_supplemental_process_without_native_lane(self):
        loader = unittest.TestLoader()
        tests = list(parallel_tests.flatten(loader.loadTestsFromName('physics_smoke_process.PhysicsCorrectnessSmoke.test_all_shapes_preserve_actions_draw_flush_and_resume')))
        self.assertFalse(loader.errors, loader.errors)
        self.assertEqual([test.id() for test in tests], ['physics_smoke_process.PhysicsCorrectnessSmoke.test_all_shapes_preserve_actions_draw_flush_and_resume'])
        self.assertIsInstance(tests[0], ProcessTestCase)
        self.assertTrue(parallel_tests.has_own_fixture(type(tests[0])))
        headless, native = parallel_tests.plan(tests, {}, workers=2)
        self.assertEqual(native, [])
        self.assertEqual([name for batch in headless for name in batch.ids], ['physics_smoke_process.PhysicsCorrectnessSmoke.test_all_shapes_preserve_actions_draw_flush_and_resume'])

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
    def artifact(self, path, *, name='physics_bench', test=False):
        return dict(reason='compiler-artifact', target=dict(name=name, kind=['example']),
                    profile=dict(test=test), executable=str(path))

    def test_launcher_preserves_profile_paths_arguments_timeouts_and_json_output(self):
        for profile in ('debug', 'release'):
            with self.subTest(profile=profile), tempfile.TemporaryDirectory() as directory:
                case = SimpleNamespace(directory=Path(directory), bin=Path(directory) / profile)
                executable = Path(directory) / 'custom-target' / profile / 'examples' / ('physics_bench' + SUFFIX)
                calls = []

                def run(command, **kwargs):
                    calls.append((command, kwargs))
                    if len(calls) == 1:
                        kwargs['stdout'].write(json.dumps(self.artifact(executable)) + '\n')
                    else:
                        kwargs['stdout'].write(json.dumps({'sample': 0}) + '\n')

                with patch.dict(os.environ, {'TOR_TEST_PROFILE': profile}), patch.object(support.subprocess, 'run', side_effect=run):
                    rows = support.benchmark(case, 'physics_bench', ['--profile', '--smoke'], 123)
                self.assertEqual(rows, [{'sample': 0}])
                build = ['cargo', 'build', '-p', 'tor-server', '--example', 'physics_bench', '--locked', '--message-format=json']
                if profile == 'release':
                    build.append('--release')
                self.assertEqual(calls[0][0], build)
                self.assertEqual(calls[1][0], [executable, '--profile', '--smoke'])
                for index, (_, kwargs) in enumerate(calls):
                    self.assertEqual(kwargs['cwd'], ROOT)
                    self.assertEqual(kwargs['timeout'], 600 if index == 0 else 123)
                    self.assertTrue(kwargs['check'])
                self.assertIs(calls[0][1]['stdout'], calls[0][1]['stderr'])
                self.assertEqual(Path(calls[0][1]['stdout'].name), case.directory / 'physics_bench-build.log')
                self.assertEqual(Path(calls[1][1]['stdout'].name), case.directory / 'physics_bench.jsonl')
                self.assertEqual(Path(calls[1][1]['stderr'].name), case.directory / 'physics_bench-stderr.log')

    def test_streaming_caller_uses_emitted_example_outside_application_bundle(self):
        class ReportBoundary(Exception):
            pass

        for profile in ('debug', 'release'):
            with self.subTest(profile=profile), tempfile.TemporaryDirectory() as directory:
                root = Path(directory).resolve()
                loader = unittest.TestLoader()
                case = next(iter(loader.loadTestsFromName(
                    'test_performance_process.PerformanceProcesses.test_streaming_benchmark_emits_valid_nested_acquisition_profiles')))
                self.assertFalse(loader.errors, loader.errors)
                case.directory = root
                case.bin = root / 'ordinary-app-bundle'
                case.bin.mkdir()
                executable = root / 'cargo-target' / profile / 'examples' / ('latency_bench' + SUFFIX)
                executable.parent.mkdir(parents=True)
                executable.write_bytes(b'independent emitted-example fixture')
                self.assertFalse((case.bin / 'examples').exists())
                args = ['--case', 'stream-r16-durable', '--cycles', '1', '--checkpoint-interval', '64']
                rows = [{'kind': 'sample', 'actor': 7}]
                calls = []

                def run(command, **kwargs):
                    calls.append((command, kwargs))
                    if command[0] == 'cargo':
                        kwargs['stdout'].write(json.dumps(self.artifact(executable, name='latency_bench')) + '\n')
                    else:
                        self.assertEqual(command, [executable, *args],
                                         'launch the Cargo example, not the ordinary-app artifact directory')
                        kwargs['stdout'].write(json.dumps(rows[0]) + '\n')

                # First establish the real helper's independent JSON/path fixture.
                with patch.dict(os.environ, {'TOR_TEST_PROFILE': profile}), \
                        patch.object(support.subprocess, 'run', side_effect=run):
                    self.assertEqual(support.benchmark(case, 'latency_bench', args, 120), rows)
                self.assertEqual(len(calls), 2)
                calls.clear()

                def report_boundary(actual_rows, **kwargs):
                    self.assertEqual(actual_rows, rows)
                    self.assertEqual(kwargs, {'selected_case': 'stream-r16-durable'})
                    # This fixture qualifies the launcher, not benchmark report semantics.
                    raise ReportBoundary

                with patch.dict(os.environ, {'TOR_TEST_PROFILE': profile}), \
                        patch.object(support.subprocess, 'run', side_effect=run), \
                        patch('performance_report.validate', side_effect=report_boundary), \
                        self.assertRaises(ReportBoundary):
                    case.test_streaming_benchmark_emits_valid_nested_acquisition_profiles()
                build = ['cargo', 'build', '-p', 'tor-server', '--example', 'latency_bench',
                         '--locked', '--message-format=json']
                if profile == 'release':
                    build.append('--release')
                self.assertEqual([command for command, _ in calls], [build, [executable, *args]])
                for index, (_, kwargs) in enumerate(calls):
                    self.assertEqual(kwargs['timeout'], 600 if index == 0 else 120)
                    self.assertEqual(kwargs['cwd'], ROOT)
                    self.assertTrue(kwargs['check'])

    def test_launcher_propagates_build_run_and_report_failures(self):
        for failure_at in (1, 2):
            with self.subTest(failure_at=failure_at), tempfile.TemporaryDirectory() as directory:
                case = SimpleNamespace(directory=Path(directory), bin=Path(directory))
                calls = []

                def fail(command, **kwargs):
                    calls.append(command)
                    if len(calls) == failure_at:
                        raise subprocess.CalledProcessError(3, command)
                    kwargs['stdout'].write(json.dumps(self.artifact(case.bin / 'physics_bench')) + '\n')

                with patch.object(support.subprocess, 'run', side_effect=fail), self.assertRaises(subprocess.CalledProcessError):
                    support.benchmark(case, 'physics_bench', ['--smoke'], 180)
        with tempfile.TemporaryDirectory() as directory:
            case = SimpleNamespace(directory=Path(directory), bin=Path(directory))

            def malformed(command, **kwargs):
                if command[0] == 'cargo':
                    kwargs['stdout'].write(json.dumps(self.artifact(case.bin / 'physics_bench')) + '\n')
                else:
                    kwargs['stdout'].write('not json\n')

            with patch.object(support.subprocess, 'run', side_effect=malformed), self.assertRaises(json.JSONDecodeError):
                support.benchmark(case, 'physics_bench', ['--smoke'], 180)

    def test_launcher_rejects_invalid_profiles_before_building(self):
        for profile in ('', 'dev', 'optimized', 'Release'):
            with patch.dict(os.environ, {'TOR_TEST_PROFILE': profile}), patch.object(support.subprocess, 'run') as run:
                with self.subTest(profile=profile), self.assertRaises(ValueError):
                    support.benchmark(SimpleNamespace(), 'physics_bench', [], 180)
                run.assert_not_called()

    def test_launcher_requires_one_exact_non_test_example_artifact(self):
        with tempfile.TemporaryDirectory() as directory:
            case = SimpleNamespace(directory=Path(directory), bin=Path(directory))
            artifact = self.artifact(case.bin / 'physics_bench')
            for artifacts in ([], [self.artifact(case.bin / 'other', name='other')],
                              [self.artifact(case.bin / 'physics_bench', test=True)],
                              [artifact, artifact], [self.artifact('relative-path')]):
                def run(command, **kwargs):
                    kwargs['stdout'].write(''.join(json.dumps(row) + '\n' for row in artifacts))

                with self.subTest(artifacts=artifacts), patch.object(support.subprocess, 'run', side_effect=run) as run_mock:
                    with self.assertRaises(ValueError):
                        support.benchmark(case, 'physics_bench', [], 180)
                    self.assertEqual(run_mock.call_count, 1)

    def test_explicit_smoke_setup_does_not_build_unused_application_binaries(self):
        with patch('process_harness.binaries') as binaries:
            physics_smoke_process.PhysicsCorrectnessSmoke.setUpClass()
        binaries.assert_not_called()


if __name__ == '__main__':
    unittest.main()
