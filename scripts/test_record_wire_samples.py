"""Pure message-selection and authored-fixture contracts for the wire recorder."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import time
import tomllib
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import nutrition_package
import record_wire_samples
from process_harness import ROOT, is_graphical_test
from process_work_budget import ProcessBudgetExceeded, ProcessWorkBudget
from record_wire_samples import has_condition_event


ESCAPE_SAMPLE = {
    "type": "snapshot", "request_id": "exact-response-request",
    "snapshot": {"state": {"observation": {
        "combat": {"objective": "retrieve_and_exit", "victory": True, "terminal": True},
        "inventory": [{"id": "exact-opaque-item", "name": "Amulet of Yendor"}],
    }}, "history": {"entries": [{"content": {"type": "action", "event": {
        "type": "escaped", "item": "exact-opaque-item"}}}]}, "cursor": "exact-cursor"},
}


def recording_case(failure=None):
    """A pure lifecycle witness; none of its hooks launch applications."""
    trace = []

    def step(name):
        trace.append(name)
        if failure == name:
            raise AssertionError("recording failed in " + name)

    class Case(unittest.TestCase):
        @classmethod
        def setUpClass(cls):
            cls.addClassCleanup(step, "class_cleanup")
            step("class_setup")

        def setUp(self):
            self.addCleanup(step, "cleanup")
            step("setup")

        def runTest(self):
            self.artifact = copy.deepcopy(ESCAPE_SAMPLE)
            self.escape_sample = self.artifact
            self.server_samples = {"zz.amulet_escape": self.artifact}
            step("body")

        def tearDown(self):
            step("teardown")

        @classmethod
        def tearDownClass(cls):
            step("class_teardown")

    case = Case()
    return case, trace


class CompleteRecordingContracts(unittest.TestCase):
    def test_preview_request_fixture_echoes_a_complete_public_action(self):
        commands = [entry["request"]["command"] for entry in record_wire_samples.CLIENT
                    if entry["type"] == "request" and entry["request"]["type"] == "command"]
        previews = [command for command in commands if command["type"] == "preview_ability_weapon"]
        self.assertEqual(len(previews), 1)
        self.assertEqual(previews[0], {
            "type": "preview_ability_weapon", "expected_revision": "3", "ability": "basic_melee",
            "invocation": {"aim": {"kind": "actor_target", "target": "a_" + "02" * 32},
                           "weapon": {"kind": "default"}},
            "supplies": {"kind": "carried"},
        })

    def test_exact_artifact_is_returned_only_after_all_lifecycle_hooks_finish(self):
        case, trace = recording_case()
        artifact = record_wire_samples.run_recording_case(case, "artifact")
        self.assertEqual(artifact, ESCAPE_SAMPLE)
        self.assertIsNot(artifact, case.artifact)
        self.assertEqual(trace, ["class_setup", "setup", "body", "teardown", "cleanup",
                                 "class_teardown", "class_cleanup"])

    def test_every_setup_test_teardown_and_cleanup_failure_blocks_the_artifact(self):
        for failure in ("class_setup", "setup", "body", "teardown", "cleanup",
                        "class_teardown", "class_cleanup"):
            with self.subTest(failure=failure):
                case, trace = recording_case(failure)
                with self.assertRaisesRegex(AssertionError, "recording failed in " + failure):
                    record_wire_samples.run_recording_case(case, "artifact")
                self.assertIn("class_cleanup", trace)
                if failure != "class_setup":
                    self.assertIn("cleanup", trace)

    def test_skip_expected_failure_unexpected_success_and_missing_artifact_are_not_recordings(self):
        for disposition in ("skip", "expected_failure", "unexpected_success", "missing_artifact"):
            with self.subTest(disposition=disposition):
                case, _ = recording_case("body" if disposition == "expected_failure" else None)
                if disposition == "skip":
                    type(case).runTest = unittest.skip("no actual recording")(type(case).runTest)
                elif disposition in ("expected_failure", "unexpected_success"):
                    type(case).runTest = unittest.expectedFailure(type(case).runTest)
                attribute = "missing" if disposition == "missing_artifact" else "artifact"
                with self.assertRaises(AssertionError):
                    record_wire_samples.run_recording_case(case, attribute)

    def test_amulet_adapter_reuses_one_existing_case_and_preserves_the_raw_response(self):
        case, trace = recording_case()
        samples = {"existing": {"type": "palette"}}
        recorder = record_wire_samples.Recorder()
        with patch("record_wire_samples.amulet_recording_case", return_value=case) as factory:
            recorder.record_amulet_escape(samples)
        factory.assert_called_once_with()
        self.assertEqual(samples, {"existing": {"type": "palette"}, "zz.amulet_escape": ESCAPE_SAMPLE})
        self.assertEqual(trace[-1], "class_cleanup")
        self.assertEqual(samples["zz.amulet_escape"]["request_id"], "exact-response-request")

    def test_amulet_factory_selects_only_the_existing_non_graphical_acceptance_case(self):
        case = record_wire_samples.amulet_recording_case()
        self.assertEqual(case.id(), "test_amulet_process.AmuletProcesses."
                         "test_unique_identity_revisits_source_free_restore_and_text_surface_escape")
        self.assertFalse(is_graphical_test(case))

    def test_scare_factory_reuses_existing_non_graphical_paid_pickup_case(self):
        case = record_wire_samples.scare_pickup_recording_case()
        self.assertEqual(case.id(), "test_living_dungeon_process.LivingDungeonProcesses."
                         "test_paid_scare_pickup_curse_crumble_remove_curse_and_restart")
        self.assertFalse(is_graphical_test(case))

    def test_terminal_escape_assertion_requires_immediate_exact_identity_success(self):
        case = record_wire_samples.amulet_recording_case()
        before = {"state": {"wizard_game": False, "observation": {
            "tick": "1000", "combat": {"victory": False, "terminal": False, "dead": False},
            "inventory": [{"id": "exact-item", "name": "Amulet of Yendor", "quantity": "1"}],
            "ground_items": [],
            "visible_cells": [{"position": {"x": 0, "y": 0, "z": 0}, "key": "surface"}],
        }}, "history": []}
        escaped = copy.deepcopy(before)
        escaped["state"]["observation"]["combat"].update(victory=True, terminal=True)
        escaped["history"] = [{"content": {"type": "action", "event": {
            "type": "escaped", "item": "exact-item"}}}]
        case.assert_escape(before, escaped, "exact-item")
        for changed in ("tick", "identity", "duplicate_event", "victory", "terminal", "dead", "location"):
            with self.subTest(changed=changed):
                invalid = copy.deepcopy(escaped)
                view = invalid["state"]["observation"]
                if changed == "tick":
                    view["tick"] = "1100"
                elif changed == "identity":
                    view["inventory"][0]["id"] = "another-item"
                elif changed == "duplicate_event":
                    invalid["history"] *= 2
                elif changed == "location":
                    view["visible_cells"][0]["key"] = "fabricated-destination"
                else:
                    view["combat"][changed] = changed == "dead"
                with self.assertRaises(AssertionError):
                    case.assert_escape(before, invalid, "exact-item")

    def test_amulet_limits_preserve_work_bounds_and_fail_at_the_measured_output_ceiling(self):
        case = record_wire_samples.amulet_recording_case()
        expected = {
            "test_unique_identity_revisits_source_free_restore_and_text_surface_escape":
                (384, 40000, 64 * 1024 * 1024, 240),
            "test_native_up_escapes_only_from_the_disclosed_surface_endpoint":
                (256, 25000, 48 * 1024 * 1024, 180),
        }
        self.assertEqual(case.limits, expected)
        for method, (requests, ticks, byte_limit, _) in expected.items():
            with self.subTest(method=method), tempfile.TemporaryDirectory() as directory:
                log = Path(directory) / "stdout.log"
                log.touch()
                budget = ProcessWorkBudget(requests=requests, actor_ticks=ticks,
                                           transcript_bytes=byte_limit)
                budget.output.paths = (log,)
                # Sparse lengths exercise real file accounting without
                # generating or retaining a large synthetic console frame.
                with log.open("r+b") as stream:
                    stream.truncate(byte_limit)
                budget.check()
                with log.open("r+b") as stream:
                    stream.truncate(byte_limit + 1)
                with self.assertRaises(ProcessBudgetExceeded) as raised:
                    budget.check()
                self.assertEqual(raised.exception.bound, "transcript_bytes")
                self.assertEqual(raised.exception.limit, byte_limit)
                self.assertEqual(raised.exception.used, byte_limit + 1)

    def test_native_frames_count_actor_time_and_output_through_the_inherited_reader(self):
        self.check_native_frame_budget(actor_limit=100, byte_limit=4)

    def test_native_frame_actor_and_output_overflow_fail_before_accepting_the_frame(self):
        for actor_limit, byte_limit, bound in ((99, 4, "actor_ticks"), (100, 3, "transcript_bytes")):
            with self.subTest(bound=bound):
                self.check_native_frame_budget(actor_limit=actor_limit, byte_limit=byte_limit, bound=bound)

    def check_native_frame_budget(self, *, actor_limit, byte_limit, bound=None):
        case = record_wire_samples.amulet_recording_case()
        case.started, case.wall_seconds = time.monotonic(), 30
        case.work_budget = ProcessWorkBudget(requests=1, actor_ticks=actor_limit,
                                            transcript_bytes=byte_limit)
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "native.log"
            log.touch()
            native = SimpleNamespace(transcript=SimpleNamespace(path=log))

            def frame(tick, busy):
                return {"type": "frame", "busy": busy,
                        "state": {"observation": {"tick": str(tick)}}}

            initial, progressing, finished = frame(1000, False), frame(1050, True), frame(1100, False)
            case.work_budget.track(native, initial)
            case.work_budget.before()
            predicates = []

            def until(predicate, seconds):
                self.assertGreater(seconds, 0)
                self.assertLessEqual(seconds, 10)
                # Exercise the actual base ASCII -> overridden frame -> budget
                # path, including an ignored native release and a busy frame.
                self.assertFalse(predicate(json.dumps({"type": "native_release", "sequence": 1})))
                for value in (progressing, finished):
                    with log.open("ab") as stream:
                        stream.write(b"xx")
                    if predicate(json.dumps(value)):
                        return
                self.fail("The settled frame must satisfy the native predicate")

            def settled(value):
                predicates.append(value)
                return not value["busy"]

            native.until = until
            if bound is None:
                self.assertEqual(case.ascii_frame(native, settled, seconds=10), finished)
                self.assertEqual(predicates, [progressing, finished])
            else:
                with self.assertRaises(ProcessBudgetExceeded) as raised:
                    case.ascii_frame(native, settled, seconds=10)
                self.assertEqual(raised.exception.bound, bound)
                self.assertEqual(predicates, [progressing])
            self.assertEqual(case.work_budget.usage(), {
                "requests": 1, "actor_ticks": 100, "transcript_bytes": 4})

    def test_a_failed_subtest_cannot_publish_its_apparently_completed_artifact(self):
        case, trace = recording_case()

        def subtest_failure(subject):
            subject.artifact = copy.deepcopy(ESCAPE_SAMPLE)
            with subject.subTest(stage="captured"):
                subject.fail("captured subtest failed")

        type(case).runTest = subtest_failure
        with self.assertRaisesRegex(AssertionError, "captured subtest failed"):
            record_wire_samples.run_recording_case(case, "artifact")
        self.assertEqual(trace[-1], "class_cleanup")

    def test_amulet_adapter_does_not_add_a_sample_after_cleanup_failure(self):
        case, _ = recording_case("cleanup")
        samples = {"existing": {"type": "palette"}}
        with patch("record_wire_samples.amulet_recording_case", return_value=case):
            with self.assertRaisesRegex(AssertionError, "recording failed in cleanup"):
                record_wire_samples.Recorder().record_amulet_escape(samples)
        self.assertEqual(samples, {"existing": {"type": "palette"}})

    def test_publication_waits_for_every_hook_and_preserves_existing_output_on_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "wire-current.json"
            for failure in ("class_setup", "setup", "body", "teardown", "cleanup",
                            "class_teardown", "class_cleanup"):
                with self.subTest(failure=failure):
                    output.write_text("prior validated samples", encoding="utf-8")
                    case, _ = recording_case(failure)
                    with patch("record_wire_samples.Recorder", return_value=case):
                        with self.assertRaises(AssertionError):
                            record_wire_samples.record_samples(output=output)
                    self.assertEqual(output.read_text(encoding="utf-8"), "prior validated samples")
            case, trace = recording_case()
            with patch("record_wire_samples.Recorder", return_value=case), patch("builtins.print"):
                record_wire_samples.record_samples(output=output)
            document = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(document["protocol"], record_wire_samples.PROTOCOL)
            self.assertEqual(document["server"], [ESCAPE_SAMPLE])
            self.assertEqual(trace[-1], "class_cleanup")

    def test_failed_atomic_publication_keeps_the_prior_fixture_and_removes_staging_file(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "wire-current.json"
            output.write_text("prior validated samples", encoding="utf-8")
            case, _ = recording_case()
            with patch("record_wire_samples.Recorder", return_value=case), \
                    patch("record_wire_samples.os.replace", side_effect=OSError("write unavailable")):
                with self.assertRaisesRegex(OSError, "write unavailable"):
                    record_wire_samples.record_samples(output=output)
            self.assertEqual(output.read_text(encoding="utf-8"), "prior validated samples")
            self.assertEqual(list(Path(directory).iterdir()), [output])


ANONYMOUS_SAMPLE = {
    "type": "update", "update": {"actor": "3", "body": {"type": "observation_delta", "state": {
        "combat": {"events": [{"type": "anonymous_ability", "caster": "a_" + "11" * 32,
                              "target": "a_" + "33" * 32, "outcome": "applied"}]},
    }}, "context": {"stream": "actual-recipient-stream", "epoch": "1"},
        "cursor": {"sequence": "15", "tick": "200"}, "branch": "actual-branch"},
}


ANONYMOUS_RECIPIENT = {"state": {"observation": {
    "self_target": "a_" + "33" * 32, "visible_actors": [{"id": "a_" + "11" * 32}],
}}}


PERSISTENT_SAMPLE = {
    "type": "snapshot", "request_id": "persistent-owner-request",
    "snapshot": {"actor": "3", "state": {"wizard_game": False, "observation": {
        "self_target": "a_" + "77" * 32, "visible_actors": [{"id": "a_" + "88" * 32}],
        "combat": {"own_stats": {"conditions": [
            {"condition": {"id": "rogue.hasted", "name": "Hasted"},
             "phase": "active", "remaining_ticks": None},
            {"condition": {"id": "rogue.invisible", "name": "Invisible"},
             "phase": "active", "remaining_ticks": None},
        ]}},
    }}, "cursor": {"sequence": "exact-sequence", "tick": "exact-tick"}},
}


class PersistentConditionRecordingContracts(unittest.TestCase):
    def case(self, failure=None, *, sample=None, anonymous=None):
        case, trace = recording_case(failure)
        original = type(case).runTest

        def record(subject):
            subject.persistent_condition_sample = copy.deepcopy(
                PERSISTENT_SAMPLE if sample is None else sample)
            subject.anonymous_ability_sample = copy.deepcopy(
                ANONYMOUS_SAMPLE if anonymous is None else anonymous)
            subject.anonymous_recipient_context = copy.deepcopy(ANONYMOUS_RECIPIENT)
            original(subject)

        type(case).runTest = record
        return case, trace

    def test_adapter_retains_the_actual_null_duration_owner_snapshot(self):
        self.assertTrue(callable(getattr(record_wire_samples.Recorder, "record_persistent_conditions", None)),
                        "The real recorder must retain a persistent owner snapshot")
        case, trace = self.case()
        samples = {"existing": {"type": "palette"}}
        with patch("record_wire_samples.utility_wands_recording_case", return_value=case) as factory:
            record_wire_samples.Recorder().record_persistent_conditions(samples)
        factory.assert_called_once_with()
        self.assertEqual(trace[-1], "class_cleanup")
        self.assertEqual(samples, {"existing": {"type": "palette"},
                                   "zz.condition_persistent": PERSISTENT_SAMPLE,
                                   "zz.anonymous_utility": ANONYMOUS_SAMPLE})
        self.assertIsNot(samples["zz.anonymous_utility"], case.anonymous_ability_sample)
        self.assertIsNot(samples["zz.condition_persistent"], case.persistent_condition_sample)

    def test_factory_reuses_only_the_existing_non_graphical_utility_acceptance_case(self):
        case = record_wire_samples.utility_wands_recording_case()
        self.assertEqual(case.id(), "test_rogue_utility_wands_process.RogueUtilityWandsProcesses."
                         "test_persistent_first_contact_utility_state_displays_in_text_and_reopens_without_source")
        self.assertFalse(is_graphical_test(case))

    def test_failure_skip_and_incomplete_capture_cannot_add_a_persistent_sample(self):
        for disposition in ("class_setup", "setup", "body", "teardown", "cleanup", "class_teardown",
                            "class_cleanup", "skip", "expected_failure", "unexpected_success", "missing_artifact"):
            with self.subTest(disposition=disposition):
                failure = "body" if disposition == "expected_failure" else disposition
                case, _ = self.case(failure)
                if disposition == "skip":
                    type(case).runTest = unittest.skip("no actual utility recording")(type(case).runTest)
                elif disposition in ("expected_failure", "unexpected_success"):
                    type(case).runTest = unittest.expectedFailure(type(case).runTest)
                elif disposition == "missing_artifact":
                    type(case).runTest = lambda subject: None
                samples = {"existing": {"type": "palette"}}
                with patch("record_wire_samples.utility_wands_recording_case", return_value=case):
                    with self.assertRaises(AssertionError):
                        record_wire_samples.Recorder().record_persistent_conditions(samples)
                self.assertEqual(samples, {"existing": {"type": "palette"}})

    def test_persistent_recording_requires_explicit_null_active_rows_on_the_actual_owner(self):
        for changed in ("finite", "zero", "string_null", "missing_duration", "protection",
                        "wrong_condition", "missing_row", "duplicate_row", "wrong_actor", "wizard"):
            with self.subTest(changed=changed):
                sample = copy.deepcopy(PERSISTENT_SAMPLE)
                snapshot = sample["snapshot"]
                rows = snapshot["state"]["observation"]["combat"]["own_stats"]["conditions"]
                if changed in ("finite", "zero", "string_null"):
                    rows[0]["remaining_ticks"] = {"finite": "100", "zero": "0", "string_null": "null"}[changed]
                elif changed == "missing_duration":
                    del rows[0]["remaining_ticks"]
                elif changed == "protection":
                    rows[0]["phase"] = "protection"
                elif changed == "wrong_condition":
                    rows[0]["condition"]["id"] = "unrelated.condition"
                elif changed == "missing_row":
                    rows.pop()
                elif changed == "duplicate_row":
                    rows.append(copy.deepcopy(rows[0]))
                elif changed == "wrong_actor":
                    snapshot["actor"] = "1"
                else:
                    snapshot["state"]["wizard_game"] = True
                case, _ = self.case(sample=sample)
                samples = {}
                with patch("record_wire_samples.utility_wands_recording_case", return_value=case):
                    with self.assertRaises(AssertionError):
                        record_wire_samples.Recorder().record_persistent_conditions(samples)
                self.assertEqual(samples, {})

    def test_anonymous_recording_requires_an_actual_applied_recipient_delta(self):
        for changed in ("missing", "missing_context", "named", "identity_field", "wrong_target", "hidden_caster",
                        "miss", "wrong_actor", "full_observation", "no_events"):
            with self.subTest(changed=changed):
                anonymous = copy.deepcopy(ANONYMOUS_SAMPLE)
                update = anonymous["update"]
                event = update["body"]["state"]["combat"]["events"][0]
                if changed == "named":
                    event["type"] = "ability"
                elif changed == "identity_field":
                    event["ability"] = {"id": "rogue.wand_invisibility", "name": "Wand of Invisibility"}
                elif changed == "wrong_target":
                    event["target"] = "a_" + "44" * 32
                elif changed == "hidden_caster":
                    event["caster"] = "a_" + "55" * 32
                elif changed == "miss":
                    event["outcome"] = "miss"
                elif changed == "wrong_actor":
                    update["actor"] = "1"
                elif changed == "full_observation":
                    update["body"]["type"] = "observation"
                elif changed == "no_events":
                    update["body"]["state"]["combat"]["events"] = []
                case, _ = self.case(anonymous=anonymous)
                if changed in ("missing", "missing_context"):
                    original = type(case).runTest
                    artifact = ("anonymous_ability_sample" if changed == "missing"
                                else "anonymous_recipient_context")

                    def missing(subject, original=original, artifact=artifact):
                        original(subject)
                        delattr(subject, artifact)

                    type(case).runTest = missing
                samples = {}
                with patch("record_wire_samples.utility_wands_recording_case", return_value=case):
                    with self.assertRaises(AssertionError):
                        record_wire_samples.Recorder().record_persistent_conditions(samples)
                self.assertEqual(samples, {}, "Neither artifact publishes after an invalid event")

    def test_anonymous_process_capture_retains_only_the_new_raw_delta(self):
        case = record_wire_samples.utility_wands_recording_case()
        target = SimpleNamespace(transcript=[json.dumps({"message": ANONYMOUS_SAMPLE})])
        start = len(target.transcript)
        with self.assertRaises(AssertionError):
            case.capture_anonymous_feedback(target, start, ANONYMOUS_RECIPIENT)
        sample = copy.deepcopy(ANONYMOUS_SAMPLE)
        target.transcript.append(json.dumps({"message": sample}))
        recipient = copy.deepcopy(ANONYMOUS_RECIPIENT)
        case.capture_anonymous_feedback(target, start, recipient)
        self.assertEqual(case.anonymous_ability_sample, sample)
        self.assertIsNot(case.anonymous_ability_sample, sample)
        recipient["state"]["observation"]["visible_actors"].clear()
        self.assertEqual(case.anonymous_recipient_context, ANONYMOUS_RECIPIENT)

    def test_anonymous_recording_uses_frozen_event_context_not_reopened_refs(self):
        case, _ = self.case()
        samples = {}
        with patch("record_wire_samples.utility_wands_recording_case", return_value=case):
            record_wire_samples.Recorder().record_persistent_conditions(samples)
        old = ANONYMOUS_RECIPIENT["state"]["observation"]
        reopened = samples["zz.condition_persistent"]["snapshot"]["state"]["observation"]
        self.assertNotEqual(old["self_target"], reopened["self_target"])
        self.assertNotEqual(old["visible_actors"], reopened["visible_actors"])
        self.assertEqual(samples["zz.anonymous_utility"], ANONYMOUS_SAMPLE)
        self.assertEqual(set(samples), {"zz.condition_persistent", "zz.anonymous_utility"},
                         "The pre-effect validation context is not a synthetic wire sample")

    def test_service_feedback_assertion_requires_anonymous_activity_not_only_redaction(self):
        from utility_wands_package import assert_anonymous_recipient_feedback
        expected = assert_anonymous_recipient_feedback(self, [ANONYMOUS_SAMPLE], ANONYMOUS_RECIPIENT)
        self.assertIs(expected, ANONYMOUS_SAMPLE)
        for messages in ([], [{"type": "snapshot", "snapshot": PERSISTENT_SAMPLE["snapshot"]}]):
            with self.subTest(messages=messages), self.assertRaises(AssertionError):
                assert_anonymous_recipient_feedback(self, messages, ANONYMOUS_RECIPIENT)

    def capture(self, responses, *, frame=None):
        case = record_wire_samples.utility_wands_recording_case()
        target = SimpleNamespace(transcript=[json.dumps({"message": PERSISTENT_SAMPLE})])
        frame = frame or {"state": copy.deepcopy(PERSISTENT_SAMPLE["snapshot"]["state"])}

        def snapshot(client):
            self.assertIs(client, target)
            target.transcript.extend(json.dumps({"message": message}) for message in responses)
            return frame

        with patch.object(case, "snapshot", side_effect=snapshot):
            case.capture_persistent_snapshot(target)
        return case, target

    def test_capture_keeps_the_exact_new_raw_response_and_does_not_synthesize_wire(self):
        sample = copy.deepcopy(PERSISTENT_SAMPLE)
        case, _ = self.capture([{"type": "palette"}, sample])
        self.assertEqual(case.persistent_condition_sample, sample)
        self.assertIsNot(case.persistent_condition_sample, sample)

    def test_capture_rejects_old_duplicate_or_mismatched_responses(self):
        changed = copy.deepcopy(PERSISTENT_SAMPLE)
        changed["snapshot"]["state"]["observation"]["combat"]["own_stats"]["conditions"][0]["remaining_ticks"] = "100"
        wrong_actor = copy.deepcopy(PERSISTENT_SAMPLE)
        wrong_actor["snapshot"]["actor"] = "1"
        for responses in ([], [PERSISTENT_SAMPLE, PERSISTENT_SAMPLE], [changed], [wrong_actor]):
            with self.subTest(responses=responses), self.assertRaises(AssertionError):
                self.capture(responses)


class ConditionEventSelection(unittest.TestCase):
    def test_full_and_delta_updates_select_only_the_standalone_condition(self):
        event = {"type": "condition", "condition": {"id": "fixture_stupor", "name": "Fixture Faint"}}
        for kind, state in (
                ("observation", {"observation": {"combat": {"events": [event]}}}),
                ("observation_delta", {"combat": {"events": [event]}})):
            with self.subTest(kind=kind):
                message = {"type": "update", "update": {"body": {"type": kind, "state": state}}}
                unchanged = copy.deepcopy(message)
                self.assertTrue(has_condition_event(message, "fixture_stupor"))
                self.assertFalse(has_condition_event(message, "another_status"))
                self.assertEqual(message, unchanged, "Selection must preserve the original server message")

    def test_ability_details_and_unrelated_messages_are_not_standalone_events(self):
        nested = {"type": "ability", "condition": {"id": "fixture_stupor"}}
        for message in (
                None,
                {"type": "snapshot"},
                {"type": "update", "update": {"body": {"type": "intention"}}},
                {"type": "update", "update": {"body": {"type": "observation_delta", "state": {}}}},
                {"type": "update", "update": {"body": {"type": "observation",
                    "state": {"observation": {"combat": None}}}}},
                {"type": "update", "update": {"body": {"type": "observation_delta",
                    "state": {"combat": {"events": [nested]}}}}}):
            with self.subTest(message=message):
                self.assertFalse(has_condition_event(message, "fixture_stupor"))


class NutritionTriggerFixtureContracts(unittest.TestCase):
    def test_shared_fixture_preserves_remove_default_and_authors_optional_protection(self):
        bundled = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text(encoding="utf-8"))

        def resolved(path, compiler):
            authored = tomllib.loads(path.read_text(encoding="utf-8"))
            return copy.deepcopy(bundled) if "import" in authored else authored

        for ticks in (None, 400):
            with self.subTest(protection_ticks=ticks), tempfile.TemporaryDirectory() as directory:
                self.directory = Path(directory)
                self.bin = self.directory / "unused-binaries"
                self.suffix = ""
                # This test checks authored operands only. The process suite
                # validates them with the real compiler and runs the recorder.
                with patch("nutrition_package.load_rules", side_effect=resolved), \
                        patch("nutrition_package.subprocess.run", return_value=subprocess.CompletedProcess(
                            [], 0, stderr="")) as validate:
                    ordinary = nutrition_package.prepare(self)
                    package = nutrition_package.prepare_trigger(self, protection_ticks=ticks)
                self.assertNotEqual(package, ordinary)
                self.assertEqual(validate.call_count, 3)
                self.assertEqual(validate.call_args.args[0], [self.bin / "tor-scenario", "validate", package])
                rules = tomllib.loads((package / "rules.toml").read_text(encoding="utf-8"))
                self.assertEqual(rules["nutrition_condition_trigger"], {
                    "kind": "on_empty_depletion", "condition": "fixture_stupor", "chance": {"one_in": 5},
                    "duration": {"kind": "rolled", "dice": {"count": 1, "sides": 8, "bonus": 3}},
                    "ticks_per_unit": 100})
                condition = next(row for row in rules["conditions"] if row["id"] == "fixture_stupor")
                self.assertEqual(condition["lifecycle"], "timed")
                self.assertEqual(condition["timing"], {"kind": "actor_active", "on_expiry": (
                    {"kind": "remove"} if ticks is None else {"kind": "protect", "duration": ticks})})
                self.assertEqual(condition["restrictions"], ["voluntary_actions"])
                region = tomllib.loads((package / "regions/1.toml").read_text(encoding="utf-8"))
                peer = region["actors"][0]
                self.assertEqual(peer["turn_ticks"], 100)
                self.assertEqual(peer["creature"]["nutrition"], {"kind": "exempt"})
                manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
                self.assertEqual(manifest["id"], package.name)


class SequenceRiderFixtureContracts(unittest.TestCase):
    def test_recorder_authors_gate_without_changing_ordinary_checkless_prefix(self):
        from strike_sequence_package import sequence_package
        bundled = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text(encoding="utf-8"))
        for conditional in (False, True):
            with self.subTest(conditional=conditional), tempfile.TemporaryDirectory() as directory:
                self.directory = Path(directory)
                self.bin = self.directory / "unused-binaries"
                self.suffix = ""
                # Pure authoring check only. Actual validation and capture remain
                # owned by the real recorder's normal compiler/server path.
                with patch("strike_sequence_package.load_rules", return_value=copy.deepcopy(bundled)), \
                        patch("strike_sequence_package.subprocess.run", return_value=subprocess.CompletedProcess(
                            [], 0, stderr="")) as validate:
                    package = sequence_package(self, autonomous=False, natural_source=True,
                                               conditional_rider=conditional)
                validate.assert_called_once()
                rules = tomllib.loads((package / "rules.toml").read_text(encoding="utf-8"))
                ability = next(row for row in rules["abilities"] if row["id"] == "triple_strike")
                riders = ability["strikes"]["additional_riders"]
                self.assertEqual(len(riders), 2 if conditional else 1)
                self.assertNotIn("condition", riders[0])
                if conditional:
                    gated = copy.deepcopy(riders[1])
                    self.assertEqual(gated.pop("condition"), {
                        "kind": "sequence_successful_hits", "minimum": 2})
                    self.assertEqual(gated, riders[0])
                manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
                strikes = manifest["creatures"]["species"]["sequence_fighter"]["melee"]["strikes"]
                self.assertEqual([row["offset"] for row in strikes], [0, 100, 200])
                self.assertTrue(all("check" not in row for row in strikes))
                self.assertTrue(all(len(row["riders"]) == 1 for row in strikes))


class DiscoveryRecordingContracts(unittest.TestCase):
    def samples(self):
        trap = {"id": "t_" + "01" * 32, "family": None, "reachable": True}
        door = {"id": "d_" + "02" * 32, "open": False}
        presence = {"type": "snapshot", "request_id": "exact-presence", "snapshot": {
            "state": {"observation": {"visible_cells": [{"trap": trap}, {"door": door}]}}}}
        family = copy.deepcopy(presence)
        family["snapshot"]["state"]["observation"]["visible_cells"][0]["trap"]["family"] = "rust"
        opened = copy.deepcopy(family)
        cells = opened["snapshot"]["state"]["observation"]["visible_cells"]
        cells[1]["door"]["open"] = True
        cells.append({"trap": {**trap, "id": "t_" + "03" * 32}})
        return {"presence": presence, "family": family, "opened": opened,
                "restored": copy.deepcopy(opened)}

    def case(self, samples, failure=None):
        case, trace = recording_case(failure)
        original = type(case).runTest

        def record(subject):
            subject.discovery_samples = copy.deepcopy(samples)
            original(subject)

        type(case).runTest = record
        return case, trace

    def test_discovery_factory_reuses_non_graphical_source_free_checkpoint_acceptance(self):
        case = record_wire_samples.discovery_recording_case()
        self.assertEqual(case.id(), "test_discovery_process.DiscoveryProcesses."
                         "test_text_examination_and_secret_open_survive_source_free_checkpoint")
        self.assertFalse(is_graphical_test(case))

    def test_discovery_adapter_retains_exact_artifacts_only_after_cleanup(self):
        source = self.samples()
        case, trace = self.case(source)
        samples = {}
        with patch("record_wire_samples.discovery_recording_case", return_value=case):
            record_wire_samples.Recorder().record_discovery(samples)
        self.assertEqual(samples, {"zz.discovery_" + name: value for name, value in source.items()})
        self.assertEqual(trace[-1], "class_cleanup")
        self.assertIsNot(samples["zz.discovery_presence"], case.discovery_samples["presence"])

    def test_incomplete_or_failed_discovery_capture_cannot_publish_partial_samples(self):
        for failure in ("cleanup", "class_cleanup", "family", "missing", "restore"):
            with self.subTest(failure=failure):
                source = self.samples()
                if failure == "family":
                    source["presence"]["snapshot"]["state"]["observation"]["visible_cells"][0]["trap"]["family"] = "rust"
                elif failure == "missing":
                    del source["opened"]
                elif failure == "restore":
                    source["restored"]["snapshot"]["state"]["observation"]["visible_cells"].pop()
                case, _ = self.case(source, failure)
                samples = {"existing": {"type": "palette"}}
                with patch("record_wire_samples.discovery_recording_case", return_value=case):
                    with self.assertRaises(AssertionError):
                        record_wire_samples.Recorder().record_discovery(samples)
                self.assertEqual(samples, {"existing": {"type": "palette"}})


class SelfDeviceRecordingContracts(unittest.TestCase):
    def case(self, failure=None):
        source = {"type": "snapshot", "request_id": "exact-self-device", "snapshot": {
            "state": {"wizard_game": False, "observation": {"interactions": {
                "inventory": [{"item": "opaque-item", "device_activation": "self_target"}]}}}}}
        case, trace = recording_case(failure)
        original = type(case).runTest
        def record(subject):
            subject.self_device_sample = copy.deepcopy(source)
            original(subject)
        type(case).runTest = record
        return case, trace, source

    def test_factory_selects_existing_non_graphical_actual_text_case(self):
        case = record_wire_samples.self_device_recording_case()
        self.assertEqual(case.id(), "test_magic_items_process.MagicItemProcesses."
                         "test_text_nothing_unknown_direction_and_identified_self_skip")
        self.assertFalse(is_graphical_test(case))

    def test_self_device_snapshot_is_preserved_only_after_cleanup(self):
        case, trace, source = self.case()
        samples = {}
        with patch("record_wire_samples.self_device_recording_case", return_value=case):
            record_wire_samples.Recorder().record_self_device(samples)
        self.assertEqual(samples, {"zz.self_device": source})
        self.assertEqual(trace[-1], "class_cleanup")
        self.assertIsNot(samples["zz.self_device"], case.self_device_sample)

    def test_self_device_failed_cleanup_does_not_publish_partial_sample(self):
        for failure in ("cleanup", "class_cleanup"):
            with self.subTest(failure=failure):
                case, _, _ = self.case(failure)
                samples = {"existing": {"type": "palette"}}
                with patch("record_wire_samples.self_device_recording_case", return_value=case):
                    with self.assertRaises(AssertionError):
                        record_wire_samples.Recorder().record_self_device(samples)
                self.assertEqual(samples, {"existing": {"type": "palette"}})


class DrainRefusalRecordingContracts(unittest.TestCase):
    def case(self, failure=None):
        source = {"type": "snapshot", "request_id": "exact-drain-refusal", "snapshot": {
            "state": {"wizard_game": False, "observation": {"interactions": {
                "completed": [], "refused": [{"action": {"type": "zap", "item": "opaque-item", "aim": {"kind": "direction", "direction": "east"}}, "reason": "too_weak"}]}}}}}
        case, trace = recording_case(failure)
        original = type(case).runTest
        def record(subject):
            subject.drain_refusal_sample = copy.deepcopy(source)
            original(subject)
        type(case).runTest = record
        return case, trace, source

    def test_factory_selects_existing_non_graphical_actual_text_case(self):
        case = record_wire_samples.drain_refusal_recording_case()
        self.assertEqual(case.id(), "test_magic_items_process.MagicItemProcesses."
                         "test_imported_drain_weak_text_attempt_spends_time_not_charge_or_identity")
        self.assertFalse(is_graphical_test(case))

    def test_drain_refusal_snapshot_is_preserved_only_after_cleanup(self):
        case, trace, source = self.case()
        samples = {}
        with patch("record_wire_samples.drain_refusal_recording_case", return_value=case):
            record_wire_samples.Recorder().record_drain_refusal(samples)
        self.assertEqual(samples, {"zz.drain_refusal": source})
        self.assertEqual(trace[-1], "class_cleanup")
        self.assertIsNot(samples["zz.drain_refusal"], case.drain_refusal_sample)

    def test_drain_refusal_failed_cleanup_does_not_publish_partial_sample(self):
        for failure in ("cleanup", "class_cleanup"):
            with self.subTest(failure=failure):
                case, _, _ = self.case(failure)
                samples = {"existing": {"type": "palette"}}
                with patch("record_wire_samples.drain_refusal_recording_case", return_value=case):
                    with self.assertRaises(AssertionError):
                        record_wire_samples.Recorder().record_drain_refusal(samples)
                self.assertEqual(samples, {"existing": {"type": "palette"}})


class FirstContactRecordingContracts(unittest.TestCase):
    def case(self, failure=None, malformed=None):
        source = {"reach": {"type": "snapshot", "snapshot": {"state": {"observation": {
            "combat": {"own_stats": {"abilities": [{"id": "rogue.wand_cancellation",
            "reach": {"kind": "until_obstruction"}, "target_kind": "directional_ray",
            "basic_attack": False, "weapon_selection": False}]}}}}}},
            "effect": {"type": "combat_diagnostics", "report": {"records": [{"outcome": {
                "effects": [{"type": "cancel_specials", "target": "2", "changed": True}]}}]}}}
        if malformed == "reach":
            source["reach"]["snapshot"]["state"]["observation"]["combat"]["own_stats"]["abilities"] = []
        elif malformed == "effect":
            source["effect"]["report"]["records"] = []
        elif malformed == "discriminator":
            effect = source["effect"]["report"]["records"][0]["outcome"]["effects"][0]
            effect["kind"] = effect.pop("type")
        case, trace = recording_case(failure)
        original = type(case).runTest
        def record(subject):
            subject.first_contact_samples = copy.deepcopy(source)
            original(subject)
        type(case).runTest = record
        return case, trace, source

    def test_factory_selects_non_graphical_actual_paid_capture_case(self):
        case = record_wire_samples.first_contact_recording_case()
        self.assertEqual(case.id(), "test_magic_items_process.MagicItemProcesses."
                         "test_imported_cancellation_privileged_capture_records_actual_reach_and_effect")
        self.assertFalse(is_graphical_test(case))

    def test_both_actual_artifacts_are_preserved_only_after_cleanup(self):
        case, trace, source = self.case()
        samples = {}
        with patch("record_wire_samples.first_contact_recording_case", return_value=case):
            record_wire_samples.Recorder().record_first_contact(samples)
        self.assertEqual(samples, {"zz.first_contact_" + key: value for key, value in source.items()})
        self.assertEqual(trace[-1], "class_cleanup")
        self.assertIsNot(samples["zz.first_contact_reach"], case.first_contact_samples["reach"])

    def test_failed_cleanup_or_missing_variants_never_publish_partial_samples(self):
        for failure, malformed in (("cleanup", None), ("class_cleanup", None), (None, "reach"), (None, "effect"), (None, "discriminator")):
            with self.subTest(failure=failure, malformed=malformed):
                case, _, _ = self.case(failure, malformed)
                samples = {"existing": {"type": "palette"}}
                with patch("record_wire_samples.first_contact_recording_case", return_value=case):
                    with self.assertRaises(AssertionError):
                        record_wire_samples.Recorder().record_first_contact(samples)
                self.assertEqual(samples, {"existing": {"type": "palette"}})


class TorchRecordingContracts(unittest.TestCase):
    def case(self, failure=None, malformed=False):
        source = {name: {"type": "snapshot", "snapshot": {"state": {"wizard_game": False,
            "observation": {"interactions": {"inventory": [{"portable_light": {
                "remaining": remaining, "lit": lit, "ignitable": not lit, "extinguishable": lit}}]}}}}}
            for name, lit, remaining in (("lit", True, "100000"), ("off", False, "99800"))}
        if malformed:
            source["off"]["snapshot"]["state"]["observation"]["interactions"]["inventory"][0]["portable_light"]["lit"] = True
        case, trace = recording_case(failure)
        original = type(case).runTest
        def record(subject):
            subject.torch_samples = copy.deepcopy(source)
            original(subject)
        type(case).runTest = record
        return case, trace, source

    def test_factory_selects_non_graphical_actual_adventure_source_free_case(self):
        case = record_wire_samples.torch_recording_case()
        self.assertEqual(case.id(), "test_magic_items_process.MagicItemProcesses."
                         "test_imported_torch_text_explicit_control_fuel_drop_and_source_free_reopen")
        self.assertFalse(is_graphical_test(case))
        actions = [row["request"]["command"]["action"] for row in record_wire_samples.CLIENT
                   if row["type"] == "request" and row["request"]["type"] == "command"
                   and row["request"]["command"]["type"] == "act"]
        self.assertEqual([row["type"] for row in actions if row["type"] in ("ignite", "extinguish")],
                         ["ignite", "extinguish"])

    def test_both_server_states_publish_after_successful_cleanup_only(self):
        case, trace, source = self.case()
        samples = {}
        with patch("record_wire_samples.torch_recording_case", return_value=case):
            record_wire_samples.Recorder().record_torch(samples)
        self.assertEqual(samples, {"zz.torch_" + key: value for key, value in source.items()})
        self.assertEqual(trace[-1], "class_cleanup")
        self.assertIsNot(samples["zz.torch_lit"], case.torch_samples["lit"])

    def test_failed_cleanup_or_invalid_off_state_never_partially_publishes(self):
        for failure, malformed in (("cleanup", False), ("class_cleanup", False), (None, True)):
            with self.subTest(failure=failure, malformed=malformed):
                case, _, _ = self.case(failure, malformed)
                samples = {"existing": {"type": "palette"}}
                with patch("record_wire_samples.torch_recording_case", return_value=case):
                    with self.assertRaises(AssertionError):
                        record_wire_samples.Recorder().record_torch(samples)
                self.assertEqual(samples, {"existing": {"type": "palette"}})




class HasteRecordingContracts(unittest.TestCase):
    def case(self, failure=None, malformed=False):
        case, trace = recording_case(failure)
        source = {branch: {"type": "combat_diagnostics", "report": {"enabled": True, "records": [
            {"ability": {"id": "fixture.haste_dose", "name": "Haste dose witness"}, "outcome": {"effects": [{"type": "haste_dose", "target": "1",
                "condition": {"id": "rogue.hasted", "name": "Hasted"},
                "rest": {"id": "rogue.haste_exhaustion", "name": "Exhausted"},
                "outcome": {"type": branch, "duration" if branch == "hasted" else "rest": ticks}}]}}]}}
            for branch, ticks in (("hasted", "400"), ("overdose", "0"))}
        if malformed:
            source["overdose"]["report"]["records"][0]["outcome"]["effects"][0]["outcome"]["rest"] = "800"
        original = type(case).runTest
        def record(subject):
            subject.haste_samples = copy.deepcopy(source)
            original(subject)
        type(case).runTest = record
        return case, trace, source

    def test_factory_selects_actual_non_graphical_privileged_paid_doses(self):
        case = record_wire_samples.haste_recording_case()
        self.assertEqual(case.id(), "test_magic_items_process.MagicItemProcesses."
                         "test_haste_self_ability_privileged_capture_records_both_real_branches")
        self.assertFalse(is_graphical_test(case))

    def test_both_real_branches_publish_only_after_successful_cleanup(self):
        case, trace, source = self.case()
        samples = {}
        with patch("record_wire_samples.haste_recording_case", return_value=case):
            record_wire_samples.Recorder().record_haste(samples)
        self.assertEqual(samples, {"zz.haste_" + key: value for key, value in source.items()})
        self.assertEqual(trace[-1], "class_cleanup")
        self.assertIsNot(samples["zz.haste_hasted"], case.haste_samples["hasted"])

    def test_cleanup_or_invalid_branch_retains_prior_samples_atomically(self):
        for failure, malformed in (("cleanup", False), ("class_cleanup", False), (None, True)):
            with self.subTest(failure=failure, malformed=malformed):
                case, _, _ = self.case(failure, malformed)
                samples = {"existing": {"type": "palette"}}
                with patch("record_wire_samples.haste_recording_case", return_value=case):
                    with self.assertRaises(AssertionError):
                        record_wire_samples.Recorder().record_haste(samples)
                self.assertEqual(samples, {"existing": {"type": "palette"}})


if __name__ == "__main__":
    unittest.main()
