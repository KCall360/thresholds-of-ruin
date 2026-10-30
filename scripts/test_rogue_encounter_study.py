"""Encounter study packages, report pooling and one real arena encounter."""
import os
from pathlib import Path
import tempfile
import tomllib
import unittest

import rogue_encounter_study as study


def participant(actor, checks, hits, damage, maximum, effects=None):
    return {"actor": str(actor), "attack_checks": str(checks), "hits": str(hits),
            "damage_dealt": str(damage), "initial": {"maximum_health": str(maximum)},
            "effects": {name: {"applications": str(count)} for name, count in (effects or {}).items()}}


class EncounterPackages(unittest.TestCase):
    def setUp(self):
        self.manifest, self.actors = study.load_source()

    def test_reference_level_grows_scheduled_rows_and_keeps_the_rest(self):
        schedule = self.manifest["characters"][0]["creature"]["growth_schedule"]
        build = study.leveled_character(self.manifest, 4)["creature"]
        self.assertEqual(build["hit_dice"][2:], schedule[:3])
        self.assertEqual(build["growth_schedule"], schedule[3:])
        self.assertEqual(build["future_growth"], {"kind": "authored"})
        exhausted = study.leveled_character(self.manifest, 30)["creature"]
        self.assertNotIn("growth_schedule", exhausted)
        self.assertEqual(exhausted["future_growth"], {"kind": "disabled"})
        self.assertEqual(len(exhausted["hit_dice"]), 2 + len(schedule))

    def test_every_species_has_an_actor_archetype(self):
        for glyph in study.reference.model.POPULATION_ORDER:
            package, region = study.encounter(self.manifest, self.actors, glyph, 3)
            self.assertEqual(region["actors"][0]["archetype"], study.archetype(glyph))
            self.assertEqual(package["arena"]["participants"][1], study.ACTOR_ID)

    def test_measure_pools_checks_and_scales_foe_actions_by_strikes(self):
        reports = [{"participants": [participant(1, 4, 2, 10, 20), participant(3, 6, 3, 6, 8)]},
                   {"participants": [participant(1, 2, 2, 8, 20), participant(3, 0, 0, 0, 8)]}]
        measured = study.measure(reports, 1, 3, foe_strikes=2)
        self.assertEqual(measured["player_hit"], 4 / 6)
        self.assertEqual(measured["foe_hit"], 3 / 6)
        self.assertEqual(measured["player_damage_per_action"], 18 / 6)
        self.assertEqual(measured["foe_damage_per_action"], 6 * 2 / 6)
        self.assertEqual(measured["player_actions_to_kill"], 8 / 3)
        self.assertEqual(measured["foe_actions_to_kill"], 20 / 2)

    def test_signature_effects_are_pooled_per_foe_action(self):
        reports = [{"participants": [participant(1, 4, 2, 10, 20),
                                     participant(3, 6, 3, 6, 8, {"corrode_armor": 2, "damage": 3})]},
                   {"participants": [participant(1, 2, 2, 8, 20),
                                     participant(3, 2, 1, 1, 8, {"corrode_armor": 1})]}]
        measured = study.measure(reports, 1, 3, foe_strikes=2, signature=("corrode_armor",))
        # Three applications over eight strike checks, which are four actions.
        self.assertEqual(measured["signature_per_action"], 3 / 4)
        self.assertEqual(measured["counts"]["foe_signature"], 3)
        unsigned = study.measure(reports, 1, 3, foe_strikes=2)
        self.assertIsNone(unsigned["signature_per_action"])

    def test_signature_effect_names_follow_the_package(self):
        self.assertEqual(study.SIGNATURE_EFFECTS["I"], ("apply_condition:rogue.frozen",))
        self.assertEqual(study.SIGNATURE_EFFECTS["M"], ("apply_condition:rogue.confused",))
        # Rogue's dragon breathes as its own ranged action, not as a hit rider,
        # so a per-hit target cannot judge it.
        self.assertNotIn("D", study.SIGNATURE_EFFECTS)
        target = next(row for row in study.reference.targets() if row.glyph == "A")
        measured = {metric: None for metric in study.METRICS}
        measured["signature_per_action"] = target.signature_per_action * 3
        rows = study.compare(target, measured, 0.35)
        self.assertFalse(rows["signature_per_action"]["within"])

    def test_thieves_have_something_to_steal(self):
        manifest, actors = study.load_source()
        gold, _ = study.encounter(manifest, actors, "L", 5)
        self.assertIn({"archetype": "rogue.gold", "quantity": study.STUDY_GOLD},
                      gold["characters"][0]["loadout"])
        charm, _ = study.encounter(manifest, actors, "N", 5)
        self.assertIn({"archetype": study.STUDY_CHARM, "quantity": 1},
                      charm["characters"][0]["loadout"])
        plain, _ = study.encounter(manifest, actors, "K", 5)
        self.assertNotIn({"archetype": "rogue.gold", "quantity": study.STUDY_GOLD},
                         plain["characters"][0]["loadout"])

    def test_compare_flags_ratios_outside_tolerance(self):
        target = study.reference.targets()[0]
        measured = {metric: None for metric in study.METRICS}
        measured["player_hit"] = target.player_hit * 2
        rows = study.compare(target, measured, 0.35)
        self.assertFalse(rows["player_hit"]["within"])
        self.assertIsNone(rows["foe_hit"]["within"])


class StudyArrangements(unittest.TestCase):
    def test_make_mean_changes_only_the_named_species(self):
        text = ('[creatures.species."rogue.orc"]\nbehavior = { disposition = "passive" }\n'
                '[creatures.species."rogue.human"]\nbehavior = { disposition = "passive" }\n')
        changed = study.make_mean(text, "rogue.orc")
        self.assertIn('rogue.orc"]\nbehavior = { disposition = "mean" }', changed)
        self.assertIn('rogue.human"]\nbehavior = { disposition = "passive" }', changed)

    def test_the_greedy_orc_is_measured_with_its_own_acquisition(self):
        # Only the adventurer is made to engage; the orc keeps the package's
        # greedy disposition, which acquires when no Gold is in the arena.
        self.assertNotIn("O", study.FOE_ENGAGES)
        species = tomllib.loads((study.PACKAGE / "catalog/creatures.toml")
                                .read_text(encoding="utf-8"))["creatures"]["species"]
        self.assertEqual(species["rogue.orc"]["behavior"]["disposition"], "greedy")

    def test_phantom_study_wears_see_invisible(self):
        manifest, actors = study.load_source()
        package, _ = study.encounter(manifest, actors, "P", 11)
        [character] = package["characters"]
        self.assertIn({"archetype": study.SEE_INVISIBLE, "quantity": 1,
                       "equipped_slot": study.RING_SLOT}, character["loadout"])
        self.assertIn(study.SEE_INVISIBLE, character["known_identities"])


class EncounterProcess(unittest.TestCase):
    def test_one_real_arena_encounter_is_measured(self):
        binaries = Path(os.environ.get("TOR_TEST_BINARIES", study.ROOT / "target/debug"))
        suffix = ".exe" if (binaries / "tor-arena.exe").exists() else ""
        with tempfile.TemporaryDirectory(dir=study.ROOT / "target") as output:
            report = study.study(Path(output), binaries / ("tor-scenario" + suffix),
                                 binaries / ("tor-arena" + suffix), [0], "K")
        [row] = report["species"]
        self.assertEqual(row["encounters"], 2)
        self.assertGreater(row["counts"]["player_checks"], 0)
        self.assertIsNotNone(row["comparison"]["player_hit"]["measured"])

    def test_flytrap_encounters_complete_with_measured_attacks(self):
        # A hold can make the adventurer's admitted attack fail; the arena counts
        # that as a lost turn instead of rejecting the whole encounter.
        binaries = Path(os.environ.get("TOR_TEST_BINARIES", study.ROOT / "target/debug"))
        suffix = ".exe" if (binaries / "tor-arena.exe").exists() else ""
        with tempfile.TemporaryDirectory(dir=study.ROOT / "target") as output:
            report = study.study(Path(output), binaries / ("tor-scenario" + suffix),
                                 binaries / ("tor-arena" + suffix), [0], "F")
        [row] = report["species"]
        self.assertEqual(row["encounters"], 2 * study.EXTRA_SEEDS["F"])
        self.assertFalse([outcome for outcome in row["outcomes"] if outcome.startswith("failure")])
        self.assertGreater(row["counts"]["foe_checks"], 0)

    def test_the_unedited_greedy_orc_attacks(self):
        # No Gold in the arena: the package's greedy orc acquires the adventurer
        # and attacks without any study edit to its behavior.
        binaries = Path(os.environ.get("TOR_TEST_BINARIES", study.ROOT / "target/debug"))
        suffix = ".exe" if (binaries / "tor-arena.exe").exists() else ""
        with tempfile.TemporaryDirectory(dir=study.ROOT / "target") as output:
            report = study.study(Path(output), binaries / ("tor-scenario" + suffix),
                                 binaries / ("tor-arena" + suffix), [0], "O")
        [row] = report["species"]
        self.assertEqual(row["encounters"], 2 * study.EXTRA_SEEDS["O"])
        self.assertFalse([outcome for outcome in row["outcomes"] if outcome.startswith("failure")])
        self.assertGreater(row["counts"]["foe_checks"], 0)


if __name__ == "__main__":
    unittest.main()
