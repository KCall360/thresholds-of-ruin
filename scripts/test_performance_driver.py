"""Preserve the legacy acknowledgement boundary while exposing driver delay."""
import io
import queue
from types import SimpleNamespace
import unittest
from unittest.mock import patch
from performance_driver import JsonProcess, wall_time_ns


class DriverTiming(unittest.TestCase):
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
