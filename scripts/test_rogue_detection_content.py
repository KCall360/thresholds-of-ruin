"""Opt-in detection contracts: source-owned labels, closed effects and no stock changes."""
from pathlib import Path
import tomllib
import unittest

PRODUCTION = Path(__file__).resolve().parents[1] / "scenarios/rogue-exploration"
KINDS = ("monster_detection", "food_detection", "magic_detection")
OLD_LABELS = ["amber potion", "smoky potion", "violet potion", "azure potion", "crimson potion",
              "silver potion", "pearl potion", "teal potion", "ochre potion", "indigo potion",
              "copper potion", "chartreuse potion"]


def source(name):
    return tomllib.loads((PRODUCTION / name).read_text(encoding="utf-8"))


class RogueDetectionContent(unittest.TestCase):
    def test_three_opt_in_doses_are_sole_self_effect_chart_operations(self):
        items = source("catalog/items.toml")["items"]
        for kind, effect in zip(KINDS, (
                {"kind": "detect_monsters", "condition": "rogue.detecting_monsters"},
                {"kind": "detect_food"}, {"kind": "detect_magic"}), strict=True):
            with self.subTest(kind=kind):
                self.assertEqual(items.get("rogue." + kind), {
                    "name": "potion of " + kind.replace("_", " "), "identity": "rogue." + kind,
                    "asset": "item.potion", "magical": True, "class": "potion", "stackable": True,
                    "appearance_pool": "rogue.potions",
                    "consumable": {"discovery": "self_effect", "effects": [effect]}})

    def test_single_marker_is_pure_target_wide_refresh_without_authored_duration(self):
        conditions = {row["path"][1]: row["value"] for row in source("rules.toml")["overrides"]
                      if row["path"][0] == "conditions" and len(row["path"]) == 2}
        self.assertEqual({key for key, value in conditions.items() if value.get("detecting_monsters")},
                         {"rogue.detecting_monsters"})
        self.assertEqual(conditions.get("rogue.detecting_monsters"), {
            "id": "rogue.detecting_monsters", "family": "rogue.detecting_monsters",
            "lifecycle": "timed", "name": "Monster detection", "detecting_monsters": True,
            "modifiers": [], "stacking": {"kind": "refresh"}, "source_scoped": False,
            "descriptors": [], "capacity": "reject",
            "timing": {"kind": "actor_active", "on_expiry": {"kind": "remove"}},
            "restrictions": [], "preparation": "preserve"})

    def test_species_labels_are_explicit_source_owned_tuples_not_character_names(self):
        species = source("catalog/creatures.toml")["creatures"]["species"]
        actors = source("catalog/actors.toml")["actors"]
        expected = {actor["creature"]["species"]: {"name": actor["creature"]["name"], "asset": actor["asset"]}
                    for actor in actors.values()}
        expected["rogue.human"] = {"name": "human", "asset": "creature.humanoid.human"}
        self.assertEqual(len(expected), 27)
        self.assertEqual(set(species), set(expected))
        self.assertEqual({key: value.get("detection_appearance") for key, value in species.items()}, expected)
        self.assertFalse({"Warrior", "Mage"} & {value["name"] for value in expected.values()})

    def test_three_new_labels_preserve_all_old_appearances_and_exact_identity_capacity(self):
        catalog = source("catalog/items.toml")
        labels = catalog["appearance_pools"]["rogue.potions"]["appearances"]
        self.assertEqual(labels, OLD_LABELS + ["saffron potion", "cerulean potion", "magenta potion"])
        identities = [name for name, item in catalog["items"].items() if item.get("appearance_pool") == "rogue.potions"]
        self.assertEqual(len(identities), 15)
        self.assertEqual(len(labels), len(set(labels)))

    def test_generated_stock_and_starter_kits_do_not_enable_opt_in_detection(self):
        manifest = source("scenario.toml")
        stage = next(row for row in manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
                     if row["id"] == "rogue.magic_potions")
        self.assertEqual(stage["bands"], [{"depth": {"kind": "open", "minimum": 1}, "choices": [
            {"archetype": "rogue." + kind, "weight": 1} for kind in
            ("healing", "confusion", "blindness", "poison", "see_invisible", "extra_healing", "levitation")]}])
        for kind in KINDS:
            self.assertNotIn("rogue." + kind, repr(manifest["floor_activation"]))
            for character in manifest["characters"]:
                self.assertNotIn("rogue." + kind, character["known_identities"])
                self.assertNotIn("rogue." + kind, [row["archetype"] for row in character["loadout"]])


if __name__ == "__main__":
    unittest.main()
