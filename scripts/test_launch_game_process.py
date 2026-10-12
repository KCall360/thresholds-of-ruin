"""Actual-process acceptance tests for the game launcher.

Each test runs `launch_game.py` as a real process against the real server and
clients, then checks the save, its launch record and that no child outlives it.
"""
import json
import os
import re
from pathlib import Path
import sqlite3
import subprocess
import sys
import threading
import time
import unittest
from contextlib import closing

from process_harness import ProcessTestCase, ROOT, SCENARIOS, inspect_save

LAUNCHER = ROOT / "scripts" / "launch_game.py"
PICKUP = re.compile(r"You pick up the (\w+) token\.")


def alive(pid):
    if os.name == "nt":
        import ctypes
        kernel32 = ctypes.WinDLL("kernel32")
        handle = kernel32.OpenProcess(0x00100000 | 0x1000, False, pid)  # SYNCHRONIZE | QUERY_LIMITED
        if not handle:
            return False
        try:
            return kernel32.WaitForSingleObject(handle, 0) == 0x102  # WAIT_TIMEOUT
        finally:
            kernel32.CloseHandle(handle)
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


def journal_rows(save):
    with closing(sqlite3.connect(save)) as connection:
        return connection.execute("SELECT COUNT(*) FROM journal").fetchone()[0]


class Launcher:
    """A running launcher whose output is collected on a background thread."""

    def __init__(self, test, args):
        environment = {k: v for k, v in os.environ.items()
                       if k not in ("TOR_SERVER_TOKEN", "TOR_SPECTATOR_TOKEN", "TOR_WIZARD_TOKEN")}
        self.child = subprocess.Popen(
            [sys.executable, "-X", "utf8", str(LAUNCHER), "--bin-dir", str(test.bin),
             "--scenarios-dir", str(SCENARIOS), "--saves-dir", str(test.saves), *args],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=environment)
        self.output = bytearray()
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()
        test.addCleanup(self.close)

    def _read(self):
        while chunk := self.child.stdout.read1(4096):
            self.output += chunk

    @property
    def text(self):
        return self.output.decode("utf-8", errors="replace")

    def write(self, text):
        self.child.stdin.write(text.encode())
        self.child.stdin.flush()

    def finish(self, seconds=60):
        self.child.stdin.close()
        code = self.child.wait(timeout=seconds)
        self.reader.join(timeout=10)
        return code

    def close(self):
        if self.child.poll() is None:
            self.child.kill()
            self.child.wait(timeout=10)
        for stream in (self.child.stdin, self.child.stdout):
            if not stream.closed:
                stream.close()


class LauncherTestCase(ProcessTestCase):
    """Helpers shared by the console and graphical launcher tests."""

    def setUp(self):
        super().setUp()
        self.saves = self.directory / "saves"

    def launcher(self, *args):
        return Launcher(self, args)

    def eventually(self, condition, message, seconds=30):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if value := condition():
                return value
            time.sleep(0.1)
        self.fail(message)

    def status(self, name):
        def read():
            logs = sorted((self.saves / name / "logs").glob("*/status.json"))
            return json.loads(logs[-1].read_text(encoding="utf-8")) if logs else None
        return self.eventually(read, f"launcher did not record status for {name}")

    def latest_log(self, name, file):
        found = lambda: sorted((self.saves / name / "logs").glob(f"*/{file}"))
        return self.eventually(found, f"launcher did not create {file} for {name}")[-1]

    def assert_children_ended(self, status):
        for role in ("server", "player", "spectator"):
            if pid := status.get(role):
                self.assertFalse(alive(pid), f"{role} process {pid} outlived the launcher")


class LaunchGameProcesses(LauncherTestCase):
    def test_menus_start_a_text_game_and_a_later_launch_resumes_it(self):
        # Menus: text client, no spectator, new game, seed and name; then play.
        first = self.launcher("--scenario", "two-room")
        first.write("1\n1\n1\n7\njourney\n")
        first.write("take the token\nquit\n")
        self.assertEqual(first.finish(), 0, first.text)
        self.assertIn("Starting journey: two-room, seed 7, text client.", first.text)
        material = PICKUP.search(first.text).group(1)
        self.assertIn("Game saved in", first.text)
        save = self.saves / "journey" / "game.db"
        self.assertEqual(json.loads((save.parent / "launch.json").read_text(encoding="utf-8"))
                         | {"created": None},
                         {"scenario": "two-room", "scenario_path": str(SCENARIOS / "two-room"),
                          "seed": 7, "created": None})
        self.assertIn(f"You pick up the {material} token.",
                      self.latest_log("journey", "player.transcript.log").read_text(encoding="utf-8"))
        self.assert_children_ended(self.status("journey"))
        # The explicit save on exit made the take durable beyond the base row.
        rows = journal_rows(save)
        self.assertGreater(rows, 1)

        # Resume by name with options only; the save keeps its own seed.
        second = self.launcher("--client", "text", "--spectator", "none", "--save", "journey")
        second.write("inventory\nquit\n")
        self.assertEqual(second.finish(), 0, second.text)
        self.assertIn("Resuming journey: two-room, text client.", second.text)
        # The same seeded token is still carried, so the save, not a new seed-7 game, resumed.
        resumed = second.text.split("Resuming", 1)[1]
        self.assertIn(f"{material} token", resumed)
        self.assertNotIn(f"{material} token lies at your feet", resumed)
        self.assertEqual(len(list((save.parent / "logs").iterdir())), 2)
        self.assert_children_ended(self.status("journey"))

    def test_resume_menu_lists_previous_games_and_refuses_reused_names(self):
        first = self.launcher("--client", "text", "--spectator", "none", "--scenario", "two-room",
                              "--seed", "3", "--new", "kept")
        first.write("quit\n")
        self.assertEqual(first.finish(), 0, first.text)
        # A clashing name is refused, then the menu offers the previous game.
        clash = self.launcher("--client", "text", "--spectator", "none", "--scenario", "two-room",
                              "--seed", "3", "--new", "kept")
        self.assertEqual(clash.finish(), 2)
        self.assertIn("already exists", clash.text)
        menu = self.launcher("--client", "text", "--spectator", "none")
        menu.write("2\nlook\nquit\n")
        self.assertEqual(menu.finish(), 0, menu.text)
        self.assertIn("2) kept (two-room, seed 3, last saved", menu.text)
        self.assertIn("Resuming kept: two-room", menu.text)

    def test_server_failure_is_reported_and_nothing_is_left_running(self):
        first = self.launcher("--client", "text", "--spectator", "none", "--scenario", "two-room",
                              "--seed", "3", "--new", "locked")
        self.status("locked")
        # A second server cannot open a save that is already in use.
        second = self.launcher("--client", "text", "--spectator", "none", "--save", "locked")
        self.assertEqual(second.finish(), 2, second.text)
        self.assertIn("Error: The server stopped during startup", second.text)
        first.write("quit\n")
        self.assertEqual(first.finish(), 0, first.text)
        for status in (self.saves / "locked" / "logs").glob("*/status.json"):
            self.assert_children_ended(json.loads(status.read_text(encoding="utf-8")))

    @unittest.skipUnless(os.name == "nt", "the job object guard is Windows-only")
    def test_killing_the_launcher_ends_its_server(self):
        launcher = self.launcher("--client", "text", "--spectator", "none", "--scenario", "two-room",
                                 "--seed", "3", "--new", "abandoned")
        status = self.status("abandoned")
        self.assertTrue(alive(status["server"]))
        launcher.child.kill()
        launcher.child.wait(timeout=10)
        self.eventually(lambda: not alive(status["server"]), "server outlived its killed launcher", seconds=10)

    @unittest.skipUnless(os.name == "nt", "text spectators open a Windows console")
    def test_text_spectator_watches_in_its_own_console_and_ends_with_the_game(self):
        launcher = self.launcher("--client", "text", "--spectator", "text", "--scenario", "two-room",
                                 "--seed", "5", "--new", "watched")
        status = self.status("watched")
        transcript = self.latest_log("watched", "spectator.transcript.log")
        self.eventually(lambda: "Spectator access is read-only." in transcript.read_text(encoding="utf-8"),
                        "text spectator did not connect as a spectator")
        launcher.write("take the token\n")
        material = self.eventually(lambda: (match := PICKUP.search(launcher.text)) and match.group(1),
                                   "player action was not acknowledged")
        self.eventually(lambda: f"You pick up an {material} token." in transcript.read_text(encoding="utf-8")
                        or f"You pick up a {material} token." in transcript.read_text(encoding="utf-8"),
                        "spectator did not observe the player's action")
        launcher.write("quit\n")
        self.assertEqual(launcher.finish(), 0, launcher.text)
        self.assertIn("Starting watched: two-room, seed 5, text client with a text spectator.", launcher.text)
        self.assert_children_ended(status)

    @unittest.skipIf(os.name == "nt", "Windows opens text spectator consoles")
    def test_text_spectator_is_refused_without_a_console_window(self):
        launcher = self.launcher("--client", "text", "--spectator", "text", "--scenario", "two-room",
                                 "--seed", "5", "--new", "watched")
        self.assertEqual(launcher.finish(), 2)
        self.assertIn("choose an ASCII spectator", launcher.text)
        self.assertFalse((self.saves / "watched").exists())


class LaunchGameWindows(LauncherTestCase):
    graphical = True

    def test_ascii_player_and_spectator_windows_share_one_game(self):
        launcher = self.launcher("--client", "ascii", "--spectator", "ascii", "--scenario", "two-room",
                                 "--seed", "9", "--new", "windows", "--report-frames")
        status = self.status("windows")

        def role(file):
            path = self.latest_log("windows", file)
            for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
                if line.startswith("{") and '"connected":true' in line:
                    return json.loads(line)["role"]
            return None
        self.assertEqual(self.eventually(lambda: role("player.stdout.log"),
                                                        "ASCII player presented no connected frame"), "player")
        self.assertEqual(self.eventually(lambda: role("spectator.stdout.log"),
                                                        "ASCII spectator presented no connected frame"), "spectator")
        self.assertIn("Playing in the ASCII window.", launcher.text)
        # Ending the player window saves the game and closes the spectator.
        subprocess.run(["taskkill", "/PID", str(status["player"]), "/F"] if os.name == "nt"
                       else ["kill", str(status["player"])], check=True, capture_output=True)
        launcher.finish()
        self.assertIn("Game saved in", launcher.text)
        self.assertNotIn("Warning", launcher.text)
        self.assertIn("ruleset", inspect_save(self.saves / "windows" / "game.db"))
        self.assert_children_ended(status)


if __name__ == "__main__":
    unittest.main()
