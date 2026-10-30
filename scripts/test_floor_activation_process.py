"""Authoritative floor entry and recovery through ordinary exploration.

A marker-only fixture isolates population receipts from combat tuning.
"""
import shutil
import sqlite3
import subprocess
import tomllib
import unittest
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
format = 36
id = "floor-activation"
version = "1.0"
ruleset = "rules.toml"
# Imported production rules reference these canonical eligible body packages.
# This closure does not enroll the geometry-only observer or spawn any creature.
modules = ["catalog/creatures.toml", "catalog/bodies.toml"]
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
        # Keep exact catalog/rules inputs, rather than removing Polymorph from
        # imported rules or inventing creature HD for this marker-only fixture.
        for relative in ("rules.toml", "catalog/creatures.toml", "catalog/bodies.toml"):
            self.assertEqual((package / relative).read_bytes(),
                             (ROOT / "scenarios/rogue-exploration" / relative).read_bytes())
        authored = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
        self.assertEqual(authored["characters"], [{"id": 1, "anchor": "dungeon/1/start", "turn_ticks": 100}])
        self.assertEqual(set(authored["archetypes"]), {"marker", "alternate"})
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
        self.assertDisclosedStateEqual(resumed["state"], before["state"])
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


OMISSION_ROUTE_POLICY = {"id": "omission_public_search", "version": 1}


def omission_route_action(controller, sweep, current):
    """Omission-only policy: Controller priority, then bounded public Search.

    This explicitly extends the shared revision-seven combat-route censor;
    it does not alter the Controller, its cohort outcomes, or its budgets.
    """
    from rogue_test_support import DisclosedFloorGraph, disclosed_body_cells, disclosed_melee_candidate
    from scalar_wire import u64
    decision = controller.choose(current["state"])
    if decision.outcome is None:
        return decision.action
    if decision.outcome == "censored_route_exhausted":
        return omission_exhausted_action(controller, sweep, current)
    if decision.outcome != "censored_no_disclosed_combat_route":
        raise AssertionError(decision)
    view = current["state"]["observation"]
    combat = view["combat"]
    hostile = {row["actor"] for row in combat["actors"] if row["hostile"]}
    foes = [row for row in view["visible_actors"] if row["id"] in hostile]
    if (not foes or u64(combat["hp"]) * 4 <= u64(combat["max_hp"])
            or any(disclosed_melee_candidate(view, row) for row in foes)):
        raise AssertionError(decision)
    def position(row):
        return tuple(row["position"][axis] for axis in ("x", "y", "z"))
    body = disclosed_body_cells(view)
    obstacles = {position(row) for row in view["visible_actors"] if row["id"] != view["self_target"]}
    obstacles.update(position(row) for row in view["visible_cells"] if row["wall"])
    blocked = {row["key"] for row in view["visible_cells"] if row["wall"] or (
        row["position"]["z"] == 0 and any(
            tuple(a + b for a, b in zip(position(row), own)) in obstacles for own in body))}
    # Ephemeral public-only blocked graph; neither saved Controller memory nor
    # Search history changes when no safe reachable unsearched candidate exists.
    graph = DisclosedFloorGraph()
    graph.edges = {key: {direction: neighbor for direction, neighbor in edges.items()
                         if neighbor not in blocked}
                   for key, edges in controller.floors[controller.depth].graph.edges.items()
                   if key not in blocked}
    here = underfoot(current["state"])["key"]
    if graph.route_to_any(here, graph.edges.keys() - sweep.searched) is None:
        raise AssertionError(decision)
    return sweep.choose(current["state"], graph)


def omission_exhausted_action(controller, sweep, current):
    """The actual route-exhausted caller; no private terrain or seed inputs."""
    return sweep.choose(current["state"], controller.floors[controller.depth].graph)


def omission_saved_authority(test):
    """Complete durable checkpoint plus independent history/creature audit."""
    import json
    from checkpoint_payload import decode_checkpoint_payload
    from rogue_earned_route_process import read_authoritative_evidence
    with closing(sqlite3.connect(test.save.resolve().as_uri() + "?mode=ro", uri=True)) as db:
        payload = db.execute("SELECT payload FROM checkpoint").fetchone()[0]
    return {"checkpoint": json.loads(decode_checkpoint_payload(payload)),
            "audit": read_authoritative_evidence(test.save, 1)}


def restart_omission_transport(test, controller, sweep, server, player, fallback, cache, on_stopped):
    """Seal all stopped logs without changing the route's controller or work budget."""
    import copy
    import json
    from rogue_earned_route_process import compress_completed_logs
    controller_before = copy.deepcopy(controller.dump())
    sweep_before = copy.deepcopy(sweep.dump())
    before = test.request(player, {"type": "snapshot"})
    test.assertIsNone(before["error"])
    test.flush_save()
    receipts = test.private_receipts()
    saved = omission_saved_authority(test)
    # Freeze authority first; disconnect must not suspend a still-running actor.
    server.stop(save=False)
    player.stop(save=False)
    for process in test.processes:
        test.assertTrue(process._stopped)
        test.assertIsNotNone(process.child.poll())
        test.assertFalse(process.reader.is_alive())
        for handle in (process.child.stdin, process.child.stdout, process.child.stderr,
                       process.lines.reader, process.lines.writer):
            test.assertTrue(handle is None or handle.closed)
    on_stopped()  # Check the old chunk before any verified raw replacement.
    retained = compress_completed_logs(test.directory, cache=cache)
    server = test.server("--checkpoint-interval", 1, scenario=fallback, seed=None)
    player, reopened = test.client(omit_memory=True)
    test.assertIsNone(reopened["error"])
    for field in ("state", "history", "branch"):
        if field == "state":
            test.assertDisclosedStateEqual(reopened[field], before[field])
        else:
            test.assertEqual(reopened[field], before[field])
    test.assertEqual(reopened["cursor"]["tick"], before["cursor"]["tick"])
    test.assertEqual(test.private_receipts(), receipts)
    test.flush_save()
    test.assertEqual(omission_saved_authority(test), saved,
                     "Transport rollover changed complete durable authority")
    test.assertEqual(controller.dump(), controller_before,
                     "Transport rollover reset controller state or cumulative action budget")
    test.assertEqual(sweep.dump(), sweep_before, "Transport rollover reset public Search coverage")
    (test.directory / f"transport-{len(test.processes):03}-equality.json").write_text(
        json.dumps({"wrapper_policy": OMISSION_ROUTE_POLICY,
                    "controller_commands": controller.commands,
                    "controller": controller_before, "search_sweep": sweep_before,
                    "tick": before["cursor"]["tick"], "branch": before["branch"],
                    "exact_state_history_receipts_checkpoint_audit": True,
                    "retained_logs": len(retained)}, sort_keys=True) + "\n", encoding="utf-8")
    return server, player, reopened


class OmittedRoomTopologyProcess(ProcessTestCase):
    """Actual Rogue stock, creatures, traps and wandering; no diagnostic tuning."""

    def test_ordinary_search_traversal_and_source_free_revisit_keep_real_rooms(self):
        self.run_topology_route(mazes=False)

    def test_dark_maze_search_traversal_and_source_free_revisit_keep_real_cells(self):
        self.run_topology_route(mazes=True)

    def run_topology_route(self, *, mazes):
        import json
        import tomllib
        from arena_matrix import toml_source
        from checkpoint_payload import authoritative_floor_activation, decode_checkpoint_payload
        from discovery_package import search
        from process_harness import settle_rogue_action
        from rogue_earned_route import Controller
        from rogue_test_support import DisclosedSearchSweep
        from process_work_budget import ProcessOutputBudget
        from rogue_earned_route_process import (
            ArchiveVerificationCache, CHUNK_SOFT_BYTES, CHUNK_HARD_BYTES, MINIMUM_FREE_BYTES,
        )

        label = "dark-maze" if mazes else "omitted"
        package = self.directory / f"{label}-rogue"
        shutil.copytree(ROOT / "scenarios/rogue-exploration", package,
                        ignore=shutil.ignore_patterns(".compiled"))
        baseline = subprocess.run([self.bin / ("tor-scenario" + SUFFIX), "validate", package],
                                  text=True, capture_output=True, encoding="utf-8", timeout=60)
        self.assertEqual(baseline.returncode, 0, baseline.stdout + baseline.stderr)
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        wandering = manifest["floor_activation"]["wandering"].copy()
        manifest["generation_recipes"]["exploration"]["room_omissions"] = {"count": [3, 3]}
        if mazes:
            # Force the new capability only in this copied acceptance package;
            # live probability, population, stock and hazard tuning stay intact.
            recipe = manifest["generation_recipes"]["exploration"]
            recipe["maze_rooms"] = {"one_in": 1}
            recipe["lighting"] = {"darkness_roll": 1, "darkness_start": 0}
        path.write_text(toml_source(manifest), encoding="utf-8")
        self.assertEqual(tomllib.loads(path.read_text(encoding="utf-8"))
                         ["floor_activation"]["wandering"], wandering)
        result = subprocess.run([self.bin / ("tor-scenario" + SUFFIX), "validate", package],
                                text=True, capture_output=True, encoding="utf-8", timeout=60)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=42)
        player, current = self.client(omit_memory=True)
        sweep = DisclosedSearchSweep()
        sweep.observe(current["state"]["observation"])
        # An actual paid Search, not repeated snapshot polling or oracle navigation.
        before_tick = int(current["state"]["observation"]["tick"])
        current = settle_rogue_action(self, player, self.act(player, search(current)))
        sweep.record_search(current["state"])
        self.assertGreater(int(current["state"]["observation"]["tick"]), before_tick)
        self.flush_save()
        with closing(sqlite3.connect(self.save)) as db:
            payload = db.execute("SELECT payload FROM checkpoint").fetchone()[0]
        saved = json.loads(decode_checkpoint_payload(payload))
        activation = authoritative_floor_activation(saved["game"]["floor_activation"], saved["shared"])
        scope = activation["scopes"]["dungeon/1"]
        self.assertEqual(scope["members"], list(range(1, 10)))
        room_ids = [row["id"] for row in scope["rooms"]]
        self.assertEqual(len(room_ids), 6)
        self.assertEqual(room_ids, sorted(set(room_ids)))
        self.assertTrue(all(row["cells"] for row in scope["rooms"]))
        if mazes:
            def has_interior_wall(room):
                cells = {(cell["position"]["x"], cell["position"]["y"])
                         for cell in room["cells"]}
                xs, ys = zip(*cells)
                return len(cells) < (max(xs) - min(xs) + 1) * (max(ys) - min(ys) + 1)
            self.assertTrue(any(has_interior_wall(room) for room in scope["rooms"]),
                            "forced-maze acceptance must contain physical interior walls")
        self.assertIsNotNone(activation["wandering"])
        first = self.private_receipts()
        gold = next(row for row in first[0]["decisions"] if row["id"] == "rogue.room_gold")
        self.assertEqual([row["room"] for row in gold["result"]["rooms"]], room_ids)
        before = self.request(player, {"type": "snapshot"})
        player.stop()
        server.stop()
        shutil.rmtree(package)
        fallback = unrelated_fallback(self)
        server = self.server("--checkpoint-interval", 1, scenario=fallback, seed=None)
        player, current = self.client(omit_memory=True)
        self.assertDisclosedStateEqual(current["state"], before["state"])
        self.assertEqual(current["history"], before["history"])
        self.assertEqual(self.private_receipts(), first)

        cache = ArchiveVerificationCache()
        chunk = None
        original_frame = self.frame

        def check_capture():
            # Actual filesystem free space includes every previously sealed archive.
            self.assertGreaterEqual(shutil.disk_usage(self.directory).free, MINIMUM_FREE_BYTES,
                                    "Insufficient space for complete retained transport evidence")
            return chunk.check() if chunk is not None else 0

        def guarded_frame(client, predicate, seconds=15):
            check_capture()
            def checked(frame):
                check_capture()
                return predicate(frame)
            result = original_frame(client, checked, seconds=seconds)
            check_capture()
            return result

        def start_chunk():
            nonlocal chunk
            chunk = ProcessOutputBudget([], limit_bytes=CHUNK_HARD_BYTES)
            # Add paths after construction so initial ready output is counted too.
            chunk.paths = tuple(process.diagnostics / name
                                for process in (server, player)
                                for name in ("stdout.log", "stderr.log")) + (server.startup_path,)
            check_capture()

        def stopped_chunk():
            nonlocal chunk
            check_capture()
            chunk = None  # Raw paths are now replaced only by verified retained archives.

        self.frame = guarded_frame
        self.addCleanup(setattr, self, "frame", original_frame)
        start_chunk()
        controller = Controller(1)
        controller.segment = "continuation"
        for _ in range(2500):
            if check_capture() >= CHUNK_SOFT_BYTES:
                server, player, current = restart_omission_transport(
                    self, controller, sweep, server, player, fallback, cache, stopped_chunk)
                start_chunk()
            # Advancement is a headless command, not an inner wire request.
            overview = self.command(player, {"type": "advancement", "command": "overview"})
            self.assertIsNone(overview["error"])
            self.assertIsNotNone(overview["advancement_page"])
            controller.observe(current["state"], overview["advancement_page"]["entries"][0]["value"])
            sweep.observe(current["state"]["observation"])
            if controller.depth == 2:
                break
            action = omission_route_action(controller, sweep, current)
            controller.consume_command(action=action)
            current = settle_rogue_action(self, player, self.act(player, action))
        else:
            self.fail(f"Ordinary {label}-room route did not reach its paired stair within 2500 actions")
        self.assertFalse(current["state"]["wizard_game"])
        self.assertTrue(underfoot(current["state"])["stairs_up"])
        second = self.private_receipts()
        self.assertEqual([row["scope"] for row in second], ["dungeon/1", "dungeon/2"])
        self.assertEqual(second[:1], first)
        for direction in ("up", "down"):
            current = settle_rogue_action(self, player, self.act(player, {"type": "move", "direction": direction}))
            self.assertTrue(underfoot(current["state"])["stairs_" + ("down" if direction == "up" else "up")])
            self.assertEqual(self.private_receipts(), second, "Revisits must not regenerate room Gold")
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
                self.assertDisclosedStateEqual(resumed["state"], before["state"])
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


class OmissionTransportCallerTests(unittest.TestCase):
    """Independent caller checks; no server launch or compiler dependency."""

    def test_two_rollovers_keep_controller_budget_and_every_raw_byte(self):
        self.exercise_rollovers()

    def test_reopen_changes_to_public_or_durable_authority_fail(self):
        for field in ("state", "history", "branch", "cursor", "authority", "sweep"):
            with self.subTest(field=field):
                self.exercise_rollovers(mismatch=field)

    def exercise_rollovers(self, mismatch=None):
        import copy
        import gzip
        import io
        import json
        import sys
        import tempfile
        from pathlib import Path
        from types import SimpleNamespace
        from unittest.mock import patch
        from rogue_earned_route import Controller, ControllerError
        from rogue_test_support import DisclosedSearchSweep
        from rogue_earned_route_process import ArchiveVerificationCache, compress_completed_logs

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            frame = {"error": None, "state": {"all": [1, 2], "revision": "1", "snapshot": "first"}, "history": [{"sequence": 7}],
                     "branch": "unchanged", "cursor": {"tick": "900"}}
            harness = SimpleNamespace(directory=root, processes=[], save=root / "save.db")
            for name in ("assertIsNone", "assertIsNotNone", "assertTrue", "assertFalse", "assertEqual"):
                setattr(harness, name, getattr(self, name))
            harness.assertDisclosedStateEqual = lambda actual, expected: ProcessTestCase.assertDisclosedStateEqual(harness, actual, expected)
            originals = {}

            def process(kind):
                folder = root / f"process-{len(harness.processes):02}-{kind}"
                folder.mkdir()
                data = (f"complete {kind} transcript {len(harness.processes)}\n" * 200).encode()
                (folder / "stdout.log").write_bytes(data)
                originals[str((folder / "stdout.log").relative_to(root))] = data
                streams = [io.BytesIO() for _ in range(5)]
                value = SimpleNamespace(_stopped=False, reader=SimpleNamespace(is_alive=lambda: not value._stopped),
                    child=SimpleNamespace(stdin=streams[0], stdout=streams[1], stderr=streams[2],
                                          poll=lambda: 0 if value._stopped else None),
                    lines=SimpleNamespace(reader=streams[3], writer=streams[4]))
                def stop(*, save):
                    self.assertFalse(save)
                    for stream in streams:
                        stream.close()
                    value._stopped = True
                value.stop = stop
                harness.processes.append(value)
                return value

            def server(*args, **kwargs):
                self.assertEqual(args, ("--checkpoint-interval", 1))
                self.assertEqual(kwargs, {"scenario": root / "fallback", "seed": None})
                return process("tor-server")

            harness.server = server
            reopened_frame = copy.deepcopy(frame)
            if mismatch == "cursor":
                reopened_frame["cursor"]["tick"] = "901"
            elif mismatch == "state":
                reopened_frame["state"]["all"] = [1, 3]
            elif mismatch and mismatch not in ("authority", "sweep"):
                reopened_frame[mismatch] = "changed"
            def client(**kwargs):
                if mismatch == "sweep":
                    sweep.searched.clear()
                return process("tor-client-headless"), copy.deepcopy(reopened_frame)
            harness.client = client
            harness.request = lambda client, request: copy.deepcopy(frame)
            harness.private_receipts = lambda: [{"scope": "dungeon/1", "rooms": [1, 3, 5]}]
            harness.flush_save = lambda: None
            controller = Controller(1, run_limit=2500)
            controller.segment = "continuation"
            controller.commands = 2499
            sweep = DisclosedSearchSweep()
            sweep.searched.add("already-searched-public-cell")
            sweep_before = copy.deepcopy(sweep.dump())
            cache = ArchiveVerificationCache()
            first_server, first_player = process("tor-server"), process("tor-client-headless")
            state = copy.deepcopy(controller.dump())
            hooks = []
            def stopped():
                self.assertTrue(all(p._stopped for p in harness.processes))
                hooks.append(len(harness.processes))
            compression_calls = []
            def checked_compressor(output, *, cache):
                # Check at the real compressor boundary, not merely an earlier hook.
                for value in harness.processes:
                    self.assertTrue(value._stopped)
                    self.assertIsNotNone(value.child.poll())
                    self.assertFalse(value.reader.is_alive())
                    for handle in (value.child.stdin, value.child.stdout, value.child.stderr,
                                   value.lines.reader, value.lines.writer):
                        self.assertTrue(handle is None or handle.closed)
                compression_calls.append(len(harness.processes))
                return compress_completed_logs(output, cache=cache)
            authority = {"checkpoint": {"wandering": [1, 2]}, "audit": {"records": [7]}}
            changed = copy.deepcopy(authority)
            changed["checkpoint"]["wandering"] = [1, 3]
            with patch.object(sys.modules[__name__], "omission_saved_authority",
                              side_effect=[authority, changed] if mismatch == "authority" else None,
                              return_value=authority), patch(
                    "rogue_earned_route_process.compress_completed_logs", side_effect=checked_compressor):
                if mismatch:
                    try:
                        with self.assertRaises(AssertionError):
                            restart_omission_transport(harness, controller, sweep, first_server, first_player,
                                                       root / "fallback", cache, stopped)
                    finally:
                        for value in harness.processes:
                            value.stop(save=False)
                    self.assertEqual(compression_calls, [2])
                    self.assertEqual(controller.dump(), state)
                    return
                for _ in range(2):
                    first_server, first_player, reopened = restart_omission_transport(
                        harness, controller, sweep, first_server, first_player, root / "fallback", cache, stopped)
                    self.assertEqual(reopened, frame)
                    self.assertEqual(controller.dump(), state)
            self.assertEqual(sweep.dump(), sweep_before)
            receipts = sorted(root.glob("transport-*-equality.json"))
            self.assertEqual(len(receipts), 2)
            for receipt in receipts:
                value = json.loads(receipt.read_text())
                self.assertEqual(value["wrapper_policy"],
                                 {"id": "omission_public_search", "version": 1})
                self.assertEqual(value["controller"], state)
                self.assertEqual(value["search_sweep"], sweep_before)
            self.assertEqual(compression_calls, [2, 4])
            self.assertEqual(hooks, [2, 4])
            manifest = json.loads((root / "retained-raw-logs.json").read_text())
            self.assertEqual(len(manifest), 4)
            for row in manifest:
                self.assertEqual(gzip.decompress((root / row["retained"]).read_bytes()), originals[row["original"]])
                self.assertFalse((root / row["original"]).exists())
            controller.consume_command(action={"type": "wait"})
            self.assertEqual(controller.commands, 2500)
            with self.assertRaises(ControllerError):
                controller.consume_command(action={"type": "wait"})
            first_server.stop(save=False)
            first_player.stop(save=False)


class OmissionSearchCallerTests(unittest.TestCase):
    def fixture(self):
        from types import SimpleNamespace
        from rogue_test_support import DisclosedFloorGraph, DisclosedSearchSweep
        def cell(key, x, y, wall=False):
            return {"key": key, "position": {"x": x, "y": y, "z": 0},
                    "wall": wall, "stairs_down": False}
        current = {"state": {"observation": {"self_target": "ordinary-owner", "visible_cells": [
            cell("visited-here", 0, 0), cell("visited-east", 1, 0),
            cell("public-opaque-edge", 1, 1, True)]}}}
        graph = DisclosedFloorGraph()
        graph.observe(current["state"])
        controller = SimpleNamespace(depth=1, floors={1: SimpleNamespace(graph=graph)})
        sweep = DisclosedSearchSweep()
        sweep.observe(current["state"]["observation"])
        sweep.searched.add("visited-here")
        return controller, sweep, current

    def test_exhausted_driver_moves_one_east_to_unsearched_public_boundary(self):
        controller, sweep, current = self.fixture()
        self.assertEqual(omission_exhausted_action(controller, sweep, current),
                         {"type": "move", "direction": "east"},
                         "Route exhaustion must not repeat Search at an already searched cell")
        self.assertEqual(sweep.searched, {"visited-here"}, "Walking is not a paid Search")

    def test_all_disclosed_cells_searched_is_explicit_finite_failure(self):
        controller, sweep, current = self.fixture()
        sweep.searched.add("visited-east")
        with self.assertRaisesRegex(AssertionError, "Search sweep exhausted"):
            omission_exhausted_action(controller, sweep, current)

    def test_private_hidden_targets_cannot_change_public_search_route(self):
        controller, sweep, current = self.fixture()
        # Unknown keys are not graph nodes. No private hints may create a route
        # or suppress the independently visible one-step east target.
        current["private"] = {"secret_entrance": "hidden-west", "stairs": "hidden-south"}
        current["state"]["saved_terrain"] = {"hidden-west": {"wall": False}}
        self.assertEqual(omission_exhausted_action(controller, sweep, current),
                         {"type": "move", "direction": "east"})
        self.assertNotIn("hidden-west", controller.floors[1].graph.edges)
        self.assertNotIn("hidden-west", sweep.searched)

    def test_arrival_searches_once_and_a_revealed_opening_clears_opaque_memory(self):
        from copy import deepcopy
        controller, sweep, current = self.fixture()
        self.assertEqual(omission_exhausted_action(controller, sweep, current),
                         {"type": "move", "direction": "east"})
        arrived = deepcopy(current)
        for cell in arrived["state"]["observation"]["visible_cells"]:
            cell["position"]["x"] -= 1
        sweep.observe(arrived["state"]["observation"])
        action = omission_exhausted_action(controller, sweep, arrived)
        self.assertEqual((action["type"], action["id"]), ("use_ability", "rogue.search"))
        self.assertEqual(sweep.searched, {"visited-here", "visited-east"})
        with self.assertRaisesRegex(AssertionError, "Search sweep exhausted"):
            omission_exhausted_action(controller, sweep, arrived)
        opening = arrived["state"]["observation"]["visible_cells"][-1]
        opening["wall"] = False
        controller.floors[1].graph.observe(arrived["state"])
        sweep.observe(arrived["state"]["observation"])
        self.assertNotIn("south", sweep.boundaries["visited-east"])
        self.assertEqual(omission_exhausted_action(controller, sweep, arrived),
                         {"type": "move", "direction": "south"},
                         "A genuinely disclosed opening becomes reachable public floor")

    def test_unreachable_public_boundary_falls_back_to_reachable_unsearched_floor(self):
        controller, sweep, current = self.fixture()
        graph = controller.floors[1].graph
        # A previously disclosed isolated patch has no known connecting edge.
        graph.edges["remembered-island"] = {}
        sweep.boundaries["remembered-island"] = {"south": "observed-island-wall"}
        sweep.boundaries["visited-east"] = {}
        self.assertEqual(omission_exhausted_action(controller, sweep, current),
                         {"type": "move", "direction": "east"})
        self.assertNotIn("remembered-island", sweep.searched)
        sweep.searched.add("visited-east")
        with self.assertRaisesRegex(AssertionError, "Search sweep exhausted"):
            omission_exhausted_action(controller, sweep, current)
        self.assertNotIn("remembered-island", sweep.searched,
                         "Remembered but unreachable terrain is not searched by proxy")


class OmissionCombatSearchCallerTests(unittest.TestCase):
    """The omission wrapper extends, never changes, Controller revision seven."""

    def fixture(self):
        from copy import deepcopy
        from rogue_earned_route import Controller
        from rogue_test_support import DisclosedSearchSweep
        def cell(key, x, y, wall=False, z=0):
            return {"key": key, "position": {"x": x, "y": y, "z": z},
                    "wall": wall, "stairs_up": False, "stairs_down": False}
        state = {"wizard_game": False, "observation": {
            "tick": "100", "ready": True, "self_target": "self",
            "visible_cells": [cell("home", 0, 0), cell("east", 1, 0),
                              cell("opaque", 1, 1, True)],
            "visible_actors": [
                {"id": "self", "position": {"x": 0, "y": 0, "z": 1}},
                {"id": "foe", "position": {"x": 12, "y": 0, "z": 1}}],
            "ground_items": [], "inventory": [], "interactions": {"inventory": []},
            "combat": {"hp": "20", "max_hp": "20", "dead": False, "terminal": False,
                "preparation_remaining": None, "actors": [{"actor": "foe", "hostile": True}],
                "own_stats": {"hit_dice": {"total": "1", "sources": [
                    {"source_kind": "class", "source": {"id": "warrior"}, "count": "1"}]},
                    "abilities": [{"id": "basic_melee", "cost": None,
                                   "cooldown_remaining": "0", "reach": {"kind": "melee"},
                                   "supply_costs": []}],
                    "resources": [], "nutrition": {"hunger": "nourished"}}}}}
        overview = {"points": {"earned": "1", "spent": "1", "unspent": "0"},
                    "growth": {key: "0" for key in (
                        "highest_earned_tier", "highest_applied_tier", "restorable_hit_dice",
                        "peak_threat", "next_threshold", "earned_xp", "spent_xp", "held_xp")}}
        controller = Controller(1, first_limit=2500, floor_limit=2500, run_limit=2500)
        # Real observe calls record both ordinary previously reached positions.
        previous = deepcopy(state)
        for row in previous["observation"]["visible_cells"]:
            row["position"]["x"] -= 1
        previous["observation"]["visible_actors"][1]["position"]["x"] -= 1
        controller.observe(previous, overview)
        controller.observe(state, overview)
        sweep = DisclosedSearchSweep()
        sweep.observe(state["observation"])
        sweep.searched.add("home")
        return controller, sweep, {"state": state}

    def assert_censor(self, controller, current):
        decision = controller.choose(current["state"])
        self.assertEqual(decision.outcome, "censored_no_disclosed_combat_route")
        self.assertIsNone(decision.action)

    def test_safe_search_revisit_extends_exact_real_controller_censor(self):
        controller, sweep, current = self.fixture()
        self.assert_censor(controller, current)
        before = controller.dump()
        self.assertEqual(omission_route_action(controller, sweep, current),
                         {"type": "move", "direction": "east"})
        self.assertEqual(controller.dump(), before, "Selection cannot spend or reset commands")
        self.assertEqual(sweep.searched, {"home"})

    def test_body_occupied_disconnected_searched_and_emergency_refuse_without_mutation(self):
        from copy import deepcopy
        for mutation in ("feet", "head", "head_wall", "disconnected", "searched", "low_hp"):
            with self.subTest(mutation=mutation):
                controller, sweep, current = self.fixture()
                view = current["state"]["observation"]
                if mutation in ("feet", "head"):
                    view["visible_actors"].append({"id": "neutral", "position": {
                        "x": 1, "y": 0, "z": int(mutation == "head")}})
                elif mutation == "head_wall":
                    view["visible_cells"].append({"key": "head-wall", "wall": True,
                        "position": {"x": 1, "y": 0, "z": 1}})
                elif mutation == "disconnected":
                    controller.floors[1].graph.edges = {"home": {}, "east": {}}
                elif mutation == "searched":
                    sweep.searched.add("east")
                else:
                    view["combat"]["hp"] = "5"
                self.assert_censor(controller, current)
                before = deepcopy((controller.dump(), sweep.dump()))
                with self.assertRaisesRegex(AssertionError, "censored_no_disclosed_combat_route"):
                    omission_route_action(controller, sweep, current)
                self.assertEqual((controller.dump(), sweep.dump()), before)

    def test_separate_height_actor_does_not_block_safe_step(self):
        controller, sweep, current = self.fixture()
        current["state"]["observation"]["visible_actors"].append(
            {"id": "neutral", "position": {"x": 1, "y": 0, "z": 2}})
        self.assert_censor(controller, current)
        self.assertEqual(omission_route_action(controller, sweep, current),
                         {"type": "move", "direction": "east"})

    def test_adjacent_hostile_preempts_unrelated_reachable_closed_door(self):
        controller, sweep, current = self.fixture()
        view = current["state"]["observation"]
        view["visible_actors"][1]["position"] = {"x": 0, "y": 1, "z": 1}
        view["visible_cells"][1]["door"] = {
            "id": "unrelated-door", "open": False, "reachable": True}
        decision = controller.choose(current["state"])
        self.assertEqual(decision.phase, "combat")
        self.assertEqual(omission_route_action(controller, sweep, current), decision.action)

    def test_low_hp_retreat_preempts_unrelated_reachable_closed_door(self):
        controller, sweep, current = self.fixture()
        view = current["state"]["observation"]
        view["combat"]["hp"] = "5"
        view["visible_actors"][1]["position"]["x"] = -12
        view["visible_cells"].append({"key": "unrelated-door-cell", "wall": False,
            "stairs_up": False, "stairs_down": False,
            "position": {"x": 0, "y": 1, "z": 0},
            "door": {"id": "unrelated-door", "open": False, "reachable": True}})
        decision = controller.choose(current["state"])
        self.assertEqual((decision.phase, decision.action),
                         ("retreat", {"type": "move", "direction": "east"}))
        self.assertEqual(omission_route_action(controller, sweep, current), decision.action)

    def test_head_blocked_preferred_west_uses_disclosed_east_alternate(self):
        controller, sweep, current = self.fixture()
        view = current["state"]["observation"]
        view["visible_cells"].append({"key": "west", "wall": False,
            "stairs_up": False, "stairs_down": False, "position": {"x": -1, "y": 0, "z": 0}})
        view["visible_actors"].append({"id": "neutral", "position": {"x": -1, "y": 0, "z": 1}})
        floor = controller.floors[1]
        floor.graph.observe(current["state"])
        floor.visited.add("west")
        sweep.boundaries["west"] = {"north": "previously-observed-wall"}
        self.assert_censor(controller, current)
        self.assertEqual(omission_route_action(controller, sweep, current),
                         {"type": "move", "direction": "east"})

    def test_normal_controller_route_required_door_still_opens(self):
        controller, sweep, current = self.fixture()
        view = current["state"]["observation"]
        view["visible_cells"][1]["door"] = {
            "id": "route-door", "open": False, "reachable": True}
        controller.floors[1].visited.remove("east")
        decision = controller.choose(current["state"])
        self.assertEqual(decision.phase, "open_door")
        self.assertEqual(omission_route_action(controller, sweep, current),
                         {"type": "set_door", "door": "route-door", "open": True})

    def test_cumulative_budget_and_other_censors_are_not_replaced(self):
        controller, sweep, current = self.fixture()
        controller.commands = controller.first_commands = 2499
        controller.consume_command(action={"type": "wait"})
        self.assertEqual(controller.commands, 2500)
        before = sweep.dump()
        with self.assertRaisesRegex(AssertionError, "censored_action_budget"):
            omission_route_action(controller, sweep, current)
        self.assertEqual(sweep.dump(), before)
        self.assertEqual(controller.commands, 2500)
        controller, sweep, current = self.fixture()
        current["state"]["observation"]["combat"].update(hp="0", dead=True, terminal=True)
        with self.assertRaisesRegex(AssertionError, "death"):
            omission_route_action(controller, sweep, current)

    def test_unavailable_attack_does_not_make_adjacent_hostile_safe_for_search(self):
        controller, sweep, current = self.fixture()
        view = current["state"]["observation"]
        view["visible_actors"][1]["position"] = {"x": 0, "y": 1, "z": 1}
        view["combat"]["own_stats"]["abilities"] = []
        self.assert_censor(controller, current)
        before = sweep.dump()
        with self.assertRaisesRegex(AssertionError, "censored_no_disclosed_combat_route"):
            omission_route_action(controller, sweep, current)
        self.assertEqual(sweep.dump(), before)

    def test_paid_arrival_search_is_finite_and_preserves_cumulative_budget(self):
        from copy import deepcopy
        controller, sweep, current = self.fixture()
        controller.commands = controller.first_commands = 2498
        action = omission_route_action(controller, sweep, current)
        self.assertEqual(action, {"type": "move", "direction": "east"})
        controller.consume_command(action=action)
        arrived = deepcopy(current)
        for row in arrived["state"]["observation"]["visible_cells"]:
            row["position"]["x"] -= 1
        arrived["state"]["observation"]["visible_actors"][1]["position"]["x"] -= 1
        controller.observe(arrived["state"], controller.dump()["_advancement"])
        sweep.observe(arrived["state"]["observation"])
        action = omission_route_action(controller, sweep, arrived)
        self.assertEqual((action["type"], action["id"]), ("use_ability", "rogue.search"))
        controller.consume_command(action=action)
        self.assertEqual((controller.commands, sweep.searched), (2500, {"home", "east"}))
        with self.assertRaisesRegex(AssertionError, "censored_action_budget"):
            omission_route_action(controller, sweep, arrived)

    def test_revealed_contact_route_returns_to_controller_before_more_search(self):
        from copy import deepcopy
        controller, sweep, current = self.fixture()
        move = omission_route_action(controller, sweep, current)
        self.assertEqual(move, {"type": "move", "direction": "east"})
        controller.consume_command(action=move)
        arrived = deepcopy(current)
        view = arrived["state"]["observation"]
        for row in view["visible_cells"]:
            row["position"]["x"] -= 1
        view["visible_actors"][1]["position"]["x"] -= 1
        view["tick"] = "200"
        controller.observe(arrived["state"], controller.dump()["_advancement"])
        sweep.observe(view)
        search = omission_route_action(controller, sweep, arrived)
        self.assertEqual((search["type"], search["id"]), ("use_ability", "rogue.search"))
        controller.consume_command(action=search)
        # A new ordinary disclosure, not saved geometry, supplies the connector.
        view["visible_cells"][-1]["wall"] = False
        view["visible_actors"][1]["position"] = {"x": 0, "y": 2, "z": 1}
        view["tick"] = "300"
        controller.observe(arrived["state"], controller.dump()["_advancement"])
        sweep.observe(view)
        expected = controller.choose(arrived["state"])
        self.assertEqual((expected.phase, expected.action),
                         ("combat_approach", {"type": "move", "direction": "south"}))
        before = sweep.dump()
        self.assertEqual(omission_route_action(controller, sweep, arrived), expected.action)
        self.assertEqual(sweep.dump(), before, "Resumed combat must not spend another Search")
        self.assertEqual(controller.commands, 2, "Only the paid move and Search consume commands")
