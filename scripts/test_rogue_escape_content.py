"""Independent authored contracts for ordinary escape and levitation items."""
from pathlib import Path
import tomllib
import unittest

PACKAGE = Path(__file__).resolve().parents[1] / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((PACKAGE / name).read_text(encoding="utf-8"))


class RogueEscapeContent(unittest.TestCase):
    def test_levitation_is_timed_extending_hover_with_completion_time_braking(self):
        catalog = source("catalog/items.toml")
        self.assertIn("rogue.levitation", catalog["items"])
        self.assertEqual(catalog["items"]["rogue.levitation"], {
            "asset": "item.potion", "name": "potion of levitation", "identity": "rogue.levitation",
            "magical": True, "class": "potion", "stackable": True, "appearance_pool": "rogue.potions",
            "consumable": {"discovery": "on_use", "effects": [
                {"kind": "apply_condition_for_turns", "condition": "rogue.levitation",
                 "duration": {"kind": "rolled", "dice": {"count": 1, "sides": 3, "bonus": 28}}},
                {"kind": "arrest_descent"}]}})
        rows = source("rules.toml")["overrides"]
        condition = next(row["value"] for row in rows if row["path"] == ["conditions", "rogue.levitation"])
        self.assertEqual(condition, {
            "id": "rogue.levitation", "family": "rogue.levitation", "name": "Levitating", "lifecycle": "timed",
            "modifiers": [{"scope": "always", "op": {"kind": "locomotion", "value": {"kind": "levitating", "enabled": True}}}],
            "stacking": {"kind": "extend"}, "source_scoped": False, "descriptors": [], "capacity": "reject",
            "timing": {"kind": "actor_active", "on_expiry": {"kind": "remove"}}, "restrictions": [], "preparation": "preserve"})

    def test_teleport_sources_have_explicit_payment_discovery_and_independent_cadence(self):
        items = source("catalog/items.toml")["items"]
        self.assertIn("rogue.teleportation", items)
        self.assertEqual(items["rogue.teleportation"]["readable"], {
            "effects": [{"kind": "teleport_random"}], "discovery": "self_effect"})
        self.assertEqual(items["rogue.ring_teleportation"]["equipment_generation"], {"kind": "cursed"})
        self.assertEqual(items["rogue.ring_teleportation"]["equipment"], {
            "slot": "ring", "modifiers": [], "potency_modifiers": [],
            "periodic_relocation": {"interval": 100, "chance_denominator": 50}})
        abilities = {row["value"]["id"]: row["value"] for row in source("rules.toml")["overrides"]
                     if row["path"][0] == "abilities" and len(row["path"]) == 2}
        for suffix, effect in (("away", "teleport_random"), ("to", "teleport_to_ray_origin")):
            with self.subTest(device=suffix):
                identity = "rogue.wand_teleport_" + suffix
                self.assertEqual(items[identity]["device"], {
                    "ability": identity, "initial_charges": {"minimum": 3, "maximum": 7}, "discovery": "never"})
                ability = abilities[identity]
                self.assertEqual(ability["interaction"], "hostile_to_target")
                self.assertEqual(ability["invocation"], "directional_contact_ray")
                self.assertEqual(ability["timing"], {"source": {"kind": "fixed", "prepare": 100, "recover": 100}})
                self.assertEqual(ability["reach"], {"kind": "sight", "range": 6})
                self.assertNotIn("cost", ability)
                self.assertEqual(ability["strikes"], {"kind": "authored", "strikes": [{
                    "offset": 0, "check": {"actor": "target", "skill": {"kind": "named", "skill": "agility"},
                    "against": {"kind": "opposed", "skill": {"kind": "named", "skill": "lore"}}},
                    "on_success": [], "on_failure": [{"kind": effect}], "descriptors": [], "riders": []}]})

    def test_new_choices_are_finite_concealed_stock_not_starter_or_replenishment(self):
        catalog, manifest = source("catalog/items.toml"), source("scenario.toml")
        expected = {"rogue.levitation": ("rogue.magic_potions", 1), "rogue.teleportation": ("rogue.magic_scrolls", 1),
                    "rogue.wand_teleport_away": ("rogue.magic_devices", 1), "rogue.wand_teleport_to": ("rogue.magic_devices", 1),
                    "rogue.ring_teleportation": ("rogue.rings", 5)}
        stages = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
        for identity, (stage_id, weight) in expected.items():
            with self.subTest(item=identity):
                self.assertIn(identity, catalog["items"])
                item = catalog["items"][identity]
                self.assertTrue(item["magical"])
                self.assertNotIn("appearance", item)
                stage = next(stage for stage in stages if stage["id"] == stage_id)
                self.assertEqual(stage["count"], [1, 1])
                self.assertEqual(stage["quantity"], 1)
                self.assertIn({"archetype": identity, "weight": weight}, stage["bands"][0]["choices"])
                for character in manifest["characters"]:
                    self.assertNotIn(identity, character["known_identities"])
                    self.assertNotIn(identity, [row["archetype"] for row in character["loadout"]])
        for pool in ("rogue.potions", "rogue.scrolls", "rogue.wands", "rogue.rings"):
            appearances = catalog["appearance_pools"][pool]["appearances"]
            count = sum(item.get("appearance_pool") == pool for item in catalog["items"].values())
            self.assertEqual(len(appearances), count)
            self.assertEqual(len(set(appearances)), count)


if __name__ == "__main__":
    unittest.main()
