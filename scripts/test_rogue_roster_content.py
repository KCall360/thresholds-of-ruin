"""New reconstruction: source contracts for retained ordinary Rogue recipes.

Catalog presence is not population activation or measured play/balance evidence.
"""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue-exploration"
ORDINARY = {"emu": (1, [(1, 2)]), "snake": (1, [(1, 3)]),
            "hobgoblin": (1, [(1, 8)]), "zombie": (2, [(1, 8)]),
            "quagga": (3, [(1, 5), (1, 5)]),
            "centaur": (4, [(1, 2), (1, 5), (1, 5)]),
            "yeti": (4, [(1, 6), (1, 6)]),
            "black_unicorn": (7, [(1, 9), (1, 9), (2, 9)])}


def source(name):
    return tomllib.loads((PACKAGE / name).read_text(encoding="utf-8"))


class RogueRosterContent(unittest.TestCase):
    def test_retained_ordinary_recipes_own_real_rolled_strikes_and_hd_records(self):
        actors = source("catalog/actors.toml")["actors"]
        species = source("catalog/creatures.toml")["creatures"]["species"]
        self.assertTrue({"rogue." + name for name in ORDINARY}.issubset(actors),
                        "Retained ordinary roster recipes are missing")
        for name, (hd, dice) in ORDINARY.items():
            with self.subTest(species=name):
                identity = "rogue." + name
                build = actors[identity]["creature"]
                self.assertEqual(build["species"], identity)
                self.assertEqual(len(build["hit_dice"]), hd)
                self.assertEqual(build["nutrition"], {"kind": "exempt"})
                self.assertEqual(build["future_growth"], {"kind": "disabled"})
                strikes = species[identity]["melee"]["strikes"]
                self.assertEqual(len(strikes), len(dice))
                for index, (strike, (count, sides)) in enumerate(zip(strikes, dice, strict=True)):
                    amount = strike["on_success"][0]["damage"]["components"][0]["amount"]
                    self.assertEqual(amount, {"kind": "rolled", "dice": {
                        "count": count, "sides": sides, "bonus": 0}})
                    self.assertIn("check", strike)
                    if index:
                        self.assertEqual(strike["unlock"], ("second_strike", "third_strike")[index - 1])

    def test_every_rogue_species_owns_explicit_reference_disposition(self):
        species = source("catalog/creatures.toml")["creatures"]["species"]
        mean = {"emu", "snake", "hobgoblin", "zombie", "quagga", "black_unicorn"}
        passive = {"human", "ice_monster", "centaur", "yeti"}
        self.assertEqual(set(species), {"rogue." + name for name in mean | passive})
        for name in mean | passive:
            with self.subTest(species=name):
                self.assertEqual(species["rogue." + name].get("behavior"), {
                    "disposition": "mean" if name in mean else "passive",
                    "idle": "wait", "memory_ticks": 1000,
                })

    def test_natural_unlocks_are_explicit_eligible_purchases_not_free_attacks(self):
        rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text())
        talents = {row["id"]: row for row in rules["talents"]}
        self.assertIn("second_strike", talents, "Retained NA1 purchase is missing")
        self.assertIn("third_strike", talents, "Retained NA2 purchase is missing")
        for identity, hd in (("second_strike", 2), ("third_strike", 4)):
            self.assertEqual(talents[identity]["modifiers"], [])
            predicates = talents[identity]["requirement"]["requirements"]
            self.assertIn({"kind": "total_hd", "minimum": hd}, predicates)
            self.assertIn({"kind": "species_flag", "flag": "natural_sequence", "present": True}, predicates)
        self.assertEqual(talents["third_strike"]["prerequisites"], ["second_strike"])

    def test_production_population_retains_only_existing_four_foes_and_stock_order(self):
        profile = source("scenario.toml")["floor_activation"]["profiles"]["rogue.low_depth"]
        stages = profile["stages"]
        self.assertEqual([row["id"] for row in stages],
                         ["rogue.emus", "rogue.snakes", "rogue.bandages", "rogue.rations"])
        self.assertEqual([row["count"] for row in stages], [[2, 2], [2, 2], [1, 1], [1, 1]])
        for stage, identity in zip(stages, ("emu", "snake", "clean_bandage", "ration"), strict=True):
            self.assertIn("bands", stage, "Production source must use the strict weighted-band schema")
            self.assertEqual(stage["bands"], [{"depth": {"kind": "open", "minimum": 1},
                              "choices": [{"archetype": "rogue." + identity, "weight": 1}]}])
        self.assertEqual([row["quantity"] for row in stages[2:]], [2, 1])
        manifest = source("scenario.toml")
        self.assertEqual(manifest["floor_activation"]["protagonist"], "selected_character")
        self.assertEqual(manifest["procedural_worlds"]["dungeon"]["domain"],
                         {"kind": "open", "max_depth": 1_000_000})
        self.assertNotIn("objective", manifest)


if __name__ == "__main__":
    unittest.main()
