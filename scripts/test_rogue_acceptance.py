"""The ratified tolerance checks on synthetic evidence."""
import unittest

import rogue_acceptance as acceptance


def run(depths, *, died=False, food=0.6, ticks=90000, empty=0, outcome=None, at_death=None):
    return {"outcome": outcome or ("death" if died else "censored_action_budget"),
            "died": died, "deepest_depth": max(depths),
            "level_on_arrival": {str(depth): level for depth, level in depths.items()},
            "ticks_per_floor": {str(depth): ticks for depth in depths},
            "item_generation": {"floors": len(depths), "food_per_floor": food},
            "samples_by_hunger": {"empty": empty},
            **({"hunger_at_death": at_death} if at_death else {})}


class Acceptance(unittest.TestCase):
    def test_level_bands_are_one_shallow_and_two_deep(self):
        reference = {5: 5, 15: 9}
        self.assertTrue(acceptance.check_levels({5: 6, 15: 11}, reference)["pass"])
        self.assertFalse(acceptance.check_levels({5: 7, 15: 9}, reference)["pass"])
        self.assertFalse(acceptance.check_levels({5: 5, 15: 12}, reference)["pass"])

    def test_depth_four_share_ignores_harness_errors(self):
        runs = [run({1: 1, 4: 5}), run({1: 1, 4: 5}), run({1: 1, 4: 5}), run({1: 1, 2: 3}),
                {"outcome": "harness_error"}]
        result = acceptance.check_depth(runs)
        self.assertEqual((result["completed"], result["reached_depth_4"]), (4, 3))
        self.assertTrue(result["pass"])
        self.assertFalse(acceptance.check_depth(runs[2:])["pass"])

    def test_starvation_share_counts_deaths_with_an_empty_reserve(self):
        runs = [run({1: 1}, died=True, empty=3)] + [run({1: 1}, died=True) for _ in range(9)]
        self.assertTrue(acceptance.check_starvation(runs)["pass"])
        self.assertFalse(acceptance.check_starvation(runs[:2])["pass"])

    def test_starvation_is_judged_by_the_hunger_at_death(self):
        # An early empty spell that ended in a meal does not make a later
        # combat death a starvation; dying while the reserve is empty does.
        fed = run({1: 1}, died=True, empty=12, at_death="nourished")
        starved = run({1: 1}, died=True, empty=0, at_death="empty")
        self.assertEqual(acceptance.check_starvation([fed])["starved"], 0)
        self.assertEqual(acceptance.check_starvation([starved])["starved"], 1)

    def test_food_is_judged_against_the_model_ratio(self):
        expected = acceptance.model_food_ratio()
        at_model = expected * 90000 / 100 / acceptance.MEAL
        self.assertTrue(acceptance.check_food([run({1: 1, 2: 2}, food=at_model)])["pass"])
        self.assertFalse(acceptance.check_food([run({1: 1}, food=at_model * 2)])["pass"])

    def test_food_model_ratio_uses_the_campaign_pace(self):
        # A slow bot spends more food per floor in Rogue too: the model's ratio
        # is taken at the measured turns per floor, so only generation counts.
        per_floor = acceptance.model_food_ratio() * 900 / acceptance.MEAL
        slow = acceptance.check_food([run({1: 1, 2: 2}, food=per_floor, ticks=150000)])
        self.assertTrue(slow["pass"], slow)
        self.assertAlmostEqual(slow["bands"]["1-5"]["turns_per_floor"], 1500)
        self.assertFalse(acceptance.check_food(
            [run({1: 1, 2: 2}, food=per_floor / 2, ticks=150000)])["pass"])

    def test_species_ignore_rare_attackers_and_flag_out_of_band_ratios(self):
        def row(glyph, checks, within):
            return {"glyph": glyph, "counts": {"player_checks": checks, "foe_checks": 0},
                    "comparison": {"player_hit": {"within": within}}}
        study = {"species": [row("K", 40, True), row("O", 5, False)]}
        self.assertTrue(acceptance.check_species(study)["pass"])
        study["species"].append(row("E", 40, False))
        self.assertEqual(acceptance.check_species(study)["failures"], {"E": ["player_hit"]})


if __name__ == "__main__":
    unittest.main()
