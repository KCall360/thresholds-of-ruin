"""New helper contracts preserve catalog/kit facts and isolate diagnostics."""
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock
from process_harness import ROOT
from rogue_roster_package import SPECIES, encounter, encounter_sources, unrelated_fallback


class RogueRosterPackageTests(unittest.TestCase):
    def test_vampire_is_an_explicit_encounter_choice_for_each_actual_starter(self):
        self.assertIn("vampire", SPECIES)
        source = encounter_sources()
        for selected in (1, 2):
            manifest, region = encounter("vampire", character=selected)
            expected = dict(source["manifest"]["characters"][selected - 1], anchor="1/start")
            self.assertEqual(manifest["default_character"], selected)
            self.assertEqual(manifest["characters"], [expected])
            self.assertEqual(region["actors"][0]["archetype"], "rogue.vampire")

    def test_encounters_preserve_actual_kit_and_reference_unchanged_actor_recipe(self):
        source = encounter_sources()
        for species in SPECIES:
            for character in (1, 2):
                with self.subTest(species=species, character=character):
                    manifest, region = encounter(species, character=character)
                    original = source["manifest"]["characters"][character - 1]
                    self.assertEqual(manifest["characters"][0]["creature"], original["creature"])
                    self.assertEqual(manifest["characters"][0]["loadout"], original["loadout"])
                    self.assertEqual(region["actors"][0]["archetype"], "rogue." + species)
                    self.assertEqual(region["actors"][0]["controller"], "external")
                    self.assertNotIn("floor_activation", manifest)
                    self.assertNotIn("arena", manifest)
                    self.assertNotIn("objective", manifest)

    def test_helpers_do_not_reuse_mutable_source_or_enable_unrequested_ai(self):
        first, _ = encounter("ice_monster")
        first["characters"][0]["creature"]["hit_dice"].clear()
        second, region = encounter("ice_monster", autonomous=True)
        self.assertEqual(len(second["characters"][0]["creature"]["hit_dice"]), 1)
        self.assertEqual(region["actors"][0]["ai"], "rogue_hostile")
        for species, character in (("dragon", 1), ("emu", True), ("emu", 3)):
            with self.assertRaises(ValueError):
                encounter(species, character=character)

    def test_source_free_fallback_is_unrelated_and_requires_real_compiler_success(self):
        for result in (0, 1):
            with self.subTest(returncode=result), tempfile.TemporaryDirectory() as directory:
                test = SimpleNamespace(directory=Path(directory), bin=Path("fixture-bin"),
                                       suffix="", assertEqual=self.assertEqual)
                with mock.patch("rogue_roster_package.subprocess.run", return_value=
                                SimpleNamespace(returncode=result, stderr="validation failed")) as run:
                    if result:
                        with self.assertRaisesRegex(AssertionError, "validation failed"):
                            unrelated_fallback(test)
                    else:
                        fallback = unrelated_fallback(test)
                        self.assertEqual((fallback / "scenario.toml").read_bytes(),
                                         (ROOT / "scenarios/two-room/scenario.toml").read_bytes())
                    self.assertEqual(run.call_args.args[0],
                                     [test.bin / "tor-scenario", "validate",
                                      test.directory / "unrelated-fallback"])


if __name__ == "__main__":
    unittest.main()
