"""Real playable clients: delayed delivery, gap rejection, snapshot on relaunch."""
import json
import unittest
import test_text_process as support
import test_headless_process as headless
import test_ascii_process as ascii_support
from test_adventure_process import AdventureProcess
from stream_relay import StreamRelay
from pathlib import Path


class StreamRecoveryProcesses(unittest.TestCase):
    setUpClass = classmethod(support.TextProcesses.setUpClass.__func__)
    setUp = headless.HeadlessProcesses.setUp
    server = headless.HeadlessProcesses.server
    launch = support.TextProcesses.launch
    client = headless.HeadlessProcesses.client
    frame = headless.HeadlessProcesses.frame
    command = headless.HeadlessProcesses.command
    request = headless.HeadlessProcesses.request
    ascii_frame = ascii_support.AsciiProcesses.frame

    def playable(self, kind, address):
        if kind == 'ascii':
            client = self.launch('tor-client-ascii', ['--connect', address, '--automation'], token=support.SPECTATOR_TOKEN)
            initial = self.ascii_frame(client, lambda f: f['state'] is not None and not f['busy'])
        else:
            client = AdventureProcess(self.bin / ('tor-client-text' + self.suffix), ['--connect', address], token=support.SPECTATOR_TOKEN)
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
            for _ in range(4):
                slow.until(lambda line: line == 'Time passes.')

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

    def test_other_actor_door_changes_use_disclosed_narration_in_both_clients(self):
        server = self.launch('tor-server', ['--listen', '127.0.0.1:0', '--save', self.save,
            '--scenario', Path(__file__).resolve().parents[1] / 'scenarios/tests/semantic-narration-setup', '--wizard'],
            extra_env={'TOR_WIZARD_TOKEN': headless.WIZARD_TOKEN,
                       'TOR_SPECTATOR_TOKEN': support.SPECTATOR_TOKEN})
        self.address = json.loads(server.until(lambda line: line.startswith('{')))['address']
        wizard = self.launch('tor-client-text', ['--connect', self.address], token=headless.WIZARD_TOKEN)
        wizard.until(lambda line: line == 'Ready.')
        wizard.command('release')
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
            sentence = 'The wooden door is now ' + ('open.' if opened else 'closed.')
            output = text.until(lambda line: line == sentence)
            self.assertNotIn('You open', output)
            self.assertNotIn('You close', output)
            shown = self.ascii_frame(window, lambda f: sentence in (f.get('narration') or []))
            self.assertFalse(shown['has_control'])
            self.assertIn('You notice a figure.' if opened else 'You can no longer see the figure.', shown['narration'])


if __name__ == '__main__':
    unittest.main()
