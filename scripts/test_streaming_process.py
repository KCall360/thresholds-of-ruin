"""Actual clients play a streaming game across detached halls, save, restart,
reconnect, spectate and rewind. See docs/region-streaming.md."""
import json
import sqlite3
import time
import unittest
from contextlib import closing
from pathlib import Path

import test_ascii_process as ascii_support
import test_headless_process as headless
import test_text_process as support

SCENARIO = Path(__file__).resolve().parents[1] / "scenarios/tests/streaming-corridor"
# From the start (x = 2 in hall 1) to the middle of hall 4. There, at the
# default radii, hall 1 is detached and hall 6 is built but frozen.
TO_HALL_4 = 68


class StreamingProcesses(unittest.TestCase):
    setUpClass = classmethod(support.TextProcesses.setUpClass.__func__)
    setUp = headless.HeadlessProcesses.setUp
    launch = support.TextProcesses.launch
    client = headless.HeadlessProcesses.client
    frame = headless.HeadlessProcesses.frame
    command = headless.HeadlessProcesses.command
    act = headless.HeadlessProcesses.act
    request = headless.HeadlessProcesses.request

    def server(self, wizard=False):
        env = {"TOR_SPECTATOR_TOKEN": support.SPECTATOR_TOKEN}
        if wizard:
            env["TOR_WIZARD_TOKEN"] = headless.WIZARD_TOKEN
        server = self.launch("tor-server", ["--listen", "127.0.0.1:0", "--seed", "5", "--save", self.save,
            "--scenario", SCENARIO, "--checkpoint-interval", "4", *(["--wizard"] if wizard else [])],
            extra_env=env)
        self.address = json.loads(server.until(lambda line: line.startswith("{")))["address"]
        return server

    def walk(self, client, direction, steps):
        for _ in range(steps):
            moved = self.act(client, {"type": "move", "direction": direction})
            self.assertIsNone(moved.get("error"), moved)
        return moved

    def rows(self):
        with closing(sqlite3.connect(self.save)) as db:
            return db.execute("SELECT count(*) FROM regions").fetchone()[0]

    def restart_after_kill(self):
        # Windows releases a killed process's file locks asynchronously, so
        # the save lock may still be held for a moment.
        for _ in range(50):
            try:
                return self.server()
            except AssertionError as error:
                if "cannot be locked" not in str(error):
                    raise
                time.sleep(0.1)
        return self.server()

    @staticmethod
    def pebble(frame):
        return [i for i in frame["state"]["observation"]["ground_items"] if i["item"]["name"] == "pebble"]

    def test_play_across_detached_halls_with_spectators_crash_and_restart(self):
        server = self.server()
        player, initial = self.client()
        self.assertEqual(len(self.pebble(initial)), 1)
        spectator, _ = self.client(support.SPECTATOR_TOKEN)
        native = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"],
            token=support.SPECTATOR_TOKEN)
        ascii_support.AsciiProcesses.frame(self, native, lambda f: f["state"] is not None)

        far = self.walk(player, "east", TO_HALL_4)
        seen = self.frame(spectator, lambda f: f["state"]["revision"] == far["state"]["revision"])
        self.assertEqual(seen["state"], far["state"])
        shown = ascii_support.AsciiProcesses.frame(self, native,
            lambda f: f["state"]["observation"]["tick"] == far["state"]["observation"]["tick"])
        self.assertEqual(shown["state"]["observation"], far["state"]["observation"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        # Hall 1 left the horizon: its record is a row on disk.
        self.assertGreaterEqual(self.rows(), 1)
        self.assertEqual(self.pebble(far), [])

        server.child.kill()
        server.child.wait(timeout=10)
        server = self.restart_after_kill()
        player, resumed = self.client()
        self.assertEqual(resumed["state"], far["state"])
        spectator, observing = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(observing["state"]["observation"], far["state"]["observation"])

        # Walking back reattaches hall 1 from its row, exactly as it was.
        back = self.walk(player, "west", TO_HALL_4)
        pebble = self.pebble(back)
        self.assertEqual(len(pebble), 1)
        self.assertEqual(pebble, self.pebble(initial))
        seen = self.frame(spectator, lambda f: f["state"]["revision"] == back["state"]["revision"])
        self.assertEqual(seen["state"], back["state"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        spectator.stop()
        server.stop()
        self.server()
        _, again = self.client()
        self.assertEqual(again["state"], back["state"])

    def test_rewind_past_a_detach_then_a_different_future_survives_restart(self):
        server = self.server(wizard=True)
        wizard, initial = self.client(headless.WIZARD_TOKEN)
        self.walk(wizard, "east", TO_HALL_4)
        self.assertIsNone(self.request(wizard, {"type": "save"})["error"])
        self.assertGreaterEqual(self.rows(), 1)
        rewound = self.command(wizard, {"type": "wizard", "command": "rewind initial"})
        self.assertIsNone(rewound["error"], rewound)
        now = self.request(wizard, {"type": "snapshot"})
        self.assertEqual(now["state"]["observation"]["tick"], initial["state"]["observation"]["tick"])
        # A different future: take the pebble before leaving hall 1, so its
        # new record differs from the abandoned one still on disk.
        self.walk(wizard, "east", 6)
        taken = self.act(wizard, {"type": "take", "item": 10})
        self.assertIsNone(taken.get("error"), taken)
        far = self.walk(wizard, "east", TO_HALL_4 - 6)
        self.assertIsNone(self.request(wizard, {"type": "save"})["error"])
        wizard.stop()
        server.stop()
        self.server(wizard=True)
        _, resumed = self.client(headless.WIZARD_TOKEN)
        self.assertEqual(resumed["state"]["observation"], far["state"]["observation"])
        self.assertTrue(any(i["name"] == "pebble" for i in resumed["state"]["observation"]["inventory"]))


if __name__ == "__main__":
    unittest.main()
