"""Conservative accounting of simulation facts shared by actor watchers."""
import json
import unittest
import socket
import threading
import time

import stream_relay


class CorruptedItemShapes(unittest.TestCase):
    def relay(self, corruption=None, *, large_base=False):
        relay = stream_relay.StreamRelay.__new__(stream_relay.StreamRelay)
        relay.large_retained_base = large_base
        for name in ('overdeep', 'overflow_delta', 'invalid_inventory',
                     'duplicate_inventory', 'oversized_retained_state',
                     'invalid_collection_range'):
            event = threading.Event()
            if name == corruption:
                event.set()
            setattr(relay, name, event)
        return relay

    def delta(self, corruption):
        state = {'inventory': [], 'ground_items': []}
        message = {'type': 'update', 'update': {'body': {
            'type': 'observation_delta', 'state': state}}}
        self.assertTrue(self.relay(corruption).corrupt_observation(message))
        return state

    def assert_nonsupply(self, item):
        self.assertIn('known_supply', tuple(item))
        self.assertIsNone(item['known_supply'])
        self.assertIn('known_currency', tuple(item))
        self.assertIsNone(item['known_currency'])
        self.assertFalse(item['identified'])

    def test_invalid_quantity_preserves_current_item_metadata(self):
        item = self.delta('invalid_inventory')['inventory'][0]['insert'][0]
        self.assert_nonsupply(item)
        self.assertEqual(item['quantity'], '0')

    def test_duplicate_inventory_preserves_current_item_metadata(self):
        items = self.delta('duplicate_inventory')['inventory'][0]['insert']
        self.assertEqual(len(items), 1001)
        self.assertEqual(items[-1], items[500])
        self.assertEqual(len({item['id'] for item in items}), 1000)
        for item in items:
            self.assert_nonsupply(item)
            self.assertEqual(item['quantity'], '1')

    def test_large_snapshot_preserves_current_item_metadata(self):
        observation = {'inventory': []}
        message = {'type': 'snapshot', 'request_id': 'attach',
                   'snapshot': {'state': {'observation': observation}}}
        self.assertTrue(self.relay(large_base=True).corrupt_observation(message))
        item = observation['inventory'][0]
        self.assert_nonsupply(item)
        self.assertEqual(item['quantity'], '1')
        self.assertEqual(len(item['description']), 8 * 1024 * 1024)

    def test_retained_size_delta_preserves_current_ground_metadata(self):
        ground = self.delta('oversized_retained_state')['ground_items'][0]['insert'][0]
        self.assertEqual(set(ground), {
            'presentation', 'reachable', 'supply_reachable', 'position', 'item'})
        self.assertIsNone(ground['presentation'])
        self.assertIn('supply_reachable', tuple(ground))
        self.assertFalse(ground['supply_reachable'])
        self.assert_nonsupply(ground['item'])
        self.assertFalse(ground['reachable'])
        self.assertEqual(len(ground['item']['description']), 8 * 1024 * 1024)

    def test_retained_size_payload_halves_fit_but_combined_records_exceed_ceiling(self):
        # Independent payload arithmetic, not a substitute for typed decoding:
        # the real-process relay also bounds each complete encoded frame.
        ceiling = 16 * 1024 * 1024
        observation = {'inventory': []}
        snapshot = {'type': 'snapshot', 'request_id': 'attach',
                    'snapshot': {'state': {'observation': observation}}}
        self.assertTrue(self.relay(large_base=True).corrupt_observation(snapshot))
        delta = self.delta('oversized_retained_state')
        self.assertEqual(delta['inventory'], [], 'delta must retain the large base item')
        self.assertEqual(len(delta['ground_items']), 1)
        edit = delta['ground_items'][0]
        self.assertEqual((edit['start'], edit['remove']), (0, 0))
        self.assertEqual(len(edit['insert']), 1)
        inserted = edit['insert'][0]
        self.assertNotEqual(observation['inventory'][0]['id'], inserted['item']['id'])
        message = {'type': 'update', 'update': {'body': {
            'type': 'observation_delta', 'state': delta}}}

        def size(value):
            return len(json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode('utf-8'))

        self.assertLess(size(snapshot), ceiling)
        self.assertLess(size(message), ceiling)
        # Even this subset exceeds the retained full-StateView ceiling. All
        # remaining state fields and their envelope only increase its size.
        self.assertGreater(size({'inventory': observation['inventory'],
                                 'ground_items': edit['insert']}), ceiling)


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
        status = {'actor': '1', 'branch': 'branch-1', 'intention': 'intention-5',
                  'entry_id': 'entry-5', 'phase': 'resolved'}
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
