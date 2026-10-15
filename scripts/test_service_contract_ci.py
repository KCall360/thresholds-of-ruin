"""Cargo artifact selection and fail-closed composition, without compiling."""
from contextlib import redirect_stdout, redirect_stderr
from io import StringIO
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

import service_contract_ci as runner


class ServiceContractCargoTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.binary = Path(self.directory.name) / "host.exe"
        self.binary.write_bytes(b"test artifact")
        self.artifact = {"reason": "compiler-artifact", "target": {"name": runner.HOST_NAME,
            "kind": ["bin"], "src_path": str(runner.HOST_SOURCE),
            "required-features": [runner.HOST_FEATURE]}, "profile": {"test": False},
            "features": [runner.HOST_FEATURE], "executable": str(self.binary)}

    def test_both_profiles_use_one_cargo_test_command_and_coherent_feature(self):
        for profile in ("debug", "release"):
            command = runner.cargo_command(profile, workspace=True)
            self.assertEqual(command[:3], ["cargo", "test", "--workspace"])
            self.assertIn("tor-server/service-contract-host", command)
            self.assertIn("--message-format=json-render-diagnostics", command)
            self.assertEqual("--release" in command, profile == "release")
            self.assertNotIn("build", command)

    def test_local_package_selection_is_not_broadened(self):
        command = runner.cargo_command("debug", ["tor-world", "tor-server"])
        self.assertEqual(command[2:6], ["-p", "tor-server", "-p", "tor-world"])
        self.assertNotIn("--workspace", command)
        for packages, workspace in (([], False), (["tor-world"], False), (["tor-server"], True)):
            with self.assertRaises(ValueError):
                runner.cargo_command("debug", packages, workspace=workspace)

    def test_exact_cargo_path_is_used_even_outside_default_target(self):
        self.assertEqual(runner.select_artifact([self.artifact]), self.binary)

    def test_missing_duplicate_wrong_source_feature_and_test_artifacts_are_rejected(self):
        variants = [[], [self.artifact, self.artifact],
            [{**self.artifact, "target": {**self.artifact["target"], "src_path": "/wrong.rs"}}],
            [{**self.artifact, "features": []}],
            [{**self.artifact, "target": {**self.artifact["target"], "required-features": []}}],
            [{**self.artifact, "profile": {"test": True}}],
            [{**self.artifact, "executable": "relative-host"}],
            [{**self.artifact, "executable": str(self.binary.parent / "missing")}]]
        for rows in variants:
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                runner.select_artifact(rows)

    def execute(self, *, cargo_status=0, artifacts=None, host_status=0):
        artifacts = [self.artifact] if artifacts is None else artifacts
        process = Mock()
        process.__enter__ = Mock(return_value=process)
        process.__exit__ = Mock(return_value=False)
        process.stdout = StringIO("ordinary libtest output\n" + "\n".join(json.dumps(row) for row in artifacts))
        process.wait.return_value = cargo_status
        with patch.object(runner.subprocess, "Popen", return_value=process) as cargo, \
                patch.object(runner, "run_suite", return_value=host_status) as tests, \
                redirect_stdout(StringIO()), redirect_stderr(StringIO()):
            result = runner.run("debug", workspace=True)
        return result, cargo, tests

    def test_cargo_failure_is_preserved_without_host_run_or_rebuild(self):
        result, cargo, tests = self.execute(cargo_status=101)
        self.assertEqual(result, 101)
        cargo.assert_called_once()
        tests.assert_not_called()

    def test_missing_artifact_fails_without_build_fallback(self):
        result, cargo, tests = self.execute(artifacts=[])
        self.assertEqual(result, 1)
        cargo.assert_called_once()
        tests.assert_not_called()

    def test_host_failure_is_preserved_and_receives_exact_artifact_identity(self):
        result, cargo, tests = self.execute(host_status=7)
        self.assertEqual(result, 7)
        cargo.assert_called_once()
        tests.assert_called_once()
        self.assertEqual(tests.call_args.args[0], self.binary)
        self.assertEqual(len(tests.call_args.args[1]), 64)

    def test_success_requires_both_rust_and_service_suites(self):
        result, cargo, tests = self.execute()
        self.assertEqual(result, 0)
        cargo.assert_called_once()
        tests.assert_called_once()

    def suite(self, source=None):
        pattern = f"case_{self._testMethodName}.py"
        if source is not None:
            (Path(self.directory.name) / pattern).write_text(source, encoding="utf-8")
        with redirect_stdout(StringIO()) as output, redirect_stderr(StringIO()) as errors:
            status = runner.run_suite(self.binary, "test identity", directory=self.directory.name, pattern=pattern)
        return status, output.getvalue(), errors.getvalue()

    def test_missing_suite_fails_instead_of_green_zero(self):
        status, _, errors = self.suite()
        self.assertEqual(status, 1)
        self.assertIn("suite is empty", errors)

    def test_present_but_empty_suite_also_fails(self):
        status, _, errors = self.suite("import unittest\nclass Empty(unittest.TestCase): pass\n")
        self.assertEqual(status, 1)
        self.assertIn("suite is empty", errors)

    def test_skipped_cases_are_reported_and_do_not_count_as_passing(self):
        status, output, _ = self.suite("import unittest\nclass Cases(unittest.TestCase):\n"
            " @unittest.skip('unavailable')\n def test_contract(self): pass\n")
        self.assertEqual(status, 1)
        self.assertIn('"skipped": 1', output)
        self.assertIn('"run": 1', output)

    def test_case_count_is_dynamic_and_environment_is_restored(self):
        with patch.dict(runner.os.environ, {"TOR_SERVICE_CONTRACT_HOST": "original"}):
            status, output, _ = self.suite("import unittest\nclass Cases(unittest.TestCase):\n"
                " def test_first(self): pass\n def test_second(self): pass\n")
            self.assertEqual(runner.os.environ["TOR_SERVICE_CONTRACT_HOST"], "original")
        self.assertEqual(status, 0)
        self.assertIn('"selected": 2', output)
        self.assertIn('"run": 2', output)

    def test_assertion_failure_is_not_a_successful_gate(self):
        status, output, _ = self.suite("import unittest\nclass Cases(unittest.TestCase):\n"
            " def test_contract(self): self.fail('regression')\n")
        self.assertEqual(status, 1)
        self.assertIn('"failures": 1', output)
