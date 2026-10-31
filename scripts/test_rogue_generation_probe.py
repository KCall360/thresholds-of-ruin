"""Pooling of first-floor generation against Rogue's food expectation."""
import math
import unittest

import rogue_generation_probe as probe


class Pooling(unittest.TestCase):
    def test_pooled_food_rate_and_z_score(self):
        rows = [{"floors": 1, "items_per_floor": 4, "food_per_floor": 1},
                {"floors": 1, "items_per_floor": 2, "food_per_floor": 0},
                {"floors": 2, "items_per_floor": 3, "food_per_floor": 0.5}]
        pooled = probe.pooled(rows)
        self.assertEqual((pooled["floors"], pooled["food"]), (4, 2))
        self.assertAlmostEqual(pooled["food_per_floor"], 0.5)
        self.assertAlmostEqual(pooled["items_per_floor"], 3)
        expected = probe.EXPECTED_FOOD * 4
        self.assertAlmostEqual(pooled["food_z"], (2 - expected) / math.sqrt(expected))
        self.assertAlmostEqual(probe.EXPECTED_FOOD, 9 * 0.36 * 0.16)

    def test_seed_ranges(self):
        self.assertEqual(list(probe.seeds("3-5")), [3, 4, 5])
        self.assertEqual(list(probe.seeds("7")), [7])


if __name__ == "__main__":
    unittest.main()
