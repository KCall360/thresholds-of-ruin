"""Structured failure locations never turn setup or cleanup into acceptance."""
import io
import unittest
from unittest.mock import patch

import test_runner


class FailureContexts(unittest.TestCase):
    def records(self, phase):
        class Example(unittest.TestCase):
            def setUp(self):
                if phase == 'setup': self.fail('setup')
                if phase == 'cleanup': self.addCleanup(self.fail, 'cleanup')

            def test_body(self):
                if phase == 'method': self.assertEqual(1, 2)
                if phase == 'subtest':
                    with self.subTest(value=1): self.assertEqual(1, 2)

            def tearDown(self):
                if phase == 'teardown': self.fail('teardown')
        rows = []
        test_runner.TimedRunner(stream=io.StringIO(), sink=rows.append).run(
            unittest.defaultTestLoader.loadTestsFromTestCase(Example))
        return rows

    def test_method_assertion_has_structured_phase_and_project_location(self):
        rows = self.records('method')
        diagnostic = next(row for row in rows if row['kind'] == 'diagnostic')
        self.assertEqual(diagnostic.get('failure_context', {}).get('phase'), 'method')
        source = diagnostic['failure_context']['source']
        self.assertEqual(source['path'], 'scripts/test_runner_failure_diagnostics.py')
        self.assertTrue(source['function'].endswith('Example.test_body'))
        self.assertGreater(source['line'], 0)
        self.assertEqual(rows[-1]['failures'], 1)

    def test_fixture_assertions_have_distinct_phases(self):
        for phase in ('setup', 'teardown', 'cleanup'):
            with self.subTest(phase=phase):
                rows = self.records(phase)
                diagnostic = next(row for row in rows if row['kind'] == 'diagnostic')
                self.assertEqual(diagnostic.get('failure_context', {}).get('phase'), phase)
                self.assertEqual(rows[-1]['failures'], 1)

    def test_mutated_lifecycle_marker_does_not_disrupt_original_failure(self):
        original = unittest.TestCase._callTestMethod
        def alternate(instance, method): original(instance, method)
        with patch.object(unittest.TestCase, '_callTestMethod', alternate):
            rows = self.records('method')
        diagnostic = next(row for row in rows if row['kind'] == 'diagnostic')
        self.assertEqual(diagnostic['failure_context']['phase'], 'method')
        self.assertEqual(rows[-1]['failures'], 1)
        with patch.object(test_runner.Path, 'resolve', side_effect=RuntimeError('unsupported filename')):
            rows = self.records('method')
        diagnostic = next(row for row in rows if row['kind'] == 'diagnostic')
        self.assertEqual(diagnostic['failure_context'], {'phase': 'unknown', 'source': None})
        self.assertEqual(rows[-1]['failures'], 1)

    def test_subtest_remains_explicit_and_cannot_hide_in_method_counts(self):
        rows = self.records('subtest')
        diagnostic = next(row for row in rows if row['kind'] == 'diagnostic')
        self.assertEqual(diagnostic.get('failure_context', {}).get('phase'), 'unknown')
        self.assertIn('subtest_id', diagnostic)
        outcome = next(row for row in rows if row['kind'] == 'test')
        self.assertEqual(outcome['subtests_failed'], 1)


if __name__ == '__main__':
    unittest.main()
