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
            self.recorded_client_samples = []
            step("body")

        def tearDown(self):
            step("teardown")

        @classmethod
        def tearDownClass(cls):
            step("class_teardown")

    case = Case()
    return case, trace


class CompleteRecordingContracts(unittest.TestCase):
    def test_polymorph_factories_reuse_two_actual_non_graphical_boundaries(self):
        for factory, method in ((record_wire_samples.polymorph_recording_case,
                "test_text_paid_self_polymorph_reopens_and_retries_without_source_or_private_ledger"),
                (record_wire_samples.polymorph_diagnostic_recording_case,
                 "test_polymorph_self_ability_privileged_capture_has_changed_only_effect")):
            case = factory()
            self.assertEqual(case.id(), "test_magic_items_process.MagicItemProcesses." + method)
            self.assertFalse(is_graphical_test(case))

    def test_polymorph_recording_preserves_real_envelopes_and_rejects_private_or_incomplete_variants(self):
        ordinary = {"affordance": {"type": "snapshot", "request_id": "actual-snapshot",
            "snapshot": {"state": {"wizard_game": False, "observation": {"interactions": {"inventory": [
                {"item": "exact-item", "device_activation": "direction_or_self"}]}}}}},
            "request": {"type": "request", "request_id": "actual-self-zap", "request": {"type": "command",
                "context": {"epoch": "exact-epoch"}, "command": {"type": "act", "expected_revision": "7",
                    "action": {"type": "zap", "item": "exact-item", "aim": {"kind": "self_target"}}}}}}
        diagnostic = {"type": "combat_diagnostics", "report": {"enabled": True, "records": [
            {"ability": {"id": "fixture.polymorph"}, "outcome": {"effects": [
                {"type": "baleful_polymorph", "target": "1", "changed": True}]}}]}}

        def case_for(attribute, artifact, failure=None):
            case, trace = recording_case(failure)
            original = type(case).runTest
            def body(subject):
                setattr(subject, attribute, copy.deepcopy(artifact))
                original(subject)
            type(case).runTest = body
            return case, trace

        def record(first, second, failure=None):
            one, trace_one = case_for("polymorph_samples", first)
            two, trace_two = case_for("polymorph_diagnostic_sample", second, failure)
            recorder = record_wire_samples.Recorder()
            recorder.recorded_client_samples = [{"existing": "unchanged"}]
            samples = {"existing": {"type": "palette"}}
            with patch("record_wire_samples.polymorph_recording_case", return_value=one), \
                    patch("record_wire_samples.polymorph_diagnostic_recording_case", return_value=two):
                try:
                    recorder.record_polymorph(samples)
                except AssertionError:
                    self.assertEqual(samples, {"existing": {"type": "palette"}})
                    self.assertEqual(recorder.recorded_client_samples, [{"existing": "unchanged"}])
                    raise
            return recorder, samples, trace_one, trace_two

        recorder, samples, one, two = record(ordinary, diagnostic)
        self.assertEqual(samples["zz.polymorph_affordance"], ordinary["affordance"])
        self.assertEqual(samples["zz.polymorph_effect"], diagnostic)
        self.assertEqual(recorder.recorded_client_samples[-1], ordinary["request"])
        self.assertEqual((one[-1], two[-1]), ("class_cleanup", "class_cleanup"))
        self.assertIsNot(recorder.recorded_client_samples[-1], ordinary["request"])
        for failure in ("body", "cleanup", "class_cleanup"):
            with self.subTest(failure=failure), self.assertRaises(AssertionError):
                record(ordinary, diagnostic, failure)
        for mutation in ("old_mode", "wrong_item", "form_leak", "unchanged"):
            first, second = copy.deepcopy(ordinary), copy.deepcopy(diagnostic)
            effect = second["report"]["records"][0]["outcome"]["effects"][0]
            if mutation == "old_mode":
                first["affordance"]["snapshot"]["state"]["observation"]["interactions"]["inventory"][0]["device_activation"] = "self_target"
            elif mutation == "wrong_item":
                first["request"]["request"]["command"]["action"]["item"] = "not-the-affordance"
            elif mutation == "form_leak":
                effect["form"] = "private.body.changed"
            else:
                effect["changed"] = False
            with self.subTest(mutation=mutation), self.assertRaises((AssertionError, ValueError)):
                record(first, second)

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

            def until(predicate, seconds, *, include_transcript):
                self.assertFalse(include_transcript)
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


def detection_recording_inputs():
    """Synthetic validator inputs only, never published as process artifacts."""
    from test_detection_process_package import public_pages

    def exchange(kind, count, suffix):
        query, pages = public_pages(count, kind)
        query["context"]["stream"] += suffix
        query["chart"] += suffix
        requests, replies, cursor = [], [], None
        for index, page in enumerate(pages):
            page["context"] = copy.deepcopy(query["context"])
            page["chart"] = query["chart"]
            current = {**copy.deepcopy(query), "cursor": cursor}
            request_id = kind + suffix + str(index)
            requests.append({"type": "request", "request_id": request_id,
                             "request": {"type": "chart", "query": current}})
            replies.append({"type": "chart_page", "request_id": request_id,
                            "context": copy.deepcopy(query["context"]), "page": page})
            cursor = page["next"]
        return {"queries": requests, "pages": replies}

    original, restored = exchange("magic", 1, "-original"), exchange("magic", 1, "-restored")
    slots, roots = {}, []
    for index, kind in enumerate(("food", "magic", "mapping")):
        slots[kind] = {"latest": index, "attempt": {"attempted_at": 200,
            "status": {"kind": "positive" if kind == "mapping" else "empty"}}}
        roots.append({"kind": kind, "points": [{"mapping": {"kind": "wall_face"}
                                                      if kind == "mapping" else None}]})
    book = {"game": {"chart_history": 0}, "shared": {"chart_history": {
        "owners": [[{"actor": 1, "slots": slots}]], "roots": roots}}}
    samples = {"query": original["queries"][0], "page": original["pages"][0],
               "restored_query": restored["queries"][0], "restored_page": restored["pages"][0],
               "mapping": exchange("mapping", 1024, "-original"),
               "restored_mapping": exchange("mapping", 1024, "-restored"),
               "checkpoint_payload": b"unit-only-decoder-input-not-a-save"}
    return samples, book


def detection_case(samples, failure=None):
    case, trace = recording_case(failure)
    original = type(case).runTest

    def run(subject):
        subject.detection_samples = copy.deepcopy(samples)
        original(subject)

    type(case).runTest = run
    return case, trace


class DetectionRecordingContracts(unittest.TestCase):
    def test_factory_selects_real_source_free_paid_history_case(self):
        case = record_wire_samples.detection_recording_case()
        self.assertEqual(case.id(), "test_magic_items_process.MagicItemProcesses."
                         "test_imported_detection_history_empty_retention_source_free_spectator")
        self.assertFalse(is_graphical_test(case))

    def test_raw_queries_pages_and_writer_payload_publish_only_after_cleanup(self):
        samples, book = detection_recording_inputs()
        case, trace = detection_case(samples)
        recorder = record_wire_samples.Recorder()
        published = {"existing": {"type": "palette"}}
        with patch("record_wire_samples.detection_recording_case", return_value=case), \
                patch("checkpoint_payload.decode_checkpoint_payload", return_value=json.dumps(book)) as decoder:
            recorder.record_detection(published)
        decoder.assert_called_once_with(samples["checkpoint_payload"])
        self.assertEqual(trace[-1], "class_cleanup")
        self.assertEqual(published["chart_page"], samples["page"])
        self.assertEqual(published["zz.detection_restored"], samples["restored_page"])
        self.assertEqual(published["zz.mapping.000"], samples["mapping"]["pages"][0])
        self.assertEqual(recorder.recorded_client_samples,
                         [samples["query"], samples["restored_query"],
                          *samples["mapping"]["queries"], *samples["restored_mapping"]["queries"]])
        self.assertEqual(recorder.detection_checkpoint_payload, samples["checkpoint_payload"])
        self.assertIsNot(published["chart_page"], case.detection_samples["page"])

    def test_every_lifecycle_failure_blocks_all_detection_publication(self):
        for failure in ("class_setup", "setup", "body", "teardown", "cleanup", "class_teardown", "class_cleanup"):
            with self.subTest(failure=failure):
                samples, _ = detection_recording_inputs()
                case, _ = detection_case(samples, failure)
                published = {"old": {"type": "palette"}}
                recorder = record_wire_samples.Recorder()
                recorder.recorded_client_samples = [{"old": "query"}]
                with patch("record_wire_samples.detection_recording_case", return_value=case):
                    with self.assertRaises(AssertionError):
                        recorder.record_detection(published)
                self.assertEqual(published, {"old": {"type": "palette"}})
                self.assertEqual(recorder.recorded_client_samples, [{"old": "query"}])
                self.assertFalse(hasattr(recorder, "detection_checkpoint_payload"))

    def test_corrupt_pages_correlation_or_saved_book_never_partially_publish(self):
        mutations = (
            lambda s, b: s["page"].update(request_id="wrong"),
            lambda s, b: s["restored_page"]["page"].update(acquired_at="101"),
            lambda s, b: s["mapping"]["queries"][0]["request"]["query"].update(cursor="0"),
            lambda s, b: s["mapping"]["pages"][0]["page"].update(next="381"),
            lambda s, b: s["mapping"]["pages"].clear(),
            lambda s, b: s["mapping"]["pages"][0]["page"]["records"][1]["mark"].update(fact={"kind": "wall_face", "normal": [0, 0, 1]}),
            lambda s, b: s["restored_page"]["page"]["records"][1].update(item="private"),
            lambda s, b: b["shared"]["chart_history"]["owners"][0][0]["slots"]["food"].update(latest=None),
            lambda s, b: b["shared"]["chart_history"]["owners"][0][0]["slots"]["magic"]["attempt"].update(status={"kind": "positive"}),
            lambda s, b: b["shared"]["chart_history"]["roots"][2]["points"][0].update(mapping=None),
            lambda s, b: s.update(checkpoint_payload="not production bytes"),
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                samples, book = detection_recording_inputs()
                mutation(samples, book)
                case, _ = detection_case(samples)
                recorder = record_wire_samples.Recorder()
                published = {"old": {"type": "palette"}}
                with patch("record_wire_samples.detection_recording_case", return_value=case), \
                        patch("checkpoint_payload.decode_checkpoint_payload", return_value=json.dumps(book)):
                    with self.assertRaises(AssertionError):
                        recorder.record_detection(published)
                self.assertEqual(published, {"old": {"type": "palette"}})
                self.assertFalse(hasattr(recorder, "recorded_client_samples"))
                self.assertFalse(hasattr(recorder, "detection_checkpoint_payload"))

    def test_atomic_wire_publication_includes_actual_client_queries_unchanged(self):
        samples, _ = detection_recording_inputs()
        case, trace = recording_case()
        original = type(case).runTest

        def run(subject):
            original(subject)
            subject.recorded_client_samples = [copy.deepcopy(samples["query"])]

        type(case).runTest = run
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "wire-unit-only.json"
            with patch("record_wire_samples.Recorder", return_value=case), patch("builtins.print"):
                record_wire_samples.record_samples(output=output)
            document = json.loads(output.read_text())
        self.assertEqual(trace[-1], "class_cleanup")
        self.assertEqual(document["client"], [*record_wire_samples.CLIENT, samples["query"]])


    def test_checkpoint_export_preserves_decoded_raw_and_hash_provenance_after_cleanup(self):
        import hashlib
        raw, decoded = b"unit-only-raw-envelope", b'{"unit":"decoded writer bytes"}'
        case, trace = recording_case()
        original = type(case).runTest

        def run(subject):
            original(subject)
            subject.detection_checkpoint_payload = raw

        type(case).runTest = run
        with tempfile.TemporaryDirectory() as directory:
            target, output = Path(directory) / "current-save.json", Path(directory) / "wire.json"
            with patch("record_wire_samples.Recorder", return_value=case), patch("builtins.print"), \
                    patch("checkpoint_payload.decode_checkpoint_payload", return_value=decoded) as decoder:
                record_wire_samples.record_samples(output=output, detection_checkpoint=target)
            decoder.assert_called_once_with(raw)
            self.assertEqual(trace[-1], "class_cleanup")
            self.assertEqual(target.read_bytes(), decoded)
            self.assertEqual(Path(str(target) + ".torc").read_bytes(), raw)
            provenance = json.loads(Path(str(target) + ".provenance.json").read_bytes())
            self.assertEqual(provenance["raw_sha256"], hashlib.sha256(raw).hexdigest())
            self.assertEqual(provenance["decoded_sha256"], hashlib.sha256(decoded).hexdigest())
            self.assertEqual((provenance["raw_bytes"], provenance["decoded_bytes"]), (len(raw), len(decoded)))

    def test_checkpoint_export_lifecycle_or_decode_failure_preserves_every_prior_file(self):
        for failure in ("cleanup", "class_cleanup", "decode"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory:
                case, _ = recording_case(None if failure == "decode" else failure)
                case.detection_checkpoint_payload = b"unit-only-raw-envelope"
                target, output = Path(directory) / "current-save.json", Path(directory) / "wire.json"
                outputs = [target, Path(str(target) + ".torc"), Path(str(target) + ".provenance.json"), output]
                for path in outputs:
                    path.write_bytes(b"prior validated artifact")
                with patch("record_wire_samples.Recorder", return_value=case), \
                        patch("checkpoint_payload.decode_checkpoint_payload", side_effect=ValueError("invalid actual envelope")):
                    with self.assertRaises((AssertionError, ValueError)):
                        record_wire_samples.record_samples(output=output, detection_checkpoint=target)
                for path in outputs:
                    self.assertEqual(path.read_bytes(), b"prior validated artifact")


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


class GrowthRecordingContracts(unittest.TestCase):
    def case(self, failure=None, malformed=False):
        case, trace = recording_case(failure)
        source = {kind: {"type": "combat_diagnostics", "report": {"enabled": True, "records": [
            {"ability": {"id": "fixture." + kind, "name": "Growth diagnostic"}, "outcome": {"effects": [
                {"type": kind, "target": "1", **payload}]}}]}}
            for kind, payload in (("gain_strength", {"change": "acquired"}), ("raise_level", {"amount": "100"}))}
        if malformed:
            source["raise_level"]["report"]["records"][0]["outcome"]["effects"][0]["amount"] = 100
        original = type(case).runTest
        def record(subject):
            subject.growth_samples = copy.deepcopy(source)
            original(subject)
        type(case).runTest = record
        return case, trace, source

    def test_factory_selects_actual_non_graphical_granted_ability_case(self):
        case = record_wire_samples.growth_recording_case()
        self.assertEqual(case.id(), "test_magic_items_process.MagicItemProcesses."
                         "test_growth_self_abilities_privileged_capture_records_actual_acquisition_and_credit")
        self.assertFalse(is_graphical_test(case))

    def test_both_actual_results_publish_after_successful_cleanup(self):
        case, trace, source = self.case()
        samples = {}
        with patch("record_wire_samples.growth_recording_case", return_value=case):
            record_wire_samples.Recorder().record_growth(samples)
        self.assertEqual(samples, {"zz.growth_" + key: value for key, value in source.items()})
        self.assertEqual(trace[-1], "class_cleanup")
        self.assertIsNot(samples["zz.growth_gain_strength"], case.growth_samples["gain_strength"])

    def test_cleanup_or_invalid_result_keeps_existing_samples_atomically(self):
        for failure, malformed in (("cleanup", False), ("class_cleanup", False), (None, True)):
            with self.subTest(failure=failure, malformed=malformed):
                case, _, _ = self.case(failure, malformed)
                samples = {"existing": {"type": "palette"}}
                with patch("record_wire_samples.growth_recording_case", return_value=case):
                    with self.assertRaises(AssertionError):
                        record_wire_samples.Recorder().record_growth(samples)
                self.assertEqual(samples, {"existing": {"type": "palette"}})


class HallucinationRecordingContracts(unittest.TestCase):
    def case(self, failure=None, mutation=None):
        case, trace = recording_case(failure)
        # Pure validator/lifecycle inputs only: never presented as wire corpus
        # recordings. Genuine corpus values come from the real process factory.
        condition = {"condition": {"id": "rogue.hallucinating", "name": "Hallucinating"},
                     "remaining_ticks": "80800"}
        view = {"self_target": "a_self", "tick": "100",
            "combat": {"own_stats": {"conditions": [condition]}},
            "visible_actors": [{"id": "a_peer", "name": "emu", "asset": "creature.bird.emu", "description": "",
                                "appearance_seed": "p_" + "00" * 32}],
            "ground_items": [{"presentation": {"name": "gold wand", "asset": "item.wand",
                "kind": {"kind": "item", "class": "wand"}}, "item": {
                    "class": "coin", "quantity": "7", "known_currency": "gold", "name": "object", "appearance": "object", "description": ""}}],
            "visible_cells": [{"stairs_up": False, "stairs_down": False}]}
        raw = {"type": "snapshot", "request_id": "exact-hallucination-request",
               "snapshot": {"state": {"observation": view}, "context": {"stream": "unchanged-stream", "epoch": "1"}}}
        source = {stage: copy.deepcopy(raw) for stage in ("visual", "extended", "cured")}
        extended = source["extended"]["snapshot"]["state"]["observation"]
        extended["tick"] = "200"
        extended["combat"]["own_stats"]["conditions"][0]["remaining_ticks"] = "161500"
        extended["visible_actors"][0].update(name="bat", asset="creature.bat")
        extended["ground_items"][0]["presentation"] = {"name": "cedar staff", "asset": "item.wand",
            "kind": {"kind": "item", "class": "wand"}}
        cured = source["cured"]["snapshot"]["state"]["observation"]
        cured["tick"] = "300"
        cured["visible_actors"][0].update(name="actual visible peer", asset=None)
        cured["combat"]["own_stats"]["conditions"] = []
        cured["ground_items"][0]["presentation"] = None
        cured["ground_items"][0]["item"]["name"] = "gold coin"
        if mutation is not None:
            mutation(source)
        original = type(case).runTest
        def record(subject):
            subject.hallucination_samples = copy.deepcopy(source)
            original(subject)
        type(case).runTest = record
        return case, trace, source

    def test_factory_selects_actual_non_graphical_text_cadence_source_free_case(self):
        case = record_wire_samples.hallucination_recording_case()
        self.assertEqual(case.id(), "test_magic_items_process.MagicItemProcesses."
                         "test_imported_hallucination_text_visual_cadence_and_source_free_reopen")
        self.assertFalse(is_graphical_test(case))

    def test_three_raw_snapshots_publish_only_after_all_lifecycle_hooks(self):
        case, trace, source = self.case()
        samples = {"existing": {"type": "palette"}}
        with patch("record_wire_samples.hallucination_recording_case", return_value=case):
            record_wire_samples.Recorder().record_hallucination(samples)
        self.assertEqual(samples, {"existing": {"type": "palette"},
            **{"zz.hallucination_" + stage: sample for stage, sample in source.items()}})
        self.assertEqual(trace, ["class_setup", "setup", "body", "teardown", "cleanup",
                                 "class_teardown", "class_cleanup"])
        for stage in source:
            self.assertIsNot(samples["zz.hallucination_" + stage], case.hallucination_samples[stage])
            self.assertEqual(samples["zz.hallucination_" + stage]["request_id"], "exact-hallucination-request")

    @staticmethod
    def multicell_occurrences(source):
        for sample in source.values():
            rows = sample["snapshot"]["state"]["observation"]["visible_actors"]
            rows[0]["position"] = {"x": 2, "y": 0, "z": 0}
            upper = copy.deepcopy(rows[0])
            upper["position"]["z"] = 1
            rows.append(upper)

    def test_two_lawful_body_occurrences_preserve_every_raw_row(self):
        case, trace, source = self.case(mutation=self.multicell_occurrences)
        samples = {}
        with patch("record_wire_samples.hallucination_recording_case", return_value=case):
            record_wire_samples.Recorder().record_hallucination(samples)
        self.assertEqual(trace[-1], "class_cleanup")
        self.assertEqual(samples, {"zz.hallucination_" + stage: sample for stage, sample in source.items()})
        for sample in samples.values():
            rows = sample["snapshot"]["state"]["observation"]["visible_actors"]
            self.assertEqual(len(rows), 2)
            self.assertEqual([row["position"]["z"] for row in rows], [0, 1])

    def test_second_peer_or_contradictory_same_target_occurrence_rejects_atomically(self):
        for stage in ("visual", "extended", "cured"):
            for field, value in (("id", "a_second_peer"), ("name", "contradictory name"),
                                 ("asset", "creature.wrong"), ("description", "true hidden description")):
                with self.subTest(stage=stage, field=field):
                    def mutate(source):
                        self.multicell_occurrences(source)
                        source[stage]["snapshot"]["state"]["observation"]["visible_actors"][1][field] = value
                    case, _, _ = self.case(mutation=mutate)
                    samples = {"existing": {"type": "palette"}}
                    with patch("record_wire_samples.hallucination_recording_case", return_value=case):
                        with self.assertRaises(AssertionError):
                            record_wire_samples.Recorder().record_hallucination(samples)
                    self.assertEqual(samples, {"existing": {"type": "palette"}})
        for stage in ("visual", "extended"):
            with self.subTest(stage=stage, field="appearance_seed"):
                def mutate(source):
                    self.multicell_occurrences(source)
                    source[stage]["snapshot"]["state"]["observation"]["visible_actors"][1]["appearance_seed"] = "p_" + "01" * 32
                case, _, _ = self.case(mutation=mutate)
                samples = {"existing": {"type": "palette"}}
                with patch("record_wire_samples.hallucination_recording_case", return_value=case):
                    with self.assertRaises(AssertionError):
                        record_wire_samples.Recorder().record_hallucination(samples)
                self.assertEqual(samples, {"existing": {"type": "palette"}})

    def test_shared_process_sample_helper_preserves_and_checks_every_body_occurrence(self):
        from hallucination_package import assert_sample
        _, _, source = self.case(mutation=self.multicell_occurrences)
        for stage, phase in (("visual", 0), ("extended", 1)):
            view = source[stage]["snapshot"]["state"]["observation"]
            before = copy.deepcopy(view)
            assert_sample(self, view, phase)
            self.assertEqual(view, before, "shared helper must never deduplicate or normalize raw rows")
            for field, value in (("id", "a_second_peer"), ("name", "contradictory name"),
                                 ("asset", "creature.wrong"), ("description", "true hidden description"),
                                 ("appearance_seed", "p_" + "01" * 32)):
                with self.subTest(stage=stage, field=field):
                    bad = copy.deepcopy(view)
                    bad["visible_actors"][1][field] = value
                    with self.assertRaises(AssertionError):
                        assert_sample(self, bad, phase)

    def test_both_inclusive_duration_endpoints_accept_only_exact_base_turn_extension(self):
        for extension in (80800, 89200):
            with self.subTest(extension=extension):
                def mutate(source):
                    visual = source["visual"]["snapshot"]["state"]["observation"]
                    extended = source["extended"]["snapshot"]["state"]["observation"]
                    first_remaining = int(visual["combat"]["own_stats"]["conditions"][0]["remaining_ticks"])
                    remaining = first_remaining + extension + int(visual["tick"]) - int(extended["tick"])
                    extended["combat"]["own_stats"]["conditions"][0]["remaining_ticks"] = str(remaining)
                case, _, source = self.case(mutation=mutate)
                samples = {}
                with patch("record_wire_samples.hallucination_recording_case", return_value=case):
                    record_wire_samples.Recorder().record_hallucination(samples)
                self.assertEqual(samples, {"zz.hallucination_" + stage: sample for stage, sample in source.items()})

    def test_any_lifecycle_failure_prevents_all_three_samples(self):
        for failure in ("class_setup", "setup", "body", "teardown", "cleanup", "class_teardown", "class_cleanup"):
            with self.subTest(failure=failure):
                case, _, _ = self.case(failure)
                samples = {"existing": {"type": "palette"}}
                with patch("record_wire_samples.hallucination_recording_case", return_value=case):
                    with self.assertRaises(AssertionError):
                        record_wire_samples.Recorder().record_hallucination(samples)
                self.assertEqual(samples, {"existing": {"type": "palette"}})

    def test_invalid_visual_extension_cure_authority_or_private_clock_never_partially_publishes(self):
        def view(samples, stage):
            return samples[stage]["snapshot"]["state"]["observation"]
        mutations = {
            "wrong_visual_name": lambda s: view(s, "visual")["visible_actors"][0].update(name="actual peer"),
            "true_sibling_leak": lambda s: view(s, "visual")["ground_items"][0]["item"].update(asset="item.coin"),
            "fake_mechanical_class": lambda s: view(s, "visual")["ground_items"][0]["item"].update(**{"class": "weapon"}),
            "wrong_sample": lambda s: view(s, "visual")["ground_items"][0]["presentation"].update(name="torch"),
            "wrong_extended_peer": lambda s: view(s, "extended")["visible_actors"][0].update(name="emu"),
            "wrong_extended_object": lambda s: view(s, "extended")["ground_items"][0]["presentation"].update(name="arrow"),
            "extended_true_sibling_leak": lambda s: view(s, "extended")["ground_items"][0]["item"].update(asset="item.coin"),
            "too_small_extension": lambda s: view(s, "extended")["combat"]["own_stats"]["conditions"][0].update(remaining_ticks="161400"),
            "too_large_extension": lambda s: view(s, "extended")["combat"]["own_stats"]["conditions"][0].update(remaining_ticks="170000"),
            "fractional_turn_extension": lambda s: view(s, "extended")["combat"]["own_stats"]["conditions"][0].update(remaining_ticks="161501"),
            "extension_tick_ignored": lambda s: view(s, "extended").update(tick="201"),
            "stale_cured_peer": lambda s: view(s, "cured")["visible_actors"][0].update(name="bat", asset="creature.bat"),
            "zero_extension": lambda s: view(s, "extended")["combat"]["own_stats"]["conditions"][0].update(remaining_ticks="0"),
            "missing_extension": lambda s: view(s, "extended")["combat"]["own_stats"].update(conditions=[]),
            "uncured_marker": lambda s: view(s, "cured")["combat"]["own_stats"].update(conditions=copy.deepcopy(view(s, "visual")["combat"]["own_stats"]["conditions"])),
            "stale_cured_art": lambda s: view(s, "cured")["ground_items"][0].update(presentation=copy.deepcopy(view(s, "visual")["ground_items"][0]["presentation"])),
            "private_clock": lambda s: view(s, "visual").update(remaining_to_shift="100"),
            "not_raw_snapshot": lambda s: s["visual"].update(type="state"),
        }
        for label, mutation in mutations.items():
            with self.subTest(label=label):
                case, _, _ = self.case(mutation=mutation)
                samples = {"existing": {"type": "palette"}}
                with patch("record_wire_samples.hallucination_recording_case", return_value=case):
                    with self.assertRaises(AssertionError):
                        record_wire_samples.Recorder().record_hallucination(samples)
                self.assertEqual(samples, {"existing": {"type": "palette"}})


if __name__ == "__main__":
    unittest.main()
