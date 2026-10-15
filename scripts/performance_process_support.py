"""Shared benchmark launch and physics assertions for full and explicit smoke tests."""
import json
import os
import subprocess

from physics_performance_report import summarize
from process_harness import ROOT, SUFFIX


def benchmark(case, name, args, timeout):
    build = ['cargo', 'build', '-p', 'tor-server', '--example', name, '--locked']
    if os.environ.get('TOR_TEST_PROFILE', 'debug') == 'release':
        build.append('--release')
    with (case.directory/f'{name}-build.log').open('w', encoding='utf-8') as log:
        subprocess.run(build, cwd=ROOT, stdout=log, stderr=log, check=True, timeout=600)
    output = case.directory/f'{name}.jsonl'
    with output.open('w', encoding='utf-8') as log, (case.directory/f'{name}-stderr.log').open('w', encoding='utf-8') as errors:
        subprocess.run([case.bin/'examples'/(name+SUFFIX), *args],
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
