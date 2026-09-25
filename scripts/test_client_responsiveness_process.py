"""Native input during blocked saving/checkpoints, and bounded burst delivery."""
import json
import sqlite3
import time
import unittest
import test_text_process as support
import test_headless_process as headless
import test_ascii_process as ascii_support
import test_doors_process as doors


class ClientResponsivenessProcesses(unittest.TestCase):
    setUpClass = classmethod(support.TextProcesses.setUpClass.__func__)
    setUp = headless.HeadlessProcesses.setUp
    launch = support.TextProcesses.launch
    client = headless.HeadlessProcesses.client
    frame = headless.HeadlessProcesses.frame
    request = headless.HeadlessProcesses.request
    command = headless.HeadlessProcesses.command
    native_keys = doors.DoorProcesses.native_keys
    ascii_frame = ascii_support.AsciiProcesses.frame

    def server(self, interval):
        server = self.launch("tor-server", ["--listen", "127.0.0.1:0", "--save", self.save,
            "--regions", "256", "--checkpoint-interval", str(interval),
            "--save-target-ms", "1", "--save-max-ms", "1000", "--save-idle-ms", "0"])
        self.address = json.loads(server.until(lambda line: line.startswith("{")))["address"]

    def native_during_save(self, interval):
        self.server(interval)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        self.ascii_frame(window, lambda f: f["has_control"] and not f["busy"])
        key = self.native_keys(window)
        with sqlite3.connect(self.save) as db:
            db.execute("BEGIN EXCLUSIVE")
            key("period", True)
            acted = self.ascii_frame(window, lambda f: f["state"]["observation"]["tick"] == 100 and not f["busy"])
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
                self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], 1)
            else:
                self.assertEqual(db.execute("SELECT max(sequence) FROM journal").fetchone()[0], 1)
        key("Escape", True)
        self.ascii_frame(window, lambda f: f["note"] is None)
        key("Escape", False)

    def test_native_input_during_saving(self):
        self.native_during_save(0)

    def test_native_input_during_checkpointing(self):
        self.native_during_save(1)

    def test_burst_preserves_final_state_and_bounded_history(self):
        self.server(16)
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
