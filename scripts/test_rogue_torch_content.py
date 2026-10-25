"""Opt-in physical torch definition; no live stock or starter-kit expansion."""
from pathlib import Path
import tomllib
import unittest

PRODUCTION = Path(__file__).resolve().parents[1] / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((PRODUCTION / name).read_text(encoding="utf-8"))


class RogueTorchContent(unittest.TestCase):
    def test_torch_is_one_hand_nonmagical_finite_light_without_weapon_attack(self):
        self.assertEqual(source("catalog/items.toml")["items"]["rogue.torch"], {
            "name": "torch", "class": "tool", "stackable": False,
            "held_light": {"radius": 4, "fuel_ticks": 100000},
            "equipment": {"slot": "held_light", "modifiers": []}})

    def test_authored_tool_sockets_append_without_changing_old_indices_or_capacity(self):
        species = source("catalog/creatures.toml")["creatures"]["species"]
        expected = {"rogue.vampire", "rogue.wraith", "rogue.medusa"}
        actual = {name for name, row in species.items() if row.get("anatomy", {}).get("slots")}
        self.assertEqual(actual, expected)
        for name in expected:
            self.assertEqual(species[name]["anatomy"], {
                "slots": ["weapon", "shield", "body_armor", "head_armor", "hands_armor", "feet_armor",
                          "cloak", "ring", "ring", "amulet", "weapon", "held_light"],
                "capacities": {"grip": 2}, "primary_attack_socket": {"kind": "socket", "slot": 0}})
        for name, row in species.items():
            if "anatomy" in row and name not in expected:
                self.assertEqual(row["anatomy"]["slots"], [], name)

    def test_torch_is_not_added_to_live_stock_or_starting_kits(self):
        manifest = source("scenario.toml")
        self.assertNotIn("rogue.torch", repr(manifest["floor_activation"]))
        stage = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][7]
        self.assertEqual(stage, {"id": "rogue.magic_devices", "version": 1, "kind": "items",
            "count": [1, 1], "quantity": 1, "bands": [{"depth": {"kind": "open", "minimum": 1},
                "choices": [{"archetype": "rogue.wand_" + kind, "weight": 1} for kind in
                    ("fire", "cold", "lightning", "invisibility", "haste", "slow", "teleport_away", "teleport_to")]}]})
        for character in manifest["characters"]:
            self.assertNotIn("rogue.torch", character["known_identities"])
            self.assertNotIn("rogue.torch", [row["archetype"] for row in character["loadout"]])


if __name__ == "__main__":
    unittest.main()
