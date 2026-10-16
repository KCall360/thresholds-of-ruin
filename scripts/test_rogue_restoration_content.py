"""Production R/VN1 and ordinary finite restoration binding contracts."""
from pathlib import Path
import re
import tomllib
import unittest

from rogue_activation_candidate import conditional_bands

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue-exploration"
RATTLESNAKE = "rogue.rattlesnake"
VENOM = "rogue.venomous_bite"
POTION = "rogue.restore_strength"


def source(name):
    return tomllib.loads((PACKAGE / name).read_text(encoding="utf-8"))


class RogueRestorationContent(unittest.TestCase):
    def test_rattlesnake_owns_two_racial_rows_only_vn1_at_hd_one(self):
        actors = source("catalog/actors.toml")["actors"]
        self.assertIn(RATTLESNAKE, actors, "Approved R recipe is missing")
        actor = actors[RATTLESNAKE]
        snake = actors["rogue.snake"]
        self.assertEqual(actor["body"], snake["body"])
        self.assertEqual(actor["turn_ticks"], snake["turn_ticks"])
        build = actor["creature"]
        self.assertEqual(build["hit_dice"], [
            {"source": "racial", "purchases": [{"pool": "advancement", "selection": {
                "kind": "talent", "talent": VENOM}}]},
            {"source": "racial", "purchases": []}])
        self.assertEqual(build["restoration"], "authored")
        self.assertEqual(build["nutrition"], {"kind": "exempt"})
        self.assertEqual(build["future_growth"], {"kind": "disabled"})
        self.assertEqual(build["species"], RATTLESNAKE)

    def test_venom_eligibility_and_contact_have_no_free_poison_grant(self):
        species = source("catalog/creatures.toml")["creatures"]["species"]
        self.assertIn(RATTLESNAKE, species, "Approved R species is missing")
        definition = species[RATTLESNAKE]
        self.assertEqual(definition["kind"], "animal")
        self.assertEqual(definition["subtypes"], ["reptilian"])
        self.assertEqual(definition["behavior"], {
            "disposition": "mean", "idle": "wait", "memory_ticks": 1000})
        self.assertEqual(definition["attributes"], species["rogue.snake"]["attributes"])
        self.assertEqual(definition["modifiers"], [{"scope": "always", "op": {
            "kind": "flag", "flag": "venomous"}}])
        strike, = definition["melee"]["strikes"]
        self.assertEqual(strike["check"], {"actor": "user", "skill": {"kind": "natural_attack"},
            "against": {"kind": "defense", "defense": "physical"}})
        self.assertEqual(strike["on_failure"], [])
        self.assertEqual(strike["riders"], [])
        self.assertEqual(strike["descriptors"], [])
        self.assertEqual(strike["on_success"], [{"kind": "damage", "damage": {"kind": "components",
            "components": [{"category": "keen", "amount": {"kind": "rolled", "dice": {
                "count": 1, "sides": 6, "bonus": 0}}}]}}])

    def test_vn1_is_owned_natural_contact_then_separately_checked_poison_loss(self):
        overrides = {tuple(row["path"]): row["value"] for row in source("rules.toml")["overrides"]}
        self.assertIn(("abilities", VENOM), overrides, "Approved VN1 signature is missing")
        self.assertEqual(overrides[("flags", "venomous")], {
            "id": "venomous", "name": "Venomous", "modifiers": []})
        self.assertEqual(overrides[("descriptors", "poison")], {"id": "poison", "name": "Poison"})
        ability = overrides[("abilities", VENOM)]
        self.assertEqual(ability["interaction"], "hostile_to_target")
        self.assertFalse(ability["basic_attack"])
        self.assertEqual(ability["strikes"], {"kind": "natural_melee", "additional_riders": [{
            "trigger": "success", "check": {"skill": {"kind": "natural_attack"}, "defense": "physical"},
            "effects": [{"kind": "lose_attribute", "attribute": "strength",
                "amount": {"kind": "fixed", "amount": 1}}], "descriptors": ["poison"]}]})
        talent = overrides[("talents", VENOM)]
        self.assertEqual(talent["requirement"], {"kind": "all", "requirements": [
            {"kind": "total_hd", "minimum": 1}, {"kind": "flag", "flag": "venomous", "present": True}]})
        self.assertEqual(talent["modifiers"], [{"scope": "always", "op": {
            "kind": "grant_ability", "ability": VENOM}}])

    def test_restore_strength_is_concealed_stackable_and_only_removes_canonical_deficit(self):
        catalog = source("catalog/items.toml")
        self.assertIn(POTION, catalog["items"], "Ordinary restore-strength potion is missing")
        item = catalog["items"][POTION]
        bound = re.search(r"pub const DERIVED_ATTRIBUTE: u16 = (\d+);",
            (ROOT / "crates/simulation/src/limits.rs").read_text()).group(1)
        self.assertEqual(item, {"name": "potion of restore strength", "identity": POTION,
            "class": "potion", "stackable": True, "appearance_pool": "rogue.potions",
            "consumable": {"effects": [{"kind": "restore_attribute", "attribute": "strength",
                "amount": {"kind": "fixed", "amount": int(bound)}}]}})
        self.assertEqual(catalog["appearance_pools"]["rogue.potions"], {
            "appearances": ["amber potion", "smoky potion", "violet potion"]})
        for character in source("scenario.toml")["characters"]:
            self.assertNotIn(POTION, character["known_identities"])
            self.assertNotIn(POTION, [row["archetype"] for row in character["loadout"]])

    def test_rattlesnake_depth_edges_zero_carry_and_appended_finite_potion_stage(self):
        stages = source("scenario.toml")["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
        self.assertEqual([row["id"] for row in stages], ["rogue.eight_species_foes", "rogue.bandages",
            "rogue.rations", "rogue.restore_strength"])
        self.assertEqual(stages[-1], {"id": "rogue.restore_strength", "version": 1, "kind": "items",
            "count": [1, 1], "quantity": 1, "bands": [{"depth": {"kind": "open", "minimum": 1},
                "choices": [{"archetype": POTION, "weight": 1}]}]})
        for origin, bands in (("source", stages[0]["bands"]), ("helper", conditional_bands())):
            for depth, expected in ((2, False), (3, True), (12, True), (13, False)):
                with self.subTest(origin=origin, depth=depth):
                    band = next(row for row in bands if row["depth"]["minimum"] <= depth
                        and (row["depth"]["kind"] == "open" or depth <= row["depth"]["maximum"]))
                    choices = [row for row in band["choices"] if row["archetype"] == RATTLESNAKE]
                    self.assertEqual(len(choices), int(expected))
                    for choice in choices:
                        self.assertNotIn("carried_loot", choice)


if __name__ == "__main__":
    unittest.main()
