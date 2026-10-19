"""Full-26 source contracts, independent of generated receipts and balance evidence.

Selector reference: phs/rogue c7c119f893bd2f8255f2ad78d58a32664ddb9a97,
monsters.c lines 21-51. Future Dragon purchases are provisional TOR tuning.
"""
from fractions import Fraction
import math
from pathlib import Path
import tomllib
import unittest

from rogue_activation_oracle import reviewed_twenty_one_activation, reviewed_twenty_six_activation

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue-exploration"
SELECTOR = "KEBSHIROZLCQANYFTWPXUMVGJD"
ORDER = "KEBSHIRZCQYFTWUVGJOLNMPXAD"
NAMES = dict(zip(ORDER, (
    "kestrel", "emu", "bat", "snake", "hobgoblin", "ice_monster", "rattlesnake",
    "zombie", "centaur", "quagga", "yeti", "venus_flytrap", "troll", "wraith",
    "black_unicorn", "vampire", "griffin", "jabberwock", "orc", "leprechaun",
    "nymph", "medusa", "phantom", "xeroc", "aquator", "dragon"), strict=True))


def source(name):
    return tomllib.loads((PACKAGE / name).read_text(encoding="utf-8"))


def stage():
    return source("scenario.toml")["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]


def choices_at(bands, depth):
    matches = [band for band in bands if band["depth"]["minimum"] <= depth and
               (band["depth"]["kind"] == "open" or depth <= band["depth"]["maximum"])]
    if len(matches) != 1:
        raise AssertionError(f"Depth {depth} must select exactly one band: {len(matches)}")
    return matches[0]["choices"]


class RoguePopulationContent(unittest.TestCase):
    def test_full_policy_matches_separate_oracle_and_preserves_other_stage_streams(self):
        activation = source("scenario.toml")["floor_activation"]
        self.assertEqual(activation, reviewed_twenty_six_activation())
        stages = activation["profiles"]["rogue.low_depth"]["stages"]
        self.assertEqual([row["id"] for row in stages], ["rogue.eight_species_foes",
            "rogue.bandages", "rogue.rations", "rogue.restore_strength", "rogue.room_gold",
            "rogue.magic_potions", "rogue.magic_scrolls", "rogue.magic_devices"])
        self.assertEqual(stages[0]["count"], [4, 4])
        self.assertEqual(stages[0]["depth_growth"], {"kind": "racial_after", "depth": 26})
        bands = stages[0]["bands"]
        self.assertEqual([band["depth"] for band in bands], [
            *[{"kind": "finite", "minimum": depth, "maximum": depth} for depth in range(1, 32)],
            {"kind": "open", "minimum": 32}])

    def test_finite_magic_stock_appends_after_unchanged_legacy_item_streams(self):
        activation = source("scenario.toml")["floor_activation"]
        stages = activation["profiles"]["rogue.low_depth"]["stages"]
        historical = reviewed_twenty_one_activation()
        self.assertEqual(stages[1:5], historical["profiles"]["rogue.low_depth"]["stages"][1:])
        self.assertEqual(activation["carried_item_pools"], historical["carried_item_pools"])
        self.assertEqual(stages[5:], [
            {"id": identity, "version": 1, "kind": "items", "count": [1, 1], "quantity": 1,
             "bands": [{"depth": {"kind": "open", "minimum": 1},
                        "choices": [{"archetype": "rogue." + name, "weight": 1} for name in names]}]}
            for identity, names in (
                ("rogue.magic_potions", ("healing", "confusion", "blindness")),
                ("rogue.magic_scrolls", ("identify_potion", "identify_scroll", "identify_wand", "sleep")),
                ("rogue.magic_devices", ("wand_fire", "wand_cold", "wand_lightning")))])

    def test_exact_selector_mass_order_and_normalized_weights_at_every_boundary(self):
        bands = stage()["bands"]
        self.assertEqual({row["archetype"] for band in bands for row in band["choices"]},
                         {"rogue." + name for name in NAMES.values()})
        for depth in (*range(1, 41), 267, 268, 272, 273, 274, 275, 1_000_000):
            with self.subTest(depth=depth):
                # A direct draw owns 1/10; a redistributed draw owns five 1/50
                # outcomes. No omitted species or conditional renormalization.
                mass = {}
                for roll in range(10):
                    index = depth + roll - 6
                    indices = range(5) if index < 0 else range(21, 26) if index > 25 else [index]
                    for selected in indices:
                        name = "rogue." + NAMES[SELECTOR[selected]]
                        mass[name] = mass.get(name, Fraction()) + Fraction(1, 10 * len(indices))
                self.assertEqual(sum(mass.values()), 1)
                choices = choices_at(bands, depth)
                self.assertEqual([row["archetype"] for row in choices],
                                 ["rogue." + NAMES[glyph] for glyph in ORDER
                                  if "rogue." + NAMES[glyph] in mass])
                weights = {row["archetype"]: row["weight"] for row in choices}
                self.assertTrue(all(type(weight) is int and weight > 0 for weight in weights.values()))
                self.assertEqual(math.gcd(*weights.values()), 1)
                self.assertEqual({name: Fraction(weight, sum(weights.values()))
                                  for name, weight in weights.items()}, mass)

    def test_aquator_and_dragon_endpoints_and_depth31_are_explicit(self):
        bands = stage()["bands"]
        for depth in (1, 8, 9, 18, 19, 21, 22, 26, 27, 31, 32, 1_000_000):
            with self.subTest(depth=depth):
                choices = {row["archetype"]: row for row in choices_at(bands, depth)}
                self.assertEqual("rogue.aquator" in choices, 9 <= depth <= 18)
                self.assertEqual("rogue.dragon" in choices, depth >= 22)
                if "rogue.aquator" in choices:
                    self.assertEqual(choices["rogue.aquator"], {"archetype": "rogue.aquator", "weight": 1})
        for depth, expected in ((27, (1, 1, 1, 1, 1)), (28, (6, 11, 11, 11, 11)),
                                (29, (7, 7, 12, 12, 12)), (30, (8, 8, 8, 13, 13)),
                                (31, (9, 9, 9, 9, 14)), (32, (1, 1, 1, 1, 1))):
            self.assertEqual({row["archetype"]: row["weight"] for row in choices_at(bands, depth)},
                             dict(zip(("rogue.medusa", "rogue.vampire", "rogue.griffin",
                                       "rogue.jabberwock", "rogue.dragon"), expected, strict=True)))

    def test_dragon_birth_stock_is_one_existing_finite_item_on_every_choice(self):
        activation = source("scenario.toml")["floor_activation"]
        self.assertEqual(activation["carried_item_pools"]["rogue.vampire_supplies"], {
            "version": 1, "choices": [
                {"archetype": "rogue.ration", "weight": 1, "quantity": 1},
                {"archetype": "rogue.clean_bandage", "weight": 1, "quantity": 1}]})
        for band in stage()["bands"]:
            for choice in band["choices"]:
                if choice["archetype"] == "rogue.dragon":
                    self.assertEqual(choice["carried_loot"], {"version": 1,
                        "pool": "rogue.vampire_supplies", "chance": {"numerator": 1, "denominator": 1}})
        actors = source("catalog/actors.toml")["actors"]
        for name in ("aquator", "dragon"):
            self.assertNotIn("loadout", actors["rogue." + name])

    def test_dragon_breath_uses_bounded_budgeted_future_ordinal_purchases(self):
        build = source("catalog/actors.toml")["actors"]["rogue.dragon"]["creature"]
        definition = source("catalog/creatures.toml")["creatures"]["species"]["rogue.dragon"]
        self.assertEqual(len(build["hit_dice"]), 10)
        self.assertTrue(all(row["source"] == "racial" for row in build["hit_dice"]))
        self.assertEqual(definition["attributes"]["willpower"], 1)
        self.assertEqual([ordinal for ordinal, row in enumerate(build["hit_dice"], 1)
                          if row.get("attribute") == "willpower"], [4, 8])
        self.assertEqual(definition["attributes"]["willpower"] +
                         sum(row.get("attribute") == "willpower" for row in build["hit_dice"]), 3)
        self.assertIn("breath", build["starting_skills"])
        purchase = {"pool": "advancement", "selection": {"kind": "skill_rank", "skill": "breath"}}
        owned = [(ordinal, row["purchases"].count(purchase))
                 for ordinal, row in enumerate(build["hit_dice"], 1) if purchase in row["purchases"]]
        self.assertEqual(owned, [(8, 1)])
        self.assertEqual(build["future_growth"], {"kind": "disabled"})
        schedule = [{"ordinal": ordinal, "purchases": [purchase]} for ordinal in (15, 20, 25)]
        for band in stage()["bands"]:
            for choice in band["choices"]:
                if choice["archetype"] == "rogue.dragon":
                    self.assertEqual(choice["advancement"], schedule)
                else:
                    self.assertNotIn("advancement", choice)
        # These are expected source-owned row counts/ranks, not runtime evidence.
        # The next floor after D256 must refuse construction, never clamp/replace D.
        for depth, hd, rank in ((22, 10, 2), (26, 10, 2), (27, 11, 2), (30, 14, 2),
                                (31, 15, 3), (35, 19, 3), (36, 20, 4), (40, 24, 4),
                                (41, 25, 5), (272, 256, 5), (273, 257, 5)):
            self.assertEqual(len(build["hit_dice"]) + max(0, depth - 26), hd)
            self.assertEqual(2 + sum(row["ordinal"] <= hd for row in schedule), rank)
        self.assertEqual(2 + len(schedule), 5)
        rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text())
        self.assertEqual(rules["constants"]["racial_point_grants"], [{"pool": "advancement", "points": 1}])
        pool = next(row for row in rules["point_pools"] if row["id"] == "advancement")
        self.assertIn("skill_rank", pool["allowed"])


if __name__ == "__main__":
    unittest.main()
