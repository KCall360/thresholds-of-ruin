"""Encounter targets against hand-computed Rogue swing() and save() examples."""
from fractions import Fraction
import io
import json
import unittest
from contextlib import redirect_stdout

import rogue_reference_targets as targets


class ReferenceTargets(unittest.TestCase):
    def test_swing_uses_a_zero_based_d20(self):
        # Level 3 adventurer, +1 mace, against an AC 7 kestrel: 11 of 20 rolls hit.
        self.assertEqual(targets.hit_chance(3, 7, 1), Fraction(11, 20))
        # A level 1 kestrel against AC 6 ring mail: 7 of 20.
        self.assertEqual(targets.hit_chance(1, 6), Fraction(7, 20))
        self.assertEqual(targets.hit_chance(1, -10), 0)
        self.assertEqual(targets.hit_chance(20, 6), 1)

    def test_saving_throw(self):
        # Level 1 poison save needs 14 on 1d20: 7 of 20.
        self.assertEqual(targets.save_chance(1, targets.SAVE_POISON), Fraction(7, 20))
        # Level 10 magic save needs 14 + 3 - 5 = 12: 9 of 20.
        self.assertEqual(targets.save_chance(10, targets.SAVE_MAGIC), Fraction(9, 20))

    def test_every_species_has_a_target_at_its_typical_depth(self):
        rows = targets.targets()
        self.assertEqual("".join(row.glyph for row in rows), targets.model.POPULATION_ORDER)
        kestrel = rows[0]
        self.assertEqual((kestrel.depth, kestrel.player_level), (2, 3))
        self.assertAlmostEqual(kestrel.player_hit, 0.55)
        self.assertEqual(kestrel.foe_hits, [0.35])
        # 2d4 + 2 averages 7; at 55% that is 3.85 damage per action.
        self.assertAlmostEqual(kestrel.player_damage_per_action, 3.85)
        # 2d4 + 2 deals at least 4, so most hits kill a 1d8 kestrel outright:
        # between one and two hits, at 55% each.
        self.assertGreater(kestrel.player_actions_to_kill, 1 / 0.55)
        self.assertLess(kestrel.player_actions_to_kill, 2 / 0.55)

    def test_actions_to_kill_counts_granular_attacks(self):
        half = {0: Fraction(1, 2), 1: Fraction(1, 2)}
        # One Health and a 50% one-damage attack: geometric, two attacks.
        self.assertAlmostEqual(targets.expected_actions_to_kill({1: Fraction(1)}, half), 2, places=4)
        # Two Health needs two successes: four attacks on average.
        self.assertAlmostEqual(targets.expected_actions_to_kill({2: Fraction(1)}, half), 4, places=4)
        self.assertIsNone(targets.expected_actions_to_kill({2: Fraction(1)}, {0: Fraction(1)}))
        attack = targets.attack_distribution([Fraction(1, 2)], ["1d2"], 1)
        self.assertEqual(attack, {0: Fraction(1, 2), 2: Fraction(1, 4), 3: Fraction(1, 4)})

    def test_signature_effects_combine_any_hit_with_their_rider(self):
        rows = {row.glyph: row for row in targets.targets()}
        self.assertEqual(rows["V"].signature, "maximum-Health drain")
        self.assertAlmostEqual(rows["V"].signature_per_action, rows["V"].foe_hits[0] * 0.3)
        aquator = rows["A"]
        either = 1 - (1 - aquator.foe_hits[0]) * (1 - aquator.foe_hits[1])
        self.assertAlmostEqual(aquator.signature_per_action, either)
        self.assertIsNone(aquator.foe_actions_to_kill, "zero-damage contact never kills")
        self.assertEqual(rows["E"].signature_per_action, 0)

    def test_report_is_strict_json(self):
        output = io.StringIO()
        with redirect_stdout(output):
            targets.main([])
        report = json.loads(output.getvalue())
        self.assertEqual(report["targets"], targets.TARGETS)
        self.assertEqual(len(report["species"]), 26)


if __name__ == "__main__":
    unittest.main()
