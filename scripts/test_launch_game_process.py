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

    def states(self):
        """Every JSON state report the graphical launcher's automation printed."""
        return [json.loads(line) for line in self.text.splitlines() if line.startswith('{"event"')]

    def send(self, command):
        self.write(json.dumps(command) + "\n")

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
        """The terminal-menu launcher (`--console`)."""
        return Launcher(self, ["--console", *args])

    def eventually(self, condition, message, seconds=30):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if value := condition():
                return value
            time.sleep(0.1)
        self.fail(message)

    def status(self, name, launches=1):
        """The process record of the `launches`-th launch of a game, once written."""
        def read():
            logs = sorted((self.saves / name / "logs").glob("*/status.json"))
            return json.loads(logs[-1].read_text(encoding="utf-8")) if len(logs) >= launches else None
        return self.eventually(read, f"launcher did not record status for launch {launches} of {name}")

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


class GraphicalLauncherProcesses(LauncherTestCase):
    """The real launcher window, driven through its `--automation` commands."""

    graphical = True

    def window(self, *args):
        window = Launcher(self, ["--automation", "--report-frames", *args])
        self.state(window, lambda state: state["event"] == "ready", "launcher window did not open")
        return window

    def state(self, window, predicate, message, seconds=60, after=0):
        """The first state report after the first `after` reports that satisfies `predicate`."""
        return self.eventually(lambda: next((s for s in window.states()[after:] if predicate(s)), None),
                               message + f"; output: {window.text[-2000:]}", seconds)

    def command(self, window, command):
        """Send one command and return the report it produced."""
        before = len(window.states())
        window.send(command)
        return self.state(window, lambda s: s["event"] == "command", f"no report for {command}", after=before)

    def connected_role(self, name, file):
        path = self.latest_log(name, file)
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            if line.startswith("{") and '"connected":true' in line:
                return json.loads(line)["role"]
        return None

    def play_until_running(self, window):
        before = len(window.states())
        state = self.command(window, {"click": "play"})
        self.assertFalse(state["error"], state["status"])
        return self.state(window, lambda s: s["running"] and not s["play_enabled"] and s["end_enabled"],
                          "game did not start", after=before)

    def end_game(self, window):
        before = len(window.states())
        self.command(window, {"click": "end"})
        return self.state(window, lambda s: s["event"] == "finished", "game did not finish", after=before)

    def test_window_starts_a_new_game_lists_it_and_resumes_it(self):
        window = self.window()
        ready = window.states()[0]
        self.assertEqual((ready["tab"], ready["saves"], ready["client"], ready["spectator"]),
                         ("new", [], "ascii", "none"))
        self.assertIn("two-room", ready["scenarios"])

        self.command(window, {"set": {"client": "ascii", "spectator": "ascii", "scenario": "two-room",
                                      "seed": "9", "name": "gui-game"}})
        self.play_until_running(window)
        status = self.status("gui-game")
        self.assertEqual(self.eventually(lambda: self.connected_role("gui-game", "player.stdout.log"),
                                         "ASCII player presented no connected frame"), "player")
        self.assertEqual(self.eventually(lambda: self.connected_role("gui-game", "spectator.stdout.log"),
                                         "ASCII spectator presented no connected frame"), "spectator")
        finished = self.end_game(window)
        save = self.saves / "gui-game" / "game.db"
        self.assertEqual(finished["status"], f"Game saved in {save.parent}")
        self.assertFalse(finished["running"])
        self.assertTrue(finished["play_enabled"])
        self.assertEqual((finished["tab"], finished["selected"]), ("saved", str(save)))
        self.assertEqual([(s["name"], s["scenario"], s["seed"]) for s in finished["saves"]],
                         [("gui-game", "two-room", 9)])
        self.assertNotEqual(finished["name"], "gui-game")
        self.assert_children_ended(status)
        self.assertGreater(journal_rows(save), 0)

        # The saved game, selected in the list, resumes without a spectator.
        self.command(window, {"set": {"spectator": "none"}})
        self.play_until_running(window)
        resumed = self.status("gui-game", launches=2)
        self.assertEqual((resumed["save"], resumed["spectator"]), (str(save), None))
        self.assertEqual(self.eventually(lambda: self.connected_role("gui-game", "player.stdout.log"),
                                         "resumed ASCII player presented no connected frame"), "player")
        self.end_game(window)
        self.assert_children_ended(resumed)
        self.assertEqual(json.loads((save.parent / "launch.json").read_text(encoding="utf-8"))["seed"], 9)

        window.send({"close": True})
        self.assertEqual(window.finish(), 0, window.text)

    def test_file_browser_adds_a_save_from_another_directory(self):
        # A save made elsewhere (here by the console launcher) is outside the list until browsed.
        other = self.directory / "other-saves"
        first = Launcher(self, ["--console", "--client", "text", "--spectator", "none", "--scenario", "two-room",
                                "--seed", "3", "--new", "elsewhere", "--saves-dir", str(other)])
        first.write("quit\n")
        self.assertEqual(first.finish(), 0, first.text)
        (other / "elsewhere" / "launch.json").unlink()  # as if made before launch records existed

        window = self.window()
        state = self.command(window, {"browse": str(other / "elsewhere" / "game.db")})
        self.assertEqual((state["tab"], state["selected"], state["origin_enabled"]),
                         ("saved", str(other / "elsewhere" / "game.db"), True))
        state = self.command(window, {"click": "play"})
        self.assertIn("choose the scenario it started from", state["status"])
        self.assertFalse(state["running"])
        self.command(window, {"set": {"origin": "two-room", "spectator": "none"}})
        self.play_until_running(window)
        logs = other / "elsewhere" / "logs"
        self.eventually(lambda: len(list(logs.iterdir())) == 2, "browsed save did not start a launch")
        player = sorted(logs.iterdir())[-1] / "player.stdout.log"
        self.eventually(lambda: player.exists() and '"connected":true' in player.read_text(encoding="utf-8"),
                        "browsed save's ASCII player presented no connected frame")
        finished = self.end_game(window)
        self.assertIn(f"Game saved in {other / 'elsewhere'}", finished["status"])
        self.assertEqual(len(list((other / "elsewhere" / "logs").iterdir())), 2)
        self.assertIn(str(other / "elsewhere" / "game.db"), [s["path"] for s in finished["saves"]])
        window.send({"close": True})
        self.assertEqual(window.finish(), 0, window.text)

    def test_invalid_choices_are_shown_and_closing_mid_game_saves_first(self):
        window = self.window("--scenario", "two-room")
        state = self.command(window, {"set": {"seed": "not a number"}, "click": "play"})
        self.assertTrue(state["error"])
        self.assertIn("Seed must be", state["status"])
        self.assertFalse(state["running"])
        self.command(window, {"set": {"seed": "4", "name": "closing", "client": "ascii", "spectator": "none"}})
        self.play_until_running(window)
        status = self.status("closing")
        self.eventually(lambda: self.connected_role("closing", "player.stdout.log"),
                        "ASCII player presented no connected frame")
        window.send({"close": True})
        self.assertEqual(window.finish(), 0, window.text)
        self.assert_children_ended(status)
        self.assertGreater(journal_rows(self.saves / "closing" / "game.db"), 0)

    @unittest.skipUnless(os.name == "nt", "the window opens text clients in Windows consoles")
    def test_text_player_and_spectator_get_their_own_console_windows(self):
        window = self.window()
        self.command(window, {"set": {"client": "text", "spectator": "text", "scenario": "two-room",
                                      "seed": "5", "name": "consoles"}})
        self.play_until_running(window)
        status = self.status("consoles")
        player = self.latest_log("consoles", "player.transcript.log")
        spectator = self.latest_log("consoles", "spectator.transcript.log")
        self.eventually(lambda: "Type help" in player.read_text(encoding="utf-8"), "text player did not connect")
        self.eventually(lambda: "Spectator access is read-only." in spectator.read_text(encoding="utf-8"),
                        "text spectator did not connect")
        finished = self.end_game(window)
        self.assertIn("Game saved in", finished["status"])
        self.assert_children_ended(status)
        window.send({"close": True})
        self.assertEqual(window.finish(), 0, window.text)


if __name__ == "__main__":
    unittest.main()
