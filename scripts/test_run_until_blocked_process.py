"""Running until blocked through real processes: a client that stops reading
can't hold the game. See docs/run-until-blocked.md."""
import json
import unittest

from stream_relay import StreamRelay

from process_harness import ProcessTestCase, SPECTATOR_TOKEN

# Enough turns to fill a stalled spectator's outgoing queue (256 messages)
# and what's left of its socket buffers several times over.
TURNS = 1500
# The server drops a client whose queue stays full this long (runner::STALL).
STALL_SECONDS = 5


class RunUntilBlockedProcesses(ProcessTestCase):
    def test_a_spectator_that_stops_reading_is_dropped_and_play_continues(self):
        self.server()
        player, initial = self.client()
        relay = StreamRelay(self.address, receive_buffer=4096)
        self.addCleanup(relay.close)
        stalled = self.launch("tor-client-headless", ["--connect", relay.address], token=SPECTATOR_TOKEN)
        self.frame(stalled, lambda f: f["type"] == "ready")
        relay.gate.clear()
        # Pipe every turn at once: the player never waits on the spectator.
        player.write((json.dumps({"type": "act", "action": {"type": "wait"}}) + "\n") * TURNS)
        final = None
        for _ in range(TURNS):
            final = self.frame(player, lambda f: f["type"] == "ready", seconds=STALL_SECONDS * 6)
            self.assertIsNone(final["error"])
        tick = initial["state"]["observation"]["tick"] + TURNS * 100
        self.assertEqual(final["state"]["observation"]["tick"], tick)
        # Released after the stall timeout, the spectator finds itself disconnected.
        relay.gate.set()
        self.assertNotEqual(stalled.child.wait(timeout=STALL_SECONDS * 4), 0)
        # A replacement spectator starts at the committed state.
        _, watching = self.client(SPECTATOR_TOKEN)
        self.assertEqual(watching["state"], final["state"])


if __name__ == "__main__":
    unittest.main()
