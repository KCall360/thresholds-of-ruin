"""Opt-in Room Light catalog contract, independent from live generation stock."""
from pathlib import Path
import tomllib
import unittest

from rogue_staves_package import expected_forms

PRODUCTION = Path(__file__).resolve().parents[1] / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((PRODUCTION / name).read_text(encoding="utf-8"))


class RogueLightContent(unittest.TestCase):
    def test_room_light_is_paid_self_device_with_on_use_discovery(self):
        self.assertEqual(source("catalog/items.toml")["items"]["rogue.wand_light"], {
            "asset": "item.wand", "name": "wand of light", "identity": "rogue.wand_light",
            "magical": True, "class": "wand", "stackable": False, "appearance_pool": "rogue.wands",
            "forms": expected_forms("light"),
            "device": {"ability": "rogue.wand_light", "initial_charges": {"minimum": 10, "maximum": 19}, "discovery": "on_use"}})
        definitions = [row["value"] for row in source("rules.toml")["overrides"]
                       if row["path"] == ["abilities", "rogue.wand_light"]]
        self.assertEqual(definitions, [{"id": "rogue.wand_light", "name": "Room Light",
            "interaction": "non_hostile", "tags": [], "basic_attack": False,
            "invocation": "actor_target", "timing": {"source": {"kind": "fixed", "prepare": 100, "recover": 100}},
            "reach": {"kind": "self_only"}, "strikes": {"kind": "authored", "strikes": [{
                "offset": 0, "on_success": [{"kind": "room_light"}], "on_failure": [], "descriptors": [], "riders": []}]},
            "ai": {"priority": 0, "hostile": False, "minimum_resource_percent": 0, "range_preference": "any"}}])

    def test_light_keeps_legacy_device_stock_and_starting_kits_unchanged(self):
        manifest = source("scenario.toml")
        stage = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][7]
        self.assertEqual(stage, {"id": "rogue.magic_devices", "version": 1, "kind": "items",
            "count": [1, 1], "quantity": 1, "bands": [{"depth": {"kind": "open", "minimum": 1},
                "choices": [{"archetype": "rogue.wand_" + kind, "weight": 1} for kind in
                    ("fire", "cold", "lightning", "invisibility", "haste", "slow", "teleport_away", "teleport_to")]}]})
        for character in manifest["characters"]:
            self.assertNotIn("rogue.wand_light", character["known_identities"])
            self.assertNotIn("rogue.wand_light", [row["archetype"] for row in character["loadout"]])
