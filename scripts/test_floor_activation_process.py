"""Authoritative floor entry and recovery through ordinary exploration.

A marker-only fixture isolates population receipts from combat tuning.
"""
import shutil
import sqlite3
import subprocess
from contextlib import closing

from process_harness import ProcessTestCase, ROOT, SUFFIX, explore_to_stairs, inspect_save, underfoot
from rogue_roster_package import unrelated_fallback


class FloorActivationProcess(ProcessTestCase):
    def fixture(self):
        package = self.directory / "activation-package"
        shutil.copytree(ROOT / "scenarios/rogue-exploration", package,
                        ignore=shutil.ignore_patterns(".compiled"))
        # Explicit geometry-only test fixture; the real Rogue package remains
        # the product receiving the playable starter and population.
        with (package / "scenario.toml").open("w", encoding="utf-8") as output:
            output.write('''
format = 27
id = "floor-activation"
version = "1.0"
ruleset = "rules.toml"
default_character = 1
[[characters]]
id = 1
anchor = "dungeon/1/start"
turn_ticks = 100
[generation_recipes.exploration]
kind = "grid_rooms"
version = 1
room_width = [4, 24]
room_height = [3, 5]
extra_connections = [0, 2]
lighting = { darkness_roll = 10, darkness_start = 1 }
[procedural_worlds.dungeon]
kind = "grid_chain"
version = 1
base_region = 1
grid = [3, 3]
size = [26, 7, 2]
recipe = "exploration"
[procedural_worlds.dungeon.domain]
kind = "open"
max_depth = 1000000
[archetypes.marker]
name = "activation marker"
[archetypes.alternate]
name = "alternate activation marker"
[floor_activation]
version = 1
protagonist = 1
families = { dungeon = "baseline" }
[floor_activation.profiles.baseline]
version = 1
[[floor_activation.profiles.baseline.stages]]
id = "marker"
version = 1
kind = "items"
count = [1, 1]
bands = [{ depth = { kind = "finite", minimum = 1, maximum = 2 }, choices = [{ archetype = "marker", weight = 1 }, { archetype = "alternate", weight = 3 }] }, { depth = { kind = "open", minimum = 4 }, choices = [{ archetype = "alternate", weight = 1 }] }]
[[floor_activation.profiles.baseline.stages]]
id = "later"
version = 1
kind = "items"
count = [1, 1]
bands = [{ depth = { kind = "open", minimum = 3 }, choices = [{ archetype = "marker", weight = 1 }] }]
''')
        result = subprocess.run([self.bin / ("tor-scenario" + SUFFIX), "validate", package],
                                text=True, capture_output=True, encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        return package

    def test_source_deleted_new_floor_has_one_entry_receipt_and_revisit_never_redecides(self):
        package = self.fixture()
        # Natural exploration has a long journal. Bound restore with a real
        # checkpoint plus tail; short starter cases below retain journal-only.
        server = self.server("--checkpoint-interval", 64, scenario=package)
        # This long route checks current views and exact saved state, never the
        # headless client's unrelated accumulated-memory diagnostic projection.
        player, ready = self.client(omit_memory=True)
        first = self.private_receipts()
        self.assertEqual([entry["scope"] for entry in first], ["dungeon/1"])
        self.assertEqual(first[0]["decisions"][1]["result"]["kind"], "skipped")

        state = explore_to_stairs(self, player, ready["state"])
        self.assertTrue(underfoot(state)["stairs_down"])
        self.assertEqual(self.private_receipts(), first, "visibility/preparation is not player entry")
        entered = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNone(entered["error"])
        self.assertTrue(underfoot(entered["state"])["stairs_up"])
        second = self.private_receipts()
        self.assertEqual([entry["scope"] for entry in second], ["dungeon/1", "dungeon/2"])

        returned = self.act(player, {"type": "move", "direction": "up"})
        self.assertIsNone(returned["error"])
        reentered = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNone(reentered["error"])
        self.assertEqual(self.private_receipts(), second)
        before = self.request(player, {"type": "snapshot"})
        self.flush_save()
        player.stop()
        server.stop()
        with closing(sqlite3.connect(self.save)) as db:
            checkpoint = db.execute("SELECT sequence FROM checkpoint").fetchone()
            self.assertIsNotNone(checkpoint)
            self.assertGreater(checkpoint[0], 0)
            self.assertGreater(db.execute("SELECT MAX(sequence) FROM journal").fetchone()[0], checkpoint[0])
        shutil.rmtree(package)

        server = self.server("--checkpoint-interval", 64, scenario=unrelated_fallback(self), seed=None)
        player, resumed = self.client(omit_memory=True)
        self.assertEqual(resumed["state"], before["state"])
        self.assertEqual(resumed["history"], before["history"])
        self.assertEqual(self.private_receipts(), second)
        state = explore_to_stairs(self, player, resumed["state"])
        entered = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNone(entered["error"])
        third = self.private_receipts()
        self.assertEqual([entry["scope"] for entry in third],
                         ["dungeon/1", "dungeon/2", "dungeon/3"])
        self.assertEqual(third[2]["decisions"][1]["result"]["kind"], "evaluated")
        self.assertEqual(third[2]["decisions"][0]["result"],
                         {"kind": "skipped", "reason": "no_eligible_content"})
        self.assertEqual(third[:2], second, "later eligibility cannot rewrite a saved skip")
        player.stop()
        server.stop()


class RogueStarterProcess(ProcessTestCase):
    def assert_starter(self, frame):
        view = frame["state"]["observation"]
        inventory = view["inventory"]
        self.assertEqual({row["name"]: int(row["quantity"]) for row in inventory},
                         {"mace": 1, "ring mail": 1, "clean bandage": 3, "food ration": 1})
        self.assertEqual(len(inventory), 4)
        self.assertFalse(frame["state"]["wizard_game"])
        names = {row["id"]: row["name"] for row in inventory}
        equipped = {names[row["item"]]: row["slot"]["id"]
                    for row in view["interactions"]["inventory"]
                    if row["equipped_slot"] is not None}
        self.assertEqual(equipped, {"mace": "weapon", "ring mail": "body_armor"})
        supplies = {row["known_supply"]["id"]: int(row["quantity"])
                    for row in inventory if row.get("known_supply")}
        self.assertEqual(supplies, {"bandage": 3, "ration": 1})
        return inventory

    def selected_starter_restart(self, selected):
        for checkpoint_interval in (0, 1):
            with self.subTest(character=selected, checkpoint_interval=checkpoint_interval):
                package = self.directory / f"rogue-{selected}-{checkpoint_interval}"
                shutil.copytree(ROOT / "scenarios/rogue-exploration", package,
                                ignore=shutil.ignore_patterns(".compiled"))
                result = subprocess.run(
                    [self.bin / ("tor-scenario" + SUFFIX), "validate", package],
                    text=True, capture_output=True, encoding="utf-8", timeout=30)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.save = self.directory / f"starter-{selected}-{checkpoint_interval}.db"
                server = self.server("--checkpoint-interval", checkpoint_interval,
                                     scenario=package, character=selected)
                player, initial = self.client(actor=selected)
                inventory = self.assert_starter(initial)
                # One ordinary action supplies a real journal/checkpoint tail.
                # This test checks initialization stock, not combat balance.
                waited = self.act(player, {"type": "wait"})
                self.assertIsNone(waited["error"])
                self.assertEqual(self.assert_starter(waited), inventory)
                self.flush_save(actor=selected)
                base = inspect_save(self.save)
                self.assertEqual(base["scenario"]["package"]["selected"], selected)
                self.assertEqual(base["initial_activations"][0]["entrant"], selected)
                before = self.request(player, {"type": "snapshot"})
                player.stop()
                server.stop()
                shutil.rmtree(package)
                # Restore the selected preset from the save. Name the actor
                # only for cleanup's durability barrier, not --character.
                server = self.server("--checkpoint-interval", checkpoint_interval,
                                     seed=None, save_actor=selected)
                player, resumed = self.client(actor=selected)
                self.assertEqual(self.assert_starter(resumed), inventory)
                self.assertEqual(resumed["state"], before["state"])
                self.assertEqual(resumed["history"], before["history"])
                player.stop()
                server.stop()
                reopened = inspect_save(self.save)
                self.assertEqual(reopened["scenario"]["package"]["selected"], selected)
                self.assertEqual(reopened["initial_activations"], base["initial_activations"])

    def test_warrior_ordinary_initial_kit_survives_source_deleted_restarts(self):
        self.selected_starter_restart(1)

    def test_mage_ordinary_initial_kit_survives_source_deleted_restarts(self):
        self.selected_starter_restart(2)
