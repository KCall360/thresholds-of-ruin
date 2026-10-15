"""Exact gameplay comparison with a closed, audited journal-envelope exception."""
import copy
import unittest

from rogue_route_replay import compare_execution


LEFT_IDS = ('11111111-1111-4111-8111-111111111111',
            '22222222-2222-4222-8222-222222222222')
RIGHT_IDS = ('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',
             'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb')
OTHER_ID = 'cccccccc-cccc-4ccc-8ccc-cccccccccccc'
BRANCH = 'dddddddd-dddd-4ddd-8ddd-dddddddddddd'


def execution(ids=LEFT_IDS):
    """Small independently authored example of the actual journal/wire schemas."""
    admit, resolved = ids
    return {
        'commands': [{'kind': 'action', 'phase': 'first_purchase',
                      'command': {'type': 'act', 'action': {'type': 'wait'}}}],
        'state': {'observation': {'actor': '1', 'target': 'opaque-unchanged',
                                  'combat': {'hp': 11, 'events': [{'damage': 1}]}}},
        'history': [{'id': resolved, 'branch': BRANCH, 'actor': '1', 'tick': '100',
                     'content': {'type': 'action', 'action': {'type': 'wait'},
                                 'event': {'type': 'waited'}}}],
        'receipts': [{'scope': 'dungeon/1', 'entrant': 1, 'population_seed': 42}],
        'controller': {'commands': 1, 'growth_history': [], 'opaque_target': 'opaque-unchanged'},
        'flows': {'lineage': BRANCH, 'branch': BRANCH, 'history_sequence': 2,
                  'entry_id': resolved, 'tick': 100, 'segments': 1,
                  'actors': {'1': {'nutrition_spent': 2, 'paid_start': {'stamina': 1}}}},
        'authoritative_anchor': {'lineage': BRANCH, 'branch': BRANCH,
                                 'history_sequence': 2, 'entry_id': resolved, 'tick': 100},
        'authoritative_records': [
            {'entry': {'id': admit, 'branch': BRANCH, 'actor': 1, 'tick': 0,
                       'content': {'type': 'intention_admitted', 'intention': 1,
                                   'action': {'type': 'wait'}}},
             'receipt': {'user': 'local', 'frontend': 'headless', 'request_id': 'fixed-request-1',
                         'actor': 1, 'branch': BRANCH,
                         'command': {'type': 'admit_intention', 'expected_revision': 0,
                                     'action': {'type': 'wait'}}}, 'activations': []},
            {'entry': {'id': resolved, 'branch': BRANCH, 'actor': 1, 'tick': 100,
                       'content': {'type': 'intention_started', 'admission': admit,
                                   'intention': 1, 'action': {'type': 'wait'},
                                   'event': {'type': 'waited'}}},
             'receipt': None, 'activations': []}],
    }


def purchase_execution(ids, stream):
    value = execution(ids)
    action = {'type': 'purchase',
              'basis': {'actor': 1, 'branch': BRANCH, 'stream': stream, 'reset': 1, 'build_epoch': 1},
              'purchase': {'owner': 0, 'pool': 'advancement',
                           'selection': {'kind': 'talent', 'talent': 'rattling_blow'}}}
    for record in value['authoritative_records']:
        record['entry']['content']['action'] = copy.deepcopy(action)
    value['authoritative_records'][0]['receipt']['command']['action'] = copy.deepcopy(action)
    return value


class RouteReplayTests(unittest.TestCase):
    def test_new_entry_ids_and_exact_references_have_published_bijection(self):
        left, right = execution(), execution(RIGHT_IDS)
        before = copy.deepcopy((left, right))
        result = compare_execution(left, right)
        self.assertTrue(result['equal'], result['differences'])
        self.assertFalse(result['raw_equal'])
        self.assertEqual(result['differences'], [])
        self.assertEqual([(row['left'], row['right']) for row in result['mapping']],
                         list(zip(LEFT_IDS, RIGHT_IDS)))
        self.assertTrue(result['normalization_schema']['rules'])
        self.assertEqual((left, right), before, 'Raw evidence must never be mutated')

    def test_identical_artifacts_are_raw_equal(self):
        result = compare_execution(execution(), execution())
        self.assertTrue(result['equal'])
        self.assertTrue(result['raw_equal'])

    def test_changed_gameplay_fact_fails_at_its_full_path(self):
        right = execution(RIGHT_IDS)
        right['state']['observation']['combat']['hp'] = 10
        result = compare_execution(execution(), right)
        self.assertFalse(result['equal'])
        self.assertIn('/state/observation/combat/hp', [d['path'] for d in result['differences']])

    def test_inconsistent_known_reference_fails(self):
        right = execution(RIGHT_IDS)
        right['authoritative_records'][1]['entry']['content']['admission'] = RIGHT_IDS[1]
        self.assertFalse(compare_execution(execution(), right)['equal'])

    def test_unknown_reference_fails_even_when_both_sides_match(self):
        left, right = execution(), execution(RIGHT_IDS)
        for side in (left, right):
            side['history'][0]['id'] = OTHER_ID
        result = compare_execution(left, right)
        self.assertFalse(result['equal'])
        self.assertTrue(any(d['kind'] == 'unresolved_entry_reference' for d in result['differences']))

    def test_uuid_at_unapproved_path_remains_strict_even_if_known(self):
        left, right = execution(), execution(RIGHT_IDS)
        left['state']['observation']['target'] = LEFT_IDS[0]
        right['state']['observation']['target'] = RIGHT_IDS[0]
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_nonaudited_segment_ids_remain_strict(self):
        left, right = execution(), execution(RIGHT_IDS)
        left['flows']['segment'] = LEFT_IDS[0]
        right['flows']['segment'] = RIGHT_IDS[0]
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_audited_receipt_request_id_has_separate_published_mapping(self):
        left, right = execution(), execution(RIGHT_IDS)
        left['authoritative_records'][0]['receipt']['request_id'] = LEFT_IDS[0]
        right['authoritative_records'][0]['receipt']['request_id'] = RIGHT_IDS[0]
        result = compare_execution(left, right)
        self.assertTrue(result['equal'], result['differences'])
        requests = [row for row in result['mapping'] if row['kind'] == 'request_id']
        self.assertEqual(len(requests), 1)
        self.assertEqual(requests[0]['left'], LEFT_IDS[0])
        self.assertEqual(requests[0]['right'], RIGHT_IDS[0])

    def test_request_id_reuse_must_stay_consistent(self):
        left, right = execution(), execution(RIGHT_IDS)
        for side, ids in ((left, LEFT_IDS), (right, RIGHT_IDS)):
            receipt = side['authoritative_records'][0]['receipt']
            receipt['request_id'] = ids[0]
            side['authoritative_records'][1]['receipt'] = copy.deepcopy(receipt)
        result = compare_execution(left, right)
        self.assertTrue(result['equal'], result['differences'])
        right['authoritative_records'][1]['receipt']['request_id'] = OTHER_ID
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_changed_command_payload_cannot_be_hidden_by_request_mapping(self):
        left, right = execution(), execution(RIGHT_IDS)
        left['authoritative_records'][0]['receipt']['request_id'] = LEFT_IDS[0]
        right['authoritative_records'][0]['receipt']['request_id'] = RIGHT_IDS[0]
        right['authoritative_records'][0]['receipt']['command']['action'] = {'type': 'move', 'direction': 'west'}
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_unknown_request_id_paths_stay_strict(self):
        left, right = execution(), execution(RIGHT_IDS)
        left['controller']['request_id'] = LEFT_IDS[0]
        right['controller']['request_id'] = RIGHT_IDS[0]
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_preexisting_request_ids_are_never_normalized(self):
        left, right = execution(), execution((LEFT_IDS[0], RIGHT_IDS[1]))
        left['initial_history_sequence'] = right['initial_history_sequence'] = 1
        left['authoritative_records'][0]['receipt']['request_id'] = LEFT_IDS[0]
        right['authoritative_records'][0]['receipt']['request_id'] = RIGHT_IDS[0]
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_every_subtree_and_event_order_is_preserved(self):
        for field in ('commands', 'state', 'history', 'receipts', 'controller', 'flows',
                      'authoritative_records', 'new_unrecognized_field'):
            with self.subTest(field=field):
                right = execution(RIGHT_IDS)
                right[field] = {'changed': True}
                self.assertFalse(compare_execution(execution(), right)['equal'])
        left, right = execution(), execution(RIGHT_IDS)
        left['state']['observation']['combat']['events'] = [1, 2]
        right['state']['observation']['combat']['events'] = [2, 1]
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_branch_lineage_rng_population_cost_and_progression_stay_exact(self):
        for field in ('branch', 'lineage'):
            right = execution(RIGHT_IDS)
            right['flows'][field] = OTHER_ID
            self.assertFalse(compare_execution(execution(), right)['equal'])
        for field in ('rules_rng', 'earned_xp', 'hp_roll', 'cost'):
            left, right = execution(), execution(RIGHT_IDS)
            left['controller'][field], right['controller'][field] = 1, 2
            self.assertFalse(compare_execution(left, right)['equal'])
        right = execution(RIGHT_IDS)
        right['receipts'][0]['population_seed'] += 1
        self.assertFalse(compare_execution(execution(), right)['equal'])

    def test_duplicate_record_identity_cannot_create_nonbijective_mapping(self):
        right = execution((RIGHT_IDS[0], RIGHT_IDS[0]))
        result = compare_execution(execution(), right)
        self.assertFalse(result['equal'])
        self.assertTrue(any(d['kind'] == 'duplicate_entry_id' for d in result['differences']))

    def test_non_uuid_record_identity_is_not_normalized(self):
        result = compare_execution(execution(), execution(('made-up', RIGHT_IDS[1])))
        self.assertFalse(result['equal'])
        self.assertTrue(any(d['kind'] == 'invalid_entry_id' for d in result['differences']))

    def test_missing_authoritative_records_cannot_establish_equivalence(self):
        left, right = execution(), execution(RIGHT_IDS)
        del left['authoritative_records']; del right['authoritative_records']
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_preexisting_accepted_history_is_never_normalized(self):
        left, right = execution(), execution(RIGHT_IDS)
        left['initial_history_sequence'] = right['initial_history_sequence'] = 1
        self.assertFalse(compare_execution(left, right)['equal'])
        right = execution((LEFT_IDS[0], RIGHT_IDS[1]))
        right['initial_history_sequence'] = 1
        result = compare_execution(left, right)
        self.assertTrue(result['equal'], result['differences'])
        self.assertEqual(len(result['mapping']), 1)

    def test_typed_annotation_and_receipt_references_are_mapped(self):
        left, right = execution(), execution(RIGHT_IDS)
        for side, ids in ((left, LEFT_IDS), (right, RIGHT_IDS)):
            side['history'][0]['content'] = {
                'type': 'annotation', 'anchor': {'type': 'entry', 'id': ids[0]}, 'text': 'same'}
            side['authoritative_records'][1]['receipt'] = {
                'command': {'type': 'resume_intention', 'admission': ids[0]}}
        result = compare_execution(left, right)
        self.assertTrue(result['equal'], result['differences'])

    def test_same_named_field_outside_its_typed_variant_is_strict(self):
        left, right = execution(), execution(RIGHT_IDS)
        for side, ids in ((left, LEFT_IDS), (right, RIGHT_IDS)):
            side['authoritative_records'][0]['entry']['content']['admission'] = ids[0]
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_history_query_cursor_is_an_explicit_entry_reference(self):
        left, right = execution(), execution(RIGHT_IDS)
        for side, ids in ((left, LEFT_IDS), (right, RIGHT_IDS)):
            side['commands'].append({'kind': 'read', 'phase': 'first_purchase', 'command': {
                'type': 'request', 'request': {'type': 'history', 'before': ids[1], 'limit': 20}}})
        result = compare_execution(left, right)
        self.assertTrue(result['equal'], result['differences'])
        right['commands'][1]['command']['request']['before'] = RIGHT_IDS[0]
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_non_uuid_request_labels_remain_exact(self):
        right = execution(RIGHT_IDS)
        right['authoritative_records'][0]['receipt']['request_id'] = 'changed-fixed-request'
        self.assertFalse(compare_execution(execution(), right)['equal'])

    def test_travel_journey_reference_keeps_its_target(self):
        left, right = execution(), execution(RIGHT_IDS)
        for side, ids in ((left, LEFT_IDS), (right, RIGHT_IDS)):
            side['authoritative_records'][1]['entry']['content'] = {
                'type': 'travel_intention_admitted', 'intention': 2, 'journey': ids[0],
                'step': 1, 'action': {'type': 'move', 'direction': 'north'}}
        result = compare_execution(left, right)
        self.assertTrue(result['equal'], result['differences'])
        right['authoritative_records'][1]['entry']['content']['journey'] = RIGHT_IDS[1]
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_approved_advancement_stream_has_audited_global_bijection(self):
        left = purchase_execution(LEFT_IDS, LEFT_IDS[0])
        right = purchase_execution(RIGHT_IDS, RIGHT_IDS[0])
        result = compare_execution(left, right)
        self.assertTrue(result['equal'], result['differences'])
        streams = [row for row in result['mapping'] if row['kind'] == 'advancement_stream']
        self.assertEqual(len(streams), 1)
        self.assertEqual(len(streams[0]['left_reference_paths']), 3)

    def test_inconsistent_advancement_stream_receipt_reference_fails(self):
        left = purchase_execution(LEFT_IDS, LEFT_IDS[0])
        right = purchase_execution(RIGHT_IDS, RIGHT_IDS[0])
        right['authoritative_records'][0]['receipt']['command']['action']['basis']['stream'] = OTHER_ID
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_advancement_stream_mapping_preserves_actor_branch_reset_and_build_epoch(self):
        for key, changed in (('actor', 2), ('branch', OTHER_ID), ('reset', 2), ('build_epoch', 2)):
            with self.subTest(key=key):
                left = purchase_execution(LEFT_IDS, LEFT_IDS[0])
                right = purchase_execution(RIGHT_IDS, RIGHT_IDS[0])
                right['authoritative_records'][0]['entry']['content']['action']['basis'][key] = changed
                self.assertFalse(compare_execution(left, right)['equal'])

    def test_stream_reuse_topology_cannot_change_across_reconnections(self):
        left = purchase_execution(LEFT_IDS, LEFT_IDS[0])
        right = purchase_execution(RIGHT_IDS, RIGHT_IDS[0])
        right['authoritative_records'][1]['entry']['content']['action']['basis']['stream'] = OTHER_ID
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_nonadvancement_action_basis_and_other_stream_paths_remain_strict(self):
        left = purchase_execution(LEFT_IDS, LEFT_IDS[0])
        right = purchase_execution(RIGHT_IDS, RIGHT_IDS[0])
        for value in (left, right):
            for record in value['authoritative_records']:
                record['entry']['content']['action']['type'] = 'wait'
            value['authoritative_records'][0]['receipt']['command']['action']['type'] = 'wait'
        self.assertFalse(compare_execution(left, right)['equal'])
        left, right = execution(), execution(RIGHT_IDS)
        left['controller']['stream'], right['controller']['stream'] = LEFT_IDS[0], RIGHT_IDS[0]
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_preexisting_advancement_stream_identity_remains_exact(self):
        left = purchase_execution(LEFT_IDS, LEFT_IDS[0])
        right = purchase_execution((LEFT_IDS[0], RIGHT_IDS[1]), RIGHT_IDS[0])
        left['initial_history_sequence'] = right['initial_history_sequence'] = 1
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_no_field_inside_accepted_prefix_is_normalized_even_if_it_names_a_new_id(self):
        left, right = execution(), execution((LEFT_IDS[0], RIGHT_IDS[1]))
        for side, target in ((left, LEFT_IDS[1]), (right, RIGHT_IDS[1])):
            side['initial_history_sequence'] = 1
            side['authoritative_records'][0]['entry']['content'] = {
                'type': 'intention_changed', 'admission': target, 'intention': 1,
                'change': 'cancelled'}
        self.assertFalse(compare_execution(left, right)['equal'])

    def test_json_scalar_types_are_not_conflated(self):
        right = execution(RIGHT_IDS)
        right['receipts'][0]['entrant'] = True
        self.assertFalse(compare_execution(execution(), right)['equal'])


if __name__ == '__main__':
    unittest.main()
