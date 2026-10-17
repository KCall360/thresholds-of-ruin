"""Vampire content contracts; accepted mechanics do not imply calibrated play."""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1] / "scenarios" / "rogue-exploration"
VAMPIRE = "rogue.vampire"
DRAIN = "rogue.drain_vitality"
REGENERATION = "rogue.regeneration"


def source(relative):
    return tomllib.loads((ROOT / relative).read_text(encoding="utf-8"))


class RogueVampireContent(unittest.TestCase):
    def test_exact_eight_racial_rows_own_drain_and_regeneration(self):
        actors = source("catalog/actors.toml")["actors"]
        self.assertIn(VAMPIRE, actors)
        creature = actors[VAMPIRE]["creature"]
        rows = creature["hit_dice"]
        self.assertEqual(len(rows), 8)
        self.assertEqual([row["source"] for row in rows], ["racial"] * 8)
        for ordinal, row in enumerate(rows):
            expected = [] if ordinal > 1 else [{"pool": "advancement", "selection": {
                "kind": "talent", "talent": [DRAIN, REGENERATION][ordinal]}}]
            self.assertEqual(row["purchases"], expected)
        self.assertEqual(creature["restoration"], "authored")
        self.assertEqual(creature["nutrition"], {"kind": "exempt"})
        self.assertEqual(creature["future_growth"], {"kind": "disabled"})

    def test_species_only_supplies_eligibility_and_one_natural_damage_source(self):
        species = source("catalog/creatures.toml")["creatures"]["species"]
        self.assertIn(VAMPIRE, species)
        vampire = species[VAMPIRE]
        self.assertEqual(vampire["kind"], "undead")
        self.assertEqual(vampire["behavior"], {
            "disposition": "mean", "idle": "explore", "memory_ticks": 1000,
            "doors": True, "care": True, "equipment": True, "loot": True, "flee_percent": 25})
        ops = [modifier["op"] for modifier in vampire["modifiers"]]
        self.assertIn({"kind": "flag", "flag": "regenerative"}, ops)
        self.assertFalse(any(op["kind"] == "grant_ability" for op in ops))
        self.assertFalse(any(op.get("stat", {}).get("kind") ==
                             "health_recovery_period_percent" for op in ops))
        strike, = vampire["melee"]["strikes"]
        self.assertEqual(strike["check"]["skill"], {"kind": "natural_attack"})
        self.assertEqual(strike["riders"], [])
        self.assertEqual(strike["on_failure"], [])
        effect, = strike["on_success"]
        self.assertEqual(effect, {"kind": "damage", "damage": {"kind": "components",
            "components": [{"category": "keen", "amount": {"kind": "rolled",
                "dice": {"count": 1, "sides": 10, "bonus": 0}}}]}})

    def test_drain_is_purchased_natural_contact_then_spiritual_checked_loss(self):
        overrides = {tuple(row["path"]): row["value"] for row in source("rules.toml")["overrides"]}
        self.assertIn(("abilities", DRAIN), overrides)
        ability = overrides[("abilities", DRAIN)]
        self.assertEqual(ability["interaction"], "hostile_to_target")
        self.assertFalse(ability["basic_attack"])
        self.assertEqual(ability["strikes"], {"kind": "natural_melee", "additional_riders": [{
            "trigger": "success", "check": {"skill": {"kind": "natural_attack"},
                "defense": "spiritual"}, "effects": [{"kind": "lose_maximum_health",
                    "amount": {"kind": "rolled", "dice": {"count": 1, "sides": 3, "bonus": 0}}}],
            "descriptors": []}]})
        talent = overrides[("talents", DRAIN)]
        self.assertEqual(talent["requirement"], {"kind": "all", "requirements": [
            {"kind": "total_hd", "minimum": 1}, {"kind": "type_in", "types": ["undead"]}]})
        self.assertEqual(talent["modifiers"], [{"scope": "always", "op": {
            "kind": "grant_ability", "ability": DRAIN}}])

    def test_regeneration_is_purchased_flag_gated_half_ordinary_period(self):
        overrides = {tuple(row["path"]): row["value"] for row in source("rules.toml")["overrides"]}
        self.assertIn(("talents", REGENERATION), overrides)
        self.assertEqual(overrides[("flags", "regenerative")]["modifiers"], [])
        talent = overrides[("talents", REGENERATION)]
        self.assertEqual(talent["requirement"], {"kind": "all", "requirements": [
            {"kind": "total_hd", "minimum": 1},
            {"kind": "flag", "flag": "regenerative", "present": True}]})
        self.assertEqual(talent["modifiers"], [{"scope": "always", "op": {
            "kind": "add", "stat": {"kind": "health_recovery_period_percent"}, "value": -50}}])
        self.assertNotIn(("abilities", REGENERATION), overrides)


if __name__ == "__main__":
    unittest.main()
