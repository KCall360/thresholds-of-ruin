"""Deterministic paid-tail budget contract, not a production survival measurement.

The authored status fixture supplies a zero-damage pause through ordinary clients.
Full production/candidate route and replay coverage lives in the earned-route
owners; no particular exploration policy revision must pause at a lucky count.
"""
import copy
import json
import os
from pathlib import Path
import time
import unittest
from unittest.mock import patch

from process_harness import ProcessTestCase, headless_fight_completion, resource
from rogue_earned_route_process import RouteExecution, compress_completed_logs, read_authoritative_evidence
from status_condition_package import LAPSE, STATUS_ABILITIES, WORK, combat, observation, prepare


def ability(frame, identity, target=None):
    return {'type': 'use_ability', 'id': identity,
            'invocation': {'aim': {'kind': 'actor_target',
                'target': target or observation(frame)['self_target']},
                'weapon': {'kind': 'default'}},
            'supplies': {'kind': 'carried'}}


class PendingWorkExecution(RouteExecution):
    """Only action selection is fixture-specific; enforcement/retention are real."""
    def run_steps(self, *, prefix_commands=None):
        self.settle(self.current)


class RogueRouteBudgetProcesses(ProcessTestCase):
    def setUp(self):
        super().setUp()
        if retained := os.environ.get('TOR_EARNED_ROUTE_ARTIFACTS'):
            self.directory = Path(retained) / self._testMethodName
            self.directory.mkdir(parents=True, exist_ok=False)
            root = self.directory
            self.addCleanup(lambda: [compress_completed_logs(path.parent)
                for path in root.rglob('attempt.json') if 'segments' not in path.relative_to(root).parts])

    def snapshot(self, player):
        return self.request(player, {'type': 'snapshot'})

    def active_work(self, route, current, identity):
        def active(frame):
            return (frame.get('state') is not None and not frame.get('state_omitted', False)
                    and combat(frame)['preparation_active']
                    and any(row['intention'] == identity and row['phase'] == 'started'
                            for row in frame.get('intentions', [])))
        return current if active(current) else route.frame(route.player, active)

    def paid_pause(self, limit):
        package = prepare(self, name='route-budget-paid-tail', prefix=False, peer=True)
        output = self.directory / 'paid-boundary'
        route = PendingWorkExecution(self, output, actor=1, seed=42, package=package,
            first_limit=limit, require_diagnostics=True, arm='authored_paid_tail_diagnostic')
        output.mkdir()
        self.directory, self.save = output, output / 'game.db'
        route.started = time.monotonic()
        route.report.update(evidence_status='running', fixture_scope='authored-paid-tail-budget',
                            fixture_peer_commands=[])
        route.start_transport()
        route.server = route.start_server('--checkpoint-interval', 1, scenario=package,
            character=1, save_actor=1, seed=42,
            extra_env={'TOR_SURVIVAL_DIAGNOSTICS_LOG': str(output / 'survival.jsonl')})
        route.connect_player()
        route.retain_initial_boundary()
        peer, _ = self.client(actor=2)
        initial = copy.deepcopy(route.current)
        route.phase = 'fixture_paid_work'
        started = route.issue({'type': 'act', 'action': ability(initial, WORK)}, kind='action')
        identity = started['intentions'][0]['intention']
        started = self.active_work(route, started, identity)
        self.assertTrue(combat(started)['preparation_active'])
        source = self.snapshot(peer)
        target = next(row['id'] for row in observation(source)['visible_actors']
                      if row['name'] == 'first subject')
        action = ability(source, STATUS_ABILITIES[LAPSE], target)
        route.report['fixture_peer_commands'].append(action)
        applied = self.act(peer, action)
        self.assertIsNone(applied['error'])
        if not headless_fight_completion(applied):
            self.frame(peer, headless_fight_completion)
        paused = route.issue({'type': 'request', 'request': {'type': 'snapshot'}})
        self.assertFalse(combat(paused)['preparation_active'])
        self.assertEqual(paused['intentions'][0]['phase'], 'paused')
        # Wait for public eligibility, not a stored deadline/private clock oracle.
        for _ in range(8):
            if identity in paused['readiness']['resume']:
                break
            action = {'type': 'wait'}
            route.report['fixture_peer_commands'].append(action)
            waited = self.act(peer, action)
            self.assertIsNone(waited['error'])
            paused = route.issue({'type': 'request', 'request': {'type': 'snapshot'}})
        self.assertIn(identity, paused['readiness']['resume'])
        self.assertEqual(paused['intentions'][0]['intention'], identity)
        self.assertEqual(paused['intentions'][0]['phase'], 'paused')
        self.assertFalse(combat(paused)['preparation_active'])
        self.assertGreater(int(combat(paused)['preparation_remaining']), 0)
        self.assertFalse(combat(paused)['terminal'])
        self.assertFalse(paused['state']['wizard_game'])
        self.assertEqual(combat(paused)['hp'], combat(initial)['hp'])
        self.assertEqual(resource(paused, 'stamina')['balance'], resource(initial, 'stamina')['balance'] - 1)
        self.assertEqual(resource(paused, 'stamina')['reserved'], 2)
        self.assertEqual(observation(paused)['inventory'], observation(initial)['inventory'])
        self.assertEqual(route.controller.commands, 1)
        self.assertEqual(route.controller.command_kinds, {'action': 1})
        route.persist()
        return route, paused

    def test_exact_budget_preserves_paused_paid_work_without_submitting_a_resume(self):
        route, paused = self.paid_pause(1)
        before = route.barrier()
        boundary = copy.deepcopy(route.current)
        with patch.object(route.player, 'write', wraps=route.player.write) as writes:
            route.run_loop()
        submitted = [json.loads(call.args[0]) for call in writes.call_args_list]
        self.assertTrue(submitted, 'The unchanged final save barrier must really be sent')
        self.assertTrue(all(row == {'type': 'request', 'request': {'type': 'save'}} for row in submitted), submitted)
        self.assertEqual(route.report['outcome'], 'censored_action_budget')
        self.assertEqual(route.report['unsubmitted_budget_command'], {'type': 'resume_intention'})
        self.assertEqual(route.controller.commands, route.controller.first_limit)
        self.assertEqual(route.controller.command_kinds, {'action': 1})
        self.assertEqual(read_authoritative_evidence(self.save, 1), before)
        for field in ('state', 'history', 'intentions'):
            self.assertEqual(route.current[field], boundary[field], field)
        self.assertEqual(combat(route.current)['preparation_remaining'], combat(paused)['preparation_remaining'])
        route.persist()
        route.finalize_segment()
        report = json.loads((route.output / 'attempt.json').read_text())
        self.assertEqual(report['outcome'], 'censored_action_budget')
        self.assertEqual(report['ordinary_evidence_status'], 'complete', report.get('failure'))
        self.assertNotEqual(report['evidence_status'], 'failed', report.get('failure'))
        self.assertNotIn('failure', report)
        self.assertGreater(report['flows']['actors']['1']['holds_acquired']['stamina'], 0)
        self.assertGreater(report['authoritative_anchor']['history_sequence'], 0)
        commands = [json.loads(line) for line in (route.output / 'commands.jsonl').read_text().splitlines()]
        self.assertEqual([row['kind'] for row in commands if row['kind'] != 'read'], ['action'])
        self.assertFalse(any(row['command']['type'] == 'resume_intention' for row in commands))

    def test_remaining_budget_submits_one_resume_for_the_same_live_paid_tail(self):
        route, paused = self.paid_pause(2)
        identity = paused['intentions'][0]['intention']
        resumed = route.issue({'type': 'resume_intention'}, kind='resume')
        self.assertIsNone(resumed['error'])
        resumed = self.active_work(route, resumed, identity)
        self.assertEqual(resumed['intentions'][0]['intention'], identity)
        self.assertTrue(combat(resumed)['preparation_active'])
        self.assertEqual(route.controller.commands, 2)
        self.assertEqual(route.controller.command_kinds, {'action': 1, 'resume': 1})
        self.assertEqual(resource(resumed, 'stamina'), resource(paused, 'stamina'))
        self.assertEqual(observation(resumed)['inventory'], observation(paused)['inventory'])
        route.report['outcome'] = 'fixture_resume_admitted'
        route.finish_evidence()
        route.persist()
        route.finalize_segment()
        self.assertEqual(route.report['ordinary_evidence_status'], 'complete', route.report.get('failure'))
        self.assertNotIn('failure', route.report)


if __name__ == '__main__':
    unittest.main()
