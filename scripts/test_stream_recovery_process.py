"""Real clients: delayed delivery, gap/invalid-state rejection, and relaunch."""
import unittest
from stream_relay import StreamRelay

from process_harness import ProcessTestCase, AdventureProcess, SPECTATOR_TOKEN


class StreamRecoveryProcesses(ProcessTestCase):
    def playable(self, kind, address):
        if kind == 'ascii':
            client = self.launch('tor-client-ascii', ['--connect', address, '--automation'], token=SPECTATOR_TOKEN)
            initial = self.ascii_frame(client, lambda f: f['state'] is not None and not f['busy'])
        else:
            client = AdventureProcess(self.bin / ('tor-client-text' + self.suffix), ['--connect', address], token=SPECTATOR_TOKEN)
            self.addCleanup(client.stop)
            initial = client.until(lambda line: line == '> ')
        return client, initial

    def exercise(self, kind):
        self.server()
        player, _ = self.client()
        relay = StreamRelay(self.address)
        self.addCleanup(relay.close)
        slow, _ = self.playable(kind, relay.address)
        relay.gate.clear()
        final = None
        for _ in range(4):
            final = self.command(player, {'type': 'act', 'action': {'type': 'wait'}})
            self.assertIsNone(final['error'])
        self.assertTrue(relay.held.wait(5), 'Relay did not hold an actual server update')
        # Server progress is established by acknowledgements while delivery is
        # stopped, rather than by a machine-specific latency threshold.
        self.assertEqual(final['state']['observation']['tick'], 400)
        if kind == 'ascii':
            slow.child.stdin.write('{"type":"key","key":"places"}\n')
            slow.child.stdin.flush()
            paused = self.ascii_frame(slow, lambda f: f['input_done'] == 'places')
            self.assertEqual(paused['state']['observation']['tick'], 0)
            self.assertTrue(paused['connected'])
            self.assertTrue(paused['places_open'])
        relay.gate.set()
        if kind == 'ascii':
            caught_up = self.ascii_frame(slow, lambda f: f['state']['observation']['tick'] == 400)
            self.assertEqual(caught_up['state'], final['state'])
            self.assertEqual(caught_up['history'], final['history'])
            self.assertEqual(caught_up['narration'], ['Time passes.'])
        else:
            # The four waits arrive together and are told as one passage.
            slow.until(lambda line: 'Time passes.' in line)

        relay.drop_observation.set()
        self.command(player, {'type': 'act', 'action': {'type': 'wait'}})
        self.assertTrue(relay.dropped.wait(5))
        final = self.command(player, {'type': 'act', 'action': {'type': 'wait'}})
        if kind == 'ascii':
            failed = self.ascii_frame(slow, lambda f: not f['connected'])
            self.assertIn('SequenceMismatch', failed['status'])
            self.assertEqual(failed['state']['observation']['tick'], 400)
        else:
            slow.until(lambda line: 'SequenceMismatch' in line)
        self.assertNotEqual(slow.child.wait(timeout=15), 0)

        replacement, initial = self.playable(kind, self.address)
        if kind == 'ascii':
            self.assertEqual(initial['state'], final['state'])
            self.assertEqual(initial['history'], final['history'])
            self.assertFalse(initial['has_control'])
            self.assertEqual(initial['narration'], [])
        else:
            replacement.child.stdin.write('history\n')
            replacement.child.stdin.flush()
            history = replacement.until(lambda line: line == '> ')
            for entry in final['history']:
                self.assertIn(entry['id'], history)
            self.assertIn('Spectator access is read-only', initial)
        self.assertIsNone(self.command(player, {'type': 'act', 'action': {'type': 'wait'}})['error'])

    def test_ascii_delayed_delivery_gap_and_relaunch(self):
        self.exercise('ascii')

    def test_text_delayed_delivery_gap_and_relaunch(self):
        self.exercise('text')

    def exercise_invalid_state(self, kind, corruption):
        self.server()
        player, _ = self.client()
        relay = StreamRelay(self.address)
        self.addCleanup(relay.close)
        spectator, initial = self.playable(kind, relay.address)
        getattr(relay, corruption).set()
        final = self.command(player, {'type': 'act', 'action': {'type': 'wait'}})
        self.assertIsNone(final['error'])
        self.assertTrue(relay.corrupted.wait(5), 'No actual delta was corrupted')
        if kind == 'ascii':
            failed = self.ascii_frame(spectator, lambda f: not f['connected'])
            self.assertIn('InconsistentState', failed['status'])
            self.assertEqual(failed['state'], initial['state'])
            self.assertEqual(failed['history'], initial['history'])
        else:
            spectator.until(lambda line: 'InconsistentState' in line)
        self.assertNotEqual(spectator.child.wait(timeout=15), 0)
        replacement, recovered = self.playable(kind, self.address)
        if kind == 'ascii':
            self.assertEqual(recovered['state'], final['state'])
            self.assertEqual(recovered['history'], final['history'])
        else:
            replacement.child.stdin.write('history\n')
            replacement.child.stdin.flush()
            history = replacement.until(lambda line: line == '> ')
            for entry in final['history']:
                self.assertIn(entry['id'], history)
        self.assertIsNone(self.command(player, {'type': 'act', 'action': {'type': 'wait'}})['error'])

    def test_ascii_overflowing_delta_and_relaunch(self):
        self.exercise_invalid_state('ascii', 'overflow_delta')

    def test_text_overflowing_delta_and_relaunch(self):
        self.exercise_invalid_state('text', 'overflow_delta')

    def test_ascii_invalid_reconstructed_inventory_and_relaunch(self):
        self.exercise_invalid_state('ascii', 'invalid_inventory')

    def test_text_invalid_reconstructed_inventory_and_relaunch(self):
        self.exercise_invalid_state('text', 'invalid_inventory')

    def test_other_actor_door_changes_use_disclosed_narration_in_both_clients(self):
        self.server(scenario='semantic-narration', seed=None)
        player, _ = self.client()
        other = self.launch('tor-client-headless', ['--connect', self.address, '--actor', '2'])
        other_state = self.frame(other, lambda f: f['type'] == 'ready')
        text, _ = self.playable('text', self.address)
        window, _ = self.playable('ascii', self.address)
        door = next(c['door']['id'] for c in other_state['state']['observation']['visible_cells'] if c['door'])
        for opened in (True, False):
            # Explicit barriers make turn handoff independent of which client's
            # socket/input task the OS schedules first.
            self.assertIsNone(self.request(player, {'type':'snapshot'})['error'])
            self.assertIsNone(self.command(player, {'type':'act','action':{'type':'wait'}})['error'])
            self.assertIsNone(self.request(other, {'type':'snapshot'})['error'])
            changed = self.command(other, {'type':'act','action':{'type':'set_door','door':door,'open':opened}})
            self.assertIsNone(changed['error'])
            told = 'The wooden door to the east swings ' + ('open.' if opened else 'shut.')
            output = text.until(lambda line: told in line)
            self.assertNotIn('You open', output)
            self.assertNotIn('You close', output)
            sentence = 'The wooden door is now ' + ('open.' if opened else 'closed.')
            shown = self.ascii_frame(window, lambda f: sentence in (f.get('narration') or []))
            self.assertFalse(shown['has_control'])
            self.assertIn('You notice a figure.' if opened else 'You can no longer see the figure.', shown['narration'])


if __name__ == '__main__':
    unittest.main()
