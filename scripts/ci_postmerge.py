"""One full-PR seal may qualify the exact merged tree, never another tree/run.

This is an optional optimization of otherwise-full main pushes. The normal plan
is the fallback. No historical-success search, arbitrary proof file, selected-run
promotion, or binary/test-result relabeling is supported.
"""
import argparse
from datetime import datetime
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import threading
import time
import zipfile
import zlib

import ci_test_plan as planner

ROOT = Path(__file__).resolve().parent.parent
VERSION = 1
WORKFLOW = '.github/workflows/ci.yml'
MAX_BYTES = 2 * 1024 * 1024
MAX_AGE = 24 * 60 * 60
PROFILE_ROLES = tuple(f'{system}_{profile}_{partition}' for system in ('linux', 'windows')
                      for profile in ('debug', 'release') for partition in ('rust', 'python')
                      if partition != 'python' or (system == 'linux' and profile == 'release')) + (
                          'linux_debug_python_shard_1', 'linux_debug_python_shard_2')
TOOLCHAIN_ROLES = (*PROFILE_ROLES, 'producer_debug', 'producer_release', 'consumer')
FULL_GROUPS = {'scope', 'tooling', 'dependencies', 'profiles', 'windows_binaries', 'windows_python',
               'selected_consumer', 'check'}
SMOKE_IDS = (
    'test_paired_stairs_process.PairedStairsProcess.test_generated_round_trip_resume_and_spectator_disclosure',
    'test_rogue_exploration_process.RogueExplorationProcess.test_short_journal_only_exploration_resumes_without_source_files',
)
require = planner.require


def decode(raw):
    require(len(raw) <= MAX_BYTES, 'JSON size bound')
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, 'duplicate JSON key')
            result[key] = value
        return result
    try:
        return json.loads(raw, object_pairs_hook=unique,
                          parse_constant=lambda value: (_ for _ in ()).throw(ValueError('nonfinite JSON')))
    except RecursionError as error:
        raise ValueError('JSON nesting bound') from error


def toolchain(value=None):
    if value is None:
        rust = subprocess.check_output(['rustc', '-Vv'], text=True, timeout=20)
        cargo = subprocess.check_output(['cargo', '-V'], text=True, timeout=20).strip()
        fields = dict(line.split(': ', 1) for line in rust.splitlines() if ': ' in line)
        value = {'release': fields['release'], 'commit': fields['commit-hash'], 'cargo': cargo}
    if isinstance(value, str):
        value = decode(value)
    require(isinstance(value, dict) and set(value) == {'release', 'commit', 'cargo'}, 'toolchain fields')
    require(isinstance(value['release'], str) and re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+', value['release']),
            'only a resolved stable release is supported')
    require(isinstance(value['commit'], str) and planner.SHA.fullmatch(value['commit']), 'compiler commit')
    require(isinstance(value['cargo'], str) and re.fullmatch(r'cargo [\x20-\x7e]{1,180}', value['cargo']), 'Cargo identity')
    return value


def context(root=ROOT):
    git = lambda *args: planner.ci_scope.git(root, *args).decode().strip()
    source = git('rev-parse', 'HEAD')
    require(source == os.environ.get('GITHUB_SHA'), 'checkout/event identity')
    require(source == os.environ.get('GITHUB_WORKFLOW_SHA'), 'workflow/source identity')
    require(not git('status', '--porcelain', '--untracked-files=normal'), 'unclean source')
    repository = os.environ.get('GITHUB_REPOSITORY', '')
    require(re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository), 'repository identity')
    run, attempt = os.environ.get('GITHUB_RUN_ID', ''), os.environ.get('GITHUB_RUN_ATTEMPT', '')
    require(re.fullmatch(r'[1-9][0-9]{0,18}', run) and re.fullmatch(r'[1-9][0-9]{0,8}', attempt), 'run identity')
    ids, metadata = planner.discover_inventory(root)
    require(set(SMOKE_IDS) <= set(ids), 'missing integration witness')
    pure = [identity for identity in ids if not metadata[identity]['process'] and not metadata[identity]['graphical']]
    require(pure and all(metadata[identity]['process'] and not metadata[identity]['graphical'] for identity in SMOKE_IDS),
            'incorrect integration witness classification')
    return {'repository': repository, 'source_sha': source, 'source_tree': git('rev-parse', 'HEAD^{tree}'),
            'parents': git('rev-list', '--parents', '-n', '1', 'HEAD').split()[1:],
            'workflow_blob': git('rev-parse', f'HEAD:{WORKFLOW}'), 'policy_version': planner.POLICY_VERSION,
            'inventory_sha256': planner.digest({'ids': ids, 'metadata': metadata}),
            'smoke_ids': sorted([*pure, *SMOKE_IDS]), 'run_id': run, 'attempt': attempt}


def seal(context_value, event, needs):
    """Only trusted successful full gates and actual tested compilers can seal."""
    require(os.environ.get('GITHUB_EVENT_NAME') == 'pull_request', 'only full PR coverage may be sealed')
    require(set(needs) == FULL_GROUPS and all(row.get('result') == 'success' for row in needs.values()),
            'full prerequisite groups did not all succeed')
    require(needs['scope']['outputs']['mode'] == 'full' and needs['scope']['outputs']['docs_only'] == 'false',
            'only exhaustive coverage may be sealed')
    pr = event['pull_request']
    require(pr['base']['repo']['full_name'] == pr['head']['repo']['full_name'] == context_value['repository']
            and pr['base']['ref'] == 'main', 'foreign or unsupported PR')
    require(context_value['parents'] == [pr['base']['sha'], pr['head']['sha']], 'tested synthetic merge parents')
    fingerprints = {}
    for group, roles in (('profiles', PROFILE_ROLES), ('windows_binaries', ('producer_debug', 'producer_release')),
                         ('selected_consumer', ('consumer',))):
        outputs = needs[group]['outputs']
        expected = {'toolchain_' + role for role in roles}
        if group == 'profiles':
            from ci_python_shards import linux_debug_cohort
            linux_debug_cohort(needs[group], context_value['attempt'])
            expected |= {'linux_debug_shard_1', 'linux_debug_shard_2'}
        if group == 'windows_binaries':
            expected |= {'producer_debug', 'producer_release'}
        require(set(outputs) == expected, 'missing or unexpected tested compiler role')
        for role in roles:
            fingerprints[role] = toolchain(outputs['toolchain_' + role])
    require(len({planner.digest(value) for value in fingerprints.values()}) == 1, 'tested toolchains differ')
    return {'version': VERSION, **context_value, 'pr_number': event['number'],
            'toolchains': fingerprints, 'assurance': 'fresh-full-pr',
            'groups': {key: needs[key]['result'] for key in sorted(FULL_GROUPS)}}


def bounded_capture(command, *, timeout):
    """Bound both pipes while reading, not after communicate has allocated them."""
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    lock = threading.Lock()
    chunks = [[], []]
    size, failures = [0], []
    def drain(index, stream):
        try:
            while block := stream.read(65536):
                with lock:
                    size[0] += len(block)
                    if size[0] > MAX_BYTES:
                        failures.append('API response size bound')
                        process.kill()
                        return
                    chunks[index].append(block)
        except OSError:
            failures.append('API pipe read failed')
    readers = [threading.Thread(target=drain, args=(index, stream), daemon=True)
               for index, stream in enumerate((process.stdout, process.stderr))]
    for reader in readers:
        reader.start()
    try:
        code = process.wait(timeout=timeout)
    except BaseException:
        process.kill()
        process.wait(timeout=5)
        raise
    finally:
        for reader in readers:
            reader.join(timeout=5)
        for reader, stream in zip(readers, (process.stdout, process.stderr)):
            if not reader.is_alive():
                stream.close()
    require(not failures and not any(reader.is_alive() for reader in readers),
            failures[0] if failures else 'API reader did not finish')
    if code:
        raise subprocess.CalledProcessError(code, command)
    return b''.join(chunks[0])


class Api:
    """Bounded GitHub CLI transport; a larger history is a normal full fallback."""
    def __init__(self, repository):
        require(re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository), 'repository identity')
        self.prefix = '/repos/' + repository
        self.deadline = time.monotonic() + 75
        self.calls = 0

    def raw(self, path):
        require(path.startswith('/') and '..' not in path and '\n' not in path, 'API path')
        self.calls += 1
        remaining = self.deadline - time.monotonic()
        require(self.calls <= 10 and remaining > 0, 'API request budget')
        return bounded_capture(['gh', 'api', '--method', 'GET', self.prefix + path], timeout=min(20, remaining))

    def get(self, path):
        return decode(self.raw(path))


def complete(value, key):
    require(isinstance(value, dict), 'API listing is not an object')
    count, rows = value.get('total_count'), value.get(key)
    require(type(count) is int and 0 <= count <= 100 and isinstance(rows, list) and len(rows) == count,
            'incomplete or oversized API listing')
    return rows


def job_roles():
    success = {'Change scope and documentation', 'Repository tooling lint', 'Dependencies (bans licenses sources)',
               '3D compatibility (linux-3d-consumer)', 'Rust (windows-latest)', 'Rust (ubuntu-latest)',
               'Full PR qualification'}
    for system in ('ubuntu-latest', 'windows-latest'):
        for profile in ('debug', 'release'):
            success.add(f'Rust ({system}, {profile}, rust)')
            if system == 'ubuntu-latest' and profile == 'release':
                success.add(f'Rust ({system}, {profile}, python)')
            else:
                if system == 'windows-latest':
                    success.add(f'Full applications ({system}, {profile})')
                for shard in (1, 2):
                    success.add(f'Rust ({system}, {profile}, python shard {shard})')
    skipped = {'Selected (${{ needs.scope.outputs.build_lane }})', 'Selected (${{ needs.scope.outputs.linux_lane }})', 'Selected (${{ needs.scope.outputs.windows_lane }})',
               'Selected (linux-release-python)', 'Post-merge integration'}
    return success, skipped


def validate_jobs(jobs, run):
    require(all(isinstance(row, dict) for row in jobs), 'malformed job metadata')
    success, skipped = job_roles()
    advisory = 'Dependencies (advisories)'
    names = [row.get('name') for row in jobs]
    require(len(names) == len(set(names)) and set(names) == success | skipped | {advisory}, 'canonical full job roles')
    for row in jobs:
        require(row.get('run_id') == run['id'] and row.get('head_sha') == run['head_sha']
                and type(row.get('run_attempt')) is int and 1 <= row['run_attempt'] <= run['run_attempt']
                and row.get('status') == 'completed', 'foreign or unfinished job')
        allowed = {'skipped'} if row['name'] in skipped else {'success'}
        if row['name'] == advisory:
            allowed = {'success', 'failure'}  # Fresh main advisory enforcement remains blocking.
        require(row.get('conclusion') in allowed, 'canonical required job did not succeed')


def timestamp(value):
    require(isinstance(value, str), 'missing timestamp')
    parsed = datetime.fromisoformat(value.replace('Z', '+00:00'))
    require(parsed.tzinfo is not None, 'timestamp must have an offset')
    return parsed.timestamp()


def fetch_candidate(current, event, api, *, now=None):
    """Resolve exactly one merged PR and its newest canonical CI run; never search back."""
    now = time.time() if now is None else now
    require(event.get('ref') == 'refs/heads/main' and event.get('forced') is False
            and event.get('created') is False and event.get('deleted') is False
            and event.get('after') == current['source_sha'] and len(current['parents']) == 2
            and current['parents'][0] == event.get('before'), 'not a single ordinary main merge')
    prs = api.get('/commits/' + current['source_sha'] + '/pulls?per_page=100')
    require(isinstance(prs, list) and len(prs) == 1, 'ambiguous merged PR')
    pr = prs[0]
    require(isinstance(pr, dict) and type(pr.get('number')) is int and pr['number'] > 0, 'malformed PR metadata')
    require(pr.get('merged_at') and pr.get('state') == 'closed' and pr.get('merge_commit_sha') == current['source_sha']
            and pr['head']['repo']['full_name'] == pr['base']['repo']['full_name'] == current['repository']
            and pr['base']['ref'] == 'main' and pr['head']['sha'] == current['parents'][1], 'foreign/unmerged PR')
    runs = complete(api.get('/actions/workflows/ci.yml/runs?event=pull_request&head_sha='
                            + pr['head']['sha'] + '&per_page=100'), 'workflow_runs')
    require(runs and all(isinstance(row, dict) and type(row.get('run_number')) is int for row in runs),
            'missing canonical PR run')
    numbers = [row['run_number'] for row in runs]
    require(len(numbers) == len(set(numbers)), 'ambiguous newest run')
    run = max(runs, key=lambda row: row['run_number'])
    require(type(run.get('id')) is int and type(run.get('run_attempt')) is int and run['run_attempt'] > 0,
            'invalid qualification run')
    require(run.get('event') == 'pull_request' and run.get('path') == WORKFLOW
            and run.get('head_sha') == pr['head']['sha'] and run['head_repository']['full_name'] == current['repository']
            and run.get('status') == 'completed' and run.get('conclusion') == 'success', 'newest run not successful canonical PR')
    finished = timestamp(run['updated_at'])
    require(0 <= now - finished <= MAX_AGE and finished <= timestamp(pr['merged_at']) <= now, 'stale or post-merge qualification')
    validate_jobs(complete(api.get(f'/actions/runs/{run["id"]}/jobs?filter=latest&per_page=100'), 'jobs'), run)
    artifacts = complete(api.get(f'/actions/runs/{run["id"]}/artifacts?per_page=100'), 'artifacts')
    require(all(isinstance(row, dict) for row in artifacts), 'malformed artifact metadata')
    name = f'full-pr-qualification-attempt-{run["run_attempt"]}'
    candidates = [row for row in artifacts if row.get('name') == name]
    require(len(candidates) == 1, 'missing or duplicate qualification artifact')
    artifact = candidates[0]
    require(artifact.get('expired') is False and type(artifact.get('id')) is int
            and type(artifact.get('size_in_bytes')) is int and 0 < artifact['size_in_bytes'] <= MAX_BYTES,
            'expired/oversized qualification artifact')
    require(artifact['workflow_run']['id'] == run['id'] and artifact['workflow_run']['head_sha'] == run['head_sha'],
            'foreign qualification artifact')
    raw = api.raw(f'/actions/artifacts/{artifact["id"]}/zip')
    import hashlib
    require(artifact.get('digest') == 'sha256:' + hashlib.sha256(raw).hexdigest(), 'artifact digest mismatch')
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        members = archive.infolist()
        require(len(members) == 1 and members[0].filename == 'qualification.json'
                and members[0].file_size <= MAX_BYTES and not members[0].is_dir()
                and not members[0].flag_bits & 1
                and members[0].compress_type in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED)
                and (members[0].external_attr >> 16) & 0o170000 != 0o120000, 'unsafe qualification archive')
        try:
            qualified = decode(archive.read(members[0]))
        except zlib.error as error:
            raise ValueError('invalid qualification compression') from error
    require(isinstance(qualified, dict), 'qualification is not an object')
    require(type(qualified.get('version')) is int and qualified['version'] == VERSION and qualified.get('assurance') == 'fresh-full-pr'
            and qualified.get('run_id') == str(run['id']) and qualified.get('attempt') == str(run['run_attempt'])
            and type(qualified.get('pr_number')) is int and qualified['pr_number'] == pr['number'], 'qualification origin')
    require(qualified.get('groups') == dict.fromkeys(FULL_GROUPS, 'success'), 'incomplete qualification gates')
    for key in ('repository', 'source_tree', 'parents', 'workflow_blob', 'policy_version', 'inventory_sha256', 'smoke_ids'):
        require(qualified.get(key) == current[key], 'qualification differs: ' + key)
    source = qualified.get('source_sha')
    require(isinstance(source, str) and planner.SHA.fullmatch(source), 'tested merge identity')
    tested = api.get('/git/commits/' + source)
    require(isinstance(tested, dict), 'tested Git object is not an object')
    require(tested.get('sha') == source and tested['tree']['sha'] == current['source_tree']
            and [row['sha'] for row in tested['parents']] == current['parents'], 'tested Git tree/parents differ')
    identities = qualified.get('toolchains', {})
    require(isinstance(identities, dict) and set(identities) == set(TOOLCHAIN_ROLES), 'missing tested compiler roles')
    normalized = [toolchain(value) for value in identities.values()]
    require(len({planner.digest(value) for value in normalized}) == 1, 'tested compiler identities disagree')
    return {'qualification': qualified, 'artifact_id': artifact['id'], 'artifact_digest': artifact['digest'],
            'qualified_at': run['updated_at'], 'main': current}


def resolve(normal, event, root=ROOT):
    """Any unavailable authority retains the normal full plan, without a bypass."""
    try:
        require(normal['mode'] == 'full' and os.environ.get('GITHUB_EVENT_NAME') == 'push', 'not eligible')
        current = context(root)
        proof = fetch_candidate(current, event, Api(current['repository']))
        # Resolve stable only after a complete eligible proof exists. PRs and
        # ordinary selected/prose main pushes never pay this setup cost.
        subprocess.run(['rustup', 'toolchain', 'install', 'stable', '--profile', 'minimal'],
                       check=True, timeout=120, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        actual = toolchain()
        require(actual == proof['qualification']['toolchains']['consumer'], 'current compiler changed')
        proof['current_toolchain'] = actual
        return {**normal, 'mode': 'postmerge', 'assurance': 'prior-exact-tree-full-plus-fresh-main',
                'reasons': ['exact merged tree has canonical full PR qualification'], 'consumer_3d': False,
                'postmerge_proof': proof}
    except (OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        normal['postmerge_optimization'] = {'status': 'declined', 'reason': str(error)[:200]}
        return normal


def validate_local(plan, root=ROOT):
    require(plan.get('mode') == 'postmerge' and os.environ.get('GITHUB_EVENT_NAME') == 'push', 'not a postmerge run')
    require(plan.get('assurance') == 'prior-exact-tree-full-plus-fresh-main'
            and plan.get('consumer_3d') is False and plan.get('lanes') == [], 'incorrect retained assurance')
    require(planner.normal_plan(root=root)['mode'] == 'full', 'normal plan is not full')
    proof = plan['postmerge_proof']
    require(proof['main'] == context(root), 'proof differs from current source/run/inventory')
    require(plan['source_sha'] == proof['main']['source_sha'] and plan['source_tree'] == proof['main']['source_tree'],
            'plan differs from proof')
    require(0 <= time.time() - timestamp(proof['qualified_at']) <= MAX_AGE, 'stale retained qualification')
    require(toolchain(proof['current_toolchain']) == proof['qualification']['toolchains']['consumer'], 'compiler proof mismatch')
    qualified = proof['qualification']
    require(type(qualified.get('version')) is int and qualified['version'] == VERSION and qualified.get('assurance') == 'fresh-full-pr'
            and qualified.get('groups') == dict.fromkeys(FULL_GROUPS, 'success'), 'incomplete full qualification')
    for key in ('repository', 'source_tree', 'parents', 'workflow_blob', 'policy_version', 'inventory_sha256', 'smoke_ids'):
        require(qualified.get(key) == proof['main'][key], 'retained qualification differs: ' + key)
    require(set(qualified['toolchains']) == set(TOOLCHAIN_ROLES)
            and all(toolchain(value) == proof['current_toolchain'] for value in qualified['toolchains'].values()),
            'missing or different tested compiler')
    return proof


def smoke_steps(workers):
    return [
        ('build-applications', ['cargo', 'build', '--workspace', '--bins', '--locked']),
        ('platform-codecs', ['cargo', 'test', '-p', 'tor-protocol', '-p', 'tor-client-common', '--locked',
                             '--features', 'tor-client-common/test-support']),
        ('python-postmerge', ['<python>', 'scripts/test_runner.py', '--workers', str(workers),
                              '--from-file', '.local/postmerge/selected-ids.txt', '-v']),
    ]


def smoke(plan):
    import ci_selected
    import verify
    proof = validate_local(plan)
    require(sys.platform.startswith('linux'), 'postmerge integration requires Linux')
    actual = toolchain()
    require(actual == proof['current_toolchain'], 'smoke compiler differs from qualified/current compiler')
    require(not verify.running_builds(), 'another Cargo target owner is active')
    logs = ROOT / '.local/postmerge'
    logs.mkdir(parents=True, exist_ok=False)
    ids = proof['main']['smoke_ids']
    (logs / 'selected-ids.txt').write_text('\n'.join(ids) + '\n', encoding='utf-8')
    workers = verify.test_workers(verify.free_memory_gb(), os.cpu_count())
    jobs = verify.jobs_for_memory(verify.free_memory_gb(), os.cpu_count())
    target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')).resolve() / 'debug'
    environment = {'TOR_TEST_PROFILE': 'debug', 'TOR_TEST_BINARIES': str(target),
                   'TOR_SCENARIO_COMPILER': str(target / 'tor-scenario')}
    results = []
    with verify.Lock(ROOT / '.local/verify.lock'):
        for name, recipe in smoke_steps(workers):
            command = [sys.executable if arg == '<python>' else arg for arg in recipe]
            result = verify.run_step(name, command, dict(environment), logs, jobs, False)
            results.append({'step': name, 'recipe': recipe, 'code': result['code']})
            require(result['code'] == 0, 'fresh postmerge stage failed: ' + name)
    rows = [decode(line) for line in (logs / 'python-postmerge-tests.jsonl').read_text().splitlines()]
    ci_selected.validate_python(rows, ids, 'debug')
    require(context() == proof['main'], 'source/run/inventory changed during postmerge integration')
    receipt = {'version': VERSION, 'proof_sha256': planner.digest(proof), 'main': proof['main'],
               'toolchain': actual, 'workers': workers, 'steps': results}
    (logs / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')


def validate_smoke(plan, directory):
    import ci_selected
    proof = validate_local(plan)
    receipt = decode((directory / 'receipt.json').read_bytes())
    require(type(receipt.get('version')) is int and receipt['version'] == VERSION and receipt.get('proof_sha256') == planner.digest(proof)
            and receipt.get('main') == proof['main'] and receipt.get('toolchain') == proof['current_toolchain'],
            'foreign smoke receipt')
    workers = receipt.get('workers')
    require(type(workers) is int and 1 <= workers <= 256, 'invalid worker count')
    require(isinstance(receipt.get('steps'), list)
            and all(isinstance(row, dict) and type(row.get('code')) is int for row in receipt['steps']), 'invalid stage status type')
    require(receipt.get('steps') == [{'step': name, 'recipe': command, 'code': 0}
                                    for name, command in smoke_steps(workers)], 'missing/extra/failed smoke stage')
    rows = [decode(line) for line in (directory / 'python-postmerge-tests.jsonl').read_text().splitlines()]
    ci_selected.validate_python(rows, proof['main']['smoke_ids'], 'debug')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    record = sub.add_parser('toolchain')
    record.add_argument('--role', required=True, choices=TOOLCHAIN_ROLES)
    sub.add_parser('seal')
    for name in ('smoke', 'aggregate'):
        item = sub.add_parser(name)
        item.add_argument('--plan', required=True, type=Path)
        if name == 'aggregate':
            item.add_argument('--directory', required=True, type=Path)
    args = parser.parse_args(argv)
    if args.command == 'toolchain':
        with Path(os.environ['GITHUB_OUTPUT']).open('a', encoding='utf-8') as handle:
            handle.write('toolchain_' + args.role + '=' + json.dumps(toolchain(), separators=(',', ':')) + '\n')
    elif args.command == 'seal':
        result = seal(context(), decode(Path(os.environ['GITHUB_EVENT_PATH']).read_bytes()), decode(os.environ['QUALIFICATION_NEEDS']))
        destination = ROOT / '.local/full-qualification'
        destination.mkdir(parents=True, exist_ok=False)
        (destination / 'qualification.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    else:
        plan = decode(args.plan.read_bytes())
        if args.command == 'smoke':
            smoke(plan)
        else:
            validate_smoke(plan, args.directory)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
