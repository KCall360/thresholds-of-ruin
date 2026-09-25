"""Preserve the legacy acknowledgement boundary while exposing driver delay."""
import io
import queue
from types import SimpleNamespace
import unittest
from unittest.mock import patch
from performance_driver import JsonProcess


class DriverTiming(unittest.TestCase):
    def test_ack_line_timestamp_excludes_later_driver_logging_and_queue_delay(self):
        client = JsonProcess.__new__(JsonProcess)
        client.lines = queue.Queue()
        client.child = SimpleNamespace(stdin=io.StringIO())
        client.lines.put(({"message":{"type":"ack"}}, 101.))
        client.lines.put(({"type":"ready"}, 102.))
        with patch("performance_driver.time.perf_counter", side_effect=[100.,105.]):
            _, start, legacy_ack, ready = client.send({"type":"act"})
        self.assertEqual((start, legacy_ack, ready), (100.,105.,102.))
        self.assertEqual(client.ack_line_received, 101.)
