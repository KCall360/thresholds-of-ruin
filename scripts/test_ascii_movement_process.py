"""Native run and height-aware bump input through the actual ASCII application."""
import shutil
import subprocess
import tomllib
import unittest

from arena_matrix import toml_source
from process_harness import ROOT, ProcessTestCase, SPECTATOR_TOKEN, ascii_input_completion
from rogue_roster_package import roster_package


class AsciiMovementProcesses(ProcessTestCase):
    graphical = True

    def test_native_shift_run_waits_for_other_actors_before_choosing_another_step(self):
        package = self.directory / "ascii-run-two-actors"
        shutil.copytree(ROOT / "scenarios/tests/travel-hazard", package)
        region_path = package / "regions/4.toml"
        region = tomllib.loads(region_path.read_text(encoding="utf-8"))
        # Keep the ordinary external actor beyond sight for both tested steps.
        # No AI, wizard action or altered protocol delays the scheduler boundary.
        region["actors"][0]["at"] = [19, 0, 0]
        region_path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stdout + validated.stderr)
        self.server(scenario=package)
        partner, _ = self.client(actor=2)
        observer, _ = self.client(SPECTATOR_TOKEN)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        self.ascii_frame(window, lambda frame: frame.get("has_control") and not frame["busy"])

        def actions(frame):
            return [row["content"]["event"] for row in frame["history"]
                    if row["content"]["type"] == "action"]

        def awaiting_partner(count):
            def boundary(frame):
                return (frame.get("state") is not None
                        and not frame["state"]["observation"]["ready"]
                        and frame["can_admit_intention"]
                        and len(actions(frame)) == count)
            return boundary

        def assert_waiting(frame, count):
            self.assertTrue(frame["window_open"])
            self.assertFalse(frame["busy"], "automatic running must not queue its next step early")
            self.assertEqual(frame["screen"]["running"], "east")
            self.assertEqual(frame["intentions"], [])
            view = frame["state"]["observation"]
            self.assertTrue(all(actor["id"] == view["self_target"] for actor in view["visible_actors"]))
            self.assertEqual(actions(frame), [{"type": "moved", "direction": "east"}] * count)
            authoritative = self.request(observer, {"type": "snapshot"})
            self.assertDisclosedStateEqual(authoritative["state"], frame["state"])
            self.assertEqual(authoritative["intentions"], [])

        native = self.native_keys(window)
        native("Shift_L", True)
        native("Right", True)
        try:
            first = self.ascii_frame(window, awaiting_partner(1))
            assert_waiting(first, 1)
        finally:
            native("Right", False)
            native("Shift_L", False)
        # Acknowledging the physical release is independent of completing a run.
        self.frame(window, lambda record: record.get("type") == "native_release")
        self.assertIsNone(self.act(partner, {"type": "wait"})["error"])
        second = self.ascii_frame(window, awaiting_partner(2))
        assert_waiting(second, 2)
        stopped = self.native_press(window, native, "Right", "right", wait_for_simulation=False)
        self.assertIsNone(stopped["screen"]["running"])
        self.assertEqual(stopped["status"], "You stop running.")
        self.assertEqual(actions(stopped), actions(second))
        self.assertEqual(self.request(observer, {"type": "snapshot"})["intentions"], [])

    def test_native_shift_direction_runs_until_the_known_corridor_ends(self):
        self.server(scenario="travel")
        observer, initial = self.client(SPECTATOR_TOKEN)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        presented = self.ascii_frame(window, lambda frame: frame.get("has_control") and not frame["busy"])
        self.assertTrue(presented["window_open"])
        native = self.native_keys(window)
        native("Shift_L", True)
        native("Right", True)
        try:
            completed = self.ascii_frame(window, ascii_input_completion("run_right", wait_for_admission=True))
        finally:
            native("Right", False)
            native("Shift_L", False)
        self.assertTrue(completed["window_open"])
        self.assertIsNone(completed["screen"]["running"])
        self.assertEqual(int(completed["state"]["observation"]["tick"])
                         - int(initial["state"]["observation"]["tick"]), 700)
        actions = [row["content"]["event"] for row in completed["history"]
                   if row["content"]["type"] == "action"]
        self.assertEqual(len(actions), 7)
        self.assertTrue(all(event == {"type": "moved", "direction": "east"} for event in actions))
        authoritative = self.request(observer, {"type": "snapshot"})
        self.assertDisclosedStateEqual(authoritative["state"], completed["state"])

    def test_native_bump_attacks_the_production_kestrel_above_foot_height(self):
        package = roster_package(self, "kestrel", height=5)
        self.server(scenario=package, seed=42)
        player, _ = self.client()
        foe, _ = self.client(actor=3)
        self.assertIsNone(self.command(foe, {"type": "act", "action": {
            "type": "move", "direction": "up"}})["error"])
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        raised = self.request(player, {"type": "snapshot"})
        kestrel = next(actor for actor in raised["state"]["observation"]["visible_actors"]
                       if actor["name"] == "kestrel")
        self.assertEqual(kestrel["position"], {"x": 1, "y": 0, "z": 1})
        player.stop()
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        initial = self.ascii_frame(window, lambda frame: frame.get("has_control") and not frame["busy"])
        self.assertTrue(any(tile["glyph"] == "K" and tile["kind"] == "creature"
                            for tile in initial["map_tiles"]))
        native = self.native_keys(window)
        native("Right", True)
        try:
            completed = self.ascii_frame(window, ascii_input_completion("right"))
        finally:
            native("Right", False)
        starts = [row["content"]["event"] for row in completed["history"]
                  if row["content"].get("event", {}).get("type") == "ability_started"]
        self.assertEqual(len(starts), 1)
        self.assertEqual(starts[0]["ability"]["id"], "basic_melee")
        self.assertEqual(starts[0]["target"], kestrel["id"])
        self.assertTrue(completed["window_open"])


if __name__ == "__main__":
    unittest.main()
