"""Normal Rogue preset, advancement and combat through the real applications.

Production presets are used without wizard setup or combat/rules overrides.
The fixed-four stock/traversal/persistence owner explicitly disables only wandering
in a validated derived diagnostic package; it is not current living survival.
Seeded success establishes a content walkthrough, not numerical balance.
"""
from scalar_wire import u64
from collections import Counter
import copy
from contextlib import closing
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess
from tempfile import TemporaryDirectory
import tomllib
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from process_harness import (
    ProcessTestCase, ProcessTestDirectory, ROOT, SPECTATOR_TOKEN, content_ids, hit_die_counts,
    disclosed_exploration_route, settle_rogue_action, underfoot,
)
from arena_matrix import toml_source
from process_work_budget import ProcessOutputBudget, ProcessWorkBudget
from rogue_activation_oracle import reviewed_twenty_six_activation
from rogue_earned_route_process import read_authoritative_evidence
from rogue_magic_receipts import (
    EQUIPMENT_ITEM_STAGE_IDS, CONTROL_ITEM_STAGE_IDS, MAGIC_ITEM_STAGE_IDS,
    assert_equipment_item_receipts, assert_control_item_receipts, assert_magic_item_receipts,
)

from rogue_test_support import (
    DisclosedFloorGraph, assert_room_gold_receipt, disclosed_melee_candidate, disclosed_step_action,
    first_floor_foes, observe_walkthrough_boundaries, walkthrough_search_targets,
)


def observation(frame):
    return frame["state"]["observation"]


def abilities(frame):
    return set(content_ids(observation(frame)["combat"]["own_stats"]["abilities"]))


def carried_stock(frame, supply):
    return sum(int(row["quantity"]) for row in observation(frame)["inventory"]
               if (row.get("known_supply") or {}).get("id") == supply)



def fixed_initial_population_manifest(manifest):
    """Reviewed current26 source, diagnostic-only fixed initial population."""
    actual = json.dumps(manifest.get("floor_activation"), sort_keys=True)
    expected = json.dumps(reviewed_twenty_six_activation(), sort_keys=True)
    if actual != expected:
        raise ValueError("Fixed-initial diagnostic requires exact reviewed current26 population")
    fixture = copy.deepcopy(manifest)
    del fixture["floor_activation"]["wandering"]
    return fixture


def fixture_source_hashes(package):
    """Retain exact authored/certificate bytes, excluding generated compiler cache."""
    return {path.relative_to(package).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(package.rglob("*"))
            if path.is_file() and ".compiled" not in path.relative_to(package).parts}


class FixedInitialPopulationFixtureTests(unittest.TestCase):
    def production(self):
        return tomllib.loads((ROOT / "scenarios/rogue-exploration/scenario.toml").read_text(encoding="utf-8"))

    def test_only_wandering_is_disabled_and_original_production_is_unchanged(self):
        source = self.production()
        original = copy.deepcopy(source)
        self.assertEqual(source["floor_activation"]["wandering"]["living_cap"], 8)
        self.assertEqual(source["floor_activation"]["wandering"]["initial_delay"], [6700, 7300])
        source_hashes = fixture_source_hashes(ROOT / "scenarios/rogue-exploration")
        fixture = fixed_initial_population_manifest(source)
        self.assertEqual(fixture_source_hashes(ROOT / "scenarios/rogue-exploration"), source_hashes)
        self.assertEqual(tomllib.loads(toml_source(fixture)), fixture)
        self.assertEqual(source, original)
        self.assertNotIn("wandering", fixture["floor_activation"])
        restored = copy.deepcopy(fixture)
        restored["floor_activation"]["wandering"] = original["floor_activation"]["wandering"]
        self.assertEqual(restored, original, "No other rule, stock, population or character may change")
        fixture["characters"][0]["name"] = "changed only in returned fixture"
        fixture["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]["count"][0] = 0
        self.assertEqual(source, original, "No mutable alias may escape the transform")

    def test_source_population_drift_is_rejected_before_diagnostic_transform(self):
        source = self.production()
        fixed_initial_population_manifest(source)  # validate positive source first
        for mutation in ("missing_wandering", "cap", "boolean_version", "stock", "actor_count"):
            changed = copy.deepcopy(source)
            activation = changed["floor_activation"]
            if mutation == "missing_wandering":
                del activation["wandering"]
            elif mutation == "cap":
                activation["wandering"]["living_cap"] = 9
            elif mutation == "boolean_version":
                activation["version"] = True
            elif mutation == "stock":
                activation["profiles"]["rogue.low_depth"]["stages"][-1]["quantity"] = 2
            else:
                activation["profiles"]["rogue.low_depth"]["stages"][0]["count"] = [3, 3]
            before = copy.deepcopy(changed)
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                fixed_initial_population_manifest(changed)
            self.assertEqual(changed, before)


class WalkthroughRations:
    """One ordinary meal across all phases; the other ration funds the respec."""
    def __init__(self):
        self.eaten = 0

    def nourish(self, test, player, current, *, prefer_half=False):
        view = observation(current)
        nutrition = view["combat"]["own_stats"]["nutrition"]
        due = (nutrition["hunger"] in ("hungry", "weak", "empty")
               or (prefer_half and self.eaten == 0
                   and nutrition["reserve"] <= nutrition["maximum"] // 2))
        if not due:
            return current
        test.assertEqual(self.eaten, 0, "The walkthrough exceeded its one-ration meal budget")
        before = carried_stock(current, "ration")
        test.assertGreater(before, 1, "The walkthrough must preserve the respec ration")
        edible = {row["item"] for row in view["interactions"]["inventory"] if row["edible"]}
        meals = [item for item in view["inventory"] if item["id"] in edible
                 and (item.get("known_supply") or {}).get("id") == "ration"]
        test.assertEqual(len(meals), 1, "Expected one disclosed carried ration stack")
        key = underfoot(current["state"])["key"]
        result = test.settled_action(player, {"type": "eat", "item": meals[0]["id"]})
        test.assertEqual(carried_stock(result, "ration"), before - 1)
        test.assertGreater(observation(result)["combat"]["own_stats"]["nutrition"]["reserve"],
                           nutrition["reserve"])
        test.assertGreater(int(observation(result)["tick"]), int(view["tick"]))
        test.assertEqual(underfoot(result["state"])["key"], key)
        self.eaten += 1
        return result


def walkthrough_route(at, floor, known_cells, targets, visited, *, search_first=False):
    """Choose this fixture's next ordinary step from disclosed geometry only."""
    # Reuse the existing disclosed BFS, but do not walk every already visible
    # room tile before approaching a place that can reveal more of the floor.
    # Marking floor as visited suppresses only the BFS's fallback for this first
    # lookup, so a reachable disclosed foe/supply retains highest priority.
    route = disclosed_exploration_route(at, floor, targets, floor)
    if route is not None:
        return route
    frontiers = {(x, y) for x, y in floor if any(
        (x + dx, y + dy) not in known_cells
        for dx, dy, _ in DisclosedFloorGraph.directions)}
    return disclosed_exploration_route(at, floor, frontiers, floor if search_first else visited)


class WalkthroughSearch:
    """A single public paid-search budget across exploration phases/reconnects."""
    def __init__(self):
        self.searched = set()
        self.boundaries = {}
        self.scanned_boundaries = set()

    def observe(self, view):
        observe_walkthrough_boundaries(view, self.boundaries)

    def step(self, test, player, current, graph):
        return test.walkthrough_search_step(
            player, current, graph, self.searched,
            boundaries=self.boundaries, scanned_boundaries=self.scanned_boundaries)


def assert_walkthrough_history_anchor(test, frame, saved, *, require_visible_endpoint=False):
    """Tie disclosed history to the exact durable endpoint without revealing it."""
    test.assertEqual(saved["anchor"]["branch"], frame["branch"])
    test.assertEqual(saved["anchor"]["tick"], int(observation(frame)["tick"]))
    test.assertTrue(frame["history"])
    public_ids = [entry["id"] for entry in frame["history"]]
    public_set = set(public_ids)
    durable_ids = [record["entry"]["id"] for record in saved["records"]]
    test.assertEqual(saved["anchor"]["entry_id"], durable_ids[-1])
    # Living but undisclosed actors can append later durable entries. Their
    # history is not the player's public endpoint or a navigation oracle.
    test.assertEqual([identity for identity in durable_ids if identity in public_set], public_ids)
    if require_visible_endpoint:
        test.assertEqual(saved["anchor"]["entry_id"], public_ids[-1])


def seal_walkthrough_transport(budget, client, manifest_path, *, closed_processes=()):
    """Retain a stopped walkthrough transport before rebasing its disk charge.

    Only output storage changes. Requests, actor ticks and every client clock
    remain cumulative; raw emitted bytes equal the retained output charge plus
    walkthrough_raw_byte_adjustment. Extra closed processes contribute only
    their explicit transport logs, never actor clocks. A failed seal never
    replaces the guard.
    """
    manifest_path, directories, explicit_logs = Path(manifest_path), set(), set()
    for process in (client, *closed_processes):
        handles = (process.child.stdin, process.child.stdout, process.child.stderr,
                   process.lines.reader, process.lines.writer)
        if (not process._stopped or process.child.poll() is None or process.reader.is_alive()
                or any(handle is not None and not handle.closed for handle in handles)):
            raise AssertionError("Transport must be stopped with all output handles closed")
        directory = process.diagnostics.resolve()
        if (process.transcript.path.resolve().parent != directory
                or manifest_path.resolve().is_relative_to(directory)):
            raise AssertionError("Seal requires explicit transport directories and a separate manifest")
        directories.add(directory)
        if startup := getattr(process, "startup_path", None):
            if not startup.is_file() or startup.suffix != ".jsonl":
                raise AssertionError("Explicit closed-process startup log is missing")
            explicit_logs.add(startup.resolve())
    if id(client) not in budget.clients:
        raise AssertionError("Seal requires a tracked client and a separate manifest")
    budget.check()
    previous = budget.output

    def digest(stream):
        checksum, length = hashlib.sha256(), 0
        while chunk := stream.read(1024 * 1024):
            checksum.update(chunk)
            length += len(chunk)
        return {"bytes": length, "sha256": checksum.hexdigest()}

    log_paths = explicit_logs | {path for directory in directories for path in directory.rglob("*")
                                if path.is_file() and path.suffix in (".log", ".jsonl")}
    raw = []
    for path in sorted(log_paths):
        with path.open("rb") as stream:
            raw.append({"path": str(path), **digest(stream)})
    if str(client.transcript.path.resolve()) not in {row["path"] for row in raw}:
        raise AssertionError("Stopped client transcript is missing")
    paths = previous.paths + tuple(Path(row["path"]) for row in raw)
    before = ProcessOutputBudget(paths, limit_bytes=previous.limit_bytes)
    before.observed, before.total_bytes = dict(previous.observed), previous.total_bytes
    before.check()  # Include any stopped stderr/spool logs before compression.
    adjustment = getattr(budget, "walkthrough_raw_byte_adjustment", 0)
    manifest = {"format": "tor-rogue-walkthrough-transport-v1", "status": "raw_recorded",
                "raw_logs": raw, "raw_emitted_bytes": before.total_bytes + adjustment,
                "requests": budget.requests, "actor_ticks": budget.actor_ticks,
                "client_tick": budget.clients[id(client)]["tick"]}
    with manifest_path.open("x", encoding="utf-8") as output:
        output.write(json.dumps(manifest, indent=2) + "\n")
    for directory in sorted(directories):
        ProcessTestDirectory.compress_logs(directory)
    # Server startup evidence can live outside its diagnostics directory. Use
    # the same verified primitive on that explicit file, never its whole parent.
    for path in explicit_logs:
        if not any(path.is_relative_to(directory) for directory in directories) and path.stat().st_size > 1024 * 1024:
            ProcessTestDirectory._verified_compress(path, path.with_name(path.name + ".gz"))
            path.unlink()
    retained, replacements = [], {}
    for row in raw:
        source = Path(row["path"])
        compressed = row["bytes"] > 1024 * 1024
        path = source.with_name(source.name + ".gz") if compressed else source
        opener = gzip.open if compressed else open
        with opener(path, "rb") as stream:
            if digest(stream) != {key: row[key] for key in ("bytes", "sha256")}:
                raise AssertionError("Retained transport does not match raw provenance")
        if compressed and source.exists():
            raise AssertionError("Verified archive must replace only its stopped raw log")
        with path.open("rb") as stream:
            retained.append({"source": str(source), "path": str(path), **digest(stream)})
        replacements[source] = path
    after = ProcessOutputBudget(tuple(replacements.get(path.resolve(), path) for path in paths),
                                limit_bytes=previous.limit_bytes)
    for path, prior in before.observed.items():
        if replacements.get(path, path) != path:
            continue
        current = after.observed.get(path)
        if current is None or current[:2] != prior[:2] or current[2] < prior[2]:
            raise AssertionError(f"output_continuity: unsealed output changed: {path}")
    after.total_bytes = sum(value[2] for value in after.observed.values())
    after.check()  # Charge all earlier archives plus all currently active logs.
    adjustment += sum(row["bytes"] - saved["bytes"] for row, saved in zip(raw, retained, strict=True))
    manifest.update(status="retained_verified", retained_logs=retained,
                    retained_output_bytes=after.total_bytes,
                    raw_emitted_bytes=after.total_bytes + adjustment,
                    raw_byte_adjustment=adjustment)
    completed = manifest_path.with_name(manifest_path.name + ".verified")
    with completed.open("x", encoding="utf-8") as output:
        output.write(json.dumps(manifest, indent=2) + "\n")
    completed.replace(manifest_path)
    budget.output = after
    budget.walkthrough_raw_byte_adjustment = adjustment
    return manifest


class WalkthroughRouteTests(unittest.TestCase):
    def corridor(self):
        floor = {(x, 0) for x in range(-1, 4)}
        known = floor | {(x + dx, y + dy) for x, y in floor
                         for dx, dy, _ in DisclosedFloorGraph.directions}
        known.remove((4, 0))
        return floor, known

    def test_disclosed_frontier_precedes_already_visible_room_interior(self):
        floor, known = self.corridor()
        self.assertEqual(walkthrough_route((0, 0), floor, known, set(), {(0, 0)}),
                         [((1, 0), "east"), ((2, 0), "east"), ((3, 0), "east")])

    def test_disclosed_foe_or_supply_precedes_frontier(self):
        floor, known = self.corridor()
        self.assertEqual(walkthrough_route((0, 0), floor, known, {(-1, 0)}, {(0, 0)}),
                         [((-1, 0), "west")])

    def test_unreachable_target_does_not_prevent_disclosed_frontier(self):
        floor, known = self.corridor()
        floor.add((0, 2))
        known.add((0, 2))
        self.assertEqual(walkthrough_route((0, 0), floor, known, {(0, 2)}, {(0, 0)})[0],
                         ((1, 0), "east"))

    def test_unknown_geometry_never_supplies_a_connecting_step(self):
        self.assertIsNone(walkthrough_route((0, 0), {(0, 0), (0, 2)},
                                           {(0, 0), (0, 2)}, {(0, 2)}, {(0, 0)}))

    def test_exhausted_frontiers_retain_bounded_unvisited_cell_fallback(self):
        floor, known = self.corridor()
        known.add((4, 0))
        self.assertEqual(walkthrough_route((0, 0), floor, known, set(), {(0, 0)}),
                         [((-1, 0), "west")])

    def test_search_can_precede_redundant_visible_room_interior_without_changing_default(self):
        floor, known = self.corridor()
        known.add((4, 0))
        self.assertIsNotNone(walkthrough_route((0, 0), floor, known, set(), {(0, 0)}))
        self.assertIsNone(walkthrough_route((0, 0), floor, known, set(), {(0, 0)}, search_first=True))

    def test_search_prefers_new_public_boundary_coverage_without_excluding_other_cells(self):
        graph = SimpleNamespace(edges={key: {} for key in ("room", "wall", "same-wall", "done")})
        boundaries = {"wall": {"north": "opaque-one"}, "same-wall": {"east": "opaque-one"},
                      "done": {"west": "opaque-two"}}
        self.assertEqual(walkthrough_search_targets(graph, {"done"}, boundaries, set()),
                         {"wall", "same-wall"})
        self.assertEqual(walkthrough_search_targets(graph, {"done", "wall"}, boundaries, {"opaque-one"}),
                         {"room", "same-wall"})

    def test_boundary_memory_uses_visible_adjacent_walls_and_clears_revealed_openings(self):
        def cell(key, x, y, wall=False, z=0):
            return {"key": key, "position": {"x": x, "y": y, "z": z}, "wall": wall}

        boundaries = {}
        observe_walkthrough_boundaries({"visible_cells": [cell("room", 0, 0)]}, boundaries)
        self.assertEqual(boundaries, {"room": {}})
        observe_walkthrough_boundaries({"visible_cells": [cell("room", 0, 0),
            cell("edge", 1, 0, True), cell("remote", 3, 0, True),
            cell("above", 0, 1, True, z=1)]}, boundaries)
        self.assertEqual(boundaries, {"room": {"east": "edge"}})
        observe_walkthrough_boundaries({"visible_cells": [cell("room", 0, 0)]}, boundaries)
        self.assertEqual(boundaries, {"room": {"east": "edge"}})
        observe_walkthrough_boundaries({"visible_cells": [cell("room", 0, 0),
            cell("edge", 1, 0)]}, boundaries)
        self.assertEqual(boundaries, {"room": {}, "edge": {}})


class WalkthroughMovementPolicyTests(unittest.TestCase):
    def frame(self, key="here", remaining="0", tick=0, *, hostile=False):
        return {"state": {"observation": {"tick": str(tick),
            "visible_cells": [{"key": key, "position": {"x": 0, "y": 0, "z": 0},
                               "wall": False, "stairs_down": False}],
            "inventory": [], "ground_items": [], "visible_actors": [],
            "combat": {"dead": False, "actors": [{"actor": "new-foe", "hostile": hostile}],
                       "own_stats": {"nutrition": {"reserve": 100, "maximum": 100, "hunger": "sated"},
                                     "conditions": [{"condition": {"id": "rogue.bear_trap"},
                                                     "phase": "active", "remaining_ticks": remaining}]}}}}}

    def graph(self):
        return SimpleNamespace(edges={"here": {}, "destination": {}}, observe=lambda _: None,
                               route_through=lambda *_: [("destination", "east")])

    def test_one_paid_wait_returns_fresh_state_without_replaying_the_move(self):
        driver = RoguePlayableProcesses()
        # An ordinary move remains the valid baseline at this actual caller.
        ordinary, destination = self.frame(), self.frame("destination", tick=100)
        with patch.object(driver, "settled_action", return_value=destination) as action:
            self.assertEqual(driver.disclosed_step(None, ordinary, "east"), (destination, True))
        action.assert_called_once_with(None, {"type": "move", "direction": "east"}, survive=True)
        current, refreshed = self.frame(remaining="200"), self.frame(remaining="100", tick=100)
        with patch.object(driver, "settled_action", return_value=refreshed) as action:
            self.assertEqual(driver.disclosed_step(None, current, "east"), (refreshed, False))
        action.assert_called_once_with(None, {"type": "wait"}, survive=True)

    def test_revisit_waits_until_expiry_and_still_visits_the_required_key(self):
        driver = RoguePlayableProcesses()
        current = self.frame(remaining="200")
        after = [self.frame(remaining="100", tick=100), self.frame(tick=200),
                 self.frame("destination", tick=300)]
        with patch.object(driver, "settled_action", side_effect=after) as action:
            result, eaten = driver.revisit_cleared_floor(
                None, current, {}, {"destination"}, {"here"}, self.graph())
        self.assertEqual((result, eaten), (after[-1], 0))
        self.assertEqual([call.args[1] for call in action.call_args_list],
                         [{"type": "wait"}, {"type": "wait"}, {"type": "move", "direction": "east"}])

    def test_revisit_reobserves_new_hostility_before_any_cached_step(self):
        driver = RoguePlayableProcesses()
        with patch.object(driver, "settled_action", return_value=self.frame(tick=100, hostile=True)) as action:
            with self.assertRaisesRegex(AssertionError, "gained a hostile"):
                driver.revisit_cleared_floor(None, self.frame(remaining="100"), {},
                                             {"destination"}, {"here"}, self.graph())
        action.assert_called_once_with(None, {"type": "wait"}, survive=True)

    def test_unending_restraint_exhausts_the_same_paid_action_bound(self):
        driver = RoguePlayableProcesses()
        current = self.frame(remaining="100")
        with patch.object(driver, "settled_action", return_value=current) as action:
            with self.assertRaisesRegex(AssertionError, "2500 paid movement-action bound"):
                driver.revisit_cleared_floor(None, current, {}, {"destination"}, {"here"}, self.graph())
        self.assertEqual(action.call_count, 2500)
        self.assertTrue(all(call.args[1] == {"type": "wait"} for call in action.call_args_list))

    def test_revisit_uses_checkpoint_replacement_client_and_current_frame(self):
        driver = RoguePlayableProcesses()
        original, replacement = object(), object()
        current = self.frame()
        refreshed = self.frame(remaining="100", tick=100)
        released, arrived = self.frame(tick=200), self.frame("destination", tick=300)
        calls = []

        def checkpoint(client, frame):
            calls.append((client, frame))
            return (replacement, refreshed) if len(calls) == 1 else (client, frame)

        with patch.object(driver, "settled_action", side_effect=[released, arrived]) as action:
            result, eaten = driver.revisit_cleared_floor(
                original, current, {}, {"destination"}, {"here"}, self.graph(),
                checkpoint=checkpoint)
        self.assertEqual((result, eaten), (arrived, 0))
        self.assertEqual(calls, [(original, current), (replacement, released)])
        self.assertEqual([call.args for call in action.call_args_list],
                         [(replacement, {"type": "wait"}),
                          (replacement, {"type": "move", "direction": "east"})])

    def test_exhausted_route_pays_search_then_opens_and_replans_a_new_door(self):
        from copy import deepcopy
        driver, searched, graph = RoguePlayableProcesses(), set(), DisclosedFloorGraph()
        current = self.frame()
        current["state"]["observation"]["self_target"] = "player"
        graph.observe(current["state"])
        found = deepcopy(current)
        found["state"]["observation"]["tick"] = "100"
        found["state"]["observation"]["visible_cells"].append({
            "key": "gate", "position": {"x": 1, "y": 0, "z": 0}, "wall": False,
            "stairs_down": False, "door": {"id": "new-door", "open": False, "reachable": True}})
        opened = deepcopy(found)
        opened["state"]["observation"]["tick"] = "200"
        opened["state"]["observation"]["visible_cells"][1]["door"]["open"] = True
        arrived = self.frame("gate", tick=300)
        with patch.object(driver, "settled_action", side_effect=[found, opened, arrived]) as action, \
                patch.object(driver, "request") as query:
            current, moved = driver.walkthrough_search_step(None, current, graph, searched)
            self.assertEqual((current, moved, searched), (found, None, {"here"}))
            graph.observe(current["state"])
            current, moved = driver.walkthrough_search_step(None, current, graph, searched)
            self.assertEqual((current, moved), (opened, None))
            current, moved = driver.walkthrough_search_step(None, current, graph, searched)
            self.assertEqual((current, moved), (arrived, "east"))
            query.assert_not_called()
        self.assertEqual([call.args[1] for call in action.call_args_list], [
            {"type": "use_ability", "id": "rogue.search", "invocation": {
                "aim": {"kind": "actor_target", "target": "player"}, "weapon": {"kind": "default"}},
             "supplies": {"kind": "carried"}},
            {"type": "set_door", "door": "new-door", "open": True},
            {"type": "move", "direction": "east"}])

    def test_failed_search_has_one_paid_attempt_per_cell_even_after_client_replacement(self):
        driver, searched, graph = RoguePlayableProcesses(), set(), DisclosedFloorGraph()
        current, unchanged = self.frame(), self.frame(tick=100)
        current["state"]["observation"]["self_target"] = "player"
        graph.observe(current["state"])
        with patch.object(driver, "settled_action", return_value=unchanged) as action:
            self.assertEqual(driver.walkthrough_search_step(object(), current, graph, searched),
                             (unchanged, None))
            with self.assertRaisesRegex(AssertionError, "paid Search sweep exhausted"):
                driver.walkthrough_search_step(object(), unchanged, graph, searched)
        self.assertEqual(action.call_count, 1)

    def test_stair_approach_reuses_paid_search_then_opens_and_replans_after_reconnect(self):
        from copy import deepcopy
        driver, search, graph = RoguePlayableProcesses(), WalkthroughSearch(), DisclosedFloorGraph()
        original, replacement = object(), object()
        current = self.frame()
        current["state"]["observation"]["self_target"] = "player"
        found = deepcopy(current)
        found["state"]["observation"]["tick"] = "100"
        found["state"]["observation"]["visible_cells"].append({
            "key": "gate", "position": {"x": 1, "y": 0, "z": 0}, "wall": False,
            "stairs_down": True, "door": {"id": "stair-door", "open": False, "reachable": True}})
        opened = deepcopy(found)
        opened["state"]["observation"]["tick"] = "200"
        opened["state"]["observation"]["visible_cells"][1]["door"]["open"] = True
        arrived = self.frame("gate", tick=300)
        arrived["state"]["observation"]["visible_cells"][0]["stairs_down"] = True

        def checkpoint(player, frame):
            return (replacement if search.searched else player), frame

        with patch.object(driver, "settled_action", side_effect=[found, opened, arrived]) as action:
            self.assertEqual(driver.walk_to_stairs(
                original, current, graph, checkpoint=checkpoint, search=search), arrived)
        self.assertEqual(search.searched, {"here"})
        self.assertEqual([call.args[0] for call in action.call_args_list],
                         [original, replacement, replacement])
        self.assertEqual([call.args[1]["type"] for call in action.call_args_list],
                         ["use_ability", "set_door", "move"])

    def test_stair_phase_cannot_reset_a_search_already_paid_during_clearing(self):
        driver, search, graph = RoguePlayableProcesses(), WalkthroughSearch(), DisclosedFloorGraph()
        current, unchanged = self.frame(), self.frame(tick=100)
        current["state"]["observation"]["self_target"] = "player"
        graph.observe(current["state"])
        search.observe(observation(current))
        with patch.object(driver, "settled_action", return_value=unchanged) as action:
            self.assertEqual(search.step(driver, object(), current, graph), (unchanged, None))
            with self.assertRaisesRegex(AssertionError, "paid Search sweep exhausted"):
                driver.walk_to_stairs(object(), unchanged, graph, search=search)
        self.assertEqual(action.call_count, 1)

    def test_one_publicly_needed_meal_spans_walkthrough_phases_and_preserves_respec_stock(self):
        from copy import deepcopy
        driver, meals = RoguePlayableProcesses(), WalkthroughRations()
        current = self.frame()
        view = current["state"]["observation"]
        view["inventory"] = [{"id": "food", "quantity": "2", "known_supply": {"id": "ration"}}]
        view["interactions"] = {"inventory": [{"item": "food", "edible": True}]}
        with patch.object(driver, "settled_action") as action:
            self.assertIs(meals.nourish(driver, None, current), current)
            action.assert_not_called()
        hungry = deepcopy(current)
        hungry["state"]["observation"]["combat"]["own_stats"]["nutrition"].update(
            reserve=20, hunger="hungry")
        fed = deepcopy(current)
        fed["state"]["observation"]["inventory"][0]["quantity"] = "1"
        fed["state"]["observation"]["tick"] = "100"
        with patch.object(driver, "settled_action", return_value=fed) as action:
            self.assertIs(meals.nourish(driver, object(), hungry), fed)
            self.assertEqual(meals.eaten, 1)
            # The same budget object crosses controller/transport phases.
            with self.assertRaisesRegex(AssertionError, "one-ration meal budget"):
                meals.nourish(driver, object(), hungry)
        self.assertEqual(action.call_count, 1)
        self.assertEqual(action.call_args.args[1], {"type": "eat", "item": "food"})
        self.assertEqual(carried_stock(fed, "ration") + meals.eaten, 2)
        last = deepcopy(hungry)
        last["state"]["observation"]["inventory"][0]["quantity"] = "1"
        with patch.object(driver, "settled_action") as action:
            with self.assertRaisesRegex(AssertionError, "preserve the respec ration"):
                WalkthroughRations().nourish(driver, None, last)
            action.assert_not_called()


class WalkthroughTransportTests(unittest.TestCase):
    """Pure local file fixtures; never launch a process or compile a package."""
    def test_active_floor_anchor_accepts_hidden_tail_but_keeps_visible_subsequence(self):
        from copy import deepcopy
        frame = {"branch": "same", "state": {"observation": {"tick": "300"}},
                 "history": [{"id": "visible-one"}, {"id": "visible-two"}]}
        saved = {"anchor": {"branch": "same", "tick": 300, "entry_id": "visible-two"},
                 "records": [{"entry": {"id": key}} for key in
                             ["earlier", "visible-one", "hidden-middle", "visible-two"]]}
        assert_walkthrough_history_anchor(self, frame, saved, require_visible_endpoint=True)
        hidden_tail = deepcopy(saved)
        hidden_tail["records"].append({"entry": {"id": "hidden-later"}})
        hidden_tail["anchor"]["entry_id"] = "hidden-later"
        assert_walkthrough_history_anchor(self, frame, hidden_tail)
        with self.assertRaises(AssertionError):
            assert_walkthrough_history_anchor(self, frame, hidden_tail, require_visible_endpoint=True)
        for history in ([{"id": "missing"}], list(reversed(frame["history"])),
                        [frame["history"][0]] * 2):
            with self.subTest(history=history), self.assertRaises(AssertionError):
                assert_walkthrough_history_anchor(self, {**frame, "history": history}, hidden_tail)
        for anchor in ({"branch": "other"}, {"tick": 301}, {"entry_id": "missing"}):
            malformed = deepcopy(hidden_tail)
            malformed["anchor"].update(anchor)
            with self.subTest(anchor=anchor), self.assertRaises(AssertionError):
                assert_walkthrough_history_anchor(self, frame, malformed)

    def test_playable_opening_returns_without_submitting_the_old_move(self):
        from copy import deepcopy
        driver = RoguePlayableProcesses()
        current = {"state": {"observation": {
            "visible_cells": [
                {"key": "here", "position": {"x": 0, "y": 0, "z": 0}, "wall": False},
                {"key": "gate", "position": {"x": 1, "y": 0, "z": 0}, "wall": False,
                 "door": {"id": "observed-gate", "open": False, "reachable": True}}],
            "combat": {"dead": False}}}}
        refreshed = deepcopy(current)
        refreshed["state"]["observation"]["visible_cells"][1]["door"]["open"] = True
        with patch.object(driver, "settled_action", return_value=refreshed) as action:
            self.assertEqual(driver.disclosed_step(None, current, "east"), (refreshed, False))
        action.assert_called_once_with(None,
            {"type": "set_door", "door": "observed-gate", "open": True}, survive=True)

    def setUp(self):
        directory = TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        # Exercise canonical-path handling without symlink privileges on any OS.
        spelling = Path(directory.name) / "path-spelling"
        spelling.mkdir()
        self.root = spelling / ".."
        self.assertNotEqual(self.root, self.root.resolve())
        self.budget = ProcessWorkBudget(requests=20, actor_ticks=100,
                                       transcript_bytes=8 * 1024 * 1024)

    def client(self, name, data=b"frame\n" * 180000, *, tracked=True):
        directory = self.root / name
        directory.mkdir()
        path = directory / "stdout.log"
        path.write_bytes(data)
        def closed():
            return SimpleNamespace(closed=True)
        client = SimpleNamespace(diagnostics=directory, _stopped=True,
            child=SimpleNamespace(poll=lambda: 0, stdin=closed(), stdout=closed(), stderr=None),
            reader=SimpleNamespace(is_alive=lambda: False),
            lines=SimpleNamespace(reader=closed(), writer=closed()),
            transcript=SimpleNamespace(path=path))
        if tracked:
            self.budget.track(client, self.frame(10))
        return client

    @staticmethod
    def frame(tick):
        return {"state": {"observation": {"tick": str(tick), "combat": {}}}}

    def test_seal_retains_verified_bytes_and_preserves_cumulative_work(self):
        client = self.client("stopped")
        active = self.client("active", b"active\n")
        original = client.transcript.path.read_bytes()
        self.budget.before()
        self.budget.after(client, self.frame(17))
        clocks = {key: dict(value) for key, value in self.budget.clients.items()}
        manifest = self.root / "seal.json"
        result = seal_walkthrough_transport(self.budget, client, manifest)
        self.assertEqual(json.loads(manifest.read_text()), result)
        self.assertEqual(result["status"], "retained_verified")
        retained = result["retained_logs"][0]
        archive = Path(retained["path"])
        self.assertEqual(gzip.decompress(archive.read_bytes()), original)
        self.assertEqual(retained["sha256"], hashlib.sha256(archive.read_bytes()).hexdigest())
        self.assertEqual(result["raw_logs"][0]["sha256"], hashlib.sha256(original).hexdigest())
        self.assertEqual(result["raw_emitted_bytes"], len(original) + 7)
        self.assertEqual(self.budget.usage()["transcript_bytes"], archive.stat().st_size + 7)
        self.assertEqual((self.budget.requests, self.budget.actor_ticks), (1, 7))
        self.assertEqual(self.budget.clients, clocks)
        with active.transcript.path.open("ab") as output:
            output.write(b"more\n")
        self.budget.after(active, self.frame(13))
        self.assertEqual(self.budget.actor_ticks, 10)
        self.assertEqual(self.budget.usage()["transcript_bytes"]
                         + self.budget.walkthrough_raw_byte_adjustment, len(original) + 12)

    def test_second_seal_keeps_prior_archives_in_cap_and_continuity_checks(self):
        first, second = self.client("first"), self.client("second")
        one = seal_walkthrough_transport(self.budget, first, self.root / "one.json")
        two = seal_walkthrough_transport(self.budget, second, self.root / "two.json")
        retained = sum(Path(row["path"]).stat().st_size
                       for result in (one, two) for row in result["retained_logs"])
        self.assertEqual(self.budget.usage()["transcript_bytes"], retained)
        self.assertEqual(two["raw_emitted_bytes"], 2 * 1080000)
        active = self.client("active", b"a")
        self.budget.limits["transcript_bytes"] = retained + 1
        self.budget.output.limit_bytes = retained + 1
        with active.transcript.path.open("ab") as output:
            output.write(b"b")
        with self.assertRaisesRegex(AssertionError, "transcript_bytes"):
            self.budget.check()
        Path(one["retained_logs"][0]["path"]).unlink()
        with self.assertRaisesRegex(AssertionError, "output_continuity"):
            self.budget.check()

    def test_small_raw_and_separate_stderr_logs_stay_retained_and_charged(self):
        client = self.client("stopped", b"raw\n")
        stderr = client.diagnostics / "stderr.log"
        stderr.write_bytes(b"error\n")
        result = seal_walkthrough_transport(self.budget, client, self.root / "seal.json")
        self.assertEqual(result["raw_emitted_bytes"], 10)
        self.assertEqual(self.budget.usage()["transcript_bytes"], 10)
        self.assertEqual(self.budget.walkthrough_raw_byte_adjustment, 0)
        self.assertEqual({row["path"] for row in result["retained_logs"]},
                         {str(client.transcript.path.resolve()), str(stderr.resolve())})
        self.assertEqual(client.transcript.path.read_bytes(), b"raw\n")
        self.assertEqual(stderr.read_bytes(), b"error\n")

    def test_extra_closed_server_logs_are_retained_without_actor_clock(self):
        client = self.client("client", b"client\n")
        server = self.client("server", tracked=False)
        stderr = server.diagnostics / "stderr.log"
        stderr.write_bytes(b"server error\n")
        server.startup_path = self.root / "startup-00.jsonl"
        startup = b"startup\n" * 140000
        server.startup_path.write_bytes(startup)
        clocks = {key: dict(value) for key, value in self.budget.clients.items()}
        result = seal_walkthrough_transport(self.budget, client, self.root / "seal.json",
                                            closed_processes=(server,))
        self.assertEqual(self.budget.clients, clocks)
        self.assertNotIn(id(server), self.budget.clients)
        self.assertEqual(result["raw_emitted_bytes"], 7 + 1080000 + 13 + len(startup))
        self.assertEqual({row["path"] for row in result["raw_logs"]},
                         {str(client.transcript.path.resolve()), str(server.transcript.path.resolve()),
                          str(stderr.resolve()), str(server.startup_path.resolve())})
        retained = sum(Path(row["path"]).stat().st_size for row in result["retained_logs"])
        self.assertEqual(self.budget.usage()["transcript_bytes"], retained)
        for row in result["retained_logs"]:
            path = Path(row["path"])
            self.assertEqual(row["sha256"], hashlib.sha256(path.read_bytes()).hexdigest())
            if row["source"] == str(server.startup_path.resolve()):
                self.assertEqual(gzip.decompress(path.read_bytes()), startup)

    def test_live_extra_server_rejects_before_any_log_or_budget_change(self):
        client = self.client("client")
        server = self.client("server", tracked=False)
        server._stopped = False
        previous = self.budget.output
        manifest = self.root / "seal.json"
        with patch.object(ProcessTestDirectory, "compress_logs") as compress:
            with self.assertRaisesRegex(AssertionError, "stopped.*closed"):
                seal_walkthrough_transport(self.budget, client, manifest, closed_processes=(server,))
            compress.assert_not_called()
        self.assertIs(self.budget.output, previous)
        self.assertTrue(client.transcript.path.exists())
        self.assertTrue(server.transcript.path.exists())
        self.assertFalse(manifest.exists())

    def test_preexisting_truncation_is_rejected_before_compression_or_manifest(self):
        client = self.client("stopped")
        previous = self.budget.output
        client.transcript.path.write_bytes(b"short")
        manifest = self.root / "seal.json"
        with patch.object(ProcessTestDirectory, "compress_logs") as compress:
            with self.assertRaisesRegex(AssertionError, "output_continuity"):
                seal_walkthrough_transport(self.budget, client, manifest)
            compress.assert_not_called()
        self.assertIs(self.budget.output, previous)
        self.assertFalse(manifest.exists())

    def test_live_process_reader_or_open_output_handle_cannot_seal(self):
        for kind in ("running", "thread", "stdout", "stdin", "stderr", "reader", "writer", "not_stopped"):
            with self.subTest(kind=kind):
                client = self.client(kind, b"raw")
                if kind == "running": client.child.poll = lambda: None
                elif kind == "thread": client.reader.is_alive = lambda: True
                elif kind == "not_stopped": client._stopped = False
                elif kind == "stderr": client.child.stderr = SimpleNamespace(closed=False)
                elif kind in ("stdin", "stdout"): getattr(client.child, kind).closed = False
                else: getattr(client.lines, kind).closed = False
                with self.assertRaisesRegex(AssertionError, "stopped.*closed"):
                    seal_walkthrough_transport(self.budget, client, self.root / f"{kind}.json")
                self.assertTrue(client.transcript.path.exists())
                self.assertFalse((self.root / f"{kind}.json").exists())

    def test_compression_failure_preserves_raw_manifest_and_does_not_rebase(self):
        client = self.client("stopped")
        previous = self.budget.output
        manifest = self.root / "seal.json"
        with patch.object(ProcessTestDirectory, "compress_logs", side_effect=OSError("compression failed")):
            with self.assertRaisesRegex(OSError, "compression failed"):
                seal_walkthrough_transport(self.budget, client, manifest)
        self.assertEqual(json.loads(manifest.read_text())["status"], "raw_recorded")
        self.assertTrue(client.transcript.path.exists())
        self.assertIs(self.budget.output, previous)
        self.assertFalse(hasattr(self.budget, "walkthrough_raw_byte_adjustment"))

    def test_wrong_compressed_payload_is_rejected_before_rebase(self):
        client = self.client("stopped")
        previous = self.budget.output
        def corrupt(directory):
            self.assertEqual(directory, client.diagnostics.resolve())
            archive = client.transcript.path.with_suffix(".log.gz")
            archive.write_bytes(gzip.compress(b"wrong"))
            client.transcript.path.unlink()
        with patch.object(ProcessTestDirectory, "compress_logs", side_effect=corrupt):
            with self.assertRaisesRegex(AssertionError, "raw provenance"):
                seal_walkthrough_transport(self.budget, client, self.root / "seal.json")
        self.assertIs(self.budget.output, previous)
        self.assertEqual(json.loads((self.root / "seal.json").read_text())["status"], "raw_recorded")

    def test_active_log_truncation_during_compression_cannot_be_rebased_away(self):
        client, active = self.client("stopped"), self.client("active", b"active")
        previous = self.budget.output
        compress = ProcessTestDirectory.compress_logs
        def truncate(directory):
            compress(directory)
            active.transcript.path.write_bytes(b"")
        with patch.object(ProcessTestDirectory, "compress_logs", side_effect=truncate):
            with self.assertRaisesRegex(AssertionError, "output_continuity"):
                seal_walkthrough_transport(self.budget, client, self.root / "seal.json")
        self.assertIs(self.budget.output, previous)

    def test_completed_manifest_failure_cannot_rebase_budget(self):
        client = self.client("stopped")
        previous = self.budget.output
        with patch.object(Path, "replace", side_effect=OSError("manifest failed")):
            with self.assertRaisesRegex(OSError, "manifest failed"):
                seal_walkthrough_transport(self.budget, client, self.root / "seal.json")
        self.assertIs(self.budget.output, previous)
        self.assertEqual(json.loads((self.root / "seal.json").read_text())["status"], "raw_recorded")


class RogueChecks:
    def advance(self, player, command):
        frame = self.command(player, {"type": "advancement", "command": command})
        self.assertIsNone(frame["error"], command)
        if (command.startswith("confirm") or command == "resume") and any(
                row["phase"] == "queued" for row in frame["intentions"]):
            frame = self.frame(player, lambda current: not current.get("state_omitted", False) and not any(
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

    def disclosed_step(self, player, current, direction, *, survive=True):
        """Do one paid wait, opening or move, then return the fresh observation."""
        action = disclosed_step_action(observation(current), direction, wait_for_restraint=True)
        origin = underfoot(current["state"])["key"]
        result = self.settled_action(player, action, survive=survive)
        moved = action["type"] == "move"
        if not moved and not observation(result)["combat"]["dead"]:
            self.assertEqual(underfoot(result["state"])["key"], origin,
                             "A paid restraint wait or opening must not move the actor")
        return result, moved

    def walkthrough_search_step(self, player, current, floor_graph, searched, *,
                                boundaries=None, scanned_boundaries=None):
        """An exhausted public route gets at most one paid Search per reached cell."""
        here = underfoot(current["state"])["key"]
        remaining = floor_graph.edges.keys() - searched
        targets = (remaining if boundaries is None else walkthrough_search_targets(
            floor_graph, searched, boundaries, scanned_boundaries))
        route = floor_graph.route_to_any(here, targets)
        if route is None and targets != remaining:
            route = floor_graph.route_to_any(here, remaining)
        self.assertIsNotNone(route, "The disclosed-floor paid Search sweep exhausted its cells")
        if not route:
            searched.add(here)
            if boundaries is not None:
                scanned_boundaries.update(boundaries.get(here, {}).values())
            view = observation(current)
            result = self.settled_action(player, {
                "type": "use_ability", "id": "rogue.search", "invocation": {
                    "aim": {"kind": "actor_target", "target": view["self_target"]},
                    "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}})
            self.assertGreater(int(observation(result)["tick"]), int(view["tick"]))
            self.assertEqual(underfoot(result["state"])["key"], here)
            return result, None
        key, direction = route[0]
        result, moved = self.disclosed_step(player, current, direction)
        if moved:
            self.assertEqual(underfoot(result["state"])["key"], key)
        return result, direction if moved else None

    def checkpoint_walkthrough_transport(self, player, server, budget, label, *, closed_processes=(),
                                         require_visible_endpoint=False):
        """Losslessly seal stopped transport, then verify exact saved authority.

        This runs only between settled driver actions. Saved evidence is an
        equality oracle here, never an input to navigation or combat decisions.
        """
        before = self.request(player, {"type": "snapshot"})
        self.flush_save()
        saved = read_authoritative_evidence(self.save, 1)
        assert_walkthrough_history_anchor(self, before, saved,
                                         require_visible_endpoint=require_visible_endpoint)
        (self.directory / f"{label}-authority.json").write_text(
            json.dumps(saved, sort_keys=True) + "\n", encoding="utf-8")
        server.stop(save=False)
        player.stop()
        seal_walkthrough_transport(budget, player, self.directory / f"{label}-seal.json",
                                   closed_processes=(server, *closed_processes))
        server = self.server("--checkpoint-interval", 1, seed=None)
        player, reopened = self.client(omit_memory=True)
        for field in ("state", "history", "branch"):
            self.assertEqual(reopened[field], before[field])
        # Sequence is attachment-local; simulation tick spans reconnects.
        self.assertEqual(reopened["cursor"]["tick"], before["cursor"]["tick"])
        (self.directory / f"{label}-cursors.json").write_text(
            json.dumps({"before": before["cursor"], "reopened": reopened["cursor"]}) + "\n",
            encoding="utf-8")
        self.flush_save()
        self.assertEqual(read_authoritative_evidence(self.save, 1), saved,
                         "Transport rollover changed exact durable authority")
        return server, player, reopened

    def clear_first_floor(self, player, *, checkpoint=None, meals=None, search=None):
        """Choose the first talent, then fight and collect through ordinary play."""
        growth = self.overview(player)
        self.assertEqual(growth["points"], {"earned": "1", "spent": "0", "unspent": "1"})
        preview = self.advance(player, "purchase 1 advancement talent rattling_blow")
        self.assertNotIn("rattling_blow", abilities(preview))
        self.advance(player, "confirm")
        current = self.request(player, {"type": "snapshot"})
        self.assertIn("rattling_blow", abilities(current))
        at, visited, floor, stock_cells = (0, 0), set(), set(), set()
        known_cells = set()
        search = search if search is not None else WalkthroughSearch()
        seen_foes = {}
        defeated = {}
        collected = Counter()
        collected_cells = set()
        cleared_cells = set()
        floor_graph = DisclosedFloorGraph()
        # The bound is the same disclosed-cell exploration budget as the floor
        # harness. It is a failure deadline, never an ignored adverse seed.
        for _ in range(2500):
            if checkpoint is not None:
                player, current = checkpoint(player, current)
            view = observation(current)
            visited.add(at)
            floor_graph.observe(current["state"])
            search.observe(view)
            cleared_cells.add(underfoot(current["state"])["key"])
            hostile = {row["actor"] for row in view["combat"]["actors"] if row["hostile"]}
            foes = [row for row in view["visible_actors"] if row["id"] in hostile]
            seen_foes.update((row["id"], row["name"]) for row in foes)
            for cell in view["visible_cells"]:
                position = cell["position"]
                if position["z"] == 0:
                    point = (at[0] + position["x"], at[1] + position["y"])
                    known_cells.add(point)
                    if not cell["wall"]:
                        floor.add(point)
            for row in view["ground_items"]:
                if row["item"].get("known_supply"):
                    position = row["position"]
                    stock_cells.add((at[0] + position["x"], at[1] + position["y"]))
            nearby = next((row for row in foes if disclosed_melee_candidate(view, row)), None)
            if nearby:
                before, start = current, len(player.transcript)
                earned_before = int(growth["growth"]["earned_xp"])
                current = self.settled_action(player, {"type": "use_ability", "id": "basic_melee",
                                                      "invocation": {"aim": {"kind": "actor_target", "target": nearby["id"]}, "weapon": {"kind": "default"}}})
                # Only a lethal attack can add XP in this walkthrough. Walking
                # and taking finite stock do not need another private page.
                growth = self.overview(player)
                events = []
                for line in player.transcript[start:]:
                    if not line.startswith("{"):
                        continue
                    state = json.loads(line).get("state")
                    # Acknowledgements can repeat an earlier turn's events.
                    if state is not None and int(state["revision"]) > int(before["state"]["revision"]):
                        events.extend(state["observation"]["combat"]["events"])
                died = any(event["type"] == "died" and event["actor"] == nearby["id"]
                           for event in events)
                earned_after = int(growth["growth"]["earned_xp"])
                if died:
                    self.assertNotIn(nearby["id"], defeated, "A kill cannot grant XP twice")
                    self.assertTrue(any(event["type"] == "ability"
                        and event["caster"] == view["self_target"]
                        and event["target"] == nearby["id"]
                        and event["ability"]["id"] == "basic_melee"
                        and event["outcome"] == "applied" for event in events),
                        "Each defeat must follow a disclosed ordinary applied attack")
                    self.assertGreater(earned_after, earned_before)
                    defeated[nearby["id"]] = {"name": nearby["name"],
                                              "earned_xp": earned_after - earned_before}
                else:
                    self.assertEqual(earned_after, earned_before)
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
            if meals is not None:
                fed = meals.nourish(self, player, current)
                if fed is not current:
                    current = fed
                    continue
            # This positive seed must actually defeat four disclosed foes. XP
            # depends on the rolled production roster and is never a route oracle.
            if len(defeated) == 4 and collected == {"bandage": 2, "ration": 1}:
                break
            targets = set(stock_cells)
            for foe in foes:
                position = foe["position"]
                targets.add((at[0] + position["x"], at[1] + position["y"]))
            route = walkthrough_route(at, floor, known_cells, targets, visited, search_first=True)
            if route is None:
                # A failed passive check is not proof the reachable floor is
                # empty. Search only from disclosed cells, once per reached key;
                # failed checks and all travel still spend this same action bound.
                current, direction = search.step(self, player, current, floor_graph)
                if direction is not None:
                    dx, dy, _ = next(row for row in DisclosedFloorGraph.directions
                                     if row[2] == direction)
                    at = (at[0] + dx, at[1] + dy)
                continue
            target, direction = route[0]
            before = underfoot(current["state"])["key"]
            current, moved = self.disclosed_step(player, current, direction)
            if not moved:
                continue
            self.assertNotEqual(underfoot(current["state"])["key"], before)
            at = target
        else:
            self.fail("The production Rogue floor did not complete its combat/stock walkthrough")
        # Movement or looting after the final hit invalidates owner context.
        # Refresh once for the caller's next advancement operation.
        growth = self.overview(player)
        self.assertEqual(set(seen_foes), set(defeated))
        self.assertEqual(int(growth["growth"]["earned_xp"]),
                         sum(row["earned_xp"] for row in defeated.values()))
        print("Fixed-initial diagnostic earned combat: " + json.dumps({
            "seed": 42, "defeated": list(defeated.values()), "growth": growth["growth"]}, sort_keys=True),
            flush=True)
        return current, growth, seen_foes, collected_cells, cleared_cells, floor_graph

    def walk_to_stairs(self, player, current, floor_graph=None, *, checkpoint=None, meals=None,
                       search=None):
        """Head for disclosed stairs, defending without a full-floor loot tour."""
        floor_graph = floor_graph or DisclosedFloorGraph()
        visited = set()
        for _ in range(2500):
            if checkpoint is not None:
                player, current = checkpoint(player, current)
            view = observation(current)
            floor_graph.observe(current["state"])
            if search is not None:
                search.observe(view)
            here = underfoot(current["state"])
            visited.add(here["key"])
            if here["stairs_down"]:
                return current
            hostile = {row["actor"] for row in view["combat"]["actors"] if row["hostile"]}
            nearby = next((row for row in view["visible_actors"]
                           if row["id"] in hostile and disclosed_melee_candidate(view, row)), None)
            if nearby:
                current = self.settled_action(player, {"type": "use_ability", "id": "basic_melee",
                    "invocation": {"aim": {"kind": "actor_target", "target": nearby["id"]}, "weapon": {"kind": "default"}}})
                continue
            if meals is not None:
                fed = meals.nourish(self, player, current)
                if fed is not current:
                    current = fed
                    continue
            route = floor_graph.route_to_any(here["key"], floor_graph.stairs_down)
            if route is None:
                route = floor_graph.route_to_any(here["key"], floor_graph.edges.keys() - visited)
            if route is None and search is not None:
                current, _ = search.step(self, player, current, floor_graph)
                continue
            self.assertTrue(route, "No disclosed route to stairs or an unexplored approach")
            key, direction = route[0]
            current, moved = self.disclosed_step(player, current, direction)
            if not moved:
                continue
            self.assertEqual(underfoot(current["state"])["key"], key,
                             "Disclosed stair-approach step did not reach its observed key")
        self.fail("The ordinary stair approach exceeded its 2500-action bound")

    def revisit_cleared_floor(self, player, current, seen_foes, collected_cells, cleared_cells, floor_graph,
                              *, checkpoint=None, meals=None):
        """Revisit every recorded combat-walkthrough and collected-supply key."""
        initial_rations = carried_stock(current, "ration")
        meals = meals if meals is not None else WalkthroughRations()
        initial_eaten = meals.eaten
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
        movement_actions = 0

        def audit_view():
            view = observation(current)
            visited_keys.add(underfoot(current["state"])["key"])
            self.assertFalse(set(seen_foes).intersection(row["id"] for row in view["visible_actors"]),
                             "A defeated first-floor foe reappeared")
            self.assertFalse(any(row["hostile"] for row in view["combat"]["actors"]),
                             "The cleared first floor gained a hostile")
            self.assertFalse(any(row["item"].get("known_supply") for row in view["ground_items"]),
                             "Collected first-floor supply stock reappeared")

        for step in [None, *route]:
            if step is not None:
                current = meals.nourish(self, player, current, prefer_half=True)
                audit_view()  # A paid meal can disclose a changed situation.
                key, direction = step
                while True:
                    if checkpoint is not None:
                        player, current = checkpoint(player, current)
                    self.assertLess(movement_actions, 2500,
                                    "The revisit exceeded its 2500 paid movement-action bound")
                    movement_actions += 1
                    current, moved = self.disclosed_step(player, current, direction)
                    # A wait/open is a full paid action, not permission to replay
                    # the cached move. Check the fresh disclosure before choosing
                    # again; a changed condition or hostile must not be ignored.
                    audit_view()
                    if moved:
                        break
                self.assertEqual(underfoot(current["state"])["key"], key,
                                 "The revisit did not reach its recorded disclosed key")
            audit_view()
        self.assertTrue(collected_cells.issubset(visited_keys),
                        "The revisit did not reach every collected supply cell")
        self.assertTrue(cleared_cells.issubset(visited_keys),
                        "The revisit did not retrace every combat-walkthrough cell")
        self.assertEqual(carried_stock(current, "ration") + meals.eaten, initial_rations + initial_eaten)
        return current, meals.eaten

    def assert_rogue_population(self, receipt, scope, cause):
        self.assertEqual(receipt["scope"], scope)
        self.assertEqual(receipt["entrant"], 1)
        self.assertEqual(receipt["cause"], cause)
        self.assertEqual(receipt["profile"], "rogue.low_depth")
        self.assertEqual(receipt["profile_version"], 1)
        depth = int(scope.removeprefix("dungeon/"))
        manifest = tomllib.loads((ROOT / "scenarios/rogue-exploration/scenario.toml").read_text(encoding="utf-8"))
        actor_stage = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]
        eligible = {choice["archetype"] for band in actor_stage["bands"]
                    if band["depth"]["minimum"] <= depth
                    and (band["depth"]["kind"] == "open" or depth <= band["depth"]["maximum"])
                    for choice in band["choices"]}
        self.assertTrue(eligible, "The requested production depth must have eligible actors")
        expected = (
            ("rogue.eight_species_foes", eligible, "actor", 4),
            ("rogue.bandages", {"rogue.clean_bandage"}, "item", 1),
            ("rogue.rations", {"rogue.ration"}, "item", 1),
            ("rogue.restore_strength", {"rogue.restore_strength"}, "item", 1),
        )
        self.assertEqual([stage["id"] for stage in receipt["decisions"]],
                         [stage for stage, _, _, _ in expected]
                         + ["rogue.room_gold", *MAGIC_ITEM_STAGE_IDS, *EQUIPMENT_ITEM_STAGE_IDS, *CONTROL_ITEM_STAGE_IDS])
        entities = assert_room_gold_receipt(self, receipt["decisions"][4], depth)
        entities.extend(assert_magic_item_receipts(self, receipt["decisions"][5:8], require_placed=True))
        entities.extend(assert_equipment_item_receipts(self, receipt["decisions"][8:12], require_placed=True))
        entities.extend(assert_control_item_receipts(self, receipt["decisions"][12:], require_placed=True))
        for stage, (_, choices, kind, count) in zip(receipt["decisions"][:4], expected, strict=True):
            self.assertEqual(stage["version"], 1)
            result = stage["result"]
            self.assertEqual(result["kind"], "evaluated")
            self.assertEqual(result["requested"], count)
            self.assertEqual(len(result["attempts"]), count)
            placed = []
            for attempt in result["attempts"]:
                self.assertIn(attempt["content"], choices)
                self.assertIn(attempt["kind"], ("placed", "skipped"))
                if attempt["kind"] == "skipped":
                    self.assertEqual(attempt["reason"], "no_eligible_cell")
                    continue
                self.assertEqual(attempt["entity"]["kind"], kind)
                placed.append(attempt)
                entities.append((kind, attempt["entity"]["id"]))
            self.assertTrue(placed, "The seed must place real foes and each finite supply stage")
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
                self.assertEqual(value["points"], {"earned": "1", "spent": "0", "unspent": "1"})
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
        self.assertEqual(selected["points"], {"earned": "1", "spent": "1", "unspent": "0"})
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
        # Bind only this walkthrough instance. Its existing exploration, stair
        # and revisit bounds can compose, and nested settlement adds requests.
        # These ceilings retain failures; they never retry or alter gameplay.
        # Current-view updates averaged about 95 KiB across nine frames per
        # request in the bounded 64 MiB diagnostic failure. This complete-floor
        # and revisit witness needs a larger, still explicit output allowance;
        # its command/work bounds and gameplay assertions are unchanged.
        budget = ProcessWorkBudget(requests=20000, actor_ticks=1000000,
                                   transcript_bytes=512 * 1024 * 1024)
        original_client, original_command = self.client, self.command
        original_frame, original_flush = self.frame, self.flush_save

        def budgeted_client(*args, **kwargs):
            budget.check()
            kwargs["omit_memory"] = True
            kwargs["compact_background"] = True
            client, frame = original_client(*args, **kwargs)
            budget.track(client, frame)
            return client, frame

        def budgeted_command(client, value):
            budget.before()
            frame = original_command(client, value)
            budget.after(client, frame)
            return frame

        def budgeted_frame(client, predicate, seconds=15):
            return budget.frame_wait(original_frame, client, predicate, seconds=seconds)

        def budgeted_flush(*, actor=None):
            # The separate durable-receipt/save-barrier socket is a request too.
            budget.before()
            original_flush(actor=actor)
            budget.check()

        def report_budget():
            report = budget.report()
            report["raw_emitted_bytes"] = (report["used"]["transcript_bytes"]
                                           + getattr(budget, "walkthrough_raw_byte_adjustment", 0))
            (self.directory / "work-budget.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print("Process budget " + self.id() + ": " + json.dumps(report, sort_keys=True), flush=True)
            self.client, self.command = original_client, original_command
            self.frame, self.flush_save = original_frame, original_flush
            if not self._failed():
                budget.check()

        self.client, self.command = budgeted_client, budgeted_command
        self.frame, self.flush_save = budgeted_frame, budgeted_flush
        # Process cleanups run first; the report is then retained before the
        # harness preserves a failed directory or removes a successful one.
        self.addCleanup(report_budget)
        # This owner proves fixed initial-four stock, traversal and persistence,
        # not living-population survival. Current living acceptance is separate;
        # the original current-package basic-only death remains retained.
        source = ROOT / "scenarios/rogue-exploration"
        source_hashes = fixture_source_hashes(source)
        production = tomllib.loads((source / "scenario.toml").read_text(encoding="utf-8"))
        fixture = fixed_initial_population_manifest(production)
        package = self.directory / "population-package"
        shutil.copytree(source, package, ignore=shutil.ignore_patterns(".compiled"))
        self.assertEqual(fixture_source_hashes(package), source_hashes)
        (package / "scenario.toml").write_text(toml_source(fixture), encoding="utf-8")
        parsed = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
        self.assertEqual(parsed, fixture)
        restored = copy.deepcopy(parsed)
        restored["floor_activation"]["wandering"] = production["floor_activation"]["wandering"]
        self.assertEqual(restored, production)
        changed = {name for name, digest in fixture_source_hashes(package).items()
                   if source_hashes.get(name) != digest}
        self.assertEqual(changed, {"scenario.toml"})
        provenance = {"format": "rogue-fixed-initial-population-diagnostic-v1",
            "scope": "fixed initial-four stock/traversal/persistence; not current living survival",
            "source": "scenarios/rogue-exploration", "source_sha256": source_hashes,
            "fixture_pre_validation_sha256": fixture_source_hashes(package),
            "only_semantic_change": "floor_activation.wandering omitted",
            "original_wandering": production["floor_activation"]["wandering"]}
        (self.directory / "fixed-initial-population-source.json").write_text(
            json.dumps(provenance, indent=2) + "\n", encoding="utf-8")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.assertEqual(fixture_source_hashes(source), source_hashes, "Original production bytes changed")
        provenance["validated_fixture_sha256"] = fixture_source_hashes(package)
        (self.directory / "fixed-initial-population-source.json").write_text(
            json.dumps(provenance, indent=2) + "\n", encoding="utf-8")
        # Current checkpoints let transport-only rollovers verify the entire
        # durable endpoint. The later interval=0 journal-tail witness is separate.
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=42)
        closed_transports = []
        # This long driver builds its route from current disclosed observations.
        # Avoid reprinting accumulated map memory on every diagnostic frame.
        player, initial = self.client(omit_memory=True)
        meals = WalkthroughRations()
        search = WalkthroughSearch()
        next_transport_checkpoint, transport_checkpoints = 128, 0

        def checkpoint_transport(client, current):
            nonlocal player, server, next_transport_checkpoint, transport_checkpoints
            if budget.requests < next_transport_checkpoint:
                return client, current
            transport_checkpoints += 1
            server, player, current = self.checkpoint_walkthrough_transport(
                client, server, budget, f"transport-chunk-{transport_checkpoints:03}")
            next_transport_checkpoint = budget.requests + 128
            return player, current

        initial_rations = carried_stock(initial, "ration")
        self.assertEqual(initial_rations, 1)
        current, growth, seen_foes, collected_cells, cleared_cells, floor_graph = self.clear_first_floor(
            player, checkpoint=checkpoint_transport, meals=meals, search=search)
        self.assertEqual(len(seen_foes), 4)
        self.assertTrue(set(seen_foes.values()).issubset(first_floor_foes().values()))
        completed_earned_xp = growth["growth"]["earned_xp"]
        self.assertEqual(growth["growth"]["spent_xp"], "100")
        self.assertEqual(int(growth["growth"]["held_xp"]), int(completed_earned_xp) - 100)
        self.assertEqual(u64(growth["growth"]["highest_earned_tier"]), 2)
        self.assertEqual(growth["points"], {"earned": "2", "spent": "1", "unspent": "1"})
        self.assertEqual(hit_die_counts(observation(current)["combat"]["own_stats"]["hit_dice"]),
                         {("class", "warrior"): 2})
        self.advance(player, "purchase 2 advancement skill heavy_weaponry")
        self.advance(player, "confirm")
        self.assertEqual(self.overview(player)["points"], {"earned": "2", "spent": "2", "unspent": "0"})
        current = self.request(player, {"type": "snapshot"})
        self.assertEqual(carried_stock(current, "bandage"), 5)
        self.assertEqual(carried_stock(current, "ration") + meals.eaten, 2)
        collected_rations = carried_stock(current, "ration") + meals.eaten - initial_rations
        self.assertEqual(collected_rations, 1)
        self.assertFalse(current["state"]["wizard_game"])

        # Revisit through ordinary disclosed stairs, never a hidden-position oracle.
        initial_receipts = self.private_receipts()
        self.assertEqual([entry["scope"] for entry in initial_receipts], ["dungeon/1"])
        self.assert_rogue_population(initial_receipts[0], "dungeon/1", "initial")
        departure = self.walk_to_stairs(
            player, current, floor_graph, checkpoint=checkpoint_transport, meals=meals, search=search)
        self.assertTrue(underfoot(departure["state"])["stairs_down"])
        departure_key = underfoot(departure["state"])["key"]
        self.assertEqual(self.private_receipts(), initial_receipts,
                         "Preparation must not activate the unvisited second floor")
        before = self.request(player, {"type": "snapshot"})
        self.flush_save()
        server.stop(save=False)
        player.stop()
        closed_transports.extend((server, player))
        with closing(sqlite3.connect(self.save)) as db:
            checkpoint = db.execute("SELECT sequence FROM checkpoint").fetchone()
            self.assertIsNotNone(checkpoint)
            self.assertGreater(checkpoint[0], 0)
            sequence = db.execute("SELECT MAX(sequence) FROM journal").fetchone()[0]
        shutil.rmtree(package)
        self.assertFalse(package.exists())

        # Activate the still-unvisited floor without sources or new checkpoints;
        # its real population must then survive recovery from the journal tail.
        server = self.server("--checkpoint-interval", 0, seed=None)
        player, reopened = self.client(omit_memory=True)
        self.assertEqual(reopened["state"], before["state"])
        self.assertEqual(self.private_receipts(), initial_receipts)
        entered = self.settled_action(player, {"type": "move", "direction": "down"})
        self.assertTrue(underfoot(entered["state"])["stairs_up"])
        entered_receipts = self.private_receipts()
        self.assertEqual([entry["scope"] for entry in entered_receipts], ["dungeon/1", "dungeon/2"])
        self.assertEqual(entered_receipts[:1], initial_receipts)
        self.assert_rogue_population(entered_receipts[1], "dungeon/2", "movement")
        self.assertFalse(entered["state"]["wizard_game"])
        for supply in ("bandage", "ration"):
            self.assertEqual(carried_stock(entered, supply), carried_stock(before, supply))
        before = self.request(player, {"type": "snapshot"})
        self.flush_save()
        server.stop(save=False)
        player.stop()
        closed_transports.extend((server, player))
        with closing(sqlite3.connect(self.save)) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone(), checkpoint)
            self.assertGreater(db.execute("SELECT MAX(sequence) FROM journal").fetchone()[0], sequence)

        # Recover the journal-only entry first. The ordinary return then creates
        # the current checkpoint needed by the exact-authority transport seal.
        server = self.server("--checkpoint-interval", 1, seed=None)
        player, reopened = self.client(omit_memory=True)
        self.assertEqual(reopened["state"], before["state"])
        self.assertEqual(self.private_receipts(), entered_receipts)
        returned = self.settled_action(player, {"type": "move", "direction": "up"})
        self.assertEqual(underfoot(returned["state"])["key"], departure_key)
        self.assertEqual(carried_stock(returned, "bandage"), 5)
        self.assertEqual(carried_stock(returned, "ration") + meals.eaten, 2)
        self.assertEqual(self.overview(player)["growth"]["earned_xp"], completed_earned_xp)
        self.assertEqual(self.private_receipts(), entered_receipts,
                         "A revisit appended or changed a durable floor-entry receipt")
        # Keep the original source-free round-trip seal as an explicit witness.
        server, player, returned = self.checkpoint_walkthrough_transport(
            player, server, budget, "transport-round-trip", closed_processes=closed_transports,
            require_visible_endpoint=True)
        next_transport_checkpoint = budget.requests + 128
        returned, eaten_rations = self.revisit_cleared_floor(
            player, returned, seen_foes, collected_cells, cleared_cells, floor_graph,
            checkpoint=checkpoint_transport, meals=meals)
        self.assertEqual(eaten_rations, 1)
        self.assertEqual(carried_stock(returned, "bandage"), 5)
        self.assertEqual(carried_stock(returned, "ration"), initial_rations + collected_rations - eaten_rations)
        self.assertEqual(self.overview(player)["growth"]["earned_xp"], completed_earned_xp)
        self.assertEqual(self.private_receipts(), entered_receipts)

        # Care and paid replacement consume the real collected stock in a safe
        # cleared floor. Neither operation grants inventory on return or reopen.
        current = self.settled_action(player, {"type": "use_ability", "id": "bind_wounds",
            "invocation": {"aim": {"kind": "actor_target", "target": observation(returned)["self_target"]}, "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}})
        self.assertEqual(carried_stock(current, "bandage"), 4)
        self.assertEqual(carried_stock(current, "ration"), 1)
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
        self.assertEqual(self.overview(player)["points"], {"earned": "2", "spent": "1", "unspent": "1"})
        current = self.request(player, {"type": "snapshot"})
        self.assertEqual(carried_stock(current, "ration"), 0)
        self.assertEqual(eaten_rations + 1 + carried_stock(current, "ration"),
                         initial_rations + collected_rations,
                         "Eating and the one-ration respec must conserve the finite acquired stock")
        self.assertEqual(carried_stock(current, "bandage"), 4)
        self.flush_save()
        player.stop()
        server.stop()
        self.server("--checkpoint-interval", 64, seed=None)
        player, reopened = self.client(omit_memory=True)
        self.assertEqual(reopened["state"], current["state"])
        self.assertEqual(carried_stock(reopened, "ration"), 0)
        self.assertEqual(carried_stock(reopened, "bandage"), 4)
        self.assertEqual(self.overview(player)["points"], {"earned": "2", "spent": "1", "unspent": "1"})
        self.assertEqual(self.overview(player)["growth"]["earned_xp"], completed_earned_xp)
        self.assertFalse(reopened["state"]["wizard_game"])
        self.assertEqual(self.private_receipts(), entered_receipts)

    def test_ordinary_rogue_death_stays_terminal_after_reopening(self):
        server = self.server(scenario="rogue-exploration", seed=42)
        # This driver checks terminal/current state, not accumulated map memory.
        player, current = self.client(omit_memory=True, compact_background=True)
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
            current, moved = self.disclosed_step(player, current, direction, survive=False)
            self.assertIsNone(current["error"])
            if moved and not observation(current)["combat"]["dead"]:
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
        player, reopened = self.client(omit_memory=True, compact_background=True)
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
