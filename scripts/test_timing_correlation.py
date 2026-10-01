"""Reject incomplete correlation and retain measured boundary distinctions."""
import unittest
import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from timing_correlation import correlate_ack, correlate_native


class TimingCorrelation(unittest.TestCase):
    def test_native_report_cost_belongs_to_preceding_frame_and_log_event(self):
        server = [dict(event='server_handled', request_id='r', unix_ns=3000000, lock_ms=0, duration_ms=1),
                  dict(event='server_ack_sent', request_id='r', unix_ns=4000000)]
        client = [dict(event='client_request', request_id='r', revision=0, unix_ns=1000000),
                  dict(event='client_request_sent', request_id='r', duration_ms=1, previous_timing_write_ms=.4),
                  dict(event='client_ack', request_id='r', unix_ns=5000000, previous_timing_write_ms=.5),
                  dict(event='client_request', request_id='next', revision=1, previous_timing_write_ms=.6)]
        result = dict(samples=[dict(index=0, presented_frame=1, request_to_presentation_ms=9,
                      input_unix_ns=0,line_unix_ns=9000000,reader_work_ms=.1,queue_delay_ms=.2,
                      intermediate_profiles=[dict(previous_report_ms=99)])])
        frames = [dict(frame=1, input_done='right', busy=False, state=dict(revision=1),
                       presented_unix_ns=7000000,profile=dict(previous_report_ms=99)),
                  dict(frame=2,profile=dict(previous_report_ms=2,previous_report_encode_ms=.5,previous_report_write_ms=1.5))]
        with tempfile.TemporaryDirectory() as directory:
            (Path(directory)/'ascii.stdout.jsonl').write_text(''.join(json.dumps(f)+'\n' for f in frames))
            with patch('timing_correlation.events', side_effect=[server,client]):
                row, = correlate_native(directory,result)
        self.assertEqual(row['measured_frame_report_ms'],2)
        self.assertEqual(row['measured_frame_write_ms'],1.5)
        self.assertEqual(row['request_diagnostic_write_ms'],.4)
        self.assertEqual(row['ack_diagnostic_write_ms'],.6)
        self.assertEqual(row['client_ack_to_presented_ms'],2)
        self.assertEqual(row['presented_to_reader_ms'],2)

    def test_native_autopickup_pairs_the_move_when_revisions_skip_the_sample(self):
        # Index 5 is the key. Revision 5 is the previous take, 6 the move, 7 this take.
        # The presented frame's revision is 8, so the old frame table has no entry 5.
        server = [dict(event='server_handled', request_id='move', unix_ns=11000000, lock_ms=0, duration_ms=1),
                  dict(event='server_ack_sent', request_id='move', unix_ns=12000000)]
        client = [dict(event='client_request', request_id='earlier', revision=5, unix_ns=8000000),
                  dict(event='client_request_sent', request_id='earlier', duration_ms=1),
                  dict(event='client_ack', request_id='earlier', unix_ns=9000000),
                  dict(event='client_request', request_id='move', revision=6, unix_ns=11000000),
                  dict(event='client_request_sent', request_id='move', duration_ms=1, previous_timing_write_ms=.2),
                  dict(event='client_ack', request_id='move', unix_ns=13000000, previous_timing_write_ms=.3),
                  dict(event='client_request', request_id='take', revision=7, unix_ns=14000000),
                  dict(event='client_request_sent', request_id='take', duration_ms=1, previous_timing_write_ms=.4),
                  dict(event='client_ack', request_id='take', unix_ns=16000000)]
        result = dict(samples=[dict(index=5, presented_frame=9, request_to_presentation_ms=9,
                      input_unix_ns=10000000, line_unix_ns=17000000, reader_work_ms=.1, queue_delay_ms=.2,
                      intermediate_profiles=[dict(previous_report_ms=1)])])
        frames = [dict(frame=9, input_done='right', busy=False, state=dict(revision=8),
                       presented_unix_ns=16500000, profile=dict(previous_report_ms=1)),
                  dict(frame=10, profile=dict(previous_report_ms=3, previous_report_encode_ms=.5, previous_report_write_ms=2.5))]
        with tempfile.TemporaryDirectory() as directory:
            (Path(directory)/'ascii.stdout.jsonl').write_text(''.join(json.dumps(f)+'\n' for f in frames))
            with patch('timing_correlation.events', side_effect=[server, client]):
                row, = correlate_native(directory, result)
        self.assertEqual(row['request_id'], 'move')
        self.assertEqual(row['input_to_client_request_ms'], 1)
        self.assertEqual(row['measured_frame_report_ms'], 3)

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
