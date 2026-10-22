"""Reject incomplete correlation and retain measured boundary distinctions."""
import unittest
import json
import tempfile
from pathlib import Path
from unittest.mock import patch
from timing_correlation import correlate_ack, correlate_native, events, server_events


def ack_capture():
    server = [dict(event='server_handled', request_id='request', unix_ns=3000000, lock_ms=.1, duration_ms=1),
              dict(event='server_ack_sent', request_id='request', unix_ns=4000000, duration_ms=.1)]
    client = [dict(event='client_request', request_id='request', unix_ns=1000000),
              dict(event='client_request_sent', request_id='request', unix_ns=2000000, duration_ms=1),
              dict(event='client_ack', request_id='request', unix_ns=5000000),
              dict(event='headless_report', request_id='request', unix_ns=7000000, duration_ms=2)]
    result = dict(actors=1, samples=[dict(request_id='request', actor=1, label='wait', expected='waited',
                  request_to_ack_ms=8, request_to_ack_line_ms=7, input_unix_ns=0, ack_line_unix_ns=7000000)])
    return server, client, result


class TimingCorrelation(unittest.TestCase):
    def test_ack_report_window_uses_request_order_despite_clock_offset_or_adjustment(self):
        for input_clock in (-20000000, 20000000):
            for adjusted in (False, True):
                with self.subTest(input_clock=input_clock, adjusted=adjusted):
                    server, client, result = ack_capture()
                    result['samples'][0]['input_unix_ns'] = input_clock
                    if adjusted:
                        client[-1]['unix_ns'] = -5000000
                    client.insert(0, dict(event='headless_report', request_id=None, unix_ns=0, duration_ms=99))
                    client.insert(-1, dict(event='headless_report', request_id=None, unix_ns=6000000, duration_ms=8))
                    client.append(dict(event='headless_report', request_id=None, unix_ns=6000000, duration_ms=77))
                    with patch('timing_correlation.events', side_effect=[server, client]):
                        row, = correlate_ack('.', result)
                    self.assertEqual(row['headless_report_max_ms'], 8)
                    self.assertEqual(row['headless_ack_report_ms'], 2)
                    self.assertEqual(row['driver_before_client_ms'], (1000000 - input_clock) / 1e6)

    def test_ack_report_before_its_request_is_not_complete_evidence(self):
        server, client, result = ack_capture()
        client.insert(0, client.pop())
        with patch('timing_correlation.events', side_effect=[server, client]):
            with self.assertRaisesRegex(ValueError, 'precedes its request'):
                correlate_ack('.', result)

    def test_loss_reports_require_a_positive_integer_count(self):
        for count in (None, 0, -1, True, 1.5, '3'):
            with self.subTest(count=count):
                with patch('timing_correlation.events', return_value=[dict(
                        event='server_diagnostics_dropped', request_id='', dropped=count)]):
                    with self.assertRaisesRegex(ValueError, 'Invalid server diagnostic loss record'):
                        server_events('unused')

    def test_dropped_server_records_reject_ack_and_native_correlation(self):
        lost = dict(timing_version=1, event='server_diagnostics_dropped', request_id='', dropped=3)
        for correlate in (correlate_ack, correlate_native):
            with self.subTest(correlator=correlate.__name__):
                with patch('timing_correlation.events', return_value=[lost]):
                    with self.assertRaisesRegex(ValueError, 'dropped 3 records'):
                        correlate('.', dict(actors=0, samples=[]))

    def test_native_report_cost_belongs_to_preceding_frame_and_log_event(self):
        for input_done in (None, "up"):
            with self.subTest(input_done=input_done):
                server = [dict(event='server_handled', request_id='r', unix_ns=3000000, lock_ms=0, duration_ms=1),
                          dict(event='server_ack_sent', request_id='r', unix_ns=4000000)]
                client = [dict(event='client_request', request_id='r', revision=0, unix_ns=1000000),
                          dict(event='client_request_sent', request_id='r', duration_ms=1, previous_timing_write_ms=.4),
                          dict(event='client_ack', request_id='r', unix_ns=5000000, previous_timing_write_ms=.5),
                          dict(event='client_request', request_id='next', revision=1, previous_timing_write_ms=.6)]
                result = dict(samples=[dict(index=0, presented_frame=1, request_to_presentation_ms=9,
                              input_unix_ns=0,line_unix_ns=9000000,reader_work_ms=.1,queue_delay_ms=.2,
                              intermediate_profiles=[dict(previous_report_ms=99)])])
                # Simulation effects can be presented after the input acknowledgement;
                # the driver's explicit frame must work without a legacy inferred frame.
                frames = [dict(frame=1, input_done=input_done, busy=False, state=dict(revision="1"),
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

    def test_a_line_cut_off_when_a_process_is_killed_is_not_evidence(self):
        # Regression: a client killed at the end of a run left half a line,
        # and reading the log failed instead of ignoring it.
        complete = json.dumps({"timing_version": 1, "event": "client_ack", "request_id": "r"})
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "actor-1.stderr.log"
            path.write_text(complete + "\n" + '{"timing_version":1,"event":"cli')
            self.assertEqual([r["request_id"] for r in events(path)], ["r"])
            # A complete last line is kept, and a broken line in the middle
            # is still an error.
            path.write_text(complete + "\n")
            self.assertEqual(len(events(path)), 1)
            path.write_text('{"broken\n' + complete + "\n")
            with self.assertRaises(json.JSONDecodeError):
                events(path)


def native_capture():
    server, client, _ = ack_capture()
    client.pop()  # Native presentation is recorded on stdout, not headless_report.
    client[0]['revision'] = 0
    result = dict(samples=[dict(index=0, presented_frame=1, request_to_presentation_ms=9,
        input_unix_ns=0, line_unix_ns=9000000, reader_work_ms=.1, queue_delay_ms=.2,
        intermediate_profiles=[dict(previous_report_ms=99)])])
    frames = [dict(frame=1, input_done='up', busy=False, state=dict(revision='1'),
                   presented_unix_ns=7000000, profile=dict(previous_report_ms=99)),
              dict(frame=2, profile=dict(previous_report_ms=2,
                                        previous_report_encode_ms=.5, previous_report_write_ms=1.5))]
    return server, client, result, frames


class CorrelationEvidenceIdentity(unittest.TestCase):
    def producer_capture(self, native):
        server, client, result, frames = native_capture() if native else (*ack_capture(), [])
        request = 'b13ddda3-2c6a-4015-a664-d307c9b9a457'
        for row in server + client:
            row['request_id'] = request
        for row in server:
            row['client'] = 2
        if not native:
            result['samples'][0]['request_id'] = request
        return server, client, result, frames

    def complete_capture(self, native, server, client, result, frames):
        """Exercise the JSONL reader as well as correlation, with producer-shaped IDs."""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, rows in [('server.stderr.log', server),
                               ('ascii.stderr.log' if native else 'actor-1.stderr.log', client)]:
                (root / name).write_text(''.join(
                    json.dumps(dict(timing_version=1, **row)) + '\n' for row in rows), encoding='utf-8')
            if native:
                (root / 'ascii.stdout.jsonl').write_text(
                    ''.join(json.dumps(frame) + '\n' for frame in frames), encoding='utf-8')
            return (correlate_native if native else correlate_ack)(root, result)

    def test_real_client_attachment_ids_are_not_global_request_identities(self):
        # Connection::connect uses "attach" for every client; run_demo has a
        # spectator plus its actors, and the native driver later adds an observer.
        for native in (False, True):
            with self.subTest(native=native):
                server, client, result, frames = self.producer_capture(native)
                baseline = self.complete_capture(native, server, client, result, frames)
                self.assertEqual(len(baseline), 1)
                attachment = dict(event='server_handled', request_id='attach', client=1,
                                  unix_ns=0, lock_ms=0, duration_ms=0)
                server.insert(0, attachment)
                self.assertEqual(self.complete_capture(native, server, client, result, frames), baseline)
                server.extend(dict(attachment, client=identity) for identity in (2, 3))
                try:
                    actual = self.complete_capture(native, server, client, result, frames)
                except ValueError as error:
                    self.fail(f'Valid cross-client attachment evidence was rejected: {error}')
                self.assertEqual(actual, baseline)

    def test_measured_server_identity_stays_unique_amid_other_clients(self):
        for native in (False, True):
            for index in range(2):
                for conflicting_client in (False, True):
                    with self.subTest(native=native, index=index, conflicting_client=conflicting_client):
                        server, client, result, frames = self.producer_capture(native)
                        self.assertEqual(len(self.complete_capture(native, server, client, result, frames)), 1)
                        duplicate = dict(server[index])
                        if conflicting_client:
                            duplicate['client'] = 3
                            duplicate['unix_ns'] += 1
                        server.append(duplicate)
                        with self.assertRaisesRegex(ValueError, 'Duplicate.*' + duplicate['event']):
                            self.complete_capture(native, server, client, result, frames)

    def native(self, server, client, result, frames):
        with tempfile.TemporaryDirectory() as directory:
            (Path(directory) / 'ascii.stdout.jsonl').write_text(
                ''.join(json.dumps(frame) + '\n' for frame in frames), encoding='utf-8')
            with patch('timing_correlation.events', side_effect=[server, client]):
                return correlate_native(directory, result)

    def control_capture(self):
        server, client, result, frames = self.producer_capture(True)
        client[0]['request_kind'] = 'act'
        # The real native startup sends AcquireControl with its own UUID at
        # revision zero, then the first measured Act at that same revision.
        # Keep complete triples, but use small independent timing constants.
        control = '597ea555-58b7-46b6-8e71-0569031d53a6'
        client[:0] = [
            dict(event='client_request', request_id=control, revision=0,
                 request_kind='non_action', unix_ns=-3000000),
            dict(event='client_request_sent', request_id=control,
                 unix_ns=-2000000, duration_ms=1),
            dict(event='client_ack', request_id=control, unix_ns=-1000000),
        ]
        server[:0] = [
            dict(event='server_handled', request_id=control, client=2,
                 unix_ns=-2000000, lock_ms=0, duration_ms=1),
            dict(event='server_ack_sent', request_id=control, client=2,
                 unix_ns=-1500000),
        ]
        return server, client, result, frames

    def test_explicit_native_control_at_action_revision_preserves_measurement(self):
        baseline = self.complete_capture(True, *self.producer_capture(True))
        for control_first in (True, False):
            with self.subTest(control_first=control_first):
                server, client, result, frames = self.control_capture()
                if not control_first:
                    client[:] = client[3:] + client[:3]
                self.assertEqual(self.complete_capture(True, server, client, result, frames), baseline)

    def test_explicit_native_kind_preserves_clock_and_server_order_semantics(self):
        server, client, result, frames = self.control_capture()
        self.assertEqual(len(self.complete_capture(True, server, client, result, frames)), 1)
        # Wall-clock jumps cannot change request ownership. The send task can
        # also publish its server record before the handler diagnostic arrives.
        client[0]['unix_ns'] = 1000000000
        result['samples'][0]['input_unix_ns'] = 20000000
        server[-1]['unix_ns'] = 2000000
        server.reverse()
        row, = self.complete_capture(True, server, client, result, frames)
        self.assertEqual(row['input_to_client_request_ms'], -19)
        self.assertEqual(row['server_handle_to_ack_sent_ms'], -1)

    def test_native_kind_does_not_guess_legacy_or_competing_actions(self):
        for kind in ('act', 'legacy'):
            with self.subTest(kind=kind):
                server, client, result, frames = self.control_capture()
                self.assertEqual(len(self.complete_capture(True, server, client, result, frames)), 1)
                if kind == 'legacy':
                    del client[0]['request_kind']
                else:
                    client[0]['request_kind'] = kind
                with self.assertRaisesRegex(ValueError, 'Ambiguous native request revision 0'):
                    self.complete_capture(True, server, client, result, frames)

        server, client, result, frames = self.control_capture()
        self.assertEqual(len(self.complete_capture(True, server, client, result, frames)), 1)
        for row in client:
            row.pop('request_kind', None)
        # The original pre-classification trace cannot identify which complete
        # request was an action. Do not guess that the first UUID was setup.
        with self.assertRaisesRegex(ValueError, 'Ambiguous native request revision 0'):
            self.complete_capture(True, server, client, result, frames)

    def test_invalid_explicit_native_request_kind_is_not_legacy_or_excluded(self):
        for kind in (None, True, 0, '', 'acquire_control', [], {}):
            with self.subTest(kind=kind):
                server, client, result, frames = self.producer_capture(True)
                baseline = self.complete_capture(True, server, client, result, frames)
                client[0]['request_kind'] = 'act'
                self.assertEqual(self.complete_capture(True, server, client, result, frames), baseline)
                client[0]['request_kind'] = kind
                with self.assertRaisesRegex(ValueError, 'request_kind'):
                    self.complete_capture(True, server, client, result, frames)

    def test_ack_duplicate_required_events_are_not_last_record_wins(self):
        for source, count in [('server', 2), ('client', 4)]:
            for index in range(count):
                for conflicting in (False, True):
                    server, client, result = ack_capture()
                    with patch('timing_correlation.events', side_effect=[server, client]):
                        row, = correlate_ack('.', result)
                    self.assertEqual(row['client_request_to_ack_ms'], 4)
                    records = server if source == 'server' else client
                    duplicate = dict(records[index])
                    if conflicting:
                        duplicate['unix_ns'] += 1000000
                    records.insert(index + 1, duplicate)
                    with self.subTest(source=source, event=duplicate['event'], conflicting=conflicting):
                        with patch('timing_correlation.events', side_effect=[server, client]):
                            with self.assertRaisesRegex(ValueError, 'Duplicate.*' + duplicate['event']):
                                correlate_ack('.', result)

    def test_ack_requires_sequential_client_log_order_not_wall_clock_order(self):
        for left, right in [(0, 1), (1, 2), (2, 3)]:
            server, client, result = ack_capture()
            with patch('timing_correlation.events', side_effect=[server, client]):
                self.assertEqual(len(correlate_ack('.', result)), 1)
            client[left], client[right] = client[right], client[left]
            with self.subTest(left=left, right=right):
                with patch('timing_correlation.events', side_effect=[server, client]):
                    with self.assertRaisesRegex(ValueError, 'order'):
                        correlate_ack('.', result)

    def test_native_duplicate_events_and_measured_revision_are_ambiguous(self):
        for source, count in [('server', 2), ('client', 3), ('revision', 1)]:
            for index in range(count):
                server, client, result, frames = native_capture()
                row, = self.native(server, client, result, frames)
                self.assertEqual(row['client_ack_to_presented_ms'], 2)
                records = server if source == 'server' else client
                duplicate = dict(records[index])
                if source == 'revision':
                    duplicate['request_id'] = 'different-request-same-revision'
                    # Both candidates are otherwise complete: ambiguity, not a
                    # missing server/send/ack record, must reject this capture.
                    server.extend(dict(row, request_id=duplicate['request_id'])
                                  for row in list(server))
                    client.extend(dict(row, request_id=duplicate['request_id'])
                                  for row in list(client[1:]))
                records.insert(index + 1, duplicate)
                with self.subTest(source=source, index=index):
                    with self.assertRaisesRegex(ValueError, 'Duplicate|Ambiguous'):
                        self.native(server, client, result, frames)

    def test_native_requires_sequential_client_log_order(self):
        for left, right in [(0, 1), (1, 2)]:
            server, client, result, frames = native_capture()
            self.assertEqual(len(self.native(server, client, result, frames)), 1)
            client[left], client[right] = client[right], client[left]
            with self.subTest(left=left, right=right):
                with self.assertRaisesRegex(ValueError, 'order'):
                    self.native(server, client, result, frames)

    def test_native_duplicate_presented_frame_is_not_overwritten(self):
        server, client, result, frames = native_capture()
        self.assertEqual(len(self.native(server, client, result, frames)), 1)
        frames.insert(1, dict(frames[0], presented_unix_ns=8000000))
        with self.assertRaisesRegex(ValueError, 'Duplicate.*frame'):
            self.native(server, client, result, frames)

    def test_native_legacy_frame_lookup_rejects_ambiguous_revision_only_when_used(self):
        server, client, result, frames = native_capture()
        frames.insert(1, dict(frames[0], frame=3))
        # Explicit measured-frame identity remains authoritative even when
        # several presented frames acknowledge the same observation revision.
        self.assertEqual(len(self.native(server, client, result, frames)), 1)
        del result['samples'][0]['presented_frame']
        with self.assertRaisesRegex(ValueError, 'Ambiguous.*frame'):
            self.native(server, client, result, frames)

    def test_native_unmeasured_revision_collision_does_not_invalidate_explicit_frame(self):
        server, client, result, frames = native_capture()
        # Setup/query requests can share an observation revision. Only the
        # measured action's revision lookup needs to be unambiguous.
        client.extend([dict(event='client_request', request_id=name, revision=7)
                       for name in ('setup-one', 'setup-two')])
        self.assertEqual(len(self.native(server, client, result, frames)), 1)

    def test_constant_host_clock_offset_preserves_ack_and_native_intervals(self):
        for offset in (-1000000000, 1000000000):
            server, client, result = ack_capture()
            with patch('timing_correlation.events', side_effect=[server, client]):
                baseline = correlate_ack('.', result)
            for record in server + client:
                record['unix_ns'] += offset
            for key in ('input_unix_ns', 'ack_line_unix_ns'):
                result['samples'][0][key] += offset
            with patch('timing_correlation.events', side_effect=[server, client]):
                self.assertEqual(correlate_ack('.', result), baseline)
            server, client, result, frames = native_capture()
            baseline = self.native(server, client, result, frames)
            for record in server + client:
                record['unix_ns'] += offset
            for key in ('input_unix_ns', 'line_unix_ns'):
                result['samples'][0][key] += offset
            frames[0]['presented_unix_ns'] += offset
            self.assertEqual(self.native(server, client, result, frames), baseline)

    def test_async_server_log_order_and_signed_intervals_remain_valid(self):
        server, client, result = ack_capture()
        server[1]['unix_ns'] = 2000000
        server.reverse()  # The send task may log before Service returns.
        with patch('timing_correlation.events', side_effect=[server, client]):
            row, = correlate_ack('.', result)
        self.assertEqual(row['server_handle_to_ack_sent_ms'], -1)
        server, client, result, frames = native_capture()
        server[1]['unix_ns'] = 2000000
        server.reverse()
        row, = self.native(server, client, result, frames)
        self.assertEqual(row['server_handle_to_ack_sent_ms'], -1)
