"""Authored Rogue content contracts, independent of generated certificates."""
import hashlib
import json
from pathlib import Path
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue-exploration"


EXPANDED_ITEMS = (
    "haste", "gain_strength", "raise_level", "hallucination", "monster_detection",
    "magic_detection", "food_detection", "magic_mapping", "wand_nothing", "wand_light",
    "wand_drain_life", "wand_cancellation", "wand_magic_missile", "wand_polymorph",
    "fruit", "dart", "shuriken",
)
BASELINE_STAGE_IDS = (
    "eight_species_foes", "bandages", "rations", "restore_strength", "room_gold",
    "magic_potions", "magic_scrolls", "magic_devices", "equipment",
    "equipment_scrolls", "ammunition", "rings", "control_scrolls",
)


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
                self.assertEqual(build["body_package"], "rogue.body.human")
                self.assertEqual(build["hit_dice"], [{"source": "racial", "purchases": []},
                                                     {"source": class_id, "purchases": []}])
                package = source("catalog/bodies.toml")["creatures"]["body_packages"][build["body_package"]]
                self.assertEqual(package["species"], build["species"])
                self.assertEqual(package["hit_dice"], build["hit_dice"][:1])
                self.assertEqual(package["body"], character["body"])
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
        self.assertEqual(set(items), {'rogue.dart', 'rogue.shuriken', 'rogue.fruit', 'rogue.wand_polymorph', 'rogue.monster_detection', 'rogue.food_detection', 'rogue.magic_detection', 'rogue.magic_mapping', 'rogue.hallucination', 'rogue.gain_strength', 'rogue.raise_level', 'rogue.haste', 'rogue.torch', 'rogue.wand_cancellation', 'rogue.wand_magic_missile', 'rogue.wand_drain_life', 'rogue.wand_light', 'rogue.wand_nothing', 'rogue.clean_bandage', 'rogue.identify_armor', 'rogue.ration', 'rogue.dagger', 'rogue.banded_mail', 'rogue.healing', 'rogue.confusion', 'rogue.blindness', 'rogue.poison', 'rogue.see_invisible', 'rogue.extra_healing', 'rogue.wand_invisibility', 'rogue.wand_haste', 'rogue.wand_slow', 'rogue.generated_ring_mail', 'rogue.enchant_armor', 'rogue.long_sword', 'rogue.remove_curse', 'rogue.wand_lightning', 'rogue.wand_cold', 'rogue.studded_leather', 'rogue.two_handed_sword', 'rogue.splint_mail', 'rogue.identify_weapon', 'rogue.plate_mail', 'rogue.enchant_weapon', 'rogue.identify_wand', 'rogue.amulet_of_yendor', 'rogue.protect_armor', 'rogue.mace', 'rogue.restore_strength', 'rogue.identify_scroll', 'rogue.leather_armor', 'rogue.chain_mail', 'rogue.ring_mail', 'rogue.generated_mace', 'rogue.spear', 'rogue.gold', 'rogue.identify_potion', 'rogue.sleep', 'rogue.scale_mail', 'rogue.wand_fire', 'rogue.shortbow', 'rogue.arrow', 'rogue.ring_protection', 'rogue.ring_add_strength', 'rogue.ring_sustain_strength', 'rogue.ring_see_invisible', 'rogue.ring_adornment', 'rogue.ring_dexterity', 'rogue.ring_increase_damage', 'rogue.ring_regeneration', 'rogue.ring_slow_digestion', 'rogue.ring_maintain_armor', 'rogue.ring_searching', 'rogue.ring_teleportation', 'rogue.levitation', 'rogue.teleportation', 'rogue.wand_teleport_away', 'rogue.wand_teleport_to', 'rogue.ring_stealth', 'rogue.ring_aggravation', 'rogue.aggravate_monster', 'rogue.create_monster', 'rogue.hold_monster', 'rogue.monster_confusion', 'rogue.scare_monster'})
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

    def test_fruit_is_ordinary_named_live_food_with_ration_nutrition(self):
        catalog = source("catalog/items.toml")["items"]
        self.assertEqual(catalog["rogue.fruit"], {
            "asset": "item.food", "name": "fruit", "identity": "rogue.fruit",
            "class": "food", "stackable": True, "supply": "ration",
            "edible": {"nutrition": {"kind": "rolled", "dice": {
                "count": 1, "sides": 400, "bonus": 1099}}}})
        self.assertEqual(catalog["rogue.ration"]["edible"], {
            "nutrition": {"kind": "rolled", "dice": {
                "count": 1, "sides": 400, "bonus": 1099}}})
        self.assertNotEqual(catalog["rogue.fruit"]["identity"], catalog["rogue.ration"]["identity"])
        # Supplemental floor stock now includes fruit; ordinary actor recipes
        # and the original ration stage remain unchanged.
        self.assertIn("rogue.fruit", repr(source("scenario.toml")["floor_activation"]))
        self.assertIn("item.food", source("scenario.toml")["assets"]["dungeon"])
        self.assertNotIn("rogue.fruit", repr(source("catalog/actors.toml")))

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
            "rogue.magic_devices", "rogue.equipment", "rogue.equipment_scrolls", "rogue.ammunition", "rogue.rings", "rogue.control_scrolls", "rogue.expanded_stock", "rogue.treasure_room"])
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
            "catalog/creatures.toml", "catalog/bodies.toml", "catalog/actors.toml", "catalog/items.toml"])
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


    def test_supplement_is_one_finite_uniform_chance_without_replacing_old_stock(self):
        stages = source("scenario.toml")["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
        self.assertEqual([row["id"] for row in stages],
                         ["rogue." + name for name in (*BASELINE_STAGE_IDS, "expanded_stock", "treasure_room")])
        self.assertEqual(stages[-2], {
            "id": "rogue.expanded_stock", "version": 1, "kind": "items", "count": [0, 1],
            "bands": [{"depth": {"kind": "open", "minimum": 1}, "choices": [
                {"archetype": "rogue." + name, "weight": 1} for name in EXPANDED_ITEMS]}],
            "quantity": 1,
        })
        # Independent before-batch oracle: existing IDs, versions, weights,
        # count/quantity policies and every depth band remain exactly unchanged.
        canonical = json.dumps(stages[:13], sort_keys=True, separators=(",", ":")).encode()
        self.assertEqual(hashlib.sha256(canonical).hexdigest(), "71e762a7f3d210c37c40349139268d2fb31e6132bca30f6d85e949654a13fde8")
        items = source("catalog/items.toml")["items"]
        self.assertEqual(len(set(EXPANDED_ITEMS)), 17)
        for name in EXPANDED_ITEMS:
            self.assertIn("rogue." + name, items)
        self.assertNotIn("torch", EXPANDED_ITEMS)

    def test_live_layout_selects_existing_bounded_policies_without_retuning_other_stages(self):
        self.assertEqual(source("scenario.toml")["generation_recipes"]["exploration"], {
            "kind": "grid_rooms", "version": 1, "room_width": [4, 24], "room_height": [3, 5],
            "extra_connections": [0, 2], "room_omissions": {"count": [0, 1]},
            "maze_rooms": {"one_in": 15}, "lighting": {"darkness_roll": 10, "darkness_start": 1},
            "secret_entrances": {"chance_per_mille": 250, "maximum": 2, "dc": 12},
            "hidden_traps": {"count": [1, 2], "dc": 12, "rust_slot": "body_armor", "rust_amount": 1,
                "sleep_condition": "rogue.sleep", "sleep_duration": 200, "sleep_exposure": "poison",
                "hazards": {"avoidance_dc": 12, "bear_condition": "rogue.bear_trap", "bear_duration": 300,
                    "arrow_ammunition": "rogue.arrow", "attack_bonus": 0, "poison_exposure": "poison"}},
        })

    def test_live_treasure_is_additional_bounded_stock_with_the_existing_next_depth_roster(self):
        manifest = source("scenario.toml")
        stages = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
        self.assertEqual(stages[-1], {
            "id": "rogue.treasure_room", "version": 1, "kind": "room_cluster",
            "chance": {"numerator": 1, "denominator": 20}, "item_count": [2, 4],
            "actor_count": [1, 3], "quantity": 1, "actor_depth_offset": 1,
            "ai": "rogue_hostile", "depth_growth": {"kind": "racial_after", "depth": 26},
            "avoid_protagonist_sight": False,
            "item_bands": [{"depth": {"kind": "open", "minimum": 1}, "choices": [
                {"archetype": "rogue." + name, "weight": 1}
                for name in ("gold", "healing", "fruit", "dart", "shuriken")]}],
            "actor_bands": stages[0]["bands"],
        })
        self.assertNotIn("examples/treasure-stage.toml", manifest["modules"])


if __name__ == "__main__":
    unittest.main()
