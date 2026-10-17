"""Behavior of the shared actual-process harness without launching applications."""
import json
import io
from pathlib import Path
import sys
import queue
import threading
import os
import sqlite3
import subprocess
import tempfile
import unittest
from contextlib import closing, contextmanager
from unittest.mock import Mock, patch

from process_harness import (
    OutputQueue, Process, ProcessTestCase, ProcessTestDirectory, Transcript, binaries, inspect_save, save_at,
    ascii_stats_viewport_text, headless_fight_completion, settle_rogue_action, hit_die_counts, non_graphical,
)


class NativeDisplayRequirement(unittest.TestCase):
    def test_direct_native_process_requires_display_before_spawning(self):
        for name in ("tor-client-ascii", "tor-client-ascii.exe"):
            for environment, extra_env in (({}, None), ({"DISPLAY": ":provided"}, {"DISPLAY": ""})):
                with self.subTest(name=name, extra_env=extra_env), \
                        patch("process_harness.os", wraps=os) as platform, \
                        patch("process_harness.subprocess.Popen", side_effect=AssertionError("unguarded native spawn")) as spawn:
                    platform.name = "posix"
                    platform.environ = environment
                    with self.assertRaisesRegex(RuntimeError, "require DISPLAY"):
                        Process(Path("synthetic-bin") / name, ["--connect", "127.0.0.1:1"], extra_env=extra_env)
                    spawn.assert_not_called()

    def test_annotated_method_cannot_bypass_native_launch_display_requirement(self):
        ran = []

        class Misclassified(ProcessTestCase):
            graphical = True

            @non_graphical
            def test_launch(self):
                ran.append("launch")
                self.launch("tor-client-ascii", ["--connect", "127.0.0.1:1"])

        test = Misclassified("test_launch")
        with patch("process_harness.os", wraps=os) as platform, \
                patch("process_harness.binaries", return_value=Path("synthetic-bin")), \
                patch("process_harness.subprocess.Popen", side_effect=AssertionError("unguarded native spawn")) as spawn, \
                tempfile.TemporaryDirectory() as directory, \
                patch("process_harness.ROOT", Path(directory)):
            platform.name = "posix"
            platform.environ = {}
            result = unittest.TestResult()
            unittest.TestSuite([test]).run(result)
        self.assertEqual(ran, ["launch"])
        self.assertEqual(result.testsRun, 1)
        self.assertEqual(len(result.errors), 1)
        self.assertIn("require DISPLAY", result.errors[0][1])
        self.assertEqual(result.failures, [])
        spawn.assert_not_called()


class HeadlessDiagnostics(unittest.TestCase):
    def test_hit_die_summary_helper_distinguishes_racial_and_class_id_racial(self):
        self.assertEqual(hit_die_counts({"total": "3", "sources": [
            {"source_kind": "class", "source": {"id": "racial", "name": "Same"}, "count": "1"},
            {"source_kind": "racial", "source": {"id": "racial", "name": "Same"}, "count": "2"},
        ]}), {("class", "racial"): 1, ("racial", "racial"): 2})

    def test_hit_die_summary_helpers_keep_counts_without_expanding_records(self):
        self.assertEqual(hit_die_counts({"total": "256", "sources": [
            {"source_kind": "class", "source": {"id": "warrior", "name": "Warrior"}, "count": "256"}]}),
            {("class", "warrior"): 256})
        self.assertEqual(hit_die_counts({"total": "0", "sources": []}), {})

    def test_hit_die_counts_require_canonical_u64_without_expanding_rows(self):
        for value in (65_535, 65_536, 65_537, 2**53 + 1, 2**64 - 1):
            with self.subTest(value=value):
                summary = {"total": str(value), "sources": [{"source_kind": "class",
                    "source": {"id": "warrior"}, "count": str(value)}]}
                original = json.loads(json.dumps(summary))
                self.assertEqual(hit_die_counts(summary), {("class", "warrior"): value})
                self.assertEqual(summary, original)
        for field in ("total", "count"):
            for invalid in (1, True, "01", "-1", "+1", "１", str(2**64)):
                with self.subTest(field=field, value=invalid), self.assertRaises(ValueError):
                    summary = {"total": "1", "sources": [{"source_kind": "class",
                        "source": {"id": "warrior"}, "count": "1"}]}
                    (summary if field == "total" else summary["sources"][0])[field] = invalid
                    hit_die_counts(summary)

    def test_memory_projection_requires_explicit_client_opt_in(self):
        harness = ProcessTestCase()
        harness.address = "127.0.0.1:1"
        for options, omitted in (({}, False), ({"omit_memory": False}, False),
                                 ({"omit_memory": True}, True)):
            with self.subTest(options=options), \
                    patch.object(harness, "launch") as launch, \
                    patch.object(harness, "frame", return_value={"type": "ready"}) as frame:
                client, ready = harness.client("credential", observe=True, actor=2, **options)
                arguments = ["--connect", harness.address, "--observe", "--actor", "2"]
                if omitted:
                    arguments.append("--omit-memory")
                launch.assert_called_once_with("tor-client-headless", arguments, token="credential")
                self.assertIs(client, launch.return_value)
                self.assertEqual(ready, {"type": "ready"})
                self.assertTrue(frame.call_args.args[1](ready))
                self.assertFalse(frame.call_args.args[1]({"type": "update"}))


    def test_compact_background_requires_explicit_opt_in_and_keeps_ready_state(self):
        harness = ProcessTestCase()
        harness.address = "127.0.0.1:1"
        ready = {"type": "ready", "state": {"observation": {"tick": "7"}}}
        for enabled in (False, True):
            with self.subTest(enabled=enabled), patch.object(harness, "launch") as launch, \
                    patch.object(harness, "frame", return_value=ready) as frame:
                _, result = harness.client(omit_memory=True, compact_background=enabled)
                arguments = ["--connect", harness.address, "--omit-memory"]
                if enabled:
                    arguments.append("--compact-background")
                self.assertEqual(launch.call_args.args, ("tor-client-headless", arguments))
                self.assertIs(result, ready)
                predicate = frame.call_args.args[1]
                self.assertTrue(predicate(ready))
                self.assertFalse(predicate({"type": "update", "state_omitted": True,
                                            "message": {"type": "map_page"}}))


class SaveBarriers(unittest.TestCase):
    def assert_cleanup_actor(self, options, expected):
        with tempfile.TemporaryDirectory() as directory:
            harness = ProcessTestCase()
            harness.directory = Path(directory)
            harness.save = harness.directory / "game.db"
            harness.bin, harness.suffix, harness.processes = Path("synthetic-bin"), "", []
            child = Mock(pid=123, stdin=io.StringIO(),
                         stdout=io.StringIO('{"address":"127.0.0.1:1"}\n'), stderr=None)
            child.poll.return_value = None
            with patch("process_harness.subprocess.Popen", return_value=child) as launch, \
                    patch("process_harness.save_at") as save:
                try:
                    server = harness.server(seed=None, **options)
                    command = launch.call_args.args[0]
                    self.assertNotIn("--seed", command)
                    if "character" in options:
                        self.assertEqual(command[command.index("--character") + 1],
                                         str(options["character"]))
                    else:
                        self.assertNotIn("--character", command)
                    server.stop()
                    save.assert_called_once_with(harness.address, server.token, expected)
                finally:
                    harness.doCleanups()

    def test_restart_cleanup_uses_explicit_actor_without_overriding_saved_selection(self):
        for actor in (2, "2"):
            with self.subTest(actor=actor):
                self.assert_cleanup_actor({"save_actor": actor}, "2")

    def test_cleanup_preserves_default_and_explicit_startup_character(self):
        for options, actor in (({}, None), ({"character": 2}, "2")):
            with self.subTest(options=options):
                self.assert_cleanup_actor(options, actor)

    def test_explicit_cleanup_actor_does_not_replace_an_explicit_startup_character(self):
        self.assert_cleanup_actor({"character": 1, "save_actor": 2}, "2")

    def test_selected_actor_is_explicit_and_stringified_for_the_save_client(self):
        for actor in (None, 2, "2"):
            with self.subTest(actor=actor), \
                    patch("process_harness.binaries", return_value=Path("synthetic-bin")), \
                    patch("process_harness.subprocess.run", return_value=subprocess.CompletedProcess(
                        [], 0, stdout="", stderr="")) as run:
                save_at("127.0.0.1:1", actor=actor)
                command = run.call_args.args[0]
                if actor is None:
                    self.assertNotIn("--actor", command)
                else:
                    self.assertEqual(command[-2:], ["--actor", "2"])
                self.assertTrue(all(isinstance(argument, str) for argument in command))

    def test_flush_save_preserves_default_and_forwards_explicit_selected_actor(self):
        harness = ProcessTestCase()
        harness.address = "127.0.0.1:1"
        with patch("process_harness.save_at") as save:
            harness.flush_save()
            save.assert_called_once_with(harness.address, actor=None)
        with patch("process_harness.save_at") as save:
            harness.flush_save(actor=2)
            save.assert_called_once_with(harness.address, actor=2)


class DurableFloorReceipts(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.harness = ProcessTestCase()
        self.harness.save = Path(directory.name) / "receipts.db"
        with closing(sqlite3.connect(self.harness.save)) as db:
            db.execute("CREATE TABLE journal (sequence INTEGER PRIMARY KEY, frame BLOB)")
            db.execute("CREATE TABLE history (sequence INTEGER PRIMARY KEY, frame BLOB)")
            db.commit()

    def append(self, sequence, payload):
        # Exercise extraction from framed JSON, not the backend's framing checks.
        frame = bytes(24) + json.dumps(payload).encode("utf-8")
        with closing(sqlite3.connect(self.harness.save)) as db:
            db.execute("INSERT INTO journal VALUES (?, ?)", (sequence, frame))
            db.commit()

    def compact(self, sequence):
        # Production checkpoint transactions move covered frames unchanged.
        with closing(sqlite3.connect(self.harness.save)) as db:
            db.execute("INSERT INTO history SELECT * FROM journal WHERE sequence>0 AND sequence<=?",
                       (sequence,))
            db.execute("DELETE FROM journal WHERE sequence>0 AND sequence<=?", (sequence,))
            db.commit()

    def test_save_barrier_precedes_reading_initial_and_ordered_later_receipts(self):
        first, second, third = ({"scope": f"dungeon/{depth}"} for depth in range(1, 4))
        self.append(0, {"archive": {"initial_activations": [first]}})
        self.append(3, {"record": {"activations": [third]}})
        self.append(1, {"record": {"activations": []}})
        self.append(4, {"annotation": {}})
        with patch.object(self.harness, "flush_save", side_effect=lambda **_: self.append(
                2, {"record": {"activations": [second]}})) as barrier:
            self.assertEqual(self.harness.private_receipts(), [first, second, third])
        barrier.assert_called_once_with(actor=None)

    def test_selected_actor_barrier_audits_compacted_history_and_preserves_duplicate_tail(self):
        first, second = ({"scope": f"dungeon/{depth}", "entrant": 2} for depth in range(1, 3))
        self.append(0, {"archive": {"initial_activations": [first]}})
        self.append(1, {"record": {"activations": [second]}})
        self.compact(1)
        base = inspect_save(self.harness.save)
        with patch.object(self.harness, "flush_save") as barrier:
            before = self.harness.private_receipts(actor=2)
            barrier.assert_called_once_with(actor=2)
        self.assertEqual(before, [first, second])
        # A real duplicate logical receipt is new evidence, even if the original
        # copy moved into history and immutable sequence-zero stayed identical.
        with patch.object(self.harness, "flush_save", side_effect=lambda **_: self.append(
                2, {"record": {"activations": [second]}})) as barrier:
            after = self.harness.private_receipts(actor=2)
            barrier.assert_called_once_with(actor=2)
        self.assertEqual(inspect_save(self.harness.save), base)
        self.assertEqual(after, [first, second, second])
        self.assertNotEqual(after, before)

    def test_duplicate_later_entry_changes_receipts_even_when_base_is_unchanged(self):
        first = {"scope": "dungeon/1", "ordinal": 1}
        self.append(0, {"archive": {"initial_activations": [first]}})
        with patch.object(self.harness, "flush_save"):
            before = self.harness.private_receipts()
            base = inspect_save(self.harness.save)
            self.append(1, {"record": {"activations": [first]}})
            self.assertEqual(inspect_save(self.harness.save), base)
            self.assertEqual(self.harness.private_receipts(), [first, first])
            self.assertNotEqual(self.harness.private_receipts(), before)

    def test_compaction_preserves_receipts_without_readding_checkpoint_receipts(self):
        first, second = ({"scope": f"dungeon/{depth}"} for depth in range(1, 3))
        self.append(0, {"archive": {"initial_activations": [first]}})
        self.append(1, {"record": {"activations": []}})
        self.append(2, {"record": {"activations": [second]}})
        with patch.object(self.harness, "flush_save"):
            before = self.harness.private_receipts()
            self.compact(2)
            self.assertEqual(before, [first, second])
            self.assertEqual(self.harness.private_receipts(), before)

    def test_compacted_history_and_tail_keep_chronological_branch_receipts(self):
        first, second, third = ({"scope": f"dungeon/{depth}"} for depth in range(1, 4))
        self.append(0, {"archive": {"initial_activations": [first]}})
        self.append(2, {"record": {"entry": {"branch": "original"}, "activations": [second]}})
        self.append(1, {"record": {"entry": {"branch": "original"}, "activations": []}})
        self.compact(2)
        self.append(4, {"record": {"entry": {"branch": "continued"}, "activations": [third]}})
        self.append(3, {"annotation": {}})
        with patch.object(self.harness, "flush_save"):
            self.assertEqual(self.harness.private_receipts(), [first, second, third])

    def test_genuine_duplicate_across_compaction_remains_observable(self):
        first, second = ({"scope": f"dungeon/{depth}"} for depth in range(1, 3))
        self.append(0, {"archive": {"initial_activations": [first]}})
        self.append(1, {"record": {"entry": {"branch": "same"}, "activations": [second]}})
        self.compact(1)
        self.append(2, {"record": {"entry": {"branch": "same"}, "activations": [second]}})
        with patch.object(self.harness, "flush_save"):
            self.assertEqual(self.harness.private_receipts(), [first, second, second])


class DisclosedExploration(unittest.TestCase):
    def test_visible_stair_without_disclosed_approach_keeps_exploring(self):
        from process_harness import disclosed_exploration_route
        route = disclosed_exploration_route(
            (0, 0), {(0, 0), (1, 0), (0, 2)}, {(0, 2)}, {(0, 0)})
        self.assertEqual(route, [((1, 0), "east")])

    def test_known_stair_route_precedes_unexplored_branch(self):
        from process_harness import disclosed_exploration_route
        route = disclosed_exploration_route(
            (0, 0), {(0, 0), (1, 0), (-1, 0)}, {(1, 0)}, {(0, 0)})
        self.assertEqual(route, [((1, 0), "east")])

    def test_exhausted_disclosure_does_not_invent_hidden_route(self):
        from process_harness import disclosed_exploration_route
        self.assertIsNone(disclosed_exploration_route(
            (0, 0), {(0, 0), (0, 2)}, {(0, 2)}, {(0, 0)}))


class ActionCompletion(unittest.TestCase):
    def test_headless_fight_waits_for_permissions_after_resolved_intention(self):
        # Windows release published the ready observation, then removed the
        # resolved intention, before updating its still-present cancel grant.
        # admission=True alone cannot authorize input at that stream boundary.
        started = {"intentions": [{"intention": "paid-1", "phase": "started"}],
                   "readiness": {"revision": "12", "admission": True,
                                 "resume": [], "cancel": ["paid-1"]},
                   "state": {"revision": "10", "observation": {
                       "tick": "348", "ready": False,
                       "combat": {"dead": False, "preparation_active": True}}}}
        observation = {**started, "state": {"revision": "11", "observation": {
            "tick": "388", "ready": True,
            "combat": {"dead": False, "preparation_active": False}}}}
        resolved = {**observation, "intentions": []}
        permitted = {**resolved, "readiness": {"revision": "13", "admission": True,
                                               "resume": [], "cancel": []}}
        self.assertFalse(headless_fight_completion(started))
        self.assertFalse(headless_fight_completion(observation))
        self.assertFalse(headless_fight_completion(resolved),
                         "Resolved work still has stale cancel permission; input is not ready")
        self.assertTrue(headless_fight_completion(permitted))

    def test_headless_fight_requires_completed_work_and_current_admission(self):
        settled = {"intentions": [],
                   "readiness": {"admission": True, "resume": [], "cancel": []},
                   "state": {"observation": {"ready": True,
                       "combat": {"dead": False, "preparation_active": False}}}}
        self.assertTrue(headless_fight_completion(settled))
        for phase in ("queued", "started", "paused", "suspended"):
            with self.subTest(phase=phase):
                self.assertFalse(headless_fight_completion({**settled,
                    "intentions": [{"intention": "paid-1", "phase": phase}]}))
        for permission in ({"admission": False}, {"resume": ["paid-1"]},
                           {"cancel": ["paid-1"]}):
            with self.subTest(permission=permission):
                self.assertFalse(headless_fight_completion({**settled,
                    "readiness": {**settled["readiness"], **permission}}))
        for ready, active in ((False, False), (True, True)):
            with self.subTest(ready=ready, active=active):
                self.assertFalse(headless_fight_completion({**settled,
                    "state": {"observation": {"ready": ready,
                        "combat": {"dead": False, "preparation_active": active}}}}))

    def test_headless_fight_death_is_observable_without_action_permission(self):
        dead = {"intentions": [{"intention": "paid-1", "phase": "started"}],
                "readiness": {"admission": False, "resume": [], "cancel": ["paid-1"]},
                "state": {"observation": {"ready": False,
                    "combat": {"dead": True, "preparation_active": False}}}}
        self.assertTrue(headless_fight_completion(dead))

    def test_headless_queued_attack_can_publish_paused_before_started(self):
        harness = ProcessTestCase()
        player = object()
        action = {"type": "use_ability", "id": "basic_melee",
                  "invocation": {"kind": "actor_target", "target": "disclosed-foe"},
                  "supplies": {"kind": "carried"}}

        def lifecycle(phase, *, identity="paid-1", revision="983", resume=()):
            status = {"intention": identity, "phase": phase}
            return {"error": None, "intentions": [status],
                    "message": {"type": "update", "update": {"body": {
                        "type": "intention", "status": status}}},
                    "readiness": {"revision": revision, "resume": list(resume)}}

        accepted = lifecycle("queued")
        paused = lifecycle("paused")
        permitted = {**paused, "message": {"type": "update", "update": {"body": {
                         "type": "readiness"}}},
                     "readiness": {"revision": "984", "resume": ["paid-1"]}}
        # An incoming hit can interrupt the paid attack in the same simulation
        # publication that starts it. No separate started lifecycle is required.
        pending = iter([accepted, lifecycle("paused", identity="unrelated-2"),
                        paused, paused, permitted])

        def frame(client, predicate):
            self.assertIs(client, player)
            selected = next((item for item in pending if predicate(item)), None)
            self.assertIsNotNone(selected, "The paused attack's permission was not observed")
            return selected

        with patch.object(harness, "command", return_value=accepted) as command, \
                patch.object(harness, "frame", side_effect=frame) as wait:
            result = harness.act(player, action)
        self.assertIs(result, permitted)
        self.assertEqual(result["intentions"], [{"intention": "paid-1", "phase": "paused"}])
        self.assertEqual(wait.call_count, 2, "Pause still needs its ordered permission update")
        command.assert_called_once_with(player, {"type": "act", "action": action})

    def test_native_fight_waits_past_started_and_recovery_frames(self):
        from process_harness import ascii_input_completion
        complete = ascii_input_completion("right", wait_for_ready=True)
        def frame(*, done=None, ready=True, phase=None, active=False, dead=False):
            return {"input_done": done, "busy": False,
                    "intentions": [{"phase": phase}] if phase else [],
                    "state": {"observation": {"ready": ready, "combat": {
                        "preparation_active": active, "dead": dead}}}}
        # The postmerge Windows trace acknowledged the input, started its
        # intention, and cleared that intention before readiness.
        self.assertFalse(complete(frame(done="right", phase="queued")))
        self.assertFalse(complete(frame(phase="started")))
        self.assertFalse(complete(frame(ready=False)))
        self.assertFalse(complete(frame(active=True)))
        self.assertTrue(complete(frame()))

    def test_native_default_still_stops_at_started_and_opt_out_at_queued(self):
        from process_harness import ascii_input_completion
        frame = {"input_done": "right", "busy": False, "intentions": [{"phase": "queued"}]}
        default = ascii_input_completion("right")
        self.assertFalse(default(frame))
        self.assertTrue(ascii_input_completion("right", wait_for_simulation=False)(frame))
        frame["input_done"] = None
        frame["intentions"][0]["phase"] = "started"
        self.assertTrue(default(frame))

    def test_native_readiness_wait_keeps_rejection_pause_and_death_observable(self):
        from process_harness import ascii_input_completion
        for phase, dead, ready in [(None, False, True), ("paused", False, True),
                                   (None, True, False)]:
            with self.subTest(phase=phase, dead=dead):
                complete = ascii_input_completion("right", wait_for_ready=True)
                frame = {"input_done": None, "busy": False,
                         "intentions": [{"phase": phase}] if phase else [],
                         "state": {"observation": {"ready": ready, "combat": {
                             "preparation_active": False, "dead": dead}}}}
                self.assertFalse(complete(frame), "readiness alone is not input acknowledgement")
                frame["input_done"] = "right"
                self.assertTrue(complete(frame))

    def test_decimal_readiness_revisions_wait_for_the_newer_permission_frame(self):
        for revision in [9, 99, 9007199254740991, 9007199254740992, 18446744073709551614]:
            with self.subTest(revision=revision):
                harness = ProcessTestCase()
                accepted = {"intentions": [{"phase": "queued", "intention": "admission-1"}]}
                executed = {
                    "message": {"type": "update", "update": {"body": {
                        "type": "intention", "status": {
                            "intention": "admission-1", "phase": "resolved"}}}},
                    "readiness": {"revision": str(revision)},
                }
                newer = {"readiness": {"revision": str(revision + 1)}}
                compact = [{"type": "update", "message": {"type": kind},
                            "state_omitted": True}
                           for kind in ("map_page", "palette", "waiting")]
                batches = [[*compact, executed], [*compact, executed, newer]]

                def frame(client, predicate):
                    selected = next((item for item in batches.pop(0) if predicate(item)), None)
                    self.assertIsNotNone(selected, "the newer permission frame was not selected")
                    return selected

                with patch.object(harness, "command", return_value=accepted), \
                        patch.object(harness, "frame", side_effect=frame):
                    self.assertIs(harness.act(object(), {"type": "wait"}), newer)
                self.assertEqual(batches, [])


class OwnerPolicyPeerCoordination(unittest.TestCase):
    def setUp(self):
        from test_creature_inspection_process import CreatureInspectionProcesses
        self.coordinate = CreatureInspectionProcesses._outgoing_policy_peer_wait
        self.harness = unittest.TestCase()
        self.peer = object()
        self.owner = {"error": None, "state": {"observation": {"tick": "150"}}}
        self.snapshot = {"error": None, "state": {"observation": {"tick": "150", "ready": True}}}
        self.refreshed = False
        self.wait_error = None
        self.operations = []
        self.harness.request = Mock(side_effect=self.request)
        self.harness.act = Mock(side_effect=self.act)

    def request(self, peer, request):
        self.assertIs(peer, self.peer)
        self.assertEqual(request, {"type": "snapshot"})
        self.operations.append("snapshot")
        self.refreshed = True
        return self.snapshot

    def act(self, peer, action):
        self.assertIs(peer, self.peer)
        self.assertEqual(action, {"type": "wait"})
        self.operations.append("wait")
        # Retained CI ordering: the owner has started, but the peer has not
        # observed that revision and its ordinary wait must be rejected.
        return {"error": self.wait_error if self.refreshed else
                "StaleRevision: Refresh the observation before acting"}

    def wait(self):
        return self.coordinate(self.harness, self.peer, self.owner)

    def test_owner_observation_requires_peer_snapshot_before_one_wait(self):
        self.assertIsNone(self.wait()["error"])
        self.assertEqual(self.operations, ["snapshot", "wait"])
        self.harness.request.assert_called_once()
        self.harness.act.assert_called_once()

    def test_snapshot_error_prevents_peer_wait(self):
        self.snapshot["error"] = "Connection refused"
        with self.assertRaisesRegex(AssertionError, "Connection refused"):
            self.wait()
        self.harness.act.assert_not_called()

    def test_snapshot_must_be_ready_at_the_owner_tick(self):
        for ready, tick in ((False, "150"), (True, "151")):
            with self.subTest(ready=ready, tick=tick):
                self.snapshot["state"]["observation"] = {"ready": ready, "tick": tick}
                with self.assertRaises(AssertionError):
                    self.wait()
                self.harness.act.assert_not_called()

    def test_peer_refusal_is_reported_without_retry(self):
        self.wait_error = "StaleRevision: Refresh the observation before acting"
        with self.assertRaisesRegex(AssertionError, "StaleRevision"):
            self.wait()
        self.assertEqual(self.operations, ["snapshot", "wait"])
        self.harness.act.assert_called_once()


class RogueActionSettlement(unittest.TestCase):
    """Script only disclosed frames; never build or launch the applications."""

    def setUp(self):
        self.harness = ProcessTestCase()
        self.player = object()

    @staticmethod
    def frame(*, ready=True, remaining=None, active=False, phase=None,
              identity="paid-1", admission=True, resume=(), dead=False, error=None):
        return {"error": error, "readiness": {
                    "revision": "2", "admission": admission, "resume": list(resume), "cancel": []},
                "intentions": [{"intention": identity, "phase": phase}] if phase else [],
                "state": {"wizard_game": False, "observation": {
                    "ready": ready, "combat": {"preparation_remaining": remaining,
                        "preparation_active": active, "terminal": dead, "dead": dead}}}}

    def paused(self, **kwargs):
        return self.frame(remaining="50", phase="paused", resume=("paid-1",), **kwargs)

    def wait_through(self, frames):
        def wait(player, predicate):
            self.assertIs(player, self.player)
            for frame in frames:
                if predicate(frame):
                    return frame
            self.fail("Settlement did not accept the final disclosed frame")
        return wait

    def test_ready_paused_preparation_resumes_existing_intention_once(self):
        paused, done = self.paused(), self.frame()
        queued = self.frame(remaining="50", phase="queued", resume=("paid-1",))
        started = self.frame(remaining="50", active=True, phase="started", admission=False)
        with patch.object(self.harness, "command", return_value=queued) as command, \
                patch.object(self.harness, "frame", side_effect=self.wait_through([started, done])), \
                patch.object(self.harness, "act") as act:
            self.assertIs(settle_rogue_action(self.harness, self.player, paused), done)
        command.assert_called_once_with(self.player, {"type": "resume_intention"})
        act.assert_not_called()

    def test_ready_observation_waits_for_admission_and_finished_lifecycle(self):
        for pending in (self.frame(admission=False), self.frame(phase="queued"),
                        self.frame(phase="started"), self.frame(ready=False)):
            with self.subTest(pending=pending):
                done = self.frame()
                with patch.object(self.harness, "command") as command, \
                        patch.object(self.harness, "frame", side_effect=self.wait_through(
                            [self.frame(admission=False), self.frame(phase="started"), done])):
                    self.assertIs(settle_rogue_action(self.harness, self.player, pending), done)
                command.assert_not_called()

    def test_active_preparation_is_not_settled_or_resumed(self):
        active = self.frame(remaining="50", active=True, phase="started", resume=("paid-1",))
        done = self.frame()
        with patch.object(self.harness, "command") as command, \
                patch.object(self.harness, "frame", side_effect=self.wait_through([done])):
            self.assertIs(settle_rogue_action(self.harness, self.player, active), done)
        command.assert_not_called()

    def test_pause_waits_for_matching_lifecycle_and_resume_permission(self):
        for phase, permission in (("started", ("paid-1",)), ("paused", ()),
                                  ("paused", ("unrelated-2",))):
            with self.subTest(phase=phase, permission=permission):
                pending = self.frame(remaining="50", phase=phase, resume=permission)
                paused, done = self.paused(), self.frame()
                queued = self.frame(remaining="50", phase="queued", admission=False)
                with patch.object(self.harness, "command", return_value=queued) as command, \
                        patch.object(self.harness, "frame", side_effect=[paused, done]) as wait:
                    def checked_wait(player, predicate):
                        candidate = paused if wait.call_count == 1 else done
                        self.assertFalse(predicate(pending))
                        self.assertTrue(predicate(candidate))
                        if wait.call_count == 1:
                            command.assert_not_called()
                        return candidate
                    wait.side_effect = checked_wait
                    self.assertIs(settle_rogue_action(self.harness, self.player, pending), done)
                command.assert_called_once_with(self.player, {"type": "resume_intention"})

    def test_rejected_resume_fails_without_readmission_or_cancellation(self):
        rejected = self.paused(error="resume rejected")
        with patch.object(self.harness, "command", return_value=rejected) as command, \
                patch.object(self.harness, "frame") as wait, \
                patch.object(self.harness, "act") as act:
            with self.assertRaisesRegex(AssertionError, "resume rejected"):
                settle_rogue_action(self.harness, self.player, self.paused())
        command.assert_called_once_with(self.player, {"type": "resume_intention"})
        wait.assert_not_called()
        act.assert_not_called()

    def test_resume_cannot_replace_the_original_intention(self):
        replacement = self.frame(remaining="50", phase="queued", identity="unrelated-2", admission=False)
        with patch.object(self.harness, "command", return_value=replacement) as command, \
                patch.object(self.harness, "frame") as wait:
            with self.assertRaisesRegex(AssertionError, "intention changed.*paid-1"):
                settle_rogue_action(self.harness, self.player, self.paused())
        command.assert_called_once_with(self.player, {"type": "resume_intention"})
        wait.assert_not_called()

    def test_paused_replacement_is_not_resumed_after_original_preparation(self):
        started = self.frame(remaining="50", active=True, phase="started", admission=False)
        replacement = self.frame(remaining="50", phase="paused", identity="unrelated-2",
                                 resume=("unrelated-2",))
        with patch.object(self.harness, "command") as command, \
                patch.object(self.harness, "frame", side_effect=self.wait_through([replacement])):
            with self.assertRaisesRegex(AssertionError, "intention changed.*paid-1"):
                settle_rogue_action(self.harness, self.player, started)
        command.assert_not_called()

    def test_stream_error_is_reported_without_more_input(self):
        started = self.frame(remaining="50", active=True, phase="started", admission=False)
        rejected = self.frame(ready=False, admission=False, error="stream rejected")
        with patch.object(self.harness, "command") as command, \
                patch.object(self.harness, "frame", side_effect=self.wait_through([rejected])):
            with self.assertRaisesRegex(AssertionError, "stream rejected"):
                settle_rogue_action(self.harness, self.player, started)
        command.assert_not_called()

    def test_deadline_reports_last_disclosed_preparation_and_permission(self):
        started = self.frame(remaining="50", active=True, phase="started", admission=False)
        paused = self.frame(remaining="25", phase="paused", resume=())

        def timeout(player, predicate):
            self.assertFalse(predicate(paused))
            raise AssertionError("Process output deadline")

        with patch.object(self.harness, "command") as command, \
                patch.object(self.harness, "frame", side_effect=timeout):
            with self.assertRaisesRegex(AssertionError, "paid-1.*25.*Process output deadline"):
                settle_rogue_action(self.harness, self.player, started)
        command.assert_not_called()

    def test_resume_limit_reports_unfinished_intention(self):
        paused = self.paused()
        queued = self.frame(remaining="50", phase="queued", admission=False)
        with patch.object(self.harness, "command", return_value=queued) as command, \
                patch.object(self.harness, "frame", side_effect=self.wait_through([paused])):
            with self.assertRaisesRegex(AssertionError, "16.*paid-1"):
                settle_rogue_action(self.harness, self.player, paused)
        self.assertEqual(command.call_args_list,
                         [unittest.mock.call(self.player, {"type": "resume_intention"})] * 16)

    def test_completion_after_sixteenth_resume_is_still_success(self):
        paused, done = self.paused(), self.frame()
        queued = self.frame(remaining="50", phase="queued", admission=False)
        with patch.object(self.harness, "command", return_value=queued) as command, \
                patch.object(self.harness, "frame", side_effect=[paused] * 15 + [done]):
            self.assertIs(settle_rogue_action(self.harness, self.player, paused), done)
        self.assertEqual(command.call_count, 16)

    def test_ready_without_preparation_needs_no_more_input(self):
        done = self.frame()
        with patch.object(self.harness, "command") as command, \
                patch.object(self.harness, "frame") as wait:
            self.assertIs(settle_rogue_action(self.harness, self.player, done), done)
        command.assert_not_called()
        wait.assert_not_called()

    def test_terminal_death_returns_without_permission_when_survival_is_not_required(self):
        dead = self.paused(ready=False, admission=False, dead=True)
        with patch.object(self.harness, "command") as command, \
                patch.object(self.harness, "frame") as wait:
            self.assertIs(settle_rogue_action(self.harness, self.player, dead, survive=False), dead)
        command.assert_not_called()
        wait.assert_not_called()

    def test_terminal_death_arriving_during_settlement_stays_observable(self):
        started = self.frame(ready=False, remaining="50", active=True, phase="started", admission=False)
        dead = self.frame(ready=False, admission=False, dead=True)
        with patch.object(self.harness, "command") as command, \
                patch.object(self.harness, "frame", side_effect=self.wait_through([dead])):
            self.assertIs(settle_rogue_action(self.harness, self.player, started, survive=False), dead)
        command.assert_not_called()

    def test_terminal_death_fails_when_survival_is_required(self):
        dead = self.paused(ready=False, admission=False, dead=True)
        with patch.object(self.harness, "command") as command, \
                patch.object(self.harness, "frame") as wait:
            with self.assertRaisesRegex(AssertionError, "died|terminal"):
                settle_rogue_action(self.harness, self.player, dead)
        command.assert_not_called()
        wait.assert_not_called()


class NativeStatsViewport(unittest.TestCase):
    def test_wrapped_status_text_is_read_from_visible_rows(self):
        frame = {"screen": {"stats_scroll": 1}, "stats_rows": [
            "offscreen before", *["visible context"] * 22,
            "Protection against Crystal Pause from actor 2: 400 active ticks", "remaining",
            "offscreen after",
        ]}
        text = ascii_stats_viewport_text(frame)
        self.assertIn("Protection against Crystal Pause from actor 2: 400 active ticks remaining", text)
        self.assertNotIn("offscreen", text)

    def test_offscreen_continuation_cannot_complete_a_visible_status(self):
        frame = {"screen": {"stats_scroll": 0}, "stats_rows": [
            *["visible context"] * 23,
            "Protection against Crystal Pause from actor 2: 400 active ticks", "remaining",
        ]}
        text = ascii_stats_viewport_text(frame)
        self.assertIn("400 active ticks", text)
        self.assertNotIn("400 active ticks remaining", text)

    def test_status_above_the_viewport_is_not_visible(self):
        frame = {"screen": {"stats_scroll": 1}, "stats_rows": [
            "Protection against Crystal Pause from actor 2: 400 active ticks remaining",
            *["visible context"] * 24,
        ]}
        self.assertNotIn("Protection against", ascii_stats_viewport_text(frame))


class NativeKeyNames(unittest.TestCase):
    def test_x11_page_keys_use_keysyms_for_both_press_and_release(self):
        harness = ProcessTestCase()
        with patch.object(harness, "_native_window", return_value="42"), \
                patch("process_harness.os.name", "posix"), \
                patch("process_harness.subprocess.run") as run:
            native = harness.native_keys(object())
            for name, keysym in (("PageDown", "Next"), ("PageUp", "Prior"), ("Down", "Down")):
                for down in (True, False):
                    with self.subTest(name=name, down=down):
                        native(name, down)
                        run.assert_called_with(
                            ["xdotool", "keydown" if down else "keyup", "--window", "42", keysym],
                            check=True, timeout=10)

    def test_windows_page_keys_send_extended_navigation_messages(self):
        import ctypes
        harness = ProcessTestCase()
        user32 = Mock()
        user32.PostMessageW.return_value = 1
        user32.MapVirtualKeyW.side_effect = lambda vk, mode: {0x22: 0x51, 0x21: 0x49}[vk]
        with patch.object(harness, "_native_window", return_value=42), \
                patch("process_harness.os.name", "nt"), \
                patch.object(ctypes, "WinDLL", return_value=user32, create=True):
            native = harness.native_keys(object())
            for name, vk, scan in (("PageDown", 0x22, 0x51), ("PageUp", 0x21, 0x49)):
                for down in (True, False):
                    with self.subTest(name=name, down=down):
                        native(name, down)
                        user32.MapVirtualKeyW.assert_called_with(vk, 0)
                        user32.PostMessageW.assert_called_with(
                            42, 0x100 if down else 0x101, vk,
                            1 | (scan << 16) | 0x01000000 | (0 if down else 0xC0000000))


class NativeKeyPresses(unittest.TestCase):
    def test_repeated_native_key_waits_for_fresh_release_before_next_press(self):
        harness = ProcessTestCase()
        window = object()
        events = []
        pending_release = [False]
        sequence = [0]

        def native(name, down):
            self.assertEqual(name, "Down")
            events.append(("key", down))
            if down:
                self.assertFalse(pending_release[0],
                                 "second Down arrived before the release pump reset its held duration")
                sequence[0] += 1
            else:
                pending_release[0] = True

        def presented(client, predicate, seconds):
            self.assertIs(client, window)
            self.assertGreater(seconds, 0)
            frame = {"type": "frame", "input_done": "down", "busy": False,
                     "native_input_sequence": sequence[0]}
            self.assertTrue(predicate(frame))
            events.append(("presented", sequence[0]))
            return frame

        def released(client, predicate, seconds):
            self.assertIs(client, window)
            self.assertGreater(seconds, 0)
            self.assertTrue(pending_release[0])
            # Unrelated/stale output, including other record types, cannot serve
            # as the release boundary for the newly acknowledged native press.
            for record in [
                {"type": "other", "sequence": sequence[0]},
                {"type": "native_release"},
                {"type": "native_release", "sequence": sequence[0] - 1},
                {"type": "native_release", "sequence": sequence[0] + 1},
            ]:
                self.assertFalse(predicate(record), record)
            record = {"type": "native_release", "sequence": sequence[0]}
            self.assertTrue(predicate(record))
            pending_release[0] = False
            events.append(("released", sequence[0]))
            return record

        with patch.object(harness, "ascii_frame", side_effect=presented), \
                patch.object(harness, "frame", side_effect=released):
            first = harness.native_press(window, native, "Down", "down")
            second = harness.native_press(window, native, "Down", "down")
        self.assertEqual((first["native_input_sequence"], second["native_input_sequence"]), (1, 2))
        self.assertEqual(events, [("key", True), ("presented", 1), ("key", False), ("released", 1),
                                  ("key", True), ("presented", 2), ("key", False), ("released", 2)])

    def test_failed_input_releases_the_key_without_masking_the_original_error(self):
        harness = ProcessTestCase()
        original = AssertionError("original presentation timeout")
        events = []
        def native(name, down):
            events.append((name, down))
            if not down:
                raise OSError("cleanup release failed")
        with patch.object(harness, "ascii_frame", side_effect=original), \
                patch.object(harness, "frame") as release_wait, \
                self.assertRaises(AssertionError) as caught:
            harness.native_press(object(), native, "Down", "down")
        self.assertIs(caught.exception, original)
        self.assertIn("cleanup release failed", " ".join(original.__notes__))
        release_wait.assert_not_called()
        self.assertEqual(events, [("Down", True), ("Down", False)])

    def test_failed_keydown_still_attempts_release_and_preserves_the_error(self):
        harness = ProcessTestCase()
        original = OSError("keydown failed")
        events = []
        def native(name, down):
            events.append(down)
            if down:
                raise original
        with patch.object(harness, "ascii_frame") as presentation, \
                self.assertRaises(OSError) as caught:
            harness.native_press(object(), native, "Down", "down")
        self.assertIs(caught.exception, original)
        presentation.assert_not_called()
        self.assertEqual(events, [True, False])

    def test_missing_native_sequence_fails_and_releases_instead_of_accepting_an_old_ack(self):
        harness = ProcessTestCase()
        events = []
        with patch.object(harness, "ascii_frame", return_value={"input_done": "down"}), \
                patch.object(harness, "frame") as release_wait, \
                self.assertRaisesRegex(AssertionError, "native input sequence"):
            harness.native_press(object(), lambda name, down: events.append(down), "Down", "down")
        release_wait.assert_not_called()
        self.assertEqual(events, [True, False])

    def test_release_acknowledgement_shares_the_original_bounded_deadline(self):
        harness = ProcessTestCase()
        def presentation(client, predicate, seconds):
            frame = {"input_done": "down", "native_input_sequence": 3, "busy": False}
            self.assertTrue(predicate(frame))
            return frame
        with patch("process_harness.time.monotonic", side_effect=[10, 10, 17]), \
                patch.object(harness, "ascii_frame", side_effect=presentation) as presented, \
                patch.object(harness, "frame", side_effect=AssertionError("release deadline")) as released, \
                self.assertRaisesRegex(AssertionError, "release deadline"):
            harness.native_press(object(), lambda name, down: None, "Down", "down", seconds=8)
        self.assertEqual(presented.call_args.kwargs["seconds"], 8)
        self.assertEqual(released.call_args.kwargs["seconds"], 1)

    def test_progress_during_release_returns_the_latest_real_presented_frame(self):
        harness = ProcessTestCase()
        started = {"type": "frame", "input_done": "down", "native_input_sequence": 4,
                   "busy": False, "intentions": [{"phase": "started"}]}
        ready = {"type": "frame", "input_done": None, "native_input_sequence": None,
                 "busy": False, "intentions": [], "state": {"observation": {"ready": True}}}
        def presentation(client, predicate, seconds):
            self.assertTrue(predicate(started), "default UI completion still accepts started work")
            return started
        def release(client, predicate, seconds):
            self.assertFalse(predicate(ready))
            ack = {"type": "native_release", "sequence": 4}
            self.assertTrue(predicate(ack))
            return ack
        with patch.object(harness, "ascii_frame", side_effect=presentation), \
                patch.object(harness, "frame", side_effect=release):
            result = harness.native_press(object(), lambda name, down: None, "Down", "down")
        self.assertIs(result, ready, "terminal progress must not be consumed and lost during release")

    def test_readiness_wait_keeps_the_sequence_from_the_earlier_input_acknowledgement(self):
        harness = ProcessTestCase()
        queued = {"type": "frame", "input_done": "down", "native_input_sequence": 7,
                  "busy": False, "intentions": [{"phase": "queued"}],
                  "state": {"observation": {"ready": False, "combat": {
                      "dead": False, "preparation_active": True}}}}
        ready = {"type": "frame", "input_done": None, "native_input_sequence": None,
                 "busy": False, "intentions": [], "state": {"observation": {
                     "ready": True, "combat": {"dead": False, "preparation_active": False}}}}
        def presentation(client, predicate, seconds):
            self.assertFalse(predicate(queued))
            self.assertTrue(predicate(ready))
            return ready
        def release(client, predicate, seconds):
            ack = {"type": "native_release", "sequence": 7}
            self.assertTrue(predicate(ack))
            return ack
        with patch.object(harness, "ascii_frame", side_effect=presentation), \
                patch.object(harness, "frame", side_effect=release):
            result = harness.native_press(object(), lambda name, down: None, "Down", "down",
                                          wait_for_ready=True)
        self.assertIs(result, ready)

    def test_failed_release_after_success_is_reported_without_another_wait(self):
        harness = ProcessTestCase()
        def presentation(client, predicate, seconds):
            frame = {"input_done": "down", "native_input_sequence": 1, "busy": False,
                     "intentions": [{"phase": "queued"}]}
            self.assertTrue(predicate(frame), "explicit queue-only completion must remain supported")
            return frame
        def native(name, down):
            if not down:
                raise OSError("release failed")
        with patch.object(harness, "ascii_frame", side_effect=presentation), \
                patch.object(harness, "frame") as release_wait, \
                self.assertRaisesRegex(OSError, "release failed"):
            harness.native_press(object(), native, "Down", "down", wait_for_simulation=False)
        release_wait.assert_not_called()


class CreaturePackages(unittest.TestCase):
    def test_transport_probes_pin_each_resource_check_range_and_payment(self):
        from process_rule_fixtures import TRANSPORT_ABILITIES, transport_ability
        cases = [
            ("stamina", "physical", {"kind": "selected_weapon"}, {"kind": "melee"}),
            ("mana", "cognitive", {"kind": "named", "skill": "spellcasting"}, {"kind": "sight", "range": 6}),
            ("focus", "spiritual", {"kind": "named", "skill": "intimidation"}, {"kind": "sight", "range": 6}),
        ]
        for identity, (pool, defense, skill, reach) in zip(TRANSPORT_ABILITIES, cases):
            with self.subTest(identity=identity):
                probe = transport_ability(identity)
                self.assertEqual(probe["cost"], {"resource": pool, "start": 1, "resolution": 1})
                self.assertEqual(probe["reach"], reach)
                self.assertEqual(probe["strikes"]["strikes"][0]["check"], {
                    "actor": "user", "skill": skill,
                    "against": {"kind": "defense", "defense": defense}})
                probe["cost"]["start"] = 100
                self.assertEqual(transport_ability(identity)["cost"]["start"], 1)
        fear = transport_ability("fixture_focus_fear")["strikes"]["strikes"][0]
        self.assertEqual(fear["on_success"], [{"kind": "apply_condition", "condition": "fear", "duration": 300}])
        self.assertEqual(fear["descriptors"], ["fear", "mind_affecting"])
        with self.assertRaises(ValueError):
            transport_ability("magic_bolt")

    def test_creature_fixture_uses_bundled_rules_and_generic_modifiers(self):
        from pathlib import Path
        import tempfile
        import tomllib
        from types import SimpleNamespace
        from process_harness import ROOT, creature_package
        fixture_rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text(encoding="utf-8"))
        with tempfile.TemporaryDirectory() as directory:
            harness = ProcessTestCase()
            harness.directory = Path(directory)
            harness.bin = Path(directory)
            harness.suffix = ""
            with patch("ruleset_sources.load_rules", return_value=fixture_rules) as resolve, \
                    patch("process_harness.subprocess.run", return_value=SimpleNamespace(returncode=0, stderr="")) as validate:
                package = creature_package(harness, "creature", abilities=("fixture_ability",),
                                           ability_aliases={"fixture_ability": ("fixture_mana_bolt", "Fixture Spell")},
                                           target_x=4)
            resolve.assert_called_once_with(package / "rules.toml", harness.bin / "tor-scenario")
            validate.assert_called_once()
            self.assertEqual(validate.call_args.args[0], [harness.bin / "tor-scenario", "validate", package])
            manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
            self.assertTrue((package / manifest["ruleset"]).is_file())
            build = manifest["characters"][0]["creature"]
            self.assertEqual(build["starting_skills"], ["heavy_weaponry", "spellcasting"])
            self.assertEqual(build["restoration"], "choose_again")
            self.assertEqual(build["hit_dice"], [{"source": "warrior"}, {"source": "mage", "purchases": [
                {"pool": "advancement", "selection": {"kind": "skill_rank", "skill": "intimidation"}}]}])
            self.assertEqual(fixture_rules["constants"]["racial_point_grants"],
                             [{"pool": "advancement", "points": 1}])
            species = manifest["creatures"]["species"]["human"]
            rules = tomllib.loads((package / manifest["ruleset"]).read_text(encoding="utf-8"))
            self.assertTrue(any(value["id"] == "fixture_ability" and value["name"] == "Fixture Spell" for value in rules["abilities"]))
            region = tomllib.loads((package / "regions/1.toml").read_text(encoding="utf-8"))
            self.assertEqual(region["actors"][-1]["at"], [4, 1, 0])
            self.assertFalse({"power_strike", "magic_bolt", "fear"} & {row["id"] for row in rules["abilities"]})
            self.assertFalse({"fixture_stamina_strike", "fixture_focus_fear"} & {row["id"] for row in rules["abilities"]})
            self.assertNotIn("grants", species)
            self.assertIn({"scope": "always", "op": {"kind": "grant_ability", "ability": "fixture_ability"}}, species["modifiers"])


class ProcessDeadlines(unittest.TestCase):
    def child(self, source):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        process = Process(sys.executable, ["-u", "-c", source], diagnostics=temporary.name)
        self.addCleanup(process.stop)
        process.reader.join(5)
        self.assertFalse(process.reader.is_alive(), "fixture child must finish producing")
        process.child.wait(timeout=5)
        return process

    def test_expired_deadline_rejects_already_queued_matching_output(self):
        process = self.child("print('late')")
        with patch("process_harness.time.monotonic", side_effect=[0, 2, 2, 2]), \
                self.assertRaisesRegex(AssertionError, "deadline"):
            process.until(lambda line: line == "late", seconds=1)

    def test_nonmatching_chatter_cannot_extend_deadline(self):
        process = self.child("print('noise'); print('late')")
        with patch("process_harness.time.monotonic", side_effect=[0, 0, 0, 2, 2, 2]), \
                self.assertRaisesRegex(AssertionError, "deadline"):
            process.until(lambda line: line == "late", seconds=1)

    def test_predicate_that_finishes_late_cannot_succeed(self):
        process = self.child("print('ready')")
        now = [0]
        def predicate(line):
            now[0] = 2
            return True
        with patch("process_harness.time.monotonic", side_effect=lambda: now[0]), \
                self.assertRaisesRegex(AssertionError, "deadline"):
            process.until(predicate, seconds=1)

    def test_eof_includes_the_output_and_exit_status(self):
        process = self.child("print('evidence'); raise SystemExit(7)")
        with self.assertRaisesRegex(AssertionError, "Unexpected process exit") as caught:
            process.until(lambda line: False)
        self.assertIn("evidence", str(caught.exception))
        self.assertIn("7", str(caught.exception))

    def test_predicate_error_keeps_original_exception_and_diagnostics(self):
        process = self.child("print('evidence')")
        def broken(line):
            raise ValueError("fixture predicate")
        with self.assertRaisesRegex(ValueError, "fixture predicate"):
            process.until(broken)
        self.assertTrue(process.failed)
        self.assertIn("evidence", process.transcript)

    def test_large_transcript_is_disk_backed_and_failure_tail_is_bounded(self):
        process = self.child("for n in range(1000): print(str(n) + ':' + 'x' * 1000)")
        with self.assertRaises(AssertionError) as caught:
            process.until(lambda line: False)
        self.assertLess(len(str(caught.exception)), 70000)
        self.assertEqual(len(process.transcript), 1000)
        self.assertTrue(process.transcript[999].startswith("999:"))
        self.assertTrue(process.transcript[998:][0].startswith("998:"))
        self.assertGreater(process.stdout_path.stat().st_size, 1000000)
        self.assertNotIsInstance(process.transcript, list)


class FailureRetention(unittest.TestCase):
    def run_case(self, failing, expected_exit=False):
        outer = self
        class Fixture(ProcessTestCase):
            @classmethod
            def setUpClass(cls):
                cls.bin = Path(sys.executable).parent
                cls.suffix = ""
            def runTest(self):
                outer.directory = self.directory
                self.save.write_bytes(b"synthetic save before failure")
                child = self.launch(Path(sys.executable).name, ["-u", "-c", "print('ready')"])
                child.until(lambda line: line == "ready")
                if expected_exit:
                    with self.assertRaisesRegex(AssertionError, "Unexpected process exit"):
                        child.until(lambda line: False)
                if failing:
                    self.fail("synthetic assertion")
        Fixture.setUpClass()
        result = unittest.TestResult()
        with patch.dict(os.environ, {"TOR_TEST_ARTIFACTS": ""}):
            Fixture().run(result)
        return result

    def test_failed_test_retains_save_and_process_diagnostics(self):
        result = self.run_case(True)
        self.addCleanup(__import__("shutil").rmtree, self.directory, ignore_errors=True)
        self.assertEqual(len(result.failures), 1)
        self.assertEqual((self.directory / "game.db").read_bytes(), b"synthetic save before failure")
        metadata = list(self.directory.glob("process-*/process.json"))
        self.assertEqual(len(metadata), 1)
        self.assertIn("command", json.loads(metadata[0].read_text()))
        self.assertTrue(list(self.directory.glob("process-*/stdout.log")))

    def test_successful_test_removes_its_temporary_directory(self):
        self.assertTrue(self.run_case(False).wasSuccessful())
        self.assertFalse(self.directory.exists())

    def test_expected_child_exit_does_not_retain_successful_case(self):
        self.assertTrue(self.run_case(False, expected_exit=True).wasSuccessful())
        self.assertFalse(self.directory.exists())


class DiskOutput(unittest.TestCase):
    def test_concurrent_unicode_fifo_eof_and_stable_transcript_slices(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "stdout.log"
            with OutputQueue(path) as pending:
                transcript = Transcript(path)
                values = [f"{n}: café 雨 🦀" for n in range(200)]
                def produce():
                    for value in values:
                        pending.put(value)
                    pending.put(None)
                producer = threading.Thread(target=produce)
                producer.start()
                for value in values:
                    self.assertEqual(pending.get(timeout=5), value)
                    transcript.append(value)
                producer.join(5)
                self.assertFalse(producer.is_alive())
                self.assertIsNone(pending.get(timeout=0))
                self.assertEqual(list(transcript), values)
                self.assertEqual(transcript[194:], values[194:])
                self.assertEqual(transcript[-3], values[-3])
                self.assertEqual(transcript[10:2:-2], values[10:2:-2])
                self.assertEqual(transcript + ["after"], values + ["after"])
                self.assertEqual(pending.read_between(0, pending.offset), "\n".join(values))
                with self.assertRaises(IndexError):
                    _ = transcript[200]

    def test_unread_lines_stay_on_disk_and_empty_wait_is_bounded(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "stdout.log"
            with OutputQueue(path) as pending:
                transcript = Transcript(path)
                with self.assertRaises(queue.Empty):
                    pending.get(timeout=0)
                pending.put("first")
                pending.put("second")
                pending.put(None)
                transcript.append(pending.get(timeout=0))
                self.assertEqual(list(transcript), ["first"])
                self.assertEqual(pending.queue, ["second", None])
                offset = pending.offset
                self.assertEqual(pending.get(timeout=0), "second")
                self.assertEqual(pending.read_between(offset, pending.offset), "second")

    def test_real_multiline_output_and_interactive_prompt_are_preserved(self):
        from process_harness import AdventureProcess
        with tempfile.TemporaryDirectory() as directory:
            child = AdventureProcess(sys.executable, ["-u", "-c", "print('café\\n雨'); print('> ', end='', flush=True)"], diagnostics=directory)
            try:
                self.assertEqual(child.until(lambda line: line == "> "), "café\n雨\n> ")
            finally:
                child.stop()

    def test_separate_stderr_is_captured_only_during_stop(self):
        with tempfile.TemporaryDirectory() as directory:
            child = Process(sys.executable, ["-u", "-c", "import sys; print('diagnostic', file=sys.stderr); print('ready'); sys.stdin.readline()"], separate_stderr=True, diagnostics=directory)
            try:
                self.assertEqual(child.until(lambda line: line == "ready"), "ready")
                self.assertFalse((Path(directory) / "stderr.log").exists())
            finally:
                child.stop()
            self.assertEqual((Path(directory) / "stderr.log").read_text(), "diagnostic\n")
            self.assertTrue(child.lines.writer.closed)
            self.assertTrue(child.lines.reader.closed)
            child.stop()  # repeated explicit/registered cleanup is harmless


class ArtifactRetention(unittest.TestCase):
    def test_compression_roundtrip_mismatch_keeps_raw_evidence(self):
        import gzip
        for publish in (False, True):
            with self.subTest(publish=publish), tempfile.TemporaryDirectory() as root:
                source = Path(root) / "stdout.log"
                evidence = b"original diagnostic\n" * 100000
                source.write_bytes(evidence)
                destination = Path(root) / "copied.log" if publish else source
                compress = gzip.open
                def wrong_read(path, mode, *args, **kwargs):
                    return io.BytesIO(b"incorrect decoded bytes") if mode == "rb" else compress(path, mode, *args, **kwargs)
                with patch("process_harness.gzip.open", side_effect=wrong_read), \
                        self.assertRaisesRegex(OSError, "round-trip verification"):
                    if publish:
                        ProcessTestDirectory.copy_artifact(source, destination)
                    else:
                        ProcessTestDirectory.compress_logs(Path(root))
                self.assertEqual(source.read_bytes(), evidence)
                self.assertTrue(destination.with_name(destination.name + ".gz").exists())

    def test_compression_verification_read_error_keeps_raw_evidence(self):
        import gzip
        with tempfile.TemporaryDirectory() as root:
            source = Path(root) / "stdout.log"
            evidence = b"original diagnostic\n" * 100000
            source.write_bytes(evidence)
            compress = gzip.open
            def failed_read(path, mode, *args, **kwargs):
                if mode == "rb":
                    raise OSError("fixture verification read failed")
                return compress(path, mode, *args, **kwargs)
            with patch("process_harness.gzip.open", side_effect=failed_read), \
                    self.assertRaisesRegex(OSError, "verification read failed"):
                ProcessTestDirectory.compress_logs(Path(root))
            self.assertEqual(source.read_bytes(), evidence)

    def test_raw_hash_is_taken_before_compression(self):
        import gzip
        with tempfile.TemporaryDirectory() as root:
            source = Path(root) / "stdout.log"
            source.write_bytes(b"original diagnostic\n" * 100000)
            changed = b"different diagnostic\n" * 100000
            compress = gzip.open
            def mutate_before_compression(path, mode, *args, **kwargs):
                if mode == "xb":
                    source.write_bytes(changed)
                return compress(path, mode, *args, **kwargs)
            with patch("process_harness.gzip.open", side_effect=mutate_before_compression), \
                    self.assertRaisesRegex(OSError, "round-trip verification"):
                ProcessTestDirectory.copy_artifact(source, Path(root) / "copied.log")
            self.assertEqual(source.read_bytes(), changed)

    def test_in_place_compression_never_overwrites_an_existing_archive(self):
        with tempfile.TemporaryDirectory() as root:
            source = Path(root) / "stdout.log"
            evidence = b"original diagnostic\n" * 100000
            source.write_bytes(evidence)
            compressed = source.with_name(source.name + ".gz")
            compressed.write_bytes(b"independent archive")
            with self.assertRaises(FileExistsError):
                ProcessTestDirectory.compress_logs(Path(root))
            self.assertEqual(source.read_bytes(), evidence)
            self.assertEqual(compressed.read_bytes(), b"independent archive")

    def test_large_logs_stream_from_source_without_raw_staging_copies(self):
        import gzip
        import shutil
        with tempfile.TemporaryDirectory() as root:
            with patch("process_harness.ROOT", Path(root)):
                directory = ProcessTestDirectory()
            source = Path(directory.name)
            nested = source / "process-1"
            nested.mkdir()
            evidence = b"complete diagnostic\n" * 120000
            paths = [nested / "stdout.log", nested / "startup.jsonl"]
            for path in paths:
                path.write_bytes(evidence)
                os.utime(path, (1600000000, 1600000000))
            (nested / "stderr.log").write_bytes(b"small diagnostic\n")
            archive = gzip.compress(b"already compressed")
            (nested / "previous.log.gz").write_bytes(archive)
            (source / "game.db").write_bytes(evidence)
            (source / "game.db-wal").write_bytes(b"complete sidecar")
            (source / "empty").mkdir()
            target = Path(root) / "artifacts"
            copy, stream_copy, compress = shutil.copyfile, shutil.copyfileobj, gzip.open
            reads = []
            def copy_without_raw_log(original, destination, **kwargs):
                self.assertNotIn(Path(original), paths, "large logs must never be copied raw")
                return copy(original, destination, **kwargs)
            def bounded_stream(original, output, *args, **kwargs):
                if not str(output.name).endswith((".log.gz", ".jsonl.gz")):
                    return stream_copy(original, output, *args, **kwargs)
                self.assertIn(Path(original.name), paths, "compression must read the source log")
                class Reader:
                    def read(inner, size=-1):
                        self.assertGreater(size, 0, "whole-file reads exhaust memory")
                        self.assertLessEqual(size, 1024 * 1024)
                        reads.append(size)
                        return original.read(size)
                return stream_copy(Reader(), output, *args, **kwargs)
            def compress_without_raw_copy(path, *args, **kwargs):
                self.assertFalse(Path(path).with_suffix("").exists())
                self.assertTrue(all(log.exists() for log in paths))
                return compress(path, *args, **kwargs)
            with patch.dict(os.environ, {"TOR_TEST_ARTIFACTS": str(target)}), \
                    patch("process_harness.shutil.copyfile", side_effect=copy_without_raw_log), \
                    patch("process_harness.shutil.copyfileobj", side_effect=bounded_stream), \
                    patch("process_harness.gzip.open", side_effect=compress_without_raw_copy):
                directory.cleanup(failed=True, test_id="fixture.streaming")
            retained = Path(directory.name)
            self.assertFalse(source.exists())
            self.assertGreater(len(reads), 2)
            for path in paths:
                compressed = retained / path.relative_to(source).with_name(path.name + ".gz")
                with compress(compressed, "rb") as log:
                    self.assertEqual(log.read(), evidence)
                self.assertEqual(compressed.stat().st_mtime_ns, 1600000000000000000)
                self.assertFalse(compressed.with_suffix("").exists())
            self.assertEqual((retained / "game.db").read_bytes(), evidence)
            self.assertEqual((retained / "game.db-wal").read_bytes(), b"complete sidecar")
            self.assertEqual((retained / "process-1/stderr.log").read_bytes(), b"small diagnostic\n")
            self.assertEqual((retained / "process-1/previous.log.gz").read_bytes(), archive)
            self.assertTrue((retained / "empty").is_dir())
            self.assertEqual(json.loads((retained / "test.json").read_text())["test_id"], "fixture.streaming")

    def test_mid_compression_failure_preserves_source_and_removes_staging(self):
        import gzip
        with tempfile.TemporaryDirectory() as root:
            with patch("process_harness.ROOT", Path(root)):
                directory = ProcessTestDirectory()
            source = Path(directory.name)
            evidence = b"original output\n" * 150000
            (source / "stdout.log").write_bytes(evidence)
            (source / "game.db").write_bytes(b"complete save")
            target = Path(root) / "artifacts"
            compress = gzip.open
            writes = []
            @contextmanager
            def interrupted_compression(path, *args, **kwargs):
                self.assertFalse(Path(path).with_suffix("").exists(), "raw staging copy used disk first")
                with compress(path, *args, **kwargs) as output:
                    class Writer:
                        def write(inner, data):
                            writes.append(len(data))
                            output.write(data)
                            if len(writes) == 2:
                                raise OSError("fixture compression interrupted")
                    yield Writer()
            with patch.dict(os.environ, {"TOR_TEST_ARTIFACTS": str(target)}), \
                    patch("process_harness.gzip.open", side_effect=interrupted_compression), \
                    self.assertRaisesRegex(OSError, "compression interrupted"):
                directory.cleanup(failed=True)
            self.assertEqual(len(writes), 2)
            self.assertEqual(Path(directory.name), source)
            self.assertEqual((source / "stdout.log").read_bytes(), evidence)
            self.assertEqual((source / "game.db").read_bytes(), b"complete save")
            self.assertEqual(list(target.iterdir()), [])

    def test_existing_compressed_sibling_is_not_overwritten_or_double_compressed(self):
        with tempfile.TemporaryDirectory() as root:
            with patch("process_harness.ROOT", Path(root)):
                directory = ProcessTestDirectory()
            source = Path(directory.name)
            evidence = b"original output\n" * 100000
            (source / "stdout.log").write_bytes(evidence)
            (source / "stdout.log.gz").write_bytes(b"independent archive")
            target = Path(root) / "artifacts"
            with patch.dict(os.environ, {"TOR_TEST_ARTIFACTS": str(target)}), \
                    self.assertRaisesRegex(OSError, "stdout.log.gz"):
                directory.cleanup(failed=True)
            self.assertEqual((source / "stdout.log").read_bytes(), evidence)
            self.assertEqual((source / "stdout.log.gz").read_bytes(), b"independent archive")
            self.assertEqual(list(target.iterdir()), [])

    def test_failed_artifacts_publish_with_identity_and_lossless_compressed_output(self):
        import gzip
        with tempfile.TemporaryDirectory() as root:
            with patch("process_harness.ROOT", Path(root)):
                directory = ProcessTestDirectory()
            original = Path(directory.name)
            saved = b"synthetic generated-floor save"
            (original / "game.db").write_bytes(saved)
            evidence = "retained diagnostic\n" * 100000
            (original / "stdout.log").write_text(evidence)
            target = Path(root) / "artifacts"
            with patch.dict(os.environ, {"TOR_TEST_ARTIFACTS": str(target)}):
                directory.cleanup(failed=True, test_id="fixture.resume")
            retained = Path(directory.name)
            self.assertEqual(retained.parent, target)
            self.assertFalse(original.exists())
            self.assertEqual((retained / "game.db").read_bytes(), saved)
            with gzip.open(retained / "stdout.log.gz", "rt") as log:
                self.assertEqual(log.read(), evidence)
            self.assertEqual(json.loads((retained / "test.json").read_text())["test_id"], "fixture.resume")

    def test_real_child_handles_close_before_artifact_copy_and_compression(self):
        import gzip
        import shutil
        children = []
        class Fixture(ProcessTestCase):
            bin, suffix = Path(sys.executable).parent, ""
            def runTest(self):
                child = self.launch(Path(sys.executable).name, ["-u", "-c",
                    "import sys; print('x' * 1100000); print('ready'); sys.stdin.readline()"])
                children.append(child)
                child.until(lambda line: line == "ready")
                self.fail("synthetic failure with an owned live child")
        compress, copy = gzip.open, shutil.copytree
        def assert_closed():
            child = children[0]
            self.assertFalse(child.reader.is_alive())
            self.assertTrue(child.lines.reader.closed)
            self.assertTrue(child.lines.writer.closed)
            self.assertIsNotNone(child.child.poll())
        def compress_after_close(*args, **kwargs):
            assert_closed()
            return compress(*args, **kwargs)
        def copy_after_close(*args, **kwargs):
            assert_closed()
            return copy(*args, **kwargs)
        with tempfile.TemporaryDirectory() as root, patch("process_harness.ROOT", Path(root)), \
                patch.dict(os.environ, {"TOR_TEST_ARTIFACTS": str(Path(root) / "artifacts")}), \
                patch("process_harness.gzip.open", side_effect=compress_after_close), \
                patch("process_harness.shutil.copytree", side_effect=copy_after_close):
            result = unittest.TestResult()
            Fixture().run(result)
            self.assertEqual(len(result.failures), 1)
            self.assertEqual(result.errors, [])
            logs = list((Path(root) / "artifacts").glob("*/process-*/stdout.log.gz"))
            self.assertEqual(len(logs), 1)
            with compress(logs[0], "rt") as log:
                self.assertTrue(log.read().endswith("ready\n"))

    def test_transfer_failure_preserves_original_and_raises(self):
        with tempfile.TemporaryDirectory() as root:
            with patch("process_harness.ROOT", Path(root)):
                directory = ProcessTestDirectory()
            saved = Path(directory.name) / "game.db"
            saved.write_bytes(b"still recoverable")
            with patch.dict(os.environ, {"TOR_TEST_ARTIFACTS": str(Path(root) / "artifacts")}), \
                    patch("process_harness.shutil.copytree", side_effect=OSError("fixture denied")), \
                    self.assertRaisesRegex(OSError, "fixture denied"):
                directory.cleanup(failed=True, test_id="fixture.resume")
            self.assertEqual(saved.read_bytes(), b"still recoverable")

    def test_partial_copy_failure_never_changes_source_or_publishes_partial_artifact(self):
        with tempfile.TemporaryDirectory() as root:
            with patch("process_harness.ROOT", Path(root)):
                directory = ProcessTestDirectory()
            source = Path(directory.name)
            (source / "game.db").write_bytes(b"complete save")
            (source / "game.db-wal").write_bytes(b"complete sidecar")
            (source / "stdout.log").write_bytes(b"x" * 1100000)
            target = Path(root) / "artifacts"
            def partial_copy(original, staging, **kwargs):
                staging.mkdir()
                (staging / "game.db").write_bytes(b"partial")
                raise OSError("fixture copy interrupted")
            with patch.dict(os.environ, {"TOR_TEST_ARTIFACTS": str(target)}), \
                    patch("process_harness.shutil.copytree", side_effect=partial_copy), \
                    self.assertRaisesRegex(OSError, "copy interrupted"):
                directory.cleanup(failed=True)
            self.assertEqual((source / "game.db").read_bytes(), b"complete save")
            self.assertEqual((source / "game.db-wal").read_bytes(), b"complete sidecar")
            self.assertEqual((source / "stdout.log").read_bytes(), b"x" * 1100000)
            self.assertEqual(list(target.iterdir()), [])

    def test_windows_locked_source_cleanup_keeps_published_artifact_usable(self):
        import shutil
        with tempfile.TemporaryDirectory() as root:
            with patch("process_harness.ROOT", Path(root)):
                directory = ProcessTestDirectory()
            source = Path(directory.name)
            (source / "game.db").write_bytes(b"locked but readable save")
            (source / "game.db-wal").write_bytes(b"unchanged sidecar")
            remove = shutil.rmtree
            def locked_remove(path, *args, **kwargs):
                if Path(path) == source:
                    raise PermissionError("fixture Windows SQLite handle is still open")
                return remove(path, *args, **kwargs)
            with patch.dict(os.environ, {"TOR_TEST_ARTIFACTS": str(Path(root) / "artifacts")}), \
                    patch("process_harness.shutil.rmtree", side_effect=locked_remove):
                directory.cleanup(failed=True, test_id="fixture.locked")
            retained = Path(directory.name)
            self.assertEqual((retained / "game.db").read_bytes(), (source / "game.db").read_bytes())
            self.assertEqual((retained / "game.db-wal").read_bytes(), b"unchanged sidecar")
            self.assertIn("SQLite handle", json.loads((retained / "test.json").read_text())["source_cleanup_error"])

    def test_failed_test_cleanup_never_calls_the_server_save_barrier(self):
        saved = []
        class FakeProcess:
            failed = False
            def __init__(self, *args, **kwargs):
                pass
            def stop(self, *, save=True):
                saved.append(save)
        class Fixture(ProcessTestCase):
            bin, suffix = Path("."), ""
            def runTest(self):
                self.launch("tor-server", [], cls=FakeProcess)
                self.fail("synthetic failure after server readiness")
        with tempfile.TemporaryDirectory() as root, patch("process_harness.ROOT", Path(root)), \
                patch.dict(os.environ, {"TOR_TEST_ARTIFACTS": str(Path(root) / "artifacts")}):
            result = unittest.TestResult()
            Fixture().run(result)
            self.assertEqual(len(result.failures), 1)
            self.assertEqual(result.errors, [])
            self.assertEqual(saved, [False])


class SaveBarrierPublication(unittest.TestCase):
    def test_barrier_markers_are_published_only_after_producer_handles_close(self):
        with tempfile.TemporaryDirectory() as directory:
            harness = ProcessTestCase()
            harness.directory = Path(directory)
            armed = Path(directory) / "save-barrier.armed"
            release = Path(directory) / "save-barrier.release"
            entered = Path(directory) / "save-barrier.entered"
            visible_before_close = []
            close = os.close

            def observe_close(fd):
                # A Windows worker cannot rename/delete a marker while the
                # producing CRT handle is still open. Observe that exact window
                # deterministically instead of relying on thread scheduling.
                visible_before_close.append((armed.exists(), release.exists()))
                close(fd)

            with patch("process_harness.os.close", side_effect=observe_close):
                with harness.held_save_writer() as barrier:
                    self.assertTrue(armed.exists())
                    armed.replace(entered)
                    barrier.wait()
                    barrier.assert_held()
            self.assertTrue(release.exists())
            self.assertEqual(visible_before_close, [(False, False), (False, False)])
            self.assertEqual({path.name for path in Path(directory).iterdir()},
                             {"save-barrier.entered", "save-barrier.release"})

    def test_barrier_failure_includes_preserved_io_error_details(self):
        with tempfile.TemporaryDirectory() as directory:
            harness = ProcessTestCase()
            harness.directory = Path(directory)
            with harness.held_save_writer() as barrier:
                (Path(directory) / "save-barrier.finished").write_text("error")
                (Path(directory) / "save-barrier.error.json").write_text(json.dumps({
                    "operation": "remove_release_marker", "kind": "PermissionDenied",
                    "raw_os_error": 32, "message": "sharing violation"}))
                with self.assertRaisesRegex(AssertionError, "sharing violation"):
                    barrier.assert_released()

    def test_publication_failure_cleans_staging_and_preserves_the_error(self):
        replace = Path.replace
        for suffix in ("armed", "release"):
            with self.subTest(marker=suffix), tempfile.TemporaryDirectory() as directory:
                harness = ProcessTestCase()
                harness.directory = Path(directory)
                def fail_publication(source, destination):
                    if str(destination).endswith("." + suffix):
                        raise PermissionError("fixture publication failure")
                    return replace(source, destination)
                with patch.object(Path, "replace", autospec=True, side_effect=fail_publication):
                    with self.assertRaisesRegex(PermissionError, "fixture publication failure"):
                        with harness.held_save_writer():
                            pass
                self.assertFalse(list(Path(directory).glob("*.pending-*")))
                self.assertFalse((Path(directory) / ("save-barrier." + suffix)).exists())

    def test_rearming_clears_previous_io_error_details(self):
        with tempfile.TemporaryDirectory() as directory:
            harness = ProcessTestCase()
            harness.directory = Path(directory)
            detail = Path(directory) / "save-barrier.error.json"
            detail.write_text('{"message":"old failure"}')
            with harness.held_save_writer():
                self.assertFalse(detail.exists())


class BuildTiming(unittest.TestCase):
    def setUp(self):
        # Parallel workers inherit the parent's prebuilt directory; these tests
        # exercise the build itself.
        prebuilt = os.environ.pop("TOR_TEST_BINARIES", None)
        if prebuilt is not None:
            self.addCleanup(os.environ.__setitem__, "TOR_TEST_BINARIES", prebuilt)

    def test_a_parallel_parent_build_is_reused_without_building_again(self):
        with patch("process_harness._binaries", None),                 patch.dict(os.environ, {"TOR_TEST_BINARIES": "prebuilt-bin"}),                 patch("process_harness.subprocess.run") as run:
            self.assertEqual(binaries(), Path("prebuilt-bin"))
        run.assert_not_called()

    def test_build_timing_records_success_failure_and_timeout_without_swallowing_them(self):
        cases = [(None, "passed", 0), (subprocess.CalledProcessError(2, ["cargo"]), "failed", 2),
                 (subprocess.TimeoutExpired(["cargo"], 600), "timeout", None)]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "timings.jsonl"
            for error, outcome, code in cases:
                with self.subTest(outcome=outcome), patch("process_harness._binaries", None), \
                        patch.dict(os.environ, {"TOR_TEST_TIMINGS": str(path), "TOR_TEST_PROFILE": "debug"}), \
                        patch("process_harness.subprocess.run", side_effect=error), \
                        patch("process_harness.subprocess.check_output", return_value=json.dumps({"target_directory": directory})):
                    if error:
                        with self.assertRaises(type(error)):
                            binaries()
                    else:
                        self.assertEqual(binaries(), Path(directory) / "debug")
                    row = json.loads(path.read_text().splitlines()[-1])
                    self.assertEqual((row["kind"], row["outcome"], row["returncode"]), ("build", outcome, code))
                    self.assertGreaterEqual(row["seconds"], 0)


if __name__ == "__main__":
    unittest.main()
