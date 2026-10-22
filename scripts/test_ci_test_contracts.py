"""Independent source fixtures for objective-based PR selection; no applications run."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import ci_test_contracts as contracts


class ContractSelectionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.ids = list(contracts.SMOKE_LINUX + contracts.SMOKE_WINDOWS + contracts.SMOKE_RELEASE)
        self.ids += [
            'test_example_process.Examples.test_one',
            'test_example_process.Examples.test_new_method',
            'test_tooling.Tools.test_valid',
            'test_ruleset_sources.RulesetSources.test_real_compiler',
            'test_example_process.PureHelpers.test_windows_paths',
            'test_floor_activation_process.OmittedRoomTopologyProcess.test_dark_maze_search_traversal_and_source_free_revisit_keep_real_cells',
            'test_rogue_earned_route_process.RogueEarnedRouteProcesses.test_fixed_seed42_earned_hd_reexecutes_with_exact_source_free_restore_and_gross_flows',
        ]
        self.ids = sorted(set(self.ids))
        # Independent fixture semantics, not inferred from filename or selector.
        self.metadata = {identity: {'process': identity not in {
            'test_tooling.Tools.test_valid', 'test_example_process.PureHelpers.test_windows_paths'},
            'graphical': identity == 'test_paired_stairs_process.PairedStairsAsciiProcess.test_native_generated_pair_round_trip'}
            for identity in self.ids}
        owners = {}
        for identity in self.ids:
            module, cls, _ = identity.split('.')
            owners.setdefault(module, set()).add(cls)
            self.source(f'scripts/{module}.py', '')
        self.owner_patch = patch.object(contracts, 'REVIEWED_OWNERS', owners)
        self.owner_patch.start()
        self.addCleanup(self.owner_patch.stop)

    def source(self, name, value=''):
        target = self.root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(value, encoding='utf-8')

    def select(self, *paths, ids=None):
        selected = self.ids if ids is None else ids
        metadata = {identity: self.metadata.get(identity, {'process': False, 'graphical': False}) for identity in selected}
        return contracts.select(list(paths), selected, root=self.root, test_metadata=metadata)

    def test_changed_known_class_selects_every_current_method_not_old_snapshot(self):
        result = self.select('scripts/test_example_process.py')
        self.assertFalse(result['full_required'], result)
        self.assertIn('test_example_process.Examples.test_new_method', result['linux_debug'])
        self.assertIn('test_example_process.Examples.test_one', result['linux_debug'])

    def test_new_unowned_class_or_module_requires_full(self):
        for unknown in ('test_example_process.NewClass.test_one', 'test_new.NewClass.test_one'):
            with self.subTest(unknown=unknown):
                result = self.select('scripts/test_example_process.py', ids=self.ids + [unknown])
                self.assertTrue(result['full_required'])

    def test_missing_fixed_boundary_and_duplicate_identity_require_full(self):
        self.assertTrue(self.select('scripts/test_example_process.py', ids=self.ids[1:])['full_required'])
        self.assertTrue(self.select('scripts/test_example_process.py', ids=self.ids + self.ids[:1])['full_required'])

    def test_transitive_local_import_in_function_selects_actual_callers(self):
        self.source('scripts/shared.py')
        self.source('scripts/middle.py', 'def use():\n import shared\n')
        self.source('scripts/test_example_process.py', 'from middle import use\n')
        result = self.select('scripts/shared.py')
        self.assertFalse(result['full_required'], result)
        self.assertIn('test_example_process.Examples.test_new_method', result['linux_debug'])

    def test_runtime_script_consumer_is_selected_even_with_existing_pure_import_owner(self):
        self.source('scripts/arena_starter_study.py')
        self.source('scripts/test_tooling.py', 'import arena_starter_study\n')
        # Hand-authored fixture exercises a reviewed runtime edge, not an import.
        self.source('scripts/test_arena_starter_study_process.py', '')
        identity = 'test_arena_starter_study_process.StarterStudyProcesses.test_real_cli'
        with patch.dict(contracts.REVIEWED_OWNERS, {'test_arena_starter_study_process': ('StarterStudyProcesses',)}):
            self.metadata[identity] = {'process': True, 'graphical': False}
            result = self.select('scripts/arena_starter_study.py', ids=self.ids + [identity])
        self.assertFalse(result['full_required'], result)
        self.assertIn(identity, result['linux_debug'])
        self.assertIn(identity, result['windows_debug'])
        self.assertTrue(result['requires_apps'])

    def test_new_local_script_launch_does_not_hide_behind_existing_pure_import(self):
        self.source('scripts/new_runtime.py')
        self.source('scripts/test_tooling.py', 'import new_runtime\n')
        self.source('scripts/test_example_process.py',
                    "import subprocess, sys\ndef launch():\n subprocess.run([sys.executable, 'new_runtime.py'])\n")
        result = self.select('scripts/new_runtime.py')
        self.assertTrue(result['full_required'])
        self.assertIn('cannot resolve Python source dependencies', result['reasons'])

    def test_actual_reviewed_runtime_cli_edges_are_registered(self):
        self.assertIn('test_rogue_playable_study_process', contracts.RUNTIME_SCRIPT_EDGES['rogue_roster_study'])
        self.assertIn('test_arena_starter_study_process', contracts.RUNTIME_SCRIPT_EDGES['arena_starter_study'])
        for name in ('client', 'combat', 'item', 'physics', 'place', 'strike'):
            self.assertIn('perf_compare', contracts.RUNTIME_SCRIPT_EDGES[name + '_performance_report'])

    def test_unknown_data_deleted_helper_and_unowned_source_fail_closed(self):
        self.source('scripts/fixtures/new-data.bin')
        self.source('other/new.rs')
        for name in ('scripts/fixtures/new-data.bin', 'scripts/deleted.py', 'other/new.rs'):
            with self.subTest(path=name):
                self.assertTrue(self.select(name)['full_required'])

    def test_protocol_compatibility_does_not_claim_full_long_route_coverage(self):
        self.source('crates/protocol/src/projectile_choices.rs')
        result = self.select('crates/protocol/src/projectile_choices.rs')
        self.assertFalse(result['full_required'], result)
        self.assertTrue(result['consumer_3d'])
        self.assertNotIn('test_rogue_earned_route_process.RogueEarnedRouteProcesses.test_fixed_seed42_earned_hd_reexecutes_with_exact_source_free_restore_and_gross_flows', result['linux_debug'])
        self.assertIn('full-cadence', result['contracts'])

    def test_generation_and_growth_escalate_their_unique_routes(self):
        examples = [('crates/server/src/generation_maze.rs', 'test_floor_activation_process.OmittedRoomTopologyProcess.test_dark_maze_search_traversal_and_source_free_revisit_keep_real_cells'),
                    ('crates/simulation/src/earned_growth.rs', 'test_rogue_earned_route_process.RogueEarnedRouteProcesses.test_fixed_seed42_earned_hd_reexecutes_with_exact_source_free_restore_and_gross_flows')]
        for path, expected in examples:
            self.source(path)
            result = self.select(path)
            self.assertFalse(result['full_required'], result)
            self.assertIn(expected, result['linux_debug'])

    def test_shared_simulation_world_runtime_owns_both_routes_and_arena(self):
        for path in ('crates/simulation/src/player_advancement.rs', 'crates/simulation/src/locomotion.rs',
                     'crates/simulation/src/navigation_map.rs', 'crates/simulation/src/arena.rs',
                     'crates/world/src/lib.rs'):
            self.source(path)
            result = self.select(path)
            self.assertIn('floor-route', result['contracts'], path)
            self.assertIn('earned-route', result['contracts'], path)
            if path.startswith('crates/simulation/'):
                self.assertIn('arena', result['contracts'], path)

    def test_unreviewed_protocol_module_fails_full_instead_of_guessing_representation(self):
        self.source('crates/protocol/src/new_shape.rs')
        self.assertTrue(self.select('crates/protocol/src/new_shape.rs')['full_required'])

    def test_save_representation_selects_both_route_contracts(self):
        self.source('crates/server/src/save_schema.rs')
        result = self.select('crates/server/src/save_schema.rs')
        self.assertIn('floor-route', result['contracts'])
        self.assertIn('earned-route', result['contracts'])

    def test_scenario_only_3d_refresh_does_not_select_full_renderer(self):
        self.source('crates/client-3d/scenarios/menagerie/scenario.toml')
        result = self.select('crates/client-3d/scenarios/menagerie/scenario.toml')
        self.assertFalse(result['full_required'], result)
        self.assertFalse(result['consumer_3d'])
        self.assertIn('scenario', result['contracts'])

    def test_malformed_inventory_or_path_fails_closed(self):
        for ids in ([], ['_FailedTest.import_failure'], ['bad'], ['test_x.X.test_x']):
            self.assertTrue(self.select('scripts/test_example_process.py', ids=ids)['full_required'])
        for path in ('../escape', '/absolute', 'scripts\\test_example_process.py', 'scripts/./test_example_process.py'):
            self.assertTrue(self.select(path)['full_required'])

    def test_pure_tooling_requires_neither_compiler_nor_application_build(self):
        result = self.select('scripts/test_tooling.py')
        self.assertFalse(result['requires_rust'])
        self.assertFalse(result['requires_apps'])
        self.assertEqual(result['linux_release'], [])
        expected = ['test_example_process.PureHelpers.test_windows_paths', 'test_tooling.Tools.test_valid']
        self.assertEqual(result['linux_debug'], expected)
        self.assertEqual(result['windows_debug'], expected)

    def test_process_assertions_need_apps_but_not_rust_suite(self):
        result = self.select('scripts/test_example_process.py')
        self.assertFalse(result['requires_rust'])
        self.assertTrue(result['requires_apps'])

    def test_runtime_semantic_changes_need_rust_and_applications(self):
        self.source('crates/server/src/generation_maze.rs')
        result = self.select('crates/server/src/generation_maze.rs')
        self.assertTrue(result['requires_rust'])
        self.assertTrue(result['requires_apps'])

    def test_service_fixture_change_selects_production_host_not_only_mock_tests(self):
        self.source('scripts/service_fixture.py')
        self.source('scripts/service_contract_tests.py', 'import service_fixture\n')
        self.source('scripts/test_tooling.py', 'import service_fixture\n')
        result = self.select('scripts/service_fixture.py')
        self.assertFalse(result['full_required'], result)
        self.assertTrue(result['requires_rust'])

    def test_windows_pure_baseline_uses_discovery_kind_not_process_filename(self):
        result = self.select('scripts/test_example_process.py')
        self.assertIn('test_tooling.Tools.test_valid', result['windows_debug'])
        self.assertIn('test_example_process.PureHelpers.test_windows_paths', result['windows_debug'])

    def test_real_authoring_compiler_owner_is_not_pure_without_harness_inheritance(self):
        result = self.select('scripts/test_ruleset_sources.py')
        self.assertTrue(result['requires_apps'])
        self.assertIn('test_ruleset_sources.RulesetSources.test_real_compiler', result['windows_debug'])
        metadata = {key: dict(value) for key, value in self.metadata.items()}
        metadata['test_ruleset_sources.RulesetSources.test_real_compiler']['process'] = False
        result = contracts.select(['scripts/test_ruleset_sources.py'], self.ids,
                                  root=self.root, test_metadata=metadata)
        self.assertTrue(result['full_required'])

    def test_native_boundary_cannot_be_relabelled_as_pure(self):
        metadata = {key: dict(value) for key, value in self.metadata.items()}
        metadata[contracts.SMOKE_WINDOWS[0]] = {'process': False, 'graphical': False}
        result = contracts.select(['scripts/test_example_process.py'], self.ids,
                                  root=self.root, test_metadata=metadata)
        self.assertTrue(result['full_required'])

    def test_missing_or_inconsistent_metadata_requires_full(self):
        bad_rows = [None, {}, {key: dict(value) for key, value in self.metadata.items()}]
        bad_rows[-1][self.ids[0]]['process'] = 'false'
        for metadata in bad_rows:
            result = contracts.select(['scripts/test_example_process.py'], self.ids,
                                      root=self.root, test_metadata=metadata)
            self.assertTrue(result['full_required'])

    def test_results_are_sorted_deduplicated_and_do_not_mutate_caller(self):
        self.source('crates/protocol/src/projectile_choices.rs')
        before = list(self.ids)
        result = self.select('crates/protocol/src/projectile_choices.rs', 'scripts/test_example_process.py')
        self.assertEqual(self.ids, before)
        for key in ('linux_debug', 'windows_debug', 'linux_release', 'contracts', 'reasons'):
            self.assertEqual(result[key], sorted(set(result[key])))


if __name__ == '__main__':
    unittest.main()
