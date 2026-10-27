"""Opt-in Drain Life catalog contract, independent from live generation stock."""
from pathlib import Path
import tomllib
import unittest

PRODUCTION = Path(__file__).resolve().parents[1] / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((PRODUCTION / name).read_text(encoding="utf-8"))


class RogueDrainContent(unittest.TestCase):
    def test_drain_life_is_paid_self_device_with_never_discovery(self):
        self.assertEqual(source("catalog/items.toml")["items"]["rogue.wand_drain_life"], {
            "asset": "item.wand", "name": "wand of drain life", "identity": "rogue.wand_drain_life",
            "magical": True, "class": "wand", "stackable": False, "appearance_pool": "rogue.wands",
            "device": {"ability": "rogue.wand_drain_life", "initial_charges": {"minimum": 3, "maximum": 7}, "discovery": "never"}})
        definitions = [row["value"] for row in source("rules.toml")["overrides"]
                       if row["path"] == ["abilities", "rogue.wand_drain_life"]]
        self.assertEqual(definitions, [{"id": "rogue.wand_drain_life", "name": "Drain Life",
            "interaction": "non_hostile", "tags": [], "basic_attack": False,
            "invocation": "actor_target", "timing": {"source": {"kind": "fixed", "prepare": 100, "recover": 100}},
            "reach": {"kind": "self_only"}, "strikes": {"kind": "authored", "strikes": [{
                "offset": 0, "on_success": [{"kind": "drain_life"}], "on_failure": [], "descriptors": [], "riders": []}]},
            "ai": {"priority": 0, "hostile": False, "minimum_resource_percent": 0, "range_preference": "any"}}])

    def test_drain_is_not_added_to_live_stock_or_starting_kits(self):
        manifest = source("scenario.toml")
        stage = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][7]
        self.assertEqual(stage, {"id": "rogue.magic_devices", "version": 1, "kind": "items",
            "count": [1, 1], "quantity": 1, "bands": [{"depth": {"kind": "open", "minimum": 1},
                "choices": [{"archetype": "rogue.wand_" + kind, "weight": 1} for kind in
                    ("fire", "cold", "lightning", "invisibility", "haste", "slow", "teleport_away", "teleport_to")]}]})
        for character in manifest["characters"]:
            self.assertNotIn("rogue.wand_drain_life", character["known_identities"])
            self.assertNotIn("rogue.wand_drain_life", [row["archetype"] for row in character["loadout"]])
