"""Pacing model against hand-computed growth terms and the committed values."""
import unittest

import rogue_pacing_model as pacing


class PacingModel(unittest.TestCase):
    def setUp(self):
        self.growth = pacing.load_growth()
        _, self.actors, self.species, self.stages = pacing.load_population()

    def test_growth_values_come_from_the_package_overrides(self):
        self.assertEqual((self.growth.threshold_base, self.growth.threshold_per_tier), (85, 0))
        self.assertEqual(self.growth.reward(3, 2), self.growth.reward_base
                         + self.growth.reward_per_relative_tier)
        self.assertEqual(self.growth.reward(1, 40), self.growth.minimum_reward)
        self.assertEqual(self.growth.threshold(7), 85)

    def test_hobgoblin_threat_tier_by_hand(self):
        # Racial + Warrior HD (2 x 70), two starting ranks (2 x 5), species,
        # type, class and Rattling Blow (4 x 10): 190 points, tier 2.
        hobgoblin = self.actors["rogue.hobgoblin"]
        self.assertEqual(pacing.threat_tier(hobgoblin, self.species["rogue.hobgoblin"],
                                            self.growth), 2)

    def test_kills_per_floor_follow_the_package_and_wandering_rate(self):
        initial, wanderers, treasure = pacing.kills_per_floor(self.stages, 940)
        self.assertEqual(initial, 4)
        self.assertAlmostEqual(wanderers, 10.0)
        self.assertAlmostEqual(treasure, 7.5 / 20)

    def test_committed_values_stay_within_one_level_of_the_rogue_model(self):
        levels = pacing.levels_by_depth(self.growth)
        reference = pacing.reference_levels()
        self.assertEqual(levels[1], 1)
        self.assertEqual(list(levels.values()), sorted(levels.values()))
        for depth, expected in reference.items():
            self.assertLessEqual(abs(levels[depth] - expected), 1, depth)

    def test_steeper_thresholds_fall_behind_deep(self):
        self.growth.threshold_base, self.growth.threshold_per_tier = 60, 40
        levels = pacing.levels_by_depth(self.growth)
        self.assertLess(levels[26], pacing.reference_levels()[26] - 2)


if __name__ == "__main__":
    unittest.main()
