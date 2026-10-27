"""Approved physical-projectile authoring contracts; runtime has separate owners."""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue-exploration"


def source(relative):
    return tomllib.loads((PACKAGE / relative).read_text(encoding="utf-8"))


def override(*path):
    rows = [row for row in source("rules.toml")["overrides"] if row["path"] == list(path)]
    if len(rows) != 1:
        raise AssertionError(f"Expected one explicit override for {path}: {rows}")
    return rows[0]["value"]


class RogueProjectileContent(unittest.TestCase):
    def test_spear_retains_melee_and_generation_with_expert_only_durable_throw(self):
        item = source("catalog/items.toml")["items"]["rogue.spear"]
        self.assertEqual(item["equipment_generation"], {"kind": "weapon", "curse_percent": 10,
                         "enchant_percent": 5, "minimum": 1, "maximum": 3})
        self.assertFalse(item.get("stackable", False))
        self.assertEqual(item["equipment"]["attack"], {
            "skill": "heavy_weaponry", "bonus": 0, "wind_up": 60, "recovery": 40,
            "damage": {"primary": {"category": "keen", "sides": 3}, "components": [
                {"category": "keen", "amount": {"type": "rolled", "count": 2,
                                                "sides": 3, "bonus": 0}}]}})
        self.assertIn("weapon_profile", item["equipment"])
        self.assertEqual(item["equipment"]["weapon_profile"], {
            "expertise": "rogue.spear", "base_traits": [], "expert_traits": {
                "add": [{"kind": "thrown", "range": {"short_feet": 20, "long_feet": 60}}],
                "remove": []}, "thrown_payload": {"recovery": "recoverable", "damage": {
                    "primary": {"category": "keen", "sides": 6}, "components": [
                        {"category": "keen", "amount": {"type": "rolled", "count": 1,
                                                        "sides": 6, "bonus": 0}}]}}})
        self.assertNotIn("projectile", item)
        self.assertEqual(item["equipment"]["modifiers"], [])

    def test_dagger_retains_damage_and_timing_with_expert_only_throw_and_offhand(self):
        equipment = source("catalog/items.toml")["items"]["rogue.dagger"]["equipment"]
        self.assertEqual(equipment["attack"], {
            "skill": "light_weaponry", "bonus": 0, "wind_up": 60, "recovery": 40,
            "damage": {"primary": {"category": "keen", "sides": 6}, "components": [
                {"category": "keen", "amount": {"type": "rolled", "count": 1,
                                                "sides": 6, "bonus": 0}}]}})
        self.assertIn("weapon_profile", equipment)
        self.assertEqual(equipment["weapon_profile"], {
            "expertise": "rogue.knife", "base_traits": [], "expert_traits": {
                "add": [{"kind": "thrown", "range": {"short_feet": 20, "long_feet": 60}},
                        {"kind": "offhand"}], "remove": []}})

    def test_shortbow_owns_range_two_handed_requirement_and_expert_quickdraw(self):
        items = source("catalog/items.toml")["items"]
        self.assertIn("rogue.shortbow", items)
        item = items["rogue.shortbow"]
        self.assertEqual(item["equipment_generation"], {"kind": "weapon", "curse_percent": 10,
                         "enchant_percent": 5, "minimum": 1, "maximum": 3})
        equipment = item["equipment"]
        self.assertEqual(equipment["attack"], {
            "skill": "light_weaponry", "bonus": 0, "wind_up": 60, "recovery": 40,
            "damage": {"primary": {"category": "impact"}, "components": [
                {"category": "impact", "amount": {"type": "fixed", "value": 1}}]}})
        self.assertEqual(equipment["weapon_profile"], {
            "expertise": "rogue.shortbow", "launcher": {"family": "rogue.bow",
                "range": {"short_feet": 80, "long_feet": 320}},
            "base_traits": [{"kind": "two_handed"}], "expert_traits": {
                "add": [{"kind": "quickdraw"}], "remove": []}})

    def test_arrows_supply_exact_payload_and_recovery_without_equipment_or_birth_roll(self):
        items = source("catalog/items.toml")["items"]
        self.assertIn("rogue.arrow", items)
        item = items["rogue.arrow"]
        self.assertEqual(item["class"], "weapon")
        self.assertIs(item["stackable"], True)
        for field in ("equipment", "equipment_generation", "supply", "edible", "consumable"):
            self.assertNotIn(field, item)
        self.assertEqual(item["projectile"], {"family": "rogue.bow", "recovery": "consumed_on_hit",
            "damage": {"primary": {"category": "keen", "sides": 3}, "components": [
                {"category": "keen", "amount": {"type": "rolled", "count": 2,
                                                "sides": 3, "bonus": 0}}]}})

    def test_throw_launch_are_real_default_actions_without_base_costs_or_melee_sequence_tags(self):
        shared = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text())
        self.assertEqual(override("constants", "default_abilities"),
                         [*shared["constants"]["default_abilities"], "rogue.search", "rogue.throw", "rogue.launch"])
        self.assertEqual(override("ability_tags", "ranged"), {"id": "ranged", "name": "Ranged attack"})
        for delivery in ("throw", "launch"):
            ability = override("abilities", "rogue." + delivery)
            self.assertEqual(ability["name"], delivery.title())
            self.assertEqual(ability["tags"], ["attack", "ranged"])
            self.assertIs(ability["basic_attack"], False)
            self.assertEqual(ability["invocation"], "directional_projectile")
            self.assertEqual(ability["interaction"], "hostile_to_target")
            self.assertEqual(ability["outgoing_edge_policy"], {"kind": "ordinary"})
            self.assertEqual(ability["reach"], {"kind": "projectile"})
            self.assertEqual(ability["strikes"], {"kind": "projectile", "delivery": delivery})
            self.assertEqual(ability["timing"], {"source": {"kind": "selected_weapon"}, "scaled_by": "speed"})
            self.assertEqual(ability["supply_costs"], [])
            self.assertNotIn("cost", ability)
            self.assertNotIn("cooldown", ability)

    def test_expertises_are_choices_with_explicit_combat_prices_and_no_automatic_grants(self):
        for name in ("knife", "shortbow", "spear"):
            self.assertEqual(override("expertises", "rogue." + name), {
                "id": "rogue." + name, "name": name.title(), "category": "equipment"})
        self.assertEqual(override("growth", "threat", "overrides"), [
            {"source": {"kind": "expertise", "id": "rogue.knife"}, "points": 10},
            {"source": {"kind": "expertise", "id": "rogue.shortbow"}, "points": 10},
            {"source": {"kind": "expertise", "id": "rogue.spear"}, "points": 10}])
        shared = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text())
        self.assertEqual(shared["growth"]["threat"]["expertise"], 0)
        pool = next(row for row in shared["point_pools"] if row["id"] == "advancement")
        self.assertIn("expertise", pool["allowed"])
        for character in source("scenario.toml")["characters"]:
            rows = character["creature"]["hit_dice"]
            self.assertEqual(character["creature"]["body_package"], "rogue.body.human")
            self.assertEqual([row["source"] for row in rows], ["racial", "warrior" if character["id"] == 1 else "mage"])
            self.assertEqual([row["purchases"] for row in rows], [[], []])
        # An NPC expertise choice must be authored, never inferred from its kit.
        for actor in source("catalog/actors.toml")["actors"].values():
            for die in actor["creature"]["hit_dice"]:
                self.assertFalse(any(row["selection"]["kind"] == "expertise" for row in die["purchases"]))
        for row in source("rules.toml")["overrides"]:
            if row["path"][0] == "talents":
                self.assertFalse(row["value"].get("expertise_grants"))

    def test_discreet_is_not_an_authored_functional_trait(self):
        for item in source("catalog/items.toml")["items"].values():
            profile = item.get("equipment", {}).get("weapon_profile")
            if profile:
                kinds = [trait["kind"] for trait in profile["base_traits"] + profile["expert_traits"]["add"]]
                self.assertNotIn("discreet", kinds)
                self.assertNotIn("discreet", profile["expert_traits"]["remove"])


    def test_standalone_darts_and_shuriken_have_distinct_payloads_and_no_live_stock(self):
        items = source("catalog/items.toml")["items"]
        expected = {
            "dart": ({"category": "keen"}, {"type": "fixed", "value": 1}, 1, 3),
            "shuriken": ({"category": "keen", "sides": 2},
                         {"type": "rolled", "count": 1, "sides": 2, "bonus": 0}, 2, 4),
        }
        for name, (primary, amount, count, sides) in expected.items():
            identity = "rogue." + name
            with self.subTest(identity=identity):
                self.assertTrue(identity in items, f"Missing standalone inventory weapon {identity}")
                self.assertEqual(items[identity], {
                    "asset": "item.weapon", "name": name, "identity": identity,
                    "class": "weapon", "stackable": True,
                    "equipment_generation": {"kind": "weapon", "curse_percent": 10,
                                             "enchant_percent": 5, "minimum": 1, "maximum": 3},
                    "equipment": {"slot": "weapon", "modifiers": [], "attack": {
                        "skill": "light_weaponry", "bonus": 0, "wind_up": 60, "recovery": 40,
                        "damage": {"primary": primary, "components": [
                            {"category": "keen", "amount": amount}]}}, "weapon_profile": {
                        "expertise": "rogue.knife", "base_traits": [], "expert_traits": {
                            "add": [{"kind": "thrown", "range": {"short_feet": 20, "long_feet": 60}}],
                            "remove": []}, "thrown_payload": {"recovery": "consumed_on_hit",
                            "damage": {"primary": {"category": "keen", "sides": sides}, "components": [
                                {"category": "keen", "amount": {"type": "rolled", "count": count,
                                                                "sides": sides, "bonus": 0}}]}}}}})
                # These are real inventory weapons, never disguised ammunition.
                self.assertNotIn("projectile", items[identity])
                self.assertNotIn("launcher", items[identity]["equipment"]["weapon_profile"])
                for path in ("scenario.toml", "catalog/actors.toml"):
                    self.assertNotIn(identity, (PACKAGE / path).read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
