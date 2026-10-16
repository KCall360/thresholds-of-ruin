"""Cheap recipe-key contracts; no Cargo invocation or cache service."""
from contextlib import redirect_stderr, redirect_stdout
import io
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest
from unittest.mock import patch

import ci_build_key
import service_contract_ci
import verify
from test_verify import run_workflow_shell


class RecipeKeyTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "Cargo.toml").write_text('[workspace]\n[profile.dev]\nopt-level = 1\n')
        self.addCleanup(patch.stopall)
        patch.object(verify, "ROOT", self.root).start()
        patch.object(verify, "SCRIPTS", self.root / "scripts").start()

    def key(self, profile="debug", partition="rust"):
        return ci_build_key.cache_key(profile, partition)

    def test_reporting_workflow_and_verifier_prose_are_not_build_inputs(self):
        before = self.key()
        self.assertRegex(before, r"^[0-9a-f]{64}$")
        for name in (".github/workflows/ci.yml", "scripts/verify.py"):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("# reporting-only edit, no changed command construction\n")
        self.assertEqual(self.key(), before)

    def test_root_profile_changes_change_key(self):
        before = self.key()
        (self.root / "Cargo.toml").write_text('[workspace]\n[profile.dev]\nopt-level = 2\n')
        self.assertNotEqual(self.key(), before)

    def test_profiles_and_partitions_have_separate_populations(self):
        self.assertEqual(len({self.key(profile, partition) for profile in ("debug", "release")
                              for partition in ("rust", "python")}), 4)

    def test_features_packages_targets_flags_and_step_environment_change_key(self):
        before = self.key()
        original = verify.plan
        for variation in ("features", "package", "target", "flag", "environment", "cwd"):
            with self.subTest(variation=variation):
                def changed(*args, **kwargs):
                    steps = original(*args, **kwargs)
                    _, command, environment = steps[1]
                    if variation == "features": command += ["--features", "tor-server/other-feature"]
                    elif variation == "package": command[command.index("--workspace"):command.index("--workspace") + 1] = ["-p", "tor-world"]
                    elif variation == "target": command += ["--target", "aarch64-unknown-linux-gnu"]
                    elif variation == "flag": command += ["--config", "profile.dev.opt-level=3"]
                    elif variation == "environment": environment["RUSTFLAGS"] = "-C opt-level=3"
                    else: environment["_cwd"] = str(self.root / "other-workspace")
                    return steps
                with patch.object(verify, "plan", side_effect=changed):
                    self.assertNotEqual(self.key(), before)

    def test_existing_step_environment_mutation_changes_key(self):
        before = self.key()
        steps = verify.plan("full", set(), [], ci_profile="debug", ci_partition="rust")
        steps[1][2]["CARGO_PROFILE_DEV_DEBUG"] = "1"
        with patch.object(verify, "plan", return_value=steps):
            self.assertNotEqual(self.key(), before)

    def test_equivalent_working_directory_spelling_does_not_change_key(self):
        before = self.key()
        steps = verify.plan("full", set(), [], ci_profile="debug", ci_partition="rust")
        steps[1][2]["_cwd"] = "."
        with patch.object(verify, "plan", return_value=steps):
            self.assertEqual(self.key(), before)

    def test_service_command_comes_from_the_actual_builder(self):
        before = self.key()
        original = service_contract_ci.cargo_command
        with patch.object(service_contract_ci, "cargo_command", side_effect=lambda *a, **kw:
                          [*original(*a, **kw), "--no-default-features"]) as builder:
            self.assertNotEqual(self.key(), before)
        builder.assert_called_once_with("debug", workspace=True)

    def test_explicit_app_and_tool_builds_are_included(self):
        for profile, prefix in (("debug", "build-process-debug"), ("release", "build-process-release"),
                                ("debug", "build-tools")):
            with self.subTest(profile=profile, prefix=prefix):
                before = self.key(profile, "python")
                steps = verify.plan("full", set(), [], ci_profile=profile, ci_partition="python")
                index = next(i for i, step in enumerate(steps) if step[0] == prefix)
                steps[index][1].append("--no-default-features")
                with patch.object(verify, "plan", return_value=steps):
                    self.assertNotEqual(self.key(profile, "python"), before)

    def test_incidental_workspace_and_python_paths_do_not_change_key(self):
        before = self.key("debug", "python")
        root = self.root / "another-checkout"
        root.mkdir()
        (root / "Cargo.toml").write_bytes((self.root / "Cargo.toml").read_bytes())
        with patch.object(verify, "ROOT", root), patch.object(verify, "SCRIPTS", root / "scripts"), \
                patch.object(sys, "executable", str(root / "venv/python")):
            self.assertEqual(self.key("debug", "python"), before)

    def test_unknown_compositions_disable_reuse(self):
        unknown = [["cargo", "custom-builder"], ["bash", "build.sh"], ["opaque-python", "test_runner.py"],
                   [sys.executable, str(verify.SCRIPTS / "new_builder.py")],
                   [sys.executable, str(self.root / "elsewhere/test_runner.py")],
                   [sys.executable, str(verify.SCRIPTS / "service_contract_ci.py"), "--profile", "debug", "--workspace", "--new"]]
        for command in unknown:
            with self.subTest(command=command), patch.object(verify, "plan", return_value=[("unknown", command, {})]):
                with self.assertRaises(ValueError):
                    self.key()

    def test_fixed_verifier_environment_uses_same_construction_as_recipe(self):
        before = self.key()
        original = verify.verification_environment
        with patch.object(verify, "verification_environment", side_effect=lambda *a, **kw:
                          {**original(*a, **kw), "CARGO_PROFILE_DEV_DEBUG": "1"}):
            self.assertNotEqual(self.key(), before)

    def test_failed_or_unknown_recipe_emits_no_key_and_keeps_verification_available(self):
        for error in (ValueError("secret diagnostic"), OSError("missing manifest"), TypeError("unknown value")):
            with self.subTest(error=type(error).__name__), patch.object(ci_build_key, "cache_key", side_effect=error), \
                    redirect_stdout(io.StringIO()) as output, redirect_stderr(io.StringIO()) as errors:
                self.assertEqual(ci_build_key.main(["--profile", "debug", "--partition", "rust"]), 0)
            self.assertEqual(output.getvalue(), "")
            self.assertIn("Dependency cache disabled", errors.getvalue())
            self.assertNotIn("secret", errors.getvalue())

    def test_missing_root_manifest_does_not_create_a_partial_key(self):
        (self.root / "Cargo.toml").unlink()
        with redirect_stdout(io.StringIO()) as output, redirect_stderr(io.StringIO()):
            self.assertEqual(ci_build_key.main(["--profile", "debug", "--partition", "rust"]), 0)
        self.assertEqual(output.getvalue(), "")


class RecipeProcessTests(unittest.TestCase):
    def test_real_cli_emits_a_key_without_starting_cargo(self):
        environment = {**os.environ, "PATH": ""}
        result = subprocess.run([sys.executable, ci_build_key.__file__, "--profile", "debug", "--partition", "rust"],
                                env=environment, capture_output=True, text=True, timeout=10, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertRegex(result.stdout, r"^key=[0-9a-f]{64}\n$")


class WorkflowKeyTests(unittest.TestCase):
    def test_actual_shell_discards_failed_helper_output_and_continues(self):
        workflow = (Path(__file__).resolve().parents[1] / ".github/workflows/ci.yml").read_text()
        block = workflow.split("      - name: Compute dependency build recipe\n", 1)[1].split("      - name:", 1)[0]
        script = textwrap.dedent(block.split("        run: |\n", 1)[1])
        script = script.replace("${{ matrix.profile }}", "debug").replace("${{ matrix.partition }}", "rust")
        for status, output in ((0, "key=" + "a" * 64), (0, ""), (7, "key=" + "a" * 64), (127, "")):
            with self.subTest(status=status, output=bool(output)), tempfile.TemporaryDirectory() as directory:
                target = Path(directory) / "outputs"
                target.write_text("")
                fake = f"python() {{ printf '%s\\n' '{output}'; return {status}; }}\n"
                with patch.dict(os.environ, {"GITHUB_OUTPUT": target.as_posix()}):
                    result = run_workflow_shell(fake + script + "\nprintf verification-still-runs", "unused")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, "verification-still-runs")
                self.assertEqual(target.read_text().strip(), output if status == 0 else "")

    def test_workflow_skips_only_cache_when_recipe_is_unavailable(self):
        workflow = (Path(__file__).resolve().parents[1] / ".github/workflows/ci.yml").read_text()
        self.assertNotIn("hashFiles('Cargo.toml', 'scripts/verify.py', '.github/workflows/ci.yml')", workflow)
        self.assertIn("      - name: Compute dependency build recipe", workflow)
        self.assertIn("        id: build_key\n        shell: bash", workflow)
        self.assertIn("if key=$(python scripts/ci_build_key.py --profile ${{ matrix.profile }} --partition ${{ matrix.partition }}); then", workflow)
        self.assertIn("printf '%s\\n' \"$key\" >> \"$GITHUB_OUTPUT\"", workflow)
        self.assertIn("echo 'Dependency cache disabled: recipe helper failed.' >&2", workflow)
        self.assertIn("        if: steps.build_key.outcome == 'success' && steps.build_key.outputs.key != ''", workflow)
        self.assertIn("shared-key: ${{ matrix.profile }}-${{ matrix.partition }}-${{ steps.build_key.outputs.key }}", workflow)
        self.assertIn("save-if: ${{ github.ref == 'refs/heads/main' }}", workflow)
        self.assertIn("run: python scripts/verify.py full --ci-profile ${{ matrix.profile }} --ci-partition ${{ matrix.partition }}", workflow)


if __name__ == "__main__":
    unittest.main()
