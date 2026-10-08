"""Conservative accounting of simulation facts shared by actor watchers."""
import json
import unittest
import socket
import threading
import time

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
    def test_bounded_pressure_connection_drains_to_natural_eof(self):
        # Exercise actual TCP pressure and drain independently of game/client work.
        # The fake producer has a bounded send buffer; no socket is force-closed
        # by the relay and the receive capacity stays fixed through both phases.
        frame = b'\x82\x7e\x04\x00' + b'x' * 1024
        header = b'HTTP/1.1 101 Switching Protocols\r\n\r\n'
        count = 8192
        sent = threading.Event()
        errors = []
        listener = socket.socket()
        self.addCleanup(listener.close)
        listener.bind(('127.0.0.1', 0))
        listener.listen(1)
        listener.settimeout(5)

        def produce():
            try:
                with listener.accept()[0] as peer:
                    peer.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 65536)
                    peer.settimeout(20)
                    peer.sendall(header)
                    for _ in range(count):
                        peer.sendall(frame)
                    sent.set()
            except Exception as error:
                errors.append(error)

        sender = threading.Thread(target=produce, daemon=True)
        sender.start()
        self.addCleanup(sender.join, 5)
        relay = stream_relay.StreamRelay(
            '127.0.0.1:' + str(listener.getsockname()[1]), receive_buffer=stream_relay.PRESSURE_RECEIVE_BYTES)
        self.addCleanup(relay.close)
        relay.gate.clear()
        with socket.create_connection(('127.0.0.1', int(relay.address.rsplit(':', 1)[1]))) as peer:
            peer.settimeout(20)
            self.assertTrue(relay.held.wait(5))
            capacity = relay._server_socket.getsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF)
            self.assertLessEqual(capacity, 2 * stream_relay.PRESSURE_RECEIVE_BYTES)
            self.assertFalse(sent.wait(.2), 'paused relay must exert actual TCP pressure')
            relay.gate.set()
            received = 0
            deadline = time.monotonic() + 20
            while True:
                remaining = deadline - time.monotonic()
                self.assertGreater(remaining, 0, 'bounded relay drain deadline')
                peer.settimeout(remaining)
                data = peer.recv(65536)
                if not data:
                    break
                received += len(data)
            self.assertEqual(received, len(header) + count * len(frame))
        relay.thread.join(5)
        self.assertFalse(relay.thread.is_alive())
        self.assertEqual(relay._server_socket.fileno(), -1)
        sender.join(5)
        self.assertFalse(sender.is_alive())
        self.assertEqual(errors, [])
        self.assertEqual(relay.errors, [])
        self.assertTrue(sent.is_set())

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
