"""Pure artifact and evidence contracts for the ordinary earned-route driver."""
import copy
from contextlib import closing
import json
import sqlite3
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, Mock

from rogue_earned_route_process import (
    EvidenceError, compare_replay, fingerprint, verify_fingerprint, validate_receipts,
    classify_failure, record_acquisition, compress_completed_logs, comparison_surface, validate_initial_anchor,
    validate_resume_binding, validate_saved_authority, retain_segment, verify_retained_segment,
    RouteExecution, prepare_campaign, POLICY_REVISION, main, read_authoritative_evidence,
    ArchiveVerificationCache, RouteMeasurements, verify_earned_history, validate_restored_picture, COMPARISON_SURFACES,
)


class RestoredPictureIdentity(unittest.TestCase):
    def test_only_attachment_picture_capabilities_change(self):
        old = {'state': {'snapshot': 'old', 'revision': '12', 'tick': '100',
                         'observation': {'ready': True}, 'memory': ['exact']},
               'history': [{'event': 'wait', 'tick': '0'}]}
        current = copy.deepcopy(old)
        current['state'].update(snapshot='new', revision='1')
        validate_restored_picture(current, old)
        for field, value in [('tick', '101'), ('observation', {'ready': False}),
                             ('memory', [])]:
            changed = copy.deepcopy(current)
            changed['state'][field] = value
            with self.subTest(field=field), self.assertRaises(EvidenceError):
                validate_restored_picture(changed, old)
        current['history'] = []
        with self.assertRaises(EvidenceError):
            validate_restored_picture(current, old)


class CompactRouteSnapshots(unittest.TestCase):
    def route(self, root):
        route = RouteExecution.__new__(RouteExecution)
        route.output = root
        route.controller = Mock(dump=lambda: {'commands': 1, 'nested': ['unchanged', None]})
        route.commands, route.acquisitions = [{'kind': 'action', 'command': {'type': 'wait'}}], []
        route.counts, route.phase_ticks = {'explore': 1}, {'explore': 2}
        route.current, route.started = None, None
        route.report = {'evidence_status': 'incomplete', 'ordinary_evidence_status': 'incomplete',
                        'outcome': 'censored_action_budget', 'frozen_inputs': {'source': 'exact'}}
        return route

    def assert_snapshot(self, route):
        raw = (route.output / 'attempt.json').read_bytes()
        expected = (json.dumps(route.report, sort_keys=True, allow_nan=False, separators=(',', ':')) + '\n').encode()
        self.assertEqual(raw, expected)
        self.assertEqual(json.loads(raw), route.report)

    def test_every_progress_write_retains_the_complete_report_in_compact_form(self):
        with tempfile.TemporaryDirectory() as directory:
            route = self.route(Path(directory))
            for sequence in (1, 2):
                route.report['progress_sequence'] = sequence
                route.persist()
                self.assert_snapshot(route)

    def test_final_retention_update_is_also_written_with_all_fields(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); route = self.route(root)
            save = root / 'game.db'; save.touch()
            route.harness = Mock(save=save)
            route.stop_processes, route.check_time = Mock(), Mock()
            with patch('rogue_earned_route_process.retain_segment', return_value={'identity': 'retained'}):
                route.finalize_segment()
            self.assert_snapshot(route)
            self.assertIn('last_stopped_unix', route.report)
            self.assertEqual(route.report['retained_segment'], {'identity': 'retained'})
            route.stop_processes.assert_called_once_with()


class EarnedRouteEvidenceTests(unittest.TestCase):
    def test_saved_milestone_distinguishes_body_composition_from_personal_advancement(self):
        for actor, source in ((1, 'warrior'), (2, 'mage')):
            for owned in (False, True):
                with self.subTest(actor=actor, owned=owned):
                    class_rows = [{'source': {'kind': 'class', 'id': source},
                                   'owner': {'kind': 'personal', 'id': identity} if owned else None}
                                  for identity in (1, 2)]
                    package = {'id': 'human-body', 'advancement': [{'source': {'kind': 'racial'}}]}
                    body = {'base': package, 'active': copy.deepcopy(package),
                            'birth_personal_count': 1} if owned else None
                    body_rows = [{'source': {'kind': 'racial'},
                                  'owner': {'kind': 'body', 'form': 'human-body', 'ordinal': 0}}] if owned else []
                    progress = {'authored_baseline': 1, 'earned_xp': 100, 'spent_xp': 100,
                                'highest_earned_tier': 2, 'highest_applied_tier': 2}
                    evidence = {'selected': actor, 'creature': {'build': {'body_build': body,
                                'hd': body_rows + class_rows}, 'earned_progress': progress}}
                    overview = {'growth': {key: str(value) for key, value in progress.items()}}
                    original = copy.deepcopy(evidence)
                    result = verify_earned_history(evidence, actor, overview)
                    self.assertEqual(result, original['creature']['build']['hd'])
                    result[0]['source']['kind'] = 'mutated-result'
                    self.assertEqual(evidence, original, 'returned history must not alias authoritative evidence')
                    for field in ('earned_xp', 'spent_xp', 'highest_earned_tier', 'highest_applied_tier'):
                        malformed = copy.deepcopy(original)
                        malformed['creature']['earned_progress'][field] = 0
                        with self.subTest(field=field), self.assertRaises(EvidenceError):
                            verify_earned_history(malformed, actor, overview)
                    if owned:
                        for mutation in ('body_only', 'new_form', 'duplicate_owner', 'extra_body', 'wrong_class'):
                            malformed = copy.deepcopy(original)
                            build = malformed['creature']['build']
                            if mutation == 'body_only':
                                build['hd'][-1] = {'source': {'kind': 'racial'},
                                                  'owner': {'kind': 'body', 'form': 'human-body', 'ordinal': 1}}
                            elif mutation == 'new_form':
                                build['body_build']['active']['id'] = 'different-form'
                            elif mutation == 'duplicate_owner':
                                build['hd'][-1]['owner'] = copy.deepcopy(build['hd'][-2]['owner'])
                            elif mutation == 'extra_body':
                                build['hd'].insert(1, copy.deepcopy(build['hd'][0]))
                            else:
                                build['hd'][-1]['source']['id'] = 'other-class'
                            with self.subTest(mutation=mutation), self.assertRaises(EvidenceError):
                                verify_earned_history(malformed, actor, overview)

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

    def test_comparison_preserves_complete_payload_but_not_picture_issuance(self):
        report = {field: [] for field in COMPARISON_SURFACES}
        report.update(ordinary_evidence_status='complete', flows={'actors': []},
                      state={'snapshot': 'old', 'revision': '25',
                             'observation': {'tick': '100', 'handle': 'private-handle'}})
        before = copy.deepcopy(report)
        first = comparison_surface(report)
        self.assertEqual(report, before, 'comparison must not rewrite raw evidence')
        report['state'].update(snapshot='new', revision='5')
        self.assertEqual(first, comparison_surface(report))
        report['state']['observation']['tick'] = '101'
        self.assertNotEqual(first, comparison_surface(report))
        self.assertEqual(first['state']['observation']['handle'], 'private-handle')

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


class RouteResumeHardeningTests(unittest.TestCase):
    def report(self, root):
        from rogue_earned_route import Controller
        source = root / 'source'; source.write_text('pinned')
        binary = root / 'server'; binary.write_text('binary')
        package = root / 'package'; package.mkdir()
        (package / 'scenario.toml').write_text('package')
        from arena_evaluation import files
        return {'actor': 1, 'seed': 0, 'arm': 'production', 'policy_revision': POLICY_REVISION,
                'first_command_limit': 2500, 'wall_limit_seconds': 900,
                'resume_limit': 16, 'minimum_free_bytes': 512 * 1024 * 1024,
                'require_diagnostics': True, 'package_path': str(package.resolve()),
                'chunk_soft_bytes': 16 * 1024 * 1024, 'chunk_hard_bytes': 32 * 1024 * 1024,
                'frozen_inputs': fingerprint([source, binary]), 'package_inputs': files(package),
                'controller': Controller(1).dump()}

    def test_resume_rejects_retained_identity_configuration_or_controller_drift(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); retained = self.report(root)
            validate_resume_binding(retained, retained, root / 'package')
            for field, wrong in [('actor', 2), ('seed', 1), ('arm', 'candidate'),
                    ('policy_revision', 3), ('first_command_limit', 99),
                    ('wall_limit_seconds', 901), ('resume_limit', 17),
                    ('minimum_free_bytes', 1), ('require_diagnostics', False),
                    ('package_path', str(root / 'elsewhere')), ('chunk_soft_bytes', 1), ('chunk_hard_bytes', 1)]:
                with self.subTest(field=field):
                    changed = copy.deepcopy(retained); changed[field] = wrong
                    with self.assertRaises(ValueError):
                        validate_resume_binding(changed, retained, root / 'package')
            for field, wrong in [('character', 2), ('first_limit', 99), ('floor_limit', 99),
                                 ('run_limit', 99), ('version', 3)]:
                changed = copy.deepcopy(retained); changed['controller'][field] = wrong
                with self.subTest(controller=field), self.assertRaises(ValueError):
                    validate_resume_binding(changed, retained, root / 'package')

    def test_resume_checks_retained_source_binary_and_package_bytes(self):
        for relative in ('source', 'server', 'package/scenario.toml'):
            with self.subTest(relative=relative), tempfile.TemporaryDirectory() as directory:
                root = Path(directory); retained = self.report(root)
                (root / relative).write_text('changed')
                expected = copy.deepcopy(retained)
                expected['frozen_inputs'] = fingerprint([root / 'source', root / 'server'])
                with self.assertRaises(ValueError):
                    validate_resume_binding(retained, expected, root / 'package')

    def test_untracked_runtime_helper_drift_rejects_reconstructed_resume_before_launch(self):
        from process_harness import SUFFIX
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / 'scripts'; scripts.mkdir()
            for name in ('rogue_earned_route.py', 'rogue_earned_route_process.py', 'process_work_budget.py', 'scalar_wire.py'):
                (scripts / name).write_text('original')
            binary = root / 'bin'; binary.mkdir()
            for name in ('tor-server', 'tor-client-headless', 'tor-scenario'):
                (binary / (name + SUFFIX)).write_text('binary')
            package = root / 'package'; package.mkdir()
            (package / 'scenario.toml').write_text('package')
            output = root / 'attempt'; output.mkdir()
            harness = Mock(bin=binary)
            # No dependency is in the Git index: runtime closure must be explicit.
            with patch('rogue_earned_route_process.ROOT', root), \
                    patch('rogue_earned_route_process.subprocess.check_output', return_value=b''):
                original = RouteExecution(harness, output, actor=1, seed=0, package=package)
                original.report['outcome'] = 'acceptance_prefix_saved'
                original.persist()
                original.report['retained_segment'] = retain_segment(output, output / 'unused.db', original.report)
                original.persist()
                before = (output / 'attempt.json').read_bytes()
                for name in ('process_work_budget.py', 'scalar_wire.py'):
                    with self.subTest(helper=name):
                        (scripts / name).write_text('changed runtime behavior')
                        reconstructed = RouteExecution(harness, output, actor=1, seed=0, package=package)
                        with self.assertRaisesRegex(EvidenceError, 'Retained source/executable fingerprints changed'):
                            reconstructed.resume()
                        (scripts / name).write_text('original')
            harness.server.assert_not_called()
            harness.launch.assert_not_called()
            self.assertEqual((output / 'attempt.json').read_bytes(), before)

    def test_exact_saved_authority_compares_every_accepted_prefix_surface(self):
        retained = {'actor': 1, 'authoritative_anchor': {'tick': 2, 'entry_id': 'exact'},
                    'authoritative_records': [{'entry': 'raw'}], 'receipts': [{'scope': 'one'}],
                    'final_creature': {'build': {'hd': [{'health_seed': 3}]}}}
        private = {'selected': 1, 'anchor': retained['authoritative_anchor'],
                   'records': retained['authoritative_records'], 'receipts': retained['receipts'],
                   'creature': retained['final_creature']}
        validate_saved_authority(retained, private)
        for field in private:
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_saved_authority(retained, {**private, field: None})

    def test_saved_authority_does_not_coerce_boolean_and_integer_scalars(self):
        retained = {'actor': 1, 'authoritative_anchor': {}, 'authoritative_records': [{'value': 1}],
                    'receipts': [], 'final_creature': {}}
        private = {'selected': 1, 'anchor': {}, 'records': [{'value': True}],
                   'receipts': [], 'creature': {}}
        with self.assertRaisesRegex(ValueError, 'authoritative_records'):
            validate_saved_authority(retained, private)

    def test_retention_closes_both_database_handles_on_success_and_backup_failure(self):
        connect = sqlite3.connect
        class BackupFailure(sqlite3.Connection):
            def backup(self, target, **kwargs):
                raise OSError('injected backup failure')
        for failure in (False, True):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory:
                root = Path(directory); save = root / 'game.db'
                with closing(connect(save)) as db, db:
                    db.execute('create table value(x)')
                    db.execute('insert into value values (1)')
                opened = []
                def tracked(*args, **kwargs):
                    db = connect(*args, **kwargs, factory=BackupFailure if failure else sqlite3.Connection)
                    opened.append(db)  # Keep alive: GC must not be the close policy.
                    return db
                try:
                    with patch('rogue_earned_route_process.sqlite3.connect', side_effect=tracked):
                        if failure:
                            with self.assertRaisesRegex(OSError, 'injected backup failure'):
                                retain_segment(root, save, {'outcome': 'earned_hd2'})
                        else:
                            retained = retain_segment(root, save, {'outcome': 'earned_hd2'})
                            with closing(connect(root / retained['save'])) as snapshot:
                                self.assertEqual(snapshot.execute('select x from value').fetchall(), [(1,)])
                    self.assertEqual(len(opened), 2)
                    for db in opened:
                        with self.subTest(connection=opened.index(db)), self.assertRaises(sqlite3.ProgrammingError):
                            db.execute('select 1')
                finally:
                    for db in opened:
                        db.close()

    def test_authority_audit_closes_database_on_success_and_query_failure(self):
        connect = sqlite3.connect
        for failure in (False, True):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory:
                save = Path(directory) / 'game.db'
                with closing(connect(save)) as db, db:
                    if not failure:
                        db.execute('create table journal(sequence, frame)')
                        db.execute('create table history(sequence, frame)')
                        db.execute('create table checkpoint(sequence, payload)')
                        archive = {'branch': 'exact', 'initial_activations': [],
                                   'scenario': {'package': {'selected': 1}}}
                        db.execute('insert into journal values (0, ?)',
                                   (b'x' * 24 + json.dumps({'archive': archive}).encode(),))
                opened = []
                def tracked(*args, **kwargs):
                    db = connect(*args, **kwargs)
                    opened.append(db)
                    return db
                try:
                    with patch('rogue_earned_route_process.sqlite3.connect', side_effect=tracked):
                        if failure:
                            with self.assertRaises(sqlite3.OperationalError):
                                read_authoritative_evidence(save, 1)
                        else:
                            self.assertEqual(read_authoritative_evidence(save, 1)['selected'], 1)
                    self.assertEqual(len(opened), 1)
                    with self.assertRaises(sqlite3.ProgrammingError):
                        opened[0].execute('select 1')
                finally:
                    for db in opened:
                        db.close()

    def test_segment_retention_is_immutable_and_sqlite_backup_is_consistent(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); save = root / 'game.db'
            with closing(sqlite3.connect(save)) as db, db:
                db.execute('pragma journal_mode=wal'); db.execute('create table value (x)')
                db.execute('insert into value values (1)'); db.commit()
                report = {'outcome': 'earned_hd2', 'controller': {'commands': 128}}
                first = retain_segment(root, save, report)
                report['retained_segment'] = first
                verify_retained_segment(root, report)
                saved = root / first['save']
                with closing(sqlite3.connect(saved)) as snapshot:
                    self.assertEqual(snapshot.execute('select x from value').fetchall(), [(1,)])
                original = (root / first['report']).read_bytes()
                db.execute('insert into value values (2)'); db.commit()
                report['controller']['commands'] = 256
                second = retain_segment(root, save, report)
                self.assertNotEqual(first['report'], second['report'])
                self.assertEqual((root / first['report']).read_bytes(), original)
                report['retained_segment'] = second
                verify_retained_segment(root, report)
                report['controller']['commands'] = 257
                with self.assertRaises(ValueError):
                    verify_retained_segment(root, report)

    def test_transport_boundary_is_compact_and_binds_append_only_trace_and_compressed_logs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'commands.jsonl').write_text('{"action":"wait"}\n')
            process = root / 'process-00-headless'; process.mkdir()
            (process / 'stdout.log').write_text('ordinary frame\n')
            report = {'outcome': 'acceptance_prefix_saved', 'controller': {'commands': 128},
                      'authoritative_records': [{'entry': 'strict'}]}
            retained = retain_segment(root, root / 'unread-game.db', report)
            self.assertEqual(retained['kind'], 'transport')
            self.assertNotIn('save', retained)
            self.assertNotIn('report', retained)
            report['retained_segment'] = retained
            compress_completed_logs(root)
            verify_retained_segment(root, report)
            with (root / 'commands.jsonl').open('a') as stream:
                stream.write('{"action":"unexpected"}\n')
            with self.assertRaisesRegex(ValueError, 'append-only trace'):
                verify_retained_segment(root, report)

    def test_log_retention_merges_verifies_and_is_idempotent(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for number in range(2):
                process = root / f'process-{number:02d}-headless'; process.mkdir()
                (process / 'stdout.log').write_bytes(bytes([number]) * 200)
                rows = compress_completed_logs(root)
                self.assertEqual(len(rows), number + 1)
            self.assertEqual(compress_completed_logs(root), rows)
            (root / rows[0]['retained']).write_bytes(b'corrupt')
            with self.assertRaises(ValueError):
                compress_completed_logs(root)

    def test_campaign_requires_explicit_arm_snapshots_before_generation(self):
        with tempfile.TemporaryDirectory() as directory, patch.dict('os.environ', {}, clear=True):
            with self.assertRaisesRegex(ValueError, 'explicit'):
                prepare_campaign(Path(directory) / 'campaign', Path(directory))

    def test_missing_cli_sources_fail_before_any_binary_build(self):
        with tempfile.TemporaryDirectory() as directory, patch.dict('os.environ', {}, clear=True), \
                patch('rogue_earned_route_process.binaries') as build, \
                patch('sys.stderr'):
            with self.assertRaises(SystemExit):
                main([directory, '--prepare-only'])
            build.assert_not_called()

    def test_post_stop_authority_or_retention_failure_invalidates_ordinary_completion(self):
        for failure in ('authority', 'retention'):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory:
                root = Path(directory); save = root / 'game.db'; save.touch()
                route = RouteExecution.__new__(RouteExecution)
                route.output, route.actor = root, 1
                route.harness = Mock(save=save)
                route.stop_processes, route.check_time, route.persist = Mock(), Mock(), Mock()
                route.report = {'outcome': 'earned_hd2', 'evidence_status': 'incomplete',
                                'ordinary_evidence_status': 'complete'}
                with patch('rogue_earned_route_process.read_authoritative_evidence', return_value={}), \
                        patch('rogue_earned_route_process.validate_saved_authority',
                              side_effect=EvidenceError('post-stop authority changed') if failure == 'authority' else None), \
                        patch('rogue_earned_route_process.retain_segment', side_effect=OSError('retention failed')):
                    route.finalize_segment()
                marker = json.loads((root / 'retention-failure.json').read_text())
                self.assertEqual(marker['evidence_status'], 'failed')
                self.assertEqual(route.report['evidence_status'], 'failed')
                self.assertEqual(route.report['ordinary_evidence_status'], 'incomplete')

    def test_cli_success_requires_nonfailed_evidence_and_no_retention_failure_marker(self):
        for failed, marker, expected in ((True, False, 2), (False, True, 2), (False, False, 0)):
            with self.subTest(failed=failed, marker=marker), tempfile.TemporaryDirectory() as directory:
                root = Path(directory); output = root / 'acceptance-seed42'; output.mkdir()
                if marker:
                    (output / 'retention-failure.json').write_text('{"evidence_status":"failed"}')
                route = Mock(output=output)
                route.execute.return_value = {'outcome': 'earned_hd2',
                    'ordinary_evidence_status': 'complete', 'evidence_status': 'failed' if failed else 'incomplete'}
                with patch('rogue_earned_route_process.configured_arm_sources', return_value={}), \
                        patch('rogue_earned_route_process.binaries', return_value=root), \
                        patch('rogue_earned_route_process.prepare_campaign'), \
                        patch('rogue_earned_route_process.RouteExecution', return_value=route), \
                        patch('rogue_earned_route_process.compress_completed_logs'), \
                        patch('builtins.print'):
                    self.assertEqual(main([str(root), '--acceptance-seed42']), expected)

    def test_resume_identity_failure_precedes_server_launch_and_preserves_original(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); retained = self.report(root)
            save = root / 'game.db'
            with closing(sqlite3.connect(save)) as db, db:
                db.execute('create table value(x)')
            retained['retained_segment'] = retain_segment(root, save, retained)
            attempt = root / 'attempt.json'; attempt.write_text(json.dumps(retained))
            before = attempt.read_bytes()
            route = RouteExecution.__new__(RouteExecution)
            route.output, route.package = root, root / 'package'
            route.measurements = RouteMeasurements()
            route.archive_cache = ArchiveVerificationCache(measurements=route.measurements)
            route.report = {**retained, 'seed': 1}; route.harness = Mock()
            with self.assertRaisesRegex(ValueError, 'seed'):
                route.resume()
            route.harness.server.assert_not_called()
            self.assertEqual(attempt.read_bytes(), before)

    def test_frozen_baseline_fixture_has_exact_source_closure_and_original_emu_snake_stages(self):
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v14'
        manifest = json.loads((snapshot / 'source.json').read_text())
        self.assertEqual(files(snapshot / 'package'), manifest['package_inputs'])
        self.assertTrue(manifest['reference'].startswith('8d09063840c51aa4189b146c287829102856b279:'))
        source = tomllib.loads((snapshot / 'package/scenario.toml').read_text())
        stages = source['floor_activation']['profiles']['rogue.low_depth']['stages']
        self.assertEqual([stage['id'] for stage in stages[:2]], ['rogue.emus', 'rogue.snakes'])
        self.assertEqual([stage['count'] for stage in stages[:2]], [[2, 2], [2, 2]])
        self.assertEqual(set(manifest['rules_dependencies']), {'interactions-v45'})

    def test_historical_baseline_retains_its_original_rules_identity_transition(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v6'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        self.assertEqual(transition['previous_snapshot'], 'rogue-earned-baseline-v5')
        self.assertEqual(transition['current_snapshot'], 'rogue-earned-baseline-v6')
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (15, 15))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v37', 'interactions-v38'))
        self.assertEqual(set(transition['previous_rules_dependencies']), {'interactions-v37'})
        prior_manifest = {'package_inputs': transition['previous_package_inputs'],
                          'reference': transition['previous_reference'],
                          'rules_dependencies': transition['previous_rules_dependencies']}
        self.assertEqual(hashlib.sha256((json.dumps(prior_manifest, sort_keys=True, indent=2)
                                        + '\n').encode()).hexdigest(), transition['previous_source_sha256'])
        self.assertEqual(transition['retained_fixture_reference'],
                         'cfa1ff81226c90aa9b737b50e93d3fad83455429:scripts/fixtures/rogue-earned-baseline-v5')
        self.assertEqual(set(source['package_inputs']), set(transition['previous_package_inputs']))
        for name, prior_hash in transition['previous_package_inputs'].items():
            current = (snapshot / 'package' / name).read_bytes()
            if name == 'rules.toml':
                self.assertEqual(current.count(b'interactions-v38'), 1)
                prior = current.replace(b'interactions-v38', b'interactions-v37', 1)
                current_inputs, prior_inputs = map(tomllib.loads,
                                                  (current.decode(), prior.decode()))
                self.assertEqual(current_inputs.pop('import'), 'interactions-v38')
                self.assertEqual(prior_inputs.pop('import'), 'interactions-v37')
                self.assertEqual(current_inputs, prior_inputs)
            else:
                prior = current
            self.assertEqual(hashlib.sha256(prior).hexdigest(), prior_hash, name)
        # The original tracked fixture is retained in Git and the frozen pilot
        # archive; only the current-format comparator is maintained here.
        self.assertFalse((ROOT / 'scripts/fixtures/rogue-earned-baseline-v5').exists())

    def test_retained_baseline_changes_only_required_format_and_rules_identity(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v7'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v6'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (15, 16))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v38', 'interactions-v39'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertEqual(source['rules_dependencies'],
                         {'interactions-v39': '2d3e4cde094a6b4f5d23ff29270e25ebbc3c779372b59308c5742dafff2455da'})
        self.assertEqual(transition['retained_fixture_reference'],
                         '39ef57229d76c01589fbe77b8617c1c7f4f37d0c:scripts/fixtures/rogue-earned-baseline-v6')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 15'), 1)
                self.assertEqual(current, old.replace(b'format = 15', b'format = 16', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 16)
                self.assertEqual(prior_source.pop('format'), 15)
                self.assertEqual(current_source, prior_source)
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v38'), 1)
                self.assertEqual(current, old.replace(b'interactions-v38', b'interactions-v39', 1))
            else:
                self.assertEqual(current, old, name)

    def test_retained_baseline_changes_only_required_idle_ai_scenario_format(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v8'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v7'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (16, 17))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v39', 'interactions-v39'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertEqual(source['rules_dependencies'],
                         {'interactions-v39': '2d3e4cde094a6b4f5d23ff29270e25ebbc3c779372b59308c5742dafff2455da'})
        self.assertEqual(source['rules_dependencies'], prior['rules_dependencies'])
        self.assertEqual(transition['retained_fixture_reference'],
                         '2bac3211f5052abb236e6eddc7c8b9293ccd9bd4:scripts/fixtures/rogue-earned-baseline-v7')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 16'), 1)
                self.assertEqual(current, old.replace(b'format = 16', b'format = 17', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 17)
                self.assertEqual(prior_source.pop('format'), 16)
                self.assertEqual(current_source, prior_source)
            else:
                self.assertEqual(current, old, name)

    def test_historical_baseline_retains_required_locomotion_and_hold_formats(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v9'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v8'
        self.assertTrue((snapshot / 'format-transition.json').is_file(),
                        'the current source-only baseline needs its explicit transition receipt')
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (17, 18))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v39', 'interactions-v40'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'],
                         {'interactions-v40': 'd162e6f2bd267b2f1fe65135a28f45d0f1b6048cae34abcaed3226fe8937c96c'})
        self.assertEqual(transition['retained_fixture_reference'],
                         'af8ed58c0c0e82b7271a20a08c10537ec8d91ad2:scripts/fixtures/rogue-earned-baseline-v8')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 17'), 1)
                self.assertEqual(current, old.replace(b'format = 17', b'format = 18', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 18)
                self.assertEqual(prior_source.pop('format'), 17)
                self.assertEqual(current_source, prior_source)
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v39'), 1)
                self.assertEqual(current, old.replace(b'interactions-v39', b'interactions-v40', 1))
            else:
                self.assertEqual(current, old, name)

    def test_retained_baseline_changes_only_required_theft_formats(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v10'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v9'
        self.assertTrue((snapshot / 'format-transition.json').is_file(),
                        'the current source-only baseline needs its explicit transition receipt')
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (18, 19))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v40', 'interactions-v41'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'],
                         {'interactions-v41': 'caf5a513d619f587dd6145b46f932a3d345a5ded4d99e78c85df4eb0d7f93631'})
        self.assertEqual(transition['retained_fixture_reference'],
                         '1b9a6f2de6bfe4649fd1ec08831ef188a8d077e2:scripts/fixtures/rogue-earned-baseline-v9')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 18'), 1)
                self.assertEqual(current, old.replace(b'format = 18', b'format = 19', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 19)
                self.assertEqual(prior_source.pop('format'), 18)
                self.assertEqual(current_source, prior_source)
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v40'), 1)
                self.assertEqual(current, old.replace(b'interactions-v40', b'interactions-v41', 1))
            else:
                self.assertEqual(current, old, name)

    def test_historical_baseline_changes_only_required_perception_formats(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v11'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v10'
        self.assertTrue((snapshot / 'format-transition.json').is_file(),
                        'the historical source-only baseline retains its explicit transition receipt')
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (19, 20))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v41', 'interactions-v42'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'],
                         {'interactions-v42': '0aa3e236c006bea28c3757f56f01a505ada16b4ab6aa3abb7ab9f37e8b9afabf'})
        self.assertEqual(transition['retained_fixture_reference'],
                         '2e5ac75afd1301158865d8d49c25a7696caa6e18:scripts/fixtures/rogue-earned-baseline-v10')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 19'), 1)
                self.assertEqual(current, old.replace(b'format = 19', b'format = 20', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 20)
                self.assertEqual(prior_source.pop('format'), 19)
                self.assertEqual(current_source, prior_source)
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v41'), 1)
                self.assertEqual(current, old.replace(b'interactions-v41', b'interactions-v42', 1))
            else:
                self.assertEqual(current, old, name)

    def test_historical_baseline_retains_corrosion_and_ray_transition(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v12'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v11'
        self.assertTrue((snapshot / 'format-transition.json').is_file(),
                        'the current source-only baseline needs its explicit transition receipt')
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (20, 21))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v42', 'interactions-v43'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'],
                         {'interactions-v43': 'cd62c9429fb9b024f99cb7bae341f04712b00e1e69df7e3102958aa3ff26616e'})
        self.assertEqual(transition['retained_fixture_reference'],
                         '6b23cba77a9a6765ef970bdbc363c2729b4da745:scripts/fixtures/rogue-earned-baseline-v11')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 20'), 1)
                self.assertEqual(current, old.replace(b'format = 20', b'format = 21', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 21)
                self.assertEqual(prior_source.pop('format'), 20)
                self.assertEqual(current_source, prior_source)
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v42'), 1)
                self.assertEqual(current, old.replace(b'interactions-v42', b'interactions-v43', 1))
            else:
                self.assertEqual(current, old, name)

    def test_historical_baseline_retains_magic_item_transition(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v13'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v12'
        self.assertTrue((snapshot / 'format-transition.json').is_file(),
                        'the current source-only baseline needs its explicit transition receipt')
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (21, 22))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v43', 'interactions-v44'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'],
                         {'interactions-v44': '784d7330ad9c6d1fc13cc5a8fad1c93af5d764534b6bbd3c94bbbb16a2f9bac3'})
        self.assertEqual(transition['retained_fixture_reference'],
                         '51e1f9b9ec78698de34e0ec8e1d53024cd58e182:scripts/fixtures/rogue-earned-baseline-v12')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 21'), 1)
                self.assertEqual(current, old.replace(b'format = 21', b'format = 22', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 22)
                self.assertEqual(prior_source.pop('format'), 21)
                self.assertEqual(current_source, prior_source)
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v43'), 1)
                self.assertEqual(current, old.replace(b'interactions-v43', b'interactions-v44', 1))
            else:
                self.assertEqual(current, old, name)

    def test_historical_baseline_retains_equipment_magic_transition(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v14'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v13'
        self.assertTrue((snapshot / 'format-transition.json').is_file(),
                        'the current source-only baseline needs its explicit transition receipt')
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (22, 23))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v44', 'interactions-v45'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'],
                         {'interactions-v45': '29506f6b10774102d8217d80eb9bd0f8c0f0763de3d27d4ffb9e51151188d0df'})
        self.assertEqual(transition['retained_fixture_reference'],
                         'ad28ec3633c468ecdf9020787f94e59b850b9ff8:scripts/fixtures/rogue-earned-baseline-v13')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 22'), 1)
                self.assertEqual(current, old.replace(b'format = 22', b'format = 23', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 23)
                self.assertEqual(prior_source.pop('format'), 22)
                self.assertEqual(current_source, prior_source)
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v44'), 1)
                self.assertEqual(current, old.replace(b'interactions-v44', b'interactions-v45', 1))
            else:
                self.assertEqual(current, old, name)

    def test_historical_baseline_changes_only_required_physical_projectile_formats(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v15'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v14'
        self.assertTrue((snapshot / 'format-transition.json').is_file(),
                        'the historical source-only baseline retains its explicit transition receipt')
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (23, 24))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v45', 'interactions-v46'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'],
                         {'interactions-v46': 'e3eda113c6dadf96bb350df1900c1ed08e63e0a69f049a9703be25237748e907'})
        self.assertEqual(transition['retained_fixture_reference'],
                         '124f31ab331c2c6b1511aa953c7b5ed2afb6801c:scripts/fixtures/rogue-earned-baseline-v14')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 23'), 1)
                self.assertEqual(current, old.replace(b'format = 23', b'format = 24', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 24)
                self.assertEqual(prior_source.pop('format'), 23)
                self.assertEqual(current_source, prior_source)
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v45'), 1)
                self.assertEqual(current, old.replace(b'interactions-v45', b'interactions-v46', 1))
            elif name == 'catalog/creatures.toml':
                old_anatomy = b'anatomy = { slots = [] }'
                new_anatomy = b'anatomy = { slots = [], capacities = {}, primary_attack_socket = { kind = "none" } }'
                self.assertEqual(old.count(old_anatomy), 5)
                self.assertEqual(current, old.replace(old_anatomy, new_anatomy))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                for species in prior_source['creatures']['species'].values():
                    if 'anatomy' in species:
                        species['anatomy'].update(capacities={}, primary_attack_socket={'kind': 'none'})
                self.assertEqual(current_source, prior_source)
            else:
                self.assertEqual(current, old, name)

    def test_retained_v16_baseline_changes_only_required_utility_magic_formats(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v16'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v15'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (24, 25))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v46', 'interactions-v47'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].startswith(prior['reference'] + '; '))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(transition['retained_fixture_reference'],
                         '7afacd26fe78c918c1c64f04fcfcbb72b3f154c8:scripts/fixtures/rogue-earned-baseline-v15')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 24'), 1)
                self.assertEqual(current, old.replace(b'format = 24', b'format = 25', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 25)
                self.assertEqual(prior_source.pop('format'), 24)
                self.assertEqual(current_source, prior_source)
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v46'), 1)
                self.assertEqual(current, old.replace(b'interactions-v46', b'interactions-v47', 1))
            else:
                self.assertEqual(current, old, name)

    def test_retained_v17_baseline_changes_only_required_shared_wearable_formats(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v17'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v16'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (25, 26))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v47', 'interactions-v48'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].startswith(prior['reference'] + '; '))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(transition['retained_fixture_reference'],
                         '8f2e135148e604f433ba351789e987cd39035be5:scripts/fixtures/rogue-earned-baseline-v16')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 25'), 1)
                self.assertEqual(current, old.replace(b'format = 25', b'format = 26', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 26)
                self.assertEqual(prior_source.pop('format'), 25)
                self.assertEqual(current_source, prior_source)
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v47'), 1)
                self.assertEqual(current, old.replace(b'interactions-v47', b'interactions-v48', 1))
            else:
                self.assertEqual(current, old, name)

    def test_current_baseline_changes_only_required_discovery_formats(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v18'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v17'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (26, 27))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v48', 'interactions-v49'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].startswith(prior['reference'] + '; '))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(transition['retained_fixture_reference'],
                         'e52f173fdd09cd7c244fc24bf272440f8a6d4099:scripts/fixtures/rogue-earned-baseline-v17')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 26'), 1)
                self.assertEqual(current, old.replace(b'format = 26', b'format = 27', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 27)
                self.assertEqual(prior_source.pop('format'), 26)
                self.assertEqual(current_source, prior_source)
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v48'), 1)
                self.assertEqual(current, old.replace(b'interactions-v48', b'interactions-v49', 1))
            else:
                self.assertEqual(current, old, name)

    def test_historical_hazards_baseline_preserves_format_transition(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v19'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v18'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (27, 28))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v49', 'interactions-v50'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].startswith(prior['reference'] + '; '))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'],
                         {'interactions-v50': '76a33642268cad95f0c10828eea8e5b633ba0f84802c172f308c5e64a51bcf5e'})
        self.assertEqual(transition['retained_fixture_reference'],
                         'c5fc1d2014944878a4283ebae35508e80213ae34:scripts/fixtures/rogue-earned-baseline-v18')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 27'), 1)
                self.assertEqual(current, old.replace(b'format = 27', b'format = 28', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 28)
                self.assertEqual(prior_source.pop('format'), 27)
                self.assertEqual(current_source, prior_source)
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v49'), 1)
                self.assertEqual(current, old.replace(b'interactions-v49', b'interactions-v50', 1))
            else:
                self.assertEqual(current, old, name)

    def test_retained_v20_baseline_changes_only_required_living_dungeon_formats(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v20'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v19'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (28, 29))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v50', 'interactions-v51'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].startswith(prior['reference'] + '; '))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'],
                         {'interactions-v51': '9327a0cb4a3ed1871eaacb17d8a2c6166bf861e3c3a632bf586cc133378da52d'})
        self.assertEqual(transition['retained_fixture_reference'],
                         '902d4d81f68c4ea66e6bff8d805c7fcc6fe6ebe4:scripts/fixtures/rogue-earned-baseline-v19')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 28'), 1)
                self.assertEqual(current, old.replace(b'format = 28', b'format = 29', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 29)
                self.assertEqual(prior_source.pop('format'), 28)
                self.assertEqual(current_source, prior_source)
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v50'), 1)
                self.assertEqual(current, old.replace(b'interactions-v50', b'interactions-v51', 1))
            else:
                self.assertEqual(current, old, name)

    def test_current_baseline_changes_only_required_room_omission_format(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v21'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v20'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (29, 30))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v51', 'interactions-v51'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].startswith(prior['reference'] + '; '))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertIn('No room_omissions policy', transition['only_transformation'])
        self.assertEqual(transition['current_rules_dependencies'], prior['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'], prior['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'],
                         {'interactions-v51': '9327a0cb4a3ed1871eaacb17d8a2c6166bf861e3c3a632bf586cc133378da52d'})
        self.assertEqual(transition['retained_fixture_reference'],
                         '13ea05f3109f78ef289076e5eb329a5d938eac85:scripts/fixtures/rogue-earned-baseline-v20')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 29'), 1)
                self.assertEqual(current, old.replace(b'format = 29', b'format = 30', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 30)
                self.assertEqual(prior_source.pop('format'), 29)
                self.assertEqual(current_source, prior_source)
                self.assertTrue(all('room_omissions' not in recipe
                                    for recipe in current_source['generation_recipes'].values()))
            else:
                self.assertEqual(current, old, name)

    def test_current_baseline_changes_only_required_dark_maze_format(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v22'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v21'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (30, 31))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v51', 'interactions-v51'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].startswith(prior['reference'] + '; '))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertIn('No maze_rooms or room_omissions policy', transition['only_transformation'])
        self.assertEqual(transition['current_rules_dependencies'], prior['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'], prior['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'],
                         {'interactions-v51': '9327a0cb4a3ed1871eaacb17d8a2c6166bf861e3c3a632bf586cc133378da52d'})
        self.assertEqual(transition['retained_fixture_reference'],
                         '4d8d7b2aa7aeed378a02c15ce6f2ec309ef448df:scripts/fixtures/rogue-earned-baseline-v21')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertEqual(old.count(b'format = 30'), 1)
                self.assertEqual(current, old.replace(b'format = 30', b'format = 31', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 31)
                self.assertEqual(prior_source.pop('format'), 30)
                self.assertEqual(current_source, prior_source)
                self.assertTrue(all('maze_rooms' not in recipe and 'room_omissions' not in recipe
                                    for recipe in current_source['generation_recipes'].values()))
            else:
                self.assertEqual(current, old, name)

    def test_current_baseline_changes_only_required_tactile_search_ruleset(self):
        import hashlib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v23'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v22'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (31, 31))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v51', 'interactions-v52'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].startswith(prior['reference'] + '; '))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertIn('No maze_rooms or room_omissions policy', transition['only_transformation'])
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'],
                         {'interactions-v52': '2512d044b1c4682e42286d4aa915d6c93016b6361ecd363f3e3d93f01ca1f3d9'})
        self.assertEqual(transition['retained_fixture_reference'],
                         '66930967965b5f999b2ecbe4f4926138736cb041:scripts/fixtures/rogue-earned-baseline-v22')
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v51'), 1)
                self.assertEqual(current, old.replace(b'interactions-v51', b'interactions-v52', 1))
            else:
                self.assertEqual(current, old, name)

    def test_historical_baseline_v24_retains_detection_mapping_transition(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v24'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v23'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual(transition['format'], 1)
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (31, 32))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v52', 'interactions-v53'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].startswith(prior['reference'] + '; '))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertIn('No maze_rooms or room_omissions policy', transition['only_transformation'])
        self.assertEqual(transition['retained_fixture_reference'],
                         '4e2d0e8188a8d419ad8158ba82ea580039bd9735:scripts/fixtures/rogue-earned-baseline-v23')
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'], {
            'interactions-v53': '78c83cdf72d1c3901963028d99db3e97981b584b178f44ad1110ac8afbb869ca'})
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertTrue(old.startswith(b'format = 31\n'))
                self.assertEqual(current, old.replace(b'format = 31\n', b'format = 32\n', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 32)
                self.assertEqual(prior_source.pop('format'), 31)
                self.assertEqual(current_source, prior_source)
                self.assertTrue(all('maze_rooms' not in recipe and 'room_omissions' not in recipe
                                    for recipe in current_source['generation_recipes'].values()))
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v52'), 1)
                self.assertEqual(current, old.replace(b'interactions-v52', b'interactions-v53', 1))
            else:
                self.assertEqual(current, old, name)

    def test_historical_baseline_v25_retains_polymorph_transition(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v25'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v24'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual(transition['format'], 1)
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (32, 33))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v53', 'interactions-v54'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].startswith(prior['reference'] + '; '))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertIn('No maze_rooms or room_omissions policy', transition['only_transformation'])
        self.assertEqual(transition['retained_fixture_reference'],
                         '487e541e4abada873808b472f1c99837be42ab68:scripts/fixtures/rogue-earned-baseline-v24')
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'], {
            'interactions-v54': 'f9dbd45087a583b1dcad03bd84ae08bb3c7d97cd5d2b8be0bf1c6169c2c6ad60'})
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertTrue(old.startswith(b'format = 32\n'))
                self.assertEqual(current, old.replace(b'format = 32\n', b'format = 33\n', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 33)
                self.assertEqual(prior_source.pop('format'), 32)
                self.assertEqual(current_source, prior_source)
                self.assertTrue(all('maze_rooms' not in recipe and 'room_omissions' not in recipe
                                    for recipe in current_source['generation_recipes'].values()))
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v53'), 1)
                self.assertEqual(current, old.replace(b'interactions-v53', b'interactions-v54', 1))
            else:
                self.assertEqual(current, old, name)

    def test_retained_baseline_v26_changes_only_treasure_throw_format_and_rules_identity(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v26'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v25'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual(transition['format'], 1)
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (33, 34))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v54', 'interactions-v55'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].startswith(prior['reference'] + '; '))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertIn('No maze_rooms or room_omissions policy', transition['only_transformation'])
        self.assertIn('treasure-room stages, darts, shuriken, fruit', transition['only_transformation'])
        self.assertEqual(transition['retained_fixture_reference'],
                         '3929f710ba4334b851ca3a9b635077225233dbab:scripts/fixtures/rogue-earned-baseline-v25')
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'], {
            'interactions-v55': '79f1b304407f248d6aba5e85992689c66b9702136e108a64f44fc67f1cab2807'})
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertTrue(old.startswith(b'format = 33\n'))
                self.assertEqual(current, old.replace(b'format = 33\n', b'format = 34\n', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 34)
                self.assertEqual(prior_source.pop('format'), 33)
                self.assertEqual(current_source, prior_source)
                self.assertTrue(all('maze_rooms' not in recipe and 'room_omissions' not in recipe
                                    for recipe in current_source['generation_recipes'].values()))
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v54'), 1)
                self.assertEqual(current, old.replace(b'interactions-v54', b'interactions-v55', 1))
            else:
                self.assertEqual(current, old, name)

    def test_retained_baseline_v27_changes_only_item_form_format_and_rules_identity(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v27'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v26'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual(transition['format'], 1)
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (34, 35))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v55', 'interactions-v56'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].startswith(prior['reference'] + '; '))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertIn('No maze_rooms or room_omissions policy', transition['only_transformation'])
        self.assertIn('spear throwing, item forms, staves', transition['only_transformation'])
        self.assertEqual(transition['retained_fixture_reference'],
                         'c759fb9714c5449934354d7e19668a08df7aa469:scripts/fixtures/rogue-earned-baseline-v26')
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'], {
            'interactions-v56': '6c4c3eb331a1567c50e1fed30fa59fd37f69f6d2413d829be49a0253748b5f80'})
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertTrue(old.startswith(b'format = 34\n'))
                self.assertEqual(current, old.replace(b'format = 34\n', b'format = 35\n', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 35)
                self.assertEqual(prior_source.pop('format'), 34)
                self.assertEqual(current_source, prior_source)
                self.assertTrue(all('maze_rooms' not in recipe and 'room_omissions' not in recipe
                                    for recipe in current_source['generation_recipes'].values()))
            elif name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v55'), 1)
                self.assertEqual(current, old.replace(b'interactions-v55', b'interactions-v56', 1))
            else:
                self.assertEqual(current, old, name)

    def test_current_baseline_v28_changes_only_flee_rules_identity(self):
        import hashlib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v28'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v27'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual(transition['format'], 1)
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (35, 35))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v56', 'interactions-v57'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].startswith(prior['reference'] + '; '))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertIn('No maze_rooms or room_omissions policy', transition['only_transformation'])
        self.assertEqual(transition['retained_fixture_reference'],
                         'b0b04c0c8b846323836efafdb9d85a99157ce7e4:scripts/fixtures/rogue-earned-baseline-v27')
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        bundled = ROOT / 'crates/simulation/data/default-ruleset.toml'
        self.assertEqual(source['rules_dependencies'], {
            'interactions-v57': hashlib.sha256(bundled.read_bytes()).hexdigest()})
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'rules.toml':
                self.assertEqual(old.count(b'interactions-v56'), 1)
                self.assertEqual(current, old.replace(b'interactions-v56', b'interactions-v57', 1))
            else:
                self.assertEqual(current, old, name)

    def test_current_baseline_v29_changes_only_scenario_format(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v29'
        previous = ROOT / 'scripts/fixtures/rogue-earned-baseline-v28'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        prior = json.loads((previous / 'source.json').read_text())
        self.assertEqual(transition['format'], 1)
        self.assertEqual((transition['previous_snapshot'], transition['current_snapshot']),
                         (previous.name, snapshot.name))
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (35, 36))
        self.assertEqual((transition['previous_ruleset'], transition['current_ruleset']),
                         ('interactions-v57', 'interactions-v57'))
        self.assertEqual(transition['previous_source_sha256'],
                         hashlib.sha256((previous / 'source.json').read_bytes()).hexdigest())
        for name in ('package_inputs', 'rules_dependencies', 'reference'):
            self.assertEqual(transition['previous_' + name], prior[name])
        self.assertEqual(files(previous / 'package'), prior['package_inputs'])
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertTrue(source['reference'].startswith(prior['reference'] + '; '))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertIn('compiler and route qualification pending', source['reference'])
        self.assertIn('source bytes only', transition['only_transformation'])
        self.assertIn('No maze_rooms or room_omissions policy', transition['only_transformation'])
        self.assertIn('scenario capability switches, growth schedules', transition['only_transformation'])
        self.assertEqual(transition['retained_fixture_reference'],
                         'a6ba68d8db402d119a31bb1d9a719564d232402e:scripts/fixtures/rogue-earned-baseline-v28')
        self.assertEqual(transition['current_rules_dependencies'], source['rules_dependencies'])
        self.assertEqual(source['rules_dependencies'], prior['rules_dependencies'])
        bundled = ROOT / 'crates/simulation/data/default-ruleset.toml'
        self.assertEqual(hashlib.sha256(bundled.read_bytes()).hexdigest(),
                         source['rules_dependencies']['interactions-v57'])
        self.assertEqual(set(source['package_inputs']), set(prior['package_inputs']))
        for name in source['package_inputs']:
            current = (snapshot / 'package' / name).read_bytes()
            old = (previous / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertTrue(old.startswith(b'format = 35\n'))
                self.assertEqual(current, old.replace(b'format = 35\n', b'format = 36\n', 1))
                current_source, prior_source = map(tomllib.loads, (current.decode(), old.decode()))
                self.assertEqual(current_source.pop('format'), 36)
                self.assertEqual(prior_source.pop('format'), 35)
                self.assertEqual(current_source, prior_source)
                self.assertNotIn('owner_advancement', current_source.get('capabilities', {}))
            else:
                self.assertEqual(current, old, name)
        loader = (ROOT / 'scripts/test_rogue_earned_route_process.py').read_text()
        self.assertIn("'production': ROOT / 'scripts/fixtures/rogue-earned-baseline-v29'", loader)

    def test_artifact_capacity_is_infrastructure_not_controller_failure(self):
        self.assertEqual(classify_failure(ValueError('JSON artifact exceeds bounded size')),
                         'failed_infrastructure')

    def test_soft_output_boundary_saves_without_consuming_or_overriding_terminal(self):
        from types import SimpleNamespace
        from rogue_earned_route_process import CHUNK_SOFT_BYTES
        for terminal in (None, 'death', 'earned_hd2', 'checkpoint_depth_27'):
            with self.subTest(terminal=terminal):
                route = RouteExecution.__new__(RouteExecution)
                route.check_time = Mock()
                route.controller = Mock(depth=27, commands=128)
                route.controller.choose.return_value = SimpleNamespace(
                    outcome=terminal, action={'type': 'wait'}, phase='rest')
                route.current = {'state': {'observation': {'tick': '100'}}}
                route.overview = {}
                route.report = {'checkpoints': {'27': {'already_recorded': True}}}
                route.output_budget = Mock()
                route.output_budget.used_bytes.return_value = CHUNK_SOFT_BYTES
                route.issue = Mock()
                route.run_steps(prefix_commands=256)
                self.assertEqual(route.report['outcome'], terminal or 'acceptance_prefix_saved')
                route.issue.assert_not_called()
                route.controller.consume_command.assert_not_called()

    def test_preparation_compiles_both_explicit_sources_with_one_compiler_and_rejects_drift(self):
        from arena_evaluation import files
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            sources = {}
            for arm in ('production', 'candidate'):
                source = root / arm; package = source / 'package'; package.mkdir(parents=True)
                (package / 'scenario.toml').write_text('format = 18')
                (package / 'rules.toml').write_text('import = "pinned"')
                (source / 'source.json').write_text(json.dumps({'reference': arm,
                    'package_inputs': files(package), 'rules_dependencies': {'pinned': 'hash'}}))
                sources[arm] = source
            compiler = root / 'bin/tor-scenario'
            with patch('rogue_earned_route_process.frozen_inputs', return_value={}), \
                    patch('rogue_earned_route_process.resolve_with_dependencies',
                          return_value=({}, {'pinned': 'hash'})), \
                    patch('rogue_earned_route_process.subprocess.run',
                          return_value=Mock(returncode=0, stdout='', stderr='')) as compile_package, \
                    patch('rogue_earned_route_process.subprocess.check_output', return_value='frozen'):
                inventory = prepare_campaign(root / 'campaign', root / 'bin', arm_sources=sources)
                self.assertEqual(inventory['preparation_status'], 'complete')
                self.assertEqual(len(compile_package.call_args_list), 2)
                from process_harness import SUFFIX
                self.assertTrue(all(call.args[0][0] == str(compiler) + SUFFIX
                                    for call in compile_package.call_args_list))
                self.assertTrue(all(row['evidence_status'] == 'unrun' for row in inventory['slots']))
                (sources['production'] / 'package/scenario.toml').write_text('changed')
                with self.assertRaisesRegex(ValueError, 'snapshot changed'):
                    prepare_campaign(root / 'drifted', root / 'bin', arm_sources=sources)
                failed = json.loads((root / 'drifted/inventory.json').read_text())
                self.assertEqual(failed['preparation_status'], 'failed')

    def test_hard_output_limit_is_infrastructure_failure(self):
        from process_work_budget import ProcessBudgetExceeded
        error = ProcessBudgetExceeded('transcript_bytes', 33, 32)
        self.assertEqual(classify_failure(error), 'failed_infrastructure')

    def test_transport_resume_does_not_refresh_logical_deadline(self):
        route = RouteExecution.__new__(RouteExecution)
        route.started = 100.0; route.logical_elapsed = 899.0; route.wall_seconds = 900
        route.output = Path('.'); route.output_budget = None
        with patch('rogue_earned_route_process.time.monotonic', return_value=101.0):
            with self.assertRaises(TimeoutError):
                route.check_time()


class PaidTailBudgetCallerTests(unittest.TestCase):
    """Exercise the actual settlement/censor caller, mocking transport only."""
    def route(self, root, limit):
        from collections import Counter
        from rogue_earned_route import Controller
        route = RouteExecution.__new__(RouteExecution)
        route.harness, route.output = self, root
        route.controller = Controller(1, first_limit=limit)
        route.controller.consume_command(kind='action', action={'type': 'wait'})
        route.report = {'outcome': None}
        route.commands, route.counts, route.phase = [], Counter(), 'paid_tail'
        route.player, route.check_time = Mock(), Mock()
        route.current = {'type': 'ready', 'error': None,
            'state': {'wizard_game': False, 'observation': {'ready': False,
                'combat': {'terminal': False, 'preparation_remaining': '250',
                           'preparation_active': False}}},
            'intentions': [{'intention': 'paid-identity', 'phase': 'paused'}],
            'readiness': {'resume': ['paid-identity'], 'cancel': ['paid-identity'],
                          'admission': False}}
        completed = copy.deepcopy(route.current)
        completed['state']['observation']['ready'] = True
        completed['state']['observation']['combat']['preparation_remaining'] = None
        completed['intentions'] = []
        completed['readiness'] = {'resume': [], 'cancel': [], 'admission': True}
        route.frame = Mock(return_value=completed)
        route.run_steps = lambda **kwargs: route.settle(route.current)
        route.finish_evidence = Mock()
        return route, completed

    def test_settlement_demands_late_picture_before_next_owner_query(self):
        with tempfile.TemporaryDirectory() as directory:
            route, completed = self.route(Path(directory), 2)
            picture = copy.deepcopy(completed)
            picture['state']['observation']['inventory'] = [{'name': 'token'}]
            route.frame.side_effect = [completed, picture]
            result = route.settle(route.current)
            self.assertEqual(result, picture)
            self.assertEqual(route.current, picture)
            self.assertEqual(route.controller.commands, 2)  # Observe is not gameplay.
            self.assertEqual([json.loads(call.args[0]) for call in route.player.write.call_args_list],
                             [{'type': 'resume_intention'},
                              {'type': 'request', 'request': {'type': 'observe'}}])

    def test_picture_demand_refusal_remains_a_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            route, completed = self.route(Path(directory), 2)
            rejected = {**completed, 'error': 'StaleContext: rejected'}
            route.frame.side_effect = [completed, rejected]
            with self.assertRaisesRegex(ValueError, 'Ordinary command refused.*StaleContext'):
                route.settle(route.current)
            self.assertEqual(route.player.write.call_count, 2)  # No retry.

    def test_confirmed_picture_rechecks_recovery_after_old_ready_picture(self):
        # Retained Windows CI sequence: lifecycle/readiness reach tick2700
        # while the picture still says ready at tick2650. Observe discloses
        # recovery at2700; the real ready picture arrives later at2750.
        with tempfile.TemporaryDirectory() as directory:
            route, old = self.route(Path(directory), 1)
            old['state']['observation']['tick'] = '2650'
            old['cursor'] = {'tick': '2700', 'revision': '8'}
            recovering = copy.deepcopy(old)
            recovering['state']['observation'].update(tick='2700', ready=False)
            recovering['state']['observation']['combat']['recovery_remaining'] = '50'
            done = copy.deepcopy(recovering)
            done['state']['observation'].update(tick='2750', ready=True)
            done['state']['observation']['combat']['recovery_remaining'] = '0'
            done['cursor'] = {'tick': '2750', 'revision': '11'}
            route.current = old
            calls = []

            def frame(player, predicate, seconds=15):
                self.assertIs(player, route.player)
                calls.append(seconds)
                if len(calls) == 1:
                    self.assertTrue(predicate(recovering))  # Observe's ready reply.
                    return recovering
                self.assertFalse(predicate(recovering))
                self.assertTrue(predicate(done))
                return done

            route.frame.side_effect = frame
            self.assertIs(route.settle(old), done)
            self.assertIs(route.current, done)
            self.assertEqual(len(calls), 2)
            self.assertEqual(route.controller.commands, 1)
            self.assertEqual([json.loads(call.args[0]) for call in route.player.write.call_args_list],
                             [{'type': 'request', 'request': {'type': 'observe'}}])

    def test_confirmed_terminal_picture_does_not_wait_for_ready_or_resume(self):
        with tempfile.TemporaryDirectory() as directory:
            route, completed = self.route(Path(directory), 1)
            terminal = copy.deepcopy(completed)
            terminal['state']['observation']['ready'] = False
            terminal['state']['observation']['combat']['terminal'] = True
            route.current = completed
            route.frame.return_value = terminal
            self.assertIs(route.settle(completed), terminal)
            route.frame.assert_called_once()
            self.assertEqual(route.controller.commands, 1)
            self.assertEqual([json.loads(call.args[0]) for call in route.player.write.call_args_list],
                             [{'type': 'request', 'request': {'type': 'observe'}}])

    def test_paid_tail_at_exact_budget_censors_before_any_transport_write(self):
        with tempfile.TemporaryDirectory() as directory:
            route, _ = self.route(Path(directory), 1)
            original = copy.deepcopy(route.current)
            route.run_loop()
            self.assertEqual(route.report['outcome'], 'censored_action_budget')
            self.assertEqual(route.report['unsubmitted_budget_command'], {'type': 'resume_intention'})
            self.assertEqual(route.current, original)
            self.assertEqual(route.controller.commands, 1)
            self.assertEqual(route.controller.command_kinds, {'action': 1})
            self.assertEqual(route.commands, [])
            self.assertFalse((route.output / 'commands.jsonl').exists())
            route.player.write.assert_not_called()
            route.frame.assert_not_called()
            route.finish_evidence.assert_called_once_with()

    def test_same_disclosed_tail_with_budget_submits_one_actual_resume(self):
        with tempfile.TemporaryDirectory() as directory:
            route, completed = self.route(Path(directory), 2)
            route.run_loop()
            self.assertEqual([json.loads(call.args[0]) for call in route.player.write.call_args_list],
                             [{'type': 'resume_intention'},
                              {'type': 'request', 'request': {'type': 'observe'}}])
            self.assertEqual(route.current, completed)
            self.assertEqual(route.controller.commands, 2)
            self.assertEqual(route.controller.command_kinds, {'action': 1, 'resume': 1})
            self.assertNotIn('unsubmitted_budget_command', route.report)
            self.assertNotEqual(route.report['outcome'], 'censored_action_budget')
            self.assertEqual([row['kind'] for row in route.commands], ['resume', 'read'])
            self.assertEqual(len((route.output / 'commands.jsonl').read_text().splitlines()), 2)
            route.finish_evidence.assert_called_once_with()


if __name__ == '__main__':
    unittest.main()
