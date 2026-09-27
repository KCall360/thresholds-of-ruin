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
        client.lines.put(({"message":{"type":"ack"}}, 101.))
        client.lines.put(({"type":"ready"}, 102.))
        with patch("performance_driver.time.perf_counter", side_effect=[100.,103.,105.,106.]):
            _, start, legacy_ack, ready = client.send({"type":"act"})
        self.assertEqual((start, legacy_ack, ready), (100.,105.,102.))
        self.assertEqual(client.ack_line_received, 101.)
