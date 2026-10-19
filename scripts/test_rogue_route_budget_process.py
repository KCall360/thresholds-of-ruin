"""An ordinary paid intention at the exact route bound is retained, not retried."""
import os
from pathlib import Path
import unittest

from arena_evaluation import digest
from process_harness import ProcessTestCase, ROOT
from rogue_activation_candidate import create_candidate
from rogue_earned_route_process import RouteExecution, compress_completed_logs, prepare_campaign, source_snapshot


class RogueRouteBudgetProcesses(ProcessTestCase):
    def setUp(self):
        super().setUp()
        if retained := os.environ.get('TOR_EARNED_ROUTE_ARTIFACTS'):
            self.directory = Path(retained) / self._testMethodName
            self.directory.mkdir(parents=True, exist_ok=False)
            root = self.directory
            self.addCleanup(lambda: [compress_completed_logs(path.parent)
                for path in root.rglob('attempt.json') if 'segments' not in path.relative_to(root).parts])

    def test_exact_budget_preserves_paused_paid_work_without_submitting_a_resume(self):
        directory = self.directory / 'campaign'
        candidate = self.directory / 'candidate-source'
        compiler = self.bin / ('tor-scenario' + self.suffix)
        create_candidate(candidate, compiler, source_package=ROOT / 'scenarios/rogue-exploration',
            source_provenance={'reference': 'repository:scenarios/rogue-exploration',
                               'kind': 'current_production_package'}, start_depth=1)
        snapshot = source_snapshot(candidate / 'package', self.directory / 'candidate-snapshot',
            compiler, reference='Generated eighteen-species source: ' + digest(candidate / 'inputs.json'))
        arm_sources = {'production': ROOT / 'scripts/fixtures/rogue-earned-baseline-v15',
                       'candidate': snapshot}
        prepare_campaign(directory, self.bin, arm_sources=arm_sources)
        # Revision3 seed42 ordinarily pauses command269; its first resume would
        # be command270. This fixed acceptance boundary is a controller fixture,
        # never a runtime private-state oracle or a replacement measurement seed.
        route = RouteExecution(self, directory / 'paid-boundary', actor=1, seed=42,
            package=directory / 'inputs/production/package', first_limit=269,
            require_diagnostics=True)
        report = route.execute(prefix_commands=128)
        while (report['outcome'] == 'acceptance_prefix_saved'
                and report.get('ordinary_evidence_status') == 'complete'
                and report['evidence_status'] != 'failed'):
            route.compress_logs()
            report = route.resume(prefix_commands=report['controller']['commands'] + 128)
        route.compress_logs()
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
