"""An ordinary paid intention at the exact route bound is retained, not retried."""
import os
from pathlib import Path
import unittest

from process_harness import ProcessTestCase
from rogue_earned_route_process import RouteExecution, compress_completed_logs, prepare_campaign


class RogueRouteBudgetProcesses(ProcessTestCase):
    def setUp(self):
        super().setUp()
        if retained := os.environ.get('TOR_EARNED_ROUTE_ARTIFACTS'):
            self.directory = Path(retained) / self._testMethodName
            self.directory.mkdir(parents=True, exist_ok=False)
            root = self.directory
            self.addCleanup(lambda: [compress_completed_logs(path.parent)
                for path in root.rglob('attempt.json')])

    def test_exact_budget_preserves_paused_paid_work_without_submitting_a_resume(self):
        directory = self.directory / 'campaign'
        prepare_campaign(directory, self.bin)
        # Revision3 seed42 ordinarily pauses command269; its first resume would
        # be command270. This fixed acceptance boundary is a controller fixture,
        # never a runtime private-state oracle or a replacement measurement seed.
        route = RouteExecution(self, directory / 'paid-boundary', actor=1, seed=42,
            package=directory / 'inputs/production/package', first_limit=269,
            require_diagnostics=True)
        report = route.execute()
        self.assertEqual(report['outcome'], 'censored_action_budget', report.get('failure'))
        self.assertEqual(report.get('ordinary_evidence_status'), 'complete', report.get('failure'))
        self.assertEqual(report['controller']['commands'], 269)
        self.assertEqual(report['controller']['command_kinds'].get('resume', 0), 0)
        combat = report['state']['observation']['combat']
        self.assertIsNotNone(combat['preparation_remaining'])
        self.assertFalse(combat['preparation_active'])
        self.assertFalse(combat['terminal'])
        self.assertIsNone(report.get('failure'))
        self.assertGreater(report['flows']['actors']['1']['holds_acquired']['stamina'], 0)
        self.assertTrue(report['authoritative_anchor']['history_sequence'] > 0)


if __name__ == '__main__':
    unittest.main()
