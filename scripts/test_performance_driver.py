"""Preserve the legacy acknowledgement boundary while exposing driver delay."""
import io
import queue
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch, Mock
from performance_driver import DiagnosticLog, JsonProcess, wall_time_ns, stop_all


class DriverTiming(unittest.TestCase):
    def test_admitted_action_waits_for_matching_execution_without_changing_ack_timing(self):
        actions = [
            {"type": "act", "action": {"type": "wait"}},
            {"type": "request", "request": {"type": "command", "branch": "branch",
                "context": {"stream": {"stream": "s", "epoch": "1"}, "readiness_revision": "1"},
                "command": {"type": "act", "expected_revision": "0", "action": {"type": "wait"}}}},
        ]
        for value in actions:
            with self.subTest(input_type=value["type"]):
                client = JsonProcess.__new__(JsonProcess)
                client.lines = queue.Queue()
                client.child = SimpleNamespace(stdin=io.StringIO())
                receipt = dict(type="admitted", intention="original", actor="1",
                               branch="branch", phase="queued")
                client.lines.put(({"message": {"type": "ack", "receipt": receipt}}, 101.))
                client.lines.put(({"type": "ready", "history": []}, 102.))
                def update(identity, branch="branch", phase="resolved"):
                    return {"message": {"type": "update", "update": {"body": {
                        "type": "intention", "status": dict(intention=identity,
                        branch=branch, actor="1", phase=phase)}}}, "history": [identity],
                        "readiness": {"revision": "9"}}
                client.lines.put((update("other"), 103.))
                client.lines.put((update("original", branch="old"), 104.))
                client.lines.put((update("original", phase="queued"), 105.))
                client.lines.put((update("original"), 106.))
                client.lines.put(({"message": {"type": "update"}, "history": ["original"],
                                  "readiness": {"revision": "10"}, "input_context": "fresh"}, 107.))
                with patch("performance_driver.time.perf_counter", return_value=100.):
                    frame, start, ack, executed = client.send(value)
                self.assertEqual(frame["history"], ["original"])
                self.assertEqual((start, ack, executed), (100., 100., 107.))
                self.assertEqual(frame["input_context"], "fresh")
                self.assertEqual(client.ack_line_received, 101.)

    def test_completion_timing_distinguishes_execution_outcome_and_fresh_context(self):
        for phase in ("resolved", "failed", "cancelled", "suspended"):
            with self.subTest(phase=phase):
                client = JsonProcess.__new__(JsonProcess)
                client.lines = queue.Queue()
                client.child = SimpleNamespace(stdin=io.StringIO())
                receipt = dict(type="admitted", intention="original", actor="1",
                               branch="branch", phase="queued")
                client.lines.put(({"message": {"type": "ack", "receipt": receipt}}, 101.))
                client.lines.put(({"type": "ready"}, 102.))
                def update(outcome, actor="1", branch="branch", identity="original"):
                    return {"message": {"type": "update", "update": {"body": {
                        "type": "intention", "status": dict(intention=identity,
                        actor=actor, branch=branch, phase=outcome)}}},
                        "readiness": {"revision": "9"}}
                client.lines.put((update("started"), 103.))
                client.lines.put((update(phase, actor="2"), 104.))
                client.lines.put((update(phase, branch="old"), 105.))
                client.lines.put((update(phase, identity="other"), 106.))
                client.lines.put((update(phase), 107.))
                client.lines.put(({"readiness": {"revision": "9"}}, 108.))
                client.lines.put(({"readiness": {"revision": "10"},
                                  "input_context": "fresh"}, 109.))
                with patch("performance_driver.time.perf_counter", return_value=100.):
                    frame, start, ack, received = client.send(
                        {"type": "act", "action": {"type": "wait"}},
                        wait_for_completion=True)
                self.assertEqual((start, ack, received), (100., 100., 109.))
                self.assertEqual(frame["input_context"], "fresh")
                self.assertEqual(client.action_timing, {
                    "version": 1, "intention": "original", "actor": "1", "branch": "branch",
                    "outcome": phase, "request_to_admission_ms": 1000.,
                    "request_to_execution_ms": 3000., "request_to_outcome_ms": 7000.,
                    "request_to_context_ms": 9000.})

    def test_default_timing_stops_at_started_and_keeps_outcome_pending(self):
        client = JsonProcess.__new__(JsonProcess)
        client.lines = queue.Queue()
        client.child = SimpleNamespace(stdin=io.StringIO())
        receipt = dict(type="admitted", intention="original", actor="1",
                       branch="branch", phase="queued")
        client.lines.put(({"message": {"type": "ack", "receipt": receipt}}, 101.))
        client.lines.put(({"type": "ready"}, 102.))
        started = {"message": {"type": "update", "update": {"body": {
            "type": "intention", "status": {**receipt, "phase": "started"}}}}}
        client.lines.put((started, 103.))
        terminal = {"message": {"type": "update", "update": {"body": {
            "type": "intention", "status": {**receipt, "phase": "resolved"}}}}}
        client.lines.put((terminal, 104.))
        with patch("performance_driver.time.perf_counter", return_value=100.):
            frame, _, _, received = client.send({"type": "act"})
        self.assertEqual(frame, started)
        self.assertEqual(received, 103.)
        self.assertEqual(client.lines.qsize(), 1)
        self.assertIsNone(client.action_timing["outcome"])
        self.assertIsNone(client.action_timing["request_to_outcome_ms"])
        self.assertIsNone(client.action_timing["request_to_context_ms"])

    def test_failed_log_cleanup_preserves_failure_and_stops_every_owned_child(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            (path/'result.json').write_text('{}')
            good = SimpleNamespace(name='good', stop=Mock())
            bad = SimpleNamespace(name='bad', stop=Mock(side_effect=RuntimeError('log cap')))
            with self.assertRaisesRegex(RuntimeError, 'log cap'):
                stop_all([good,bad],path,dict(samples=[1]))
            good.stop.assert_called_once()
            self.assertFalse((path/'result.json').exists())
            self.assertTrue((path/'rejected-result.json').exists())
            self.assertIn('log cap',(path/'failure.json').read_text())

    def test_deferred_log_preserves_bytes_without_writing_during_measurement(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/"log"
            log = DiagnosticLog(path, True, limit=5)
            log.write("abc\n")
            log.flush()
            self.assertEqual(path.read_bytes(), b"")
            with self.assertRaisesRegex(RuntimeError, "cap exceeded"):
                log.write("xy")
            log.close()
            self.assertEqual(path.read_text(), "abc\n")

    def test_host_timestamp_uses_the_recorded_monotonic_boundary(self):
        with patch("performance_driver._host_clock_offset", return_value=42):
            self.assertEqual(wall_time_ns(1.25), 1250000042)

    def test_ack_line_timestamp_excludes_later_driver_logging_and_queue_delay(self):
        client = JsonProcess.__new__(JsonProcess)
        client.lines = queue.Queue()
        client.child = SimpleNamespace(stdin=io.StringIO())
        client.action_timing = {"outcome": "resolved"}
        client.lines.put(({"message":{"type":"ack"}}, 101.))
        client.lines.put(({"type":"ready"}, 102.))
        with patch("performance_driver.time.perf_counter", side_effect=[100.,103.,105.,106.]):
            _, start, legacy_ack, ready = client.send({"type":"act"})
        self.assertEqual((start, legacy_ack, ready), (100.,105.,102.))
        self.assertEqual(client.ack_line_received, 101.)
        self.assertIsNone(client.action_timing)
