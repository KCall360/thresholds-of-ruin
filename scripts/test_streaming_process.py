"""Actual clients play a streaming game across detached halls, save, restart,
reconnect, spectate and rewind. See docs/region-streaming.md."""
import json
import os
import shutil
import sqlite3
import subprocess
import time
import unittest
from contextlib import closing
from pathlib import Path

import test_ascii_process as ascii_support
import test_headless_process as headless
import test_text_process as support

SCENARIO = Path(__file__).resolve().parents[1] / "scenarios/tests/streaming-corridor"
# Two authored halls around two generated caves. Each cave's entries share a
# row, so walking east from the start hall's portal always crosses it.
GENERATED = SCENARIO.parent / "generated-filler"
# From the start into the first cave, and through both caves to the far hall.
INTO_CAVE = 12
THROUGH_CAVES = 35
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

    def server(self, wizard=False, scenario=SCENARIO):
        env = {"TOR_SPECTATOR_TOKEN": support.SPECTATOR_TOKEN}
        if wizard:
            env["TOR_WIZARD_TOKEN"] = headless.WIZARD_TOKEN
        server = self.launch("tor-server", ["--listen", "127.0.0.1:0", "--seed", "5", "--save", self.save,
            *(["--scenario", scenario] if scenario else []), "--checkpoint-interval", "4",
            *(["--wizard"] if wizard else [])], extra_env=env)
        self.address = json.loads(server.until(lambda line: line.startswith("{")))["address"]
        return server

    def walk(self, client, direction, steps):
        for _ in range(steps):
            moved = self.act(client, {"type": "move", "direction": direction})
            self.assertIsNone(moved.get("error"), moved)
        return moved

    def east_through(self, client, steps):
        """Walk east. The server runs the caves' rats between the
        character's turns, so a move may come before the character is ready:
        try again shortly. While the character is ready, game time waits for
        it, so a fresh snapshot then shows whether a rat blocks the way, and
        the walk waits a turn only then: the same commands every run."""
        for _ in range(steps):
            for _ in range(200):
                moved = self.act(client, {"type": "move", "direction": "east"})
                if moved.get("error") is None:
                    break
                now = self.request(client, {"type": "snapshot"})["state"]["observation"]
                east = {"x": 1, "y": 0, "z": 0}
                if now["ready"] and any(a["position"] == east for a in now["visible_actors"]):
                    self.assertIsNone(self.act(client, {"type": "wait"}).get("error"))
                else:
                    time.sleep(0.05)
            else:
                self.fail("blocked for good: " + str(moved.get("error")))
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

    def test_a_moved_package_is_named_to_resume_and_every_client_plays_on(self):
        directory = Path(self.save).parent
        package = directory / "package"
        shutil.copytree(SCENARIO, package)
        server = self.server(scenario=package)
        player, _ = self.client()
        # Into hall 2: halls 1-4 are built, and 5-7 aren't.
        far = self.walk(player, "east", 20)
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        moved = directory / "moved"
        package.rename(moved)
        env = {k: v for k, v in os.environ.items() if k not in ("TOR_WIZARD_TOKEN", "TOR_SPECTATOR_TOKEN")}
        env["TOR_SERVER_TOKEN"] = support.TOKEN
        before = Path(self.save).read_bytes()
        refused = subprocess.run([self.bin / ("tor-server" + self.suffix), "--listen", "127.0.0.1:0",
            "--save", self.save], env=env, capture_output=True, text=True, timeout=15)
        self.assertNotEqual(refused.returncode, 0)
        self.assertIn("isn't available", refused.stderr)
        self.assertIn("--scenario", refused.stderr)
        self.assertEqual(Path(self.save).read_bytes(), before)

        self.server(scenario=moved)
        native = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"])
        shown = ascii_support.AsciiProcesses.frame(self, native, lambda f: f["state"] is not None)
        self.assertEqual(shown["state"]["observation"], far["state"]["observation"])
        native.stop()
        text = self.launch("tor-client-text", ["--connect", self.address])
        text.until(lambda line: line == "Ready.")
        self.assertIn("Waited", text.command("wait"))
        text.stop()
        # Walking on builds hall 5 from the moved package.
        player, _ = self.client()
        self.walk(player, "east", 20)
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with closing(sqlite3.connect(self.save)) as db:
            copied = [r[0] for r in db.execute("SELECT region FROM region_sources ORDER BY region")]
        self.assertEqual(copied, [1, 2, 3, 4, 5, 6])

    def test_generated_caves_are_played_spectated_rewound_and_resumed(self):
        server = self.server(wizard=True, scenario=GENERATED)
        wizard, _ = self.client(headless.WIZARD_TOKEN)
        native = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"],
            token=support.SPECTATOR_TOKEN)
        ascii_support.AsciiProcesses.frame(self, native, lambda f: f["state"] is not None)
        # Into the first cave. Rewinding and walking in again builds the same
        # caves, so the same commands see exactly the same thing.
        inside = self.east_through(wizard, INTO_CAVE)
        rewound = self.command(wizard, {"type": "wizard", "command": "rewind initial"})
        self.assertIsNone(rewound["error"], rewound["error"])
        again = self.east_through(wizard, INTO_CAVE)
        self.assertEqual(again["state"]["observation"], inside["state"]["observation"])
        far = self.east_through(wizard, THROUGH_CAVES - INTO_CAVE)
        shown = ascii_support.AsciiProcesses.frame(self, native,
            lambda f: f["state"]["observation"]["tick"] == far["state"]["observation"]["tick"])
        self.assertEqual(shown["state"]["observation"], far["state"]["observation"])
        # The rats act until the character is ready again; then time waits.
        for _ in range(200):
            settled = self.request(wizard, {"type": "snapshot"})["state"]["observation"]
            if settled["ready"]:
                break
            time.sleep(0.05)
        self.assertIsNone(self.request(wizard, {"type": "save"})["error"])
        with closing(sqlite3.connect(self.save)) as db:
            copied = [r[0] for r in db.execute("SELECT region FROM region_sources ORDER BY region")]
        self.assertEqual(copied, [1, 2, 3, 4])
        native.stop()
        wizard.stop()
        server.stop()
        self.server(wizard=True, scenario=GENERATED)
        _, resumed = self.client(headless.WIZARD_TOKEN)
        self.assertEqual(resumed["state"]["observation"], settled)

    def palette_of(self, client):
        """The next palette message a headless client printed."""
        frame = self.frame(client, lambda f: (f.get("message") or {}).get("type") == "palette")
        return frame["message"]

    def test_palettes_reach_real_clients_whole_on_attach_and_on_request(self):
        """Delta palettes, which need regions farther apart than this
        fixture's default radii reach, are covered over WebSocket in
        crates/server/tests/it/palettes.rs."""
        self.server(scenario=GENERATED)
        everything = {"terrain.floor.stone", "terrain.wall.stone", "terrain.floor.cave",
            "terrain.wall.cave", "creature.rat", "item.coin", "terrain.floor.marble",
            "terrain.wall.marble", "creature.delver"}
        # Observing sends no control request, so the palette that follows
        # attaching comes after the ready line rather than inside it.
        player = self.launch("tor-client-headless", ["--connect", self.address, "--observe"])
        first = self.frame(player, lambda f: f["type"] == "ready")
        palette = self.palette_of(player)
        self.assertIsNone(palette["request_id"])
        self.assertEqual(palette["palette"]["revision"], 1)
        self.assertEqual(palette["palette"]["body"]["type"], "full")
        self.assertEqual(set(palette["palette"]["body"]["assets"]), everything)
        cells = first["state"]["observation"]["visible_cells"]
        self.assertTrue(any(c.get("asset") == "terrain.floor.stone" for c in cells))
        # Spectators get the palette too, and may ask for it again.
        spectator, _ = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(self.palette_of(spectator)["palette"]["body"]["type"], "full")
        spectator.child.stdin.write(json.dumps({"type": "request", "request": {"type": "palette"}}) + "\n")
        spectator.child.stdin.flush()
        answered = self.palette_of(spectator)
        self.assertIsNotNone(answered["request_id"])
        self.assertEqual(answered["palette"]["revision"], 2)
        self.assertEqual(set(answered["palette"]["body"]["assets"]), everything)
        done = self.frame(spectator, lambda f: f["type"] == "ready")
        self.assertIsNone(done["error"])
        # Reconnecting starts over with the whole palette.
        spectator.stop()
        again, _ = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(self.palette_of(again)["palette"]["revision"], 1)

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
