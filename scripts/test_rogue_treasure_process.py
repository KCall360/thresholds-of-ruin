"""Shared selected-room births through the real compiler, server and clients."""
import copy
import json
import sqlite3
import shutil
import subprocess
import tomllib
import unittest

from arena_matrix import toml_source
from checkpoint_payload import authoritative_floor_activation, decode_checkpoint_payload
from fruit_catalog_package import remove_sources
from process_harness import ProcessTestCase, SPECTATOR_TOKEN, underfoot
from rogue_treasure_package import PRODUCTION, fixture, prepare
from rogue_magic_receipts import assert_finite_item_identity, assert_live_content_receipts
from status_condition_package import checkpoint, observation


def manifest():
    return tomllib.loads((PRODUCTION / "scenario.toml").read_text(encoding="utf-8"))


def coverage_fixture():
    """Force each live choice and geometry boundary; keep catalog/rules untouched."""
    source = copy.deepcopy(manifest())
    source["id"] = "rogue-live-content-acceptance"
    source["characters"][0]["anchor"] = "dungeon/1/down"
    source["factions"] = {name: [] for name in source["factions"]}
    source["ai_profiles"]["acceptance_wait"] = {
        "idle": "wait", "doors": False, "care": False, "equipment": False,
        "loot": False, "gold": False, "flee_percent": 0}
    recipe = source["generation_recipes"]["exploration"]
    recipe["room_omissions"]["count"] = [1, 1]
    recipe["maze_rooms"]["one_in"] = 1
    recipe["lighting"] = {"darkness_roll": 1, "darkness_start": 0}
    stages = source["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
    stages[0]["ai"] = "acceptance_wait"
    supplemental = stages.pop(-2)
    treasure = stages[-1]
    treasure.update(chance={"numerator": 1, "denominator": 1}, ai="acceptance_wait")
    # Separate stable test streams make all live choices real ordinary births.
    # This verifies closure/placement, not their live occurrence probability.
    for ordinal, choice in enumerate(supplemental["bands"][0]["choices"]):
        stage = copy.deepcopy(supplemental)
        stage["id"] += f".{ordinal}"
        stage["count"] = [1, 1]
        stage["bands"][0]["choices"] = [copy.deepcopy(choice)]
        stages.insert(-1, stage)
    return source


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


    def test_forcing_changes_only_named_acceptance_boundaries(self):
        original = manifest()
        fixture = coverage_fixture()
        self.assertEqual(manifest(), original)
        for key in ("format", "version", "ruleset", "modules", "default_character", "themes",
                    "assets", "capabilities", "procedural_worlds"):
            self.assertEqual(fixture[key], original[key])
        characters = copy.deepcopy(fixture["characters"])
        characters[0]["anchor"] = original["characters"][0]["anchor"]
        self.assertEqual(characters, original["characters"])
        activation = fixture["floor_activation"]
        for key in ("version", "protagonist", "families", "wandering", "carried_item_pools"):
            self.assertEqual(activation[key], original["floor_activation"][key])
        old = original["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
        stages = activation["profiles"]["rogue.low_depth"]["stages"]
        first = copy.deepcopy(stages[:13])
        first[0]["ai"] = old[0]["ai"]
        self.assertEqual(first, old[:13])
        choices = old[-2]["bands"][0]["choices"]
        self.assertEqual(len(choices), 17)
        for ordinal, (stage, choice) in enumerate(zip(stages[13:-1], choices, strict=True)):
            expected = copy.deepcopy(old[-2])
            expected.update(id=f"rogue.expanded_stock.{ordinal}", count=[1, 1])
            expected["bands"][0]["choices"] = [choice]
            self.assertEqual(stage, expected)
        treasure = copy.deepcopy(stages[-1])
        treasure.update(chance=old[-1]["chance"], ai=old[-1]["ai"])
        self.assertEqual(treasure, old[-1])
        recipe = copy.deepcopy(fixture["generation_recipes"]["exploration"])
        for key in ("room_omissions", "maze_rooms", "lighting"):
            recipe[key] = original["generation_recipes"]["exploration"][key]
        self.assertEqual(recipe, original["generation_recipes"]["exploration"])


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
        self.assertDisclosedStateEqual(reopened["state"], final["state"])
        self.assertEqual(checkpoint(self, player), saved)
        self.assertEqual(self.activation(player), activation)
        self.assertEqual(self.private_receipts(), receipts)
        for _ in range(3):
            self.assertDisclosedStateEqual(self.request(player, {"type": "snapshot"})["state"], final["state"])
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


    def prepare(self, *, forced):
        package = self.directory / "live-content"
        shutil.copytree(PRODUCTION, package, ignore=shutil.ignore_patterns(".compiled"))
        if forced:
            (package / "scenario.toml").write_text(toml_source(coverage_fixture()),
                                                    encoding="utf-8", newline="\n")
        for name in (*manifest()["modules"], "rules.toml"):
            self.assertEqual((package / name).read_bytes(), (PRODUCTION / name).read_bytes())
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=60)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return package


    def test_unmodified_live_package_records_both_appended_stages_without_extra_activation(self):
        package = self.prepare(forced=False)
        self.server("--checkpoint-interval", 1, scenario=package, seed=42)
        player, _ = self.client()
        receipts = self.private_receipts()
        receipt, = receipts
        self.assertEqual(receipt["scope"], "dungeon/1")
        stages = manifest()["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
        self.assertEqual([row["id"] for row in receipt["decisions"]], [row["id"] for row in stages])
        stock = receipt["decisions"][-2]["result"]
        self.assertEqual(stock["kind"], "evaluated")
        self.assertIn(stock["requested"], (0, 1))
        self.assertEqual(len(stock["attempts"]), stock["requested"])
        if stock["attempts"]:
            self.assertIn(stock["attempts"][0]["content"],
                          [row["archetype"] for row in stages[-2]["bands"][0]["choices"]])
        self.assertEqual(receipt["decisions"][-1]["result"]["kind"], "room_cluster")
        assert_live_content_receipts(self, receipt["decisions"][-2:], 1)
        self.request(player, {"type": "snapshot"})
        self.assertEqual(self.private_receipts(), receipts)


    def test_every_live_choice_and_combined_layout_survive_source_free_reopen_and_revisit(self):
        package = self.prepare(forced=True)
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=42)
        player, _ = self.client()
        receipt, = self.private_receipts()
        supplemental = receipt["decisions"][13:-1]
        expected = manifest()["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][-2]
        self.assertEqual(len(supplemental), 17)
        for decision, choice in zip(supplemental, expected["bands"][0]["choices"], strict=True):
            self.assertEqual(decision["result"]["requested"], 1)
            birth, = decision["result"]["attempts"]
            self.assertEqual((birth["kind"], birth["content"], birth["entity"]["kind"],
                              birth["entity"]["quantity"]), ("placed", choice["archetype"], "item", 1))
        cluster = receipt["decisions"][-1]["result"]["selection"]
        self.assertEqual((cluster["kind"], cluster["actor_depth"]), ("selected", 2))
        self.assertTrue(2 <= cluster["item_requested"] <= 4)
        self.assertTrue(1 <= cluster["actor_requested"] <= 3)
        for decision in supplemental:
            live_stock = {**decision, "id": "rogue.expanded_stock"}
            assert_live_content_receipts(self, [live_stock, receipt["decisions"][-1]], 1)
        self.advance(player, {"type": "wait"})
        activation = self.activation(player)
        scope = activation["scopes"]["dungeon/1"]
        self.assertEqual(scope["members"], list(range(1, 10)))
        self.assertEqual(len(scope["rooms"]), 8, "one omitted slot remains connected passage space")
        def has_interior_wall(room):
            cells = {(cell["position"]["x"], cell["position"]["y"]) for cell in room["cells"]}
            xs, ys = zip(*cells)
            return len(cells) < (max(xs) - min(xs) + 1) * (max(ys) - min(ys) + 1)
        self.assertTrue(any(has_interior_wall(room) for room in scope["rooms"]))
        selected, = [room for room in scope["rooms"] if room["id"] == cluster["room"]]
        for birth in [*cluster["items"], *cluster["actors"]]:
            self.assertEqual(birth["kind"], "placed")
            self.assertIn(birth["location"], selected["cells"])
            self.assertNotIn(birth["location"], scope["protected"])
        final = self.request(player, {"type": "snapshot"})
        saved = checkpoint(self, player)
        for decision in supplemental:
            birth, = decision["result"]["attempts"]
            item = saved["items"][str(birth["entity"]["id"])]
            self.assertEqual(item["quantity"], 1)
            assert_finite_item_identity(self, item, birth["content"])
        player.stop(); server.stop()
        remove_sources(self, package)
        self.server("--checkpoint-interval", 1, seed=None)
        player, reopened = self.client()
        self.assertDisclosedStateEqual(reopened["state"], final["state"])
        self.assertEqual(checkpoint(self, player), saved)
        self.assertEqual(self.activation(player), activation)
        self.assertEqual(self.private_receipts(), [receipt])
        self.assertTrue(underfoot(reopened["state"])["stairs_down"])
        entered = self.advance(player, {"type": "move", "direction": "down"})
        self.assertTrue(underfoot(entered["state"])["stairs_up"])
        second = self.private_receipts()
        self.assertEqual([row["scope"] for row in second], ["dungeon/1", "dungeon/2"])
        self.advance(player, {"type": "move", "direction": "up"})
        self.advance(player, {"type": "move", "direction": "down"})
        self.assertEqual(self.private_receipts(), second, "revisits never replenish either stage")


if __name__ == "__main__":
    unittest.main()
