"""Exact-tree retained qualification and fresh-main boundary contracts."""
from pathlib import Path
from copy import deepcopy
from datetime import datetime, timezone
import hashlib
import io
import json
import os
import re
import subprocess
import tempfile
import sys
from types import SimpleNamespace
from unittest.mock import patch
import zipfile

import ci_postmerge as postmerge
import ci_test_plan as planner
import unittest


ROOT = Path(__file__).resolve().parent.parent


class PostmergeTests(unittest.TestCase):
    def test_existing_full_gate_retains_both_platforms_and_blocking_main_advisory(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertIn('os: [windows-latest, ubuntu-latest]', workflow)
        self.assertIn('profile: [debug, release]', workflow)
        self.assertIn("matrix.checks == 'advisories' && github.event_name == 'pull_request'", workflow)
        self.assertIn("name: Rust (${{ matrix.os }})", workflow)

    def test_qualified_full_pr_has_separate_success_only_seal_not_self_dependency(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertIn('\n  full_qualification:\n', workflow,
                      'Complete full PR coverage needs a bounded exact-tree qualification seal')
        block = workflow.split('\n  full_qualification:\n', 1)[1].split('\n  check:\n', 1)[0]
        self.assertIn('check', block)
        self.assertIn("needs.check.result == 'success'", block)
        self.assertNotIn('needs: [full_qualification', block)

    def fixture(self):
        self.now = 1800000000
        self.compiler = {'release': '1.99.1', 'commit': '1' * 40, 'cargo': 'cargo 1.99.1 (abc 2026-01-01)'}
        self.current = {'repository': 'owner/repo', 'source_sha': 'c' * 40, 'source_tree': 'd' * 40,
                        'parents': ['a' * 40, 'b' * 40], 'workflow_blob': 'f' * 40,
                        'policy_version': planner.POLICY_VERSION, 'inventory_sha256': '9' * 64,
                        'smoke_ids': sorted(['fixture.Pure.test_a', *postmerge.SMOKE_IDS]),
                        'run_id': '900', 'attempt': '1'}
        self.event = {'before': 'a' * 40, 'after': 'c' * 40, 'ref': 'refs/heads/main',
                      'created': False, 'deleted': False, 'forced': False}
        self.qualified = {**deepcopy(self.current), 'source_sha': 'e' * 40, 'run_id': '123', 'attempt': '1',
                          'version': 1, 'assurance': 'fresh-full-pr', 'pr_number': 7,
                          'groups': dict.fromkeys(postmerge.FULL_GROUPS, 'success'),
                          'toolchains': {role: dict(self.compiler) for role in postmerge.TOOLCHAIN_ROLES}}
        stamp = lambda seconds: datetime.fromtimestamp(seconds, timezone.utc).isoformat()
        self.pr = {'number': 7, 'merged_at': stamp(self.now - 50), 'state': 'closed',
                   'merge_commit_sha': self.current['source_sha'],
                   # The live base tip is allowed to have moved since testing.
                   'base': {'sha': '0' * 40, 'ref': 'main', 'repo': {'full_name': 'owner/repo'}},
                   'head': {'sha': 'b' * 40, 'repo': {'full_name': 'owner/repo'}}}
        self.run = {'id': 123, 'run_number': 99, 'run_attempt': 1, 'event': 'pull_request',
                    'path': postmerge.WORKFLOW, 'head_sha': 'b' * 40,
                    'head_repository': {'full_name': 'owner/repo'}, 'status': 'completed',
                    'conclusion': 'success', 'updated_at': stamp(self.now - 100)}
        successful, skipped = postmerge.job_roles()
        self.jobs = [{'name': name, 'run_id': 123, 'head_sha': 'b' * 40, 'run_attempt': 1,
                      'status': 'completed', 'conclusion': 'skipped' if name in skipped else 'success'}
                     for name in sorted(successful | skipped | {'Dependencies (advisories)'})]
        self.extra_runs = []
        self.calls = []
        self.overrides = {}

    def responses(self):
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, 'w') as archive:
            archive.writestr('qualification.json', json.dumps(self.qualified))
        raw = buffer.getvalue()
        artifact = {'id': 321, 'name': 'full-pr-qualification-attempt-1', 'expired': False,
                    'size_in_bytes': len(raw), 'digest': 'sha256:' + hashlib.sha256(raw).hexdigest(),
                    'workflow_run': {'id': 123, 'head_sha': 'b' * 40}}
        responses = {
            '/commits/' + 'c' * 40 + '/pulls?per_page=100': [self.pr],
            '/actions/workflows/ci.yml/runs?event=pull_request&head_sha=' + 'b' * 40 + '&per_page=100':
                {'total_count': 1 + len(self.extra_runs), 'workflow_runs': [self.run, *self.extra_runs]},
            '/actions/runs/123/jobs?filter=latest&per_page=100': {'total_count': len(self.jobs), 'jobs': self.jobs},
            '/actions/runs/123/artifacts?per_page=100': {'total_count': 1, 'artifacts': [artifact]},
            '/actions/artifacts/321/zip': raw,
            '/git/commits/' + 'e' * 40: {'sha': 'e' * 40, 'tree': {'sha': 'd' * 40},
                                       'parents': [{'sha': value} for value in self.current['parents']]},
        }
        responses.update(self.overrides)
        return responses

    def transport(self, command, **kwargs):
        self.calls.append(command)
        if command[:2] == ['rustup', 'toolchain']:
            self.assertEqual(command, ['rustup', 'toolchain', 'install', 'stable', '--profile', 'minimal'])
            return SimpleNamespace(returncode=0)
        self.assertEqual(command[:4], ['gh', 'api', '--method', 'GET'])
        self.assertLessEqual(kwargs['timeout'], 20)
        self.assertTrue(command[4].startswith('/repos/owner/repo/'))
        value = self.responses()[command[4][len('/repos/owner/repo'):]]
        return SimpleNamespace(stdout=value if isinstance(value, bytes) else json.dumps(value).encode(), stderr=b'')

    def fetch(self):
        with patch.object(postmerge, 'bounded_capture', side_effect=lambda command, **kw: self.transport(command, **kw).stdout):
            return postmerge.fetch_candidate(self.current, self.event, postmerge.Api('owner/repo'), now=self.now)

    def test_actual_fetch_boundary_accepts_exact_tree_not_mutable_base_tip(self):
        self.fixture()
        proof = self.fetch()
        self.assertEqual(proof['qualification']['source_sha'], 'e' * 40)
        self.assertEqual(proof['main']['source_sha'], 'c' * 40)
        self.assertEqual(proof['qualification']['source_tree'], proof['main']['source_tree'])
        self.assertEqual(len(self.calls), 6)
        self.assertTrue(all(call[:4] == ['gh', 'api', '--method', 'GET'] for call in self.calls))

    def test_newer_failure_never_falls_back_to_older_success(self):
        for status in ('failure', 'cancelled', None):
            self.fixture()
            self.assertTrue(self.fetch())
            self.extra_runs = [dict(self.run, id=124, run_number=100, conclusion=status)]
            self.calls.clear()
            with self.assertRaisesRegex(ValueError, 'newest run'):
                self.fetch()
            self.assertEqual(len(self.calls), 2)

    def test_merge_repository_tree_inventory_policy_and_workflow_are_exact(self):
        for key, value in [('source_tree', '0' * 40), ('parents', ['x', 'y']), ('repository', 'foreign/repo'),
                           ('workflow_blob', '0' * 40), ('inventory_sha256', '0' * 64), ('policy_version', -1),
                           ('smoke_ids', []), ('assurance', 'selected-contract'), ('attempt', '2'), ('pr_number', 8), ('version', True)]:
            with self.subTest(key=key):
                self.fixture()
                self.assertTrue(self.fetch())
                self.qualified[key] = value
                with self.assertRaises(ValueError):
                    self.fetch()

    def test_tested_git_object_must_confirm_sealed_tree_and_both_parents(self):
        for key, value in [('sha', '0' * 40), ('tree', {'sha': '0' * 40}), ('parents', [{'sha': 'b' * 40}])]:
            self.fixture()
            self.assertTrue(self.fetch())
            endpoint = '/git/commits/' + 'e' * 40
            changed = deepcopy(self.responses()[endpoint])
            changed[key] = value
            self.overrides[endpoint] = changed
            with self.assertRaisesRegex(ValueError, 'Git tree/parents'):
                self.fetch()

    def test_foreign_nonmerge_forced_or_batch_push_cannot_qualify(self):
        for mutation in ('foreign', 'open', 'wrong_merge', 'forced', 'before', 'parents'):
            self.fixture()
            self.assertTrue(self.fetch())
            if mutation == 'foreign': self.pr['head']['repo']['full_name'] = 'fork/repo'
            elif mutation == 'open': self.pr['state'] = 'open'
            elif mutation == 'wrong_merge': self.pr['merge_commit_sha'] = '0' * 40
            elif mutation == 'forced': self.event['forced'] = True
            elif mutation == 'before': self.event['before'] = '0' * 40
            else: self.current['parents'].append('0' * 40)
            with self.assertRaises(ValueError): self.fetch()

    def test_complete_latest_job_roles_reject_missing_duplicate_failed_or_pending(self):
        for mutation in ('missing', 'duplicate', 'failure', 'cancelled', 'pending', 'foreign'):
            self.fixture()
            self.assertTrue(self.fetch())
            role = next(row for row in self.jobs if row['name'] == 'Rust (windows-latest)')
            if mutation == 'missing': self.jobs.remove(role)
            elif mutation == 'duplicate': self.jobs.append(dict(role))
            elif mutation == 'pending': role['status'] = 'in_progress'
            elif mutation == 'foreign': role['run_id'] = 999
            else: role['conclusion'] = mutation
            with self.assertRaises(ValueError): self.fetch()

    def test_advisory_pr_failure_is_not_relabelled_as_main_success(self):
        self.fixture()
        next(row for row in self.jobs if row['name'] == 'Dependencies (advisories)')['conclusion'] = 'failure'
        self.assertTrue(self.fetch())
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertIn("dependencies == 'success' and all_skipped", workflow)
        self.assertIn("matrix.checks == 'advisories' && github.event_name == 'pull_request'", workflow)

    def test_missing_expired_duplicate_tampered_or_large_artifact_rejects(self):
        for mutation in ('missing', 'expired', 'duplicate', 'digest', 'foreign', 'size'):
            self.fixture()
            self.assertTrue(self.fetch())
            endpoint = '/actions/runs/123/artifacts?per_page=100'
            body = self.responses()[endpoint]
            row = body['artifacts'][0]
            if mutation == 'missing': body.update(total_count=0, artifacts=[])
            elif mutation == 'duplicate': body.update(total_count=2, artifacts=[row, row])
            elif mutation == 'expired': row['expired'] = True
            elif mutation == 'digest': row['digest'] = 'sha256:' + '0' * 64
            elif mutation == 'foreign': row['workflow_run']['id'] = 999
            else: row['size_in_bytes'] = postmerge.MAX_BYTES + 1
            self.overrides[endpoint] = body
            with self.assertRaises(ValueError): self.fetch()

    def test_incomplete_api_listing_or_stale_proof_has_no_history_fallback(self):
        for mutation in ('listing', 'stale', 'future', 'after_merge'):
            self.fixture()
            self.assertTrue(self.fetch())
            if mutation == 'listing':
                endpoint = '/actions/runs/123/jobs?filter=latest&per_page=100'
                self.overrides[endpoint] = {'total_count': 101, 'jobs': self.jobs}
            else:
                seconds = self.now - postmerge.MAX_AGE - 1 if mutation == 'stale' else self.now + 1
                if mutation == 'after_merge': seconds = self.now - 25
                self.run['updated_at'] = datetime.fromtimestamp(seconds, timezone.utc).isoformat()
            with self.assertRaises(ValueError): self.fetch()

    def test_tested_compiler_requires_all_distinct_roles_and_one_identity(self):
        for mutation in ('missing', 'extra', 'drift', 'invalid'):
            self.fixture()
            self.assertTrue(self.fetch())
            identities = self.qualified['toolchains']
            if mutation == 'missing': del identities['producer_debug']
            elif mutation == 'extra': identities['unowned'] = dict(self.compiler)
            elif mutation == 'drift': identities['producer_debug']['release'] = '1.99.2'
            else: identities['producer_debug']['commit'] = 'not-a-commit'
            with self.assertRaises(ValueError): self.fetch()

    def test_real_planner_and_fetch_select_lean_only_after_complete_proof_and_current_compiler(self):
        self.fixture()
        normal = planner.full_plan(self.current['source_sha'], self.current['source_tree'], ['controls'])
        with patch.dict(os.environ, {'GITHUB_EVENT_NAME': 'push'}), \
                patch.object(planner, 'normal_plan', return_value=normal), \
                patch.object(planner.ci_scope, 'classify', return_value={'docs_only': False}), \
                patch.object(postmerge, 'context', return_value=self.current), \
                patch.object(postmerge.time, 'time', return_value=self.now), \
                patch.object(postmerge, 'bounded_capture', side_effect=lambda command, **kw: self.transport(command, **kw).stdout), \
                patch.object(postmerge.subprocess, 'run', side_effect=self.transport), \
                patch.object(postmerge, 'toolchain', wraps=postmerge.toolchain) as identity:
            # The only live probe boundary returns the real-command shaped identity.
            with patch.object(postmerge.subprocess, 'check_output', side_effect=[
                    'release: 1.99.1\ncommit-hash: ' + '1' * 40 + '\n', self.compiler['cargo'] + '\n']):
                result = planner.current_plan(event_name='push', event=self.event)
            self.assertEqual(result['mode'], 'postmerge')
            self.assertEqual(result['assurance'], 'prior-exact-tree-full-plus-fresh-main')
            self.assertGreater(identity.call_count, 9)
        self.assertEqual(self.calls[-1][:2], ['rustup', 'toolchain'])

    def test_normal_selected_prose_pr_and_schedule_never_probe_compiler_or_proof(self):
        for event, mode, prose in [('push', 'selected', False), ('push', 'full', True),
                                    ('pull_request', 'full', False), ('schedule', 'full', False)]:
            normal = planner.full_plan('a' * 40, 'b' * 40, ['normal'])
            normal['mode'] = mode
            with patch.object(planner, 'normal_plan', return_value=normal), \
                    patch.object(planner.ci_scope, 'classify', return_value={'docs_only': prose}), \
                    patch.object(postmerge, 'resolve') as resolve:
                self.assertIs(planner.current_plan(event_name=event, event={}), normal)
                resolve.assert_not_called()

    def test_candidate_transport_error_or_current_toolchain_drift_retains_full(self):
        for mutation in ('network', 'compiler'):
            self.fixture()
            normal = planner.full_plan('c' * 40, 'd' * 40, ['controls'])
            with patch.dict(os.environ, {'GITHUB_EVENT_NAME': 'push'}), \
                    patch.object(postmerge, 'context', return_value=self.current), \
                    patch.object(postmerge.time, 'time', return_value=self.now), \
                    patch.object(postmerge, 'bounded_capture', side_effect=(
                        subprocess.TimeoutExpired('gh', 20) if mutation == 'network' else lambda command, **kw: self.transport(command, **kw).stdout)), \
                    patch.object(postmerge.subprocess, 'run', side_effect=self.transport), \
                    patch.object(postmerge.subprocess, 'check_output', side_effect=[
                        'release: 1.99.2\ncommit-hash: ' + '1' * 40 + '\n', self.compiler['cargo'] + '\n']):
                self.assertIs(postmerge.resolve(normal, self.event), normal)

    def test_strict_json_and_request_budget(self):
        for raw in ('{"a":1,"a":2}', '{"a":NaN}', 'x' * (postmerge.MAX_BYTES + 1)):
            with self.assertRaises(ValueError): postmerge.decode(raw)
        api = postmerge.Api('owner/repo')
        api.calls = 10
        with patch.object(postmerge, 'bounded_capture') as run, self.assertRaisesRegex(ValueError, 'budget'):
            api.get('/actions/runs/123')
        run.assert_not_called()

    def test_workflow_uses_scope_resolved_compiler_and_explicit_postmerge_gate(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertIn('RUSTUP_TOOLCHAIN: ${{ needs.scope.outputs.postmerge_release }}', workflow)
        self.assertIn('python scripts/ci_postmerge.py smoke --plan', workflow)
        self.assertIn('python scripts/ci_postmerge.py aggregate --plan', workflow)
        self.assertIn("os.environ['POSTMERGE_RESULT'] == 'success'", workflow)
        for role in postmerge.TOOLCHAIN_ROLES:
            self.assertIn('toolchain_' + role + ': ${{ steps.toolchain.outputs.toolchain_' + role + ' }}', workflow)

    def seal_inputs(self):
        self.fixture()
        current = {key: deepcopy(self.qualified[key]) for key in self.current}
        event = {'number': 7, 'pull_request': deepcopy(self.pr)}
        event['pull_request']['base']['sha'] = 'a' * 40
        needs = {key: {'result': 'success', 'outputs': {}} for key in postmerge.FULL_GROUPS}
        needs['scope']['outputs'] = {'mode': 'full', 'docs_only': 'false'}
        for group, roles in [('profiles', postmerge.PROFILE_ROLES),
                             ('windows_binaries', ('producer_debug', 'producer_release')),
                             ('selected_consumer', ('consumer',))]:
            needs[group]['outputs'] = {'toolchain_' + role: json.dumps(self.compiler) for role in roles}
        needs['windows_binaries']['outputs'].update(producer_debug='1', producer_release='1')
        return current, event, needs

    def test_seal_requires_full_pr_all_gates_and_all_unique_compiler_outputs(self):
        with patch.dict(os.environ, {'GITHUB_EVENT_NAME': 'pull_request'}):
            for mutation in ('mode', 'event', 'missing_role', 'extra_role', 'failed_group', 'missing_group', 'parents'):
                current, event, needs = self.seal_inputs()
                good = postmerge.seal(current, event, needs)
                self.assertEqual(good['assurance'], 'fresh-full-pr')
                self.assertEqual(set(good['toolchains']), set(postmerge.TOOLCHAIN_ROLES))
                if mutation == 'mode': needs['scope']['outputs']['mode'] = 'postmerge'
                elif mutation == 'event': event['pull_request']['head']['repo']['full_name'] = 'fork/repo'
                elif mutation == 'missing_role': del needs['profiles']['outputs']['toolchain_linux_debug_rust']
                elif mutation == 'extra_role': needs['profiles']['outputs']['unowned'] = 'x'
                elif mutation == 'failed_group': needs['check']['result'] = 'failure'
                elif mutation == 'missing_group': del needs['check']
                else: current['parents'].reverse()
                with self.assertRaises((ValueError, KeyError)): postmerge.seal(current, event, needs)
        current, event, needs = self.seal_inputs()
        with patch.dict(os.environ, {'GITHUB_EVENT_NAME': 'push'}), self.assertRaises(ValueError):
            postmerge.seal(current, event, needs)

    def test_compiler_metadata_does_not_relax_existing_windows_attempt_cohort(self):
        import ci_python_shards
        # The metadata is optional only for backward-compatible cohort parsing;
        # a full qualification seal independently requires every compiler role.
        self.fixture()
        producers = {'result': 'success', 'outputs': {'producer_debug': '1', 'producer_release': '1'}}
        shards = {'result': 'success', 'outputs': dict.fromkeys(
            ('shard_debug_1', 'shard_debug_2', 'shard_release_1', 'shard_release_2'), '1')}
        expected = ci_python_shards.cohort_attempt(producers, shards, '2')
        producers['outputs'].update({'toolchain_producer_' + profile: json.dumps(self.compiler)
                                     for profile in ('debug', 'release')})
        self.assertEqual(ci_python_shards.cohort_attempt(producers, shards, '2'), expected)
        for mutation in ('unknown', 'partial', 'bad_compiler', 'mixed_attempt'):
            candidate = deepcopy(producers)
            if mutation == 'unknown': candidate['outputs']['unowned'] = '1'
            elif mutation == 'partial': del candidate['outputs']['toolchain_producer_debug']
            elif mutation == 'bad_compiler': candidate['outputs']['toolchain_producer_debug'] = '{}'
            else: candidate['outputs']['producer_debug'] = '2'
            with self.assertRaises(ValueError): ci_python_shards.cohort_attempt(candidate, shards, '2')

    def receipt_fixture(self):
        self.fixture()
        proof = {'main': self.current, 'qualification': self.qualified, 'current_toolchain': self.compiler}
        receipt = {'version': 1, 'main': self.current, 'toolchain': self.compiler,
                   'proof_sha256': planner.digest(proof), 'workers': 2,
                   'steps': [{'step': name, 'recipe': command, 'code': 0}
                             for name, command in postmerge.smoke_steps(2)]}
        rows = [{'kind': 'selection', 'test_ids': self.current['smoke_ids']}]
        rows.extend({'kind': 'test', 'test_id': identity, 'outcome': 'passed', 'failures': 0,
                     'errors': 0, 'subtests_failed': 0, 'subtests_skipped': 0} for identity in self.current['smoke_ids'])
        rows.append({'kind': 'suite', 'successful': True, 'tests_run': len(self.current['smoke_ids']),
                     'failures': 0, 'errors': 0, 'skipped': 0, 'waived': 0, 'expected_failures': 0, 'unexpected_successes': 0})
        for row in rows: row.update(schema_version=1, profile='debug')
        return proof, receipt, rows

    def test_smoke_accounting_rejects_noop_missing_extra_failed_or_foreign_stages(self):
        for mutation in ('noop', 'missing', 'extra', 'command', 'failed', 'foreign', 'missing_test', 'skipped_test', 'bool_version', 'bool_code'):
            proof, receipt, rows = self.receipt_fixture()
            with tempfile.TemporaryDirectory() as temporary, patch.object(postmerge, 'validate_local', return_value=proof):
                directory = Path(temporary)
                def write():
                    (directory / 'receipt.json').write_text(json.dumps(receipt))
                    (directory / 'python-postmerge-tests.jsonl').write_text(''.join(json.dumps(row) + '\n' for row in rows))
                write()
                postmerge.validate_smoke({}, directory)
                if mutation == 'noop': receipt['steps'] = [{'step': 'noop', 'recipe': ['true'], 'code': 0}]
                elif mutation == 'missing': receipt['steps'].pop(1)
                elif mutation == 'extra': receipt['steps'].append(dict(receipt['steps'][0]))
                elif mutation == 'command': receipt['steps'][1]['recipe'].append('--lib')
                elif mutation == 'failed': receipt['steps'][0]['code'] = 1
                elif mutation == 'foreign': receipt['proof_sha256'] = '0' * 64
                elif mutation == 'missing_test': rows.pop(1)
                elif mutation == 'bool_version': receipt['version'] = True
                elif mutation == 'bool_code': receipt['steps'][0]['code'] = False
                else: rows[1]['outcome'] = 'skipped'
                write()
                with self.assertRaises(ValueError): postmerge.validate_smoke({}, directory)

    def test_actual_smoke_dispatch_checks_toolchain_before_build_and_runs_unfiltered_codecs(self):
        import verify
        proof, _, _ = self.receipt_fixture()
        class ReachedCodec(Exception): pass
        seen = []
        def execute(name, command, environment, logs, jobs, rerun):
            seen.append(name)
            if name == 'platform-codecs':
                self.assertEqual(command, ['cargo', 'test', '-p', 'tor-protocol', '-p', 'tor-client-common',
                                           '--locked', '--features', 'tor-client-common/test-support'])
                self.assertEqual(environment['TOR_TEST_PROFILE'], 'debug')
                raise ReachedCodec()
            self.assertEqual(name, 'build-applications')
            return {'code': 0}
        with tempfile.TemporaryDirectory() as temporary, \
                patch.object(postmerge, 'ROOT', Path(temporary)), \
                patch.object(postmerge, 'validate_local', return_value=proof), \
                patch.object(postmerge, 'toolchain', return_value=self.compiler), \
                patch.object(postmerge.sys, 'platform', 'linux'), \
                patch.object(verify, 'running_builds', return_value=[]), \
                patch.object(verify, 'run_step', side_effect=execute):
            with self.assertRaises(ReachedCodec): postmerge.smoke({})
        self.assertEqual(seen, ['build-applications', 'platform-codecs'])
        with patch.object(postmerge, 'validate_local', return_value=proof), \
                patch.object(postmerge.sys, 'platform', 'linux'), \
                patch.object(postmerge, 'toolchain', return_value=dict(self.compiler, release='1.99.2')), \
                patch.object(verify, 'run_step') as run, self.assertRaisesRegex(ValueError, 'smoke compiler'):
            postmerge.smoke({})
        run.assert_not_called()

    def test_actual_required_gate_accepts_only_complete_retained_and_fresh_roles(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        gate = workflow.split('      - name: Require the complete selected CI gate', 1)[1]
        match = re.search(r"python - <<'PY'\n(.*?)\n          PY", gate, re.S)
        program = '\n'.join(line[10:] for line in match[1].splitlines())
        valid = dict(DOCS_ONLY='false', SCOPE_RESULT='success', TOOLING_RESULT='success',
                     ASSURANCE_MODE='postmerge', PROFILES_RESULT='skipped', DEPENDENCIES_RESULT='success',
                     WINDOWS_PYTHON_RESULT='skipped', WINDOWS_BINARIES_RESULT='skipped',
                     SELECTED_RUST='skipped', SELECTED_LINUX='skipped', SELECTED_WINDOWS='skipped',
                     SELECTED_RELEASE='skipped', SELECTED_CONSUMER='skipped', POSTMERGE_RESULT='success',
                     CONSUMER_REQUIRED='false', APPLICATIONS_REQUIRED='false')
        variants = [(valid, True)]
        for key in ('SCOPE_RESULT', 'TOOLING_RESULT', 'DEPENDENCIES_RESULT', 'POSTMERGE_RESULT',
                    'PROFILES_RESULT', 'WINDOWS_PYTHON_RESULT', 'WINDOWS_BINARIES_RESULT',
                    'SELECTED_RUST', 'SELECTED_LINUX', 'SELECTED_WINDOWS', 'SELECTED_RELEASE', 'SELECTED_CONSUMER'):
            for value in ('success', 'failure', 'cancelled', 'skipped'):
                if value != valid[key]: variants.append(({**valid, key: value}, False))
        for environment, expected in variants:
            with self.subTest(environment=environment), patch.dict(os.environ, environment), self.assertRaises(SystemExit) as result:
                exec(compile(program, '<actual postmerge gate>', 'exec'), {})
            self.assertEqual(result.exception.code == 0, expected)

    def test_actual_profile_condition_skips_verified_postmerge_but_not_unknown_mode(self):
        from test_ci_scope import job_condition
        self.assertFalse(job_condition('profiles', 'success', 'false', assurance='postmerge'))
        self.assertTrue(job_condition('dependencies', 'success', 'false', assurance='postmerge'))
        self.assertTrue(job_condition('profiles', 'success', 'false', assurance='unknown'))

    def test_local_proof_revalidation_rejects_changed_source_run_and_compiler_roles(self):
        self.fixture()
        proof = self.fetch()
        proof['current_toolchain'] = self.compiler
        plan = {**planner.full_plan('c' * 40, 'd' * 40, []), 'mode': 'postmerge',
                'assurance': 'prior-exact-tree-full-plus-fresh-main', 'consumer_3d': False, 'postmerge_proof': proof}
        with patch.dict(os.environ, {'GITHUB_EVENT_NAME': 'push'}), \
                patch.object(postmerge, 'context', return_value=self.current), \
                patch.object(planner, 'normal_plan', return_value={'mode': 'full'}), \
                patch.object(postmerge.time, 'time', return_value=self.now):
            self.assertEqual(postmerge.validate_local(plan), proof)
            for mutation in ('run', 'tree', 'assurance', 'role', 'compiler', 'inventory'):
                candidate = deepcopy(plan)
                changed = candidate['postmerge_proof']
                if mutation == 'run': changed['main']['run_id'] = '901'
                elif mutation == 'tree': changed['qualification']['source_tree'] = '0' * 40
                elif mutation == 'assurance': candidate['assurance'] = 'fresh-full'
                elif mutation == 'role': del changed['qualification']['toolchains']['producer_debug']
                elif mutation == 'compiler': changed['qualification']['toolchains']['producer_debug']['release'] = '1.99.2'
                else: changed['qualification']['inventory_sha256'] = '0' * 64
                with self.assertRaises(ValueError): postmerge.validate_local(candidate)

    def test_compiler_cli_emits_only_its_exact_success_role(self):
        self.fixture()
        for role in postmerge.TOOLCHAIN_ROLES:
            with tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary) / 'output'
                with patch.dict(os.environ, {'GITHUB_OUTPUT': str(output)}), \
                        patch.object(postmerge, 'toolchain', return_value=self.compiler):
                    self.assertEqual(postmerge.main(['toolchain', '--role', role]), 0)
                key, value = output.read_text().strip().split('=', 1)
                self.assertEqual(key, 'toolchain_' + role)
                self.assertEqual(json.loads(value), self.compiler)

    def test_real_bounded_transport_caps_both_pipes_and_terminates_timeout(self):
        self.assertEqual(postmerge.bounded_capture([sys.executable, '-c', 'print("valid")'], timeout=10), b'valid\n')
        for stream in ('stdout', 'stderr'):
            command = [sys.executable, '-c', f'import sys; sys.{stream}.write("x" * {postmerge.MAX_BYTES + 65536})']
            with self.subTest(stream=stream), self.assertRaisesRegex(ValueError, 'size bound'):
                postmerge.bounded_capture(command, timeout=10)
        with self.assertRaises(subprocess.TimeoutExpired):
            postmerge.bounded_capture([sys.executable, '-c', 'import time; time.sleep(10)'], timeout=.2)

    def test_malformed_external_shapes_restore_normal_full_without_compiler_setup(self):
        variants = [
            ('/actions/workflows/ci.yml/runs?event=pull_request&head_sha=' + 'b' * 40 + '&per_page=100', []),
            ('/actions/runs/123/jobs?filter=latest&per_page=100', {'total_count': 1, 'jobs': [None]}),
            ('/actions/runs/123/artifacts?per_page=100', {'total_count': 1, 'artifacts': [[]]}),
            ('/git/commits/' + 'e' * 40, []),
            ('/git/commits/' + 'e' * 40, b'[' * 2000 + b'0' + b']' * 2000),
        ]
        for endpoint, malformed in variants:
            self.fixture()
            self.assertTrue(self.fetch())
            self.overrides[endpoint] = malformed
            normal = planner.full_plan('c' * 40, 'd' * 40, ['controls'])
            with patch.dict(os.environ, {'GITHUB_EVENT_NAME': 'push'}), \
                    patch.object(postmerge, 'context', return_value=self.current), \
                    patch.object(postmerge.time, 'time', return_value=self.now), \
                    patch.object(postmerge, 'bounded_capture', side_effect=lambda command, **kw: self.transport(command, **kw).stdout), \
                    patch.object(postmerge.subprocess, 'run') as setup:
                self.assertIs(postmerge.resolve(normal, self.event), normal)
                self.assertEqual(normal['mode'], 'full')
                setup.assert_not_called()

    def test_canonical_role_inventory_is_independent_of_api_fixture_builder(self):
        successful, skipped = postmerge.job_roles()
        self.assertEqual(successful, {
            'Change scope and documentation', 'Repository tooling lint', 'Dependencies (bans licenses sources)',
            'Rust (ubuntu-latest, debug, rust)', 'Rust (ubuntu-latest, release, rust)',
            'Rust (ubuntu-latest, debug, python)', 'Rust (ubuntu-latest, release, python)',
            'Rust (windows-latest, debug, rust)', 'Rust (windows-latest, release, rust)',
            'Full applications (windows-latest, debug)', 'Full applications (windows-latest, release)',
            'Rust (windows-latest, debug, python shard 1)', 'Rust (windows-latest, debug, python shard 2)',
            'Rust (windows-latest, release, python shard 1)', 'Rust (windows-latest, release, python shard 2)',
            '3D compatibility (linux-3d-consumer)', 'Rust (ubuntu-latest)', 'Rust (windows-latest)',
            'Full PR qualification'})
        self.assertEqual(skipped, {'Selected (${{ needs.scope.outputs.build_lane }})', 'Selected (${{ needs.scope.outputs.linux_lane }})',
                                  'Selected (${{ needs.scope.outputs.windows_lane }})', 'Selected (linux-release-python)', 'Post-merge integration'})
        from test_ci_workflow_contracts import workflow_job
        # Observed REST jobs on full PR run 39867462940 retain skipped expressions verbatim.
        for job, output in (('selected_rust', 'build_lane'), ('selected_linux', 'linux_lane'),
                            ('selected_windows', 'windows_lane')):
            literal = 'Selected (${{ needs.scope.outputs.' + output + ' }})'
            self.assertIn(literal, skipped)
            self.assertIn('name: ' + literal, workflow_job(job))
        self.assertIn('name: Full PR qualification', workflow_job('full_qualification'))
        self.assertIn('name: Post-merge integration', workflow_job('postmerge'))
        self.assertIn('name: Rust (${{ matrix.os }}, ${{ matrix.profile }}, ${{ matrix.partition }})', workflow_job('profiles'))
        self.assertIn('profile: [debug, release]', workflow_job('profiles'))
        self.assertIn('partition: [rust, python]', workflow_job('profiles'))
        self.assertIn('shard: [1, 2]', workflow_job('windows_python'))

    def test_invalid_archive_and_nonobject_seal_return_normal_full(self):
        import struct
        for mutation in ('encrypted', 'unsupported', 'traversal', 'duplicate', 'nonobject', 'deflate'):
            self.fixture()
            self.assertTrue(self.fetch())
            buffer = io.BytesIO()
            with zipfile.ZipFile(buffer, 'w', compression=zipfile.ZIP_DEFLATED if mutation == 'deflate' else zipfile.ZIP_STORED) as archive:
                name = '../qualification.json' if mutation == 'traversal' else 'qualification.json'
                archive.writestr(name, json.dumps([] if mutation == 'nonobject' else self.qualified))
                if mutation == 'duplicate': archive.writestr('extra.json', '{}')
            raw = bytearray(buffer.getvalue())
            central = raw.index(b'PK\x01\x02')
            if mutation == 'encrypted':
                struct.pack_into('<H', raw, 6, 1)
                struct.pack_into('<H', raw, central + 8, 1)
            elif mutation == 'unsupported':
                struct.pack_into('<H', raw, 8, 99)
                struct.pack_into('<H', raw, central + 10, 99)
            elif mutation == 'deflate':
                name_bytes, extra_bytes = struct.unpack_from('<HH', raw, 26)
                raw[30 + name_bytes + extra_bytes] = 7  # Reserved DEFLATE block type.
            endpoint = '/actions/runs/123/artifacts?per_page=100'
            body = self.responses()[endpoint]
            body['artifacts'][0]['digest'] = 'sha256:' + hashlib.sha256(raw).hexdigest()
            body['artifacts'][0]['size_in_bytes'] = len(raw)
            self.overrides[endpoint] = body
            self.overrides['/actions/artifacts/321/zip'] = bytes(raw)
            normal = planner.full_plan('c' * 40, 'd' * 40, ['controls'])
            with patch.dict(os.environ, {'GITHUB_EVENT_NAME': 'push'}), \
                    patch.object(postmerge, 'context', return_value=self.current), \
                    patch.object(postmerge.time, 'time', return_value=self.now), \
                    patch.object(postmerge, 'bounded_capture', side_effect=lambda command, **kw: self.transport(command, **kw).stdout), \
                    patch.object(postmerge.subprocess, 'run') as setup:
                self.assertIs(postmerge.resolve(normal, self.event), normal)
                self.assertEqual(normal['mode'], 'full')
                setup.assert_not_called()

    def test_actual_smoke_success_receipt_round_trip_and_failure_cannot_emit_receipt(self):
        import verify
        for failure in (None, 'stage', 'accounting', 'source'):
            proof, _, rows = self.receipt_fixture()
            seen = []
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                target = root / 'shared-target'
                logs = root / '.local/postmerge'
                def execute(name, command, environment, directory, jobs, rerun):
                    seen.append(name)
                    self.assertEqual(directory, logs)
                    self.assertEqual(environment['TOR_TEST_BINARIES'], str(target / 'debug'))
                    self.assertEqual(environment['TOR_SCENARIO_COMPILER'], str(target / 'debug/tor-scenario'))
                    self.assertEqual((directory / 'selected-ids.txt').read_text().splitlines(), proof['main']['smoke_ids'])
                    self.assertFalse(rerun)
                    if name == 'python-postmerge':
                        self.assertEqual(command[1:3], ['scripts/test_runner.py', '--workers'])
                        self.assertIn('.local/postmerge/selected-ids.txt', command)
                        emitted = rows if failure != 'accounting' else [rows[0], *rows[2:]]
                        (directory / 'python-postmerge-tests.jsonl').write_text(
                            ''.join(json.dumps(row) + '\n' for row in emitted))
                    return {'code': 1 if failure == 'stage' and name == 'platform-codecs' else 0}
                with patch.dict(os.environ, {'CARGO_TARGET_DIR': str(target)}), \
                        patch.object(postmerge, 'ROOT', root), \
                        patch.object(postmerge, 'validate_local', return_value=proof), \
                        patch.object(postmerge, 'toolchain', return_value=self.compiler), \
                        patch.object(postmerge, 'context', return_value={} if failure == 'source' else proof['main']), \
                        patch.object(postmerge.sys, 'platform', 'linux'), \
                        patch.object(verify, 'running_builds', return_value=[]), \
                        patch.object(verify, 'run_step', side_effect=execute):
                    if failure:
                        with self.assertRaises(ValueError): postmerge.smoke({})
                        self.assertFalse((logs / 'receipt.json').exists())
                    else:
                        postmerge.smoke({})
                        postmerge.validate_smoke({}, logs)
                        self.assertEqual(json.loads((logs / 'receipt.json').read_text())['main'], proof['main'])
                self.assertEqual(seen, ['build-applications', 'platform-codecs']
                                 + ([] if failure == 'stage' else ['python-postmerge']))
