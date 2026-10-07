"""Conservative accounting of simulation facts shared by actor watchers."""
import json
import unittest

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


if __name__ == '__main__':
    unittest.main()
