"""Imported ordinary fruit through real Text/headless clients and source-free replay.

Shared nutrition tests own dice, debt, paused claims and native Eat selection;
this new boundary proves the production fruit identity, named commands, distinct
ration stack, ordinary capped food effect and durable exact-unit settlement.
"""
import copy
import tomllib
import unittest

from fruit_catalog_package import PRODUCTION, fixture, prepare, remove_sources
from process_harness import ProcessTestCase
from status_condition_package import checkpoint, observation


def inventory(frame, name):
    return next(row for row in observation(frame)["inventory"] if row["name"] == name)


class FruitCatalogFixtureTests(unittest.TestCase):
    def test_fixture_changes_only_geometry_and_finite_food_placement(self):
        source = tomllib.loads((PRODUCTION / "scenario.toml").read_text(encoding="utf-8"))
        manifest, region = fixture()
        for key in ("format", "version", "ruleset", "modules", "factions", "ai_profiles", "themes", "assets"):
            self.assertEqual(manifest[key], source[key])
        selected = copy.deepcopy(source["characters"][0])
        selected["anchor"] = "1/start"
        self.assertEqual(manifest["characters"], [selected])
        self.assertEqual(region["items"], [
            {"id": 30, "at": [1, 2, 0], "archetype": "rogue.fruit", "quantity": 2}])
        self.assertNotIn("actors", region)
        self.assertNotIn("floor_activation", manifest)
        self.assertNotIn("archetypes", manifest)


class RogueFruitProcesses(ProcessTestCase):
    def snapshot(self, client):
        frame = self.request(client, {"type": "snapshot"})
        self.assertIsNone(frame["error"])
        return frame

    def test_imported_fruit_named_pickup_eat_and_source_free_reopen(self):
        package = prepare(self)
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=42)
        probe, initial = self.client(observe=True)
        text, _ = self.text_client()
        fruit, = observation(initial)["ground_items"]
        self.assertEqual((fruit["item"]["name"], fruit["item"]["quantity"], fruit["item"]["class"]),
                         ("fruit", "2", "food"))
        self.assertEqual(inventory(initial, "food ration")["quantity"], "1")
        self.assertIn("You pick up ", text.command("take fruit"))
        carried = self.snapshot(probe)
        selected = inventory(carried, "fruit")
        ration = inventory(carried, "food ration")
        self.assertEqual(selected["quantity"], "2")
        self.assertEqual((selected["known_supply"] or {})["id"], "ration")
        self.assertNotEqual(selected["id"], ration["id"])
        self.assertEqual(ration["quantity"], "1")
        self.assertIn("fruit", text.command("inventory"))
        affordance = next(row for row in observation(carried)["interactions"]["inventory"]
                          if row["item"] == selected["id"])
        self.assertTrue(affordance["edible"])
        self.assertFalse(affordance["drinkable"])
        before = checkpoint(self, probe)
        self.assertIn("ItemStarted", text.command("eat fruit"))
        eaten = self.snapshot(probe)
        self.assertEqual(inventory(eaten, "fruit")["quantity"], "1")
        self.assertEqual(inventory(eaten, "food ration"), ration)
        nutrition = observation(eaten)["combat"]["own_stats"]["nutrition"]
        self.assertEqual((nutrition["reserve"], nutrition["maximum"]), (2000, 2000))
        self.assertIn("Nutrition: 2000/2000", text.command("stats"))
        self.assertGreater(int(observation(eaten)["tick"]), int(observation(carried)["tick"]))
        self.assertEqual(observation(eaten)["combat"]["own_stats"]["conditions"],
                         observation(carried)["combat"]["own_stats"]["conditions"])
        saved = checkpoint(self, probe)
        self.assertNotEqual(saved["rules_rng"], before["rules_rng"])
        self.assertEqual(saved["items"]["30"]["quantity"], 1)
        self.assertEqual(saved["items"]["30"]["holds"], [])
        self.assertFalse(eaten["state"]["wizard_game"])
        text.stop(); probe.stop(); server.stop()
        remove_sources(self, package)
        server = self.server("--checkpoint-interval", 1, seed=None)
        probe, restored = self.client(observe=True)
        self.assertEqual(restored["state"], eaten["state"])
        self.assertEqual(checkpoint(self, probe), saved)
        text, _ = self.text_client()
        self.assertIn("fruit", text.command("inventory"))
        self.assertIn("ItemStarted", text.command("eat fruit"))
        depleted = self.snapshot(probe)
        self.assertFalse(any(row["name"] == "fruit" for row in observation(depleted)["inventory"]))
        self.assertEqual(inventory(depleted, "food ration"), ration)
        completed = checkpoint(self, probe)
        self.assertNotIn("30", completed["items"])
        text.stop(); probe.stop(); server.stop()
        self.server("--checkpoint-interval", 1, seed=None)
        probe, reopened = self.client(observe=True)
        self.assertEqual(reopened["state"], depleted["state"])
        self.assertEqual(checkpoint(self, probe), completed)
        for _ in range(3):
            self.assertEqual(self.snapshot(probe)["state"], depleted["state"])
        self.assertEqual(checkpoint(self, probe), completed)


if __name__ == "__main__":
    unittest.main()
