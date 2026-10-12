"""Unit tests for the game launcher's choices, saves and process commands."""
import argparse
import datetime
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import launch_game
from launch_game import Choices, LaunchError, Prompter, Save


def arguments(**values):
    defaults = dict(client=None, spectator=None, scenario=None, seed=None, save=None, new=None)
    return argparse.Namespace(**{**defaults, **values})


class Answers(Prompter):
    """A prompter that answers from a list and records what it showed."""

    def __init__(self, *answers):
        self.answers = list(answers)
        self.shown = []
        super().__init__(read_line=self._next, write=self.shown.append)

    def _next(self):
        if not self.answers:
            raise EOFError
        return self.answers.pop(0)

    @property
    def text(self):
        return "".join(self.shown)


class LauncherDirectory(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.scenarios = self.root / "scenarios"
        self.saves = self.root / "saves"
        for name in ("two-room", "rogue-exploration", "first-dungeon"):
            (self.scenarios / name).mkdir(parents=True)
            (self.scenarios / name / "scenario.toml").write_text("", encoding="utf-8")
        (self.scenarios / "tests").mkdir()
        (self.scenarios / "tests" / "nested").mkdir()

    def save(self, name, metadata=None, file="game.db", mtime=None):
        directory = self.saves / name
        directory.mkdir(parents=True)
        path = directory / file
        path.write_bytes(b"save")
        if metadata is not None:
            (directory / "launch.json").write_text(json.dumps(metadata), encoding="utf-8")
        if mtime is not None:
            os.utime(path, (mtime, mtime))
        return path

    def choose(self, prompter, **values):
        return launch_game.choose(arguments(**values), prompter, scenarios_dir=self.scenarios, saves_dir=self.saves)


class Discovery(LauncherDirectory):
    def test_scenarios_are_directories_with_a_manifest(self):
        self.assertEqual(list(launch_game.scenarios(self.scenarios)),
                         ["first-dungeon", "rogue-exploration", "two-room"])
        self.assertEqual(launch_game.scenarios(self.root / "missing"), {})

    def test_saves_list_run_directories_with_a_save_newest_first(self):
        old = self.save("old", {"scenario": "two-room", "seed": 3}, mtime=1_000_000)
        legacy = self.save("legacy", file="game.json", mtime=2_000_000)
        (self.saves / "empty").mkdir()
        (self.saves / "loose.db").write_bytes(b"not a run directory")
        found = launch_game.saves(self.saves)
        self.assertEqual([save.path for save in found], [legacy, old])
        self.assertEqual(found[1].metadata, {"scenario": "two-room", "seed": 3})
        self.assertIn("old (two-room, seed 3, last saved ", found[1].describe())
        self.assertEqual(launch_game.saves(self.root / "missing"), [])

    def test_corrupt_launch_record_is_treated_as_missing(self):
        path = self.save("broken")
        (path.parent / "launch.json").write_text("{not json", encoding="utf-8")
        self.assertEqual(launch_game.saves(self.saves)[0].metadata, {})
        (path.parent / "launch.json").write_text("[1, 2]", encoding="utf-8")
        self.assertEqual(launch_game.saves(self.saves)[0].metadata, {})

    def test_existing_save_resolves_by_name_directory_or_file(self):
        path = self.save("mine", {"seed": 9})
        for value in ("mine", str(path.parent), str(path)):
            with self.subTest(value=value):
                save = launch_game.resolve_save(value, self.saves)
                self.assertEqual((save.path, save.metadata), (path, {"seed": 9}))
        with self.assertRaisesRegex(LaunchError, "No save found"):
            launch_game.resolve_save("absent", self.saves)


class Validation(LauncherDirectory):
    def test_seed_accepts_the_server_range_only(self):
        self.assertEqual(launch_game.parse_seed(" 42 "), 42)
        self.assertEqual(launch_game.parse_seed(str(2**64 - 1)), 2**64 - 1)
        for text in ("-1", str(2**64), "abc", "", "1.5"):
            with self.subTest(text=text), self.assertRaisesRegex(LaunchError, "Seed must be"):
                launch_game.parse_seed(text)

    def test_new_save_names_are_fresh_and_cannot_escape_the_saves_directory(self):
        self.assertEqual(launch_game.new_save(" My game_2.1 ", self.saves), self.saves / "My game_2.1" / "game.db")
        self.save("taken")
        with self.assertRaisesRegex(LaunchError, "already exists"):
            launch_game.new_save("taken", self.saves)
        for name in ("", "../escape", "a/b", "a\\b", ".hidden", "trailing.", "x" * 65, "C:", "star*"):
            with self.subTest(name=name), self.assertRaisesRegex(LaunchError, "Save names use"):
                launch_game.new_save(name, self.saves)

    def test_default_name_is_timestamped(self):
        self.assertEqual(launch_game.default_name(datetime.datetime(2026, 10, 12, 9, 5, 7)), "game-20261012-090507")

    def test_scenario_accepts_a_name_or_a_package_directory(self):
        available = launch_game.scenarios(self.scenarios)
        self.assertEqual(launch_game.scenario_path("two-room", available), self.scenarios / "two-room")
        self.assertEqual(launch_game.scenario_path(str(self.scenarios / "first-dungeon"), available),
                         (self.scenarios / "first-dungeon").resolve())
        with self.assertRaisesRegex(LaunchError, "Unknown scenario 'tests'"):
            launch_game.scenario_path("tests", available)

    def test_resumed_scenario_prefers_recorded_path_then_current_package_name(self):
        available = launch_game.scenarios(self.scenarios)
        moved = Save(self.saves / "a/game.db", {"scenario": "two-room", "scenario_path": str(self.root / "gone")})
        self.assertEqual(launch_game.resume_scenario(moved, available), self.scenarios / "two-room")
        recorded = Save(self.saves / "a/game.db", {"scenario": "two-room",
                                                    "scenario_path": str(self.scenarios / "first-dungeon")})
        self.assertEqual(launch_game.resume_scenario(recorded, available), self.scenarios / "first-dungeon")
        self.assertIsNone(launch_game.resume_scenario(Save(self.saves / "a/game.db", {}), available))


class Menus(LauncherDirectory):
    def test_new_game_menus_cover_client_spectator_scenario_seed_and_name(self):
        prompter = Answers("1", "3", "", "3", "77", "first run")
        choices = self.choose(prompter)
        self.assertEqual(choices, Choices("text", "ascii", self.saves / "first run" / "game.db", True,
                                          self.scenarios / "two-room", 77))
        for label in ("Player client:", "Spectator:", "Game:", "New game", "Scenario:", "Seed [", "Save name [game-"):
            self.assertIn(label, prompter.text)

    def test_defaults_are_ascii_no_spectator_new_rogue_exploration_random_seed(self):
        prompter = Answers("", "", "", "", "", "")
        with patch.object(launch_game.random, "randrange", return_value=1234):
            choices = self.choose(prompter)
        self.assertEqual((choices.client, choices.spectator, choices.new, choices.scenario.name, choices.seed),
                         ("ascii", "none", True, "rogue-exploration", 1234))
        self.assertRegex(choices.save.parent.name, r"^game-\d{8}-\d{6}$")

    def test_invalid_answers_are_explained_and_asked_again(self):
        self.save("taken")
        prompter = Answers("9", "x", "1", "1", "1", "2", "-5", "8", "taken", "fresh")
        choices = self.choose(prompter)
        self.assertEqual((choices.client, choices.seed, choices.save.parent.name), ("text", 8, "fresh"))
        self.assertIn("Enter a number from 1 to 2.", prompter.text)
        self.assertIn("Seed must be", prompter.text)
        self.assertIn("already exists", prompter.text)

    def test_resume_lists_saves_and_uses_the_recorded_scenario_without_a_seed(self):
        path = self.save("journey", {"scenario": "first-dungeon", "seed": 5})
        prompter = Answers("2", "1", "2")
        choices = self.choose(prompter)
        self.assertEqual(choices, Choices("ascii", "none", path, False, self.scenarios / "first-dungeon", None))
        self.assertIn("2) journey (first-dungeon, seed 5, last saved", prompter.text)
        self.assertNotIn("Seed", prompter.text)

    def test_unrecorded_save_asks_which_scenario_it_came_from(self):
        path = self.save("legacy", file="game.json")
        prompter = Answers("1", "1", "2", "3")
        choices = self.choose(prompter)
        self.assertEqual((choices.save, choices.scenario), (path, self.scenarios / "two-room"))
        self.assertIn("This save has no launch record", prompter.text)

    def test_resume_from_another_path_retries_until_a_save_is_found(self):
        elsewhere = self.root / "elsewhere"
        elsewhere.mkdir()
        (elsewhere / "game.json").write_bytes(b"save")
        prompter = Answers("2", "1", "2", str(self.root / "nothing"), str(elsewhere), "1")
        choices = self.choose(prompter)
        self.assertEqual((choices.save, choices.new), (elsewhere / "game.json", False))
        self.assertIn("No save found", prompter.text)

    def test_options_skip_their_menus(self):
        prompter = Answers()
        choices = self.choose(prompter, client="text", spectator="none", scenario="two-room", seed="4", new="named")
        self.assertEqual(choices, Choices("text", "none", self.saves / "named" / "game.db", True,
                                          self.scenarios / "two-room", 4))
        self.assertEqual(prompter.text, "")

    def test_bare_new_option_uses_a_default_name_without_prompting(self):
        choices = self.choose(Answers(), client="text", spectator="none", scenario="two-room", seed="4", new="")
        self.assertRegex(choices.save.parent.name, r"^game-\d{8}-\d{6}$")

    def test_save_option_resumes_and_scenario_option_overrides_the_record(self):
        path = self.save("journey", {"scenario": "first-dungeon"})
        choices = self.choose(Answers(), client="ascii", spectator="text", save="journey", scenario="two-room")
        self.assertEqual((choices.save, choices.scenario, choices.seed, choices.new),
                         (path, self.scenarios / "two-room", None, False))

    def test_input_ending_mid_menu_raises_eof_rather_than_looping(self):
        with self.assertRaises(EOFError):
            self.choose(Answers("1", "1", "1"))

    def test_piped_prompt_reads_one_line_without_consuming_the_next(self):
        read, write = os.pipe()
        os.write(write, "first\r\nsecond\n".encode())
        os.close(write)
        saved = os.dup(0)
        try:
            os.dup2(read, 0)
            with patch.object(launch_game.sys.stdin, "isatty", return_value=False):
                self.assertEqual(Prompter._stdin_line(), "first")
                self.assertEqual(os.read(0, 100), b"second\n")
                with self.assertRaises(EOFError):
                    Prompter._stdin_line()
        finally:
            os.dup2(saved, 0)
            os.close(saved)
            os.close(read)


class Commands(LauncherDirectory):
    def setUp(self):
        super().setUp()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name in ("tor-server", "tor-client-text", "tor-client-ascii", "tor-client-headless"):
            (self.bin / (name + launch_game.SUFFIX)).write_bytes(b"")

    def test_server_command_passes_seed_only_for_new_games(self):
        new = Choices("text", "none", self.saves / "n/game.db", True, self.scenarios / "two-room", 6)
        command = launch_game.server_command(self.bin, new)
        self.assertEqual(command[1:], ["--listen", "127.0.0.1:0", "--scenario", str(self.scenarios / "two-room"),
                                       "--save", str(self.saves / "n/game.db"), "--seed", "6"])
        resumed = Choices("text", "none", self.saves / "n/game.db", False, self.scenarios / "two-room", None)
        self.assertNotIn("--seed", launch_game.server_command(self.bin, resumed))

    def test_clients_connect_as_actor_one_and_ascii_reports_frames_only_on_request(self):
        for client in ("text", "ascii"):
            self.assertEqual(launch_game.client_command(self.bin, client, "127.0.0.1:9")[1:],
                             ["--connect", "127.0.0.1:9", "--actor", "1"])
        self.assertEqual(launch_game.client_command(self.bin, "ascii", "127.0.0.1:9", True)[-1], "--report-frames")
        self.assertNotIn("--report-frames", launch_game.client_command(self.bin, "text", "127.0.0.1:9", True))

    def test_missing_binary_names_the_build_command(self):
        (self.bin / ("tor-client-ascii" + launch_game.SUFFIX)).unlink()
        with self.assertRaisesRegex(LaunchError, "Missing executable: .*cargo build --release"):
            launch_game.client_command(self.bin, "ascii", "127.0.0.1:9")

    def test_credentials_are_fresh_distinct_and_spectator_only_when_requested(self):
        player, spectator = launch_game.credentials("ascii")
        self.assertGreaterEqual(len(player), 16)
        self.assertNotEqual(player, spectator)
        self.assertNotEqual(player, launch_game.credentials("none")[0])
        self.assertIsNone(launch_game.credentials("none")[1])

    def test_environments_never_leak_other_roles_credentials(self):
        inherited = {"TOR_SERVER_TOKEN": "outer", "TOR_SPECTATOR_TOKEN": "outer-s", "TOR_WIZARD_TOKEN": "outer-w"}
        with patch.dict(os.environ, inherited):
            server = launch_game.server_environment("p" * 32, None)
            self.assertEqual(server["TOR_SERVER_TOKEN"], "p" * 32)
            self.assertNotIn("TOR_SPECTATOR_TOKEN", server)
            self.assertNotIn("TOR_WIZARD_TOKEN", server)
            self.assertEqual(launch_game.server_environment("p" * 32, "s" * 32)["TOR_SPECTATOR_TOKEN"], "s" * 32)
            client = launch_game.client_environment("s" * 32)
            self.assertEqual(client["TOR_SERVER_TOKEN"], "s" * 32)
            self.assertFalse({"TOR_SPECTATOR_TOKEN", "TOR_WIZARD_TOKEN"} & client.keys())

    def test_text_spectator_without_windows_consoles_fails_before_creating_anything(self):
        choices = Choices("ascii", "text", self.saves / "n/game.db", True, self.scenarios / "two-room", 1)
        with patch.object(launch_game, "WINDOWS", False), patch.object(launch_game.subprocess, "Popen") as popen:
            with self.assertRaisesRegex(LaunchError, "choose an ASCII spectator"):
                launch_game.launch(choices, self.bin, write=lambda text: None)
        popen.assert_not_called()
        self.assertFalse((self.saves / "n").exists())

    def test_command_line_rejects_a_seed_for_a_resumed_save(self):
        with self.assertRaises(SystemExit), patch("sys.stderr"):
            launch_game.main(["--save", "x", "--seed", "3"])

    def test_choice_errors_are_reported_without_a_traceback(self):
        with patch("sys.stderr") as stderr:
            code = launch_game.main(["--client", "text", "--spectator", "none", "--new", "x", "--seed", "1",
                                     "--scenario", "absent", "--scenarios-dir", str(self.scenarios),
                                     "--saves-dir", str(self.saves), "--bin-dir", str(self.bin)])
        self.assertEqual(code, 2)
        self.assertIn("Error: Unknown scenario 'absent'", "".join(call.args[0] for call in stderr.write.call_args_list))
        self.assertFalse((self.saves / "x").exists())


if __name__ == "__main__":
    unittest.main()
