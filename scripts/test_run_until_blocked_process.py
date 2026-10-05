"""Running until blocked through real processes: a client that stops reading
can't hold the game. See docs/run-until-blocked.md."""
import os
import sqlite3
import subprocess
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier
import unittest

from stream_relay import StreamRelay

from process_harness import ProcessTestCase, SPECTATOR_TOKEN, TOKEN

# Enough turns to fill a stalled spectator's outgoing queue (256 messages)
# and what's left of its socket buffers several times over.
TURNS = 1500
# The server drops a client whose queue stays full this long (runner::STALL).
STALL_SECONDS = 5


class RunUntilBlockedProcesses(ProcessTestCase):
    def test_invalid_outbound_limits_fail_before_creating_or_changing_save(self):
        configurations = [
            ["--outbound-frame-bytes", "0"],
            ["--outbound-frame-bytes", str(16 * 1024 * 1024 + 1)],
            ["--outbound-client-bytes", "1"],
            ["--outbound-total-bytes", "1"],
            ["--outbound-total-bytes", str(2 ** 32)],
        ]
        executable = self.bin / ("tor-server" + self.suffix)
        environment = {k: v for k, v in os.environ.items()
                       if k not in ("TOR_WIZARD_TOKEN", "TOR_SPECTATOR_TOKEN")}
        environment["TOR_SERVER_TOKEN"] = TOKEN

        def reject(flags):
            result = subprocess.run([str(executable), "--save", str(self.save), *flags],
                                    env=environment, capture_output=True, text=True, timeout=15)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Invalid outbound byte limits", result.stderr)
            self.assertNotIn(TOKEN, result.stdout + result.stderr)

        for flags in configurations:
            reject(flags)
            self.assertFalse(self.save.exists())
        server = self.server()
        player, _ = self.client()
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        self.flush_save()
        player.stop()
        server.stop()
        saved = self.save.read_bytes()
        for flags in configurations:
            reject(flags)
            self.assertEqual(self.save.read_bytes(), saved)

    def test_blocked_timing_diagnostics_do_not_block_play_or_durable_save(self):
        server = self.server("--save-target-ms", 1, "--save-max-ms", 5000, "--save-idle-ms", 0,
                             extra_env={"TOR_TIMING_DIAGNOSTICS": "1"}, separate_stderr=True)
        # Keep stderr unread: logging must not stop the simulation owner. Linux
        # permits shrinking the pipe; Windows anonymous pipes are already small.
        if os.name != "nt":
            import fcntl
            fcntl.fcntl(server.child.stderr.fileno(), fcntl.F_SETPIPE_SZ, 4096)
        def kill_server():
            if server.child.poll() is None:
                server.child.kill()
            server.child.wait(timeout=10)
        self.addCleanup(kill_server)
        player, initial = self.client()
        for _ in range(256):
            snapshot = self.request(player, {"type": "snapshot"})
            self.assertIsNone(snapshot["error"])
            self.assertEqual(snapshot["state"], initial["state"])
        with sqlite3.connect(self.save) as database:
            database.execute("BEGIN EXCLUSIVE")
            played = self.act(player, {"type": "wait"})
            self.assertIsNone(played["error"])
            warning = self.frame(player, lambda f: (f.get("message") or {}).get("type") == "error")
            self.assertIn("save failed", warning["message"]["message"].lower())
        saved = self.request(player, {"type": "save"})
        self.assertIsNone(saved["error"])
        player.stop()
        # The explicit barrier above is durable; do not require the unread
        # diagnostic pipe to drain before ending the process.
        server.child.kill()
        server.child.wait(timeout=10)
        server.stop()
        self.server()
        resumed, restored = self.client()
        self.assertEqual(restored["state"], played["state"])
        self.assertEqual(restored["history"], played["history"])
        self.assertIsNone(self.act(resumed, {"type": "wait"})["error"])

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
        self.slow_spectator_recovery()

    def test_small_outbound_byte_budgets_preserve_play_recovery_and_durable_restart(self):
        server, player, watcher, final = self.slow_spectator_recovery(
            "--outbound-frame-bytes", 65536,
            "--outbound-client-bytes", 262144,
            "--outbound-total-bytes", 524288,
        )
        boundary = self.request(player, {"type": "snapshot"})
        self.assertEqual(boundary["state"], final["state"])
        self.flush_save()
        player.stop()
        watcher.stop()
        server.stop()
        self.server()
        resumed, restored = self.client()
        self.assertEqual(restored["state"], boundary["state"])
        self.assertEqual(restored["history"], boundary["history"])
        self.assertIsNone(self.act(resumed, {"type": "wait"})["error"])

    def slow_spectator_recovery(self, *server_args):
        server = self.server(*server_args)
        player, initial = self.client()
        relay = StreamRelay(self.address, receive_buffer=4096)
        self.addCleanup(relay.close)
        stalled = self.launch("tor-client-headless", ["--connect", relay.address], token=SPECTATOR_TOKEN)
        self.frame(stalled, lambda f: f["type"] == "ready")
        relay.gate.clear()
        # Wait for this actor's simulation effects before filling its one queue
        # slot again. Delivery to the stalled spectator cannot gate these turns.
        final = None
        for _ in range(TURNS):
            final = self.act(player, {"type": "wait"})
            self.assertIsNone(final["error"])
        tick = initial["state"]["observation"]["tick"] + TURNS * 100
        self.assertEqual(final["state"]["observation"]["tick"], tick)
        # Released after the stall timeout, the spectator finds itself disconnected.
        relay.gate.set()
        self.assertNotEqual(stalled.child.wait(timeout=STALL_SECONDS * 4), 0)
        # A replacement spectator starts at the committed state.
        watcher, watching = self.client(SPECTATOR_TOKEN)
        self.assertEqual(watching["state"], final["state"])
        return server, player, watcher, final


if __name__ == "__main__":
    unittest.main()
