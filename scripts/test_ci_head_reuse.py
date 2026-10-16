"""Synthetic evidence only; these fixtures never authorize actual CI reuse."""
import copy
import os
import json
from types import SimpleNamespace
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import ci_head_reuse as reuse
import ci_qualification as qualification
import test_ci_adoption_cell as fixtures
from test_ci_qualification import ApiFixture, BASE, HEAD, MERGE, TREE, PREFIX, NOW

encoded = fixtures.encoded


STUDY = 'test_rogue_playable_study_process.RoguePlayableStudyProcesses.test_actual_unspent_starters_preserve_identity_through_cli_and_source_free_replay'
OTHER = 'test_unchanged_process.Contract.test_route'
PATH = 'scripts/test_rogue_playable_study_process.py'


def fixture_rows():
    common = {'schema_version': 1, 'profile': 'debug'}
    return [{**common, 'kind': 'selection', 'test_ids': sorted([STUDY, OTHER])},
            {**common, 'kind': 'diagnostic', 'test_id': STUDY, 'exception_type': 'AssertionError',
             'traceback': 'AssertionError: stale expected value', 'traceback_truncated': False,
             'failure_context': {'phase': 'method', 'source': {'path': PATH, 'function': 'RoguePlayableStudyProcesses.study_round_trip', 'line': 10}}},
            *[{**common, 'kind': 'test', 'test_id': name, 'outcome': 'failed' if name == STUDY else 'passed',
               'failures': int(name == STUDY), 'errors': 0, 'subtests_failed': 0, 'subtests_skipped': 0} for name in (STUDY, OTHER)],
            {**common, 'kind': 'suite', 'tests_run': 2, 'successful': False, 'failures': 1, 'errors': 0,
             'skipped': 0, 'waived': 0, 'expected_failures': 0, 'unexpected_successes': 0}]


def changes():
    return {PATH: {'methods': [STUDY], 'old_sites': [{'function': 'RoguePlayableStudyProcesses.study_round_trip', 'line': 10}]}}


def raw(rows):
    return b'\n'.join(encoded(row) for row in rows) + b'\n'


class PythonReceipts(unittest.TestCase):
    def validate(self, rows):
        return reuse.python_receipt(raw(rows), sorted([STUDY, OTHER]), 'debug', changes())

    def test_complete_failed_stage_retains_only_passing_unaffected_result(self):
        result = self.validate(fixture_rows())
        self.assertEqual(result['passed'], [OTHER])
        self.assertEqual(result['failed'], [STUDY])

    def test_incomplete_duplicate_unknown_or_waived_evidence_rejects(self):
        mutations = [lambda r: r.pop(), lambda r: r.pop(2), lambda r: r.append(r[-1]),
                     lambda r: r[0]['test_ids'].append(STUDY), lambda r: r.insert(2, r[2]),
                     lambda r: r[-1].update(not_started=[OTHER]), lambda r: r[-1].update(batch_errors=['worker failed']),
                     lambda r: r[-1].update(tests_run=1), lambda r: r[-1].update(failures=0),
                     lambda r: r[3].update(outcome='waived'), lambda r: r[3].update(outcome='interrupted'),
                     lambda r: r[2].update(subtests_failed=1), lambda r: r[2].update(errors=1),
                     lambda r: r[-1].update(skipped=1), lambda r: r[-1].update(expected_failures=1)]
        for mutate in mutations:
            rows = fixture_rows(); mutate(rows)
            with self.subTest(rows=rows), self.assertRaises((qualification.EvidenceError, KeyError)):
                self.validate(rows)

    def test_failures_require_exact_old_acceptance_site_and_method_phase(self):
        mutations = [lambda r: r.pop(1), lambda r: r.insert(1, r[1]),
                     lambda r: r[1].update(test_id=OTHER), lambda r: r[1].update(exception_type='RuntimeError'),
                     lambda r: r[1].update(traceback_truncated=True), lambda r: r[1].update(subtest_id='subtest'),
                     lambda r: r[1]['failure_context'].update(phase='setup'),
                     lambda r: r[1]['failure_context'].update(phase='teardown'),
                     lambda r: r[1]['failure_context'].update(phase='cleanup'),
                     lambda r: r[1]['failure_context']['source'].update(line=11),
                     lambda r: r[1]['failure_context']['source'].update(function='invoke')]
        for mutate in mutations:
            rows = fixture_rows(); mutate(rows)
            with self.subTest(rows=rows), self.assertRaises(qualification.EvidenceError): self.validate(rows)

    def test_unknown_old_diagnostic_is_not_upgraded(self):
        rows = fixture_rows(); del rows[1]['failure_context']
        with self.assertRaises(qualification.EvidenceError): self.validate(rows)


class CrossHeadCell(unittest.TestCase):
    def setUp(self):
        fixtures.CellValidation.setUp(self)
        fixtures.CellValidation.configure_python(self)
        self.identity = {'schema_version': 1, 'mode': 'cross_head_composite', 'pr': 1, 'base': 'e' * 40,
                         'source': {**self.source, 'head': 'd' * 40},
                         'target': {'run_id': 102, 'checkout_sha': 'c' * 40, 'tree': '2' * 40,
                                    'head': '3' * 40, 'created_at': '2026-10-16T02:01:00Z'}, 'changes': changes()}
        self.target['checkout'].update(tree='2' * 40, parents=['e' * 40, '3' * 40])
        self.target['github'].update(event='pull_request', ref='refs/pull/1/merge',
            workflow_ref=qualification.REPOSITORY + '/.github/workflows/ci.yml@refs/pull/1/merge')
        self.rows = [{'test_id': STUDY, 'kind': 'process'}, {'test_id': OTHER, 'kind': 'process'}]
        self.records['fixture-full-ci-debug-python/python-debug-tests.jsonl'] = raw(fixture_rows())
        self.summary['steps'][-1].update(code=1, status='failed')
        self.summary.update(gate_passed=False, clean_passed=False)
        self.job['conclusion'] = 'failure'

    def validate(self):
        records = {**self.records, 'observations/before.json': encoded(self.before), 'observations/after.json': encoded(self.after),
                   'fixture-full-ci-debug-python/summary.json': encoded(self.summary)}
        return reuse.validate_cell(records, self.identity, self.job, self.target, profile='debug', partition='python', jobs=4, workers=2, rows=self.rows)

    def test_prior_failed_cell_qualifies_unaffected_methods(self):
        coverage = self.validate()
        self.assertEqual(coverage['fresh'], [STUDY])
        self.assertEqual(coverage['borrowed'], [OTHER])

    def test_missing_origin_and_relabelled_composite_reject_cross_head_source(self):
        del self.summary['evidence_origin']
        with self.assertRaises(ValueError): self.validate()
        self.summary.update(evidence_origin='fresh', cross_head={'union': {'complete': True}})
        with self.assertRaises(ValueError): self.validate()

    def test_cross_head_identity_and_completed_setup_are_required(self):
        for mutate in (lambda: self.identity['source'].update(tree='9' * 40),
                       lambda: self.identity['target'].update(head='9' * 40),
                       lambda: self.identity.update(base='9' * 40),
                       lambda: self.summary['steps'][0].update(code=1),
                       lambda: self.summary.update(evidence_origin='cross_head_composite'),
                       lambda: self.summary.update(not_run_steps=['build-tools']),
                       lambda: self.job.update(completed_at='2026-10-16T02:02:00Z')):
            self.setUp(); mutate()
            with self.assertRaises(qualification.EvidenceError): self.validate()


class CrossHeadIdentity(unittest.TestCase):
    def setUp(self):
        self.api = ApiFixture()
        self.new_head = '4' * 40
        self.api.data[f'{PREFIX}/git/ref/heads/main']['object']['sha'] = BASE
        self.api.data[f'{PREFIX}/actions/runs/102'].update(event='pull_request', head_sha=self.new_head)
        self.api.source().update(conclusion='cancelled', updated_at='2026-10-16T11:55:00Z')
        self.api.data[f'{PREFIX}/pulls/9'].update(state='open', merged=False,
            head={'sha': self.new_head, 'repo': {'id': qualification.REPOSITORY_ID}})
        self.api.data[f'{PREFIX}/git/commits/{MERGE}']['parents'][1]['sha'] = self.new_head
        self.api.data[f'{PREFIX}/git/commits/{self.new_head}'] = {
            'sha': self.new_head, 'parents': [{'sha': HEAD}], 'tree': {'sha': TREE}}
        self.target = {'github': {'ref': 'refs/pull/9/merge', 'event': 'pull_request',
            'repository': qualification.REPOSITORY, 'run_id': 102}, 'checkout': {
                'sha': MERGE, 'tree': TREE, 'parents': [BASE, self.new_head]}}
        self.patch = self.enterContext(patch.object(reuse.policy, 'validate_trees', return_value=changes()))
        self.artifact = {'id': 200, 'name': 'verification-ubuntu-latest-debug-python', 'expired': False,
            'workflow_run': {'id': 101, 'head_sha': HEAD, 'repository_id': qualification.REPOSITORY_ID,
                             'head_repository_id': qualification.REPOSITORY_ID},
            'created_at': '2026-10-16T11:38:00Z', 'updated_at': '2026-10-16T11:38:01Z',
            'expires_at': '2026-10-20T11:38:00Z'}
        self.api.data[f'{PREFIX}/actions/runs/101/artifacts?per_page=100&page=1'] = {'total_count': 1, 'artifacts': [self.artifact]}

    def inspect(self):
        return reuse.inspect(self.api, self.target, now=NOW)

    def test_completed_cell_in_superseded_cancelled_run_is_eligible(self):
        identity, jobs, _ = self.inspect()
        job, artifact = reuse.source_cell(self.api, identity, jobs, 'ubuntu-latest', 'debug', 'python', now=NOW)
        self.assertEqual(job['conclusion'], 'success')
        self.assertEqual(artifact['id'], 200)
        self.assertEqual(identity['source']['head'], HEAD)
        self.assertEqual(identity['target']['head'], self.new_head)

    def test_identity_ambiguity_changed_base_controls_and_reruns_reject(self):
        mutations = [lambda: self.api.source().update(run_attempt=2),
                     lambda: self.api.source().update(status='in_progress'),
                     lambda: self.api.source().update(run_started_at='2026-10-14T11:00:00Z'),
                     lambda: self.api.source()['pull_requests'][0]['base'].update(sha='8' * 40),
                     lambda: self.api.data[f'{PREFIX}/pulls/9'].update(state='closed'),
                     lambda: self.api.data[f'{PREFIX}/actions/runs/102'].update(conclusion='cancelled'),
                     lambda: self.api.data[f'{PREFIX}/git/commits/{self.new_head}']['parents'].append({'sha': BASE}),
                     lambda: self.api.jobs()['jobs'].pop(),
                     lambda: self.api.data[f'{PREFIX}/git/trees/{TREE}?recursive=1']['tree'].append(
                         {'path': '.github/new-control.yml', 'type': 'blob', 'mode': '100644', 'sha': '7' * 40})]
        for mutate in mutations:
            self.setUp(); mutate()
            with self.assertRaises(qualification.EvidenceError): self.inspect()

    def test_final_live_authority_rejects_new_head_base_closed_pr_or_source_rerun(self):
        for variant in ('head', 'base', 'closed', 'rerun', 'cancelled', 'unchanged'):
            self.setUp()
            identity, _, _ = self.inspect()
            identity['source']['snapshot'] = qualification.run_snapshot(identity['source']['current'])
            if variant == 'head': self.api.data[f'{PREFIX}/pulls/9']['head']['sha'] = '9' * 40
            if variant == 'base': self.api.data[f'{PREFIX}/git/ref/heads/main']['object']['sha'] = '9' * 40
            if variant == 'closed': self.api.data[f'{PREFIX}/pulls/9']['state'] = 'closed'
            if variant == 'rerun': self.api.data[f'{PREFIX}/actions/runs/101']['run_attempt'] = 2
            if variant == 'cancelled': self.api.data[f'{PREFIX}/actions/runs/102']['status'] = 'completed'
            if variant == 'unchanged': reuse.recheck(self.api, identity, now=NOW)
            else:
                with self.subTest(variant=variant), self.assertRaises(ValueError): reuse.recheck(self.api, identity, now=NOW)

    def test_each_source_endpoint_requires_its_own_valid_interval(self):
        for endpoint in ('attempt', 'list', 'current'):
            for field, value in (('updated_at', '2026-10-16T10:00:00Z'),
                                 ('updated_at', '2026-10-16T12:00:01Z'),
                                 ('updated_at', 'not-a-time'),
                                 ('run_started_at', '2026-10-16T11:56:00Z')):
                self.setUp()
                responses = self.api.independent_source_responses()
                responses[endpoint][field] = value
                with self.subTest(endpoint=endpoint, field=field, value=value), self.assertRaises(ValueError):
                    self.inspect()

    def test_cancelled_incomplete_late_cells_and_artifacts_reject(self):
        for variant in ('cancelled', 'in_progress', 'late', 'step', 'artifact_time', 'artifact_head', 'duplicate'):
            self.setUp()
            identity, jobs, _ = self.inspect()
            job = next(row for row in jobs if row['name'] == 'Rust (ubuntu-latest, debug, python)')
            if variant == 'cancelled': job['conclusion'] = 'cancelled'
            if variant == 'in_progress': job['status'] = 'in_progress'
            if variant == 'late': job['completed_at'] = '2026-10-16T11:51:00Z'
            if variant == 'step': job['steps'][-1]['conclusion'] = 'cancelled'
            if variant == 'artifact_time': self.artifact['updated_at'] = '2026-10-16T11:45:00Z'
            if variant == 'artifact_head': self.artifact['workflow_run']['head_sha'] = self.new_head
            if variant == 'duplicate': jobs.append(copy.deepcopy(job))
            with self.subTest(variant=variant), self.assertRaises(qualification.EvidenceError):
                reuse.source_cell(self.api, identity, jobs, 'ubuntu-latest', 'debug', 'python', now=NOW)


class SelectionAndConsumers(unittest.TestCase):
    def test_selected_plan_preserves_builds_and_reruns_changed_module_plus_tools(self):
        tool = 'test_tool.Contract.test_inventory'
        rows = [{'test_id': name, 'kind': kind} for name, kind in ((STUDY, 'process'), (OTHER, 'process'), (tool, 'tool'))]
        proof = {'schema_version': 1, 'mode': 'cross_head_composite', 'base': BASE,
                 'target': {'checkout_sha': MERGE, 'tree': TREE, 'head': HEAD, 'run_id': 102},
                 'cell': ['ubuntu-latest', 'debug', 'python'], 'limits': {'jobs': 4, 'test_workers': 2},
                 'changes': changes(), 'coverage': {'expected': sorted([STUDY, OTHER, tool]),
                     'fresh': sorted([STUDY, tool]), 'borrowed': [OTHER]}}
        plan = reuse.verify.plan('full', set(), [], ci_profile='debug', ci_partition='python', xvfb=True, workers=2)
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {
                'GITHUB_SHA': MERGE, 'GITHUB_RUN_ID': '102', 'GITHUB_EVENT_NAME': 'pull_request'}), \
                patch.object(reuse, 'current_checkout'), patch.object(reuse, 'inventory', return_value=rows):
            selected = reuse.selected_plan(proof, plan, profile='debug', partition='python', jobs=4, workers=2, directory=directory)
            self.assertEqual(selected[:-1], plan[:-1])
            command, overrides = selected[-1][1:]
            self.assertEqual(command[:len(reuse.verify.python_test_prefix(plan[-1][1]))], reuse.verify.python_test_prefix(plan[-1][1]))
            self.assertEqual(Path(command[command.index('--from-file') + 1]).read_text().splitlines(), sorted([STUDY, tool]))
            self.assertEqual(overrides['_cwd'], 'scripts')
            proof['coverage']['fresh'] = [STUDY]
            with self.assertRaises(ValueError):
                reuse.selected_plan(proof, plan, profile='debug', partition='python', jobs=4, workers=2, directory=directory)

    def test_direct_python_rust_and_service_consumers_reject(self):
        for path in ('scripts/helper.py', 'scripts/test_other_process.py', 'scripts/service_contract_tests.py', 'crates/server/reader.rs'):
            with patch.object(reuse.subprocess, 'run', return_value=SimpleNamespace(returncode=0, stdout=f'{MERGE}:{path}\0'.encode())):
                with self.subTest(path=path), self.assertRaises(ValueError): reuse.direct_consumers(changes(), MERGE)
        with patch.object(reuse.subprocess, 'run', return_value=SimpleNamespace(returncode=0,
                stdout=f'{MERGE}:scripts/test_ci_head_reuse.py\0'.encode())):
            reuse.direct_consumers(changes(), MERGE)


class UnionCoverage(unittest.TestCase):
    def test_final_union_requires_live_clean_checkout(self):
        proof = {'base': BASE, 'target': {'checkout_sha': MERGE, 'run_id': 102, 'tree': TREE, 'head': HEAD},
                 'cell': ['ubuntu-latest', 'release', 'rust'], 'coverage': {'expected': [], 'fresh': [], 'borrowed': []}}
        clean = {'sha': MERGE, 'tree': TREE, 'parents': [BASE, HEAD], 'tracked_dirty': False, 'untracked_present': False}
        with patch.dict(os.environ, {'GITHUB_SHA': MERGE, 'GITHUB_RUN_ID': '102'}):
            for field, value in (('sha', HEAD), ('tree', HEAD), ('tracked_dirty', True), ('untracked_present', True)):
                with patch.object(reuse.observation, 'checkout', return_value={**clean, field: value}):
                    with self.subTest(field=field), self.assertRaises(ValueError): reuse.union(proof, [], [], '.', 4)

    def test_final_union_rejects_holes_duplicates_overlap_and_target_drift(self):
        rows = fixture_rows(); rows.pop(1); rows = [row for row in rows if row.get('test_id') != OTHER]
        rows[0]['test_ids'] = [STUDY]
        rows[1].update(outcome='passed', failures=0)
        rows[-1].update(tests_run=1, failures=0, successful=True)
        proof = {'base': BASE, 'target': {'checkout_sha': MERGE, 'run_id': 102, 'tree': TREE, 'head': HEAD}, 'cell': ['ubuntu-latest', 'debug', 'python'],
                 'coverage': {'fresh': [STUDY], 'borrowed': [OTHER], 'expected': sorted([STUDY, OTHER]),
                              'source_receipt': {'passed': [OTHER]}}}
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {'GITHUB_SHA': MERGE, 'GITHUB_RUN_ID': '102'}), \
                patch.object(reuse.verify, 'receipt_completeness', return_value={'complete': True}), \
                patch.object(reuse.observation, 'checkout', return_value={'sha': MERGE, 'tree': TREE,
                    'parents': [BASE, HEAD], 'tracked_dirty': False, 'untracked_present': False}), patch.object(reuse, 'final_authority'):
            Path(directory, 'python-debug-tests.jsonl').write_bytes(raw(rows))
            plan = [('python-debug', ['python', 'test_runner.py'], {})]
            self.assertTrue(reuse.union(proof, plan, [], directory, 4)['complete'])
            for change in ('hole', 'overlap', 'duplicate', 'source_failure', 'target'):
                broken = copy.deepcopy(proof)
                if change == 'hole': broken['coverage']['borrowed'] = []
                if change == 'overlap': broken['coverage']['borrowed'].append(STUDY)
                if change == 'duplicate': broken['coverage']['fresh'].append(STUDY)
                if change == 'source_failure': broken['coverage']['source_receipt']['passed'] = []
                if change == 'target': broken['target']['checkout_sha'] = HEAD
                with self.subTest(change=change), self.assertRaises(qualification.EvidenceError):
                    reuse.union(broken, plan, [], directory, 4)


class RealUnionReceipt(unittest.TestCase):
    def test_real_completeness_and_union_preserve_old_resume_rejection(self):
        import ci_observation
        from rogue_earned_route_process import fingerprint, validate_resume_binding
        from test_rogue_earned_route_process_support import RouteResumeHardeningTests
        rows = fixture_rows(); rows.pop(1); rows = [row for row in rows if row.get('test_id') != OTHER]
        rows[0]['test_ids'] = [STUDY]; rows[1].update(outcome='passed', failures=0)
        rows[-1].update(tests_run=1, failures=0, successful=True)
        proof = {'target': {'checkout_sha': MERGE, 'run_id': 102}, 'cell': ['ubuntu-latest', 'debug', 'python'],
                 'coverage': {'fresh': [STUDY], 'borrowed': [OTHER], 'expected': sorted([STUDY, OTHER]),
                              'source_receipt': {'passed': [OTHER], 'sha256': 'a' * 64}}}
        command = ['python', 'test_runner.py', STUDY]
        environment = ci_observation.build_test_environment(reuse.verify.verification_environment(4, {}), [])
        environment['scope'] = 'verifier_stage'
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {'GITHUB_SHA': MERGE, 'GITHUB_RUN_ID': '102'}), \
                patch.object(reuse, 'current_checkout'), patch.object(reuse, 'final_authority'):
            root = Path(directory)
            Path(root, 'python-debug-tests.jsonl').write_bytes(raw(rows))
            stage = {'step': 'python-debug', 'code': 0, 'note': '', 'waived_tests': [], 'command': command,
                     'cwd': '.', 'environment': environment, 'environment_issues': [], 'log': root / 'python-debug.log'}
            ledger = reuse.union(proof, [('python-debug', command, {})], [stage], root, 4)
            self.assertTrue(ledger['complete'])
            retained = RouteResumeHardeningTests().report(root)
            original = json.dumps(retained, sort_keys=True)
            (root / 'source').write_text('changed eligible test expectations')
            expected = copy.deepcopy(retained)
            expected['frozen_inputs'] = fingerprint([root / 'source', root / 'server'])
            with self.assertRaisesRegex(ValueError, 'fingerprints changed'):
                validate_resume_binding(retained, expected, root / 'package')
            self.assertEqual(json.dumps(retained, sort_keys=True), original)
            rows[-1]['tests_run'] = 0
            Path(root, 'python-debug-tests.jsonl').write_bytes(raw(rows))
            with self.assertRaisesRegex(ValueError, 'fresh stages incomplete'):
                reuse.union(proof, [('python-debug', command, {})], [stage], root, 4)


class VerifierReplay(unittest.TestCase):
    def test_actual_selection_and_union_orchestrate_valid_rejected_and_failed_fresh_runs(self):
        from contextlib import redirect_stdout
        from io import StringIO
        from test_verify import metadata
        tool = 'test_tool.Contract.test_inventory'
        inventory = [{'test_id': name, 'kind': kind} for name, kind in ((STUDY, 'process'), (OTHER, 'process'), (tool, 'tool'))]
        source_rows = fixture_rows()
        source_rows[0]['test_ids'] = sorted([STUDY, OTHER, tool])
        source_rows.insert(-1, {**source_rows[3], 'test_id': tool})
        source_rows[-1]['tests_run'] = 3
        receipt = reuse.python_receipt(raw(source_rows), sorted([STUDY, OTHER, tool]), 'debug', changes())
        proof = {'schema_version': 1, 'mode': 'cross_head_composite', 'base': BASE,
                 'target': {'checkout_sha': MERGE, 'tree': TREE, 'head': HEAD, 'run_id': 102},
                 'cell': ['ubuntu-latest', 'debug', 'python'], 'limits': {'jobs': 1, 'test_workers': 1},
                 'changes': changes(), 'coverage': {'expected': sorted([STUDY, OTHER, tool]),
                     'fresh': sorted([STUDY, tool]), 'borrowed': [OTHER], 'source_receipt': receipt}}
        clean = {'sha': MERGE, 'tree': TREE, 'parents': [BASE, HEAD], 'tracked_dirty': False, 'untracked_present': False}
        for mode in ('valid', 'rejected', 'fresh_failure'):
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                handoff = root / 'proof.json'
                candidate = copy.deepcopy(proof)
                if mode == 'rejected': candidate['target']['checkout_sha'] = '9' * 40
                handoff.write_text(json.dumps(candidate))
                observed = []
                def run(name, command, overrides, logs, jobs, rerun):
                    environment = reuse.observation.build_test_environment({**os.environ, **reuse.verify.verification_environment(
                        jobs, {key: value for key, value in overrides.items() if not key.startswith('_')})}, [])
                    environment['scope'] = 'verifier_stage'
                    log = logs / f'{name}.log'; log.write_text('synthetic successful build')
                    failed = False
                    if reuse.verify.python_test_prefix(command):
                        ids = (Path(command[command.index('--from-file') + 1]).read_text().splitlines()
                               if '--from-file' in command else sorted([STUDY, OTHER, tool]))
                        observed.extend(ids)
                        records = [{ 'schema_version': 1, 'profile': 'debug', 'kind': 'selection', 'test_ids': ids}]
                        for ident in ids:
                            row = {**source_rows[3], 'test_id': ident, 'outcome': 'passed', 'failures': 0}
                            records.append(row)
                        failed = mode == 'fresh_failure'
                        if failed: records[1].update(outcome='failed', failures=1)
                        records.append({**source_rows[-1], 'tests_run': len(ids), 'successful': not failed, 'failures': int(failed)})
                        (logs / f'{name}-tests.jsonl').write_bytes(raw(records))
                    return {'step': name, 'code': int(failed), 'seconds': 0, 'log': log, 'note': '', 'waived_tests': [],
                            'command': command, 'cwd': overrides.get('_cwd', '.'), 'environment': environment, 'environment_issues': []}
                with patch.dict(os.environ, {'GITHUB_SHA': MERGE, 'GITHUB_RUN_ID': '102', 'GITHUB_EVENT_NAME': 'pull_request'}, clear=True), \
                        patch.object(reuse.verify, 'ROOT', root), patch.object(reuse.verify, 'running_builds', return_value=[]), \
                        patch.object(reuse.verify.subprocess, 'run', return_value=SimpleNamespace(stdout=json.dumps(metadata()))), \
                        patch.object(reuse.verify, 'free_memory_gb', return_value=8), patch.object(reuse.verify, 'run_step', side_effect=run), \
                        patch.object(reuse.observation, 'checkout', return_value=clean), patch.object(reuse, 'inventory', return_value=inventory), \
                        patch.object(reuse, 'final_authority') as authority, redirect_stdout(StringIO()):
                    code = reuse.verify.main(['full', '--ci-profile', 'debug', '--ci-partition', 'python',
                        '--jobs', '1', '--test-workers', '1', '--cross-head-proof', str(handoff)])
                summary = json.loads(next(root.glob('.local/verify/*/summary.json')).read_text())
                self.assertEqual(code, int(mode == 'fresh_failure'))
                self.assertEqual(observed, sorted([STUDY, OTHER, tool]) if mode == 'rejected' else sorted([STUDY, tool]))
                self.assertEqual(authority.call_count, int(mode == 'valid'))
                self.assertEqual(summary['evidence_origin'], 'fresh' if mode == 'rejected' else 'cross_head_composite')
                if mode == 'valid': self.assertTrue(summary['cross_head']['union']['complete'])


class RustWholeCellReuse(unittest.TestCase):
    def test_real_retained_rust_cell_is_required_before_empty_fresh_selection(self):
        CrossHeadIdentity.setUp(self)
        identity, jobs, _ = reuse.inspect(self.api, self.target, now=NOW)
        self.artifact['name'] = 'verification-ubuntu-latest-release-rust'
        job, _ = reuse.source_cell(self.api, identity, jobs, 'ubuntu-latest', 'release', 'rust', now=NOW)
        fixtures.CellValidation.setUp(self)
        source, target = identity['source'], identity['target']
        self.before['checkout'].update(sha=source['checkout_sha'], tree=source['tree'], parents=[BASE, HEAD])
        self.before['github'].update(reported_sha=source['checkout_sha'], workflow_sha=source['checkout_sha'],
            ref='refs/pull/9/merge', workflow_ref=qualification.REPOSITORY + '/.github/workflows/ci.yml@refs/pull/9/merge')
        self.before['recorded_at'] = '2026-10-16T11:02:00Z'
        self.after = copy.deepcopy(self.before)
        self.after.update(phase='after', recorded_at='2026-10-16T11:38:00Z')
        current = copy.deepcopy(self.before)
        current['checkout'].update(sha=target['checkout_sha'], tree=target['tree'], parents=[BASE, target['head']])
        current['github'].update(run_id=102, reported_sha=target['checkout_sha'], workflow_sha=target['checkout_sha'])
        current['recorded_at'] = '2026-10-16T11:51:00Z'
        records = {**self.records, 'observations/before.json': encoded(self.before), 'observations/after.json': encoded(self.after),
                   'fixture-full-ci-release-rust/summary.json': encoded(self.summary)}
        validate = lambda evidence: reuse.validate_cell(evidence, identity, job, current,
            profile='release', partition='rust', jobs=4, workers=2, rows=[])
        coverage = validate(records)  # Actual full Service receipt/completeness reader, not a mocked pass.
        proof = {**identity, 'cell': ['ubuntu-latest', 'release', 'rust'],
                 'limits': {'jobs': 4, 'test_workers': 2}, 'coverage': coverage}
        proof['source']['snapshot'] = qualification.run_snapshot(source['current'])
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {
                'GITHUB_SHA': MERGE, 'GITHUB_RUN_ID': '102', 'GITHUB_EVENT_NAME': 'pull_request'}), \
                patch.object(reuse.observation, 'checkout', return_value=current['checkout']), \
                patch.object(reuse, 'final_authority', side_effect=lambda candidate, _: reuse.recheck(self.api, candidate, now=NOW)):
            selected = reuse.selected_plan(proof, self.plan, profile='release', partition='rust', jobs=4, workers=2, directory=directory)
            self.assertEqual(selected, [])
            self.assertTrue(reuse.union(proof, selected, [], directory, 4)['borrowed_cell'])
            broken = copy.deepcopy(proof); broken['coverage']['fresh'] = [OTHER]
            with self.assertRaisesRegex(ValueError, 'whole-cell coverage'):
                reuse.union(broken, selected, [], directory, 4)
        malformed = {**records, 'fixture-full-ci-release-rust/rust-release.log': b'no complete Service result\n'}
        with self.assertRaises(ValueError): validate(malformed)
        for conclusion in ('failure', 'cancelled'):
            job['conclusion'] = conclusion
            with self.subTest(conclusion=conclusion), self.assertRaises(ValueError):
                reuse.source_cell(self.api, identity, jobs, 'ubuntu-latest', 'release', 'rust', now=NOW)
