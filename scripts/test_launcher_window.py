"""Unit tests for the graphical launcher window, with a real (hidden) Tk root.

Games are not started: `start` records the choices instead, and the file
dialog is replaced by a function returning a chosen path.
"""
import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch

import tkinter as tk

import launch_game
import launcher_window
from launch_game import Choices
from test_launch_game import LauncherDirectory


class WindowTests(LauncherDirectory):
    def setUp(self):
        super().setUp()
        self.tk = tk.Tk()
        self.tk.withdraw()
        self.addCleanup(self.destroy)
        self.started = []
        self.browse_result = None
        self.bin = self.root / "bin"

    def destroy(self):
        try:
            self.tk.destroy()
        except tk.TclError:
            pass

    def window(self, **initial):
        window = launcher_window.LauncherWindow(
            self.tk, bin_dir=self.bin, scenarios_dir=self.scenarios, saves_dir=self.saves,
            start=lambda choices, session: self.started.append((choices, session)),
            ask_save_file=lambda: self.browse_result, initial=initial)
        self.tk.update()
        return window

    def test_defaults_offer_ascii_no_spectator_and_a_new_rogue_exploration_game(self):
        window = self.window()
        state = window.state()
        self.assertEqual((state["client"], state["spectator"], state["tab"], state["scenario"]),
                         ("ascii", "none", "new", "rogue-exploration"))
        self.assertEqual(state["scenarios"], ["first-dungeon", "rogue-exploration", "two-room"])
        self.assertRegex(state["name"], r"^game-\d{8}-\d{6}$")
        self.assertTrue(0 <= int(state["seed"]) < 2**32)
        self.assertTrue(state["play_enabled"])
        self.assertFalse(state["end_enabled"])

    def test_new_game_choices_come_from_the_widgets(self):
        window = self.window()
        window.client.set("text")
        window.spectator.set("ascii")
        window.scenario.set("two-room")
        window.seed.set("77")
        window.name.set("first run")
        window.play_button.invoke()
        self.assertEqual(self.started[0][0], Choices("text", "ascii", self.saves / "first run" / "game.db",
                                                     True, self.scenarios / "two-room", 77))
        self.assertIsInstance(self.started[0][1], launch_game.Session)

    def test_invalid_new_game_choices_are_shown_and_nothing_starts(self):
        self.save("taken")
        window = self.window()
        for field, value, message in (("seed", "-3", "Seed must be"), ("seed", "abc", "Seed must be"),
                                      ("name", "taken", "already exists"), ("name", "../x", "Save names use")):
            with self.subTest(field=field, value=value):
                window.seed.set("1")
                window.name.set("fine")
                getattr(window, field).set(value)
                window.play_button.invoke()
                self.assertIn(message, window.status.get())
                self.assertTrue(window.state()["error"])
        self.assertEqual(self.started, [])

    def test_random_button_rolls_a_new_valid_seed(self):
        window = self.window()
        with patch.object(launcher_window.random, "randrange", return_value=31337):
            window.random_button.invoke()
        self.assertEqual(window.seed.get(), "31337")

    def test_saved_games_tab_lists_saves_newest_first_with_their_records(self):
        self.save("old", {"scenario": "two-room", "seed": 3}, mtime=1_000_000)
        self.save("legacy", file="game.json", mtime=2_000_000)
        window = self.window()
        saves = window.state()["saves"]
        self.assertEqual([(s["name"], s["scenario"], s["seed"]) for s in saves],
                         [("legacy", None, None), ("old", "two-room", 3)])
        rows = [window.tree.item(item) for item in window.tree.get_children()]
        self.assertEqual([row["text"] for row in rows], ["legacy", "old"])
        self.assertEqual(rows[1]["values"][:2], ["two-room", 3])
        self.assertEqual(rows[0]["values"][:2], ["unknown", ""])

    def test_resuming_a_selected_save_uses_its_recorded_scenario(self):
        path = self.save("journey", {"scenario": "first-dungeon", "seed": 5})
        window = self.window()
        window.tabs.select(window.saved_tab)
        window.play_button.invoke()
        self.assertIn("Choose a saved game", window.status.get())
        self.assertTrue(window.select_save("journey"))
        self.tk.update()
        state = window.state()
        self.assertEqual((state["origin"], state["origin_enabled"]), ("first-dungeon", False))
        window.play_button.invoke()
        self.assertEqual(self.started[0][0], Choices("ascii", "none", path, False, self.scenarios / "first-dungeon", None))

    def test_unrecorded_save_requires_choosing_its_scenario(self):
        path = self.save("legacy", file="game.json")
        window = self.window()
        window.select_save("legacy")
        self.assertTrue(window.state()["origin_enabled"])
        window.play_button.invoke()
        self.assertIn("choose the scenario it started from", window.status.get())
        window.origin.set("two-room")
        window.play_button.invoke()
        self.assertEqual(self.started[0][0].scenario, self.scenarios / "two-room")
        self.assertEqual(self.started[0][0].save, path)

    def test_browse_adds_a_save_from_anywhere_and_keeps_it_after_refresh(self):
        elsewhere = self.root / "elsewhere"
        elsewhere.mkdir()
        (elsewhere / "game.json").write_bytes(b"save")
        (elsewhere / "launch.json").write_text(json.dumps({"scenario": "two-room", "seed": 8}), encoding="utf-8")
        window = self.window()
        self.browse_result = str(elsewhere / "game.json")
        window.browse_button.invoke()
        state = window.state()
        self.assertEqual((state["tab"], state["selected"], state["origin"]), ("saved", str(elsewhere / "game.json"), "two-room"))
        window.refresh_saves()
        self.assertIn(str(elsewhere / "game.json"), [s["path"] for s in window.state()["saves"]])
        window.select_save(str(elsewhere / "game.json"))
        window.play_button.invoke()
        self.assertEqual(self.started[0][0].save, elsewhere / "game.json")

    def test_browse_rejects_files_that_are_not_saves_and_cancel_changes_nothing(self):
        window = self.window()
        self.browse_result = ""
        window.browse_button.invoke()
        self.assertEqual((window.state()["saves"], window.status.get()), ([], ""))
        self.browse_result = str(self.root / "missing.db")
        window.browse_button.invoke()
        self.assertIn("No save found", window.status.get())

    def test_running_game_disables_choices_and_finishing_lists_the_new_save(self):
        window = self.window(scenario="two-room", seed="4", new="fresh")
        window.play_button.invoke()
        choices, session = self.started[0]
        state = window.state()
        self.assertTrue(state["running"])
        self.assertFalse(state["play_enabled"])
        self.assertTrue(state["end_enabled"])
        self.assertIn("disabled", window.client_buttons["text"].state())
        window.play_button.invoke()
        self.assertEqual(len(self.started), 1)
        # The worker reports the game ending; the save now exists.
        choices.save.parent.mkdir(parents=True)
        choices.save.write_bytes(b"save")
        window.handle(("finished", choices, None))
        state = window.state()
        self.assertFalse(state["running"])
        self.assertTrue(state["play_enabled"])
        self.assertEqual((state["tab"], state["selected"]), ("saved", str(choices.save)))
        self.assertIn("Game saved in", state["status"])
        self.assertNotEqual(state["name"], "fresh")

    def test_end_game_ends_the_session_and_errors_from_the_game_are_shown(self):
        window = self.window(scenario="two-room", seed="4", new="fresh")
        window.play_button.invoke()
        choices, session = self.started[0]
        window.end_button.invoke()
        self.assertTrue(session.ended)
        window.handle(("finished", choices, "The server stopped during startup: boom"))
        self.assertIn("boom", window.status.get())
        self.assertTrue(window.state()["error"])

    def test_closing_during_a_game_ends_it_first(self):
        window = self.window(scenario="two-room", seed="4", new="fresh")
        window.play_button.invoke()
        choices, session = self.started[0]
        window.close()
        self.assertTrue(session.ended)
        self.assertFalse(window.destroyed)
        window.handle(("finished", choices, None))
        self.assertTrue(window.destroyed)

    def test_options_prefill_the_window(self):
        path = self.save("journey", {"scenario": "two-room"})
        window = self.window(client="text", spectator="ascii", save="journey")
        state = window.state()
        self.assertEqual((state["client"], state["spectator"], state["tab"], state["selected"]),
                         ("text", "ascii", "saved", str(path)))

    def test_text_spectator_is_unavailable_without_windows_consoles(self):
        with patch.object(launch_game, "WINDOWS", False):
            window = self.window(spectator="text")
            self.assertEqual(window.spectator.get(), "none")
            self.assertIn("disabled", window.spectator_buttons["text"].state())
            window.spectator.set("text")
            window.play_button.invoke()
        self.assertIn("choose an ASCII spectator", window.status.get())
        self.assertEqual(self.started, [])


class ConsolePython(unittest.TestCase):
    def test_client_windows_use_a_console_python_even_under_pythonw(self):
        directory = Path(launch_game.ROOT / "target" / "launcher-unit")
        directory.mkdir(parents=True, exist_ok=True)
        suffix = ".exe" if os.name == "nt" else ""
        (directory / ("python" + suffix)).write_bytes(b"")
        with patch.object(launch_game.sys, "executable", str(directory / ("pythonw" + suffix))):
            self.assertEqual(launch_game.console_python(), str(directory / ("python" + suffix)))
        with patch.object(launch_game.sys, "executable", str(directory / ("python3" + suffix))):
            self.assertEqual(launch_game.console_python(), str(directory / ("python3" + suffix)))


if __name__ == "__main__":
    unittest.main()
