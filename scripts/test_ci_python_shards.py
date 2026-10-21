"""Exact release inventory and fail-closed two-owner coverage contracts."""
from collections import Counter
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

import ci_python_shards as shards
import parallel_tests

SHA = 'a' * 40
RUN = '12345'
IDS = ['test_rogue_earned_route_process.Route.test_one',
       'test_arena_matrix_process.Matrix.test_two',
       'test_performance_process.Native.test_three',
       'test_floor_activation_process.Omitted.test_four',
       'test_future_new_process.Future.test_five']


def evidence():
    receipts, summaries = {}, {}
    for index in (1, 2):
        selected = shards.selection(IDS, index)
        rows = [{'kind': 'selection', 'test_ids': selected,
                 'shard': shards.metadata(IDS, index, SHA, RUN)}]
        rows += [{'kind': 'test', 'test_id': name, 'outcome': 'passed',
                  'failures': 0, 'errors': 0, 'subtests_failed': 0, 'subtests_skipped': 0}
                 for name in selected]
        rows += [{'kind': 'suite', 'successful': True, 'tests_run': len(selected),
                  'failures': 0, 'errors': 0, 'skipped': 0, 'waived': 0, 'expected_failures': 0,
                  'unexpected_successes': 0, 'waived_tests': [], 'not_started': [], 'batch_errors': []}]
        receipts[index] = [dict(schema_version=1, profile='release', **row) for row in rows]
        summaries[index] = {'ci_profile': 'release', 'ci_partition': 'python', 'ci_python_shard': index,
                            'clean_passed': True, 'gate_passed': True, 'evidence_origin': 'fresh',
                            'receipt_completeness': {'complete': True}, 'waived_steps': [], 'not_run_steps': [],
                            'planned_steps': ['build-process-release', 'process-release'],
                            'steps': [{'step': name, 'status': 'passed', 'code': 0}
                                      for name in ('build-process-release', 'process-release')]}
    return receipts, summaries


class PythonShardSelectionTests(unittest.TestCase):
    def test_explicit_modules_and_future_modules_have_one_owner(self):
        left, right = shards.selection(IDS, 1), shards.selection(IDS, 2)
        self.assertEqual(left, IDS[:3])
        self.assertEqual(right, IDS[3:])
        self.assertFalse(set(left) & set(right))
        self.assertEqual(Counter(left + right), Counter(IDS))

    def test_empty_duplicate_loader_failure_and_invalid_shards_reject(self):
        for ids, index in [([], 1), (IDS[:3], 1), (IDS[3:], 2), (IDS + IDS[:1], 1),
                           (IDS + ['unittest.loader._FailedTest.bad_import'], 1), (IDS, True), (IDS, 3)]:
            with self.subTest(ids=ids, index=index), self.assertRaises(ValueError):
                shards.selection(ids, index)

    def test_actual_registered_release_multiset_and_fixture_owners_are_preserved(self):
        # Discover only: no setup, build, or process execution occurs here.
        root = Path(__file__).resolve().parent
        suite = unittest.TestLoader().discover(str(root), pattern='test_*process.py', top_level_dir=str(root))
        tests = list(parallel_tests.flatten(suite))
        ids = [test.id() for test in tests]
        left, right = shards.selection(ids, 1), shards.selection(ids, 2)
        self.assertGreater(len(ids), 600)
        self.assertEqual(Counter(left + right), Counter(ids))
        self.assertFalse(set(left) & set(right))
        ownership = {name: 1 for name in left} | {name: 2 for name in right}
        for module in {type(test).__module__ for test in tests}:
            owners = {ownership[test.id()] for test in tests if type(test).__module__ == module}
            self.assertEqual(len(owners), 1, module)
        # Every original exclusive fixture/native owner stays intact in its
        # own runner; native lanes on separate hosts cannot contend for focus.
        for cls in {type(test) for test in tests}:
            if parallel_tests.has_own_fixture(cls) or getattr(cls, 'graphical', False):
                self.assertEqual(len({ownership[test.id()] for test in tests if type(test) is cls}), 1)


class PythonShardCoverageTests(unittest.TestCase):
    def test_complete_current_identity_and_exact_union_pass(self):
        receipts, summaries = evidence()
        result = shards.validate_pair(receipts, summaries, source_sha=SHA, run_id=RUN, expected_ids=IDS)
        self.assertEqual(result, {'source_sha': SHA, 'run_id': RUN, 'shards': 2,
                                 'selected_tests': 5, 'completed_tests': 5, 'complete': True})

    def test_absent_empty_cancelled_failed_incomplete_or_wrong_identity_fail(self):
        for mutation in ('absent', 'empty', 'cancelled', 'failed', 'duplicate', 'missing_test',
                         'wrong_source', 'wrong_run', 'wrong_shard', 'different_full', 'overlap',
                         'summary_failed', 'summary_missing_stage', 'adopted', 'skip', 'waived_count', 'waived_bool', 'wrong_profile'):
            with self.subTest(mutation=mutation):
                receipts, summaries = evidence()
                rows = receipts[2]
                if mutation == 'absent': del receipts[2]
                elif mutation == 'empty': receipts[2] = []
                elif mutation == 'cancelled': rows.pop()
                elif mutation == 'failed': rows[1]['outcome'] = 'failed'
                elif mutation == 'duplicate': rows.insert(1, deepcopy(rows[1]))
                elif mutation == 'missing_test': rows.pop(1)
                elif mutation == 'wrong_source': rows[0]['shard']['source_sha'] = 'b' * 40
                elif mutation == 'wrong_run': rows[0]['shard']['run_id'] = '2'
                elif mutation == 'wrong_shard': rows[0]['shard']['index'] = 1
                elif mutation == 'different_full': rows[0]['shard']['full_test_ids'] = IDS[:-1]
                elif mutation == 'overlap': rows[0]['test_ids'] = shards.selection(IDS, 1)
                elif mutation == 'summary_failed': summaries[2]['clean_passed'] = False
                elif mutation == 'summary_missing_stage': summaries[2]['receipt_completeness']['complete'] = False
                elif mutation == 'adopted': summaries[2]['evidence_origin'] = 'cross_head_composite'
                elif mutation == 'skip': rows[-1]['skipped'] = 1
                elif mutation == 'waived_count': rows[-1]['waived'] = 1
                elif mutation == 'waived_bool': rows[-1]['waived'] = False
                else: rows[1]['profile'] = 'debug'
                with self.assertRaises(ValueError):
                    shards.validate_pair(receipts, summaries, source_sha=SHA, run_id=RUN, expected_ids=IDS)

    def test_artifact_reader_requires_both_unambiguous_complete_files(self):
        receipts, summaries = evidence()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ValueError, 'Missing'):
                shards.read_pair(root, source_sha=SHA, run_id=RUN, expected_ids=IDS)
            for index in (1, 2):
                folder = root / str(index) / 'full-ci-release-python'
                folder.mkdir(parents=True)
                (folder / 'process-release-tests.jsonl').write_text(
                    '\n'.join(json.dumps(row) for row in receipts[index]), encoding='utf-8')
                raw = (folder / 'process-release-tests.jsonl').read_bytes()
                summaries[index]['receipt_completeness']['stages'] = [{'step': 'process-release',
                    'complete': True, 'record': {'file': 'process-release-tests.jsonl',
                    'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}}]
                (folder / 'summary.json').write_text(json.dumps(summaries[index]), encoding='utf-8')
            self.assertTrue(shards.read_pair(root, source_sha=SHA, run_id=RUN, expected_ids=IDS)['complete'])
            (root / '2' / 'summary.json').write_text('{}', encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'ambiguous'):
                shards.read_pair(root, source_sha=SHA, run_id=RUN, expected_ids=IDS)

    def test_joint_omission_cannot_replace_independent_current_source_inventory(self):
        receipts, summaries = evidence()
        omitted = IDS[-1]
        shorter = IDS[:-1]
        for index in (1, 2):
            receipts[index][0]['shard'] = shards.metadata(shorter, index, SHA, RUN)
            receipts[index][0]['test_ids'] = shards.selection(shorter, index)
            receipts[index] = [row for row in receipts[index] if row.get('test_id') != omitted]
            receipts[index][-1]['tests_run'] = len(shards.selection(shorter, index))
        with self.assertRaisesRegex(ValueError, 'current-source inventory'):
            shards.validate_pair(receipts, summaries, expected_ids=IDS, source_sha=SHA, run_id=RUN)

    def test_boolean_integer_aliases_and_failed_build_stage_are_not_evidence(self):
        for field in ('version', 'index', 'count', 'summary', 'counter', 'stage'):
            with self.subTest(field=field):
                receipts, summaries = evidence()
                if field in ('version', 'index', 'count'):
                    receipts[1][0]['shard'][field] = True
                elif field == 'summary': summaries[1]['ci_python_shard'] = True
                elif field == 'counter': receipts[1][1]['failures'] = False
                else: summaries[1]['steps'][0]['code'] = 1
                with self.assertRaises(ValueError):
                    shards.validate_pair(receipts, summaries, expected_ids=IDS, source_sha=SHA, run_id=RUN)
