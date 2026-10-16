"""Synthetic contract fixtures; injected context is never real adoption proof."""
import copy
import hashlib
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import ci_adoption as adoption
import ci_observation as observation
import ci_qualification as qualification
import verify


def encoded(value):
    return json.dumps(value, sort_keys=True).encode()


class CellValidation(unittest.TestCase):
    def setUp(self):
        self.profile, self.partition = 'release', 'rust'
        self.environment = {'CARGO_INCREMENTAL': '0', 'CARGO_PROFILE_DEV_DEBUG': '0', 'PATH': '/fixture/bin'}
        self.env_patch = self.enterContext(patch.dict(os.environ, self.environment, clear=True))
        self.source = {'run_id': 101, 'attempt': 1, 'checkout_sha': 'a' * 40, 'tree': 'b' * 40}
        self.identity = {'candidate': True, 'target_sha': 'c' * 40, 'target_run': 102, 'source': self.source}
        self.job = {'name': 'Rust (ubuntu-latest, release, rust)', 'run_id': 101, 'run_attempt': 1,
                    'head_sha': 'd' * 40, 'status': 'completed', 'conclusion': 'success',
                    'started_at': '2026-10-16T01:00:00Z', 'completed_at': '2026-10-16T02:00:00Z'}
        github = {'repository': qualification.REPOSITORY, 'job': 'profiles', 'event': 'pull_request',
                  'run_id': 101, 'run_attempt': 1, 'reported_sha': 'a' * 40, 'workflow_sha': 'a' * 40,
                  'workflow_ref': qualification.REPOSITORY + '/.github/workflows/ci.yml@refs/pull/1/merge',
                  'ref': 'refs/pull/1/merge'}
        self.before = {'schema_version': 1, 'status': 'untrusted_observation', 'phase': 'before', 'complete': True,
                       'issues': [], 'recorded_at': '2026-10-16T01:01:00Z', 'github': github,
                       'labels': {'profile': 'release', 'partition': 'rust'},
                       'checkout': {'sha': 'a' * 40, 'tree': 'b' * 40, 'parents': ['e' * 40, 'd' * 40],
                                    'tracked_dirty': False, 'untracked_present': False},
                       'runner': {'os': 'Linux', 'architecture': 'X64', 'image_os': 'ubuntu24', 'image_version': 'fixture'},
                       'file_hashes': {name: {'status': 'present', 'bytes': 1, 'sha256': 'f' * 64}
                                       for name in observation.CONFIG_FILES},
                       'tools': {'rustc': {'release': 'fixture'}, 'cargo': 'fixture', 'python': {'version': 'fixture'}},
                       'build_test_environment': observation.build_test_environment(self.environment, []),
                       'native_packages': {'status': 'present', 'count': 1, 'sha256': 'f' * 64},
                       'graphical': {'mode': 'none'}, 'execution_limits': {'jobs': 4, 'test_workers': 2}}
        self.after = copy.deepcopy(self.before)
        self.after.update(phase='after', recorded_at='2026-10-16T01:59:00Z')
        self.target = copy.deepcopy(self.before)
        self.target['checkout']['sha'] = 'c' * 40
        self.target['github'].update(event='push', run_id=102, ref='refs/heads/main', reported_sha='c' * 40,
                                     workflow_sha='c' * 40,
                                     workflow_ref=qualification.REPOSITORY + '/.github/workflows/ci.yml@refs/heads/main')
        self.plan = verify.plan('full', [], [], ci_profile='release', ci_partition='rust', workers=2)
        name, command, overrides = self.plan[0]
        stage_environment = observation.build_test_environment({**self.environment, **verify.verification_environment(4, overrides)}, [])
        stage_environment['scope'] = 'verifier_stage'
        artifact = {'executable': str(verify.ROOT / 'target/release/tor-service-contract-host'), 'sha256': '1' * 64,
                    'profile': 'release', 'features': ['service-contract-host']}
        result = {'selected': 13, 'run': 13, 'failures': 0, 'errors': 0, 'skipped': 0}
        log = b'SERVICE_CONTRACT_ARTIFACT ' + encoded(artifact) + b'\nSERVICE_CONTRACT_RESULT ' + encoded(result) + b'\n'
        row = {'step': name, 'code': 0, 'status': 'passed', 'note': '', 'waived_tests': [], 'command': command,
               'cwd': '.', 'environment': stage_environment, 'environment_issues': [],
               'log': '.local/verify/fixture-full-ci-release-rust/rust-release.log'}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'rust-release.log'
            path.write_bytes(log)
            local = {**row, 'log': str(path)}
            complete = verify.receipt_completeness(self.plan, [local], directory, 4)
        self.assertTrue(complete['complete'])
        self.summary = {'schema_version': 1, 'tier': 'full', 'scope': 'ci-partition', 'ci_profile': 'release',
                        'ci_partition': 'rust', 'jobs': 4, 'test_workers': 2, 'clean_passed': True, 'gate_passed': True,
                        'graphical_waiver': None, 'waived_steps': [], 'not_run_steps': [], 'planned_steps': [name],
                        'steps': [row], 'receipt_completeness': complete}
        self.records = {'fixture-full-ci-release-rust/rust-release.log': log}

    def validate(self):
        records = {**self.records, 'observations/before.json': encoded(self.before),
                   'observations/after.json': encoded(self.after),
                   f'fixture-full-ci-{self.profile}-{self.partition}/summary.json': encoded(self.summary)}
        return adoption.validate_cell(records, self.identity, self.job, self.target,
                                      profile=self.profile, partition=self.partition, jobs=4, workers=2)

    def configure_python(self):
        self.profile, self.partition = 'debug', 'python'
        for observation_row in (self.before, self.after, self.target):
            observation_row['labels'] = {'profile': 'debug', 'partition': 'python'}
            observation_row['graphical'] = {'mode': 'xvfb', 'screen': '0 1280x1024x24'}
        self.job['name'] = 'Rust (ubuntu-latest, debug, python)'
        self.plan = verify.plan('full', [], [], ci_profile='debug', ci_partition='python', workers=2, xvfb=True)
        common = {'schema_version': 1, 'profile': 'debug'}
        selected = ['test_fixture.Contract.test_one', 'test_fixture.Contract.test_two']
        rows = [{**common, 'kind': 'selection', 'test_ids': selected},
                {**common, 'kind': 'build', 'seconds': 0.01, 'outcome': 'passed', 'returncode': 0}]
        rows += [{**common, 'kind': 'test', 'test_id': name, 'outcome': 'passed', 'failures': 0, 'errors': 0,
                  'subtests_failed': 0, 'subtests_skipped': 0} for name in selected]
        rows.append({**common, 'kind': 'suite', 'successful': True, 'tests_run': 2, 'failures': 0, 'errors': 0,
                     'skipped': 0, 'waived': 0, 'expected_failures': 0, 'unexpected_successes': 0})
        self.python_rows = rows
        self.records, stages = {}, []
        prefix = 'fixture-full-ci-debug-python'
        with tempfile.TemporaryDirectory() as directory:
            local = []
            for name, command, overrides in self.plan:
                environment = observation.build_test_environment({**self.environment, **verify.verification_environment(4, overrides)}, [])
                environment['scope'] = 'verifier_stage'
                filename = name + ('-tests.jsonl' if verify.python_test_prefix(command) is not None else '.log')
                value = b'\n'.join(map(encoded, rows)) + b'\n' if filename.endswith('.jsonl') else b'build succeeded\n'
                self.records[f'{prefix}/{filename}'] = value
                path = Path(directory) / filename
                path.write_bytes(value)
                row = {'step': name, 'code': 0, 'status': 'passed', 'note': '', 'waived_tests': [], 'command': command,
                       'cwd': '.', 'environment': environment, 'environment_issues': [], 'log': f'.local/verify/{prefix}/{name}.log'}
                stages.append(row)
                local.append({**row, 'log': str(path)})
            complete = verify.receipt_completeness(self.plan, local, directory, 4)
        self.assertTrue(complete['complete'])
        self.summary.update(ci_profile='debug', ci_partition='python', planned_steps=[name for name, _, _ in self.plan],
                            steps=stages, receipt_completeness=complete)

    def test_python_plan_records_include_selection_build_and_terminal_accounting(self):
        self.configure_python()
        self.assertIsNone(self.validate())
        for change in ('duplicate', 'interrupted', 'failed'):
            rows = copy.deepcopy(self.python_rows)
            if change == 'duplicate': rows[0]['test_ids'].append(rows[0]['test_ids'][0])
            if change == 'interrupted': rows.pop()
            if change == 'failed': rows[2]['outcome'] = 'failed'
            data = b'\n'.join(map(encoded, rows)) + b'\n'
            self.records['fixture-full-ci-debug-python/python-debug-tests.jsonl'] = data
            record = self.summary['receipt_completeness']['stages'][-1]['record']
            record.update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
            with self.subTest(change=change), self.assertRaises(adoption.EvidenceError): self.validate()

    def test_matching_but_unsupported_native_display_and_runtime_context_rejects(self):
        for field, value in (('native_packages', {'status': 'unavailable'}), ('graphical', {'mode': 'external_unsupported'})):
            with self.subTest(field=field):
                old = self.before[field]
                for row in (self.before, self.after, self.target): row[field] = copy.deepcopy(value)
                with self.assertRaises(adoption.EvidenceError): self.validate()
                for row in (self.before, self.after, self.target): row[field] = copy.deepcopy(old)
        with patch.dict(os.environ, {'TOR_TEST_BINARIES': '/external/apps'}):
            for row in (self.before, self.after, self.target):
                row['build_test_environment'] = observation.build_test_environment(os.environ, [])
            with self.assertRaises(adoption.EvidenceError): self.validate()

    def test_synthetic_complete_matching_cell_passes_without_running_tools(self):
        with patch.object(verify.subprocess, 'run', side_effect=AssertionError('validator must not execute tools')):
            self.assertIsNone(self.validate())

    def test_missing_new_context_and_mutating_or_dirty_observations_reject(self):
        for change in ('native_missing', 'display_missing', 'dirty', 'untracked', 'mutation', 'incomplete'):
            with self.subTest(change=change):
                before, after = copy.deepcopy(self.before), copy.deepcopy(self.after)
                if change == 'native_missing': del self.before['native_packages']
                if change == 'display_missing': del self.before['graphical']
                if change == 'dirty': self.before['checkout']['tracked_dirty'] = True
                if change == 'untracked': self.before['checkout']['untracked_present'] = True
                if change == 'mutation': self.after['tools']['cargo'] = 'changed'
                if change == 'incomplete': self.before['complete'] = False
                with self.assertRaises((adoption.EvidenceError, KeyError)): self.validate()
                self.before, self.after = before, after

    def test_wrong_run_checkout_cell_or_outside_job_observation_rejects(self):
        for obj, key, value in ((self.before['github'], 'run_id', 999), (self.before['github'], 'reported_sha', '0' * 40),
                                (self.before['labels'], 'profile', 'debug'), (self.before, 'recorded_at', '2026-10-16T00:00:00Z'),
                                (self.target['github'], 'event', 'workflow_dispatch'),
                                (self.target['checkout'], 'tree', '0' * 40), (self.identity, 'candidate', False)):
            with self.subTest(key=key, value=value):
                old = obj[key]; obj[key] = value
                with self.assertRaises(adoption.EvidenceError): self.validate()
                obj[key] = old

    def test_stable_bad_source_context_rejects_against_independent_identity(self):
        for area, key, value in (('github', 'run_id', 999), ('checkout', 'sha', '0' * 40),
                                 ('github', 'reported_sha', '0' * 40), ('github', 'job', 'different-job')):
            with self.subTest(area=area, key=key):
                old = self.before[area][key]
                self.before[area][key] = self.after[area][key] = value
                with self.assertRaises(adoption.EvidenceError): self.validate()
                self.before[area][key] = self.after[area][key] = old
        self.target['github']['run_id'] = 999
        with self.assertRaises(adoption.EvidenceError): self.validate()

    def test_wrong_target_environment_image_packages_and_external_binary_reject(self):
        for key in ('runner', 'tools', 'file_hashes', 'native_packages', 'graphical', 'build_test_environment'):
            with self.subTest(key=key):
                old = self.target[key]; self.target[key] = {'changed': True}
                with self.assertRaises((adoption.EvidenceError, KeyError)): self.validate()
                self.target[key] = old
        with patch.dict(os.environ, {'TOR_TEST_BINARIES': '/external/apps'}), self.assertRaises(adoption.EvidenceError):
            self.validate()

    def test_inherited_stage_environment_is_checked_beyond_explicit_overrides(self):
        row = self.summary['steps'][0]
        values = row['environment']['variables']
        values['PATH']['sha256'] = '0' * 64
        row['environment']['sha256'] = hashlib.sha256(json.dumps(values, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        # Existing invocation completeness checks only explicit stage overrides.
        with self.assertRaises(adoption.EvidenceError): self.validate()

    def test_failed_waived_missing_or_changed_stage_and_digest_reject(self):
        for key, value in (('gate_passed', False), ('clean_passed', False), ('scope', 'local'),
                           ('not_run_steps', ['missing']), ('waived_steps', ['waived']), ('jobs', 2),
                           ('test_workers', 1), ('planned_steps', [])):
            with self.subTest(key=key):
                old = self.summary[key]; self.summary[key] = value
                with self.assertRaises(adoption.EvidenceError): self.validate()
                self.summary[key] = old
        self.records['fixture-full-ci-release-rust/rust-release.log'] += b'changed'
        with self.assertRaises(adoption.EvidenceError): self.validate()

    def test_ambiguous_nonobject_and_nonfinite_documents_reject(self):
        for raw in (b'{"same":1,"same":2}', b'[]', b'{"value":NaN}', b'{"value":Infinity}', b'{"value":1e309}'):
            with self.subTest(raw=raw), self.assertRaises(adoption.EvidenceError): adoption.document(raw)

    def test_recomputed_receipt_requires_exact_json_types(self):
        receipt = self.summary['receipt_completeness']
        for key, value in (('schema_version', True), ('complete', 1), ('reporting_only', 1)):
            with self.subTest(key=key):
                old = receipt[key]; receipt[key] = value
                with self.assertRaises(adoption.EvidenceError): self.validate()
                receipt[key] = old

    def test_target_sizing_must_be_exact_bounded_integers(self):
        for value in (True, 0, 5, None, 4.0):
            old = self.target['execution_limits']['jobs']
            self.target['execution_limits']['jobs'] = value
            with self.subTest(value=value), self.assertRaises(adoption.EvidenceError): self.validate()
            self.target['execution_limits']['jobs'] = old

    def test_typed_schema_and_stage_outcome_are_required(self):
        self.summary['schema_version'] = True
        with self.assertRaises(adoption.EvidenceError): self.validate()
        self.summary['schema_version'] = 1
        self.summary['steps'][0]['status'] = 'skipped'
        with self.assertRaises(adoption.EvidenceError): self.validate()

    def test_artifact_log_paths_never_authorize_external_reads(self):
        self.summary['steps'][0]['log'] = '/untrusted/outside/record.log'
        # The original path is not an authority; the validated plan owns filenames.
        self.assertIsNone(self.validate())


if __name__ == '__main__':
    unittest.main()
