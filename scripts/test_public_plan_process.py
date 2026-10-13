"""Paid public-plan disclosure through the real server and ordinary clients."""
import json
import unittest

from process_harness import ProcessTestCase, SPECTATOR_TOKEN, content_ids
from public_plan_package import paid_preparation, public_plan_package


def combat(frame):
    return frame["state"]["observation"]["combat"]


def record(frame):
    return combat(frame)["public_plan"]


class PublicPlanProcesses(ProcessTestCase):
    def test_headless_paid_record_is_frozen_for_spectators_pause_save_and_reconnect(self):
        package = public_plan_package(self)
        server = self.server(scenario=package)
        player, initial = self.client()
        subject, subject_initial = self.client(actor=2)
        observed = paid_preparation(self, player, initial, subject, subject_initial)
        frozen = record(observed)
        self.assertIsNone(frozen["state"]["ability"])
        encoded = json.dumps(frozen, sort_keys=True)
        for hidden in ("target_actor", "location", "region", "slow_plan", "Slow Plan"):
            self.assertNotIn(hidden, encoded)
        subject_before = self.request(subject, {"type": "snapshot"})
        self.assertIsNone(record(subject_before))
        self.assertTrue(combat(subject_before)["preparation_active"])
        intention = next(row["intention"] for row in subject_before["intentions"]
                         if row["phase"] == "started")
        spectator, watched = self.client(SPECTATOR_TOKEN)
        self.assertEqual(record(watched), frozen)
        for _ in range(2):
            self.assertEqual(record(self.request(player, {"type": "snapshot"})), frozen)
        released = self.request(subject, {"type": "release_control"})
        self.assertIsNone(released["error"])
        self.assertFalse(released["has_control"])
        # Releasing control acknowledges the request before the scheduler
        # commits the pause. Wait for the matching ordered lifecycle update.
        def paused_intention(frame):
            message = frame.get("message") or {}
            body = (message.get("update") or {}).get("body") or {}
            status = body.get("status") or {}
            return (body.get("type") == "intention"
                    and status.get("intention") == intention and status.get("phase") == "paused")

        paused = self.frame(subject, paused_intention)
        self.assertFalse(combat(paused)["preparation_active"])
        self.assertEqual(combat(paused)["preparation_remaining"],
                         combat(subject_before)["preparation_remaining"])
        self.assertEqual(record(self.request(spectator, {"type": "snapshot"})), frozen)
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        server.stop()
        player.stop()
        subject.stop()
        spectator.stop()
        self.server(scenario=package, seed=None)
        player, restored = self.client()
        self.assertEqual(record(restored), frozen)
        _, watched = self.client(SPECTATOR_TOKEN)
        self.assertEqual(record(watched), frozen)

    def test_rewind_restores_the_observers_pre_payment_record(self):
        package = public_plan_package(self, known=True)
        self.server(scenario=package, wizard=True)
        player, initial = self.client()
        subject, subject_initial = self.client(actor=2)
        self.assertIsNone(record(initial))
        observed = paid_preparation(self, player, initial, subject, subject_initial)
        self.assertEqual(record(observed)["state"]["ability"], {"id": "slow_plan", "name": "Slow Plan"})
        wizard = self.wizard()
        self.wizard_command(wizard, "rewind initial")
        reset = self.request(player, {"type": "snapshot"})
        self.assertIsNone(record(reset))
        self.assertNotEqual(reset["branch"], observed["branch"])

    def test_text_reads_the_same_historical_record_after_payment(self):
        package = public_plan_package(self)
        self.server(scenario=package)
        observer, _ = self.client(observe=True)
        subject, subject_initial = self.client(actor=2)
        text, _ = self.adventure()
        narration = self.say(text, "read the rhythm subject")
        self.assertIn("You prepare to use Read the Rhythm.", narration)
        target = next(actor["id"] for actor in subject_initial["state"]["observation"]["visible_actors"]
                      if actor["name"] == "observer")
        self.assertIsNone(self.act(subject, {"type": "use_ability", "id": "slow_plan", "target": target})["error"])
        observed = self.frame(observer, lambda frame: frame.get("state") is not None and record(frame) is not None)
        self.assertIsNone(record(observed)["state"]["ability"])
        first = self.say(text, "look")
        second = self.say(text, "look")
        for description in (first, second):
            self.assertIn("Observed at tick 100: subject was preparing; ability undisclosed;", description)
            self.assertIn("200 work remaining.", description)
            self.assertNotIn("Slow Plan", description)
        self.assertEqual(record(self.request(observer, {"type": "snapshot"})), record(observed))


class PublicPlanNativeProcesses(ProcessTestCase):
    graphical = True

    def test_native_ability_menu_and_stats_present_the_paid_historical_record(self):
        package = public_plan_package(self)
        self.server(scenario=package)
        window, initial = self.window()
        subject, subject_initial = self.client(actor=2)
        menu = self.key(window, "abilities")
        index = content_ids(menu["ability_choices"]).index("read_the_rhythm")
        for _ in range(index):
            self.key(window, "down")
        selected = self.key(window, "enter")
        self.assertEqual(selected["selected_ability"]["id"], "read_the_rhythm")
        self.assertEqual(selected["state"], initial["state"])
        self.key(window, "enter", wait_for_simulation=False)
        target = next(actor["id"] for actor in subject_initial["state"]["observation"]["visible_actors"]
                      if actor["name"] == "observer")
        self.assertIsNone(self.act(subject, {"type": "use_ability", "id": "slow_plan", "target": target})["error"])
        observed = self.ascii_frame(window, lambda frame: record(frame) is not None)
        frozen = record(observed)
        self.assertEqual(frozen["observed_at"], "100")
        self.assertEqual(frozen["state"]["remaining_work"], "200")
        self.assertIsNone(frozen["state"]["ability"])
        stats = self.key(window, "stats")
        self.assertTrue(stats["window_open"])
        self.assertTrue(stats["screen"]["stats"])
        rows = "\n".join(stats["stats_rows"])
        self.assertIn("Observed at tick 100: subject was preparing; ability undisclosed;", rows)
        self.assertIn("200 work remaining.", rows)
        self.assertNotIn("Slow Plan", rows)
        self.assertEqual(record(stats), frozen)
        self.key(window, "escape")
        repeated = self.key(window, "stats")
        self.assertEqual(record(repeated), frozen)


if __name__ == "__main__":
    unittest.main()
