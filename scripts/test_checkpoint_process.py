"""Actual clients exercise checkpoint rotation, crash rollback and restart."""
import json
import sqlite3
import unittest
import test_text_process as support
import test_headless_process as headless
import test_ascii_process as ascii_support


class CheckpointProcesses(unittest.TestCase):
    setUpClass = classmethod(support.TextProcesses.setUpClass.__func__)
    setUp = headless.HeadlessProcesses.setUp
    launch = support.TextProcesses.launch
    client = headless.HeadlessProcesses.client
    frame = headless.HeadlessProcesses.frame
    command = headless.HeadlessProcesses.command
    act = headless.HeadlessProcesses.act
    request = headless.HeadlessProcesses.request

    def server(self, target=3600000, maximum=7200000):
        server = self.launch("tor-server", ["--listen", "127.0.0.1:0", "--save", self.save,
            "--checkpoint-interval", "4", "--save-target-ms", str(target), "--save-max-ms", str(maximum), "--save-idle-ms", "0"],
            extra_env={"TOR_SPECTATOR_TOKEN": support.SPECTATOR_TOKEN})
        self.address = json.loads(server.until(lambda line: line.startswith("{")))["address"]
        return server

    def test_checkpoint_tail_crash_and_restart_through_real_headless_client(self):
        server = self.server()
        player, _ = self.client()
        for _ in range(10):
            saved = self.act(player, {"type": "wait"})
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], 8)
            self.assertEqual(db.execute("SELECT count(*) FROM history").fetchone()[0], 8)
            self.assertEqual(db.execute("SELECT count(*) FROM journal WHERE sequence>0").fetchone()[0], 2)
        for _ in range(3):
            self.act(player, {"type": "wait"})
        server.child.kill(); server.child.wait(timeout=10)
        self.server()
        player, resumed = self.client()
        self.assertEqual(resumed["state"], saved["state"])
        self.assertEqual(resumed["history"], saved["history"])
        for _ in range(3):
            continued = self.act(player, {"type": "wait"})
        self.assertEqual(continued["state"]["observation"]["tick"], 1300)
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], 12)

    def test_text_player_and_native_ascii_spectator_resume_checkpointed_game(self):
        server = self.server()
        player, _ = support.TextProcesses.client(self)
        spectator = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"],
            token=support.SPECTATOR_TOKEN)
        ascii_support.AsciiProcesses.frame(self, spectator, lambda f: f["state"] is not None)
        for n in range(1, 10):
            player.command("wait")
            saved = ascii_support.AsciiProcesses.frame(self, spectator,
                lambda f: f["state"]["observation"]["tick"] == n*100)
        self.assertIn("Done.", player.command("save"))
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], 8)
        spectator.stop(); player.stop(); server.stop()
        self.server()
        player, welcome = support.TextProcesses.client(self)
        self.assertIn("tick 900", welcome)
        spectator = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"],
            token=support.SPECTATOR_TOKEN)
        resumed = ascii_support.AsciiProcesses.frame(self, spectator, lambda f: f["state"] is not None)
        self.assertEqual(resumed["state"], saved["state"])
        self.assertEqual(resumed["history"], saved["history"])
        player.command("wait")
        ascii_support.AsciiProcesses.frame(self, spectator,
            lambda f: f["state"]["observation"]["tick"] == 1000)

    def test_checkpoint_writer_wait_does_not_block_new_actions(self):
        import time
        self.server(target=1, maximum=1000)
        player, _ = self.client()
        with sqlite3.connect(self.save) as db:
            db.execute("BEGIN EXCLUSIVE")
            started = time.monotonic()
            for _ in range(6):
                self.act(player, {"type": "wait"})
            self.assertLess(time.monotonic()-started, 1.5)
            db.rollback()
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], 4)
