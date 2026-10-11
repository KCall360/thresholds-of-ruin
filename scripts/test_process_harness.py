"""Behavior of the shared actual-process harness without launching applications."""
import unittest
from unittest.mock import patch

from process_harness import ProcessTestCase


class ActionCompletion(unittest.TestCase):
    def test_decimal_readiness_revisions_wait_for_the_newer_permission_frame(self):
        for revision in [9, 99, 9007199254740991, 9007199254740992, 18446744073709551614]:
            with self.subTest(revision=revision):
                harness = ProcessTestCase()
                accepted = {"intentions": [{"phase": "queued", "intention": "admission-1"}]}
                executed = {
                    "message": {"type": "update", "update": {"body": {
                        "type": "intention", "status": {
                            "intention": "admission-1", "phase": "resolved"}}}},
                    "readiness": {"revision": str(revision)},
                }
                newer = {"readiness": {"revision": str(revision + 1)}}
                batches = [[executed], [executed, newer]]

                def frame(client, predicate):
                    selected = next((item for item in batches.pop(0) if predicate(item)), None)
                    self.assertIsNotNone(selected, "the newer permission frame was not selected")
                    return selected

                with patch.object(harness, "command", return_value=accepted), \
                        patch.object(harness, "frame", side_effect=frame):
                    self.assertIs(harness.act(object(), {"type": "wait"}), newer)
                self.assertEqual(batches, [])


if __name__ == "__main__":
    unittest.main()


class CreaturePackages(unittest.TestCase):
    def test_creature_fixture_uses_bundled_rules_and_generic_modifiers(self):
        from pathlib import Path
        import tempfile
        import tomllib
        from types import SimpleNamespace
        from process_harness import creature_package
        with tempfile.TemporaryDirectory() as directory:
            harness = ProcessTestCase()
            harness.directory = Path(directory)
            harness.bin = Path(directory)
            harness.suffix = ""
            with patch("process_harness.subprocess.run", return_value=SimpleNamespace(returncode=0, stderr="")):
                package = creature_package(harness, "creature", abilities=("fixture_ability",),
                                           ability_aliases={"fixture_ability": ("magic_bolt", "Fixture Spell")})
            manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
            self.assertTrue((package / manifest["ruleset"]).is_file())
            species = manifest["creatures"]["species"]["human"]
            rules = tomllib.loads((package / manifest["ruleset"]).read_text(encoding="utf-8"))
            self.assertTrue(any(value["id"] == "fixture_ability" and value["name"] == "Fixture Spell" for value in rules["abilities"]))
            self.assertNotIn("grants", species)
            self.assertIn({"scope": "always", "op": {"kind": "grant_ability", "ability": "fixture_ability"}}, species["modifiers"])
