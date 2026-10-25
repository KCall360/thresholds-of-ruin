"""Authored Rogue content contracts, independent of generated certificates."""
from pathlib import Path
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((PACKAGE / name).read_text(encoding="utf-8"))


class RoguePlayableContent(unittest.TestCase):
    def test_both_presets_keep_the_initial_purchase_and_future_class_explicit(self):
        manifest = source("scenario.toml")
        self.assertEqual(manifest["default_character"], 1)
        self.assertEqual(len(manifest["characters"]), 2)
        for character, expected in zip(manifest["characters"], (
                (1, "Warrior", "warrior", ["heavy_weaponry", "discipline"]),
                (2, "Mage", "mage", ["spellcasting", "heavy_weaponry"])), strict=True):
            actor, name, class_id, ranks = expected
            with self.subTest(character=actor):
                self.assertEqual(character["id"], actor)
                self.assertEqual(character["unselected"], "omit")
                self.assertEqual(character["anchor"], "dungeon/1/start")
                # Ground-target acceptance uses this explicit eye-relative
                # Manhattan offset; do not silently change the driver geometry.
                self.assertEqual(character["body"]["eye"], [0, 0, 1])
                build = character["creature"]
                self.assertEqual(build["name"], name)
                self.assertEqual(build["species"], "rogue.human")
                self.assertEqual(build["binding"], "intellect")
                self.assertEqual(build["starting_skills"], ranks)
                self.assertEqual(build["restoration"], "choose_again")
                self.assertEqual(build["nutrition"], {"kind": "required"})
                self.assertEqual(build["hit_dice"], [{"source": class_id, "purchases": []}])
                self.assertEqual(build["future_growth"], {
                    "kind": "selected", "source": {"kind": "class", "class": class_id}})

    def test_selected_character_loadout_declares_equipment_and_finite_known_stock(self):
        manifest = source("scenario.toml")
        items = source("catalog/items.toml")["items"]
        expected = [
            {"archetype": "rogue.mace", "quantity": 1, "equipped_slot": 0},
            {"archetype": "rogue.ring_mail", "quantity": 1, "equipped_slot": 2},
            {"archetype": "rogue.clean_bandage", "quantity": 3},
            {"archetype": "rogue.ration", "quantity": 1},
        ]
        self.assertEqual(set(items), {'rogue.torch', 'rogue.wand_cancellation', 'rogue.wand_magic_missile', 'rogue.wand_drain_life', 'rogue.wand_light', 'rogue.wand_nothing', 'rogue.clean_bandage', 'rogue.identify_armor', 'rogue.ration', 'rogue.dagger', 'rogue.banded_mail', 'rogue.healing', 'rogue.confusion', 'rogue.blindness', 'rogue.poison', 'rogue.see_invisible', 'rogue.extra_healing', 'rogue.wand_invisibility', 'rogue.wand_haste', 'rogue.wand_slow', 'rogue.generated_ring_mail', 'rogue.enchant_armor', 'rogue.long_sword', 'rogue.remove_curse', 'rogue.wand_lightning', 'rogue.wand_cold', 'rogue.studded_leather', 'rogue.two_handed_sword', 'rogue.splint_mail', 'rogue.identify_weapon', 'rogue.plate_mail', 'rogue.enchant_weapon', 'rogue.identify_wand', 'rogue.amulet_of_yendor', 'rogue.protect_armor', 'rogue.mace', 'rogue.restore_strength', 'rogue.identify_scroll', 'rogue.leather_armor', 'rogue.chain_mail', 'rogue.ring_mail', 'rogue.generated_mace', 'rogue.spear', 'rogue.gold', 'rogue.identify_potion', 'rogue.sleep', 'rogue.scale_mail', 'rogue.wand_fire', 'rogue.shortbow', 'rogue.arrow', 'rogue.ring_protection', 'rogue.ring_add_strength', 'rogue.ring_sustain_strength', 'rogue.ring_see_invisible', 'rogue.ring_adornment', 'rogue.ring_dexterity', 'rogue.ring_increase_damage', 'rogue.ring_regeneration', 'rogue.ring_slow_digestion', 'rogue.ring_maintain_armor', 'rogue.ring_searching', 'rogue.ring_teleportation', 'rogue.levitation', 'rogue.teleportation', 'rogue.wand_teleport_away', 'rogue.wand_teleport_to', 'rogue.ring_stealth', 'rogue.ring_aggravation', 'rogue.aggravate_monster', 'rogue.create_monster', 'rogue.hold_monster', 'rogue.monster_confusion', 'rogue.scare_monster'})
        for character in manifest["characters"]:
            self.assertEqual(character["loadout"], expected)
            self.assertEqual(set(character["known_identities"]), {"rogue.mace", "rogue.ring_mail", "rogue.clean_bandage", "rogue.ration", "rogue.amulet_of_yendor"})
        self.assertEqual(items["rogue.mace"]["equipment"]["slot"], "weapon")
        self.assertEqual(items["rogue.ring_mail"]["equipment"]["slot"], "body_armor")
        for item, supply in [("rogue.clean_bandage", "bandage"), ("rogue.ration", "ration")]:
            self.assertEqual(items[item]["supply"], supply)
            self.assertEqual(items[item]["identity"], item)
            self.assertTrue(items[item]["stackable"])
        # One finite stock supports either ordinary eating or the existing respec transaction.
        self.assertNotIn("consumable", items["rogue.ration"])
        self.assertEqual(items["rogue.ration"]["edible"], {"nutrition": {"kind": "rolled",
            "dice": {"count": 1, "sides": 400, "bonus": 1099}}})

    def test_floor_profile_is_selected_protagonist_once_only_finite_content(self):
        manifest = source("scenario.toml")
        self.assertEqual(manifest["capabilities"], {"npc_stair_traversal": False})
        activation = manifest["floor_activation"]
        self.assertEqual(activation["version"], 1)
        self.assertEqual(activation["protagonist"], "selected_character")
        self.assertEqual(activation["families"], {"dungeon": "rogue.low_depth"})
        profile = activation["profiles"]["rogue.low_depth"]
        self.assertEqual(profile["version"], 1)
        stages = profile["stages"]
        self.assertEqual([row["id"] for row in stages],
                         ["rogue.eight_species_foes", "rogue.bandages", "rogue.rations",
                          "rogue.restore_strength", "rogue.room_gold", "rogue.magic_potions", "rogue.magic_scrolls",
            "rogue.magic_devices", "rogue.equipment", "rogue.equipment_scrolls", "rogue.ammunition", "rogue.rings", "rogue.control_scrolls"])
        for stage, count in zip([*stages[:4], *stages[5:8]], (4, 1, 1, 1, 1, 1, 1), strict=True):
            self.assertEqual(stage["version"], 1)
            self.assertEqual(stage["count"], [count, count])
        actor = stages[0]
        self.assertEqual(actor["kind"], "actors")
        self.assertEqual(actor["ai"], "rogue_hostile")
        self.assertTrue(actor["avoid_protagonist_sight"])
        self.assertEqual(actor["depth_growth"], {"kind": "racial_after", "depth": 26})
        self.assertNotIn("quantity", actor)
        for stage, archetype, quantity in zip(stages[1:4], (
                "rogue.clean_bandage", "rogue.ration", "rogue.restore_strength"), (2, 1, 1), strict=True):
            self.assertEqual(stage["kind"], "items")
            self.assertEqual(stage["bands"], [{"depth": {"kind": "open", "minimum": 1},
                              "choices": [{"archetype": archetype, "weight": 1}]}])
            self.assertEqual(stage["quantity"], quantity)
            self.assertNotIn("ai", stage)
            self.assertNotIn("avoid_protagonist_sight", stage)

    def test_foes_use_shared_builds_with_rolled_attacks_and_disabled_autonomous_growth(self):
        actors = source("catalog/actors.toml")["actors"]
        species = source("catalog/creatures.toml")["creatures"]["species"]
        self.assertEqual(set(actors), {"rogue.emu", "rogue.snake", "rogue.rattlesnake", "rogue.hobgoblin",
                         "rogue.zombie", "rogue.quagga", "rogue.centaur", "rogue.yeti", "rogue.troll", "rogue.wraith",
                         "rogue.black_unicorn", "rogue.ice_monster", "rogue.vampire", "rogue.jabberwock", "rogue.bat", "rogue.kestrel", "rogue.venus_flytrap", "rogue.griffin",
                         "rogue.leprechaun", "rogue.nymph", "rogue.orc", "rogue.medusa", "rogue.phantom", "rogue.xeroc",
                         "rogue.aquator", "rogue.dragon"})
        for identity, category, sides in [("rogue.emu", "impact", 2), ("rogue.snake", "keen", 3)]:
            with self.subTest(species=identity):
                build = actors[identity]["creature"]
                self.assertEqual(build["species"], identity)
                self.assertEqual(build["faction"], "dungeon_foes")
                self.assertEqual(build["future_growth"], {"kind": "disabled"})
                self.assertEqual(build["nutrition"], {"kind": "exempt"})
                self.assertEqual(build["hit_dice"], [{"source": "racial", "purchases": []}])
                definition = species[identity]
                self.assertEqual(definition["kind"], "animal")
                self.assertEqual(definition["anatomy"], {
                    "slots": [], "capacities": {}, "primary_attack_socket": {"kind": "none"}})
                strikes = definition["melee"]["strikes"]
                self.assertEqual(len(strikes), 1)
                damage = strikes[0]["on_success"][0]["damage"]
                self.assertEqual(damage, {"kind": "components", "components": [{
                    "category": category, "amount": {"kind": "rolled", "dice": {
                        "count": 1, "sides": sides, "bonus": 0}}}]})
        self.assertEqual(species["rogue.snake"]["subtypes"], ["reptilian"])
        self.assertNotIn("subtypes", species["rogue.emu"])

    def test_playable_catalog_keeps_compact_geometry_and_shared_rules(self):
        manifest = source("scenario.toml")
        self.assertEqual(manifest["modules"], [
            "catalog/creatures.toml", "catalog/actors.toml", "catalog/items.toml"])
        self.assertEqual(set(manifest["procedural_worlds"]), {"dungeon"})
        family = manifest["procedural_worlds"]["dungeon"]
        self.assertEqual(family["grid"], [3, 3])
        self.assertEqual(family["size"], [26, 7, 2])
        self.assertEqual(family["domain"], {"kind": "open", "max_depth": 1_000_000})
        self.assertNotIn("objective", manifest)
        self.assertNotIn("generation_groups", manifest)
        self.assertFalse((PACKAGE / "regions").exists())
        rules = source("rules.toml")
        self.assertEqual(set(rules), {"import", "overrides"})
        policies = {tuple(row["path"]): row["value"] for row in rules["overrides"]}
        self.assertEqual(policies[("nutrition",)]["initial"], 1300)
        self.assertEqual(policies[("nutrition",)]["starve_at"], -852)
        self.assertEqual(policies[("nutrition",)]["regeneration_cost"], {
            "denominator": 1, "health": 1, "stamina": 1, "focus": 1, "mana": 1})
        for field in ("hungry_recovery", "weak_recovery", "empty_recovery"):
            self.assertEqual(policies[("nutrition",)][field], {"kind": "normal"})
        trigger = policies[("nutrition_condition_trigger",)]
        self.assertEqual(trigger["chance"], {"one_in": 5})
        self.assertEqual(trigger["duration"], {"kind": "rolled", "dice": {"count": 1, "sides": 8, "bonus": 3}})
        self.assertEqual(trigger["ticks_per_unit"], 100)
        condition = policies[("conditions", trigger["condition"])]
        self.assertEqual(condition["restrictions"], ["voluntary_actions"])
        self.assertEqual(condition["preparation"], "pause")
        self.assertEqual(condition["timing"], {"kind": "actor_active", "on_expiry": {"kind": "remove"}})
        self.assertEqual(condition["descriptors"], [])
        self.assertTrue(rules["import"].startswith("interactions-v"))


if __name__ == "__main__":
    unittest.main()
