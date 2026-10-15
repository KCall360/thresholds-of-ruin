"""Pure adapter guards; these do not claim production Service coverage."""
import hashlib
import io
import json
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time
import unittest
from unittest.mock import Mock, patch

from service_contract_host import HostError, ServiceHost, current_protocol


IDENTITY = {"host_version": 1, "feature": "service-contract-host", "protocol": current_protocol()}


class ServiceHostAdapterTests(unittest.TestCase):
    def host(self, responses, identity=IDENTITY):
        process = Mock()
        process.stdin = io.StringIO()
        process.stdout = io.StringIO("\n".join(json.dumps(row) for row in [identity, *responses]) + "\n")
        with patch("service_contract_host.subprocess.Popen", return_value=process):
            host = ServiceHost(sys.executable, expected_sha256=hashlib.sha256(Path(sys.executable).read_bytes()).hexdigest(), timeout=.05)
        self.addCleanup(host.close)
        return host, process

    def test_calls_use_one_explicit_operation_without_build_fallback(self):
        host, process = self.host([{"ok": {"case": 7}}])
        self.assertEqual(host.call("create", seed=42), {"case": 7})
        self.assertEqual(json.loads(process.stdin.getvalue()), {"op": "create", "seed": 42})

    def test_host_rejection_is_not_returned_as_a_successful_protocol_reply(self):
        host, _ = self.host([{"error": "No matching active case"}])
        with self.assertRaisesRegex(HostError, "No matching active case"):
            host.call("close", case=99)

    def test_unknown_or_ambiguous_envelopes_fail_closed(self):
        for response in ({"ok": {}, "error": "bad"}, {"unexpected": {}}, {"ok": []}):
            with self.subTest(response=response):
                host, _ = self.host([response])
                with self.assertRaises(HostError):
                    host.call("close", case=1)

    def test_wrong_feature_identity_stops_the_child(self):
        process = Mock(stdin=io.StringIO(), stdout=io.StringIO('{"host_version":1}\n'))
        with patch("service_contract_host.subprocess.Popen", return_value=process):
            with self.assertRaisesRegex(HostError, "Unsupported"):
                ServiceHost(sys.executable, expected_sha256=hashlib.sha256(Path(sys.executable).read_bytes()).hexdigest(), timeout=.05)
        process.wait.assert_called_once()
        self.assertTrue(process.stdin.closed)
        self.assertTrue(process.stdout.closed)

    def test_eof_is_reported_instead_of_waiting_or_retrying(self):
        host, _ = self.host([])
        with self.assertRaisesRegex(HostError, "closed its output"):
            host.call("close", case=1)

    def test_timeout_stops_host_and_never_retries_operation(self):
        host, process = self.host([])
        host._reader.join(timeout=1)
        host._responses = queue.Queue()
        with self.assertRaisesRegex(HostError, "timed out"):
            host.call("close", case=1)
        process.wait.assert_called_once()
        with self.assertRaisesRegex(HostError, "is closed"):
            host.call("close", case=1)

    def test_cleanup_is_idempotent_and_kills_unresponsive_child(self):
        host, process = self.host([])
        process.wait.side_effect = [subprocess.TimeoutExpired("host", .01), 0]
        host.close()
        host.close()
        process.kill.assert_called_once()
        self.assertEqual(process.wait.call_count, 2)

    def test_raw_multiline_input_is_rejected_without_writing(self):
        host, process = self.host([])
        with self.assertRaises(ValueError):
            host.raw("{}\n{}")
        self.assertEqual(process.stdin.getvalue(), "")

    def test_protocol_mismatch_stops_host_before_any_case(self):
        with self.assertRaisesRegex(HostError, "expected protocol"):
            self.host([], identity={**IDENTITY, "protocol": current_protocol() + 1})

    def test_unqualified_binary_is_rejected_before_launch(self):
        with patch("service_contract_host.subprocess.Popen") as launch:
            with self.assertRaisesRegex(HostError, "SHA-256"):
                ServiceHost(sys.executable, expected_sha256="wrong")
        launch.assert_not_called()

    def test_broken_write_and_close_still_reap_the_child(self):
        host, process = self.host([])
        original = process.stdin
        process.stdin = Mock()
        process.stdin.write.side_effect = BrokenPipeError("dead child")
        process.stdin.close.side_effect = BrokenPipeError("dead child")
        with self.assertRaisesRegex(HostError, "Cannot write.*dead child"):
            host.call("close", case=1)
        process.wait.assert_called_once()
        original.close()

    def test_watchdog_is_joined_before_success_and_late_expiry_is_not_success(self):
        host, _ = self.host([{"ok": {}}])
        watchdogs = []
        def watchdog(expired):
            value = Mock()
            value.cancel.side_effect = expired.set
            watchdogs.append(value)
            return value
        with patch.object(host, "_watchdog", side_effect=watchdog):
            with self.assertRaisesRegex(HostError, "operation timed out"):
                host.call("close", case=1)
        self.assertTrue(watchdogs)
        for value in watchdogs:
            value.cancel.assert_called_once()
            value.join.assert_called_once()

    def test_oversized_input_is_rejected_before_write(self):
        host, process = self.host([])
        with self.assertRaisesRegex(HostError, "exceeds 1 MiB"):
            host.raw("x" * (1024 * 1024))
        self.assertEqual(process.stdin.getvalue(), "")

    def test_oversized_output_fails_and_reaps_child(self):
        process = Mock(stdin=io.StringIO(), stdout=io.StringIO("x" * (2 * 1024 * 1024 + 2)))
        with patch("service_contract_host.subprocess.Popen", return_value=process):
            with self.assertRaisesRegex(HostError, "exceeds its bound"):
                ServiceHost(sys.executable,
                    expected_sha256=hashlib.sha256(Path(sys.executable).read_bytes()).hexdigest(), timeout=.1)
        process.wait.assert_called_once()

    def test_handshaken_child_that_never_reads_cannot_block_a_large_write(self):
        # Real OS pipes, portable across Windows and Linux. Only process launch
        # is redirected; the adapter's write, watchdog and cleanup are real.
        launch = subprocess.Popen
        script = f"import time; print({json.dumps(IDENTITY)!r}, flush=True); time.sleep(60)"
        def child(_command, **options):
            return launch([sys.executable, "-u", "-c", script], **options)
        with patch("service_contract_host.subprocess.Popen", side_effect=child):
            host = ServiceHost(sys.executable,
                expected_sha256=hashlib.sha256(Path(sys.executable).read_bytes()).hexdigest(), timeout=.5)
        self.addCleanup(host.close)
        started = time.monotonic()
        errors = []
        def write():
            try:
                host.raw(json.dumps({"op": "request", "padding": "x" * (512 * 1024)}))
            except HostError as error:
                errors.append(error)
        writer = threading.Thread(target=write, daemon=True)
        writer.start()
        try:
            writer.join(timeout=5)
            self.assertFalse(writer.is_alive(), "whole-operation deadline failed to release blocked pipe write")
        finally:
            # Keep the regression itself bounded even against the old broken
            # adapter: kill first to release a write before invoking close.
            if writer.is_alive():
                host.process.kill()
                writer.join(timeout=5)
        self.assertEqual(len(errors), 1)
        self.assertIn("timed out", str(errors[0]))
        self.assertLess(time.monotonic() - started, 5, "liveness bound, not a performance threshold")
        self.assertIsNotNone(host.process.returncode)
        self.assertTrue(host.process.stdin.closed)
        self.assertTrue(host.process.stdout.closed)
