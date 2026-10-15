"""Native input during blocked saving/checkpoints, and bounded burst delivery."""
import sqlite3
import threading
import time
import unittest
from concurrent.futures import ThreadPoolExecutor

from process_harness import ProcessTestCase


class ClientResponsivenessProcesses(ProcessTestCase):
    graphical = True

    def saving_server(self, interval, *, writer_barrier=False):
        """A 256-region server that saves at once and checkpoints every `interval` records."""
        return self.server("--regions", 256, "--checkpoint-interval", interval, "--save-target-ms", 1,
                           "--save-max-ms", 1000, "--save-idle-ms", 0, seed=None, spectator=False, writer_barrier=writer_barrier)

    def native_during_save(self, interval):
        self.saving_server(interval, writer_barrier=True)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        initial = self.ascii_frame(window, lambda f: f["has_control"] and not f["busy"])
        tick = int(initial["state"]["observation"]["tick"])
        with sqlite3.connect(self.save) as db:
            sequence = db.execute("SELECT max(sequence) FROM (SELECT sequence FROM journal UNION ALL SELECT sequence FROM history)").fetchone()[0]
        key = self.native_keys(window)
        with self.held_save_writer() as barrier:
            key("period", True)
            try:
                acted = self.ascii_frame(window, lambda f: int(f["state"]["observation"]["tick"]) == tick + 100 and not f["busy"])
            finally:
                key("period", False)
            barrier.wait()
            key("F4", True)
            try:
                opened = self.ascii_frame(window, lambda f: f["note"] == "")
            finally:
                key("F4", False)
            self.assertEqual(opened["state"], acted["state"])
            self.assertTrue(opened["connected"])
            self.assertLessEqual(opened["profile"]["network_events"], 16)
            barrier.assert_held()
        observer = self.launch("tor-client-headless", ["--connect", self.address, "--observe"])
        self.frame(observer, lambda f: f.get("type") == "ready")
        self.assertIsNone(self.request(observer, {"type":"save"})["error"])
        barrier.assert_released()
        with sqlite3.connect(self.save) as db:
            if interval:
                self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], sequence + 2)
            else:
                self.assertEqual(db.execute("SELECT max(sequence) FROM journal").fetchone()[0], sequence + 2)
        key("Escape", True)
        self.ascii_frame(window, lambda f: f["note"] is None)
        key("Escape", False)

    def test_native_input_during_saving(self):
        self.native_during_save(0)

    def test_native_input_during_checkpointing(self):
        self.native_during_save(1)

    def test_explored_save_restart_and_native_input_during_checkpointing(self):
        import os
        from pathlib import Path
        from saved_exploration_driver import run_saved_exploration
        output = self.save.parent / "explored"
        regions = int(os.environ.get("TOR_SAVED_EXPLORATION_REGIONS", "8"))
        result = run_saved_exploration(self.bin, output, regions=regions, correlate=True, defer_logs=True)
        from timing_correlation import correlate_native
        correlated = correlate_native(output, result)
        self.assertEqual(len(correlated), result["actions"])
        self.assertTrue(all(row['ack_diagnostic_write_ms'] is not None for row in correlated[:-1]))
        self.assertTrue(all(row['presented_to_reader_ms'] is not None for row in correlated))
        self.assertTrue(result["restart_equal"])
        self.save = Path(output) / "game.db"
        self.native_during_save(1)

    def test_burst_preserves_final_state_and_bounded_history(self):
        self.saving_server(16)
        # Keep the full state/history workload without repeated diagnostic map memory.
        player, _ = self.client(omit_memory=True)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--observe", "--report-frames"])
        self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"])
        key = self.native_keys(window)
        first_completed = threading.Event()
        request_in_flight = threading.Event()
        modal_presented = threading.Event()
        stop_producer = threading.Event()
        observer, _ = self.client(observe=True, omit_memory=True)

        def produce_burst():
            # Continue actual gameplay while native input is being handled.
            # Merely pausing a producer here would remove the concurrency this
            # test exists to exercise. Finish on an eight-action checkpoint
            # boundary only after the modal has presented and at least 160 turns.
            final = None
            actions = 0
            deadline = time.monotonic() + 120
            while actions < 160 or not modal_presented.is_set() or actions % 8:
                if stop_producer.is_set():
                    raise AssertionError("native burst consumer failed")
                if time.monotonic() >= deadline:
                    raise AssertionError("native concurrent burst exceeded its watchdog")
                request_in_flight.set()
                try:
                    final = self.act(player, {"type": "wait"})
                finally:
                    request_in_flight.clear()
                self.assertIsNone(final["error"])
                if not final["readiness"]["admission"]:
                    final = self.frame(player, lambda f: f["readiness"]["admission"])
                actions += 1
                first_completed.set()
            return final, actions

        with ThreadPoolExecutor(max_workers=1) as producer:
            burst = producer.submit(produce_burst)
            try:
                self.assertTrue(first_completed.wait(15))
                self.assertTrue(request_in_flight.wait(15))
                self.assertFalse(burst.done())
                key("F4", True)
                try:
                    opened = self.ascii_frame(window, lambda f: f["note"] == "")
                finally:
                    key("F4", False)
                self.assertTrue(opened["connected"])
                self.assertFalse(burst.done(), "actual gameplay must continue until modal presentation")
                # A second client positively observes further server execution
                # while that modal remains open, independent of input latency.
                opened_tick = int(opened["state"]["observation"]["tick"])
                progressed = self.frame(observer, lambda f:
                    int(f["state"]["observation"]["tick"]) > opened_tick)
                self.assertGreater(int(progressed["state"]["observation"]["tick"]), opened_tick)
                modal_presented.set()
                final, actions = burst.result(timeout=120)
            finally:
                stop_producer.set()
                modal_presented.set()
        self.assertEqual(final["state"]["observation"]["tick"], str(actions * 100))
        presented = opened if opened["state"]["observation"]["tick"] == str(actions * 100) else self.ascii_frame(
            window, lambda f: f["state"]["observation"]["tick"] == str(actions * 100))
        self.assertEqual(presented["state"], final["state"])
        self.assertEqual(presented["history"], final["history"])
        self.assertEqual(len(presented["history"]), 100)
        self.assertTrue(presented["connected"])
        self.assertLessEqual(presented["profile"]["network_events"], 16)
        self.assertIsNone(self.request(player, {"type":"save"})["error"])
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], actions * 2)


if __name__ == "__main__":
    unittest.main()
