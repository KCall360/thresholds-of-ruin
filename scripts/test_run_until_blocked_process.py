"""Running until blocked through real processes: a client that stops reading
can't hold the game. See docs/run-until-blocked.md."""
import os
from pathlib import Path
import socket
import sys
import sqlite3
import subprocess
import time
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier
import unittest

from stream_relay import StreamRelay

from process_harness import ProcessTestCase, SPECTATOR_TOKEN, TOKEN

# Retain the original gameplay coverage, but do not assume a turn's wire size.
TURNS = 1500
MAX_PRESSURE_TURNS = 20000
OUTBOUND_SLOTS = 256
RECEIVE_BUFFER = 4096


def tcp_send_buffer_budget():
    """Host TCP buffering budget, used only to size this pressure workload."""
    if sys.platform.startswith("linux"):
        # The server uses default TCP send buffers. Linux may autotune them up
        # to tcp_wmem[2], even when the receiving peer advertises a tiny window.
        # https://docs.kernel.org/networking/ip-sysctl.html#tcp-wmem
        values = [int(value) for value in Path("/proc/sys/net/ipv4/tcp_wmem").read_text(encoding="utf-8").split()]
        if len(values) != 3 or not 0 < values[0] <= values[1] <= values[2]:
            raise ValueError("Invalid host tcp_wmem budget")
        return values[2]
    with socket.socket() as probe:
        return max(65536, probe.getsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF))
# The server drops a client whose queue stays full this long (runner::STALL).
STALL_SECONDS = 5


class RunUntilBlockedProcesses(ProcessTestCase):
    def test_connection_guarantees_reject_excess_client_without_interrupting_play(self):
        frame = 65536
        self.server("--outbound-frame-bytes", frame, "--outbound-client-bytes", frame,
                    "--outbound-total-bytes", frame * 2)
        player, ready = self.client()
        self.assertEqual(ready["capabilities"]["max_connections"], 2)
        self.assertEqual(ready["capabilities"]["max_response_bytes"], frame)
        self.assertEqual(ready["capabilities"]["max_request_bytes"], 16 * 1024)
        observer, observed = self.client(SPECTATOR_TOKEN, observe=True)
        self.assertEqual(observed["capabilities"], ready["capabilities"])
        excess = self.launch("tor-client-headless", ["--connect", self.address], token=SPECTATOR_TOKEN)
        rejected = self.frame(excess, lambda value: value.get("type") == "fatal")
        self.assertEqual(rejected["error"], "Connection rejected: ResourceLimit")
        self.assertNotIn(SPECTATOR_TOKEN, rejected["error"])
        self.assertNotEqual(excess.child.wait(timeout=15), 0)
        final = self.act(player, {"type": "wait"})
        self.assertIsNone(final["error"])
        observed = self.frame(observer, lambda value: value["state"] == final["state"])
        self.assertEqual(observed["state"]["observation"]["tick"], "100")
        self.assertIsNone(observer.child.poll())

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
            # Overdue and failed saves are distinct asynchronous warnings. Keep
            # the lock until an actual failure, regardless of which arrives first.
            warning = self.frame(player, lambda f:
                (f.get("message") or {}).get("type") == "error"
                and f["message"].get("code") == "storage_failure"
                and "save failed" in f["message"].get("message", "").lower())
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
            tick = int(initial["state"]["observation"]["tick"])
            for _ in range(32):
                frame = self.request(client, {"type": "snapshot"})
                self.assertIsNone(frame["error"])
                self.assertEqual(frame["branch"], initial["branch"])
                next_tick = int(frame["state"]["observation"]["tick"])
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
        self.assertGreater(int(boundary["state"]["observation"]["tick"]),
                           int(initial["state"]["observation"]["tick"]))
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
        player_relay = StreamRelay(self.address, measure_actor_facts=True)
        self.addCleanup(player_relay.close)
        player = self.launch("tor-client-headless", ["--connect", player_relay.address], token=TOKEN)
        initial = self.frame(player, lambda f: f["type"] == "ready")
        send_budget = tcp_send_buffer_budget()
        relay = StreamRelay(self.address, receive_buffer=RECEIVE_BUFFER)
        self.addCleanup(relay.close)
        stalled = self.launch("tor-client-headless", ["--connect", relay.address], token=SPECTATOR_TOKEN)
        self.frame(stalled, lambda f: f["type"] == "ready")
        if sys.platform.startswith("linux"):
            self.assertTrue(relay.server_connection_open(server.child.pid),
                            "socket ownership probe must identify the attached spectator")
        relay.gate.clear()
        player_relay.reset_traffic()
        # Wait for this actor's simulation effects before filling its one queue
        # slot again. Delivery to the stalled spectator cannot gate these turns.
        final = None
        for turns in range(1, MAX_PRESSURE_TURNS + 1):
            final = self.act(player, {"type": "wait"})
            self.assertIsNone(final["error"])
            observed_bytes, largest_frame = player_relay.traffic()
            # Shared state, event and intention payloads must exceed TCP buffering
            # plus a full host message queue, one in-flight frame and the relay's
            # held frame.
            # Extra metadata/window allowance keeps differing stream cursors
            # and the OS's receive-buffer accounting out of the lower bound.
            pressure_bytes = send_budget + (OUTBOUND_SLOTS + 2) * (largest_frame + 64) + 2 * RECEIVE_BUFFER
            if turns >= TURNS and observed_bytes > pressure_bytes:
                break
        else:
            self.fail(f"Pressure workload exhausted {MAX_PRESSURE_TURNS} turns: "
                      f"{observed_bytes} shared actor bytes, {pressure_bytes} required, "
                      f"{send_budget} TCP budget, {largest_frame} largest frame")
        print(f"Slow-spectator pressure: {turns} turns, {observed_bytes} shared actor bytes, "
              f"{pressure_bytes} required, {send_budget} TCP budget, {largest_frame} largest frame",
              file=sys.stderr)
        self.assertTrue(relay.held.wait(timeout=STALL_SECONDS), "spectator relay must actually hold output")
        self.assertEqual(player_relay.errors, [])
        self.assertEqual(relay.errors, [])
        tick = int(initial["state"]["observation"]["tick"]) + turns * 100
        self.assertEqual(int(final["state"]["observation"]["tick"]), tick)
        if sys.platform.startswith("linux"):
            # Crossing a byte budget is not itself proof that the server closed
            # the stream. Keep pressure applied through its existing stall/I/O
            # deadline, and distinguish server closure from later client drain.
            deadline = time.monotonic() + STALL_SECONDS * 2
            while relay.server_connection_open(server.child.pid):
                if time.monotonic() >= deadline:
                    self.fail("Server still owns the stalled spectator socket after pressure and stall deadline")
                time.sleep(0.05)
        relay.resume_reading()
        self.assertNotEqual(stalled.child.wait(timeout=STALL_SECONDS * 4), 0)
        # A replacement spectator starts at the committed state.
        watcher, watching = self.client(SPECTATOR_TOKEN)
        self.assertEqual(watching["state"], final["state"])
        return server, player, watcher, final


if __name__ == "__main__":
    unittest.main()
