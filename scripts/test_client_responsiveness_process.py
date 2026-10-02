"""Native input during blocked saving/checkpoints, and bounded burst delivery."""
import sqlite3
import time
import unittest

from process_harness import ProcessTestCase


class ClientResponsivenessProcesses(ProcessTestCase):
    def saving_server(self, interval):
        """A 256-region server that saves at once and checkpoints every `interval` actions."""
        return self.server("--regions", 256, "--checkpoint-interval", interval, "--save-target-ms", 1,
                           "--save-max-ms", 1000, "--save-idle-ms", 0, seed=None, spectator=False)

    def native_during_save(self, interval):
        self.saving_server(interval)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        initial = self.ascii_frame(window, lambda f: f["has_control"] and not f["busy"])
        tick = initial["state"]["observation"]["tick"]
        with sqlite3.connect(self.save) as db:
            sequence = db.execute("SELECT max(sequence) FROM (SELECT sequence FROM journal UNION ALL SELECT sequence FROM history)").fetchone()[0]
        key = self.native_keys(window)
        with sqlite3.connect(self.save) as db:
            db.execute("BEGIN EXCLUSIVE")
            key("period", True)
            acted = self.ascii_frame(window, lambda f: f["state"]["observation"]["tick"] == tick + 100 and not f["busy"])
            key("period", False)
            # Force the real worker to remain in its save transaction while a
            # native local modal opens. The SQLite timeout is two seconds.
            time.sleep(.1)
            started = time.monotonic()
            key("F4", True)
            opened = self.ascii_frame(window, lambda f: f["note"] == "")
            key("F4", False)
            self.assertLess(time.monotonic()-started, 1.5)
            self.assertEqual(opened["state"], acted["state"])
            self.assertTrue(opened["connected"])
            self.assertLessEqual(opened["profile"]["network_events"], 16)
            db.rollback()
        observer = self.launch("tor-client-headless", ["--connect", self.address, "--observe"])
        self.frame(observer, lambda f: f.get("type") == "ready")
        self.assertIsNone(self.request(observer, {"type":"save"})["error"])
        with sqlite3.connect(self.save) as db:
            if interval:
                self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], sequence + 1)
            else:
                self.assertEqual(db.execute("SELECT max(sequence) FROM journal").fetchone()[0], sequence + 1)
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
        player, _ = self.client()
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--observe", "--report-frames"])
        self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"])
        key = self.native_keys(window)
        player.child.stdin.write('{"type":"act","action":{"type":"wait"}}\n' * 160)
        player.child.stdin.flush()
        started = time.monotonic()
        key("F4", True)
        opened = self.ascii_frame(window, lambda f: f["note"] == "")
        key("F4", False)
        self.assertLess(time.monotonic()-started, 1.5)
        self.assertTrue(opened["connected"])
        final = None
        for _ in range(160):
            final = self.frame(player, lambda f: f.get("type") == "ready")
            self.assertIsNone(final["error"])
        presented = opened if opened["state"]["observation"]["tick"] == 16000 else self.ascii_frame(
            window, lambda f: f["state"]["observation"]["tick"] == 16000)
        self.assertEqual(presented["state"], final["state"])
        self.assertEqual(presented["history"], final["history"])
        self.assertEqual(len(presented["history"]), 100)
        self.assertTrue(presented["connected"])
        self.assertLessEqual(presented["profile"]["network_events"], 16)
        self.assertIsNone(self.request(player, {"type":"save"})["error"])
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], 160)


if __name__ == "__main__":
    unittest.main()
