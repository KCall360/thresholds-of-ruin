"""New helper contracts preserve catalog/kit facts and isolate diagnostics."""
import copy
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock
from process_harness import ROOT
from rogue_roster_package import SPECIES, encounter, encounter_sources, unrelated_fallback


class RogueRosterPackageTests(unittest.TestCase):
    def test_production_retrieval_uses_one_selected_player_bound_ordinary_amulet(self):
        source = encounter_sources()
        policy = source['manifest'].get('unique_retrieval')
        self.assertEqual(policy, {'version': 1, 'protagonist': 'selected_character',
            'family': 'dungeon', 'minimum_depth': 26, 'archetype': 'rogue.amulet_of_yendor',
            'exit_anchor': 'dungeon/1/up'})
        item = source['items']['rogue.amulet_of_yendor']
        self.assertEqual(item, {'name': 'Amulet of Yendor', 'identity': 'rogue.amulet_of_yendor',
                                'class': 'amulet', 'asset': 'item.amulet', 'stackable': False,
                                'magical': True})
        for character in source['manifest']['characters']:
            self.assertIn('rogue.amulet_of_yendor', character['known_identities'])
            self.assertNotIn('rogue.amulet_of_yendor',
                             [row['archetype'] for row in character['loadout']])
        for profile in source['manifest']['floor_activation']['profiles'].values():
            for stage in profile['stages']:
                for band in stage['bands']:
                    self.assertNotIn('rogue.amulet_of_yendor',
                                     [row['archetype'] for row in band['choices']])
        for pool in source['manifest']['floor_activation']['carried_item_pools'].values():
            self.assertNotIn('rogue.amulet_of_yendor', [row['archetype'] for row in pool['choices']])

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
                    self.assertNotIn("unique_retrieval", manifest)

    def test_encounters_preserve_structural_asset_forecasts_without_sharing_source_lists(self):
        source = encounter_sources()
        original = copy.deepcopy(source)
        with mock.patch("rogue_roster_package.encounter_sources", return_value=source):
            for species in SPECIES:
                for character in (1, 2):
                    for autonomous in (False, True):
                        with self.subTest(species=species, character=character, autonomous=autonomous):
                            manifest, region = encounter(species, character=character,
                                                         autonomous=autonomous)
                            self.assertEqual(manifest.get("themes"), source["manifest"]["themes"])
                            self.assertEqual(manifest.get("assets"), source["manifest"]["assets"])
                            forecast = {asset for theme in manifest["themes"]
                                        for asset in manifest["assets"][theme]}
                            self.assertIn(source["actors"][region["actors"][0]["archetype"]]["asset"],
                                          forecast)
                            manifest["assets"][manifest["themes"][0]].clear()
                            manifest["themes"].clear()
                            self.assertEqual(source, original)

    def test_helpers_do_not_reuse_mutable_source_or_enable_unrequested_ai(self):
        first, _ = encounter("ice_monster")
        first["characters"][0]["creature"]["hit_dice"].clear()
        second, region = encounter("ice_monster", autonomous=True)
        self.assertEqual(len(second["characters"][0]["creature"]["hit_dice"]), 1)
        self.assertEqual(region["actors"][0]["ai"], "rogue_hostile")
        for species, character in (("dragon", 1), ("emu", True), ("emu", 3)):
            with self.assertRaises(ValueError):
                encounter(species, character=character)

    def test_flight_witness_height_changes_only_bounded_geometry(self):
        normal_manifest, normal_region = encounter("griffin")
        manifest, region = encounter("griffin", height=5)
        self.assertEqual(manifest, normal_manifest)
        normal_region["size"][2] = 5
        self.assertEqual(region, normal_region)
        for height in (True, 0, 1, 9, 3.5):
            with self.assertRaises(ValueError):
                encounter("griffin", height=height)

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
