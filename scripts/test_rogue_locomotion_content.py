"""Approved movement/grasp roster source and exact population boundary contracts."""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((PACKAGE / name).read_text(encoding="utf-8"))


class RogueLocomotionContent(unittest.TestCase):
    def test_new_roster_assets_use_valid_disclosed_identifiers(self):
        actors = source("catalog/actors.toml")["actors"]
        assets = source("scenario.toml")["assets"]["dungeon"]
        for name, expected in (
                ("bat", "creature.bat"), ("kestrel", "creature.bird.kestrel"),
                ("venus_flytrap", "creature.plant.venus.flytrap"), ("griffin", "creature.griffin")):
            asset = actors["rogue." + name]["asset"]
            self.assertLessEqual(len(asset), 80)
            self.assertRegex(asset, r"\A[a-z0-9-]+(?:\.[a-z0-9-]+)*\Z")
            self.assertEqual(asset, expected)
            self.assertIn(asset, assets)

    def test_flying_roster_owns_exact_racial_rows_and_effective_movement(self):
        actors = source("catalog/actors.toml")["actors"]
        species = source("catalog/creatures.toml")["creatures"]["species"]
        for name, hd, kind, disposition, dice in (
                ("bat", 1, "animal", "passive", [(1, 2)]),
                ("kestrel", 1, "animal", "mean", [(1, 4)]),
                ("griffin", 13, "magical_beast", "mean", [(4, 3), (3, 5)])):
            identity = "rogue." + name
            self.assertIn(identity, actors, "Approved flying actor recipe is missing")
            actor, creature = actors[identity], species[identity]
            self.assertEqual(creature["kind"], kind)
            self.assertEqual(creature["behavior"]["disposition"], disposition)
            self.assertEqual(len(actor["creature"]["hit_dice"]), hd)
            self.assertTrue(all(row["source"] == "racial" for row in actor["creature"]["hit_dice"]))
            ops = [row["op"] for row in creature["modifiers"]]
            self.assertIn({"kind": "locomotion", "value": {"kind": "flying", "enabled": True}}, ops)
            actual = [strike["on_success"][0]["damage"]["components"][0]["amount"]["dice"]
                      for strike in creature["melee"]["strikes"]]
            self.assertEqual(actual, [{"count": count, "sides": sides, "bonus": 0} for count, sides in dice])
            purchases = {index + 1: row["purchases"] for index, row in enumerate(actor["creature"]["hit_dice"])
                         if row["purchases"]}
            expected = {1: [{"pool": "advancement", "selection": {"kind": "talent", "talent": "rogue.regeneration"}}],
                        2: [{"pool": "advancement", "selection": {"kind": "talent", "talent": "second_strike"}}]} if name == "griffin" else {}
            self.assertEqual(purchases, expected)
            erratic = [op["value"] for op in ops if op["kind"] == "locomotion" and op["value"]["kind"] == "erratic"]
            self.assertEqual(erratic, [{"kind": "erratic", "chance": {"numerator": 1, "denominator": 2}}] if name == "bat" else [])

    def test_flytrap_is_stationary_plant_with_exact_owned_grasp_rows(self):
        actors = source("catalog/actors.toml")["actors"]
        self.assertIn("rogue.venus_flytrap", actors, "Approved stationary actor recipe is missing")
        actor = actors["rogue.venus_flytrap"]["creature"]
        creature = source("catalog/creatures.toml")["creatures"]["species"]["rogue.venus_flytrap"]
        self.assertEqual(creature["kind"], "plant")
        self.assertEqual(creature["behavior"]["disposition"], "mean")
        self.assertIn({"scope": "always", "op": {"kind": "locomotion", "value": {"kind": "stationary", "enabled": True}}}, creature["modifiers"])
        self.assertEqual(len(actor["hit_dice"]), 8)
        self.assertTrue(all(row["source"] == "racial" for row in actor["hit_dice"]))
        self.assertEqual([index + 1 for index, row in enumerate(actor["hit_dice"]) if row["purchases"]], [1, 6])
        rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text(encoding="utf-8"))
        gr2 = next(talent for talent in rules["talents"] if talent["id"] == "tightening_hold")
        self.assertEqual(gr2["modifiers"], [{"scope": "always", "op": {
            "kind": "add", "stat": {"kind": "ability_tag", "tag": "attack",
                                    "stat": "hold_damage_increment"}, "value": 1}}])
        for name in ("clutch", "basic_melee"):
            ability = next(ability for ability in rules["abilities"] if ability["id"] == name)
            self.assertIn("attack", ability["tags"])
            self.assertEqual(ability["strikes"]["kind"],
                             "natural_melee" if name == "clutch" else "selected_melee")

    def test_new_population_boundaries_preserve_stage_pool_ids_and_stock(self):
        manifest = source("scenario.toml")
        activation = manifest["floor_activation"]
        stages = activation["profiles"]["rogue.low_depth"]["stages"]
        self.assertEqual([stage["id"] for stage in stages], ["rogue.eight_species_foes", "rogue.bandages", "rogue.rations",
            "rogue.restore_strength", "rogue.room_gold", "rogue.magic_potions", "rogue.magic_scrolls", "rogue.magic_devices"])
        self.assertEqual(stages[0]["count"], [4, 4])
        self.assertEqual(stages[0]["depth_growth"], {"kind": "racial_after", "depth": 26})
        bands = stages[0]["bands"]
        for name, first, last in (("kestrel", 1, 6), ("bat", 1, 8), ("venus_flytrap", 12, 21), ("griffin", 20, None)):
            for depth in (1, first - 1, first, last or 31, (last + 1) if last else 267, 268, 275):
                if depth < 1:
                    continue
                band = next(band for band in bands if band["depth"]["minimum"] <= depth
                            and (band["depth"]["kind"] == "open" or depth <= band["depth"]["maximum"]))
                choices = {row["archetype"]: row for row in band["choices"]}
                eligible = first <= depth and (last is None or depth <= last)
                self.assertEqual("rogue." + name in choices, eligible, (name, depth))
                if eligible:
                    stock = choices["rogue." + name].get("carried_loot")
                    self.assertEqual(stock, {"version": 1, "pool": "rogue.vampire_supplies", "chance": {"numerator": 1, "denominator": 5}} if name == "griffin" else None)
        self.assertEqual({row["archetype"] for row in bands[-1]["choices"]}, {"rogue.vampire", "rogue.griffin", "rogue.jabberwock", "rogue.medusa", "rogue.dragon"})


if __name__ == "__main__":
    unittest.main()
