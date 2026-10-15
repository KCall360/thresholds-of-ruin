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
            for name in ('rogue_earned_route.py', 'rogue_earned_route_process.py', 'process_work_budget.py'):
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
                (scripts / 'process_work_budget.py').write_text('changed budget behavior')
                reconstructed = RouteExecution(harness, output, actor=1, seed=0, package=package)
                with self.assertRaisesRegex(EvidenceError, 'Retained source/executable fingerprints changed'):
                    reconstructed.resume()
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
            route.report = {**retained, 'seed': 1}; route.harness = Mock()
            with self.assertRaisesRegex(ValueError, 'seed'):
                route.resume()
            route.harness.server.assert_not_called()
            self.assertEqual(attempt.read_bytes(), before)

    def test_frozen_baseline_fixture_has_exact_source_closure_and_original_emu_snake_stages(self):
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v5'
        manifest = json.loads((snapshot / 'source.json').read_text())
        self.assertEqual(files(snapshot / 'package'), manifest['package_inputs'])
        self.assertTrue(manifest['reference'].startswith('8d09063840c51aa4189b146c287829102856b279:'))
        source = tomllib.loads((snapshot / 'package/scenario.toml').read_text())
        stages = source['floor_activation']['profiles']['rogue.low_depth']['stages']
        self.assertEqual([stage['id'] for stage in stages[:2]], ['rogue.emus', 'rogue.snakes'])
        self.assertEqual([stage['count'] for stage in stages[:2]], [[2, 2], [2, 2]])
        self.assertEqual(set(manifest['rules_dependencies']), {'interactions-v37'})

    def test_current_baseline_changes_only_the_declared_scenario_format(self):
        import hashlib
        import tomllib
        from arena_evaluation import files
        from process_harness import ROOT
        snapshot = ROOT / 'scripts/fixtures/rogue-earned-baseline-v5'
        transition_bytes = (snapshot / 'format-transition.json').read_bytes()
        transition = json.loads(transition_bytes)
        source = json.loads((snapshot / 'source.json').read_text())
        self.assertEqual(transition['previous_snapshot'], 'rogue-earned-baseline-v4')
        self.assertEqual(transition['current_snapshot'], 'rogue-earned-baseline-v5')
        self.assertEqual((transition['previous_scenario_format'], transition['current_scenario_format']),
                         (14, 15))
        self.assertTrue(source['reference'].endswith(
            'transition-sha256:' + hashlib.sha256(transition_bytes).hexdigest()))
        self.assertEqual(files(snapshot / 'package'), source['package_inputs'])
        self.assertEqual(source['rules_dependencies'], transition['previous_rules_dependencies'])
        prior_manifest = {'package_inputs': transition['previous_package_inputs'],
                          'reference': transition['previous_reference'],
                          'rules_dependencies': transition['previous_rules_dependencies']}
        self.assertEqual(hashlib.sha256((json.dumps(prior_manifest, sort_keys=True, indent=2)
                                        + '\n').encode()).hexdigest(), transition['previous_source_sha256'])
        self.assertEqual(transition['retained_fixture_reference'],
                         '9a0e67a8d85ad065d65a41047b4be2dca5228fb7:scripts/fixtures/rogue-earned-baseline-v4')
        self.assertEqual(set(source['package_inputs']), set(transition['previous_package_inputs']))
        for name, prior_hash in transition['previous_package_inputs'].items():
            current = (snapshot / 'package' / name).read_bytes()
            if name == 'scenario.toml':
                self.assertTrue(current.startswith(b'format = 15\n'))
                prior = current.replace(b'format = 15\n', b'format = 14\n', 1)
                current_semantics, prior_semantics = map(tomllib.loads,
                                                        (current.decode(), prior.decode()))
                current_semantics.pop('format')
                prior_semantics.pop('format')
                self.assertEqual(current_semantics, prior_semantics)
            else:
                prior = current
            self.assertEqual(hashlib.sha256(prior).hexdigest(), prior_hash, name)
        # The original tracked fixture is retained in Git and the frozen pilot
        # archive; only the current-format comparator is maintained here.
        self.assertFalse((ROOT / 'scripts/fixtures/rogue-earned-baseline-v4').exists())

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
                (package / 'scenario.toml').write_text('format = 15')
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


if __name__ == '__main__':
    unittest.main()
