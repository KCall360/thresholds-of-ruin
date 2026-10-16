"""Optional reviewed expectation-repair reuse; any missing proof runs full CI.

Source receipts stay immutable. A composite is never fresh source evidence and
never grants cross-head campaign resume. The registry relies on code review to
maintain its audited consumer boundary, not universal dependency inference.
"""
import argparse
import base64
from datetime import datetime, timedelta, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

import ci_adoption as adoption
import ci_observation as observation
import ci_reuse_policy as policy
import verify
from ci_qualification import (PREFIX, REPOSITORY, REPOSITORY_ID, SHA, SOURCE_NAME, WORKFLOW_ID,
                              GitHub, commit, controls, pages, require, run_identity, run_lifecycle,
                              run_snapshot, timestamp)

PROOF = verify.ROOT / '.local/verify/cross-head.json'


def tree(api, sha):
    value = api.get(f'{PREFIX}/git/trees/{sha}?recursive=1')
    require(value['sha'] == sha and value['truncated'] is False and len(value['tree']) <= 10000, 'incomplete input tree')
    result, cases = {}, set()
    for row in value['tree']:
        path = row['path']
        require(isinstance(path, str) and re.fullmatch(r'[A-Za-z0-9_./+-]{1,512}', path)
                and all(part not in ('', '.', '..') for part in path.split('/')) and path.casefold() not in cases,
                'ambiguous input path')
        cases.add(path.casefold())
        if row['type'] == 'tree': continue
        require(row['type'] == 'blob' and row['mode'] in ('100644', '100755') and SHA.fullmatch(row['sha']), 'unsupported input entry')
        result[path] = [row['mode'], row['sha']]
    return result


def blob(api, sha):
    value = api.get(f'{PREFIX}/git/blobs/{sha}')
    require(value['sha'] == sha and value['encoding'] == 'base64' and type(value['size']) is int
            and 0 < value['size'] <= 128 * 1024, 'unsupported source blob')
    raw = base64.b64decode(''.join(value['content'].split()), validate=True)
    require(len(raw) == value['size'] and hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest() == sha,
            'source blob identity mismatch')
    return raw


def pull_binding(pull, number, head, base, synthetic):
    require(pull['number'] == number and pull['state'] == 'open' and pull['merged'] is False
            and pull['head']['sha'] == head and pull['base']['sha'] == base and pull['base']['ref'] == 'main'
            and pull['merge_commit_sha'] == synthetic
            and pull['head']['repo']['id'] == pull['base']['repo']['id'] == REPOSITORY_ID, 'PR relationship changed')


def source_interval(run, target_created, now):
    started, updated = timestamp(run['run_started_at']), timestamp(run['updated_at'])
    require(started <= updated <= now and started <= timestamp(target_created) <= now
            and now - started <= timedelta(hours=24), 'source endpoint interval invalid')
    return started, updated


def inspect(api, target, *, now):
    """Separate cross-head identity proof; exact-tree main adoption stays separate."""
    github = target['github']
    match = re.fullmatch(r'refs/pull/([1-9][0-9]*)/merge', github['ref'])
    require(match and github['event'] == 'pull_request' and github['repository'] == REPOSITORY, 'not a same-repository PR')
    number, target_id, synthetic = int(match[1]), github['run_id'], target['checkout']['sha']
    current = api.get(f'{PREFIX}/actions/runs/{target_id}')
    run_identity(current, target_id, 'pull_request')
    require(current['status'] == 'in_progress' and current['conclusion'] is None, 'target run is not active')
    merged = commit(api, synthetic)
    base, head = [row['sha'] for row in merged['parents']]
    require(current['head_sha'] == head and target['checkout']['parents'] == [base, head]
            and target['checkout']['tree'] == merged['tree']['sha'], 'target synthetic mismatch')
    pull = api.get(f'{PREFIX}/pulls/{number}')
    pull_binding(pull, number, head, base, synthetic)
    require(api.get(f'{PREFIX}/git/ref/heads/main')['object']['sha'] == base, 'PR base advanced')
    new_head = commit(api, head)
    require(len(new_head['parents']) == 1, 'only direct corrective head updates are supported')
    old_head = new_head['parents'][0]['sha']
    query = f'{PREFIX}/actions/workflows/{WORKFLOW_ID}/runs?event=pull_request&head_sha={old_head}'
    listed = pages(api, query, 'workflow_runs')
    require(len(listed) == 1, 'ambiguous prior head run')
    source = api.get(f"{PREFIX}/actions/runs/{listed[0]['id']}/attempts/1")
    source_current = api.get(f"{PREFIX}/actions/runs/{source['id']}")
    for run in (source, listed[0], source_current):
        run_identity(run, source['id'], 'pull_request')
        require(run['head_sha'] == old_head and run['id'] != target_id and run['status'] == 'completed'
                and run['conclusion'] in ('success', 'failure', 'cancelled'), 'source run is not terminal first-attempt evidence')
        refs = run['pull_requests']
        require(len(refs) == 1 and refs[0]['number'] == number and refs[0]['base']['sha'] == base
                and refs[0]['head']['sha'] == old_head and refs[0]['base']['ref'] == 'main'
                and refs[0]['base']['repo']['id'] == refs[0]['head']['repo']['id'] == REPOSITORY_ID, 'prior PR association mismatch')
    require(run_lifecycle(source) == run_lifecycle(listed[0]) == run_lifecycle(source_current), 'source attempt changed')
    for run in (source, listed[0], source_current): source_interval(run, current['created_at'], now)
    jobs = pages(api, f"{PREFIX}/actions/runs/{source['id']}/attempts/1/jobs", 'jobs')
    identities = [SOURCE_NAME.fullmatch(job['name']) for job in jobs if SOURCE_NAME.fullmatch(job['name'])]
    require(len(identities) == 1 and len({job['id'] for job in jobs}) == len(jobs), 'ambiguous source jobs')
    old_synthetic = identities[0][1]
    old = commit(api, old_synthetic)
    require([row['sha'] for row in old['parents']] == [base, old_head], 'prior synthetic base or head mismatch')
    baseline = controls(api, commit(api, base)['tree']['sha'])
    for revision in (old, merged, new_head, commit(api, old_head)):
        require(controls(api, revision['tree']['sha']) == baseline, 'reviewed controls changed in PR')
    old_tree, new_tree = tree(api, old['tree']['sha']), tree(api, merged['tree']['sha'])
    changes = policy.validate_trees(old_tree, new_tree, lambda path: blob(api, old_tree[path][1]),
                                   lambda path: blob(api, new_tree[path][1]))
    return {'schema_version': 1, 'mode': 'cross_head_composite', 'pr': number, 'base': base,
            'target': {'run_id': target_id, 'head': head, 'checkout_sha': synthetic, 'tree': merged['tree']['sha'], 'created_at': current['created_at']},
            'source': {'run_id': source['id'], 'attempt': 1, 'head': old_head, 'checkout_sha': old_synthetic,
                       'tree': old['tree']['sha'], 'run': source, 'listed': listed[0], 'current': source_current},
            'changes': changes}, jobs, query


def source_cell(api, identity, jobs, platform, profile, partition, *, now):
    source, target = identity['source'], identity['target']
    selected = [job for job in jobs if job['name'] == f'Rust ({platform}, {profile}, {partition})']
    require(len(selected) == 1, 'ambiguous source cell')
    job = selected[0]
    require(job['run_id'] == source['run_id'] and job['run_attempt'] == 1 and job['head_sha'] == source['head']
            and job['status'] == 'completed' and job['conclusion'] in (('success',) if partition == 'rust' else ('success', 'failure'))
            and platform in job['labels'], 'source cell did not complete')
    require(timestamp(source['run']['run_started_at']) <= timestamp(job['started_at']) <= timestamp(job['completed_at'])
            <= min(timestamp(target['created_at']), *(timestamp(source[key]['updated_at']) for key in ('run', 'listed', 'current'))), 'source cell interval is not prior and terminal')
    for name in ('Checkout exact source', 'Verify complete profile partition'):
        steps = [row for row in job['steps'] if row['name'] == name]
        require(len(steps) == 1 and steps[0]['status'] == 'completed'
                and steps[0]['conclusion'] == ('success' if name == 'Checkout exact source' else job['conclusion']), 'source verification step incomplete')
    artifacts = pages(api, f"{PREFIX}/actions/runs/{source['run_id']}/artifacts", 'artifacts')
    selected = [row for row in artifacts if row['name'] == f'verification-{platform}-{profile}-{partition}']
    require(len(selected) == 1, 'ambiguous source artifact')
    artifact, origin = selected[0], selected[0]['workflow_run']
    require(origin['id'] == source['run_id'] and origin['head_sha'] == source['head']
            and origin['repository_id'] == origin['head_repository_id'] == REPOSITORY_ID and artifact['expired'] is False,
            'artifact source mismatch')
    require(timestamp(job['started_at']) <= timestamp(artifact['created_at']) <= timestamp(artifact['updated_at'])
            <= timestamp(job['completed_at']) and now < timestamp(artifact['expires_at']), 'artifact outside cell interval')
    return job, artifact


def python_receipt(raw, expected, profile, changes):
    """Structural completion is separate from all-pass receipt completeness."""
    require(0 < len(raw) <= adoption.MAX_RECORD, 'receipt byte budget')
    rows = [adoption.document(line) for line in raw.splitlines()]
    require(len(rows) <= 30005 and len(rows) >= 3 and rows[0].get('kind') == 'selection' and rows[-1].get('kind') == 'suite', 'incomplete Python stage')
    require(all(type(row.get('schema_version')) is int and row['schema_version'] == 1 and row.get('profile') == profile for row in rows), 'receipt schema or profile mismatch')
    require(rows[0]['test_ids'] == expected and len(expected) == len(set(expected)) and 0 < len(expected) <= 10000, 'selection does not exactly cover inventory')
    require(all(row.get('kind') in ('test', 'diagnostic', 'build') for row in rows[1:-1]), 'unknown stage evidence')
    tests = [row for row in rows[1:-1] if row['kind'] == 'test']
    diagnostics = [row for row in rows[1:-1] if row['kind'] == 'diagnostic']
    builds = [row for row in rows[1:-1] if row['kind'] == 'build']
    require(len(builds) <= 1 and all(row.get('outcome') == 'passed' and type(row.get('returncode')) is int and row['returncode'] == 0 for row in builds), 'incomplete implicit build')
    require(sorted(row['test_id'] for row in tests) == sorted(expected), 'duplicate or missing outcomes')
    failed = []
    for row in tests:
        require(row.get('outcome') in ('passed', 'failed') and all(type(row.get(key)) is int and row[key] == 0
                for key in ('errors', 'subtests_failed', 'subtests_skipped')), 'unsupported outcome or subtest')
        failure = row['outcome'] == 'failed'
        require(type(row.get('failures')) is int and row['failures'] == int(failure), 'failure accounting mismatch')
        if failure: failed.append(row['test_id'])
    require(sorted(row['test_id'] for row in diagnostics) == sorted(failed), 'missing or extra diagnostics')
    for diagnostic in diagnostics:
        name = diagnostic['test_id']
        path = 'scripts/' + name.split('.')[0] + '.py'
        context = diagnostic.get('failure_context', {})
        source = context.get('source', {})
        require(path in changes and name in changes[path]['methods'] and diagnostic.get('exception_type') == 'AssertionError'
                and diagnostic.get('traceback_truncated') is False and isinstance(diagnostic.get('traceback'), str)
                and 0 < len(diagnostic['traceback']) <= 65536 and 'subtest_id' not in diagnostic and context.get('phase') == 'method'
                and source.get('path') == path and {'line': source.get('line'), 'function': source.get('function')} in changes[path]['old_sites'],
                'failure is not a reviewed expectation assertion')
    terminal = rows[-1]
    require(type(terminal.get('tests_run')) is int and terminal['tests_run'] == len(expected)
            and type(terminal.get('failures')) is int and terminal['failures'] == len(failed)
            and terminal.get('successful') is (not failed)
            and all(type(terminal.get(key)) is int and terminal[key] == 0 for key in ('errors', 'skipped', 'waived', 'expected_failures', 'unexpected_successes'))
            and not terminal.get('waived_tests') and not terminal.get('not_started') and not terminal.get('batch_errors'), 'terminal stage accounting mismatch')
    return {'passed': sorted(set(expected) - set(failed)), 'failed': sorted(failed),
            'sha256': hashlib.sha256(raw).hexdigest(), 'bytes': len(raw)}


def inventory(profile):
    import ci_policy
    rows = [row for row in ci_policy.discover()['tests'] if row['discovery'] == 'default']
    if profile == 'release': rows = [row for row in rows if Path(row['source']).name.endswith('process.py')]
    return rows


def validate_cell(records, identity, job, target, *, profile, partition, jobs, workers, rows):
    before, after, platform = adoption.validate_observations(records, target, profile=profile, partition=partition, jobs=jobs, workers=workers)
    for key, record in (('source', before), ('target', target)):
        bound = identity[key]
        checkout, github = record['checkout'], record['github']
        require(checkout['sha'] == bound['checkout_sha'] and checkout['tree'] == bound['tree']
                and checkout['parents'] == [identity['base'], bound['head']], 'cross-head checkout mismatch')
        require(github['repository'] == REPOSITORY and github['job'] == 'profiles' and github['event'] == 'pull_request'
                and github['run_id'] == bound['run_id'] and type(github['run_attempt']) is int and github['run_attempt'] == 1
                and github['reported_sha'] == github['workflow_sha'] == bound['checkout_sha']
                and github['ref'] == f"refs/pull/{identity['pr']}/merge"
                and github['workflow_ref'] == f"{REPOSITORY}/.github/workflows/ci.yml@{github['ref']}", 'cross-head workflow mismatch')
    require(timestamp(job['started_at']) <= timestamp(before['recorded_at']) <= timestamp(after['recorded_at'])
            <= timestamp(job['completed_at']) <= timestamp(identity['target']['created_at']), 'observation outside prior cell')
    require(all(target['build_test_environment']['variables'][name] == {'status': 'absent'} for name in policy.ROUTE_ENVIRONMENT), 'unsupported route mode')
    if partition == 'rust':
        adoption.validate_receipts(records, target, profile=profile, partition=partition, jobs=jobs, workers=workers)
        return {'expected': [], 'fresh': [], 'borrowed': [], 'source_receipt': None}
    names = [name for name in records if name.endswith('/summary.json')]
    require(len(names) == 1 and re.fullmatch(rf'[^/]+-full-ci-{profile}-python/summary.json', names[0]), 'ambiguous full summary')
    summary, prefix = adoption.document(records[names[0]]), names[0].rsplit('/', 1)[0]
    require(summary.get('evidence_origin') == 'fresh' and 'cross_head' not in summary
            and type(summary['schema_version']) is int and summary['schema_version'] == 1
            and summary['tier'] == 'full' and summary['scope'] == 'ci-partition' and summary['ci_profile'] == profile
            and summary['ci_partition'] == partition and type(summary['jobs']) is int and summary['jobs'] == jobs
            and type(summary['test_workers']) is int and summary['test_workers'] == workers
            and summary['graphical_waiver'] is None and summary['waived_steps'] == summary['not_run_steps'] == [], 'unsupported source summary')
    plan = verify.plan('full', set(), [], xvfb=target['graphical']['mode'] == 'xvfb', ci_profile=profile, ci_partition=partition, workers=workers)
    require(summary['planned_steps'] == [name for name, _, _ in plan] == [row['step'] for row in summary['steps']], 'full source stage inventory mismatch')
    expected = sorted(row['test_id'] for row in rows)
    receipt = None
    with tempfile.TemporaryDirectory() as directory:
        for (name, command, overrides), stage in zip(plan, summary['steps']):
            environment = observation.build_test_environment({**os.environ, **verify.verification_environment(jobs, overrides)}, [])
            environment['scope'] = 'verifier_stage'
            require(stage['command'] == command and stage['cwd'] == '.' and stage['environment'] == environment
                    and stage['environment_issues'] == [] and stage['waived_tests'] == [] and stage['note'] == '', 'source invocation changed')
            if verify.python_test_prefix(command) is not None:
                receipt = python_receipt(records[f'{prefix}/{name}-tests.jsonl'], expected, profile, identity['changes'])
                failed = bool(receipt['failed'])
                require(type(stage['code']) is int and stage['code'] == int(failed) and stage['status'] == ('failed' if failed else 'passed')
                        and summary['gate_passed'] is (not failed) and summary['clean_passed'] is (not failed)
                        and job['conclusion'] == ('failure' if failed else 'success'), 'stage result disagrees with receipt')
            else:
                path = Path(directory) / f'{name}.log'
                path.write_bytes(records[f'{prefix}/{name}.log'])
                require(stage['status'] == 'passed' and verify.receipt_completeness([(name, command, overrides)],
                        [{**stage, 'log': str(path)}], directory, jobs)['complete'], 'source setup stage incomplete')
    require(receipt is not None, 'missing Python stage')
    changed = {name for leaf in identity['changes'].values() for name in leaf['methods']}
    fresh = sorted(row['test_id'] for row in rows if row['kind'] == 'tool' or row['test_id'] in changed)
    borrowed = sorted(set(expected) - set(fresh))
    require(changed <= set(fresh) and set(borrowed) <= set(receipt['passed']) and set(receipt['failed']) <= set(fresh), 'uncovered source failures')
    return {'expected': expected, 'fresh': fresh, 'borrowed': borrowed, 'source_receipt': receipt}


def direct_consumers(changes, revision):
    """Defense in depth only; computed consumers remain a registry review duty."""
    command = ['git', '--no-replace-objects', 'grep', '--name-only', '-z', '-F']
    for path in changes: command.extend(['-e', Path(path).stem])
    command.extend([revision, '--', '*.py', '*.rs', '*.toml'])
    result = subprocess.run(command, cwd=verify.ROOT, capture_output=True, timeout=10, check=False)
    require(result.returncode in (0, 1) and len(result.stdout) <= 2 * 1024 * 1024, 'consumer scan unavailable')
    for value in result.stdout.split(b'\0'):
        if not value: continue
        prefix, path = value.decode('utf-8').split(':', 1)
        require(prefix == revision, 'consumer scan revision mismatch')
        cheap = path.startswith('scripts/test_') and path.endswith('.py') and not path.endswith('process.py')
        require(path in changes or cheap, 'registered leaf has a direct borrowed consumer')


def attempt(args, output):
    deadline = time.monotonic() + 90
    require(os.environ.get('GITHUB_EVENT_NAME') == 'pull_request' and os.environ.get('GITHUB_REPOSITORY') == REPOSITORY,
            'cross-head reuse is PR-only')
    target = adoption.document((verify.ROOT / '.local/verify/observations/before.json').read_bytes())
    require(target['checkout']['sha'] == os.environ['GITHUB_SHA'] and target['github']['run_id'] == int(os.environ['GITHUB_RUN_ID']), 'target environment mismatch')
    api, now = GitHub(os.environ['GH_TOKEN']), datetime.now(timezone.utc)
    identity, jobs, query = inspect(api, target, now=now)
    direct_consumers(identity['changes'], identity['target']['checkout_sha'])
    job, artifact = source_cell(api, identity, jobs, args.platform, args.profile, args.partition, now=now)
    records = adoption.artifact_records(adoption.download_artifact(api, artifact, deadline=deadline), deadline=deadline)
    limits = target['execution_limits']
    coverage = validate_cell(records, identity, job, target, profile=args.profile, partition=args.partition,
                             jobs=limits['jobs'], workers=limits['test_workers'], rows=inventory(args.profile) if args.partition == 'python' else [])
    source, final = identity['source'], identity['target']
    latest = api.get(f"{PREFIX}/actions/runs/{source['run_id']}")
    run_identity(latest, source['run_id'], 'pull_request')
    relisted = pages(api, query, 'workflow_runs')
    require(len(relisted) == 1 and run_snapshot(relisted[0]) == run_snapshot(source['listed'])
            and run_snapshot(latest) == run_snapshot(source['current'])
            and run_snapshot(api.get(f"{PREFIX}/actions/runs/{source['run_id']}/attempts/1")) == run_snapshot(source['run']), 'source lifecycle changed')
    source_interval(latest, final['created_at'], datetime.now(timezone.utc))
    latest_job, latest_artifact = source_cell(api, identity,
        pages(api, f"{PREFIX}/actions/runs/{source['run_id']}/attempts/1/jobs", 'jobs'), args.platform, args.profile, args.partition, now=datetime.now(timezone.utc))
    require(latest_job == job and latest_artifact == artifact, 'source cell or artifact changed')
    pull_binding(api.get(f"{PREFIX}/pulls/{identity['pr']}"), identity['pr'], final['head'], identity['base'], final['checkout_sha'])
    current = api.get(f"{PREFIX}/actions/runs/{final['run_id']}")
    run_identity(current, final['run_id'], 'pull_request')
    require(current['status'] == 'in_progress' and current['conclusion'] is None and current['head_sha'] == final['head'], 'target no longer active')
    adoption.remaining(deadline)
    source['snapshot'] = run_snapshot(source['current'])
    del source['run'], source['listed'], source['current']
    identity.update(coverage=coverage, cell=[args.platform, args.profile, args.partition], limits=limits,
                    artifact={'id': artifact['id'], 'digest': artifact['digest']},
                    source_job={'id': job['id'], 'started_at': job['started_at'], 'completed_at': job['completed_at']})
    output.write_text(json.dumps(identity, sort_keys=True) + '\n', encoding='utf-8')


def recheck(api, proof, *, now):
    """Revalidate mutable authority after fresh execution, without downloading again."""
    target, source = proof['target'], proof['source']
    pull_binding(api.get(f"{PREFIX}/pulls/{proof['pr']}"), proof['pr'], target['head'], proof['base'], target['checkout_sha'])
    require(api.get(f'{PREFIX}/git/ref/heads/main')['object']['sha'] == proof['base'], 'base advanced during fresh execution')
    current = api.get(f"{PREFIX}/actions/runs/{target['run_id']}")
    run_identity(current, target['run_id'], 'pull_request')
    require(current['status'] == 'in_progress' and current['conclusion'] is None and current['head_sha'] == target['head'],
            'target superseded during fresh execution')
    previous = api.get(f"{PREFIX}/actions/runs/{source['run_id']}")
    run_identity(previous, source['run_id'], 'pull_request')
    require(run_snapshot(previous) == source['snapshot'], 'source superseded during fresh execution')
    source_interval(previous, target['created_at'], now)


def final_authority(proof, logs):
    path = Path(logs) / 'cross-head-recheck.json'
    path.write_text(json.dumps(proof, sort_keys=True) + '\n', encoding='utf-8')
    platform, profile, partition = proof['cell']
    command = [sys.executable, __file__, '--platform', platform, '--profile', profile, '--partition', partition,
               '--recheck-proof', str(path)]
    require(adoption.bounded_worker(command, timeout=60), 'final live PR authority unavailable or changed')


def current_checkout(proof):
    issues = []
    checkout = observation.checkout(verify.ROOT, issues)
    target = proof['target']
    require(not issues and checkout == {'sha': target['checkout_sha'], 'tree': target['tree'],
            'parents': [proof['base'], target['head']], 'tracked_dirty': False, 'untracked_present': False},
            'current checkout differs from proof or is dirty')


def selected_plan(proof, steps, *, profile, partition, jobs, workers, directory):
    """Only a worker-validated current-target handoff may narrow execution."""
    require(proof['schema_version'] == 1 and proof['mode'] == 'cross_head_composite'
            and proof['target']['checkout_sha'] == os.environ.get('GITHUB_SHA')
            and proof['target']['run_id'] == int(os.environ.get('GITHUB_RUN_ID', '0'))
            and os.environ.get('GITHUB_EVENT_NAME') == 'pull_request'
            and proof['cell'][1:] == [profile, partition] and proof['limits'] == {'jobs': jobs, 'test_workers': workers}, 'proof does not bind target invocation')
    current_checkout(proof)
    if partition == 'rust': return []
    rows = inventory(profile)
    expected = sorted(row['test_id'] for row in rows)
    coverage = proof['coverage']
    changed = {name for leaf in proof['changes'].values() for name in leaf['methods']}
    fresh = sorted(row['test_id'] for row in rows if row['kind'] == 'tool' or row['test_id'] in changed)
    require(coverage['expected'] == expected and coverage['fresh'] == fresh
            and coverage['borrowed'] == sorted(set(expected) - set(fresh)), 'current coverage changed')
    destination = Path(directory) / 'fresh-test-ids.txt'
    destination.write_text('\n'.join(fresh) + '\n', encoding='utf-8')
    result = []
    for name, command, overrides in steps:
        prefix = verify.python_test_prefix(command)
        if prefix is not None:
            command = [*prefix, '--workers', str(workers), '--from-file', str(destination), '-v']
            overrides = {**overrides, '_cwd': 'scripts'}
        result.append((name, command, overrides))
    return result


def union(proof, steps, results, logs, jobs):
    """The gate requires immutable borrowed coverage plus exact fresh outcomes."""
    require(proof['target']['checkout_sha'] == os.environ.get('GITHUB_SHA')
            and proof['target']['run_id'] == int(os.environ.get('GITHUB_RUN_ID', '0')), 'union target mismatch')
    current_checkout(proof)
    final_authority(proof, logs)
    coverage = proof['coverage']
    if proof['cell'][2] == 'rust':
        require(steps == results == [] and coverage['expected'] == coverage['fresh'] == coverage['borrowed'] == [], 'Rust whole-cell coverage mismatch')
        return {'complete': True, 'borrowed_cell': True, 'fresh_ids': [], 'borrowed_ids': []}
    fresh, borrowed, expected = (coverage[key] for key in ('fresh', 'borrowed', 'expected'))
    require(all(len(group) == len(set(group)) for group in (fresh, borrowed, expected))
            and not set(fresh) & set(borrowed) and sorted(fresh + borrowed) == expected
            and set(borrowed) <= set(coverage['source_receipt']['passed']), 'coverage union has holes or overlap')
    require(verify.receipt_completeness(steps, results, logs, jobs)['complete'], 'fresh stages incomplete')
    names = [name for name, command, _ in steps if verify.python_test_prefix(command) is not None]
    require(len(names) == 1, 'ambiguous fresh Python stage')
    receipt = python_receipt((Path(logs) / f'{names[0]}-tests.jsonl').read_bytes(), fresh, proof['cell'][1], {})
    require(receipt['failed'] == [], 'fresh tests failed')
    return {'complete': True, 'borrowed_cell': False, 'fresh_ids': fresh, 'borrowed_ids': borrowed,
            'fresh_receipt': receipt, 'source_receipt': coverage['source_receipt']}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    parser.add_argument('--profile', choices=('debug', 'release'), required=True)
    parser.add_argument('--partition', choices=('rust', 'python'), required=True)
    parser.add_argument('--platform', choices=('ubuntu-latest', 'windows-latest'), required=True)
    parser.add_argument('--worker-proof', type=Path, help=argparse.SUPPRESS)
    parser.add_argument('--recheck-proof', type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    try:
        if args.recheck_proof:
            raw = args.recheck_proof.read_bytes()
            require(len(raw) <= 2 * 1024 * 1024, 'proof exceeds budget')
            recheck(GitHub(os.environ['GH_TOKEN']), adoption.document(raw), now=datetime.now(timezone.utc))
        elif args.worker_proof:
            attempt(args, args.worker_proof)
        else:
            PROOF.unlink(missing_ok=True)
            with tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / 'proof.json'
                command = [sys.executable, __file__, '--profile', args.profile, '--partition', args.partition,
                           '--platform', args.platform, '--worker-proof', str(output)]
                require(adoption.bounded_worker(command), 'cross-head proof unavailable')
                raw = output.read_bytes()
                require(len(raw) <= 2 * 1024 * 1024, 'proof exceeds budget')
                adoption.document(raw)
                PROOF.parent.mkdir(parents=True, exist_ok=True)
                PROOF.write_bytes(raw)
        sys.stdout.buffer.write(b'adopted\n')
        return 0
    except Exception:
        return 1  # Optional bounded helper: ordinary full verification is fallback.


if __name__ == '__main__':
    raise SystemExit(main())
