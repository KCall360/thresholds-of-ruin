"""Contracts for focused feedback: explicit scope, real outcomes, and no gate claims."""

from contextlib import redirect_stderr, redirect_stdout
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import feedback


SUCCESS = "test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 779 filtered out; finished in 0.00s\n"


class Planning(unittest.TestCase):
    def test_selects_one_explicit_target_and_filter(self):
        args = feedback.parser().parse_args(["-p", "tor-simulation", "--test", "it", "--filter", "health::"])
        self.assertEqual(feedback.command(args), ["cargo", "test", "-p", "tor-simulation", "--test", "it",
                                                 "--locked", "--message-format=json-render-diagnostics", "--", "health::"])

    def test_library_binary_and_exact_selection(self):
        for selector in (["--lib"], ["--bin", "tor-server"]):
            args = feedback.parser().parse_args(["-p", "tor-server", *selector, "--filter", "health", "--exact"])
            actual = feedback.command(args)
            self.assertEqual(actual[4:4 + len(selector)], selector)
            self.assertEqual(actual[-3:], ["--", "health", "--exact"])
            self.assertNotIn("--workspace", actual)

    def test_filter_cannot_become_a_cargo_selection_flag(self):
        args = feedback.parser().parse_args(["-p", "tor-simulation", "--lib", "--filter=--workspace"])
        actual = feedback.command(args)
        self.assertEqual(actual[-2:], ["--", "--workspace"])
        self.assertNotIn("--workspace", actual[:actual.index("--")])

    def test_required_feature_binary_keeps_explicit_feature_identity(self):
        args = feedback.parser().parse_args([
            "-p", "tor-server", "--bin", "tor-service-contract-host", "--check",
            "--features", "service-contract-host", "--features", "another,third", "--no-default-features"])
        actual = feedback.command(args)
        self.assertEqual(actual, ["cargo", "check", "-p", "tor-server", "--bin", "tor-service-contract-host",
                                  "--locked", "--message-format=json-render-diagnostics",
                                  "--features", "service-contract-host", "--features", "another,third",
                                  "--no-default-features"])
        self.assertNotIn("--workspace", actual)
        self.assertNotIn("--all-targets", actual)

    def test_features_precede_test_runner_arguments(self):
        args = feedback.parser().parse_args(["-p", "tor-server", "--lib", "--features", "service-contract-host",
                                            "--filter", "catalog::", "--exact"])
        self.assertEqual(feedback.command(args)[-5:],
                         ["--features", "service-contract-host", "--", "catalog::", "--exact"])

    def test_preflight_checks_without_running(self):
        args = feedback.parser().parse_args(["-p", "tor-simulation", "--test", "it", "--check"])
        self.assertEqual(feedback.command(args)[1], "check")

    def test_requires_package_and_single_target(self):
        for arguments in ([], ["-p", "tor-simulation"], ["--lib"],
                          ["-p", "tor-simulation", "--lib", "--test", "it"]):
            with self.subTest(arguments=arguments), redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                feedback.parser().parse_args(arguments)

    def test_incremental_override_is_local_and_target_is_isolated(self):
        inherited = {"CARGO_INCREMENTAL": "0", "CARGO_TARGET_DIR": "/qualified", "OTHER": "kept"}
        env = feedback.environment(Path("/repo"), inherited, 2)
        self.assertEqual(env["CARGO_INCREMENTAL"], "1")
        self.assertEqual(env["CARGO_TARGET_DIR"], str(Path("/repo/.local/target-feedback")))
        self.assertEqual(env["CARGO_PROFILE_DEV_DEBUG"], "0")
        self.assertEqual(env["CARGO_BUILD_JOBS"], "2")
        self.assertEqual(env["OTHER"], "kept")
        self.assertEqual(inherited["CARGO_INCREMENTAL"], "0")
        self.assertEqual(inherited["CARGO_TARGET_DIR"], "/qualified")


    def test_deliberate_nonincremental_comparison_is_preserved(self):
        args = feedback.parser().parse_args(["-p", "tor-simulation", "--lib", "--no-incremental"])
        env = feedback.environment(Path("/repo"), {"CARGO_INCREMENTAL": "1"}, args.jobs,
                                   incremental=not args.no_incremental)
        self.assertEqual(env["CARGO_INCREMENTAL"], "0")


class Outcomes(unittest.TestCase):
    def test_reports_actual_counts_without_claiming_unselected_coverage(self):
        passed, rows, note = feedback.outcome(SUCCESS, 0)
        self.assertTrue(passed)
        self.assertEqual(rows[0]["passed"], 6)
        self.assertEqual(rows[0]["filtered_out"], 779)
        self.assertIn("779 filtered out", note)

    def test_zero_tests_failure_missing_and_multiple_summaries_never_pass(self):
        for output, code in ((SUCCESS.replace("6 passed", "0 passed"), 0), (SUCCESS, 1),
                             ("", 0), (SUCCESS + SUCCESS, 0),
                             (SUCCESS.replace("0 failed", "1 failed"), 0)):
            with self.subTest(output=output, code=code):
                self.assertFalse(feedback.outcome(output, code)[0])

    def test_check_result_explicitly_does_not_claim_behavior(self):
        passed, rows, note = feedback.outcome("", 0, check=True)
        self.assertTrue(passed)
        self.assertEqual(rows, [])
        self.assertIn("behavioral tests not run", note)
        self.assertFalse(feedback.outcome("", 1, check=True)[0])

    def test_snapshot_detects_content_changes_and_missing_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            file = root / "rule.rs"
            file.write_text("first", encoding="utf-8")
            with patch.object(feedback.subprocess, "check_output", return_value=b"rule.rs\0missing\0"):
                before = feedback.snapshot(root)
                file.write_text("second", encoding="utf-8")
                after = feedback.snapshot(root)
            self.assertNotEqual(before, after)
            self.assertEqual(before[0], {"path": "missing", "sha256": None})


class Execution(unittest.TestCase):
    def exercise(self, output=SUCCESS, code=0, changed=False, artifacts=True):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            snapshots = [[{"path": "rule.rs", "sha256": "before"}],
                         [{"path": "rule.rs", "sha256": "after" if changed else "before"}]]
            binary = root / "test-executable"
            binary.write_bytes(b"compiled test executable")
            artifact = {"reason": "compiler-artifact", "executable": str(binary),
                        "features": [], "profile": {"test": True, "opt_level": "1"},
                        "target": {"name": "it", "kind": ["test"]}}
            def execute(command, **kwargs):
                if artifacts:
                    kwargs["stdout"].write(json.dumps(artifact) + "\n")
                kwargs["stdout"].write(output)
                self.assertEqual(kwargs["env"]["CARGO_INCREMENTAL"], "1")
                return subprocess.CompletedProcess(command, code)
            with patch.object(feedback, "ROOT", root), patch.dict(os.environ, {"CI": "", "CARGO_INCREMENTAL": "0"}), \
                 patch.object(feedback, "snapshot", side_effect=snapshots), \
                 patch.object(feedback.subprocess, "check_output", return_value=b"identity"), \
                 patch.object(feedback.subprocess, "run", side_effect=execute), redirect_stdout(io.StringIO()):
                result = feedback.main(["-p", "tor-simulation", "--test", "it", "--filter", "health::"])
            receipt = json.loads(next(root.glob(".local/feedback/*/receipt.json")).read_text())
            self.assertFalse((root / ".local/verify.lock").exists())
            self.assertFalse(receipt["gate"])
            self.assertEqual(receipt["kind"], "local-feedback")
            return result, receipt

    def test_writes_nongate_receipt_and_observed_counts(self):
        code, receipt = self.exercise()
        self.assertEqual(code, 0)
        self.assertEqual(receipt["test_summaries"][0]["passed"], 6)
        self.assertTrue(receipt["inputs_unchanged"])
        self.assertEqual(receipt["environment"]["CARGO_INCREMENTAL"], "1")
        self.assertEqual(len(receipt["artifacts"][0]["sha256"]), 64)

    def test_failing_cargo_is_not_reported_as_passing(self):
        code, receipt = self.exercise(code=101)
        self.assertEqual(code, 1)
        self.assertFalse(receipt["success"])
        self.assertEqual(receipt["exit_code"], 101)

    def test_source_change_invalidates_success(self):
        code, receipt = self.exercise(changed=True)
        self.assertEqual(code, 1)
        self.assertFalse(receipt["inputs_unchanged"])
        self.assertFalse(receipt["success"])

    def test_missing_artifact_identity_cannot_pass(self):
        code, receipt = self.exercise(artifacts=False)
        self.assertEqual(code, 1)
        self.assertFalse(receipt["success"])
        self.assertIn("identifiable", receipt["note"])

    def test_zero_matched_tests_fail_the_command(self):
        code, receipt = self.exercise(output=SUCCESS.replace("6 passed", "0 passed"))
        self.assertEqual(code, 1)
        self.assertFalse(receipt["success"])

    def test_existing_verification_owner_blocks_cargo(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / ".local").mkdir()
            (root / ".local/verify.lock").write_text("owner", encoding="utf-8")
            with patch.object(feedback, "ROOT", root), patch.dict(os.environ, {"CI": ""}), \
                 patch.object(feedback.subprocess, "run") as run, redirect_stdout(io.StringIO()), self.assertRaises(SystemExit):
                feedback.main(["-p", "tor-simulation", "--lib"])
            run.assert_not_called()
            self.assertEqual((root / ".local/verify.lock").read_text(), "owner")


class ActualCommand(unittest.TestCase):
    def test_actual_entrypoint_explains_scope(self):
        result = subprocess.run([sys.executable, str(Path(feedback.__file__)), "--help"],
                                capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0)
        self.assertIn("never a verification gate", " ".join(result.stdout.split()))

    def test_actual_entrypoint_rejects_ci_before_starting_cargo(self):
        result = subprocess.run([sys.executable, str(Path(feedback.__file__)), "-p", "tor-simulation", "--lib"],
                                env={**os.environ, "CI": "true"}, capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 2)
        self.assertIn("not a CI or merge gate", result.stderr)

    def test_actual_entrypoint_rejects_invalid_scope(self):
        for extra in (["--jobs", "0"], ["--exact"], ["--check", "--filter", "health::"],
                      ["--filter=--ignored"]):
            result = subprocess.run([sys.executable, str(Path(feedback.__file__)), "-p", "tor-simulation", "--lib", *extra],
                                    env={**os.environ, "CI": ""}, capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 2)


if __name__ == "__main__":
    unittest.main()
