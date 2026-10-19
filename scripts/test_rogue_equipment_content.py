"""Finite equipment birth/content contracts; no compiler or runtime invocation."""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1] / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((ROOT / name).read_text(encoding="utf-8"))


class RogueEquipmentContent(unittest.TestCase):
    def test_generated_gear_keeps_source_distributions_and_nonlauncher_damage(self):
        items = source("catalog/items.toml")["items"]
        weapons = {"generated_mace": (2, 4), "long_sword": (3, 4), "dagger": (1, 6),
                   "two_handed_sword": (4, 4), "spear": (2, 3)}
        armor = {"leather_armor": 1, "generated_ring_mail": 2, "studded_leather": 2,
                 "scale_mail": 3, "chain_mail": 4, "splint_mail": 5,
                 "banded_mail": 5, "plate_mail": 6}
        generated = {key for key, value in items.items()
                     if value.get("equipment_generation", {}).get("kind") in ("weapon", "armor")}
        self.assertEqual(generated, {"rogue." + key for key in weapons | armor} | {"rogue.shortbow"})
        for key, dice in weapons.items():
            item = items["rogue." + key]
            self.assertEqual(item["class"], "weapon")
            self.assertEqual(item["equipment_generation"], {"kind": "weapon", "curse_percent": 10,
                             "enchant_percent": 5, "minimum": 1, "maximum": 3})
            attack = item["equipment"]["attack"]
            amount = attack["damage"]["components"][0]["amount"]
            self.assertEqual((amount["count"], amount["sides"], amount["bonus"]), (*dice, 0))
            self.assertEqual((attack["bonus"], attack["wind_up"], attack["recovery"]), (0, 60, 40))
        for key, physical in armor.items():
            item = items["rogue." + key]
            self.assertEqual(item["class"], "armor")
            self.assertEqual(item["equipment_generation"], {"kind": "armor", "curse_percent": 20,
                             "enchant_percent": 8, "minimum": 1, "maximum": 3})
            self.assertEqual(item["equipment"]["armor"], {"physical_reduction": physical,
                             "material": "nonmetal" if key == "leather_armor" else "metal",
                             "rustproof": False})

    def test_starting_kit_is_explicit_authored_and_keeps_effective_stats(self):
        items = source("catalog/items.toml")["items"]
        for starter, generated in (("mace", "generated_mace"), ("ring_mail", "generated_ring_mail")):
            first, second = items["rogue." + starter], items["rogue." + generated]
            self.assertEqual(first["equipment"], second["equipment"])
            self.assertEqual(first["identity"], second["identity"])
            self.assertEqual(first["equipment_generation"]["kind"], "authored")
        mace = items["rogue.mace"]
        attack = mace["equipment"]["attack"]
        birth = mace["equipment_generation"]
        self.assertEqual(attack["bonus"] + birth["hit_enchantment"], 1)
        amount = attack["damage"]["components"][0]["amount"]
        self.assertEqual((amount["count"], amount["sides"], amount["bonus"] + birth["damage_enchantment"]), (2, 4, 1))
        self.assertEqual(items["rogue.ring_mail"]["equipment"]["armor"]["physical_reduction"], 2)
        for character in source("scenario.toml")["characters"]:
            self.assertEqual(character["loadout"][:2], [
                {"archetype": "rogue.mace", "quantity": 1, "equipped_slot": 0},
                {"archetype": "rogue.ring_mail", "quantity": 1, "equipped_slot": 2}])

    def test_scroll_targets_and_discovery_are_explicit(self):
        catalog = source("catalog/items.toml")
        items = catalog["items"]
        for key in ("enchant_armor", "protect_armor", "enchant_weapon", "remove_curse"):
            selection = ({"kind": "all"} if key == "remove_curse" else
                         {"kind": "slot", "slot": "weapon" if key == "enchant_weapon" else "body_armor",
                          "class": "weapon" if key == "enchant_weapon" else "armor",
                          "last": key == "enchant_weapon"})
            self.assertEqual(items["rogue." + key]["readable"], {"discovery": "never", "effects": [
                {"kind": "alter_equipment", "selection": selection, "alteration": key}]})
        for category in ("weapon", "armor"):
            self.assertEqual(items["rogue.identify_" + category]["readable"],
                             {"identify": category, "discovery": "on_use"})
        scrolls = [item for item in items.values() if item.get("appearance_pool") == "rogue.scrolls"]
        self.assertEqual(len(scrolls), 10)
        self.assertEqual(len(set(catalog["appearance_pools"]["rogue.scrolls"]["appearances"])), len(scrolls))

    def test_existing_stages_are_preserved_and_new_stock_is_finite(self):
        stages = source("scenario.toml")["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
        self.assertEqual([stage["id"] for stage in stages], ["rogue.eight_species_foes", "rogue.bandages",
                         "rogue.rations", "rogue.restore_strength", "rogue.room_gold", "rogue.magic_potions",
                         "rogue.magic_scrolls", "rogue.magic_devices", "rogue.equipment", "rogue.equipment_scrolls", "rogue.ammunition"])
        expected = [
            ["healing", "confusion", "blindness"],
            ["identify_potion", "identify_scroll", "identify_wand", "sleep"],
            ["wand_fire", "wand_cold", "wand_lightning"],
            ["generated_mace", "long_sword", "dagger", "two_handed_sword", "spear", "leather_armor",
             "generated_ring_mail", "studded_leather", "scale_mail", "chain_mail", "splint_mail", "banded_mail", "plate_mail", "shortbow"],
            ["enchant_armor", "protect_armor", "enchant_weapon", "remove_curse", "identify_weapon", "identify_armor"],
        ]
        for stage, names in zip(stages[5:10], expected, strict=True):
            self.assertEqual(stage, {"id": stage["id"], "version": 1, "kind": "items", "count": [1, 1], "quantity": 1,
                             "bands": [{"depth": {"kind": "open", "minimum": 1}, "choices": [
                                 {"archetype": "rogue." + name, "weight": 1} for name in names]}]})


    def test_ammunition_is_one_finite_six_arrow_stack_after_existing_stages(self):
        stages = source("scenario.toml")["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
        self.assertEqual(stages[10], {
            "id": "rogue.ammunition", "version": 1, "kind": "items", "count": [1, 1],
            "quantity": 6, "bands": [{"depth": {"kind": "open", "minimum": 1},
            "choices": [{"archetype": "rogue.arrow", "weight": 1}]}]})



if __name__ == "__main__":
    unittest.main()
