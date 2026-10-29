"""Real clients: delayed delivery, atomic gap/invalid-state repair, and relaunch."""
import json
import unittest
from stream_relay import StreamRelay

from process_harness import ProcessTestCase, AdventureProcess, SPECTATOR_TOKEN


class StreamRecoveryFixture:
    def playable(self, kind, address):
        if kind == 'ascii':
            client = self.launch('tor-client-ascii', ['--connect', address, '--automation'], token=SPECTATOR_TOKEN)
            initial = self.ascii_frame(client, lambda f: f['state'] is not None and not f['busy'])
        else:
            client = self.launch('tor-client-text', ['--connect', address], cls=AdventureProcess, token=SPECTATOR_TOKEN)
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
        for direction in ('east', 'west', 'east', 'west'):
            final = self.act(player, {'type': 'move', 'direction': direction})
            self.assertIsNone(final['error'])
        self.assertTrue(relay.held.wait(5), 'Relay did not hold an actual server update')
        # Server progress is established by execution updates while delivery is
        # stopped, rather than by a machine-specific latency threshold.
        self.assertEqual(final['state']['observation']['tick'], '400')
        if kind == 'ascii':
            slow.child.stdin.write('{"type":"key","key":"places"}\n')
            slow.child.stdin.flush()
            paused = self.ascii_frame(slow, lambda f: f['input_done'] == 'places')
            self.assertEqual(paused['state']['observation']['tick'], '0')
            self.assertTrue(paused['connected'])
            self.assertTrue(paused['places_open'])
        relay.gate.set()
        if kind == 'ascii':
            caught_up = self.ascii_frame(slow, lambda f: f['state']['observation']['tick'] == '400')
            self.assertDisclosedStateEqual(caught_up['state'], final['state'])
            self.assertEqual(caught_up['history'], final['history'])
            self.assertIn('You move west.', caught_up['narration'])
        else:
            # Ordered movement outcomes survive coalesced display snapshots.
            passage = slow.until(lambda line: line == '> ')
            self.assertIn('east', passage)
            self.assertIn('west', passage)

        relay.repair_gate.clear()
        relay.drop_observation.set()
        before_gap = self.act(player, {'type': 'move', 'direction': 'east'})
        self.assertIsNone(before_gap['error'])
        self.assertTrue(relay.dropped.wait(5))
        # A dropped final picture is detected by the next ordered update, not
        # by elapsed time. Its valid earlier Outcome remains committed.
        self.assertIsNone(self.act(player, {'type': 'move', 'direction': 'west'})['error'])
        self.assertTrue(relay.repair_held.wait(5), 'Client did not request a repair snapshot')
        self.assert_frozen(kind, slow, initial=caught_up if kind == 'ascii' else None,
                           committed=before_gap)
        # Continue authoritative play while the single repair frame is held.
        final = self.act(player, {'type': 'move', 'direction': 'east'})
        self.assertIsNone(final['error'])
        self.assertEqual(final['state']['observation']['tick'], '700')
        relay.repair_gate.set()
        self.assert_recovered(kind, slow, final)
        self.assertEqual(relay.attachments, 1)
        self.assertEqual(relay.repairs, 1)
        slow.stop()

        replacement, initial = self.playable(kind, self.address)
        if kind == 'ascii':
            self.assertDisclosedStateEqual(initial['state'], final['state'])
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
        self.assertIsNone(self.act(player, {'type': 'move', 'direction': 'east'})['error'])


    def assert_frozen(self, kind, client, *, initial, committed):
        self.assertIsNone(client.child.poll(), 'Recoverable stream error killed the client')
        if kind == 'ascii':
            # A local presentation event makes a frame without sending gameplay.
            client.child.stdin.write('{"type":"key","key":"places"}\n')
            client.child.stdin.flush()
            held = self.ascii_frame(client, lambda f: f['input_done'] == 'places')
            self.assertTrue(held['connected'])
            self.assertEqual(held['state'], initial['state'])
            # The valid Outcome preceding the rejected picture is not rolled back.
            self.assertEqual(held['history'], committed['history'])
        else:
            # The repair is held at the relay. An actual local command creates
            # an output barrier and must present the recovery refusal, never
            # the corrupt item names or a new simulated passage.
            client.write('inventory')
            refused = False
            def refusal_prompt(line):
                nonlocal refused
                refused |= 'Resynchronizing; input was not sent.' in line
                return refused and line == '> '
            held = client.until(refusal_prompt)
            self.assertIn('Resynchronizing; input was not sent.', held)
            for forbidden in ('invalid test fixture', 'duplicate inventory fixture',
                              'inserted test item', 'Time passes.'):
                self.assertNotIn(forbidden, held)


    def assert_recovered(self, kind, client, final):
        if kind == 'ascii':
            recovered = self.ascii_frame(client, lambda f:
                f['connected'] and f['state']['observation'] == final['state']['observation'] and f['history'] == final['history'])
            self.assertDisclosedStateEqual(recovered['state'], final['state'])
            self.assertFalse(recovered['has_control'])
        else:
            client.until(lambda line: line == '> ')
            client.child.stdin.write('history\n')
            client.child.stdin.flush()
            history = client.until(lambda line: line == '> ')
            for entry in final['history']:
                self.assertIn(entry['id'], history)
        self.assertIsNone(client.child.poll(), 'Repair must retain the original client process')


    def exercise_invalid_state(self, kind, corruption):
        self.server()
        player, _ = self.client()
        relay = StreamRelay(self.address, large_retained_base=corruption == 'oversized_retained_state')
        self.addCleanup(relay.close)
        spectator, initial = self.playable(kind, relay.address)
        relay.corrupted.clear()
        relay.repair_gate.clear()
        getattr(relay, corruption).set()
        final = self.act(player, {'type': 'move', 'direction': 'east'})
        self.assertIsNone(final['error'])
        self.assertTrue(relay.corrupted.wait(5), 'No actual delta was corrupted')
        self.assertTrue(relay.repair_held.wait(5), 'Client did not request a repair snapshot')
        self.assert_frozen(kind, spectator, initial=initial, committed=final)
        relay.repair_gate.set()
        self.assert_recovered(kind, spectator, final)
        # Subsequent authoritative updates must work on the same attachment.
        final = self.act(player, {'type': 'move', 'direction': 'west'})
        self.assertIsNone(final['error'])
        if kind == 'ascii':
            continued = self.ascii_frame(spectator, lambda f: f['state']['observation'] == final['state']['observation'])
            self.assertDisclosedStateEqual(continued['state'], final['state'])
            self.assertEqual(continued['history'], final['history'])
            self.assertTrue(continued['connected'])
        else:
            passage = spectator.until(lambda line: line == '> ')
            self.assertIn('west', passage)
        self.assertEqual(relay.attachments, 1)
        self.assertEqual(relay.repairs, 1)
        spectator.stop()
        replacement, recovered = self.playable(kind, self.address)
        if kind == 'ascii':
            self.assertDisclosedStateEqual(recovered['state'], final['state'])
            self.assertEqual(recovered['history'], final['history'])
        else:
            replacement.child.stdin.write('history\n')
            replacement.child.stdin.flush()
            history = replacement.until(lambda line: line == '> ')
            for entry in final['history']:
                self.assertIn(entry['id'], history)
        self.assertIsNone(self.act(player, {'type': 'move', 'direction': 'east'})['error'])



class StreamRecoveryProcesses(StreamRecoveryFixture, ProcessTestCase):
    def test_small_frames_preserve_complete_history_across_reset_and_restart(self):
        frame_bytes = 4096
        limits = ["--outbound-frame-bytes", frame_bytes,
                  "--outbound-client-bytes", 65536,
                  "--outbound-total-bytes", 262144]
        server = self.server(*limits)
        player, initial = self.client()
        notes = [f"Note {n}: " + "é\\\"" * 40 for n in range(6)]
        for note in notes:
            result = self.request(player, {
                "type": "command", "context": initial["input_context"], "branch": initial["branch"],
                "command": {"type": "annotate", "anchor": {"type": "state", "snapshot": initial["state"]["snapshot"]},
                            "text": note, "source": "user", "category": "note", "audience": "actor"}})
            self.assertIsNone(result["error"])
        reset = self.request(player, {"type": "snapshot"})
        # This subset alone exceeds a frame; complete wire snapshots include
        # additional authority, cursor and history-page fields.
        subset = json.dumps({"state": reset["state"], "history": reset["history"]},
                            ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        self.assertGreater(len(subset), frame_bytes)
        self.assertDisclosedStateEqual(reset["state"], initial["state"])
        self.assertEqual([entry["content"]["text"] for entry in reset["history"]], notes)
        spectator, observed = self.client(SPECTATOR_TOKEN)
        self.assertDisclosedStateEqual(observed["state"], reset["state"])
        self.assertEqual(observed["history"], reset["history"])
        self.flush_save()
        player.stop()
        spectator.stop()
        server.stop()
        self.server(*limits)
        resumed, restored = self.client()
        self.assertDisclosedStateEqual(restored["state"], reset["state"])
        self.assertEqual(restored["history"], reset["history"])
        self.assertIsNone(self.act(resumed, {"type": "wait"})["error"])


    def test_text_delayed_delivery_gap_and_relaunch(self):
        self.exercise('text')


    def test_text_overflowing_delta_and_relaunch(self):
        self.exercise_invalid_state('text', 'overflow_delta')


    def test_text_invalid_reconstructed_inventory_and_relaunch(self):
        self.exercise_invalid_state('text', 'invalid_inventory')


    def test_text_duplicate_unordered_inventory_repairs_without_partial_publication(self):
        self.exercise_invalid_state('text', 'duplicate_inventory')


    def test_text_invalid_collection_range_and_relaunch(self):
        self.exercise_invalid_state('text', 'invalid_collection_range')


    def test_text_fitting_delta_exceeds_retained_state_bound_and_repairs(self):
        self.exercise_invalid_state('text', 'oversized_retained_state')


    def test_headless_overdeep_response_is_fatal_while_healthy_player_continues(self):
        self.server()
        upstream = self.address
        player, _ = self.client()
        relay = StreamRelay(upstream)
        self.addCleanup(relay.close)
        self.address = relay.address
        spectator, initial = self.client(SPECTATOR_TOKEN, observe=True)
        self.address = upstream
        relay.overdeep.set()
        final = self.act(player, {'type': 'move', 'direction': 'east'})
        self.assertIsNone(final['error'])
        self.assertTrue(relay.corrupted.wait(5), 'No actual observation was corrupted')

        def rejection_or_observation(frame):
            if frame['type'] == 'fatal':
                return True
            message = frame.get('message') or {}
            return (message.get('type') == 'update' and
                    message['update']['body']['type'] in ('observation', 'observation_delta'))

        rejected = self.frame(spectator, rejection_or_observation)
        self.assertEqual(rejected['type'], 'fatal', 'Overdeep observations must never be presented')
        self.assertIn('nesting', rejected['error'])
        self.assertEqual(spectator.child.wait(timeout=10), 1)
        self.assertEqual(relay.repairs, 0, 'Malformed wire input must not trigger stream repair')
        self.assertEqual(relay.attachments, 1)
        self.assertFalse(relay.errors)
        replacement, recovered = self.client(SPECTATOR_TOKEN, observe=True)
        self.assertDisclosedStateEqual(recovered['state'], final['state'])
        self.assertGreater(int(recovered['state']['observation']['tick']),
                           int(initial['state']['observation']['tick']))
        self.assertIsNone(self.act(player, {'type': 'move', 'direction': 'east'})['error'])
        saved = self.request(player, {'type': 'save'})
        self.assertIsNone(saved['error'])



class NativeStreamRecoveryProcesses(StreamRecoveryFixture, ProcessTestCase):
    graphical = True

    def test_ascii_delayed_delivery_gap_and_relaunch(self):
        self.exercise('ascii')


    def test_ascii_overflowing_delta_and_relaunch(self):
        self.exercise_invalid_state('ascii', 'overflow_delta')


    def test_ascii_invalid_reconstructed_inventory_and_relaunch(self):
        self.exercise_invalid_state('ascii', 'invalid_inventory')


    def test_ascii_duplicate_unordered_inventory_repairs_without_partial_publication(self):
        self.exercise_invalid_state('ascii', 'duplicate_inventory')


    def test_ascii_invalid_collection_range_and_relaunch(self):
        self.exercise_invalid_state('ascii', 'invalid_collection_range')


    def test_ascii_fitting_delta_exceeds_retained_state_bound_and_repairs(self):
        self.exercise_invalid_state('ascii', 'oversized_retained_state')


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
            self.assertIsNone(self.act(player, {'type':'wait'})['error'])
            self.assertIsNone(self.request(other, {'type':'snapshot'})['error'])
            changed = self.act(other, {'type':'set_door','door':door,'open':opened})
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
