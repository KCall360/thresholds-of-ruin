"""Rogue reference model against hand-computed values from the research notes."""
from fractions import Fraction
import json
import io
import unittest
from contextlib import redirect_stdout

import rogue_reference_model as model


class ReferenceModel(unittest.TestCase):
    def test_experience_levels_follow_the_threshold_table(self):
        self.assertEqual(model.level_for_xp(0), 1)
        self.assertEqual(model.level_for_xp(9), 1)
        self.assertEqual(model.level_for_xp(10), 2)
        self.assertEqual(model.level_for_xp(1299), 8)
        self.assertEqual(model.level_for_xp(1300), 9)
        self.assertEqual(model.level_for_xp(8000000), 21)

    def test_depth_one_draws_the_first_five_species_equally(self):
        # Rolls 0..4 give negative indices (rnd(5)); rolls 5..9 give K E B S H.
        self.assertEqual(model.species_distribution(1),
                         {glyph: Fraction(1, 5) for glyph in "KEBSH"})

    def test_deep_index_redistributes_onto_the_last_five(self):
        # Depth 31: index 25 is D; indices 26..34 (nine rolls) spread over M V G J D.
        deep = model.species_distribution(31)
        self.assertEqual(deep["D"], Fraction(1, 10) + Fraction(9, 50))
        for glyph in "MVGJ":
            self.assertEqual(deep[glyph], Fraction(9, 50))
        self.assertEqual(deep["D"] / deep["M"], Fraction(14, 9))
        self.assertEqual(sum(deep.values()), 1)

    def test_wandering_rerolls_excluded_species(self):
        wander = model.species_distribution(6, wandering=True)
        self.assertFalse(set(wander) & model.WANDERING_EXCLUDED)
        self.assertEqual(sum(wander.values()), 1)
        # Depth 6 indices 0..9 are K E B S H I R O Z L; I and L are excluded.
        self.assertEqual(wander, {glyph: Fraction(1, 8) for glyph in "KEBSHROZ"})

    def test_kill_xp_uses_level_one_hp_adjustment(self):
        # Emu: level 1, 1d8 HP, adjustment floor(hp / 8) is 1 only on an 8.
        self.assertEqual(model.monster_stats("E", 1)["xp"], 2 + Fraction(1, 8))
        # Zombie: level 2, 2d8 HP, adjustment floor(hp / 6).
        expected = sum(chance * (hp // 6) for hp, chance in model.dice_distribution(2, 8).items())
        self.assertEqual(model.monster_stats("Z", 1)["xp"], 6 + expected)

    def test_deep_monsters_gain_level_lower_armor_and_xp(self):
        shallow, deep = model.monster_stats("V", 26), model.monster_stats("V", 28)
        self.assertEqual((deep["level"], deep["armor"]), (shallow["level"] + 2, shallow["armor"] - 2))
        self.assertGreater(deep["xp"], shallow["xp"] + 20)

    def test_initial_monsters_per_floor(self):
        # 7.5 present rooms on average, 0.5 * 80% + 0.5 * 25% = 52.5% each.
        self.assertEqual(model.initial_monsters_per_floor(), Fraction(15, 2) * Fraction(105, 200))

    def test_wandering_rate(self):
        # 70-turn startup then 1/6 every 4 turns: one wanderer per 94 turns.
        self.assertEqual(model.wanderers_per_turn(), Fraction(1, 94))

    def test_food_with_drought_guarantee_exceeds_ordinary_rate(self):
        ordinary = 9 * Fraction(36, 100) * Fraction(16, 100)
        mean, drought = model.food_per_floor()
        self.assertGreater(mean, ordinary)
        self.assertLess(mean, ordinary + Fraction(1, 5))
        self.assertGreater(drought, 0)
        self.assertLess(drought, Fraction(1, 5))

    def test_drought_forces_food_on_the_fourth_foodless_level(self):
        # Rogue increments no_food before placing a new level's items and forces
        # food when it exceeds 3: three earlier foodless levels suffice.
        attempts, chance = model.ITEM_ATTEMPTS, model.ITEM_CHANCE
        miss = (1 - chance * model.food_share()) ** attempts
        nothing = (1 - chance) ** attempts
        weights = [Fraction(1), miss, miss ** 2, miss ** 3 / (1 - nothing)]
        _, drought = model.food_per_floor()
        self.assertEqual(drought, weights[-1] / sum(weights))
        self.assertEqual(model.DROUGHT_EARLIER_FLOORS, 3)

    def test_traps_start_at_depth_four(self):
        self.assertEqual(model.traps_per_floor(3), 0)
        # Depth 4: 40% chance, rnd(1) + 1 = one trap.
        self.assertEqual(model.traps_per_floor(4), Fraction(2, 5))
        # Depth 12: always considered, rnd(3) + 1 averages two.
        self.assertEqual(model.traps_per_floor(12), 2)

    def test_doctor_rate(self):
        self.assertEqual(model.healing_per_turn(1), Fraction(1, 19))
        self.assertEqual(model.healing_per_turn(7), Fraction(1, 7))
        # Level 10: every third turn heals rnd(3) + 1, two on average.
        self.assertEqual(model.healing_per_turn(10), Fraction(2, 3))

    def test_expectations_are_monotonic_in_level_and_report_json(self):
        rows = model.expectations({1, 5, 10, 26}, turns_per_floor=900)
        levels = [row.level_on_arrival for row in rows]
        self.assertEqual(levels, sorted(levels))
        self.assertEqual(rows[0].level_on_arrival, 1)
        self.assertGreater(rows[-1].level_on_arrival, 8)
        output = io.StringIO()
        with redirect_stdout(output):
            model.main(["--depths", "1-3", "--turns-per-floor", "600"])
        report = json.loads(output.getvalue())
        self.assertEqual(report["model"], model.MODEL)
        self.assertEqual([row["depth"] for row in report["depths"]], [1, 2, 3])


if __name__ == "__main__":
    unittest.main()
