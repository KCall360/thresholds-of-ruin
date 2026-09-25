"""Reject incomplete correlation and retain measured boundary distinctions."""
import unittest
from unittest.mock import patch
from timing_correlation import correlate_ack


class TimingCorrelation(unittest.TestCase):
    def test_server_client_reader_are_joined_by_request_identity(self):
        server = [dict(event='server_handled', request_id='request', unix_ns=3000000, lock_ms=.1, duration_ms=1),
                  dict(event='server_ack_sent', request_id='request', unix_ns=4000000, duration_ms=.1)]
        client = [dict(event='client_request', request_id='request', unix_ns=1000000),
                  dict(event='client_request_sent', request_id='request', unix_ns=2000000, duration_ms=1),
                  dict(event='client_ack', request_id='request', unix_ns=5000000),
                  dict(event='headless_report', request_id='request', unix_ns=7000000, duration_ms=2)]
        result = dict(actors=1,samples=[dict(request_id='request',actor=1,label='wait',expected='waited',
                      request_to_ack_ms=8,request_to_ack_line_ms=7,input_unix_ns=0,ack_line_unix_ns=7000000)])
        with patch('timing_correlation.events', side_effect=[server,client]):
            row, = correlate_ack('.', result)
        self.assertEqual(row['client_request_to_ack_ms'],4)
        self.assertEqual(row['server_handle_ms'],1)
        self.assertEqual(row['client_ack_to_reader_ms'],2)
        self.assertEqual(row['driver_queue_ms'],1)
        with patch('timing_correlation.events', side_effect=[server,client[:-1]]):
            with self.assertRaises(KeyError):
                correlate_ack('.', result)
