"""Ordinary carried stock, death drops, finite use and source-free recovery."""
import copy
import shutil
import subprocess
import tomllib
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT, SPECTATOR_TOKEN, explore_to_stairs, settle_rogue_action, underfoot
from rogue_roster_package import encounter_sources, unrelated_fallback


def view(frame):
    return frame["state"]["observation"]


class SpawnCarriedLootProcesses(ProcessTestCase):
    def fixture(self, numerator=1, denominator=1):
        package = self.directory / f"carried-{numerator}-{denominator}"
        package.mkdir()
        shutil.copy2(ROOT / "scenarios/rogue-exploration/rules.toml", package / "rules.toml")
        source = tomllib.loads((ROOT / "scenarios/tests/generated-filler/scenario.toml").read_text(encoding="utf-8"))
        manifest = {key: copy.deepcopy(source[key]) for key in ("format", "version", "ruleset", "creatures")}
        # Current Rogue rules include an opt-in Polymorph roster. Register its
        # immutable definition closure, without enrolling either synthetic actor
        # or changing their one-HD recipes, identities, or birth-stock stream.
        production = encounter_sources()
        self.assertFalse(set(manifest["creatures"]["species"]) & set(production["species"]))
        manifest["creatures"]["species"].update(copy.deepcopy(production["species"]))
        manifest["creatures"]["body_packages"] = copy.deepcopy(production["body_packages"])
        character = copy.deepcopy(source["characters"][0])
        character.pop("asset", None)
        character["anchor"] = "dungeon/1/start"
        manifest.update(id="spawn-carried-stock", default_character=1, characters=[character],
                        factions={"delvers": ["vermin"], "vermin": []},
                        ai_profiles={"idle": {"care": False, "equipment": False, "loot": False, "flee_percent": 0}})
        rat = copy.deepcopy(source["archetypes"]["rat"])
        rat.pop("asset", None)
        manifest["creatures"]["species"]["cave_rat"]["behavior"] = {
            "disposition": "passive", "idle": "wait", "memory_ticks": 1000}
        manifest["archetypes"] = {"carrier": rat, "tonic": {
            "name": "stock tonic", "class": "potion", "stackable": True,
            "consumable": {"effects": [{"kind": "heal", "amount": {"kind": "fixed", "amount": 1}}]}}}
        manifest["generation_recipes"] = {"tiny": {"kind": "grid_rooms", "version": 1,
            "room_width": [4, 4], "room_height": [3, 3], "extra_connections": [0, 0]}}
        manifest["procedural_worlds"] = {"dungeon": {"kind": "grid_chain", "version": 1,
            "base_region": 1, "grid": [1, 1], "size": [8, 5, 2], "recipe": "tiny",
            "domain": {"kind": "open", "max_depth": 100}}}
        manifest["floor_activation"] = {"version": 1, "protagonist": 1,
            "families": {"dungeon": "baseline"},
            "carried_item_pools": {"supplies": {"version": 1, "choices": [
                {"archetype": "tonic", "weight": 1, "quantity": 2}]}},
            "profiles": {"baseline": {"version": 1, "stages": [{"id": "carriers", "kind": "actors",
                "version": 1, "count": [1, 1], "ai": "idle", "bands": [{
                    "depth": {"kind": "open", "minimum": 1}, "choices": [{"archetype": "carrier", "weight": 1,
                    "carried_loot": {"version": 1, "pool": "supplies", "chance": {
                        "numerator": numerator, "denominator": denominator}}}]}]}]}}}
        (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                text=True, capture_output=True, encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        return package

    def execute(self, player, action):
        return settle_rogue_action(self, player, self.act(player, action))

    def approach_and_defeat(self, player, frame):
        self.assertTrue(any(row["name"] == "cave rat" for row in view(frame)["visible_actors"]),
                        "tiny lit fixture must initially disclose its real carrier")
        for _ in range(30):
            foes = [row for row in view(frame)["visible_actors"] if row["name"] == "cave rat"]
            if not foes:
                return frame
            foe = foes[0]
            p = foe["position"]
            if max(abs(p["x"]), abs(p["y"])) > 1:
                direction = ("east" if p["x"] > 0 else "west") if p["x"] else ("south" if p["y"] > 0 else "north")
                frame = self.execute(player, {"type": "move", "direction": direction})
            else:
                frame = self.execute(player, {"type": "use_ability", "id": "basic_melee",
                    "invocation": {"aim": {"kind": "actor_target", "target": foe["id"]}, "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}})
            self.assertIsNone(frame["error"])
        self.fail("Ordinary melee did not defeat the passive carrier within the bounded fixture")

    def collect_tonic(self, player, frame):
        for _ in range(10):
            stocks = [row for row in view(frame)["ground_items"] if row["item"]["name"] == "stock tonic"]
            self.assertEqual(len(stocks), 1, "death must disclose one ordinary birth stack")
            stock = stocks[0]
            p = stock["position"]
            if p["x"] == 0 and p["y"] == 0:
                result = self.execute(player, {"type": "take", "item": stock["item"]["id"], "quantity": None})
                self.assertIsNone(result["error"])
                return result
            direction = ("east" if p["x"] > 0 else "west") if p["x"] else ("south" if p["y"] > 0 else "north")
            frame = self.execute(player, {"type": "move", "direction": direction})
            self.assertIsNone(frame["error"])
        self.fail("The ordinary dropped stack was not reachable")

    def test_birth_death_take_use_source_deleted_restart_and_unvisited_floor(self):
        package = self.fixture()
        server = self.server("--checkpoint-interval", 1, scenario=package)
        player, initial = self.client()
        self.assertFalse(initial["state"]["wizard_game"])
        first = self.private_receipts()
        loot = first[0]["decisions"][0]["result"]["attempts"][0]["entity"]["carried_loot"]
        self.assertEqual((loot["kind"], loot["content"], loot["quantity"]), ("carried", "tonic", 2))
        frame = self.collect_tonic(player, self.approach_and_defeat(player, initial))
        tonic = next(row for row in view(frame)["inventory"] if row["name"] == "stock tonic")
        self.assertEqual(tonic["quantity"], "2")
        text, _ = self.text_client(SPECTATOR_TOKEN, observe=True)
        self.assertIn("2 x stock tonic", text.command("inventory"))
        text.stop()
        for remaining in ["1", None]:
            frame = self.execute(player, {"type": "drink", "item": tonic["id"]})
            self.assertIsNone(frame["error"])
            inventory = [row for row in view(frame)["inventory"] if row["name"] == "stock tonic"]
            self.assertEqual([row["quantity"] for row in inventory], [] if remaining is None else [remaining])
        self.assertEqual(self.private_receipts(), first, "consumption cannot rewrite birth evidence")
        before = self.request(player, {"type": "snapshot"})
        self.flush_save(); player.stop(); server.stop()
        shutil.rmtree(package)
        server = self.server("--checkpoint-interval", 1, scenario=unrelated_fallback(self), seed=None)
        player, restored = self.client()
        self.assertDisclosedStateEqual(restored["state"], before["state"])
        self.assertEqual(restored["history"], before["history"])
        self.assertEqual(self.private_receipts(), first)
        state = explore_to_stairs(self, player, restored["state"])
        self.assertTrue(underfoot(state)["stairs_down"])
        entered = self.execute(player, {"type": "move", "direction": "down"})
        self.assertIsNone(entered["error"])
        receipts = self.private_receipts()
        self.assertEqual(len(receipts), 2)
        second = receipts[1]["decisions"][0]["result"]["attempts"][0]["entity"]["carried_loot"]
        self.assertEqual(second["quantity"], 2)
        self.assertGreater(second["item"], loot["item"])
        self.assertIsNone(self.execute(player, {"type": "move", "direction": "up"})["error"])
        returned = self.execute(player, {"type": "move", "direction": "down"})
        self.assertIsNone(returned["error"])
        self.assertEqual(self.private_receipts(), receipts)
        self.assertFalse(any(row["name"] == "stock tonic" for row in view(returned)["inventory"]))
        player.stop(); server.stop()

    def test_explicit_miss_and_fixed_one_fifth_seed_pair_survive_reconnect(self):
        # The policy domain uses the same canonical names as the pinned unit vectors.
        for numerator, denominator, seed, expected in [(0, 1, 0, "not_carried"), (1, 5, 0, "carried"), (1, 5, 1, "not_carried")]:
            with self.subTest(chance=(numerator, denominator), seed=seed):
                package = self.fixture(numerator, denominator)
                self.save = self.directory / f"chance-{numerator}-{denominator}-{seed}.db"
                server = self.server(scenario=package, seed=seed)
                player, before = self.client()
                receipts = self.private_receipts()
                self.assertEqual(receipts[0]["decisions"][0]["result"]["attempts"][0]["entity"]["carried_loot"]["kind"], expected)
                player.stop()
                player, after = self.client()
                self.assertDisclosedStateEqual(after["state"], before["state"])
                self.assertEqual(self.private_receipts(), receipts)
                self.flush_save(); player.stop(); server.stop()
                shutil.rmtree(package)
                server = self.server(seed=None)
                player, resumed = self.client()
                self.assertDisclosedStateEqual(resumed["state"], before["state"])
                self.assertEqual(resumed["history"], before["history"])
                self.assertEqual(self.private_receipts(), receipts)
                player.stop(); server.stop()


if __name__ == "__main__":
    unittest.main()
