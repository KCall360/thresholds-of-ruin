"""Current wide wire scalars are exact and never accept numeric JSON tokens."""
import unittest

from scalar_wire import u64


class ScalarWireTests(unittest.TestCase):
    def test_zero_and_wide_boundaries_are_exact(self):
        for value in (0, 1, 65_535, 65_536, 65_537, 2**53 + 1, 2**64 - 1):
            with self.subTest(value=value):
                self.assertEqual(u64(str(value)), value)
                self.assertIs(type(u64(str(value))), int)

    def test_old_numeric_noncanonical_and_overflow_values_are_rejected(self):
        for value in (0, 1, True, False, 1.0, None, "", "00", "01", "-1", "+1", "1.0",
                      " 1", "1 ", "1\n", "１", "١", str(2**64), "1" * 10000):
            with self.subTest(value=str(value)[:25]), self.assertRaises(ValueError):
                u64(value)
