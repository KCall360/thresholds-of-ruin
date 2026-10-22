"""Two explicit per-profile Windows Python owners; never reduce the full inventory.

Whole modules preserve class/module fixture ownership. Native graphical tests
remain serialized within each isolated runner; the shards share no desktop,
workspace, target, or process state. Durations do not select shard membership.
"""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re

import ci_example_prerequisites

SHARD_COUNT = 2
LONG_MODULES = frozenset({"test_rogue_earned_route_process", "test_arena_matrix_process",
                          "test_performance_process"})


def require(condition, message):
    if not condition:
        raise ValueError(message)


def selection(ids, index, *, profile="release"):
    """Partition the complete discovered profile selection, failing closed."""
    require(profile in ("debug", "release"), "Invalid Python shard profile")
    require(type(index) is int and 1 <= index <= SHARD_COUNT, "Invalid Python shard")
    require(isinstance(ids, list) and 0 < len(ids) <= 10000, "Empty or oversized full selection")
    pattern = r'test_\w+(?:\.[A-Za-z_]\w*){2,}' if profile == 'debug' else r'test_\w+_process(?:\.[A-Za-z_]\w*){2,}'
    require(all(isinstance(name, str) and len(name) <= 1024 and
                re.fullmatch(pattern, name) for name in ids),
            "Shard selection must be complete profile-test identities")
    require(len(ids) == len(set(ids)), "Duplicate full selection identity")
    groups = {1: [], 2: []}
    for name in ids:
        groups[1 if name.split('.')[0] in LONG_MODULES else 2].append(name)
    require(all(groups.values()), "Empty Python shard")
    require(Counter(groups[1] + groups[2]) == Counter(ids), "Inexact Python shard union")
    return groups[index]


def plan_digest(ids, *, profile="release"):
    selection(ids, 1, profile=profile)
    payload = {"version": 1, "count": SHARD_COUNT, "long_modules": sorted(LONG_MODULES),
               "full_test_ids": sorted(ids)}
    if profile == "debug":
        payload["profile"] = profile
    return hashlib.sha256(json.dumps(payload, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def discover_inventory(*, profile="release"):
    """Independent current-source discovery only, never test setup or execution."""
    import unittest
    import parallel_tests
    root = Path(__file__).resolve().parent
    previous = os.environ.get('TOR_TEST_PROFILE')
    require(profile in ('debug', 'release'), 'Invalid Python shard profile')
    os.environ['TOR_TEST_PROFILE'] = profile
    try:
        suite = unittest.TestLoader().discover(str(root), pattern='test_*.py' if profile == 'debug' else 'test_*process.py', top_level_dir=str(root))
        ids = [test.id() for test in parallel_tests.flatten(suite)]
    finally:
        if previous is None:
            os.environ.pop('TOR_TEST_PROFILE', None)
        else:
            os.environ['TOR_TEST_PROFILE'] = previous
    selection(ids, 1, profile=profile)  # Missing imports, empty owners and duplicates fail closed.
    return ids


def attempt_number(value):
    require(isinstance(value, str) and re.fullmatch(r'[1-9][0-9]{0,8}', value) is not None,
            'Invalid execution attempt')
    return int(value)


def cohort_attempt(producers, shards, current_attempt):
    """Retain one successful same-run cohort, never search older artifacts."""
    current = attempt_number(current_attempt)
    values = []
    roles = ((producers, {'producer_debug', 'producer_release'}),
             (shards, {'shard_debug_1', 'shard_debug_2', 'shard_release_1', 'shard_release_2'}))
    # Status is authoritative even when a failed newer job emitted no outputs.
    require(all(isinstance(group, dict) and group.get('result') == 'success'
                for group, _ in roles), 'Current Windows groups did not succeed')
    for group, keys in roles:
        outputs = group.get('outputs')
        require(isinstance(outputs, dict) and set(outputs) == keys, 'Missing or unexpected execution role')
        values.extend(attempt_number(outputs[key]) for key in sorted(keys))
    require(len(set(values)) == 1 and values[0] <= current,
            'Mixed or future Windows execution attempts; rerun all jobs')
    return {'origin_attempt': str(values[0]), 'aggregate_attempt': current_attempt}


def metadata(ids, index, source_sha, run_id, origin_attempt, *, profile="release"):
    selection(ids, index, profile=profile)
    attempt_number(origin_attempt)
    return {"version": 2, "origin_attempt": origin_attempt, "index": index, "count": SHARD_COUNT,
            "full_test_ids": ids, "plan_sha256": plan_digest(ids, profile=profile), "source_sha": source_sha, "run_id": run_id}


def validate_pair(receipts, summaries, *, expected_ids, source_sha, run_id, origin_attempt, profile="release"):
    """Both original complete owners must prove the exact current-source multiset."""
    attempt_number(origin_attempt)
    require(re.fullmatch(r'[0-9a-f]{40}', source_sha or '') is not None, "Invalid shard source")
    require(isinstance(run_id, str) and run_id.isdigit(), "Invalid shard run")
    require(set(receipts) == set(summaries) == {1, 2}, "Missing Python shard artifact")
    expected_digest = plan_digest(expected_ids, profile=profile)
    full = None
    completed = []
    for index in (1, 2):
        rows, summary = receipts[index], summaries[index]
        require(isinstance(rows, list) and len(rows) >= 3, "Incomplete shard receipt")
        require(rows[0].get('kind') == 'selection' and rows[-1].get('kind') == 'suite',
                "Incomplete shard lifecycle")
        header = rows[0].get('shard', {})
        require(type(header.get('version')) is int and header['version'] == 2
                and type(header.get('count')) is int and type(header.get('index')) is int
                and header['index'] == index and header.get('count') == SHARD_COUNT
                and header.get('source_sha') == source_sha and header.get('run_id') == run_id
                and header.get('origin_attempt') == origin_attempt,
                "Shard identity changed")
        inventory = header.get('full_test_ids')
        expected = selection(inventory, index, profile=profile)
        require(Counter(inventory) == Counter(expected_ids)
                and header.get('plan_sha256') == expected_digest, 'Incomplete current-source inventory')
        require(full is None or inventory == full, "Shard full inventories disagree")
        full = inventory
        require(rows[0].get('test_ids') == expected, "Shard selection changed")
        require(all(row.get('profile') == profile and type(row.get('schema_version')) is int and row['schema_version'] == 1 for row in rows),
                "Shard profile or receipt version changed")
        tests = [row for row in rows[1:-1] if row.get('kind') == 'test']
        require(all(row.get('kind') in ('test', 'build') for row in rows[1:-1]),
                "Unexpected shard receipt row")
        builds = [row for row in rows[1:-1] if row.get('kind') == 'build']
        require(len(builds) <= 1 and all(row.get('outcome') == 'passed'
                and type(row.get('returncode')) is int and row['returncode'] == 0 for row in builds),
                'Shard prerequisite build failed')
        require(Counter(row.get('test_id') for row in tests) == Counter(expected),
                "Missing or duplicate shard test outcome")
        require(all(row.get('outcome') == 'passed' and all(type(row.get(key)) is int and row[key] == 0 for key in
                    ('failures', 'errors', 'subtests_failed', 'subtests_skipped')) for row in tests),
                "Shard test did not pass")
        terminal = rows[-1]
        require(terminal.get('successful') is True and type(terminal.get('tests_run')) is int and terminal['tests_run'] == len(expected)
                and all(type(terminal.get(key)) is int and terminal[key] == 0 for key in
                        ('failures', 'errors', 'skipped', 'waived', 'expected_failures', 'unexpected_successes'))
                and not terminal.get('waived_tests') and not terminal.get('not_started')
                and not terminal.get('batch_errors'), "Shard did not finish cleanly")
        require(summary.get('ci_profile') == profile and summary.get('ci_partition') == 'python'
                and type(summary.get('ci_python_shard')) is int and summary['ci_python_shard'] == index
                and summary.get('clean_passed') is True
                and summary.get('gate_passed') is True and summary.get('evidence_origin') == 'fresh'
                and not summary.get('waived_steps') and not summary.get('not_run_steps')
                and summary.get('receipt_completeness', {}).get('complete') is True,
                "Shard verifier did not finish cleanly")
        stages = ['build-process-debug', 'build-tools', 'python-debug'] if profile == 'debug' else ['build-process-release', 'process-release']
        prerequisite = ci_example_prerequisites.stage(expected, profile)
        if prerequisite:
            stages.insert(-1, prerequisite[0])
        require(summary.get('planned_steps') == stages
                and [row.get('step') for row in summary.get('steps', [])] == stages
                and all(row.get('status') == 'passed' and type(row.get('code')) is int and row['code'] == 0
                        for row in summary['steps']), 'Missing or failed shard verification stage')
        completed.extend(row['test_id'] for row in tests)
    require(Counter(completed) == Counter(full), "Incomplete or overlapping shard union")
    return {"source_sha": source_sha, "run_id": run_id, "origin_attempt": origin_attempt, "shards": SHARD_COUNT,
            "selected_tests": len(full), "completed_tests": len(completed), "complete": True}


def read_pair(directory, *, source_sha, run_id, origin_attempt, expected_ids=None, profile="release"):
    if expected_ids is None:
        expected_ids = discover_inventory(profile=profile)
    require(profile in ("debug", "release"), "Invalid Python shard profile")
    stage = "python-debug" if profile == "debug" else "process-release"
    receipts, summaries = {}, {}
    for index in (1, 2):
        root = Path(directory) / str(index)
        logs = list(root.glob(f'**/{stage}-tests.jsonl'))
        reports = list(root.glob('**/summary.json'))
        require(len(logs) == len(reports) == 1, "Missing or ambiguous Python shard artifact")
        require(logs[0].stat().st_size <= 16 * 1024 * 1024
                and reports[0].stat().st_size <= 2 * 1024 * 1024, "Oversized shard evidence")
        raw = logs[0].read_bytes()
        receipts[index] = [json.loads(line, object_pairs_hook=unique_object) for line in raw.decode('utf-8').splitlines()]
        summaries[index] = json.loads(reports[0].read_text(encoding='utf-8'), object_pairs_hook=unique_object)
        stages = summaries[index].get('receipt_completeness', {}).get('stages', [])
        bound = [row for row in stages if row.get('step') == stage]
        require(len(bound) == 1 and bound[0].get('complete') is True
                and bound[0].get('record') == {'file': logs[0].name, 'bytes': len(raw),
                                              'sha256': hashlib.sha256(raw).hexdigest()},
                'Shard receipt does not match verifier binding')
    return validate_pair(receipts, summaries, expected_ids=expected_ids, source_sha=source_sha, run_id=run_id, origin_attempt=origin_attempt, profile=profile)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'Duplicate shard evidence key')
        result[key] = value
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', choices=('debug', 'release'), default='release')
    parser.add_argument('--directory', type=Path)
    parser.add_argument('--origin-attempt')
    parser.add_argument('--cohort', action='store_true')
    parser.add_argument('--current-attempt')
    parser.add_argument('--source-sha', required=True)
    parser.add_argument('--run-id', required=True)
    args = parser.parse_args()
    require(re.fullmatch(r'[0-9a-f]{40}', args.source_sha) is not None, 'Invalid shard source')
    require(re.fullmatch(r'[1-9][0-9]*', args.run_id) is not None, 'Invalid shard run')
    if args.cohort:
        require(args.directory is None and args.origin_attempt is None, 'Ambiguous cohort request')
        result = cohort_attempt(
            json.loads(os.environ['WINDOWS_BINARIES'], object_pairs_hook=unique_object),
            json.loads(os.environ['WINDOWS_PYTHON'], object_pairs_hook=unique_object), args.current_attempt)
        result.update(source_sha=args.source_sha, run_id=args.run_id)
        with Path(os.environ['GITHUB_OUTPUT']).open('a', encoding='utf-8') as handle:
            handle.write(f"origin_attempt={result['origin_attempt']}\n")
    else:
        require(args.directory is not None and args.current_attempt is None, 'Missing or ambiguous shard directory')
        result = read_pair(args.directory, source_sha=args.source_sha, run_id=args.run_id,
                           origin_attempt=args.origin_attempt, profile=args.profile)
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
