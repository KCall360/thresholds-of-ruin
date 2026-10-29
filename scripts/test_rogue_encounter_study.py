"""Encounter study packages, report pooling and one real arena encounter."""
import os
from pathlib import Path
import tempfile
import unittest

import rogue_encounter_study as study


def participant(actor, checks, hits, damage, maximum):
    return {"actor": str(actor), "attack_checks": str(checks), "hits": str(hits),
            "damage_dealt": str(damage), "initial": {"maximum_health": str(maximum)}}


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

    def test_compare_flags_ratios_outside_tolerance(self):
        target = study.reference.targets()[0]
        measured = {metric: None for metric in study.METRICS}
        measured["player_hit"] = target.player_hit * 2
        rows = study.compare(target, measured, 0.35)
        self.assertFalse(rows["player_hit"]["within"])
        self.assertIsNone(rows["foe_hit"]["within"])


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


if __name__ == "__main__":
    unittest.main()
