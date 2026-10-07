"""Conservative accounting of simulation facts shared by actor watchers."""
import json
import unittest
from unittest.mock import Mock, patch
import socket

import stream_relay


class SharedActorTraffic(unittest.TestCase):
    def size(self, value):
        return len(json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode('utf-8'))

    def test_counts_state_and_event_but_excludes_stream_and_authority_metadata(self):
        state = {'revision': '4', 'name': 'caf\u00e9'}
        event = {'actor': '1', 'text': 'waited'}
        for kind in ('observation', 'observation_delta'):
            message = {'type': 'update', 'update': {'context': {'stream': 'private-stream'},
                'cursor': {'sequence': '99'}, 'body': {'type': kind, 'state': state, 'event': event}}}
            self.assertEqual(stream_relay.actor_fact_bytes(message), self.size(state) + self.size(event))
            message['update']['body']['event'] = None
            self.assertEqual(stream_relay.actor_fact_bytes(message), self.size(state))

    def test_counts_intention_status_shared_with_spectators(self):
        status = {'actor': '1', 'id': '5', 'phase': 'executed'}
        message = {'type': 'update', 'update': {'body': {'type': 'intention', 'status': status}}}
        self.assertEqual(stream_relay.actor_fact_bytes(message), self.size(status))

    def test_excludes_replies_readiness_and_other_updates(self):
        for message in ({'type': 'ack', 'update': {'body': {'type': 'intention', 'status': {}}}},
                        {'type': 'snapshot'}, {'type': 'error'},
                        {'type': 'update', 'update': {'body': {'type': 'readiness', 'readiness': {}}}},
                        {'type': 'update', 'update': {'body': {'type': 'travel', 'status': {}}}}):
            self.assertEqual(stream_relay.actor_fact_bytes(message), 0)


class PressureConnection(unittest.TestCase):
    def test_resume_restores_receive_capacity_before_releasing_held_frame(self):
        relay = stream_relay.StreamRelay.__new__(stream_relay.StreamRelay)
        relay.receive_buffer = 4096
        relay._server_socket = Mock()
        relay.gate = Mock()
        calls = Mock()
        calls.attach_mock(relay._server_socket, 'socket')
        calls.attach_mock(relay.gate, 'gate')
        with patch.object(stream_relay.sys, "platform", "win32"):
            relay.resume_reading()
        self.assertEqual(calls.mock_calls, [
            unittest.mock.call.socket.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 65536),
            unittest.mock.call.gate.set(),
        ])

    def test_linux_resume_restores_advertised_window_before_releasing_pressure(self):
        relay = stream_relay.StreamRelay.__new__(stream_relay.StreamRelay)
        relay.receive_buffer = 4096
        relay._server_socket = Mock()
        relay.gate = Mock()
        capacity = {"buffer": 4096, "window": 4096}

        def configure(level, option, value):
            if (level, option) == (socket.SOL_SOCKET, socket.SO_RCVBUF):
                capacity["buffer"] = value
            elif (level, option) == (socket.IPPROTO_TCP, 10):
                capacity["window"] = value

        def resume():
            self.assertGreaterEqual(min(capacity.values()), 65536,
                                    "Receive memory and advertised window both remain pressure controls")

        relay._server_socket.setsockopt.side_effect = configure
        relay.gate.set.side_effect = resume
        with patch.object(stream_relay.sys, "platform", "linux"), \
                patch.object(socket, "TCP_WINDOW_CLAMP", 10, create=True):
            relay.resume_reading()
        relay.gate.set.assert_called_once_with()

    def test_resume_without_pressure_buffer_preserves_socket_configuration(self):
        relay = stream_relay.StreamRelay.__new__(stream_relay.StreamRelay)
        relay.receive_buffer = None
        relay._server_socket = Mock()
        relay.gate = Mock()
        relay.resume_reading()
        relay._server_socket.setsockopt.assert_not_called()
        relay.gate.set.assert_called_once_with()

    def test_server_socket_requires_matching_endpoint_and_owned_inode(self):
        # /proc/net/tcp lists local endpoint before remote endpoint. TIME_WAIT
        # can retain the same endpoints after the server has released its fd.
        rows = 'header\n 0: 0100007F:1F90 0100007F:C350 01 00000010:00000000 00:0 0 1000 0 12345 1\n'
        owned = {'12345'}
        self.assertTrue(stream_relay.tcp_socket_owned(rows, owned, 8080, 50000))
        self.assertFalse(stream_relay.tcp_socket_owned(rows, set(), 8080, 50000))
        self.assertFalse(stream_relay.tcp_socket_owned(rows, owned, 50000, 8080))
        self.assertFalse(stream_relay.tcp_socket_owned('header\n', owned, 8080, 50000))


if __name__ == '__main__':
    unittest.main()
