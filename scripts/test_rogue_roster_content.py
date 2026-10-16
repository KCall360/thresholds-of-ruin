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
            "black_unicorn": (7, [(1, 9), (1, 9), (2, 9)]),
            "vampire": (8, [(1, 10)])}


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
        mean = {"emu", "snake", "hobgoblin", "zombie", "quagga", "black_unicorn", "vampire"}
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

    def test_centaur_yeti_keep_exact_owned_hd_purchases_and_cold_immunity(self):
        actors = source("catalog/actors.toml")["actors"]
        species = source("catalog/creatures.toml")["creatures"]["species"]
        for name, owner, unlocks in (("centaur", "warrior", {2: "second_strike", 4: "third_strike"}),
                                     ("yeti", "racial", {2: "second_strike"})):
            with self.subTest(species=name):
                build = actors["rogue." + name]["creature"]
                self.assertEqual(build["restoration"], "authored")
                self.assertEqual(build["hit_dice"], [{"source": owner, "purchases": [
                    {"pool": "advancement", "selection": {"kind": "talent", "talent": unlocks[ordinal]}}
                ] if ordinal in unlocks else []} for ordinal in range(1, 5)])
        self.assertEqual(species["rogue.yeti"]["subtypes"], ["cold"])
        rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text())
        cold = next(row for row in rules["subtypes"] if row["id"] == "cold")
        self.assertFalse(cold["mechanics_deferred"])
        self.assertEqual(cold["modifiers"], [{"scope": "always", "op": {
            "kind": "immunity", "selector": {"kind": "descriptor", "descriptor": "cold"}}}])

    def test_generated_carrier_fixture_preserves_production_policies_and_recipes(self):
        from rogue_vampire_package import generated_carrier_manifest
        manifest = source("scenario.toml")
        canonical = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]
        for name, depth in (("centaur", 7), ("yeti", 11), ("vampire", 27)):
            with self.subTest(species=name):
                fixture = generated_carrier_manifest(name)
                self.assertEqual(fixture["modules"], manifest["modules"])
                self.assertEqual(fixture["ai_profiles"], manifest["ai_profiles"])
                self.assertEqual(fixture["floor_activation"]["carried_item_pools"],
                                 manifest["floor_activation"]["carried_item_pools"])
                stage = fixture["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]
                self.assertEqual(stage["id"], canonical["id"])
                self.assertEqual(stage["ai"], canonical["ai"])
                self.assertEqual(stage["depth_growth"], canonical["depth_growth"])
                self.assertEqual(stage["count"], [1, 1])
                choice = stage["bands"][0]["choices"][0]
                expected = next(row for band in canonical["bands"] for row in band["choices"]
                                if row["archetype"] == "rogue." + name)
                self.assertEqual(choice["carried_loot"], expected["carried_loot"])
                self.assertEqual(fixture["characters"][0]["anchor"], f"dungeon/{depth}/up")
                self.assertEqual(len(fixture["characters"][0]["creature"]["hit_dice"]), 16)
                self.assertEqual(fixture["characters"][0]["loadout"], manifest["characters"][0]["loadout"])

    def test_production_population_selects_ten_species_and_preserves_stock_order(self):
        manifest = source("scenario.toml")
        activation = manifest["floor_activation"]
        profile = activation["profiles"]["rogue.low_depth"]
        self.assertEqual(profile["version"], 1)
        stages = profile["stages"]
        self.assertEqual([row["id"] for row in stages],
                         ["rogue.eight_species_foes", "rogue.bandages", "rogue.rations"])
        self.assertEqual([row["count"] for row in stages], [[4, 4], [1, 1], [1, 1]])
        actor = stages[0]
        self.assertEqual(actor["kind"], "actors")
        self.assertEqual(actor["version"], 1)
        self.assertEqual(actor["ai"], "rogue_hostile")
        self.assertTrue(actor["avoid_protagonist_sight"])
        self.assertEqual(actor["depth_growth"], {"kind": "racial_after", "depth": 26})
        self.assertEqual(len(actor["bands"]), 21)
        self.assertEqual({row["archetype"] for band in actor["bands"] for row in band["choices"]},
                         {"rogue." + name for name in (
                             "emu", "snake", "hobgoblin", "ice_monster", "zombie", "centaur", "quagga", "yeti", "black_unicorn", "vampire")})
        chances = {"rogue.centaur": (3, 20), "rogue.yeti": (3, 10), "rogue.vampire": (1, 5)}
        for band in actor["bands"]:
            for row in band["choices"]:
                if row["archetype"] in chances:
                    numerator, denominator = chances[row["archetype"]]
                    self.assertEqual(row["carried_loot"], {"version": 1,
                        "pool": "rogue.vampire_supplies",
                        "chance": {"numerator": numerator, "denominator": denominator}})
                else:
                    self.assertNotIn("carried_loot", row)
        self.assertEqual(activation["carried_item_pools"], {"rogue.vampire_supplies": {
            "version": 1, "choices": [
                {"archetype": "rogue.ration", "weight": 1, "quantity": 1},
                {"archetype": "rogue.clean_bandage", "weight": 1, "quantity": 1}]}})
        for stage, identity, quantity in zip(stages[1:], ("clean_bandage", "ration"), (2, 1), strict=True):
            self.assertEqual(stage["kind"], "items")
            self.assertEqual(stage["bands"], [{"depth": {"kind": "open", "minimum": 1},
                              "choices": [{"archetype": "rogue." + identity, "weight": 1}]}])
            self.assertEqual(stage["quantity"], quantity)
        self.assertEqual(activation["protagonist"], "selected_character")
        self.assertEqual(manifest["procedural_worlds"]["dungeon"]["domain"],
                         {"kind": "open", "max_depth": 1_000_000})
        self.assertNotIn("objective", manifest)


if __name__ == "__main__":
    unittest.main()
