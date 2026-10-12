"""Behavior of the shared actual-process harness without launching applications."""
import json
from pathlib import Path
import sys
import queue
import threading
import os
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from process_harness import OutputQueue, Process, ProcessTestCase, ProcessTestDirectory, Transcript, binaries


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
                batches = [[executed], [executed, newer]]

                def frame(client, predicate):
                    selected = next((item for item in batches.pop(0) if predicate(item)), None)
                    self.assertIsNotNone(selected, "the newer permission frame was not selected")
                    return selected

                with patch.object(harness, "command", return_value=accepted), \
                        patch.object(harness, "frame", side_effect=frame):
                    self.assertIs(harness.act(object(), {"type": "wait"}), newer)
                self.assertEqual(batches, [])


class CreaturePackages(unittest.TestCase):
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
                                           ability_aliases={"fixture_ability": ("magic_bolt", "Fixture Spell")})
            resolve.assert_called_once_with(package / "rules.toml", harness.bin / "tor-scenario")
            validate.assert_called_once()
            self.assertEqual(validate.call_args.args[0], [harness.bin / "tor-scenario", "validate", package])
            manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
            self.assertTrue((package / manifest["ruleset"]).is_file())
            species = manifest["creatures"]["species"]["human"]
            rules = tomllib.loads((package / manifest["ruleset"]).read_text(encoding="utf-8"))
            self.assertTrue(any(value["id"] == "fixture_ability" and value["name"] == "Fixture Spell" for value in rules["abilities"]))
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
