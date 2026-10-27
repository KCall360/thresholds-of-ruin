"""Opt-in first-body devices preserve original profiles and unchanged live stock."""
from pathlib import Path
import tomllib
import unittest

from rogue_staves_package import expected_forms

PRODUCTION = Path(__file__).resolve().parents[1] / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((PRODUCTION / name).read_text(encoding="utf-8"))


def expected_item(kind, name, discovery):
    identity = "rogue.wand_" + kind
    return {"asset": "item.wand", "name": "wand of " + name, "identity": identity,
            "magical": True, "class": "wand", "stackable": False,
            "appearance_pool": "rogue.wands", "forms": expected_forms(name), "device": {"ability": identity,
            "initial_charges": {"minimum": 3, "maximum": 7}, "discovery": discovery}}


def expected_ability(kind, name, tags, strike):
    return {"id": "rogue.wand_" + kind, "name": name, "interaction": "hostile_to_target",
            "tags": tags, "basic_attack": False, "invocation": "directional_contact_ray",
            "timing": {"source": {"kind": "fixed", "prepare": 100, "recover": 100}},
            "reach": {"kind": "until_obstruction"},
            "strikes": {"kind": "authored", "strikes": [strike]},
            "ai": {"priority": 0, "hostile": True, "minimum_resource_percent": 0, "range_preference": "any"}}


class RogueFirstContactContent(unittest.TestCase):
    def assert_ability(self, kind, expected):
        definitions = [row["value"] for row in source("rules.toml")["overrides"]
                       if row["path"] == ["abilities", "rogue.wand_" + kind]]
        self.assertEqual(definitions, [expected])

    def test_cancellation_is_checkless_first_body_never_identifying(self):
        self.assertEqual(source("catalog/items.toml")["items"]["rogue.wand_cancellation"],
                         expected_item("cancellation", "cancellation", "never"))
        self.assert_ability("cancellation", expected_ability("cancellation", "Cancellation", [], {
            "offset": 0, "on_success": [{"kind": "cancel_specials"}],
            "on_failure": [], "descriptors": [], "riders": []}))

    def test_missile_is_one_lore_agility_save_with_one_d4_plus_one_on_failure(self):
        self.assertEqual(source("catalog/items.toml")["items"]["rogue.wand_magic_missile"],
                         expected_item("magic_missile", "magic missile", "on_use"))
        self.assert_ability("magic_missile", expected_ability("magic_missile", "Magic Missile", ["attack"], {
            "offset": 0,
            "check": {"actor": "target", "skill": {"kind": "named", "skill": "agility"},
                      "against": {"kind": "opposed", "skill": {"kind": "named", "skill": "lore"}}},
            "on_success": [], "on_failure": [{"kind": "damage", "damage": {"kind": "components",
                "components": [{"category": "energy", "amount": {"kind": "rolled", "dice": {
                    "count": 1, "sides": 4, "bonus": 1}}}]}}], "descriptors": [], "riders": []}))

    def test_first_contact_catalog_does_not_change_live_eight_or_starting_kits(self):
        manifest = source("scenario.toml")
        stage = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][7]
        self.assertEqual(stage, {"id": "rogue.magic_devices", "version": 1, "kind": "items",
            "count": [1, 1], "quantity": 1, "bands": [{"depth": {"kind": "open", "minimum": 1},
                "choices": [{"archetype": "rogue.wand_" + kind, "weight": 1} for kind in
                    ("fire", "cold", "lightning", "invisibility", "haste", "slow", "teleport_away", "teleport_to")]}]})
        for character in manifest["characters"]:
            for identity in ("rogue.wand_cancellation", "rogue.wand_magic_missile"):
                self.assertNotIn(identity, character["known_identities"])
                self.assertNotIn(identity, [row["archetype"] for row in character["loadout"]])

    def test_cancellation_metadata_marks_only_reviewed_special_sources(self):
        rules = source("rules.toml")
        overrides = {tuple(row["path"]): row for row in rules["overrides"]}
        specials = {"rogue." + name for name in ("freezing_touch", "drain_vitality", "venomous_bite",
                    "energy_drain", "confusing_gaze", "corrode", "breath_weapon")}
        marked_abilities = {path[1] for path, row in overrides.items()
                            if len(path) == 2 and path[0] == "abilities"
                            and row["value"].get("cancellable", False)}
        self.assertEqual(marked_abilities, specials)
        nested = {path: row for path, row in overrides.items() if path[-1] == "cancellable"}
        self.assertEqual(nested, {("abilities", name, "cancellable"):
            {"op": "add", "path": ["abilities", name, "cancellable"], "value": True}
            for name in ("filch_gold", "filch_charm")})
        # Resolver starts raw shared TOML; add is valid because these keys are absent.
        shared = tomllib.loads((PRODUCTION.parents[1] / "crates/simulation/data/default-ruleset.toml").read_text())
        for ability in shared["abilities"]:
            self.assertFalse(ability.get("cancellable", False), "shared global rules remain unchanged")
        vanish = next(row for row in shared["talents"] if row["id"] == "vanish")
        self.assertEqual(vanish["modifiers"], [{"scope": "always", "op": {"kind": "flag", "flag": "theft_vanish"}}])

        def marked(value, path=()):
            result = set()
            if isinstance(value, dict):
                if value.get("cancellable", False):
                    result.add(path)
                for key, child in value.items():
                    result.update(marked(child, path + (key,)))
            elif isinstance(value, list):
                for index, child in enumerate(value):
                    result.update(marked(child, path + (index,)))
            return result
        species = source("catalog/creatures.toml")["creatures"]["species"]
        self.assertEqual(marked(species), {
            ("rogue.phantom", "modifiers", 0),
            ("rogue.venus_flytrap", "modifiers", 1),
            ("rogue.venus_flytrap", "melee", "strikes", 0),
        }, "ordinary stats/defenses/flight/erratic/regen/natural strikes must not be marked")
        talents = {path[1]: row["value"] for path, row in overrides.items()
                   if len(path) == 2 and path[0] == "talents"}
        self.assertEqual(marked(talents), {("rogue.object_guise", "modifiers", 0),
            ("rogue.object_guise", "modifiers", 1), ("rogue.ambush_from_guise", "modifiers", 0)})
        phantom = species["rogue.phantom"]
        self.assertEqual(phantom["modifiers"][1], {"scope": "always", "op": {
            "kind": "locomotion", "value": {"kind": "erratic", "chance": {"numerator": 1, "denominator": 5}}}})
        self.assertEqual(species["rogue.venus_flytrap"]["modifiers"][0], {"scope": "always", "op": {
            "kind": "locomotion", "value": {"kind": "stationary", "enabled": True}}})
