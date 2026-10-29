"""Shared benchmark launch and physics assertions for full and explicit smoke tests."""
import json
import os
from pathlib import Path
import subprocess

from physics_performance_report import summarize
from process_harness import ROOT


def benchmark_executable(case, name):
    """Build/check once and resolve Cargo's exact example, separate from app bundles."""
    profile = os.environ.get('TOR_TEST_PROFILE', 'debug')
    if profile not in ('debug', 'release'):
        raise ValueError('TOR_TEST_PROFILE must be debug or release')
    build = ['cargo', 'build', '-p', 'tor-server', '--example', name, '--locked', '--message-format=json']
    if profile == 'release':
        build.append('--release')
    build_log = case.directory/f'{name}-build.log'
    with build_log.open('w', encoding='utf-8') as log:
        subprocess.run(build, cwd=ROOT, stdout=log, stderr=log, check=True, timeout=600)
    artifacts = [json.loads(line) for line in build_log.read_text(encoding='utf-8').splitlines()
                 if line.startswith('{')]
    executables = [row.get('executable') for row in artifacts
                   if row.get('reason') == 'compiler-artifact'
                   and row.get('target', {}).get('name') == name
                   and row.get('target', {}).get('kind') == ['example']
                   and row.get('profile', {}).get('test') is False]
    if len(executables) != 1 or not isinstance(executables[0], str) or not Path(executables[0]).is_absolute():
        raise ValueError(f'Cargo must report one exact non-test executable for example {name}')
    return Path(executables[0])


def benchmark(case, name, args, timeout):
    executable = benchmark_executable(case, name)
    output = case.directory/f'{name}.jsonl'
    with output.open('w', encoding='utf-8') as log, (case.directory/f'{name}-stderr.log').open('w', encoding='utf-8') as errors:
        subprocess.run([executable, *args],
                       cwd=ROOT, stdout=log, stderr=errors, check=True, timeout=timeout)
    return [json.loads(line) for line in output.read_text(encoding='utf-8').splitlines()]


def assert_physics_report(case, rows, *, smoke=False):
    """Apply the same per-row correctness assertions to either exact sample matrix."""
    summarize(rows, smoke=smoke)
    case.assertEqual(len(rows), 8 if smoke else 24)
    for row in rows:
        status = row['checkpoint_profile']['status']
        case.assertEqual(status['accepted_sequence'], status['durable_sequence'])
        case.assertEqual(status['pending_bytes'], 0)
        case.assertFalse(status['saving'])
        case.assertIsNone(status['error'])
        case.assertGreaterEqual(status['checkpoints'], 1)
        case.assertGreaterEqual(status['last_commit_ms'], 0)
        case.assertLessEqual(status['last_commit_ms'], status['last_batch_ms'])
