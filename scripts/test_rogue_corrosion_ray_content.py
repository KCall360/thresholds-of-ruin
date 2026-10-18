"""A/D source contracts; mechanics have separate runtime owners."""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]


class RogueCorrosionRayContent(unittest.TestCase):
    def test_ring_mail_is_rustable_physical_armor_not_a_defense_bonus(self):
        for relative in ("scenarios/rogue-exploration/catalog/items.toml",
                         "crates/client-3d/scenarios/menagerie/catalog/items.toml"):
            with self.subTest(package=relative):
                items = tomllib.loads((ROOT / relative).read_text(encoding="utf-8"))["items"]
                equipment = items["rogue.ring_mail"]["equipment"]
                self.assertEqual(equipment.get("armor"), {
                    "physical_reduction": 2, "material": "metal", "rustproof": False})
                self.assertFalse(any(modifier["op"].get("stat", {}).get("kind") == "defense"
                                     for modifier in equipment.get("modifiers", [])))

    def test_aquator_owns_one_zero_damage_contact_and_co1_at_hd1(self):
        package = ROOT / "scenarios/rogue-exploration"
        actors = tomllib.loads((package / "catalog/actors.toml").read_text())["actors"]
        species = tomllib.loads((package / "catalog/creatures.toml").read_text())["creatures"]["species"]
        self.assertIn("rogue.aquator", actors)
        actor, definition = actors["rogue.aquator"], species["rogue.aquator"]
        self.assertEqual(actor["creature"]["hit_dice"], [
            {"source":"racial", "purchases":[{"pool":"advancement", "selection":{
                "kind":"talent", "talent":"rogue.corrode"}}]},
            *[{"source":"racial", "purchases":[]} for _ in range(4)]])
        self.assertEqual((definition["kind"], definition["subtypes"]), ("aberration", ["aquatic"]))
        self.assertEqual(definition["behavior"]["disposition"], "mean")
        self.assertEqual(len(definition["melee"]["strikes"]), 1)
        strike = definition["melee"]["strikes"][0]
        self.assertNotIn("unlock", strike)
        self.assertEqual(strike["on_success"][0]["damage"]["components"][0]["amount"],
                         {"kind":"fixed", "amount":0})
        self.assertNotIn("loadout", actor)
        overrides = tomllib.loads((package / "rules.toml").read_text())["overrides"]
        ability = next(row["value"] for row in overrides if row["path"] == ["abilities","rogue.corrode"])
        self.assertEqual(ability["strikes"]["additional_riders"], [{
            "trigger":"success", "condition":{"kind":"sequence_successful_hits", "minimum":1},
            "effects":[{"kind":"corrode_armor", "slot":"body_armor", "amount":1}], "descriptors":[]}])


    def test_dragon_owns_budgeted_willpower_breath_and_energy_fire_ray(self):
        package = ROOT / "scenarios/rogue-exploration"
        actors = tomllib.loads((package / "catalog/actors.toml").read_text())["actors"]
        species = tomllib.loads((package / "catalog/creatures.toml").read_text())["creatures"]["species"]
        self.assertIn("rogue.dragon", actors)
        creature = actors["rogue.dragon"]["creature"]
        definition = species["rogue.dragon"]
        self.assertEqual((definition["kind"], definition["subtypes"]), ("dragon", ["fire"]))
        self.assertEqual(definition["attributes"]["willpower"], 1)
        self.assertEqual(definition["behavior"]["disposition"], "mean")
        self.assertEqual(creature["starting_skills"], ["heavy_weaponry", "breath"])
        rows = creature["hit_dice"]
        self.assertEqual(len(rows), 10)
        self.assertEqual([row.get("attribute") for row in rows],
                         [None, None, None, "willpower", None, None, None, "willpower", None, None])
        expected = {2: {"kind":"talent", "talent":"second_strike"},
                    4: {"kind":"talent", "talent":"third_strike"},
                    6: {"kind":"talent", "talent":"rogue.breath_weapon"},
                    8: {"kind":"skill_rank", "skill":"breath"}}
        for ordinal, row in enumerate(rows, 1):
            self.assertEqual(row["source"], "racial")
            self.assertEqual(row["purchases"], [{"pool":"advancement", "selection":expected[ordinal]}]
                             if ordinal in expected else [])
        strikes = definition["melee"]["strikes"]
        self.assertEqual([row.get("unlock") for row in strikes], [None, "second_strike", "third_strike"])
        self.assertEqual([row["on_success"][0]["damage"]["components"][0]["amount"]["dice"]
                          for row in strikes], [{"count":1,"sides":8,"bonus":0},
                                               {"count":1,"sides":8,"bonus":0},
                                               {"count":3,"sides":10,"bonus":0}])
        overrides = tomllib.loads((package / "rules.toml").read_text())["overrides"]
        ray = next(row["value"] for row in overrides if row["path"] == ["abilities","rogue.breath_weapon"])
        self.assertEqual(ray["invocation"], "directional_ray")
        self.assertEqual(ray["reach"], {"kind":"sight", "range":6})
        self.assertEqual(ray["timing"], {"source":{"kind":"fixed", "prepare":100, "recover":100}})
        self.assertEqual(ray["cooldown"], 2000)
        strike, = ray["strikes"]["strikes"]
        self.assertEqual(strike["check"], {"actor":"target", "skill":{"kind":"named", "skill":"agility"},
                         "against":{"kind":"opposed", "skill":{"kind":"named", "skill":"breath"}}})
        self.assertEqual(strike["on_success"], [])
        self.assertEqual(strike["descriptors"], [])
        self.assertEqual(strike["on_failure"], [{"kind":"damage", "damage":{"kind":"components", "components":[{
            "category":"energy", "descriptor":"fire", "amount":{"kind":"rolled", "dice":{"count":6,"sides":6,"bonus":0}}}]}}])


if __name__ == "__main__":
    unittest.main()
