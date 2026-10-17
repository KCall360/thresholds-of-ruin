"""Automatic nutrition conditions through ordinary real server/client processes.

The tiny fixture makes deadline and suspension boundaries observable. Production
Warrior/Mage activation is covered separately without rewriting their food policy.
"""
import shutil
import subprocess
import unittest

from nutrition_package import ZERO_WORD_SEED, observed_meal_complete, prepare_trigger as trigger_package
from process_harness import ProcessTestCase, ROOT, SPECTATOR_TOKEN, read_capture, settle_rogue_action
from status_condition_package import actor_creature, checkpoint, combat, observation


GAMMA = 0x9E3779B97F4A7C15


class NutritionTriggerProcesses(ProcessTestCase):
    def test_wire_recorder_captures_both_personal_phases_and_the_automatic_event(self):
        from record_wire_samples import Recorder

        samples = {}
        Recorder.record_personal_conditions(self, samples)
        self.assertEqual(set(samples), {
            "zz.condition_active", "zz.condition_protection", "zz.condition_event"})
        for key in ("zz.condition_active", "zz.condition_protection"):
            self.assertEqual(samples[key]["type"], "snapshot")
        self.assertEqual(samples["zz.condition_event"]["type"], "update")

    def test_fainting_pauses_tied_meal_and_survives_queries_reconnect_and_source_free_restart(self):
        package = trigger_package(self)
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=ZERO_WORD_SEED)
        player, initial = self.client()
        peer, _ = self.client(actor=2)
        meal = next(item for item in observation(initial)["inventory"] if item["name"] == "fixture meal")
        self.assertIsNone(self.act(player, {"type": "eat", "item": meal["id"]})["error"])
        self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
        paused = self.request(player, {"type": "snapshot"})
        self.assertEqual(combat(paused)["own_stats"]["nutrition"]["reserve"], -1)
        self.assertFalse(combat(paused)["preparation_active"])
        self.assertEqual(paused["intentions"][0]["phase"], "paused")
        saved = checkpoint(self, player)
        self.assertEqual(saved["rules_rng"], GAMMA)
        self.assertEqual(combat(paused)["own_stats"]["conditions"], [{
            "condition": {"id": "fixture_stupor", "name": "Fixture Faint"},
            "phase": "active", "remaining_ticks": "1100"}])
        other = self.request(peer, {"type": "snapshot"})
        self.assertEqual(combat(other)["own_stats"]["conditions"], [])
        self.assertEqual(actor_creature(saved)["conditions"][0]["remaining"], 1100)
        self.assertEqual(saved["items"]["10"]["quantity"], 2)
        self.assertTrue(saved["items"]["10"]["holds"])
        spectator, _ = self.client(SPECTATOR_TOKEN)
        for client in (player, spectator):
            for _ in range(3):
                self.assertIsNone(self.request(client, {"type": "snapshot"})["error"])
                self.assertIsNone(self.request(client, {"type": "history", "limit": 50, "before": None})["error"])
        self.assertEqual(checkpoint(self, player), saved)
        spectator.stop(); player.stop(); peer.stop(); server.stop()
        for path in package.iterdir():
            if path.name != ".compiled":
                if path.is_dir():
                    shutil.rmtree(path)
                else:
                    path.unlink()
        self.server("--checkpoint-interval", 1, seed=None)
        player, restored = self.client()
        peer, _ = self.client(actor=2)
        self.assertEqual(restored["state"], paused["state"])
        self.assertEqual(checkpoint(self, player), saved)
        for _ in range(851):
            saved = checkpoint(self, player)
            if not actor_creature(saved)["conditions"]:
                break
            self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
        else:
            self.fail("The finite empty episode did not reach a permitted decision")
        released = self.request(player, {"type": "snapshot"})
        self.assertFalse(combat(released)["preparation_active"])
        self.assertEqual(released["intentions"][0]["phase"], "paused")
        self.assertIsNone(self.command(player, {"type": "resume_intention"})["error"])
        finished = self.request(player, {"type": "snapshot"})
        self.assertEqual(combat(finished)["own_stats"]["nutrition"]["reserve"], 6)
        self.assertEqual(next(item for item in observation(finished)["inventory"]
                              if item["id"] == meal["id"])["quantity"], "1")

    def test_text_reports_empty_nutrition_while_a_nondamaging_faint_denies_input(self):
        package = trigger_package(self)
        self.server("--checkpoint-interval", 1, scenario=package, seed=ZERO_WORD_SEED)
        text, _ = self.text_client()
        peer, _ = self.client(actor=2)
        observer, _ = self.client(observe=True)
        self.assertIn("Nutrition: 0/2000 (empty)", text.command("stats"))
        text.command("wait")
        self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
        shown = text.command("stats")
        self.assertIn("Nutrition: -1/2000 (empty)", shown)
        self.assertIn("Fixture Faint: 1100 active ticks remaining", shown)
        self.assertNotIn("Protection against Fixture Faint", shown)
        before = checkpoint(self, observer)
        text.command("wait")
        self.assertEqual(checkpoint(self, observer), before)
        self.assertEqual(actor_creature(before)["conditions"][0]["condition"], "fixture_stupor")


class RogueNutritionProcesses(ProcessTestCase):
    def test_both_real_presets_use_one_edible_supply_and_restart_without_sources(self):
        for actor in (1, 2):
            with self.subTest(actor=actor):
                self.save = self.directory / f"rogue-nutrition-{actor}.db"
                package = self.directory / f"rogue-nutrition-{actor}"
                shutil.copytree(ROOT / "scenarios/rogue-exploration", package,
                                ignore=shutil.ignore_patterns(".compiled"))
                compiler = self.bin / ("tor-scenario" + self.suffix)
                result = subprocess.run([compiler, "validate", package], capture_output=True,
                                        text=True, encoding="utf-8", timeout=30)
                self.assertEqual(result.returncode, 0, result.stderr)
                server = self.server("--checkpoint-interval", 1, scenario=package, character=actor, save_actor=actor)
                player, initial = self.client(actor=actor)
                own = combat(initial)["own_stats"]
                self.assertEqual(own["nutrition"]["reserve"], 1300)
                self.assertEqual(own["nutrition"]["hunger"], "nourished")
                self.assertEqual(own["conditions"], [])
                self.assertFalse(own["health_recovery"]["stopped"])
                meal = next(item for item in observation(initial)["inventory"] if item["name"] == "food ration")
                self.assertEqual(meal["known_supply"]["id"], "ration")
                self.assertTrue(next(row for row in observation(initial)["interactions"]["inventory"]
                                     if row["item"] == meal["id"])["edible"])
                result = self.act(player, {"type": "eat", "item": meal["id"]})
                self.assertIsNone(result["error"])
                eaten = settle_rogue_action(self, player, result)
                self.assertGreater(combat(eaten)["own_stats"]["nutrition"]["reserve"], 1300)
                self.assertLessEqual(combat(eaten)["own_stats"]["nutrition"]["reserve"], 2000)
                self.assertFalse(any(item["id"] == meal["id"] for item in observation(eaten)["inventory"]))
                self.assertIsNone(self.request(player, {"type": "save"})["error"])
                player.stop(); server.stop()
                for path in package.iterdir():
                    if path.name != ".compiled":
                        if path.is_dir():
                            shutil.rmtree(path)
                        else:
                            path.unlink()
                resumed = self.server(seed=None, save_actor=actor)
                player, reopened = self.client(actor=actor)
                self.assertEqual(reopened["state"], eaten["state"])
                self.assertFalse(reopened["state"]["wizard_game"])
                player.stop(); resumed.stop()

    def test_real_warrior_text_shows_nutrition_and_consumes_its_only_ration(self):
        self.server(scenario="rogue-exploration", seed=42)
        text, _ = self.text_client()
        observer, initial = self.client(observe=True)
        meal = next(item for item in observation(initial)["inventory"] if item["name"] == "food ration")
        self.assertEqual(meal["quantity"], "1")
        self.assertIn("Nutrition: 1300/2000 (nourished)", text.command("stats"))
        self.assertIn("ItemStarted", text.command("eat food ration"))
        current = self.request(observer, {"type": "snapshot"})
        def finished(frame):
            return observed_meal_complete(frame, meal["id"], 1300)
        current = current if finished(current) else self.frame(observer, finished)
        self.assertIsNone(current["error"])
        self.assertFalse(current["has_control"])
        self.assertFalse(current["state"]["wizard_game"])
        self.assertGreater(combat(current)["own_stats"]["nutrition"]["reserve"], 1300)
        self.assertFalse(any(item["name"] == "food ration" for item in observation(current)["inventory"]))
        reserve = combat(current)["own_stats"]["nutrition"]["reserve"]
        self.assertIn(f"Nutrition: {reserve}/2000 (nourished)", text.command("stats"))


class NutritionTriggerNativeProcesses(ProcessTestCase):
    graphical = True

    def test_native_personal_stats_present_fainting_and_hunger_without_causer_identity(self):
        package = trigger_package(self)
        self.server("--checkpoint-interval", 1, scenario=package, seed=ZERO_WORD_SEED)
        player, _ = self.client()
        peer, _ = self.client(actor=2)
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
        saved = checkpoint(self, player)
        capture = self.directory / "faint-stats.ppm"
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--observe",
                             "--report-frames", "--capture", capture])
        self.ascii_frame(window, lambda frame: frame["state"] is not None and not frame["busy"])
        before = read_capture(capture)
        native = self.native_keys(window)
        native("Shift_L", True)
        try:
            shown = self.native_press(window, native, "2", "stats")
        finally:
            native("Shift_L", False)
        rows = "\n".join(shown["stats_rows"])
        self.assertIn("Nutrition: -1/2000 (empty)", rows)
        self.assertIn("Fixture Faint: 1100 active ticks remaining", rows)
        self.assertNotIn("Protection against Fixture Faint", rows)
        self.assertNotIn("from actor", rows)
        self.assertNotEqual(read_capture(capture), before)
        self.assertEqual(checkpoint(self, player), saved)

    def test_native_real_mage_eats_the_authored_ration_and_shows_its_reserve(self):
        self.server(scenario="rogue-exploration", seed=42, character=2, save_actor=2)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--actor", "2", "--report-frames"])
        initial = self.ascii_frame(window, lambda frame: frame["has_control"] and not frame["busy"])
        self.assertEqual(combat(initial)["own_stats"]["nutrition"]["reserve"], 1300)
        meal = next(item for item in observation(initial)["inventory"] if item["name"] == "food ration")
        self.assertEqual(meal["quantity"], "1")
        native = self.native_keys(window)
        picker = self.native_press(window, native, "e", "eat")
        self.assertEqual(picker["state"], initial["state"])
        accepted = self.native_press(window, native, "Return", "enter")
        def finished(frame):
            return observed_meal_complete(frame, meal["id"], 1300)
        final = accepted if finished(accepted) else self.ascii_frame(window, finished)
        self.assertFalse(final["state"]["wizard_game"])
        self.assertFalse(any(item["name"] == "food ration" for item in observation(final)["inventory"]))
        native("Shift_L", True)
        try:
            shown = self.native_press(window, native, "2", "stats")
        finally:
            native("Shift_L", False)
        self.assertIn("Nutrition:", "\n".join(shown["stats_rows"]))
        self.assertTrue(shown["window_open"])


if __name__ == "__main__":
    unittest.main()
