"""Reporting completeness cannot change execution or authorize reused CI."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import verify


class ReceiptCompleteness(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.logs = Path(directory.name)
        self.path = self.logs / 'python-debug-tests.jsonl'
        self.steps = [('python-debug', [sys.executable, str(verify.SCRIPTS / 'test_runner.py'),
                                       'discover', '-s', 'scripts'], {})]
        self.results = [{'step': 'python-debug', 'code': 0, 'note': '', 'log': self.logs / 'python-debug.log'}]
        self.context = {'command': self.steps[0][1], 'cwd': '.', 'environment_issues': [], 'waived_tests': [],
                        'environment': verify.ci_observation.build_test_environment(verify.verification_environment(1, {}), [])}
        self.context['environment']['scope'] = 'verifier_stage'
        self.results[0].update(copy.deepcopy(self.context))
        self.rows = [
            {'kind': 'selection', 'test_ids': ['sample.Case.test_a']},
            {'kind': 'test', 'test_id': 'sample.Case.test_a', 'outcome': 'passed',
             'failures': 0, 'errors': 0, 'subtests_failed': 0, 'subtests_skipped': 0},
            {'kind': 'suite', 'successful': True, 'tests_run': 1, 'failures': 0, 'errors': 0,
             'skipped': 0, 'waived': 0, 'expected_failures': 0, 'unexpected_successes': 0},
        ]

    def report(self, rows=None):
        rows = self.rows if rows is None else rows
        self.path.write_text(''.join(json.dumps({'schema_version': 1, 'profile': 'debug', **row}) + '\n' for row in rows))
        return verify.receipt_completeness(self.steps, self.results, self.logs, 1)

    def test_complete_receipt_links_existing_bytes_without_copying_test_ids(self):
        result = self.report()
        self.assertTrue(result['complete'], result)
        self.assertIs(result['reporting_only'], True)
        self.assertEqual(result['stages'][0]['record']['sha256'], hashlib.sha256(self.path.read_bytes()).hexdigest())
        self.assertNotIn('sample.Case.test_a', json.dumps(result))
        self.assertNotIn('adopt', json.dumps(result))

    def test_missing_duplicate_malformed_and_empty_selections_are_incomplete(self):
        variants = [self.rows[1:], [self.rows[0], *self.rows]] + [
                    [{**self.rows[0], 'test_ids': ids}, *self.rows[1:]]
                    for ids in ([], [''], [None], ['x', 'x'], ['  '], ['sample.\x00.test'], 'not a list')]
        for rows in variants:
            with self.subTest(rows=rows):
                self.assertFalse(self.report(rows)['complete'])

    def test_missing_or_mismatched_effective_stage_context_is_incomplete(self):
        for mutation in ('command', 'cwd', 'environment', 'environment_issues', 'scope', 'digest', 'override'):
            with self.subTest(mutation=mutation):
                self.results[0].update(copy.deepcopy(self.context))
                if mutation == 'command': self.results[0]['command'] = ['unexpected-command']
                elif mutation == 'cwd': self.results[0]['cwd'] = 'elsewhere'
                elif mutation == 'environment': self.results[0].pop('environment')
                elif mutation == 'environment_issues': self.results[0]['environment_issues'] = ['missing']
                elif mutation == 'scope': self.results[0]['environment']['scope'] = 'observer_process'
                elif mutation == 'digest': self.results[0]['environment']['sha256'] = None
                else: self.results[0]['environment']['variables']['CARGO_PROFILE_DEV_DEBUG'] = {'status': 'absent'}
                self.assertFalse(self.report()['complete'])

    def test_missing_explicit_stage_accounting_fields_are_incomplete(self):
        for key in ('environment_issues', 'note', 'waived_tests'):
            with self.subTest(key=key):
                self.results[0].update(copy.deepcopy(self.context), note='')
                self.results[0].pop(key)
                self.assertFalse(self.report()['complete'])

    def test_missing_extra_duplicate_unknown_and_nonterminal_outcomes_are_incomplete(self):
        for mutate in ('missing', 'extra', 'duplicate', 'unknown', 'interrupted', 'terminal_missing',
                       'terminal_duplicate', 'after_terminal', 'count', 'subtest_skip', 'expected_failure', 'profile'):
            with self.subTest(mutate=mutate):
                rows = copy.deepcopy(self.rows)
                if mutate == 'missing': rows.pop(1)
                elif mutate == 'extra': rows[1]['test_id'] = 'foreign.Case.test_x'
                elif mutate == 'duplicate': rows.insert(1, rows[1].copy())
                elif mutate in ('unknown', 'interrupted'): rows[1]['outcome'] = mutate
                elif mutate == 'terminal_missing': rows.pop()
                elif mutate == 'terminal_duplicate': rows.append(rows[-1].copy())
                elif mutate == 'after_terminal': rows.append({'kind': 'diagnostic'})
                elif mutate == 'count': rows[-1]['tests_run'] = 2
                elif mutate == 'subtest_skip': rows[1]['subtests_skipped'] = 1
                elif mutate == 'profile': rows[1]['profile'] = 'release'
                else: rows[1]['outcome'] = 'expected_failure'
                self.assertFalse(self.report(rows)['complete'])

    def test_unreadable_oversized_and_malformed_records_never_raise_or_rewrite_exits(self):
        for contents in (b'{bad\n', b'{"kind":"suite","kind":"selection"}\n', b'[]\n',
                         b'{"kind":"suite","tests_run":NaN}\n', b'x' * (16 * 1024 * 1024 + 1)):
            with self.subTest(bytes=len(contents)):
                self.path.write_bytes(contents)
                report = verify.receipt_completeness(self.steps, self.results, self.logs, 1)
                self.assertFalse(report['complete'])
                self.assertEqual(self.results[0]['code'], 0)
        self.path.unlink()
        self.assertFalse(verify.receipt_completeness(self.steps, self.results, self.logs, 1)['complete'])

    def test_incomplete_diagnostic_timings_cannot_prevent_a_summary(self):
        for contents in ('{bad\n', '[]\n', '{"kind":"test","test_id":"x","seconds":"bad"}\n',
                         '{"kind":"test","test_id":"x","seconds":NaN}\n',
                         '{"kind":"test","test_id":"x","seconds":' + '1' * 1000 + '}\n'):
            with self.subTest(contents=contents):
                self.path.write_text(contents)
                self.assertEqual(verify.slowest_tests([self.path]), [])
                self.assertFalse(verify.receipt_completeness(self.steps, self.results, self.logs, 1)['complete'])

    def test_planned_and_executed_stages_must_match_and_failures_remain_failures(self):
        self.report()
        for results in ([], self.results * 2, [{**self.results[0], 'step': 'foreign'}],
                        [{**self.results[0], 'code': None}], [{**self.results[0], 'code': 1}],
                        [{**self.results[0], 'note': 'WAIVED local failure'}]):
            with self.subTest(results=results):
                before = copy.deepcopy(results)
                self.assertFalse(verify.receipt_completeness(self.steps, results, self.logs, 1)['complete'])
                self.assertEqual(results, before)
        self.assertFalse(verify.receipt_completeness([], [], self.logs, 1)['complete'])

    def test_service_receipt_preserves_exact_artifact_and_result_without_a_rust_inventory(self):
        command = [sys.executable, str(verify.SCRIPTS / 'service_contract_ci.py'), '--profile', 'release', '--workspace']
        log = self.logs / 'rust-release.log'
        artifact = {'executable': '/owned/tor-service-contract-host', 'sha256': 'a' * 64,
                    'profile': 'release', 'features': ['service-contract-host']}
        result = {'selected': 13, 'run': 13, 'failures': 0, 'errors': 0, 'skipped': 0}
        steps = [('rust-release', command, {})]
        results = [{**copy.deepcopy(self.context), 'step': 'rust-release', 'code': 0, 'note': '', 'log': log, 'command': command}]
        for variation in ('valid', 'missing', 'duplicate', 'profile', 'hash', 'count', 'skipped', 'extra', 'features'):
            with self.subTest(variation=variation):
                a, r = copy.deepcopy(artifact), copy.deepcopy(result)
                if variation == 'profile': a['profile'] = 'debug'
                if variation == 'hash': a['sha256'] = 'not a digest'
                if variation == 'count': r['run'] -= 1
                if variation == 'skipped': r['skipped'] = 1
                if variation == 'extra': a['raw_log'] = 'Do not copy arbitrary fields'
                if variation == 'features': a['features'] *= 65
                lines = ['ordinary Cargo/libtest output\n', 'SERVICE_CONTRACT_ARTIFACT ' + json.dumps(a) + '\n']
                if variation != 'missing': lines.append('SERVICE_CONTRACT_RESULT ' + json.dumps(r) + '\n')
                if variation == 'duplicate': lines.append(lines[-1])
                log.write_text(''.join(lines))
                report = verify.receipt_completeness(steps, results, self.logs, 1)
                self.assertEqual(report['complete'], variation == 'valid', report)
                if variation == 'valid':
                    self.assertEqual(report['stages'][0]['service_artifact'], artifact)
                    self.assertEqual(report['stages'][0]['service_result'], result)
                    self.assertNotIn('rust_test_ids', report)

    def test_actual_toy_stage_keeps_selection_order_exit_and_effective_environment(self):
        root = self.logs / 'source'
        root.mkdir()
        (root / 'test_sample.py').write_text('import unittest\nclass Sample(unittest.TestCase):\n'
                                           ' def test_a(self): pass\n def test_b(self): pass\n')
        for workers in (1, 2):
            name = 'toy-' + str(workers)
            command = [sys.executable, str(verify.SCRIPTS / 'test_runner.py'), '--workers', str(workers), 'discover', '-s', '.']
            overrides = {'TOR_TEST_PROFILE': 'release'}
            with patch.object(verify, 'ROOT', root):
                result = verify.run_step(name, command, dict(overrides), self.logs, 2, False)
                report = verify.receipt_completeness([(name, command, overrides)], [result], self.logs, 2)
            self.assertEqual(result['code'], 0)
            self.assertEqual(result['command'], command)
            self.assertTrue(report['complete'], report)
            self.assertEqual(report['stages'][0]['selected_tests'], 2)
            self.assertNotIn(str(root), json.dumps(result['environment']))


if __name__ == '__main__':
    unittest.main()
