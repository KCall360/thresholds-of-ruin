"""Opt-in same-build measurement after a complete Windows Python CI partition.

This does not implement a merge gate or evidence reuse. The full baseline remains
mandatory. The second pass is warm and ordered after the baseline, so its elapsed
seconds cannot predict total CI latency or demonstrate an unbiased speedup.
"""
import argparse
from collections import Counter
import json
import math
from pathlib import Path
import sys


def baseline_settings(summary, profile):
    """Recover the actual baseline concurrency, never choose a faster setting."""
    if (summary.get('scope') != 'ci-partition' or summary.get('tier') != 'full'
            or summary.get('ci_profile') != profile or summary.get('ci_partition') != 'python'
            or summary.get('clean_passed') is not True or summary.get('gate_passed') is not True
            or summary.get('waived_steps') != [] or summary.get('not_run_steps') != []):
        raise ValueError('a complete, unwaived full Python partition is required')
    name = 'python-debug' if profile == 'debug' else 'process-release'
    if any(row.get('code') != 0 or row.get('status') != 'passed' or row.get('waived_tests', [])
           for row in summary.get('steps', [])):
        raise ValueError('every baseline stage must have passed without waivers')
    steps = [row for row in summary.get('steps', []) if row.get('step') == name]
    if len(steps) != 1 or steps[0].get('code') != 0 or steps[0].get('status') != 'passed':
        raise ValueError('exactly one successful baseline test step is required')
    step = steps[0]
    command = step.get('command', [])
    if (not isinstance(command, list) or any(not isinstance(s, str) for s in command)
            or not any(Path(s).name == 'test_runner.py' for s in command)):
        raise ValueError('baseline must use the existing test runner')
    if command.count('--workers') > 1:
        raise ValueError('ambiguous worker count')
    try:
        workers = int(command[command.index('--workers') + 1]) if '--workers' in command else 1
    except (ValueError, IndexError) as error:
        raise ValueError('invalid baseline worker count') from error
    jobs, seconds = summary.get('jobs'), step.get('seconds')
    if (workers < 1 or type(jobs) is not int or jobs < 1 or type(seconds) not in (int, float)
            or not math.isfinite(seconds) or seconds < 0):
        raise ValueError('invalid concurrency or baseline duration')
    return workers, jobs, seconds


def validate_results(rows, selected, profile):
    """A zero runner exit cannot conceal missing, duplicate or skipped methods."""
    if any(row.get('kind') == 'local_graphical_waiver' for row in rows):
        raise ValueError('shadow measurement cannot waive native methods')
    tests = [row for row in rows if row.get('kind') == 'test']
    if Counter(row.get('test_id') for row in tests) != Counter(selected):
        raise ValueError('executed method inventory differs from selection')
    for row in tests:
        if (row.get('profile') != profile or row.get('outcome') != 'passed'
                or any(row.get(key) != 0 for key in ('failures', 'errors', 'subtests_failed', 'subtests_skipped'))):
            raise ValueError('every selected method and subtest must pass without skips')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    parser.add_argument('--profile', choices=('debug', 'release'), required=True)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args(argv)
    # Import discovery only for execution: unit checks do not need the policy.
    import ci_policy
    import verify
    try:
        if sys.platform != 'win32':
            raise ValueError('this measurement is for the Windows candidate only')
        summary = ci_policy.read_json(args.baseline)
        workers, jobs, baseline_seconds = baseline_settings(summary, args.profile)
        manifest = ci_policy.read_json(ci_policy.DEFAULT_MANIFEST)
        inventory = ci_policy.discover(audit_inputs=manifest['audit']['sources'])
        candidate = ci_policy.reviewed_candidate(inventory, manifest)
        selected = candidate
        baseline_ids = ci_policy.default_test_ids(inventory)
        if args.profile == 'release':
            process_ids = {row['test_id'] for row in inventory['tests'] if row['kind'] == 'process'}
            selected = [name for name in selected if name in process_ids]
            baseline_ids = [name for name in baseline_ids if name in process_ids]
        if not selected or len(selected) != len(set(selected)):
            raise ValueError('candidate must contain unique selected methods')
        output = args.output_dir.resolve()
        output.mkdir(parents=True, exist_ok=False)
        ids = output / 'selected.ids'
        ids.write_text('\n'.join(selected) + '\n', encoding='utf-8')
        command = [sys.executable, str(verify.SCRIPTS / 'test_runner.py'), '--workers', str(workers),
                   '--from-file', str(ids), '-v']
        env = {'TOR_TEST_PROFILE': args.profile}
        if args.profile == 'debug':
            # Match the baseline's scenario compiler, including a custom Cargo target.
            steps = verify.plan('full', set(), [], ci_profile='debug', ci_partition='python')
            env.update(next(values for name, _, values in steps if name == 'python-debug'))
        result = verify.run_step('shadow-process', command, env, output, jobs, False)
        timings = [json.loads(line) for line in (output / 'shadow-process-tests.jsonl').read_text(encoding='utf-8').splitlines()]
        error = None
        try:
            validate_results(timings, selected, args.profile)
        except ValueError as failure:
            error = str(failure)
        report = {'schema_version': 1, 'status': 'measurement_only', 'profile': args.profile,
                  'inventory_sha256': inventory['sha256'], 'baseline_ids': baseline_ids, 'selected': selected,
                  'deferred': sorted(set(baseline_ids) - set(selected)),
                  'supplemental': sorted(set(selected) - set(baseline_ids)),
                  'workers': workers, 'build_jobs': jobs, 'baseline_test_seconds': baseline_seconds,
                  'shadow_test_seconds': result['seconds'], 'exit_code': result['code'],
                  'coverage_error': error, 'passed': result['code'] == 0 and error is None,
                  'limitations': ['Second pass reuses builds and warm caches; order bias is uncontrolled.',
                                  'Test-step elapsed time only; not predicted whole-job or merge latency.',
                                  'All original full checks remain required.']}
        (output / 'comparison.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
        print(json.dumps({key: value for key, value in report.items() if key not in ('selected', 'baseline_ids', 'deferred', 'supplemental')}, indent=2))
        return 0 if report['passed'] else 1
    except (OSError, ValueError, KeyError, TypeError) as error:
        print('Shadow measurement failed: ' + str(error), file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
