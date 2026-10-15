"""Pure artifact and evidence contracts for the ordinary earned-route driver."""
import json
from pathlib import Path
import tempfile
import unittest

from rogue_earned_route_process import (
    EvidenceError, compare_replay, fingerprint, verify_fingerprint, validate_receipts,
    classify_failure, record_acquisition, compress_completed_logs, comparison_surface, validate_initial_anchor,
)


class EarnedRouteEvidenceTests(unittest.TestCase):
    def test_source_and_binary_drift_invalidates_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'source'
            path.write_text('before')
            frozen = fingerprint([path])
            verify_fingerprint(frozen)
            path.write_text('after')
            with self.assertRaisesRegex(EvidenceError, 'changed'):
                verify_fingerprint(frozen)

    def test_missing_input_invalidates_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'binary'
            path.write_bytes(b'binary')
            frozen = fingerprint([path]); path.unlink()
            with self.assertRaises(EvidenceError):
                verify_fingerprint(frozen)

    def test_duplicate_activation_receipts_are_never_deduplicated(self):
        rows = [{'scope': 'dungeon/1', 'entrant': 2}, {'scope': 'dungeon/1', 'entrant': 2}]
        with self.assertRaisesRegex(EvidenceError, 'duplicate'):
            validate_receipts(rows, 2)
        self.assertEqual(validate_receipts(rows[:1], 2), rows[:1])
        with self.assertRaisesRegex(EvidenceError, 'entrant'):
            validate_receipts(rows[:1], 1)

    def test_replay_checks_commands_state_history_receipts_controller_and_flows(self):
        pristine = {field: [field] for field in ('commands', 'state', 'history', 'receipts', 'controller', 'flows')}
        self.assertEqual(compare_replay(pristine, json.loads(json.dumps(pristine))), [])
        for field in pristine:
            replay = {**pristine, field: ['different']}
            self.assertEqual(compare_replay(pristine, replay), [field])

    def test_error_and_timeout_never_become_survival(self):
        self.assertEqual(classify_failure(TimeoutError('wall deadline')), 'failed_infrastructure')
        self.assertEqual(classify_failure(AssertionError('Process output deadline')), 'failed_infrastructure')
        self.assertEqual(classify_failure(EvidenceError('bad receipt')), 'failed_evidence')
        self.assertEqual(classify_failure(ValueError('rejected action')), 'failed_controller')

    def test_accepted_take_needs_matching_owner_stock_and_full_stack(self):
        before = {'inventory': [{'known_supply': {'id': 'ration'}, 'quantity': '1'}],
                  'ground_items': [{'item': {'id': 'opaque', 'known_supply': {'id': 'ration'}, 'quantity': '2'}}]}
        after = {'inventory': [{'known_supply': {'id': 'ration'}, 'quantity': '3'}], 'ground_items': []}
        action = {'type': 'take', 'item': 'opaque', 'quantity': '2'}
        self.assertEqual(record_acquisition(before, action, after), {'supply': 'ration', 'quantity': 2, 'item': 'opaque'})
        with self.assertRaises(EvidenceError):
            record_acquisition(before, {**action, 'quantity': '1'}, after)
        with self.assertRaises(EvidenceError):
            record_acquisition(before, action, before)
        self.assertIsNone(record_acquisition(before, {'type': 'wait'}, after))

    def test_completed_log_compression_preserves_exact_raw_bytes(self):
        import gzip
        import hashlib
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            process = root / 'process-00-headless'; process.mkdir()
            raw = (b'raw observation with ordered history\n' * 100)
            path = process / 'stdout.log'; path.write_bytes(raw)
            rows = compress_completed_logs(root)
            self.assertEqual(len(rows), 1)
            self.assertFalse(path.exists())
            self.assertEqual(gzip.decompress((root / rows[0]['retained']).read_bytes()), raw)
            self.assertEqual(rows[0]['uncompressed_sha256'], hashlib.sha256(raw).hexdigest())
            self.assertEqual(rows[0]['uncompressed_bytes'], len(raw))

    def test_comparison_refuses_unavailable_gross_flows(self):
        with self.assertRaisesRegex(EvidenceError, 'incomplete ordinary'):
            comparison_surface({'ordinary_evidence_status': 'failed'})
        with self.assertRaisesRegex(EvidenceError, 'gross flows'):
            comparison_surface({'ordinary_evidence_status': 'complete', 'flows': {'status': 'unavailable'}})

    def test_initial_diagnostic_anchor_must_match_independent_save_exactly(self):
        anchor = {'lineage': 'saved-lineage', 'branch': 'saved-branch',
                  'history_sequence': 0, 'entry_id': None, 'tick': 0}
        begin = {'event': 'survival.begin', 'selected_actor': 2, **anchor}
        validate_initial_anchor(begin, anchor, 2)
        for field in ('branch', 'lineage', 'history_sequence', 'entry_id', 'tick'):
            with self.subTest(field=field), self.assertRaises(EvidenceError):
                validate_initial_anchor({**begin, field: 'wrong'}, anchor, 2)
        with self.assertRaises(EvidenceError):
            validate_initial_anchor(begin, anchor, 1)


if __name__ == '__main__':
    unittest.main()
