"""Opt-in Nothing content uses shared paid self-device semantics, not live stock."""
from pathlib import Path
import tomllib
import unittest

PRODUCTION = Path(__file__).resolve().parents[1] / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((PRODUCTION / name).read_text(encoding="utf-8"))


class RogueNothingContent(unittest.TestCase):
    def test_opt_in_nothing_has_exact_paid_noop_definition(self):
        catalog = source("catalog/items.toml")
        self.assertEqual(catalog["items"]["rogue.wand_nothing"], {
            "asset": "item.wand", "name": "wand of nothing", "identity": "rogue.wand_nothing",
            "magical": True, "class": "wand", "stackable": False, "appearance_pool": "rogue.wands",
            "device": {"ability": "rogue.wand_nothing", "initial_charges": {"minimum": 3, "maximum": 7}, "discovery": "never"}})
        definitions = [row["value"] for row in source("rules.toml")["overrides"]
                       if row["path"] == ["abilities", "rogue.wand_nothing"]]
        self.assertEqual(definitions, [{"id": "rogue.wand_nothing", "name": "Nothing",
            "interaction": "non_hostile", "tags": [], "basic_attack": False,
            "invocation": "actor_target", "timing": {"source": {"kind": "fixed", "prepare": 100, "recover": 100}},
            "reach": {"kind": "self_only"}, "strikes": {"kind": "authored", "strikes": [{
                "offset": 0, "on_success": [], "on_failure": [], "descriptors": [], "riders": []}]},
            "ai": {"priority": 0, "hostile": False, "minimum_resource_percent": 0, "range_preference": "any"}}])

    def test_live_device_stock_remains_exactly_eight_original_choices(self):
        manifest = source("scenario.toml")
        stage = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][7]
        self.assertEqual(stage, {"id": "rogue.magic_devices", "version": 1, "kind": "items",
            "count": [1, 1], "quantity": 1, "bands": [{"depth": {"kind": "open", "minimum": 1},
                "choices": [{"archetype": "rogue.wand_" + kind, "weight": 1} for kind in
                    ("fire", "cold", "lightning", "invisibility", "haste", "slow", "teleport_away", "teleport_to")]}]})
        for character in manifest["characters"]:
            self.assertNotIn("rogue.wand_nothing", character["known_identities"])
            self.assertNotIn("rogue.wand_nothing", [row["archetype"] for row in character["loadout"]])

    def test_distinct_appearances_cover_opt_in_catalog_identities(self):
        catalog = source("catalog/items.toml")
        identities = {identity for identity, item in catalog["items"].items()
                      if item.get("appearance_pool") == "rogue.wands"}
        self.assertEqual(identities, {"rogue.wand_" + kind for kind in
            ("fire", "cold", "lightning", "invisibility", "haste", "slow", "teleport_away", "teleport_to", "nothing", "light")})
        appearances = catalog["appearance_pools"]["rogue.wands"]["appearances"]
        self.assertEqual(appearances, ["ash wand", "copper wand", "ivory wand", "obsidian wand",
                                     "oak wand", "glass wand", "willow wand", "iron wand", "elm wand", "birch wand"])
        self.assertEqual(len(appearances), 10)
        self.assertEqual(len(set(appearances)), len(identities))
