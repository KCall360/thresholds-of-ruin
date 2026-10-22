"""Selected assurance keeps independent test accounting and never claims full."""
import copy
from contextlib import redirect_stdout
import io
import os
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

import ci_selected as selected
import ci_test_plan as planner


class SelectedTests(unittest.TestCase):
    def rows(self):
        return [{"kind": "selection", "test_ids": ["test_fixture.Case.test_value"]},
                {"kind": "test", "test_id": "test_fixture.Case.test_value", "outcome": "passed",
                 "failures": 0, "errors": 0, "subtests_failed": 0, "subtests_skipped": 0},
                {"kind": "suite", "successful": True, "tests_run": 1, "failures": 0, "errors": 0,
                 "skipped": 0, "waived": 0, "expected_failures": 0, "unexpected_successes": 0}]

    def test_exact_positive_receipt(self):
        selected.validate_python(self.rows(), ["test_fixture.Case.test_value"])

    def test_missing_duplicate_unexpected_skipped_and_incomplete_receipts_fail(self):
        variants = []
        missing = self.rows(); missing.pop(1); variants.append(missing)
        duplicate = self.rows(); duplicate.insert(1, duplicate[1]); variants.append(duplicate)
        skipped = self.rows(); skipped[1]["outcome"] = "skipped"; variants.append(skipped)
        partial = self.rows(); partial[-1]["not_started"] = ["test_fixture.Case.test_value"]; variants.append(partial)
        subtest = self.rows(); subtest[1]["subtests_skipped"] = 1; variants.append(subtest)
        wrong = self.rows(); wrong[0]["test_ids"] = ["test_fixture.Case.test_other"]; variants.append(wrong)
        for rows in variants:
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                selected.validate_python(rows, ["test_fixture.Case.test_value"])

    def test_zero_selection_is_not_qualification(self):
        with self.assertRaises(ValueError):
            selected.validate_python(self.rows(), [])

    def test_linux_deterministic_lane_preserves_service_and_integration_wrapper(self):
        lane = {"id": "linux-debug-rust", "profile": "debug", "partition": "rust", "test_ids": []}
        steps = selected.steps_for(lane, Path("/logs"), 2)
        commands = {name: command for name, command, _ in steps}
        self.assertIn("--workspace", commands["rust-debug"])
        self.assertIn("service_contract_ci.py", str(commands["rust-debug"]))
        self.assertIn("fixture_contract::", commands["simulation-isolated"])
        self.assertNotIn("--release", commands["rust-debug"])

    def test_windows_codec_dispatch_enables_support_for_all_integration_targets(self):
        class ReachedCodecStage(Exception):
            pass
        lane = {'id': 'windows-debug-python', 'os': 'windows-latest', 'profile': 'debug',
                'partition': 'python', 'test_ids': ['test_fixture.Case.test_value']}
        seen = []
        def execute(name, command, environment, directory, jobs, rerun):
            seen.append(name)
            if name == 'platform-codecs':
                self.assertEqual(command, ['cargo', 'test', '-p', 'tor-protocol', '-p', 'tor-client-common',
                                           '--locked', '--features', 'tor-client-common/test-support'])
                self.assertEqual(environment, {})
                # No --lib, --no-run, target filter, or Python execution may
                # disguise an integration-library feature failure.
                raise ReachedCodecStage
            self.assertEqual(name, 'build-applications')
            return {'step': name, 'code': 0, 'seconds': 0, 'note': ''}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(selected, 'ROOT', root), patch.object(selected.sys, 'platform', 'win32'), \
                    patch.object(selected, 'clean_source'), patch.object(selected.verify, 'running_builds', return_value=False), \
                    patch.object(selected.verify, 'free_memory_gb', return_value=8), \
                    patch.object(selected.verify, 'run_step', side_effect=execute), redirect_stdout(io.StringIO()), \
                    self.assertRaises(ReachedCodecStage):
                selected.run({'mode': 'selected', 'lanes': [lane]}, lane['id'], root / 'logs')
        self.assertEqual(seen, ['build-applications', 'platform-codecs'])

    def test_selected_physics_build_is_an_explicit_pre_worker_prerequisite(self):
        lane = {"id": "linux-debug-python", "profile": "debug", "partition": "python", "test_ids": [
            "physics_smoke_process.PhysicsCorrectnessSmoke.test_all_shapes_preserve_actions_draw_flush_and_resume"]}
        steps = selected.steps_for(lane, Path("/logs"), 2, Path("/verified-apps"))
        self.assertEqual([name for name, _, _ in steps], ["build-examples-debug", "python-selected"])
        self.assertEqual(steps[0][1], ['cargo', 'build', '-p', 'tor-server', '--locked', '--example', 'physics_bench'])
        self.assertEqual(steps[0][2], {})

    def test_example_prerequisites_have_exact_owner_subset_and_profile(self):
        import ci_example_prerequisites as examples
        prefix = 'test_performance_process.PerformanceProcesses.'
        small = prefix + 'test_small_maintained_producers_emit_complete_valid_reports'
        streaming = prefix + 'test_streaming_benchmark_emits_valid_nested_acquisition_profiles'
        physics = 'physics_smoke_process.PhysicsCorrectnessSmoke.test_all_shapes_preserve_actions_draw_flush_and_resume'
        self.assertIsNone(examples.stage(['test_verify.Plans.test_full'], 'debug'))
        self.assertEqual(examples.stage([streaming], 'release'), ('build-examples-release',
                         ['cargo', 'build', '-p', 'tor-server', '--locked', '--example', 'latency_bench', '--release'], {}))
        command = examples.stage([small, small], 'debug')[1]
        self.assertEqual(command[5:], ['--example', 'combat_bench', '--example', 'horizon-profile',
                                      '--example', 'item_bench', '--example', 'place_bench', '--example', 'scenario_bench'])
        self.assertEqual(selected.required_stages('linux-debug-python', [physics]),
                         ['validate-applications', 'build-examples-debug', 'python-selected'])
        with self.assertRaises(ValueError): examples.stage([physics], 'relase')
        with self.assertRaises(ValueError): examples.stage(physics, 'debug')
        lane = {'id': 'linux-tooling', 'profile': 'debug', 'partition': 'tooling', 'test_ids': [physics]}
        with self.assertRaisesRegex(ValueError, 'tooling lane'):
            selected.steps_for(lane, Path('/logs'), 2)

    def test_example_owner_mapping_tracks_actual_benchmark_callers(self):
        import ast
        import ci_example_prerequisites as examples
        owners = set()
        for module in ('test_performance_process', 'physics_smoke_process'):
            tree = ast.parse((selected.ROOT / 'scripts' / (module + '.py')).read_text())
            for cls in (node for node in tree.body if isinstance(node, ast.ClassDef)):
                for method in (node for node in cls.body if isinstance(node, ast.FunctionDef) and node.name.startswith('test_')):
                    calls = [node for node in ast.walk(method) if isinstance(node, ast.Call)]
                    if any((isinstance(node.func, ast.Name) and node.func.id == 'benchmark') or
                           (isinstance(node.func, ast.Attribute) and node.func.attr == 'benchmark') for node in calls):
                        identity = f'{module}.{cls.name}.{method.name}'
                        owners.add(identity)
                        names = {node.value for node in ast.walk(method)
                                 if isinstance(node, ast.Constant) and isinstance(node.value, str)
                                 and (node.value.endswith('_bench') or node.value == 'horizon-profile')}
                        self.assertEqual(set(examples.OWNERS.get(identity, ())), names, identity)
        self.assertEqual(set(examples.OWNERS), owners)
        names = examples.stage(sorted(owners), 'debug')[1][6::2]
        self.assertEqual(names, ['combat_bench', 'horizon-profile', 'item_bench', 'latency_bench',
                                 'physics_bench', 'place_bench', 'rogue_bench', 'scenario_bench'])

    def test_selected_receipts_reject_missing_extra_and_reordered_example_stages(self):
        physics = 'physics_smoke_process.PhysicsCorrectnessSmoke.test_all_shapes_preserve_actions_draw_flush_and_resume'
        plan = {'mode': 'selected', 'policy_version': planner.POLICY_VERSION, 'source_sha': 'a' * 40,
                'source_tree': 'b' * 40, 'lanes': [{'id': 'windows-debug-python', 'test_ids': [physics]}]}
        row = {'policy_version': planner.POLICY_VERSION, 'assurance': 'selected-pr-contracts',
               'source_sha': plan['source_sha'], 'source_tree': plan['source_tree'],
               'plan_sha256': planner.digest(plan), 'run_id': os.environ.get('GITHUB_RUN_ID', ''),
               'run_attempt': os.environ.get('GITHUB_RUN_ATTEMPT', ''), 'lane': 'windows-debug-python',
               'evidence_origin': 'fresh', 'passed': True, 'test_ids': [physics],
               'steps': [{'step': name, 'code': 0} for name in
                         ['build-applications', 'platform-codecs', 'build-examples-debug', 'python-selected']]}
        selected.validate_receipts(plan, [row])
        for names in (['build-applications', 'platform-codecs', 'python-selected'],
                      ['build-applications', 'platform-codecs', 'build-examples-release', 'python-selected'],
                      ['build-applications', 'platform-codecs', 'python-selected', 'build-examples-debug'],
                      ['build-applications', 'platform-codecs', 'build-examples-debug', 'build-examples-debug', 'python-selected']):
            with self.subTest(names=names), self.assertRaisesRegex(ValueError, 'canonical lane stage'):
                selected.validate_receipts(plan, [dict(row, steps=[{'step': name, 'code': 0} for name in names])])
        broken = copy.deepcopy(row)
        broken['steps'][2]['code'] = 1
        with self.assertRaisesRegex(ValueError, 'canonical lane stage'):
            selected.validate_receipts(plan, [broken])

    def test_release_has_real_optimized_correctness_and_explicit_selected_ids(self):
        lane = {"id": "linux-release-python", "profile": "release", "partition": "python",
                "test_ids": ["test_fixture.Case.test_value"]}
        commands = {name: (command, env) for name, command, env in selected.steps_for(lane, Path("/logs"), 2)}
        self.assertIn("--release", commands["release-sentinels"][0])
        self.assertEqual(commands["python-selected"][1]["TOR_TEST_PROFILE"], "release")
        self.assertIn("--from-file", commands["python-selected"][0])

    def test_3d_consumer_checks_actual_binary_and_model_not_native_rendering(self):
        lane = {"id": "linux-3d-consumer", "profile": "debug", "partition": "consumer", "test_ids": []}
        steps = selected.steps_for(lane, Path("/logs"), 2)
        self.assertEqual([name for name, _, _ in steps], ["consumer-check", "consumer-model"])
        self.assertIn("--bin", steps[0][1])
        self.assertIn("tor-client-3d", steps[0][1])
        self.assertIn("tor-client-3d-model", steps[1][1])
        self.assertFalse(any("process" in str(command) for _, command, _ in steps))

    def test_aggregate_requires_exact_fresh_source_policy_plan_and_lane_union(self):
        plan = {"mode": "selected", "policy_version": planner.POLICY_VERSION, "source_sha": "a" * 40,
                "source_tree": "b" * 40, "lanes": [{"id": "linux-3d-consumer", "test_ids": []}]}
        receipt = {"policy_version": planner.POLICY_VERSION, "assurance": "selected-pr-contracts", "lane": "linux-3d-consumer",
                   "source_sha": plan["source_sha"], "source_tree": plan["source_tree"], "plan_sha256": planner.digest(plan),
                   "passed": True, "evidence_origin": "fresh", "run_id": os.environ.get("GITHUB_RUN_ID", ""),
                   "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT", ""), "test_ids": [], "steps": [{"step": name, "code": 0} for name in ("consumer-check", "consumer-model")]}
        selected.validate_receipts(plan, [receipt])
        for key, value in (("source_sha", "c" * 40), ("policy_version", 1), ("passed", False),
                           ("evidence_origin", "reused"), ("test_ids", ["extra"]), ("plan_sha256", "wrong")):
            invalid = copy.deepcopy(receipt); invalid[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                selected.validate_receipts(plan, [invalid])
        for receipts in ([], [receipt, receipt]):
            with self.assertRaises(ValueError):
                selected.validate_receipts(plan, receipts)

    def test_independent_stage_roles_reject_green_noops_and_missing_service_sentinels(self):
        for lane in ("linux-debug-rust", "linux-release-python", "linux-debug-python"):
            required = selected.required_stages(lane)
            self.assertTrue(required)
            plan = {"mode": "selected", "policy_version": planner.POLICY_VERSION, "source_sha": "a" * 40,
                    "source_tree": "b" * 40, "lanes": [{"id": lane, "test_ids": []}]}
            receipt = {"policy_version": planner.POLICY_VERSION, "assurance": "selected-pr-contracts", "lane": lane,
                       "source_sha": plan["source_sha"], "source_tree": plan["source_tree"], "plan_sha256": planner.digest(plan),
                       "passed": True, "evidence_origin": "fresh", "run_id": os.environ.get("GITHUB_RUN_ID", ""),
                   "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT", ""), "test_ids": [],
                       "application_record": {"file": "application-manifest.json", "bytes": 1, "sha256": "0" * 64}}
            for names in (["noop"], required[:-1], required + [required[0]], list(reversed(required))):
                with self.subTest(lane=lane, names=names), self.assertRaises(ValueError):
                    selected.validate_receipts(plan, [{**receipt, "steps": [{"step": name, "code": 0} for name in names]}])

    def test_tooling_lane_has_no_rust_binary_or_graphical_prerequisite(self):
        for lane_id in ("linux-tooling", "windows-tooling"):
            lane = {"id": lane_id, "profile": "debug", "partition": "tooling", "test_ids": ["test_fixture.Case.test_value"]}
            steps = selected.steps_for(lane, Path("/logs"), 2)
            self.assertEqual([name for name, _, _ in steps], ["python-selected"])
            self.assertNotIn("cargo", str(steps))
            self.assertNotIn("xvfb", str(steps))
            self.assertNotIn("TOR_SCENARIO_COMPILER", steps[0][2])

    def test_uploaded_log_digest_and_actual_service_result_are_independently_checked(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / "rust-debug.log"
            path.write_text("all green but no production Service result\n", encoding="utf-8")
            record = selected.binding(path)
            plan = {"lanes": [{"id": "linux-debug-rust", "partition": "rust", "profile": "debug", "test_ids": []}]}
            receipt = {"lane": "linux-debug-rust", "steps": [{"step": "rust-debug", "record": record}]}
            with self.assertRaisesRegex(ValueError, "Service"):
                selected.validate_files(plan, receipt, root)
            path.write_text("changed after receipt\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "bytes"):
                selected.bound_bytes(root, record)
            for filename in ("../escape.log", "/absolute.log", ".."):
                with self.subTest(filename=filename), self.assertRaises(ValueError):
                    selected.bound_bytes(root, {**record, "file": filename})

    def test_tooling_receipt_cannot_omit_the_no_build_audit(self):
        plan = {"mode": "selected", "policy_version": planner.POLICY_VERSION, "source_sha": "a" * 40,
                "source_tree": "b" * 40, "lanes": [{"id": "linux-tooling", "test_ids": []}]}
        receipt = {"policy_version": planner.POLICY_VERSION, "assurance": "selected-pr-contracts", "lane": "linux-tooling",
                   "source_sha": plan["source_sha"], "source_tree": plan["source_tree"], "plan_sha256": planner.digest(plan),
                   "passed": True, "evidence_origin": "fresh", "run_id": os.environ.get("GITHUB_RUN_ID", ""),
                   "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT", ""), "test_ids": [],
                   "steps": [{"step": "python-selected", "code": 0}]}
        with self.assertRaisesRegex(ValueError, "no-build"):
            selected.validate_receipts(plan, [receipt])


if __name__ == "__main__":
    unittest.main()
