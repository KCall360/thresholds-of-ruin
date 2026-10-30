"""Opt-in Hallucination source, duration recipe and exact blind-safe cure contract."""
from pathlib import Path
import tomllib
import unittest

PRODUCTION = Path(__file__).resolve().parents[1] / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((PRODUCTION / name).read_text(encoding="utf-8"))


class RogueHallucinationContent(unittest.TestCase):
    def test_dose_uses_base_turn_recipe_and_on_use_recognition(self):
        self.assertEqual(source("catalog/items.toml")["items"].get("rogue.hallucination"), {
            "name": "potion of hallucination", "identity": "rogue.hallucination",
            "asset": "item.potion", "magical": True, "class": "potion", "stackable": True,
            "appearance_pool": "rogue.potions", "consumable": {"discovery": "on_use", "effects": [
                {"kind": "apply_condition_for_turns", "condition": "rogue.hallucinating",
                 "duration": {"kind": "rolled", "dice": {"count": 1, "sides": 85, "bonus": 807}}}]}})

    def test_marker_is_a_single_pure_target_wide_extend_condition(self):
        conditions = {row["path"][1]: row["value"] for row in source("rules.toml")["overrides"]
                      if row["path"][0] == "conditions" and len(row["path"]) == 2}
        self.assertEqual({key for key, value in conditions.items() if value.get("hallucinating")},
                         {"rogue.hallucinating"})
        self.assertEqual(conditions.get("rogue.hallucinating"), {
            "id": "rogue.hallucinating", "family": "rogue.hallucinating", "lifecycle": "timed",
            "name": "Hallucinating", "hallucinating": True, "modifiers": [],
            "stacking": {"kind": "extend"}, "source_scoped": False, "descriptors": [],
            "capacity": "reject", "timing": {"kind": "actor_active", "on_expiry": {"kind": "remove"}},
            "restrictions": [], "preparation": "preserve"})

    def test_only_extra_healing_cures_exact_marker_unconditionally(self):
        items = source("catalog/items.toml")["items"]
        self.assertEqual(items["rogue.extra_healing"]["consumable"], {
            "discovery": "on_use", "effects": [
                {"kind": "heal_per_hit_die", "sides": 8, "bonus": 0,
                 "overflow": "improve_earliest_hit_dice_by_excess"},
                {"kind": "remove_condition", "condition": "rogue.blindness"},
                {"kind": "remove_condition", "condition": "rogue.hallucinating"}]})
        self.assertEqual({name for name, item in items.items()
                          if {"kind": "remove_condition", "condition": "rogue.hallucinating"}
                          in item.get("consumable", {}).get("effects", [])}, {"rogue.extra_healing"})

    def test_old_appearances_are_preserved_and_new_identity_has_distinct_label(self):
        catalog = source("catalog/items.toml")
        self.assertEqual(catalog["appearance_pools"]["rogue.potions"]["appearances"], [
            "amber potion", "smoky potion", "violet potion", "azure potion", "crimson potion",
            "silver potion", "pearl potion", "teal potion", "ochre potion", "indigo potion", "copper potion",
            "chartreuse potion", "saffron potion", "cerulean potion"])
        identities = [name for name, item in catalog["items"].items()
                      if item.get("appearance_pool") == "rogue.potions"]
        self.assertEqual(len(identities), 14)

    def test_supplement_preserves_existing_potion_stock_and_starter_kits(self):
        manifest = source("scenario.toml")
        self.assertIn("rogue.hallucination", repr(manifest["floor_activation"]))
        potion = next(stage for stage in manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
                      if stage["id"] == "rogue.magic_potions")
        self.assertEqual(potion["bands"], [{"depth": {"kind": "open", "minimum": 1}, "choices": [
            {"archetype": "rogue." + kind, "weight": 1} for kind in
            ("healing", "confusion", "blindness", "poison", "see_invisible", "extra_healing", "levitation")]}])
        for character in manifest["characters"]:
            self.assertNotIn("rogue.hallucination", character["known_identities"])
            self.assertNotIn("rogue.hallucination", [row["archetype"] for row in character["loadout"]])


if __name__ == "__main__":
    unittest.main()
