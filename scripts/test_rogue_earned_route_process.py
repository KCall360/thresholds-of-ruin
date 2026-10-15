"""Actual ordinary-process qualification of the bounded earned-route adapter.

Fixed acceptance seed42 is separate from the eight original seed0/1 slots.
Failures, censored routes and unavailable diagnostic/replay gates remain explicit.
"""
import os
from pathlib import Path
import unittest

from process_harness import ProcessTestCase
from rogue_earned_route_process import (
    RouteExecution, compare_saved_initial_execution, compress_completed_logs, prepare_campaign,
)


class RogueEarnedRouteProcesses(ProcessTestCase):
    def setUp(self):
        super().setUp()
        if retained := os.environ.get('TOR_EARNED_ROUTE_ARTIFACTS'):
            self.directory = Path(retained) / self._testMethodName
            self.directory.mkdir(parents=True, exist_ok=False)
            root = self.directory
            self.addCleanup(lambda: [compress_completed_logs(path.parent)
                for path in root.rglob('attempt.json')])

    def campaign(self):
        directory = self.directory / 'campaign'
        inventory = prepare_campaign(directory, self.bin)
        self.assertEqual(len(inventory['slots']), 8)
        self.assertTrue(all(row['evidence_status'] == 'unrun' for row in inventory['slots']))
        return directory

    def test_zero_budget_is_censored_and_all_eight_measurement_slots_stay_unrun(self):
        directory = self.campaign()
        report = RouteExecution(self, directory / 'zero', actor=1, seed=42,
            package=directory / 'inputs/production/package', first_limit=0,
            require_diagnostics=True).execute()
        self.assertEqual(report['outcome'], 'censored_action_budget', report.get('failure'))
        self.assertEqual(report.get('ordinary_evidence_status'), 'complete', report.get('failure'))
        self.assertEqual(report['controller']['commands'], 0)
        self.assertEqual(report['evidence_status'], 'incomplete')

    def test_candidate_mage_first_purchase_is_ordinary_and_budget_one_censored(self):
        directory = self.campaign()
        report = RouteExecution(self, directory / 'candidate-mage', actor=2, seed=42,
            package=directory / 'inputs/candidate/package', arm='candidate', first_limit=1,
            require_diagnostics=True).execute()
        self.assertEqual(report['outcome'], 'censored_action_budget', report.get('failure'))
        self.assertEqual(report.get('ordinary_evidence_status'), 'complete', report.get('failure'))
        self.assertEqual(report['overview']['points'], {'earned': 1, 'spent': 1, 'unspent': 0})
        self.assertEqual(report['controller']['commands'], 1)
        self.assertEqual(report['state']['observation']['combat']['own_stats']['hit_dice']['total'], 1)

    def test_fixed_seed42_earned_hd_reexecutes_with_exact_source_free_restore_and_gross_flows(self):
        directory = self.campaign()
        original = RouteExecution(self, directory / 'known-path-42', actor=1, seed=42,
            package=directory / 'inputs/production/package', require_diagnostics=True)
        report = original.execute()
        self.assertEqual(report['outcome'], 'earned_hd2', report.get('failure') or report['outcome'])
        self.assertEqual(report.get('ordinary_evidence_status'), 'complete', report.get('failure'))
        self.assertEqual(len(report['earned_hit_dice_history']), 2)
        self.assertGreater(int(report['overview']['growth']['earned_xp']), 0)
        self.assertEqual(report['overview']['points']['spent'], 1)
        repeated = RouteExecution(self, directory / 'repeated-42', actor=1, seed=42,
            package=directory / 'inputs/production/package', require_diagnostics=True)
        prefix = repeated.execute(prefix_commands=12, initial_save=original.output / 'initial.db')
        self.assertEqual(prefix['outcome'], 'acceptance_prefix_saved', prefix.get('failure'))
        self.assertEqual(prefix.get('ordinary_evidence_status'), 'complete', prefix.get('failure'))
        replayed = repeated.resume(delete_source=True)
        self.assertEqual(replayed['outcome'], 'earned_hd2', replayed.get('failure'))
        self.assertEqual(replayed.get('ordinary_evidence_status'), 'complete', replayed.get('failure'))
        self.assertEqual(replayed['source_free_restart']['status'], 'passed')
        comparison = compare_saved_initial_execution(report, replayed)
        self.assertTrue(comparison['equal'], comparison['differences'][:10])
        self.assertFalse(comparison['raw_equal'], 'New history/request envelopes should remain visible')


if __name__ == '__main__':
    unittest.main()
