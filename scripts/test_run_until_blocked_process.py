"""Running until blocked through real processes: a client that stops reading
can't hold the game. See docs/run-until-blocked.md."""
import json
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier
import unittest

from stream_relay import StreamRelay

from process_harness import ProcessTestCase, SPECTATOR_TOKEN

# Enough turns to fill a stalled spectator's outgoing queue (256 messages)
# and what's left of its socket buffers several times over.
TURNS = 1500
# The server drops a client whose queue stays full this long (runner::STALL).
STALL_SECONDS = 5


class RunUntilBlockedProcesses(ProcessTestCase):
    def test_concurrent_readers_preserve_ai_progress_and_saved_boundaries(self):
        server = self.server(scenario="first-dungeon", seed=None)
        player, initial = self.client()
        watchers = [self.client(SPECTATOR_TOKEN)[0] for _ in range(4)]
        started = Barrier(len(watchers) + 1)

        def read_snapshots(client):
            started.wait(timeout=15)
            tick = initial["state"]["observation"]["tick"]
            for _ in range(32):
                frame = self.request(client, {"type": "snapshot"})
                self.assertIsNone(frame["error"])
                self.assertEqual(frame["branch"], initial["branch"])
                next_tick = frame["state"]["observation"]["tick"]
                self.assertGreaterEqual(next_tick, tick)
                tick = next_tick

        with ThreadPoolExecutor(max_workers=len(watchers)) as readers:
            pending = [readers.submit(read_snapshots, client) for client in watchers]
            started.wait(timeout=15)
            for _ in range(3):
                played = self.act(player, {"type": "wait"})
                self.assertIsNone(played["error"])
                if not played["state"]["observation"]["ready"]:
                    played = self.frame(player, lambda f: f.get("state") is not None
                                        and f["state"]["observation"]["ready"])
            for future in pending:
                future.result(timeout=30)

        boundary = self.request(player, {"type": "snapshot"})
        self.assertGreater(boundary["state"]["observation"]["tick"],
                           initial["state"]["observation"]["tick"])
        self.assertTrue(boundary["state"]["observation"]["ready"])
        for client in watchers:
            watching = self.request(client, {"type": "snapshot"})
            self.assertEqual(watching["state"], boundary["state"])
            client.stop()
        self.flush_save()
        player.stop()
        server.stop()
        self.server(scenario="first-dungeon", seed=None)
        resumed, restored = self.client()
        self.assertEqual(restored["state"], boundary["state"])
        self.assertEqual(restored["history"], boundary["history"])
        self.assertIsNone(self.act(resumed, {"type": "wait"})["error"])

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
