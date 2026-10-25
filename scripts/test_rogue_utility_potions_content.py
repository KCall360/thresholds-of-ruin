"""Source-backed poison/sight bindings and bounded generated-stock contracts."""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((PACKAGE / name).read_text(encoding="utf-8"))


class RogueUtilityPotionsContent(unittest.TestCase):
    def test_extra_healing_uses_owned_hd_excess_policy_and_blindness_cure(self):
        items = source("catalog/items.toml")["items"]
        self.assertIn("rogue.extra_healing", set(items))
        self.assertEqual(items["rogue.extra_healing"], {
            "asset": "item.potion", "name": "potion of extra healing", "identity": "rogue.extra_healing",
            "magical": True, "class": "potion", "stackable": True, "appearance_pool": "rogue.potions",
            "consumable": {"discovery": "on_use", "effects": [
                {"kind": "heal_per_hit_die", "sides": 8, "bonus": 0,
                 "overflow": "improve_earliest_hit_dice_by_excess"},
                {"kind": "remove_condition", "condition": "rogue.blindness"},
                {"kind": "remove_condition", "condition": "rogue.hallucinating"}]}})

    def test_poison_uses_only_rolled_restorable_strength_loss_and_on_use_discovery(self):
        items = source("catalog/items.toml")["items"]
        self.assertIn("rogue.poison", set(items), "The approved ordinary poison potion is missing")
        self.assertEqual(items["rogue.poison"], {
            "asset": "item.potion", "name": "potion of poison", "identity": "rogue.poison",
            "magical": True, "class": "potion", "stackable": True, "appearance_pool": "rogue.potions",
            "consumable": {"discovery": "on_use", "descriptors": ["poison"], "effects": [{"kind": "lose_attribute",
                "attribute": "strength", "amount": {"kind": "rolled", "dice": {
                    "count": 1, "sides": 3, "bonus": 0}}}]}})

    def test_sight_uses_finite_source_turns_cures_blindness_and_does_not_identify(self):
        items = source("catalog/items.toml")["items"]
        self.assertIn("rogue.see_invisible", set(items), "The approved ordinary sight potion is missing")
        self.assertEqual(items["rogue.see_invisible"], {
            "asset": "item.potion", "name": "potion of see invisible", "identity": "rogue.see_invisible",
            "magical": True, "class": "potion", "stackable": True, "appearance_pool": "rogue.potions",
            "consumable": {"discovery": "never", "effects": [
                {"kind": "apply_condition_for_turns", "condition": "rogue.see_invisible",
                 "duration": {"kind": "rolled", "dice": {"count": 1, "sides": 85, "bonus": 807}}},
                {"kind": "remove_condition", "condition": "rogue.blindness"}]}})
        overrides = {tuple(row["path"]): row["value"] for row in source("rules.toml")["overrides"]}
        self.assertEqual(overrides[("conditions", "rogue.see_invisible")], {
            "id": "rogue.see_invisible", "family": "rogue.see_invisible", "lifecycle": "timed",
            "name": "See Invisible", "modifiers": [{"scope": "always", "op": {"kind": "perception",
                "value": {"kind": "see_invisible", "enabled": True}}}],
            "stacking": {"kind": "extend"}, "source_scoped": False, "descriptors": [],
            "capacity": "reject", "timing": {"kind": "actor_active", "on_expiry": {"kind": "remove"}},
            "restrictions": [], "preparation": "preserve"})

    def test_distinct_concealed_potions_extend_only_the_existing_finite_stock_choices(self):
        catalog = source("catalog/items.toml")
        potion_ids = {key for key, item in catalog["items"].items()
                      if item.get("appearance_pool") == "rogue.potions"}
        self.assertEqual(potion_ids, {"rogue.restore_strength", "rogue.healing", "rogue.confusion",
                                     "rogue.blindness", "rogue.poison", "rogue.see_invisible", "rogue.extra_healing", "rogue.levitation", "rogue.haste", "rogue.gain_strength", "rogue.raise_level", "rogue.hallucination", "rogue.monster_detection", "rogue.magic_detection"})
        appearances = catalog["appearance_pools"]["rogue.potions"]["appearances"]
        self.assertEqual(len(appearances), len(set(appearances)))
        self.assertEqual(len(appearances), len(potion_ids))
        manifest = source("scenario.toml")
        stages = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
        self.assertEqual([stage["id"] for stage in stages], ["rogue.eight_species_foes", "rogue.bandages",
            "rogue.rations", "rogue.restore_strength", "rogue.room_gold", "rogue.magic_potions",
            "rogue.magic_scrolls", "rogue.magic_devices", "rogue.equipment", "rogue.equipment_scrolls",
            "rogue.ammunition", "rogue.rings", "rogue.control_scrolls"])
        self.assertEqual(stages[5], {"id": "rogue.magic_potions", "version": 1, "kind": "items",
            "count": [1, 1], "quantity": 1, "bands": [{"depth": {"kind": "open", "minimum": 1},
                "choices": [{"archetype": "rogue." + name, "weight": 1}
                            for name in ("healing", "confusion", "blindness", "poison", "see_invisible", "extra_healing", "levitation")]}]})
        for character in manifest["characters"]:
            for identity in ("rogue.poison", "rogue.see_invisible", "rogue.extra_healing"):
                self.assertNotIn(identity, character["known_identities"])
                self.assertNotIn(identity, [row["archetype"] for row in character["loadout"]])

    def test_service_and_process_fixture_preserve_production_effects_species_and_rules(self):
        from utility_potions_package import fixture
        data = fixture()
        manifest = data["manifest"]
        self.assertEqual(data["rules"], (PACKAGE / "rules.toml").read_text(encoding="utf-8"))
        self.assertEqual(manifest["creatures"], source("catalog/creatures.toml")["creatures"])
        for identity, item in source("catalog/items.toml")["items"].items():
            self.assertEqual(manifest["archetypes"][identity], item)
        phantom = source("catalog/actors.toml")["actors"]["rogue.phantom"]
        self.assertEqual(manifest["archetypes"]["rogue.phantom"], phantom)
        self.assertEqual(data["regions"][0]["actors"], [{"archetype": "rogue.phantom", "id": 3,
            "at": [3, 2, 0], "turn_ticks": 1000000, "controller": "external"}])
        self.assertFalse(data["wizard"])
        self.assertEqual(manifest["characters"][0]["known_identities"],
                         ["rogue.blindness", "rogue.restore_strength"])


if __name__ == "__main__":
    unittest.main()
