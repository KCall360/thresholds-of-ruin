"""Normal Rogue preset, advancement and combat through the real applications.

The production package is used without wizard setup or combat/rules overrides.
Seeded success establishes a content walkthrough, not numerical balance.
"""
from collections import Counter
from contextlib import closing
import shutil
import sqlite3
import subprocess
import unittest

from process_harness import (
    ProcessTestCase, ROOT, SPECTATOR_TOKEN, content_ids, hit_die_counts,
    disclosed_exploration_route, settle_rogue_action, underfoot,
)

from rogue_test_support import DisclosedFloorGraph, cardinal_melee_candidate


def observation(frame):
    return frame["state"]["observation"]


def abilities(frame):
    return set(content_ids(observation(frame)["combat"]["own_stats"]["abilities"]))


def carried_stock(frame, supply):
    return sum(int(row["quantity"]) for row in observation(frame)["inventory"]
               if (row.get("known_supply") or {}).get("id") == supply)


class RogueChecks:
    def advance(self, player, command):
        frame = self.command(player, {"type": "advancement", "command": command})
        self.assertIsNone(frame["error"], command)
        if (command.startswith("confirm") or command == "resume") and any(
                row["phase"] == "queued" for row in frame["intentions"]):
            frame = self.frame(player, lambda current: not any(
                row["phase"] == "queued" for row in current["intentions"]))
        return frame

    def overview(self, player):
        return self.advance(player, "overview")["advancement_page"]["entries"][0]["value"]

    def rows(self, player, command):
        page = self.advance(player, command)["advancement_page"]
        result = list(page["entries"])
        while page["next"] is not None:
            previous = page
            page = self.advance(player, "next")["advancement_page"]
            self.assertEqual(page["basis"], previous["basis"])
            self.assertEqual(page["revision"], previous["revision"])
            result.extend(page["entries"])
        self.assertEqual(len(result), page["total"])
        return result

    def assert_starter(self, frame, source):
        view = observation(frame)
        self.assertFalse(frame["state"]["wizard_game"])
        self.assertFalse(view["combat"]["terminal"])
        self.assertEqual(hit_die_counts(view["combat"]["own_stats"]["hit_dice"]), {("class", source): 1})
        self.assertEqual({row["name"]: int(row["quantity"]) for row in view["inventory"]},
                         {"mace": 1, "ring mail": 1, "clean bandage": 3, "food ration": 1})
        self.assertEqual(len(view["inventory"]), 4)
        self.assertEqual(carried_stock(frame, "bandage"), 3)
        self.assertEqual(carried_stock(frame, "ration"), 1)
        names = {row["id"]: row["name"] for row in view["inventory"]}
        equipped = {names[row["item"]]: row["slot"]["id"]
                    for row in view["interactions"]["inventory"]
                    if row["equipped_slot"] is not None}
        self.assertEqual(equipped, {"mace": "weapon", "ring mail": "body_armor"})
        self.assertTrue({"basic_melee", "bind_wounds"}.issubset(abilities(frame)))
        self.assertTrue({"spark_lance_cold", "spark_lance_fire", "falter", "shell_impact",
                         "shell_keen", "shell_energy"}.isdisjoint(abilities(frame)))

    def settled_action(self, player, action, *, survive=True):
        frame = self.act(player, action)
        self.assertIsNone(frame["error"], action)
        return settle_rogue_action(self, player, frame, survive=survive)

    def clear_first_floor(self, player):
        """Choose the first talent, then fight and collect through ordinary play."""
        growth = self.overview(player)
        self.assertEqual(growth["points"], {"earned": 1, "spent": 0, "unspent": 1})
        preview = self.advance(player, "purchase 1 advancement talent rattling_blow")
        self.assertNotIn("rattling_blow", abilities(preview))
        self.advance(player, "confirm")
        current = self.request(player, {"type": "snapshot"})
        self.assertIn("rattling_blow", abilities(current))
        at, visited, floor, stock_cells = (0, 0), set(), set(), set()
        seen_foes = {}
        collected = Counter()
        collected_cells = set()
        cleared_cells = set()
        floor_graph = DisclosedFloorGraph()
        # The bound is the same disclosed-cell exploration budget as the floor
        # harness. It is a failure deadline, never an ignored adverse seed.
        for _ in range(2500):
            view = observation(current)
            visited.add(at)
            floor_graph.observe(current["state"])
            cleared_cells.add(underfoot(current["state"])["key"])
            hostile = {row["actor"] for row in view["combat"]["actors"] if row["hostile"]}
            foes = [row for row in view["visible_actors"] if row["id"] in hostile]
            seen_foes.update((row["id"], row["name"]) for row in foes)
            for cell in view["visible_cells"]:
                position = cell["position"]
                if position["z"] == 0 and not cell["wall"]:
                    floor.add((at[0] + position["x"], at[1] + position["y"]))
            for row in view["ground_items"]:
                if row["item"].get("known_supply"):
                    position = row["position"]
                    stock_cells.add((at[0] + position["x"], at[1] + position["y"]))
            nearby = next((row for row in foes if cardinal_melee_candidate(row)), None)
            if nearby:
                current = self.settled_action(player, {"type": "use_ability", "id": "basic_melee",
                                                      "invocation": {"kind": "actor_target", "target": nearby["id"]}})
                # Only a lethal attack can add XP in this walkthrough. Walking
                # and taking finite stock do not need another private page.
                growth = self.overview(player)
                continue
            reachable = next((row for row in view["ground_items"]
                              if row["reachable"] and row["item"].get("known_supply")), None)
            if reachable:
                item = reachable["item"]
                collected_cells.add(next(cell["key"] for cell in view["visible_cells"]
                                         if cell["position"] == reachable["position"]))
                supply, quantity = item["known_supply"]["id"], int(item["quantity"])
                before = carried_stock(current, supply)
                current = self.settled_action(player, {"type": "take", "item": item["id"], "quantity": str(quantity)})
                self.assertEqual(carried_stock(current, supply), before + quantity)
                self.assertNotIn(item["id"], [row["item"]["id"] for row in observation(current)["ground_items"]])
                collected[supply] += quantity
                position = reachable["position"]
                stock_cells.discard((at[0] + position["x"], at[1] + position["y"]))
                continue
            if growth["growth"]["earned_xp"] == "110" and collected == {"bandage": 2, "ration": 1}:
                break
            targets = set(stock_cells)
            for foe in foes:
                position = foe["position"]
                targets.add((at[0] + position["x"], at[1] + position["y"]))
            route = disclosed_exploration_route(at, floor, targets, visited)
            self.assertIsNotNone(route, "No disclosed route to remaining foes, supplies or unexplored cells")
            target, direction = route[0]
            before = underfoot(current["state"])["key"]
            current = self.settled_action(player, {"type": "move", "direction": direction})
            self.assertNotEqual(underfoot(current["state"])["key"], before)
            at = target
        else:
            self.fail("The production Rogue floor did not complete its combat/stock walkthrough")
        # Movement or looting after the final hit invalidates owner context.
        # Refresh once for the caller's next advancement operation.
        growth = self.overview(player)
        self.assertEqual(growth["growth"]["earned_xp"], "110")
        return current, growth, seen_foes, collected_cells, cleared_cells, floor_graph

    def walk_to_stairs(self, player, current, floor_graph=None):
        """Head for disclosed stairs, defending without a full-floor loot tour."""
        floor_graph = floor_graph or DisclosedFloorGraph()
        visited = set()
        for _ in range(2500):
            view = observation(current)
            floor_graph.observe(current["state"])
            here = underfoot(current["state"])
            visited.add(here["key"])
            if here["stairs_down"]:
                return current
            hostile = {row["actor"] for row in view["combat"]["actors"] if row["hostile"]}
            nearby = next((row for row in view["visible_actors"]
                           if row["id"] in hostile and cardinal_melee_candidate(row)), None)
            if nearby:
                current = self.settled_action(player, {"type": "use_ability", "id": "basic_melee",
                    "invocation": {"kind": "actor_target", "target": nearby["id"]}})
                continue
            route = floor_graph.route_to_any(here["key"], floor_graph.stairs_down)
            if route is None:
                route = floor_graph.route_to_any(here["key"], floor_graph.edges.keys() - visited)
            self.assertTrue(route, "No disclosed route to stairs or an unexplored approach")
            key, direction = route[0]
            current = self.settled_action(player, {"type": "move", "direction": direction})
            self.assertEqual(underfoot(current["state"])["key"], key,
                             "Disclosed stair-approach step did not reach its observed key")
        self.fail("The ordinary stair approach exceeded its 2500-action bound")

    def revisit_cleared_floor(self, player, current, seen_foes, collected_cells, cleared_cells, floor_graph):
        """Revisit every recorded combat-walkthrough and collected-supply key."""
        required = collected_cells | cleared_cells
        here = underfoot(current["state"])["key"]
        floor_graph.observe(current["state"])
        try:
            route = floor_graph.route_through(here, required)
        except ValueError as error:
            self.fail(str(error))
        self.assertLessEqual(len(route), min(2500, 2 * (len(floor_graph.edges) - 1)),
                             "The required-key revisit exceeded its disclosed-tree bound")
        visited_keys = set()
        for step in [None, *route]:
            if step is not None:
                key, direction = step
                current = self.settled_action(player, {"type": "move", "direction": direction})
                self.assertEqual(underfoot(current["state"])["key"], key,
                                 "The revisit did not reach its recorded disclosed key")
            view = observation(current)
            visited_keys.add(underfoot(current["state"])["key"])
            self.assertFalse(set(seen_foes).intersection(row["id"] for row in view["visible_actors"]),
                             "A defeated first-floor foe reappeared")
            self.assertFalse(any(row["hostile"] for row in view["combat"]["actors"]),
                             "The cleared first floor gained a hostile")
            self.assertFalse(any(row["item"].get("known_supply") for row in view["ground_items"]),
                             "Collected first-floor supply stock reappeared")
        self.assertTrue(collected_cells.issubset(visited_keys),
                        "The revisit did not reach every collected supply cell")
        self.assertTrue(cleared_cells.issubset(visited_keys),
                        "The revisit did not retrace every combat-walkthrough cell")
        return current

    def assert_rogue_population(self, receipt, scope, cause):
        self.assertEqual(receipt["scope"], scope)
        self.assertEqual(receipt["entrant"], 1)
        self.assertEqual(receipt["cause"], cause)
        self.assertEqual(receipt["profile"], "rogue.low_depth")
        expected = (
            ("rogue.emus", "rogue.emu", "actor", 2),
            ("rogue.snakes", "rogue.snake", "actor", 2),
            ("rogue.bandages", "rogue.clean_bandage", "item", 1),
            ("rogue.rations", "rogue.ration", "item", 1),
        )
        self.assertEqual([stage["id"] for stage in receipt["decisions"]],
                         [stage for stage, _, _, _ in expected])
        entities = []
        for stage, (_, content, kind, count) in zip(receipt["decisions"], expected, strict=True):
            result = stage["result"]
            self.assertEqual(result["kind"], "evaluated")
            self.assertEqual(result["requested"], count)
            self.assertEqual(len(result["attempts"]), count)
            for attempt in result["attempts"]:
                self.assertEqual(attempt["kind"], "placed")
                self.assertEqual(attempt["content"], content)
                self.assertEqual(attempt["entity"]["kind"], kind)
                entities.append((kind, attempt["entity"]["id"]))
        self.assertEqual(len(set(entities)), len(entities))


class RoguePlayableProcesses(RogueChecks, ProcessTestCase):
    def test_each_preset_has_equipped_stock_an_unspent_point_and_matching_future_source(self):
        for actor, source, ranks in (
                (1, "warrior", ["heavy_weaponry", "discipline"]),
                (2, "mage", ["spellcasting", "heavy_weaponry"])):
            with self.subTest(character=actor):
                self.save = self.directory / f"preset-{actor}.db"
                server = self.server(scenario="rogue-exploration", seed=42, character=actor)
                player, initial = self.client(actor=actor)
                self.assert_starter(initial, source)
                value = self.overview(player)
                self.assertEqual(content_ids(value["starting_skills"]), ranks)
                self.assertEqual(value["points"], {"earned": 1, "spent": 0, "unspent": 1})
                self.assertEqual(value["growth"]["future"], {
                    "kind": "selected", "source": {"kind": "class", "class": source}})
                self.assertEqual(value["growth"]["earned_xp"], "0")
                self.assertEqual(value["growth"]["held_xp"], "0")
                player.stop()
                server.stop()

    def test_mage_exact_form_and_future_source_survive_source_deleted_restart(self):
        package = self.directory / "mage-package"
        shutil.copytree(ROOT / "scenarios/rogue-exploration", package,
                        ignore=shutil.ignore_patterns(".compiled"))
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server(scenario=package, seed=42, character=2)
        player, initial = self.client(actor=2)
        before = self.overview(player)
        for selection in ("talent evocation_initiate", "talent binding_initiate form spark_lance_cold"):
            rejected = self.command(player, {"type": "advancement", "command":
                                             "purchase 1 advancement " + selection})
            self.assertIsNotNone(rejected["error"])
            self.assertEqual(self.overview(player)["points"], before["points"])
        preview = self.advance(player, "purchase 1 advancement talent evocation_initiate form spark_lance_cold")
        # An owner snapshot resets the preview context. Inspect the preview's
        # current state without invalidating the confirmation under test.
        self.assertNotIn("spark_lance_cold", abilities(preview))
        self.advance(player, "confirm")
        acquired = self.request(player, {"type": "snapshot"})
        self.assertIn("spark_lance_cold", abilities(acquired))
        self.assertNotIn("spark_lance_fire", abilities(acquired))
        self.assertEqual(carried_stock(acquired, "ration"), carried_stock(initial, "ration"))
        # The purchase changed the build basis; open a fresh owner page before
        # previewing the next, distinct advancement operation.
        self.overview(player)
        self.advance(player, "source class warrior")
        self.advance(player, "confirm")
        selected = self.overview(player)
        self.assertEqual(selected["growth"]["future"], {
            "kind": "selected", "source": {"kind": "class", "class": "warrior"}})
        self.assertEqual(selected["points"], {"earned": 1, "spent": 1, "unspent": 0})
        current = self.request(player, {"type": "snapshot"})
        self.assertEqual(hit_die_counts(observation(current)["combat"]["own_stats"]["hit_dice"]), {("class", "mage"): 1})
        # The owner snapshot cleared page context; reopen it before paging.
        self.overview(player)
        ledger = self.rows(player, "ledger")
        self.assertEqual(len(ledger), 1)
        self.assertEqual(ledger[0]["value"]["selection"], {
            "kind": "talent_form", "talent": "evocation_initiate", "form": "spark_lance_cold"})
        spectator, watched = self.client(SPECTATOR_TOKEN, observe=True, actor=2)
        self.assertEqual(watched["state"], current["state"])
        denied = self.command(spectator, {"type": "advancement", "command": "overview"})
        self.assertIsNotNone(denied["error"])
        self.flush_save(actor=2)
        player.stop()
        spectator.stop()
        server.stop()
        shutil.rmtree(package)
        # Recover the selected Mage from the save, without --character. Only
        # cleanup's explicit durability barrier needs this known actor identity.
        self.server(seed=None, save_actor=2)
        player, reopened = self.client(actor=2)
        self.assertEqual(reopened["state"], current["state"])
        self.assertEqual(self.overview(player)["growth"], selected["growth"])
        self.assertEqual(self.rows(player, "ledger"), ledger)
        self.assertEqual(carried_stock(reopened, "ration"), 1)

    def test_text_warrior_spends_its_first_point_only_after_explicit_confirmation(self):
        self.server(scenario="rogue-exploration", seed=42)
        text, _ = self.text_client()
        self.assertIn("Starting skills", text.command("advancement"))
        preview = text.command("advancement purchase 1 advancement talent rattling_blow")
        self.assertIn("talent rattling_blow", preview)
        observer, before = self.client(observe=True)
        self.assertNotIn("rattling_blow", abilities(before))
        self.assertNotIn("Server error", text.command("advancement confirm"))
        after = self.request(observer, {"type": "snapshot"})
        self.assertIn("rattling_blow", abilities(after))
        self.assertEqual(carried_stock(after, "ration"), 1)
        self.assertFalse(after["state"]["wizard_game"])
        text.command("advancement")
        self.assertIn("talent rattling_blow", text.command("advancement ledger"))

    def test_ordinary_first_floor_foes_earn_second_hd_and_ground_supplies_are_finite(self):
        server = self.server("--checkpoint-interval", 64, scenario="rogue-exploration", seed=42)
        # This long driver builds its route from current disclosed observations.
        # Avoid reprinting accumulated map memory on every diagnostic frame.
        player, _ = self.client(omit_memory=True)
        current, growth, seen_foes, collected_cells, cleared_cells, floor_graph = self.clear_first_floor(player)
        self.assertEqual(Counter(seen_foes.values()), {"emu": 2, "snake": 2})
        self.assertEqual(growth["growth"]["spent_xp"], "100")
        self.assertEqual(growth["growth"]["held_xp"], "10")
        self.assertEqual(growth["growth"]["highest_earned_tier"], 2)
        self.assertEqual(growth["points"], {"earned": 2, "spent": 1, "unspent": 1})
        self.assertEqual(hit_die_counts(observation(current)["combat"]["own_stats"]["hit_dice"]),
                         {("class", "warrior"): 2})
        self.advance(player, "purchase 2 advancement skill heavy_weaponry")
        self.advance(player, "confirm")
        self.assertEqual(self.overview(player)["points"], {"earned": 2, "spent": 2, "unspent": 0})
        current = self.request(player, {"type": "snapshot"})
        self.assertEqual(carried_stock(current, "bandage"), 5)
        self.assertEqual(carried_stock(current, "ration"), 2)
        self.assertFalse(current["state"]["wizard_game"])

        # Revisit through ordinary disclosed stairs, never a hidden-position oracle.
        initial_receipts = self.private_receipts()
        self.assertEqual([entry["scope"] for entry in initial_receipts], ["dungeon/1"])
        departure = self.walk_to_stairs(player, current, floor_graph)
        departure_key = underfoot(departure["state"])["key"]
        entered = self.settled_action(player, {"type": "move", "direction": "down"})
        self.assertTrue(underfoot(entered["state"])["stairs_up"])
        entered_receipts = self.private_receipts()
        self.assertEqual([entry["scope"] for entry in entered_receipts], ["dungeon/1", "dungeon/2"])
        self.assertEqual(entered_receipts[:1], initial_receipts)
        returned = self.settled_action(player, {"type": "move", "direction": "up"})
        self.assertEqual(underfoot(returned["state"])["key"], departure_key)
        self.assertEqual(carried_stock(returned, "bandage"), 5)
        self.assertEqual(carried_stock(returned, "ration"), 2)
        self.assertEqual(self.overview(player)["growth"]["earned_xp"], "110")
        self.assertEqual(self.private_receipts(), entered_receipts,
                         "A revisit appended or changed a durable floor-entry receipt")
        returned = self.revisit_cleared_floor(
            player, returned, seen_foes, collected_cells, cleared_cells, floor_graph)
        self.assertEqual(carried_stock(returned, "bandage"), 5)
        self.assertEqual(carried_stock(returned, "ration"), 2)
        self.assertEqual(self.overview(player)["growth"]["earned_xp"], "110")
        self.assertEqual(self.private_receipts(), entered_receipts)

        # Care and paid replacement consume the real collected stock in a safe
        # cleared floor. Neither operation grants inventory on return or reopen.
        current = self.settled_action(player, {"type": "use_ability", "id": "bind_wounds",
            "invocation": {"kind": "actor_target", "target": observation(returned)["self_target"]}, "supplies": {"kind": "carried"}})
        self.assertEqual(carried_stock(current, "bandage"), 4)
        self.assertEqual(carried_stock(current, "ration"), 2)
        quiet = self.overview(player)
        # Bound comes from the disclosed 600-tick quiet policy and 100-tick Wait.
        for _ in range(1 + (int(quiet["timings"]["received_loss_interval"]) + 99) // 100):
            if int(quiet["received_loss_remaining"]) == 0:
                break
            current = self.settled_action(player, {"type": "wait"})
            quiet = self.overview(player)
        self.assertEqual(quiet["received_loss_remaining"], "0")
        self.assertFalse(quiet["perceived_threat"])
        self.advance(player, "remove 2 1")
        self.advance(player, "confirm using carried")
        current = self.request(player, {"type": "snapshot"})
        if not observation(current)["ready"]:
            current = self.frame(player, lambda frame: frame.get("state") is not None
                and observation(frame)["ready"])
        self.assertEqual(self.overview(player)["points"], {"earned": 2, "spent": 1, "unspent": 1})
        current = self.request(player, {"type": "snapshot"})
        self.assertEqual(carried_stock(current, "ration"), 1)
        self.assertEqual(carried_stock(current, "bandage"), 4)
        self.flush_save()
        player.stop()
        server.stop()
        self.server("--checkpoint-interval", 64, seed=None)
        player, reopened = self.client(omit_memory=True)
        self.assertEqual(reopened["state"], current["state"])
        self.assertEqual(carried_stock(reopened, "ration"), 1)
        self.assertEqual(carried_stock(reopened, "bandage"), 4)
        self.assertEqual(self.overview(player)["points"], {"earned": 2, "spent": 1, "unspent": 1})
        self.assertFalse(reopened["state"]["wizard_game"])
        self.assertEqual(self.private_receipts(), entered_receipts)

    def test_source_deleted_rogue_restart_populates_an_unvisited_floor_with_real_foes_and_supplies(self):
        package = self.directory / "population-package"
        shutil.copytree(ROOT / "scenarios/rogue-exploration", package,
                        ignore=shutil.ignore_patterns(".compiled"))
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        # Exercise source-free activation independently of the full combat/
        # finite-stock walkthrough. Defend only along the ordinary stair route;
        # the subsequent entry deliberately replays from the journal tail.
        server = self.server("--checkpoint-interval", 64, scenario=package, seed=42)
        player, current = self.client(omit_memory=True)
        departure = self.walk_to_stairs(player, current)
        self.assertTrue(underfoot(departure["state"])["stairs_down"])
        first = self.private_receipts()
        self.assertEqual(len(first), 1, "Preparation must not activate the unvisited second floor")
        self.assert_rogue_population(first[0], "dungeon/1", "initial")
        before = self.request(player, {"type": "snapshot"})
        player.stop()
        server.stop()
        with closing(sqlite3.connect(self.save)) as db:
            checkpoint = db.execute("SELECT sequence FROM checkpoint").fetchone()
            self.assertIsNotNone(checkpoint)
            self.assertGreater(checkpoint[0], 0)
            sequence = db.execute("SELECT MAX(sequence) FROM journal").fetchone()[0]
        shutil.rmtree(package)
        self.assertFalse(package.exists())

        server = self.server("--checkpoint-interval", 0, seed=None)
        player, reopened = self.client(omit_memory=True)
        self.assertEqual(reopened["state"], before["state"])
        self.assertEqual(self.private_receipts(), first)
        entered = self.settled_action(player, {"type": "move", "direction": "down"})
        self.assertTrue(underfoot(entered["state"])["stairs_up"])
        self.assertFalse(entered["state"]["wizard_game"])
        for supply in ("bandage", "ration"):
            self.assertEqual(carried_stock(entered, supply), carried_stock(before, supply))
        second = self.private_receipts()
        self.assertEqual(len(second), 2)
        self.assertEqual(second[:1], first)
        self.assert_rogue_population(second[1], "dungeon/2", "movement")
        before = self.request(player, {"type": "snapshot"})
        player.stop()
        server.stop()
        with closing(sqlite3.connect(self.save)) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone(), checkpoint)
            self.assertGreater(db.execute("SELECT MAX(sequence) FROM journal").fetchone()[0], sequence)

        self.server("--checkpoint-interval", 0, seed=None)
        player, reopened = self.client(omit_memory=True)
        self.assertEqual(reopened["state"], before["state"])
        self.assertEqual(self.private_receipts(), second)

    def test_ordinary_rogue_death_stays_terminal_after_reopening(self):
        server = self.server(scenario="rogue-exploration", seed=42)
        player, current = self.client()
        at, visited, floor = (0, 0), set(), set()
        for _ in range(2500):
            view = observation(current)
            if view["combat"]["dead"]:
                break
            hostile = {row["actor"] for row in view["combat"]["actors"] if row["hostile"]}
            if any(row["id"] in hostile for row in view["visible_actors"]):
                current = self.settled_action(player, {"type": "wait"}, survive=False)
                self.assertIsNone(current["error"])
                continue
            visited.add(at)
            for cell in view["visible_cells"]:
                position = cell["position"]
                if position["z"] == 0 and not cell["wall"]:
                    floor.add((at[0] + position["x"], at[1] + position["y"]))
            route = disclosed_exploration_route(at, floor, set(), visited)
            self.assertIsNotNone(route, "No disclosed route to an ordinary hostile")
            target, direction = route[0]
            before = underfoot(current["state"])["key"]
            current = self.settled_action(player, {"type": "move", "direction": direction}, survive=False)
            self.assertIsNone(current["error"])
            if not observation(current)["combat"]["dead"]:
                self.assertNotEqual(underfoot(current["state"])["key"], before)
                at = target
        else:
            self.fail("Ordinary hostile pressure did not reach death within the disclosed-play bound")
        self.assertTrue(observation(current)["combat"]["dead"])
        self.assertTrue(observation(current)["combat"]["terminal"])
        self.assertFalse(current["state"]["wizard_game"])
        self.flush_save()
        current = self.request(player, {"type": "snapshot"})
        player.stop()
        server.stop()
        self.server(seed=None)
        player, reopened = self.client()
        self.assertEqual(reopened["state"], current["state"])
        self.assertTrue(observation(reopened)["combat"]["dead"])
        self.assertTrue(observation(reopened)["combat"]["terminal"])
        denied = self.act(player, {"type": "wait"})
        self.assertIsNotNone(denied["error"])
        self.assertEqual(denied["state"], reopened["state"])


class RoguePlayableNativeProcesses(RogueChecks, ProcessTestCase):
    graphical = True

    def test_native_mage_f10_requires_exact_form_choice_in_the_real_rogue_package(self):
        self.server(scenario="rogue-exploration", seed=42, character=2)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--actor", "2", "--report-frames"])
        initial = self.ascii_frame(window, lambda frame: frame["has_control"] and not frame["busy"])
        self.assert_starter(initial, "mage")
        native = self.native_keys(window)

        def press(name, reported):
            return self.native_press(window, native, name, reported)

        def command(text):
            self.native_text(window, text)
            self.ascii_frame(window, lambda frame: frame["advancement_command"] == text)
            return press("Return", "enter")

        opened = press("F10", "advancement")
        self.assertIn("Starting skills", " ".join(opened["advancement_rows"]))
        catalog = command("talents")
        while "Evocation Initiate" not in " ".join(catalog["advancement_rows"]):
            self.assertIn("Next available", " ".join(catalog["advancement_rows"]))
            catalog = command("next")
        self.assertIn("spark_lance_cold", " ".join(catalog["advancement_rows"]))
        preview = command("purchase 1 advancement talent evocation_initiate form spark_lance_cold")
        self.assertIn("talent evocation_initiate form spark_lance_cold", " ".join(preview["advancement_rows"]))
        self.assertNotIn("spark_lance_cold", abilities(preview))
        command("confirm")
        observer, current = self.client(observe=True, actor=2)
        self.assertIn("spark_lance_cold", abilities(current))
        self.assertNotIn("spark_lance_fire", abilities(current))
        self.assertEqual(carried_stock(current, "ration"), 1)
        self.assertFalse(current["state"]["wizard_game"])
        command("overview")
        self.assertIn("form spark_lance_cold", " ".join(command("ledger")["advancement_rows"]))
        observer.stop()


if __name__ == "__main__":
    unittest.main()
