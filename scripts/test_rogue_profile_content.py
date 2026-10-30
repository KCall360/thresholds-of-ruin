"""The Rogue-only package: one locked build and Rogue's mechanics only.

It shares the rogue-exploration catalog by copy. Divergence is deliberate per
tuning milestone, so any catalog difference must be listed here.
"""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue"
STARTER = ROOT / "scenarios/rogue-exploration"


def load(path):
    return tomllib.loads(path.read_text(encoding="utf-8"))


def ranks(build):
    counts = {skill: 1 for skill in build["starting_skills"]}
    for row in build["hit_dice"] + build["growth_schedule"]:
        for purchase in row.get("purchases", []):
            if purchase["selection"]["kind"] == "skill_rank":
                skill = purchase["selection"]["skill"]
                counts[skill] = counts.get(skill, 0) + 1
    return counts


class RogueProfileContent(unittest.TestCase):
    def setUp(self):
        self.manifest = load(PACKAGE / "scenario.toml")
        self.rules = load(PACKAGE / "rules.toml")

    def test_one_preset_with_a_locked_scheduled_build(self):
        [character] = self.manifest["characters"]
        self.assertEqual(self.manifest["default_character"], character["id"])
        build = character["creature"]
        self.assertEqual(build["restoration"], "authored")
        self.assertEqual(build["future_growth"], {"kind": "authored"})
        self.assertEqual([row["source"] for row in build["hit_dice"]], ["racial", "warrior"])
        schedule = build["growth_schedule"]
        # Rogue's experience table has 20 thresholds after level 1.
        self.assertEqual(len(schedule), 20)
        self.assertTrue(all(row["source"] == "warrior" for row in schedule))
        self.assertTrue(all(len(row["purchases"]) <= 1 for row in schedule))
        self.assertTrue(all(count <= 5 for count in ranks(build).values()), ranks(build))

    def test_capabilities_disable_owner_choices_and_corpses(self):
        self.assertEqual(self.manifest["capabilities"], {
            "npc_stair_traversal": False, "owner_advancement": False, "corpses": False})

    def test_rogue_starting_kit_has_no_bandages(self):
        [character] = self.manifest["characters"]
        kit = {row["archetype"]: row["quantity"] for row in character["loadout"]}
        self.assertEqual(kit, {"rogue.mace": 1, "rogue.ring_mail": 1, "rogue.shortbow": 1,
                               "rogue.arrow": 32, "rogue.ration": 1})
        self.assertTrue(set(kit) <= set(character["known_identities"]))

    def test_no_bandage_stock_or_universal_care(self):
        stages = self.manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
        self.assertNotIn("rogue.bandages", [stage["id"] for stage in stages])
        for pool in self.manifest["floor_activation"]["carried_item_pools"].values():
            self.assertFalse([row for row in pool["choices"] if "bandage" in row["archetype"]])
        [defaults] = [row["value"] for row in self.rules["overrides"]
                      if row["path"] == ["constants", "default_abilities"]]
        self.assertNotIn("bind_wounds", defaults)

    def test_item_stages_follow_rogue_generation(self):
        stages = {stage["id"]: stage for stage in
                  self.manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]}
        # Two independent groups keep each type's expected count equal to
        # Rogue's nine 36% attempts with category x subtype weights.
        expected = {}
        for stage in (stages["rogue.items"], stages["rogue.items_other"]):
            self.assertEqual((stage["kind"], stage["attempts"]), ("chance_items", 9))
            chance = stage["chance"]["numerator"] / stage["chance"]["denominator"]
            weights = {row["archetype"]: row["weight"] for row in stage["bands"][0]["choices"]}
            for archetype, weight in weights.items():
                expected[archetype] = 9 * chance * weight / sum(weights.values())
        self.assertAlmostEqual(sum(expected.values()), 9 * 0.36)
        food = expected["rogue.ration"] + expected["rogue.fruit"]
        self.assertAlmostEqual(food, 9 * 0.36 * 0.16)
        self.assertAlmostEqual(expected["rogue.ration"] / food, 0.9)
        self.assertAlmostEqual(expected["rogue.healing"], 9 * 0.36 * 0.26 * 0.13)
        drought = stages["rogue.food_drought"]
        # TOR's threshold counts earlier floors and places food when more than
        # it were foodless; Rogue's fourth foodless level is forced, so 2.
        self.assertEqual((drought["kind"], drought["threshold"]), ("food_drought", 2))
        self.assertEqual(set(drought["food"]), {"rogue.ration", "rogue.fruit"})
        self.assertEqual(stages["rogue.treasure_room"]["chance"], {"numerator": 1, "denominator": 20})
        # The committed stages are exactly the generator's current output.
        import rogue_item_table
        text = (PACKAGE / "scenario.toml").read_text(encoding="utf-8")
        self.assertEqual(rogue_item_table.rewrite(text), text)

    def test_only_listed_sources_differ_from_the_starter_package(self):
        for name in ["catalog/actors.toml", "catalog/bodies.toml", "catalog/items.toml"]:
            self.assertEqual((PACKAGE / name).read_bytes(), (STARTER / name).read_bytes(), name)
        # Species differ only in tuned values: Health, defense and accuracy
        # modifiers, the melee bonus and damage amounts. Orcs are greedy, as in
        # Rogue, where the starter package keeps them passive.
        def untuned(path):
            species = load(path / "catalog/creatures.toml")["creatures"]["species"]
            species["rogue.orc"]["behavior"]["disposition"] = "greedy"
            for row in species.values():
                row["modifiers"] = [modifier for modifier in row.get("modifiers", [])
                                    if not (modifier["op"].get("kind") == "add"
                                            and modifier["op"]["stat"]["kind"] in ("health", "defense"))
                                    and modifier["op"].get("kind") != "attribute_without_defense"]
                if "bonus" in row.get("melee", {}):
                    row["melee"]["bonus"] = 0
                for strike in row.get("melee", {}).get("strikes", []):
                    for effect in strike.get("on_success", []):
                        for component in effect.get("damage", {}).get("components", []):
                            # Damage amounts are tuned values (die size and bonus).
                            component["amount"] = "tuned"
            return species
        self.assertEqual(untuned(PACKAGE), untuned(STARTER))
        starter = load(STARTER / "rules.toml")
        changed = [row["path"] for row, old in zip(self.rules["overrides"], starter["overrides"])
                   if row != old]
        self.assertEqual(changed, [["nutrition"], ["constants", "default_abilities"]])
        added = [row["path"] for row in self.rules["overrides"][len(starter["overrides"]):]]
        self.assertEqual(added, [["health_recovery"], ["growth", "threshold_base"],
                                 ["growth", "threshold_per_tier"]])

    def test_orcs_are_greedy_and_every_other_species_keeps_its_disposition(self):
        tuned = load(PACKAGE / "catalog/creatures.toml")["creatures"]["species"]
        starter = load(STARTER / "catalog/creatures.toml")["creatures"]["species"]
        self.assertEqual(tuned["rogue.orc"]["behavior"]["disposition"], "greedy")
        self.assertTrue(tuned["rogue.orc"]["behavior"]["gold"])
        self.assertEqual(starter["rogue.orc"]["behavior"]["disposition"], "passive")
        changed = [name for name in tuned
                   if tuned[name].get("behavior") != starter[name].get("behavior")]
        self.assertEqual(changed, ["rogue.orc"])

    def test_rogue_value_tuning_in_rules(self):
        by_path = {tuple(row["path"]): row["value"] for row in self.rules["overrides"]}
        nutrition = by_path[("nutrition",)]
        # Rogue's food constants; healing never costs food.
        self.assertEqual((nutrition["initial"], nutrition["maximum"], nutrition["hungry_below"],
                          nutrition["weak_below"], nutrition["starve_at"]), (1300, 2000, 300, 150, -852))
        self.assertEqual({key: value for key, value in nutrition["regeneration_cost"].items()
                          if key != "denominator"}, {"health": 0, "stamina": 0, "focus": 0, "mana": 0})
        recovery = by_path[("health_recovery",)]

        def turns_per_hp(level):
            hd = level + 1  # the body's racial HD plus one Warrior HD per level
            period = recovery["base_period"] - recovery["reduction_per_hd"] * (hd - 1)
            return max(recovery["minimum_period"], period) / 100
        # Rogue's doctor: 21 - 2 x level turns per HP below level 8.
        for level in (1, 3, 5, 7):
            self.assertLessEqual(abs(turns_per_hp(level) - (21 - 2 * level)), 2.5, level)


if __name__ == "__main__":
    unittest.main()
