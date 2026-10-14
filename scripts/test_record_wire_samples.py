"""Pure message-selection and authored-fixture contracts for the wire recorder."""
import copy
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch

import nutrition_package
from process_harness import ROOT
from record_wire_samples import has_condition_event


class ConditionEventSelection(unittest.TestCase):
    def test_full_and_delta_updates_select_only_the_standalone_condition(self):
        event = {"type": "condition", "condition": {"id": "fixture_stupor", "name": "Fixture Faint"}}
        for kind, state in (
                ("observation", {"observation": {"combat": {"events": [event]}}}),
                ("observation_delta", {"combat": {"events": [event]}})):
            with self.subTest(kind=kind):
                message = {"type": "update", "update": {"body": {"type": kind, "state": state}}}
                unchanged = copy.deepcopy(message)
                self.assertTrue(has_condition_event(message, "fixture_stupor"))
                self.assertFalse(has_condition_event(message, "another_status"))
                self.assertEqual(message, unchanged, "Selection must preserve the original server message")

    def test_ability_details_and_unrelated_messages_are_not_standalone_events(self):
        nested = {"type": "ability", "condition": {"id": "fixture_stupor"}}
        for message in (
                None,
                {"type": "snapshot"},
                {"type": "update", "update": {"body": {"type": "intention"}}},
                {"type": "update", "update": {"body": {"type": "observation_delta", "state": {}}}},
                {"type": "update", "update": {"body": {"type": "observation",
                    "state": {"observation": {"combat": None}}}}},
                {"type": "update", "update": {"body": {"type": "observation_delta",
                    "state": {"combat": {"events": [nested]}}}}}):
            with self.subTest(message=message):
                self.assertFalse(has_condition_event(message, "fixture_stupor"))


class NutritionTriggerFixtureContracts(unittest.TestCase):
    def test_shared_fixture_preserves_remove_default_and_authors_optional_protection(self):
        bundled = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text(encoding="utf-8"))

        def resolved(path, compiler):
            authored = tomllib.loads(path.read_text(encoding="utf-8"))
            return copy.deepcopy(bundled) if "import" in authored else authored

        for ticks in (None, 400):
            with self.subTest(protection_ticks=ticks), tempfile.TemporaryDirectory() as directory:
                self.directory = Path(directory)
                self.bin = self.directory / "unused-binaries"
                self.suffix = ""
                # This test checks authored operands only. The process suite
                # validates them with the real compiler and runs the recorder.
                with patch("nutrition_package.load_rules", side_effect=resolved), \
                        patch("nutrition_package.subprocess.run", return_value=subprocess.CompletedProcess(
                            [], 0, stderr="")) as validate:
                    ordinary = nutrition_package.prepare(self)
                    package = nutrition_package.prepare_trigger(self, protection_ticks=ticks)
                self.assertNotEqual(package, ordinary)
                self.assertEqual(validate.call_count, 3)
                self.assertEqual(validate.call_args.args[0], [self.bin / "tor-scenario", "validate", package])
                rules = tomllib.loads((package / "rules.toml").read_text(encoding="utf-8"))
                self.assertEqual(rules["nutrition_condition_trigger"], {
                    "kind": "on_empty_depletion", "condition": "fixture_stupor", "chance": {"one_in": 5},
                    "duration": {"kind": "rolled", "dice": {"count": 1, "sides": 8, "bonus": 3}},
                    "ticks_per_unit": 100})
                condition = next(row for row in rules["conditions"] if row["id"] == "fixture_stupor")
                self.assertEqual(condition["lifecycle"], "timed")
                self.assertEqual(condition["timing"], {"kind": "actor_active", "on_expiry": (
                    {"kind": "remove"} if ticks is None else {"kind": "protect", "duration": ticks})})
                self.assertEqual(condition["restrictions"], ["voluntary_actions"])
                region = tomllib.loads((package / "regions/1.toml").read_text(encoding="utf-8"))
                peer = region["actors"][0]
                self.assertEqual(peer["turn_ticks"], 100)
                self.assertEqual(peer["creature"]["nutrition"], {"kind": "exempt"})
                manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
                self.assertEqual(manifest["id"], package.name)


if __name__ == "__main__":
    unittest.main()
