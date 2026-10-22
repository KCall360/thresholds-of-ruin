"""Selected assurance keeps independent test accounting and never claims full."""
import copy
import os
import tempfile
from pathlib import Path
import unittest

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
