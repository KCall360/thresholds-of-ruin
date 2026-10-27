"""Existing-API regressions for opt-in lossless process stdout retention.

The child is fake, but Process owns its real reader thread, FIFO and transcript.
These are capture/caller tests, not actual network-pressure qualification.
"""
from contextlib import contextmanager, nullcontext
import gzip
import hashlib
import io
import json
import os
import queue
from pathlib import Path
import tempfile
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, call, patch

from process_harness import AdventureProcess, Process, ProcessTestCase, TOKEN, SPECTATOR_TOKEN
import test_run_until_blocked_process as pressure


@contextmanager
def capture_directory():
    """Use the canonical spelling of our owned temporary diagnostics directory.

    The storage API deliberately rejects aliases. OS temporary-directory paths
    need not already use canonical spelling (for example Windows short names).
    """
    with tempfile.TemporaryDirectory() as directory:
        yield Path(directory).resolve(strict=True)


class SegmentedFixtureDirectories(unittest.TestCase):
    def test_owned_alias_is_canonicalized_before_storage_setup(self):
        from segmented_process_capture import SegmentedOutputQueue, validate_retained_capture
        with tempfile.TemporaryDirectory() as owner:
            root = Path(owner).resolve(strict=True)
            baseline = root / "baseline"
            baseline.mkdir()
            capture = SegmentedOutputQueue(baseline / "stdout.log", 128)
            capture.put("valid baseline")
            capture.put(None)
            capture.close()
            self.assertEqual(validate_retained_capture(capture.path)["total_lines"], 1)

            child = root / "existing-child"
            child.mkdir()
            alias = child / ".."
            self.assertTrue(alias.is_dir())
            self.assertNotEqual(alias, root)
            self.assertEqual(alias.resolve(strict=True), root)
            # Exercise the real fixture adapter, not a mocked capture guard.
            # This models a legal owned directory returned with alias spelling.
            with patch.object(tempfile, "TemporaryDirectory", return_value=nullcontext(str(alias))):
                with capture_directory() as diagnostics:
                    self.assertEqual(diagnostics, root)
                    self.assertEqual(diagnostics, diagnostics.resolve(strict=True))
                    capture = SegmentedOutputQueue(diagnostics / "stdout.log", 128)
                    capture.put("owned alias α")
                    capture.put(None)
                    capture.close()
                    self.assertEqual(validate_retained_capture(capture.path)["total_lines"], 1)
            self.assertEqual((root / "stdout-segments.json").read_bytes(),
                             (alias / "stdout-segments.json").read_bytes())

    def test_storage_still_rejects_noncanonical_root_and_wrong_name_without_writes(self):
        from segmented_process_capture import CaptureError, SegmentedOutputQueue
        with tempfile.TemporaryDirectory() as owner:
            root = Path(owner).resolve(strict=True)
            child = root / "existing-child"
            child.mkdir()
            baseline = root / "baseline"
            baseline.mkdir()
            capture = SegmentedOutputQueue(baseline / "stdout.log", 128)
            capture.put("baseline")
            capture.put(None)
            capture.close()
            before = {p.relative_to(root): p.read_bytes() for p in root.rglob("*") if p.is_file()}
            for path in (child / ".." / "stdout.log", root / "wrong.log"):
                with self.subTest(path=path):
                    with self.assertRaisesRegex(CaptureError, "existing real diagnostics directory"):
                        SegmentedOutputQueue(path, 128)
                    self.assertEqual(before, {
                        p.relative_to(root): p.read_bytes() for p in root.rglob("*") if p.is_file()
                    })
                    self.assertEqual(sorted(p.name for p in root.iterdir()), ["baseline", "existing-child"])


class SegmentedProcessCapture(unittest.TestCase):
    def test_opt_in_keeps_fifo_transcript_and_exact_decoded_bytes_after_stop(self):
        values = ["α" * 70 + ":one", "😀" * 70 + ":two", "", "final fragment"]
        # Process already normalizes each received line, including an EOF
        # fragment, to one LF in its capture. Preserve that exact old contract.
        source = "\n".join(values)
        expected = (source + "\n").encode("utf-8")
        child = SimpleNamespace(
            pid=12345, stdin=io.StringIO(), stdout=io.StringIO(source),
            stderr=io.StringIO("complete stderr\n"),
            poll=Mock(return_value=0), wait=Mock(return_value=0), kill=Mock(),
        )
        with capture_directory() as directory:
            diagnostics = Path(directory)
            with patch("process_harness.subprocess.Popen", return_value=child) as launch:
                process = Process("tor-client-headless", ["--connect", "unused:1"],
                    extra_env={"TOR_TEST_SEGMENTED_STDOUT_BYTES": "128", "CAPTURE_FIXTURE": "kept"},
                    diagnostics=diagnostics, separate_stderr=True)
            try:
                # Validate the existing transport fixture before demanding any
                # new storage behavior: FIFO, byte offsets and consumed slices.
                process.reader.join(timeout=5)
                self.assertFalse(process.reader.is_alive())
                self.assertEqual(process.until(lambda line: line == values[1]), "\n".join(values[:2]))
                first_end = len(("\n".join(values[:2]) + "\n").encode("utf-8"))
                self.assertEqual(process.lines.offset, first_end)
                self.assertEqual(process.lines.read_between(0, first_end), "\n".join(values[:2]))
                self.assertEqual(list(process.transcript), values[:2])
                self.assertEqual(process.lines.queue, [*values[2:], None])
                self.assertEqual(process.until(lambda line: line == values[-1]), "\n".join(values[2:]))
                self.assertEqual(process.lines.offset, len(expected))
                self.assertEqual(process.lines.read_between(first_end, len(expected)), "\n".join(values[2:]))
                self.assertEqual(process.transcript[:], values)
                self.assertEqual(process.transcript[::-1], values[::-1])
                self.assertEqual(process.transcript[-1], values[-1])
                with self.assertRaisesRegex(AssertionError, "Unexpected process exit"):
                    process.until(lambda _line: False)
            finally:
                process.stop(save=False)

            self.assertTrue(process._stopped)
            self.assertEqual(child.poll(), 0)
            self.assertFalse(process.reader.is_alive())
            for handle in (child.stdin, child.stdout, child.stderr,
                           process.lines.reader, process.lines.writer):
                self.assertTrue(handle.closed)
            self.assertEqual(list(process.transcript), values)
            # Separate stderr retains the existing native text-file convention;
            # only the stdout FIFO/segments below promise exact UTF-8 LF bytes.
            self.assertEqual((diagnostics / "stderr.log").read_bytes(),
                             ("complete stderr" + os.linesep).encode("utf-8"))

            # On the unchanged implementation all valid-baseline assertions
            # above pass; the first red is missing segmented retention here.
            manifest_path = diagnostics / "stdout-segments.json"
            self.assertTrue(manifest_path.is_file(), "Opted-in stdout must publish its segment manifest")
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            self.assertTrue(manifest["complete"])
            self.assertEqual(manifest["segment_bytes"], 128)
            self.assertEqual(manifest["total_bytes"], len(expected))
            self.assertEqual(manifest["total_lines"], len(values))
            self.assertGreater(len(manifest["segments"]), 1)
            self.assertTrue(any(row.get("archive") for row in manifest["segments"]))
            decoded = bytearray()
            for row in manifest["segments"]:
                self.assertEqual(row["start"], len(decoded))
                self.assertLessEqual(row["bytes"], 128)
                if row.get("archive"):
                    retained = (diagnostics / row["archive"]).read_bytes()
                    self.assertEqual(row["archive_bytes"], len(retained))
                    self.assertEqual(row["archive_sha256"], hashlib.sha256(retained).hexdigest())
                    raw = gzip.decompress(retained)
                else:
                    raw = (diagnostics / row["raw"]).read_bytes()
                self.assertEqual(row["bytes"], len(raw))
                self.assertEqual(row["sha256"], hashlib.sha256(raw).hexdigest())
                decoded.extend(raw)
            self.assertEqual(bytes(decoded), expected)
            self.assertNotIn("TOR_TEST_SEGMENTED_STDOUT_BYTES", launch.call_args.kwargs["env"])
            self.assertEqual(launch.call_args.kwargs["env"]["CAPTURE_FIXTURE"], "kept")

    def test_full_capture_fixture_keeps_exact_bytes_with_lf_and_crlf_stderr_writers(self):
        original_open = Path.open
        for newline in ("\n", "\r\n"):
            with self.subTest(stderr_newline=repr(newline)):
                writes = []

                def native_stderr_open(path, mode="r", *args, **kwargs):
                    if path.name == "stderr.log" and mode == "w":
                        self.assertEqual(args, ())
                        self.assertEqual(kwargs, {"encoding": "utf-8"})
                        # Model each platform's real text-file translation at
                        # the existing Process.stop I/O boundary, nowhere else.
                        kwargs["newline"] = newline
                        writes.append(newline)
                    return original_open(path, mode, *args, **kwargs)

                with patch.object(Path, "open", new=native_stderr_open), \
                        patch.object(os, "linesep", newline):
                    # Reuse every FIFO, Unicode, raw/archive byte and lifecycle
                    # assertion, rather than creating a weaker parallel fixture.
                    self.test_opt_in_keeps_fifo_transcript_and_exact_decoded_bytes_after_stop()
                self.assertEqual(writes, [newline])

    def test_invalid_opt_in_is_rejected_before_launch_or_storage_creation(self):
        invalid = ["", "0", "-1", "01", "1.0", " 128", "128 ", "１２８",
                   str(64 * 1024 * 1024 + 1), 128, True]
        with capture_directory() as directory:
            diagnostics = Path(directory) / "not-created"
            for value in invalid:
                with self.subTest(value=value), patch("process_harness.subprocess.Popen") as launch:
                    with self.assertRaisesRegex(ValueError, "canonical integer"):
                        Process("tor-client-headless", [], diagnostics=diagnostics,
                                extra_env={"TOR_TEST_SEGMENTED_STDOUT_BYTES": value})
                    launch.assert_not_called()
                    self.assertFalse(diagnostics.exists())

    def test_global_environment_does_not_opt_in_or_leak_the_private_option(self):
        child = SimpleNamespace(
            pid=12345, stdin=io.StringIO(), stdout=io.StringIO("ordinary\n"),
            stderr=io.StringIO(), poll=Mock(return_value=0),
            wait=Mock(return_value=0), kill=Mock(),
        )
        with capture_directory() as directory:
            diagnostics = Path(directory)
            with patch.dict(os.environ, {"TOR_TEST_SEGMENTED_STDOUT_BYTES": "128"}), \
                    patch("process_harness.subprocess.Popen", return_value=child) as launch:
                process = Process("tor-client-headless", [], diagnostics=diagnostics,
                                  separate_stderr=True)
            try:
                self.assertEqual(process.until(lambda line: line == "ordinary"), "ordinary")
            finally:
                process.stop(save=False)
            self.assertEqual((diagnostics / "stdout.log").read_bytes(), b"ordinary\n")
            self.assertFalse((diagnostics / "stdout-segments.json").exists())
            self.assertEqual(list(process.transcript), ["ordinary"])
            self.assertNotIn("TOR_TEST_SEGMENTED_STDOUT_BYTES", launch.call_args.kwargs["env"])

    def test_final_transcript_wait_uses_the_original_process_deadline_failure(self):
        child = SimpleNamespace(
            pid=12345, stdin=io.StringIO(), stdout=io.StringIO("ready\n"),
            stderr=io.StringIO(), poll=Mock(return_value=0),
            wait=Mock(return_value=0), kill=Mock(),
        )
        with capture_directory() as directory:
            with patch("process_harness.subprocess.Popen", return_value=child):
                process = Process("tor-client-headless", [], diagnostics=Path(directory),
                    separate_stderr=True, extra_env={"TOR_TEST_SEGMENTED_STDOUT_BYTES": "128"})
            try:
                with patch.object(process.lines, "read_between", side_effect=queue.Empty) as transcript, \
                        patch("process_harness.time.monotonic", return_value=100.0):
                    with self.assertRaisesRegex(AssertionError, "Process output deadline"):
                        process.until(lambda line: line == "ready")
                    transcript.assert_called_once()
                    self.assertEqual(transcript.call_args.kwargs, {"deadline": 115.0})
                self.assertIsNone(process.lines._error)
            finally:
                process.stop(save=False)

    def test_headless_frame_never_replays_an_already_matched_lossless_capture(self):
        from segmented_process_capture import iter_retained_lines, validate_retained_capture
        frames = [
            {"type": "update", "message": {"phase": "queued", "id": "paid-1"}},
            {"type": "update", "message": {"phase": "resolved", "id": "paid-1"}},
            {"type": "ready", "readiness": {"admission": True}},
        ]
        lines = [json.dumps(frame) for frame in frames]
        child = SimpleNamespace(
            pid=12345, stdin=io.StringIO(), stdout=io.StringIO("\n".join(lines) + "\n"),
            stderr=io.StringIO(), poll=Mock(return_value=0),
            wait=Mock(return_value=0), kill=Mock(),
        )
        with capture_directory() as directory:
            with patch("process_harness.subprocess.Popen", return_value=child):
                process = Process("tor-client-headless", [], diagnostics=Path(directory),
                    separate_stderr=True, extra_env={"TOR_TEST_SEGMENTED_STDOUT_BYTES": "128"})
            try:
                process.reader.join(timeout=5)
                self.assertFalse(process.reader.is_alive())
                # Exact response matching must finish without rebuilding an
                # ignored string from archived chunks. Default full-transcript
                # callers still exercise their original deadline test above.
                with patch.object(process.lines, "read_between", side_effect=queue.Empty) as replay:
                    result = ProcessTestCase().frame(process, lambda value:
                        value.get("message") == {"phase": "resolved", "id": "paid-1"})
                    self.assertEqual(result, frames[1])
                    self.assertEqual(len(process.transcript), 2)
                    self.assertEqual(ProcessTestCase().frame(process,
                        lambda value: value["type"] == "ready"), frames[2])
                    replay.assert_not_called()
                self.assertEqual(list(process.transcript), lines)
            finally:
                process.stop(save=False)
            self.assertEqual(validate_retained_capture(process.stdout_path)["total_lines"], 3)
            self.assertEqual(list(iter_retained_lines(process.stdout_path)), lines)

    def test_headless_frame_keeps_wait_and_predicate_deadlines_without_transcript_replay(self):
        for slow in ("waiting", "predicate"):
            with self.subTest(slow=slow), capture_directory() as directory:
                child = SimpleNamespace(
                    pid=12345, stdin=io.StringIO(), stdout=io.StringIO('{"type":"ready"}\n'),
                    stderr=io.StringIO(), poll=Mock(return_value=0),
                    wait=Mock(return_value=0), kill=Mock(),
                )
                with patch("process_harness.subprocess.Popen", return_value=child):
                    process = Process("tor-client-headless", [], diagnostics=Path(directory),
                        separate_stderr=True, extra_env={"TOR_TEST_SEGMENTED_STDOUT_BYTES": "128"})
                try:
                    process.reader.join(timeout=5)
                    self.assertFalse(process.reader.is_alive())
                    now = [100.0]
                    def matched(_value):
                        now[0] = 116.0
                        return True
                    waiting = (patch.object(process.lines, "get", side_effect=queue.Empty)
                               if slow == "waiting" else nullcontext())
                    with waiting as get, patch("process_harness.time.monotonic", side_effect=lambda: now[0]), \
                            patch.object(process.lines, "read_between") as replay:
                        with self.assertRaisesRegex(AssertionError, "Process output deadline"):
                            ProcessTestCase().frame(process, matched, seconds=15)
                        if slow == "waiting":
                            get.assert_called_once_with(timeout=15.0)
                        replay.assert_not_called()
                    self.assertIsNone(process.lines._error)
                finally:
                    process.stop(save=False)

    def test_reader_exception_after_valid_prefix_never_marks_capture_complete(self):
        from segmented_process_capture import CaptureError
        for process_type in (Process, AdventureProcess):
            with self.subTest(process_type=process_type.__name__), capture_directory() as directory:
                release = threading.Event()
                class BrokenStream(io.StringIO):
                    def fail_after_prefix(self):
                        if not release.wait(5):
                            raise AssertionError("Test did not release broken stream")
                        raise OSError("synthetic source failure")
                    def __next__(self):
                        try:
                            return super().__next__()
                        except StopIteration:
                            return self.fail_after_prefix()
                    def read(self, size=-1):
                        value = super().read(size)
                        return value if value else self.fail_after_prefix()
                child = SimpleNamespace(pid=12345, stdin=io.StringIO(), stdout=BrokenStream("retained\n"),
                    stderr=io.StringIO(), poll=Mock(return_value=0), wait=Mock(return_value=0), kill=Mock())
                with patch("threading.excepthook") as thread_failure:
                    with patch("process_harness.subprocess.Popen", return_value=child):
                        process = process_type("tor-client-headless", [], diagnostics=Path(directory),
                            separate_stderr=True, extra_env={"TOR_TEST_SEGMENTED_STDOUT_BYTES": "128"})
                    try:
                        self.assertEqual(process.until(lambda line: line == "retained"), "retained")
                    finally:
                        release.set()
                        process.reader.join(5)
                    self.assertFalse(process.reader.is_alive())
                    thread_failure.assert_called_once()
                    self.assertIsInstance(thread_failure.call_args.args[0].exc_value, OSError)
                    with self.assertRaises(CaptureError):
                        process.stop(save=False)
                manifest = Path(directory) / "stdout-segments.json"
                if manifest.exists():
                    self.assertFalse(json.loads(manifest.read_text())["complete"])
                self.assertEqual((Path(directory) / "stdout-segment-00000000.raw").read_bytes(), b"retained\n")
                self.assertFalse(process.lines._worker.is_alive())
                self.assertTrue(all(stream.closed for stream in (child.stdin, child.stdout, child.stderr)))

    def test_unjoined_reader_status_never_marks_capture_complete(self):
        from segmented_process_capture import CaptureError
        child = SimpleNamespace(pid=12345, stdin=io.StringIO(), stdout=io.StringIO("retained\n"),
            stderr=io.StringIO(), poll=Mock(return_value=0), wait=Mock(return_value=0), kill=Mock())
        with capture_directory() as directory:
            with patch("process_harness.subprocess.Popen", return_value=child):
                process = Process("tor-client-headless", [], diagnostics=Path(directory),
                    separate_stderr=True, extra_env={"TOR_TEST_SEGMENTED_STDOUT_BYTES": "128"})
            self.assertEqual(process.until(lambda line: line == "retained"), "retained")
            process.reader.join(5)
            self.assertFalse(process.reader.is_alive())
            # Only simulate the join result, never leave a real unowned thread.
            with patch.object(process.reader, "is_alive", return_value=True):
                with self.assertRaises(CaptureError):
                    process.stop(save=False)
            manifest = Path(directory) / "stdout-segments.json"
            if manifest.exists():
                self.assertFalse(json.loads(manifest.read_text())["complete"])
            self.assertFalse(process.lines._worker.is_alive())
            self.assertTrue(all(stream.closed for stream in (child.stdin, child.stdout, child.stderr)))

    def test_actual_pressure_caller_opts_in_only_the_normal_player(self):
        driver = pressure.RunUntilBlockedProcesses()
        driver.address = "127.0.0.1:1"
        player_relay = SimpleNamespace(address="127.0.0.1:2", close=Mock())
        spectator_relay = SimpleNamespace(address="127.0.0.1:3", close=Mock())
        initial = {"type": "ready", "state": {"observation": {"tick": "0"}}}
        with patch.object(driver, "server"), patch.object(driver, "addCleanup"), \
                patch.object(pressure, "StreamRelay", side_effect=[player_relay, spectator_relay]) as relay, \
                patch.object(pressure, "tcp_send_buffer_budget", return_value=4 * 1024 * 1024), \
                patch.object(driver, "launch") as launch, \
                patch.object(driver, "frame", side_effect=[initial, StopIteration]):
            # Stop after both real caller launch sites, before opening gates or
            # taking any action. No socket, child or gameplay loop is simulated.
            with self.assertRaises(StopIteration):
                driver.slow_spectator_recovery()
        self.assertEqual(relay.call_args_list, [
            call(driver.address, measure_actor_facts=True),
            call(driver.address, receive_buffer=pressure.RECEIVE_BUFFER),
        ])
        self.assertEqual(launch.call_args_list, [
            call("tor-client-headless", ["--connect", player_relay.address,
                 "--omit-memory", "--compact-background"], token=TOKEN,
                 extra_env={"TOR_TEST_SEGMENTED_STDOUT_BYTES": "8388608"}),
            call("tor-client-headless", ["--connect", spectator_relay.address], token=SPECTATOR_TOKEN),
        ])
        self.assertEqual((pressure.TURNS, pressure.MAX_PRESSURE_TURNS, pressure.OUTBOUND_SLOTS,
                          pressure.RECEIVE_BUFFER, pressure.STALL_SECONDS),
                         (1500, 20000, 256, 65536, 5))


class SegmentedStorageBoundaries(unittest.TestCase):
    def capture(self, directory, size=128):
        from segmented_process_capture import SegmentedOutputQueue
        return SegmentedOutputQueue(Path(directory) / "stdout.log", size)

    def test_slow_fifo_consumer_reopens_archived_unicode_chunks_in_order(self):
        from segmented_process_capture import iter_retained_lines, validate_retained_capture
        values = ["αβγ", "", "😀" * 9 + "tail", "last"]
        expected = ("\n".join(values) + "\n").encode()
        with capture_directory() as directory:
            capture = self.capture(directory, 8)
            try:
                for value in values:
                    capture.put(value)
                capture.put(None)
                # No FIFO consumer has opened any segment yet. Even unread
                # complete chunks can be archived without slowing the producer.
                with capture.condition:
                    self.assertTrue(capture.condition.wait_for(
                        lambda: all(row["_status"] == "archived" for row in capture._rows), 5))
                self.assertFalse(list(Path(directory).glob("*.raw")))
                self.assertEqual(capture.queue, [*values, None])
                offsets = []
                for value in values:
                    self.assertEqual(capture.get(timeout=1), value)
                    offsets.append(capture.offset)
                self.assertEqual(offsets, [len(("\n".join(values[:n]) + "\n").encode())
                                           for n in range(1, len(values) + 1)])
                self.assertIsNone(capture.get(timeout=1))
                self.assertEqual(capture.read_between(0, len(expected)), "\n".join(values))
            finally:
                capture.close()
            manifest = validate_retained_capture(capture.path)
            self.assertEqual((manifest["total_bytes"], manifest["total_lines"]), (len(expected), 4))
            self.assertTrue(all(row["bytes"] <= 8 for row in manifest["segments"]))
            self.assertEqual(list(iter_retained_lines(capture.path)), values)
            self.assertTrue(capture.reader.closed and capture.writer.closed)
            self.assertFalse(capture._worker.is_alive())

    def test_paused_compressor_does_not_hold_the_producer_condition(self):
        from segmented_process_capture import SegmentedOutputQueue
        entered, release, produced = threading.Event(), threading.Event(), threading.Event()
        original = SegmentedOutputQueue._archive
        errors = []
        def paused(owner, row):
            self.assertEqual(row["_refs"], 0)
            self.assertIsNot(owner._current, row)
            entered.set()
            if not release.wait(5):
                raise AssertionError("Test did not release compressor")
            original(owner, row)
        with capture_directory() as directory, \
                patch.object(SegmentedOutputQueue, "_archive", paused):
            capture = self.capture(directory)
            writer = None
            try:
                capture.put("x" * 127)
                self.assertTrue(entered.wait(5))
                def produce():
                    try:
                        capture.put("next record")
                    except BaseException as error:
                        errors.append(error)
                    finally:
                        produced.set()
                writer = threading.Thread(target=produce)
                writer.start()
                self.assertTrue(produced.wait(5), "Compression must not hold the stdout producer lock")
                self.assertEqual(errors, [])
                self.assertFalse(release.is_set())
            finally:
                release.set()
                if writer is not None:
                    writer.join(5)
                capture.put(None)
                capture.close()
            self.assertFalse(writer.is_alive())
            self.assertEqual(list(capture.iter_lines()), ["x" * 127, "next record"])

    def test_get_timeout_during_second_chunk_restores_the_whole_pending_record(self):
        from segmented_process_capture import SegmentedOutputQueue
        entered, release, done = threading.Event(), threading.Event(), threading.Event()
        original = SegmentedOutputQueue._archive
        observed = []
        def paused(owner, row):
            if row["index"] == 1:
                entered.set()
                if not release.wait(5):
                    raise AssertionError("Test did not release second archive")
            original(owner, row)
        with capture_directory() as directory, \
                patch.object(SegmentedOutputQueue, "_archive", paused):
            capture = self.capture(directory, 8)
            consumer = None
            try:
                capture.put("abcdefg😀tail")
                capture.put(None)
                self.assertTrue(entered.wait(5))
                # Positive fixture: first chunk is already complete/archived;
                # only the second chunk's publication is intentionally gated.
                first = capture._rows[0]
                self.assertEqual(first["_status"], "archived")
                self.assertEqual(gzip.decompress((Path(directory) / first["archive"]).read_bytes()), "abcdefg😀tail".encode()[:8])
                def consume():
                    try:
                        observed.append(capture.get(timeout=0.01))
                    except BaseException as error:
                        observed.append(error)
                    finally:
                        done.set()
                consumer = threading.Thread(target=consume)
                consumer.start()
                self.assertTrue(done.wait(2), "get(timeout) must not wait indefinitely for archive publication")
                self.assertEqual(len(observed), 1)
                self.assertIsInstance(observed[0], queue.Empty)
                self.assertEqual((capture.offset, capture.count), (0, 1))
                self.assertIsNone(capture._error)
                release.set()
                self.assertEqual(capture.get(timeout=2), "abcdefg😀tail")
                self.assertEqual((capture.offset, capture.count), (16, 0))
                self.assertIsNone(capture.get(timeout=1))
            finally:
                release.set()
                if consumer is not None:
                    consumer.join(5)
                capture.close()
            self.assertFalse(consumer.is_alive())

    def test_held_diagnostic_iterator_does_not_deadlock_close_or_lose_raw_bytes(self):
        from segmented_process_capture import iter_retained_lines, validate_retained_capture
        with capture_directory() as directory:
            capture = self.capture(directory)
            capture.put("alpha")
            capture.put("beta")
            iterator = capture.iter_lines()
            done = threading.Event()
            errors = []
            def close_capture():
                try:
                    capture.close()
                except BaseException as error:
                    errors.append(error)
                finally:
                    done.set()
            closer = threading.Thread(target=close_capture, daemon=True)
            try:
                self.assertEqual(next(iterator), "alpha")
                capture.put(None)
                closer.start()
                self.assertTrue(done.wait(2), "Close must not wait for a caller-held diagnostic iterator")
                self.assertEqual(errors, [])
                self.assertFalse(capture._worker.is_alive())
                manifest = validate_retained_capture(capture.path)
                self.assertIsNone(manifest["segments"][0]["archive"])
                self.assertEqual(list(iterator), ["beta"])
                self.assertEqual(list(iter_retained_lines(capture.path)), ["alpha", "beta"])
            finally:
                iterator.close()
                if closer.ident is not None:
                    closer.join(5)
                    self.assertFalse(closer.is_alive())
                capture.close()

    def test_compression_failure_preserves_raw_prefix_and_closes_owned_handles(self):
        from segmented_process_capture import CaptureError
        with capture_directory() as directory, \
                patch("segmented_process_capture.gzip.GzipFile", side_effect=OSError("controlled compressor failure")):
            capture = self.capture(directory)
            capture.put("all original bytes")
            capture.put(None)
            with capture.condition:
                self.assertTrue(capture.condition.wait_for(lambda: capture._error is not None, 5))
            with self.assertRaises(CaptureError):
                capture.close()
            self.assertTrue(capture.reader.closed and capture.writer.closed)
            self.assertFalse(capture._worker.is_alive())
            self.assertEqual((Path(directory) / "stdout-segment-00000000.raw").read_bytes(),
                             b"all original bytes\n")
            self.assertFalse(list(Path(directory).glob("*.raw.gz")))
            manifest = Path(directory) / "stdout-segments.json"
            self.assertTrue(not manifest.exists() or not json.loads(manifest.read_text())["complete"])

    def test_offline_reopen_rejects_corrupt_archives_and_malformed_manifests(self):
        from segmented_process_capture import CaptureError, validate_retained_capture
        with capture_directory() as directory:
            capture = self.capture(directory, 8)
            capture.put("unicode αβγ across chunks")
            capture.put(None)
            capture.close()
            manifest = validate_retained_capture(capture.path)
            path = Path(directory) / "stdout-segments.json"
            original = path.read_bytes()
            changes = [lambda v: v.update(complete=False),
                       lambda v: v.update(version=True),
                       lambda v: v.update(total_bytes=v["total_bytes"] + 1),
                       lambda v: v.update(total_lines=v["total_lines"] + 1),
                       lambda v: v["segments"][0].update(start=1),
                       lambda v: v["segments"][0].update(raw="../outside"),
                       lambda v: v["segments"][0].update(archive="../outside.gz")]
            for change in changes:
                bad = json.loads(original)
                change(bad)
                path.write_text(json.dumps(bad))
                with self.assertRaises(CaptureError):
                    validate_retained_capture(capture.path)
            path.write_bytes(b'{"version":1,' + original[1:])
            with self.assertRaisesRegex(CaptureError, "Duplicate"):
                validate_retained_capture(capture.path)
            path.write_bytes(original)
            row = manifest["segments"][0]
            archive = Path(directory) / row["archive"]
            data = archive.read_bytes()
            archive.write_bytes(bytes([data[0] ^ 1]) + data[1:])
            with self.assertRaisesRegex(CaptureError, "Compressed segment identity"):
                validate_retained_capture(capture.path)
            # A coherent compressed checksum must not hide the wrong decoded
            # payload. Keep the original captured-byte digest independently.
            wrong = gzip.compress(b"X" * row["bytes"], compresslevel=1, mtime=0)
            archive.write_bytes(wrong)
            rewritten = json.loads(original)
            rewritten["segments"][0].update(archive_bytes=len(wrong),
                archive_sha256=hashlib.sha256(wrong).hexdigest())
            path.write_text(json.dumps(rewritten))
            with self.assertRaisesRegex(CaptureError, "Decoded segment identity"):
                validate_retained_capture(capture.path)

    def test_raw_corruption_is_rejected_even_when_length_and_mtime_are_restored(self):
        from segmented_process_capture import CaptureError, validate_retained_capture
        with capture_directory() as directory:
            capture = self.capture(directory)
            capture.put("unchanged-sized record")
            iterator = capture.iter_lines()
            self.assertEqual(next(iterator), "unchanged-sized record")
            try:
                capture.put(None)
                capture.close()
            finally:
                iterator.close()
            manifest = validate_retained_capture(capture.path)
            row = manifest["segments"][0]
            self.assertIsNone(row["archive"])
            raw = Path(directory) / row["raw"]
            data = raw.read_bytes()
            raw.write_bytes(b"X" + data[1:])
            os.utime(raw, ns=(row["mtime_ns"], row["mtime_ns"]))
            with self.assertRaisesRegex(CaptureError, "Raw segment identity"):
                validate_retained_capture(capture.path)

    def test_existing_namespace_and_archive_collision_are_never_overwritten(self):
        from segmented_process_capture import CaptureError
        names = ["stdout.log", "stdout-segments.json", "stdout-segments.json.partial",
                 "stdout-segment-00000000.raw"]
        for name in names:
            with self.subTest(name=name), capture_directory() as directory:
                existing = Path(directory) / name
                existing.write_bytes(b"preexisting owner")
                with self.assertRaises(CaptureError):
                    self.capture(directory)
                self.assertEqual(existing.read_bytes(), b"preexisting owner")
                self.assertEqual(list(Path(directory).iterdir()), [existing])
        with capture_directory() as directory:
            capture = self.capture(directory)
            archive = Path(directory) / "stdout-segment-00000000.raw.gz"
            archive.write_bytes(b"different archive owner")
            capture.put("preserved source")
            capture.put(None)
            with self.assertRaises(CaptureError):
                capture.close()
            self.assertEqual(archive.read_bytes(), b"different archive owner")
            self.assertEqual((Path(directory) / "stdout-segment-00000000.raw").read_bytes(),
                             b"preserved source\n")
            self.assertFalse(capture._worker.is_alive())

    def test_changed_root_identity_is_rejected_before_any_chunk_creation(self):
        from segmented_process_capture import CaptureError
        with capture_directory() as directory:
            capture = self.capture(directory)
            with patch("segmented_process_capture._directory_identity", return_value=(-1, -1)):
                with self.assertRaisesRegex(CaptureError, "directory was replaced"):
                    capture.put("must not escape")
            with self.assertRaises(CaptureError):
                capture.close()
            self.assertEqual(list(Path(directory).iterdir()), [])
            self.assertFalse(capture._worker.is_alive())

    def test_producer_digest_rejects_raw_replacement_before_archive_publication(self):
        from segmented_process_capture import CaptureError, SegmentedOutputQueue
        entered, release = threading.Event(), threading.Event()
        original = SegmentedOutputQueue._archive
        def paused(owner, row):
            entered.set()
            if not release.wait(5):
                raise AssertionError("Test did not release archival")
            original(owner, row)
        with capture_directory() as directory, \
                patch.object(SegmentedOutputQueue, "_archive", paused):
            capture = self.capture(directory, 8)
            try:
                capture.put("abcdefg")
                capture.put(None)
                self.assertTrue(entered.wait(5))
                row = capture._rows[0]
                self.assertEqual(row["sha256"], hashlib.sha256(b"abcdefg\n").hexdigest())
                raw = Path(directory) / row["raw"]
                raw.write_bytes(b"xbcdefg\n")
                os.utime(raw, ns=(row["mtime_ns"], row["mtime_ns"]))
            finally:
                release.set()
                with self.assertRaises(CaptureError):
                    capture.close()
            self.assertEqual(raw.read_bytes(), b"xbcdefg\n")
            self.assertFalse(list(Path(directory).glob("*.raw.gz")))
            self.assertFalse(capture._worker.is_alive())

    def test_popen_failure_closes_prepared_capture_and_empty_reopen_is_valid(self):
        from segmented_process_capture import SegmentedOutputQueue, iter_retained_lines
        owned = []
        def prepare(*args, **kwargs):
            capture = SegmentedOutputQueue(*args, **kwargs)
            owned.append(capture)
            return capture
        with capture_directory() as directory, \
                patch("segmented_process_capture.SegmentedOutputQueue", side_effect=prepare), \
                patch("process_harness.subprocess.Popen", side_effect=OSError("controlled launch failure")):
            with self.assertRaisesRegex(OSError, "controlled launch failure"):
                Process("tor-client-headless", [], diagnostics=Path(directory),
                        extra_env={"TOR_TEST_SEGMENTED_STDOUT_BYTES": "128"})
            self.assertEqual(len(owned), 1)
            self.assertTrue(owned[0].writer.closed and owned[0].reader.closed)
            self.assertFalse(owned[0]._worker.is_alive())
            self.assertEqual(list(iter_retained_lines(owned[0].path)), [])


if __name__ == "__main__":
    unittest.main()
