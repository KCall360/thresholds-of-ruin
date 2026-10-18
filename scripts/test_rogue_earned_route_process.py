"""Actual ordinary-process qualification of the bounded earned-route adapter.

Fixed acceptance seed42 is separate from the eight original seed0/1 slots.
Failures, censored routes and unavailable diagnostic/replay gates remain explicit.
"""
from scalar_wire import u64
import os
from pathlib import Path
import unittest

from arena_evaluation import write_json, digest
from process_harness import ProcessTestCase, ROOT
from rogue_activation_candidate import create_candidate
from rogue_earned_route_process import (
    RouteExecution, compare_saved_initial_execution, compress_completed_logs, prepare_campaign, source_snapshot,
)


class RogueEarnedRouteProcesses(ProcessTestCase):
    def setUp(self):
        super().setUp()
        if retained := os.environ.get('TOR_EARNED_ROUTE_ARTIFACTS'):
            self.directory = Path(retained) / self._testMethodName
            self.directory.mkdir(parents=True, exist_ok=False)
            root = self.directory
            self.addCleanup(lambda: [compress_completed_logs(path.parent)
                for path in root.rglob('attempt.json') if 'segments' not in path.relative_to(root).parts])

    def campaign(self):
        directory = self.directory / 'campaign'
        candidate = self.directory / 'candidate-source'
        compiler = self.bin / ('tor-scenario' + self.suffix)
        create_candidate(candidate, compiler, source_package=ROOT / 'scenarios/rogue-exploration',
            source_provenance={'reference': 'repository:scenarios/rogue-exploration',
                               'kind': 'current_production_package'}, start_depth=1)
        snapshot = source_snapshot(candidate / 'package', self.directory / 'candidate-snapshot',
            compiler, reference='Generated eighteen-species source: ' + digest(candidate / 'inputs.json'))
        arm_sources = {'production': ROOT / 'scripts/fixtures/rogue-earned-baseline-v9',
                       'candidate': snapshot}
        inventory = prepare_campaign(directory, self.bin, arm_sources=arm_sources)
        self.assertEqual(len(inventory['slots']), 8)
        self.assertTrue(all(row['evidence_status'] == 'unrun' for row in inventory['slots']))
        return directory

    def complete_prefixes(self, route, report):
        while (report['outcome'] == 'acceptance_prefix_saved'
                and report.get('ordinary_evidence_status') == 'complete'
                and report['evidence_status'] != 'failed'):
            route.compress_logs()
            report = route.resume(prefix_commands=report['controller']['commands'] + 128)
        route.compress_logs()
        return report

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
        self.assertEqual(report['overview']['points'], {'earned': "1", 'spent': "1", 'unspent': "0"})
        self.assertEqual(report['controller']['commands'], 1)
        self.assertEqual(report['checkpoints']['1']['commands'], 0)
        self.assertEqual(report['checkpoints']['1']['state'], report['initial']['state'])
        self.assertEqual(report['checkpoints']['1']['overview']['points'],
                         {'earned': "1", 'spent': "0", 'unspent': "1"})
        self.assertEqual(u64(report['state']['observation']['combat']['own_stats']['hit_dice']['total']), 1)

    def test_fixed_seed42_earned_hd_reexecutes_with_exact_source_free_restore_and_gross_flows(self):
        directory = self.campaign()
        original = RouteExecution(self, directory / 'known-path-42', actor=1, seed=42,
            package=directory / 'inputs/production/package', require_diagnostics=True)
        report = self.complete_prefixes(original, original.execute(prefix_commands=128))
        self.assertEqual(report['outcome'], 'earned_hd2', report.get('failure') or report['outcome'])
        self.assertEqual(report.get('ordinary_evidence_status'), 'complete', report.get('failure'))
        self.assertEqual(len(report['earned_hit_dice_history']), 2)
        self.assertGreater(int(report['overview']['growth']['earned_xp']), 0)
        self.assertEqual(u64(report['overview']['points']['spent']), 1)
        repeated = RouteExecution(self, directory / 'repeated-42', actor=1, seed=42,
            package=directory / 'inputs/production/package', require_diagnostics=True)
        prefix = repeated.execute(prefix_commands=12, initial_save=original.output / 'initial.db')
        self.assertEqual(prefix['outcome'], 'acceptance_prefix_saved', prefix.get('failure'))
        self.assertEqual(prefix.get('ordinary_evidence_status'), 'complete', prefix.get('failure'))
        repeated.compress_logs()
        replayed = self.complete_prefixes(repeated, repeated.resume(delete_source=True,
            prefix_commands=prefix['controller']['commands'] + 128))
        self.assertEqual(replayed['outcome'], 'earned_hd2', replayed.get('failure'))
        self.assertEqual(replayed.get('ordinary_evidence_status'), 'complete', replayed.get('failure'))
        self.assertEqual(replayed['source_free_restart']['status'], 'passed')
        comparison = compare_saved_initial_execution(report, replayed)
        write_json(directory / 'comparison.json', comparison)
        self.assertTrue(comparison['equal'], comparison['differences'][:10])
        self.assertFalse(comparison['raw_equal'], 'New history/request envelopes should remain visible')


if __name__ == '__main__':
    unittest.main()
