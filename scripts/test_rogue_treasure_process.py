"""Shared selected-room births through the real compiler, server and clients."""
import copy
import json
import sqlite3
import tomllib
import unittest

from checkpoint_payload import authoritative_floor_activation, decode_checkpoint_payload
from fruit_catalog_package import remove_sources
from process_harness import ProcessTestCase, SPECTATOR_TOKEN, underfoot
from rogue_treasure_package import PRODUCTION, fixture, prepare
from status_condition_package import checkpoint, observation


class RogueTreasureFixtureTests(unittest.TestCase):
    def test_cluster_fixture_keeps_source_catalogs_character_and_explicit_independent_counts(self):
        original = tomllib.loads((PRODUCTION / "scenario.toml").read_text(encoding="utf-8"))
        manifest = fixture()
        for key in ("format", "version", "ruleset", "modules", "themes", "assets"):
            self.assertEqual(manifest[key], original[key])
        self.assertEqual(manifest["factions"], {name: [] for name in original["factions"]})
        profiles = copy.deepcopy(original["ai_profiles"])
        profiles["acceptance_wait"] = {
            "idle": "wait", "doors": False, "care": False, "equipment": False,
            "loot": False, "gold": False, "flee_percent": 0}
        self.assertEqual(manifest["ai_profiles"], profiles)
        selected = copy.deepcopy(original["characters"][0])
        selected["anchor"] = "dungeon/1/down"
        self.assertEqual(manifest["characters"], [selected])
        stage, = manifest["floor_activation"]["profiles"]["treasure"]["stages"]
        self.assertEqual((stage["item_count"], stage["actor_count"], stage["actor_depth_offset"]),
                         ([2, 2], [1, 1], 1))
        self.assertEqual(stage["chance"], {"numerator": 1, "denominator": 1})
        authored = tomllib.loads((PRODUCTION / "examples/treasure-stage.toml").read_text(encoding="utf-8"))
        expected, = authored["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
        expected.update(chance={"numerator": 1, "denominator": 1}, item_count=[2, 2],
                        actor_count=[1, 1], quantity=2, ai="acceptance_wait")
        self.assertEqual(stage, expected)
        self.assertNotIn("wandering", manifest)
        self.assertNotIn("archetypes", manifest)


class RogueTreasureProcesses(ProcessTestCase):
    def activation(self, player):
        checkpoint(self, player)
        with sqlite3.connect(self.save.resolve().as_uri() + "?mode=ro", uri=True) as database:
            payload, = database.execute("SELECT payload FROM checkpoint").fetchone()
        source = json.loads(decode_checkpoint_payload(payload))
        return authoritative_floor_activation(source["game"]["floor_activation"], source["shared"])

    def advance(self, player, action):
        result = self.act(player, action)
        self.assertIsNone(result["error"])
        def settled(frame):
            return observation(frame)["ready"] and not frame["intentions"]
        return result if settled(result) else self.frame(player, settled)

    def test_generated_cluster_shares_one_room_uses_next_depth_and_reopens_without_authoring_sources(self):
        package = prepare(self)
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=42)
        player, initial = self.client()
        receipts = self.private_receipts()
        self.assertEqual(len(receipts), 1)
        receipt, = receipts
        self.assertEqual((receipt["scope"], receipt["entrant"], receipt["cause"]),
                         ("dungeon/1", 1, "initial"))
        decision, = receipt["decisions"]
        self.assertEqual(decision["result"]["kind"], "room_cluster")
        selected = decision["result"]["selection"]
        self.assertEqual((selected["kind"], selected["room"], selected["actor_depth"]),
                         ("selected", 1, 2))
        self.assertEqual((selected["item_requested"], selected["actor_requested"]), (2, 1))
        self.assertEqual(len(selected["items"]), 2)
        self.assertEqual(len(selected["actors"]), 1)
        for birth in selected["items"]:
            self.assertIn(birth["content"], ("rogue.gold", "rogue.healing", "rogue.fruit", "rogue.dart", "rogue.shuriken"))
            self.assertEqual((birth["kind"], birth["entity"]["kind"], birth["entity"]["quantity"]),
                             ("placed", "item", 2))
        actor, = selected["actors"]
        self.assertEqual((actor["kind"], actor["content"], actor["entity"]["kind"]),
                         ("placed", "rogue.hobgoblin", "actor"))
        # One ordinary admitted wait produces a real interval-one checkpoint;
        # neutral AI births wait without autonomous combat or player authority.
        self.advance(player, {"type": "wait"})
        saved = checkpoint(self, player)
        activation = self.activation(player)
        scope = activation["scopes"]["dungeon/1"]
        self.assertEqual(scope["depth"], 1, "effective monster birth depth cannot change floor identity")
        room, = scope["rooms"]
        self.assertEqual(selected["room_area"], len(room["cells"]))
        for birth in [*selected["items"], actor]:
            self.assertIn(birth["location"], room["cells"])
            self.assertNotIn(birth["location"], scope["protected"])
        monster = saved["actors"][str(actor["entity"]["id"])]["combat"]["creature"]
        self.assertEqual(monster["build"]["species"], "rogue.hobgoblin")
        observer, watched = self.client(token=SPECTATOR_TOKEN, observe=True)
        public = json.dumps(watched)
        self.assertNotIn('"room_cluster"', public)
        self.assertNotIn('"actor_depth"', public)
        self.assertEqual(self.private_receipts(), receipts, "queries/spectators cannot activate or reroll")
        final = self.request(player, {"type": "snapshot"})
        saved = checkpoint(self, player)
        activation = copy.deepcopy(self.activation(player))
        player.stop(); observer.stop()
        server.stop()
        remove_sources(self, package)
        self.server("--checkpoint-interval", 1, seed=None)
        player, reopened = self.client()
        self.assertEqual(reopened["state"], final["state"])
        self.assertEqual(checkpoint(self, player), saved)
        self.assertEqual(self.activation(player), activation)
        self.assertEqual(self.private_receipts(), receipts)
        for _ in range(3):
            self.assertEqual(self.request(player, {"type": "snapshot"})["state"], final["state"])
        self.assertEqual(self.private_receipts(), receipts)
        self.assertTrue(underfoot(reopened["state"])["stairs_down"])
        entered = self.advance(player, {"type": "move", "direction": "down"})
        self.assertTrue(underfoot(entered["state"])["stairs_up"])
        second = self.private_receipts()
        self.assertEqual([entry["scope"] for entry in second], ["dungeon/1", "dungeon/2"])
        self.assertEqual(second[0], receipt)
        next_cluster = second[1]["decisions"][0]["result"]["selection"]
        self.assertEqual(next_cluster["actor_depth"], 3)
        self.assertEqual(next_cluster["actors"][0]["content"], "rogue.hobgoblin")
        self.advance(player, {"type": "move", "direction": "up"})
        self.advance(player, {"type": "move", "direction": "down"})
        self.assertEqual(self.private_receipts(), second, "revisits cannot refill the selected room")


if __name__ == "__main__":
    unittest.main()
