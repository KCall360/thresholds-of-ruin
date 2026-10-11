"""Tests for verify.py's change selection, which must never select too little."""
from pathlib import Path
import tempfile
import json
import os
import subprocess
import sys
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
from unittest.mock import patch
import unittest

import verify


def metadata():
    def package(name, deps, kind=None):
        return {
            "id": name, "name": name,
            "manifest_path": f"/repo/crates/{name.removeprefix('tor-')}/Cargo.toml",
            "dependencies": [{"name": d, "kind": kind} for d in deps] + [{"name": "serde", "kind": None}],
        }

    packages = [
        package("tor-world", []),
        package("tor-simulation", ["tor-world"]),
        package("tor-protocol", []),
        package("tor-client-common", ["tor-protocol"]),
        package("tor-client-ascii", ["tor-client-common", "tor-protocol"]),
        package("tor-server", ["tor-world", "tor-simulation", "tor-protocol", "tor-client-ascii"]),
    ]
    return {"workspace_root": "/repo", "workspace_members": [p["id"] for p in packages], "packages": packages}


class Selection(unittest.TestCase):
    def setUp(self):
        self.dirs, self.deps = verify.workspace(metadata())

    def test_workspace_keeps_only_member_dependencies_including_dev(self):
        self.assertEqual(self.dirs["tor-client-ascii"], "crates/client-ascii")
        self.assertEqual(self.deps["tor-server"], {"tor-world", "tor-simulation", "tor-protocol", "tor-client-ascii"})

    def test_changes_reach_every_reverse_dependency(self):
        self.assertEqual(verify.dependents({"tor-world"}, self.deps), {"tor-world", "tor-simulation", "tor-server"})
        self.assertEqual(
            verify.dependents({"tor-protocol"}, self.deps),
            {"tor-protocol", "tor-client-common", "tor-client-ascii", "tor-server"},
        )

    def test_shared_and_unknown_paths_select_everything(self):
        for path in ("Cargo.lock", "scenarios/two-room/package.json", "rust-toolchain.toml", "assets/new.png"):
            _, everything, _ = verify.classify([path], self.dirs)
            self.assertTrue(everything, path)
        _, _, unknown = verify.classify(["assets/new.png"], self.dirs)
        self.assertEqual(unknown, ["assets/new.png"])

    def test_docs_and_scripts_select_no_packages(self):
        direct, everything, unknown = verify.classify(["docs/testing.md", "scripts/verify.py"], self.dirs)
        self.assertEqual((direct, everything, unknown), (set(), False, []))

    def test_crate_prefix_does_not_match_a_longer_sibling(self):
        dirs = {"tor-client": "crates/client", "tor-client-ascii": "crates/client-ascii"}
        direct, _, _ = verify.classify(["crates/client-ascii/src/lib.rs"], dirs)
        self.assertEqual(direct, {"tor-client-ascii"})


class ProcessSelection(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.scripts = Path(self.temp.name)
        files = {
            "process_harness.py": "BINS = ['tor-client-text', 'tor-client-ascii', 'tor-client-headless']\n",
            "test_text_process.py": "import process_harness\nself.text_client()\n",
            "test_ascii_process.py": "import process_harness\nself.window()\n",
            "test_items_process.py": "import process_harness\nself.key(window, 'g')\n",
            "test_headless_process.py": "from process_harness import ProcessTestCase\nBIN = 'tor-client-headless'\n",
            "test_perf_ledger.py": "import json\n",
        }
        for name, text in files.items():
            (self.scripts / name).write_text(text, encoding="utf-8")

    def select(self, paths, affected, everything=False):
        return verify.select_process_tests(paths, set(affected), everything, self.scripts)

    def test_runtime_changes_select_every_process_test(self):
        everything = ["test_ascii_process", "test_headless_process", "test_items_process", "test_text_process"]
        self.assertEqual(self.select([], ["tor-server"]), everything)
        self.assertEqual(self.select([], [], everything=True), everything)

    def test_client_change_selects_tests_that_can_launch_it_through_helpers(self):
        self.assertEqual(self.select([], ["tor-client-ascii"]), ["test_ascii_process", "test_items_process"])

    def test_harness_helpers_do_not_select_every_test_for_one_client(self):
        self.assertEqual(self.select([], ["tor-client-text"]), ["test_text_process"])
        self.assertEqual(self.select([], ["tor-client-headless"]), ["test_headless_process"])

    def test_support_script_change_selects_its_importers(self):
        self.assertEqual(self.select(["scripts/test_ascii_process.py"], []), ["test_ascii_process"])
        self.assertEqual(len(self.select(["scripts/process_harness.py"], [])), 4)

    def test_docs_only_change_selects_no_process_tests(self):
        self.assertEqual(self.select(["docs/testing.md"], []), [])


class Reporting(unittest.TestCase):
    def test_python_failures_are_parsed_for_rerun(self):
        log = (
            "FAIL: test_click (test_travel_process.TravelProcesses.test_click)\n"
            "ERROR: test_save (test_dungeon_process.Dungeon.test_save)\n"
            "ok\n"
        )
        self.assertEqual(
            verify.python_failures(log),
            ["test_dungeon_process.Dungeon.test_save", "test_travel_process.TravelProcesses.test_click"],
        )

    def test_environment_hints_name_the_machine_problem(self):
        self.assertIn("lower --jobs", verify.environment_hint("memory allocation of 4096 bytes failed"))
        self.assertIsNone(verify.environment_hint("assertion failed: left == right"))

    def test_jobs_follow_free_memory_and_cpu_count(self):
        self.assertEqual(verify.jobs_for_memory(14, 12), 4)
        self.assertEqual(verify.jobs_for_memory(7, 12), 2)
        self.assertEqual(verify.jobs_for_memory(2, 12), 1)
        self.assertEqual(verify.jobs_for_memory(14, 2), 2)
        self.assertEqual(verify.jobs_for_memory(14, None), 1)


class MissingTests(unittest.TestCase):
    def test_code_change_without_tests_warns(self):
        self.assertTrue(verify.missing_tests(["crates/world/src/sight.rs"], "+fn cast() {}\n"))
        self.assertTrue(verify.missing_tests(["scripts/perf_compare.py"], "+x = 1\n"))

    def test_test_files_or_added_unit_tests_satisfy_it(self):
        self.assertFalse(verify.missing_tests(
            ["crates/world/src/sight.rs", "crates/world/tests/it/sight3d.rs"], "+fn cast() {}\n"))
        self.assertFalse(verify.missing_tests(
            ["crates/world/src/sight.rs"], "+    #[test]\n+    fn casts() {}\n"))
        self.assertFalse(verify.missing_tests(
            ["crates/server/src/lib.rs"], "+    #[tokio::test]\n+    async fn serves() {}\n"))
        self.assertFalse(verify.missing_tests(["scripts/verify.py", "scripts/test_verify.py"], ""))

    def test_docs_only_change_does_not_warn(self):
        self.assertFalse(verify.missing_tests(["docs/testing.md", "AGENTS.md"], "+text\n"))


class Tiers(unittest.TestCase):
    def test_failed_push_is_never_reported_as_complete(self):
        for code in (0, 1):
            with tempfile.TemporaryDirectory() as directory, \
                    patch.object(verify, "ROOT", Path(directory)), \
                    patch.object(verify.subprocess, "run") as commands, \
                    patch.object(verify, "changes", return_value=([], "")), \
                    patch.object(verify, "free_memory_gb", return_value=8), \
                    patch.object(verify, "running_builds", return_value=[]), \
                    patch.object(verify, "plan", return_value=[("check", ["check"], {})]), \
                    patch.object(verify, "run_step", return_value={
                        "step": "check", "code": code, "seconds": 0, "log": None, "note": ""}), \
                    redirect_stdout(StringIO()) as output:
                commands.return_value.stdout = json.dumps(metadata())
                self.assertEqual(code, verify.main(["push", "--jobs", "1"]))
                if code:
                    self.assertNotIn("Local debug push gate complete", output.getvalue())
                    self.assertIn("Local debug push gate FAILED", output.getvalue())
                else:
                    self.assertIn("Local debug push gate complete", output.getvalue())

    def names(self, tier, affected=(), process=()):
        return [name for name, _, _ in verify.plan(tier, set(affected), list(process))]

    def test_full_matches_ci(self):
        self.assertEqual(
            self.names("full", ["tor-world"], ["test_text_process"]),
            ["fmt", "clippy", "python-debug", "architecture", "rustdoc", "rust-debug", "rust-release", "process-release"],
        )

    def test_ci_profile_partitions_preserve_every_full_command_and_environment(self):
        for xvfb in [False, True]:
            full = verify.plan("full", {"tor-world"}, ["test_text_process"], xvfb=xvfb)
            debug = verify.plan("full", {"tor-world"}, [], xvfb=xvfb, ci_profile="debug")
            release = verify.plan("full", set(), [], xvfb=xvfb, ci_profile="release")
            self.assertEqual(debug + release, full)
            self.assertFalse(set(n for n, _, _ in debug) & set(n for n, _, _ in release))
            self.assertEqual([n for n, _, _ in release], ["rust-release", "process-release"])
            self.assertIn("--workspace", release[0][1])
            self.assertEqual(release[1][2], {"TOR_TEST_PROFILE": "release"})
            self.assertIn("test_*process.py", release[1][1])
            self.assertIn("test_*.py", next(c for n, c, _ in debug if n == "python-debug"))

    def test_ci_profile_cannot_partition_a_reduced_tier(self):
        for tier in ["quick", "push"]:
            for profile in ["debug", "release"]:
                with self.assertRaises(ValueError):
                    verify.plan(tier, set(), [], ci_profile=profile)
        with self.assertRaises(ValueError):
            verify.plan("full", set(), [], ci_profile="unknown")

    def test_ci_profiles_do_not_need_merge_history_to_select_complete_coverage(self):
        for profile in ["debug", "release"]:
            with self.subTest(profile=profile), \
                    patch.object(verify.subprocess, "run") as commands, \
                    patch.object(verify, "changes", side_effect=AssertionError("No merge history in CI")) as changes, \
                    patch.object(verify, "free_memory_gb", return_value=8), \
                    redirect_stdout(StringIO()) as output:
                commands.return_value.stdout = json.dumps(metadata())
                self.assertEqual(0, verify.main(["full", "--ci-profile", profile, "--dry-run"]))
                changes.assert_not_called()
                for package in metadata()["packages"]:
                    self.assertIn(package["name"], output.getvalue())
                self.assertIn("complete workspace", output.getvalue())

    def test_cli_rejects_ci_profile_on_reduced_tier_before_inspecting_workspace(self):
        with patch.object(verify.subprocess, "run") as commands, redirect_stderr(StringIO()):
            with self.assertRaises(SystemExit) as error:
                verify.main(["push", "--ci-profile", "release", "--dry-run"])
        self.assertEqual(error.exception.code, 2)
        commands.assert_not_called()

    def test_push_runs_every_debug_check_without_local_release(self):
        for xvfb in [False, True]:
            for affected, process in [(set(), []), ({"tor-world"}, ["test_text_process"])]:
                with self.subTest(xvfb=xvfb, affected=affected):
                    push = verify.plan("push", affected, process, xvfb=xvfb)
                    debug = verify.plan("full", set(), [], xvfb=xvfb, ci_profile="debug")
                    self.assertEqual(push, debug)
                    self.assertEqual([n for n, _, _ in push],
                                     ["fmt", "clippy", "python-debug", "architecture", "rustdoc", "rust-debug"])
                    for _, command, environment in push:
                        self.assertNotIn("--release", command)
                        self.assertNotEqual(environment.get("TOR_TEST_PROFILE"), "release")

    def test_quick_limits_rust_to_affected_packages(self):
        steps = {n: c for n, c, _ in verify.plan("quick", {"tor-world"}, [])}
        self.assertEqual(list(steps), ["fmt", "clippy", "architecture", "python-tools", "rust-debug"])
        self.assertIn("tor-world", steps["rust-debug"])
        self.assertNotIn("--workspace", steps["rust-debug"])

    def test_docs_only_quick_still_formats_and_runs_tool_tests(self):
        self.assertEqual(self.names("quick"), ["fmt", "architecture", "python-tools"])

    def test_tool_tests_exclude_process_tests(self):
        steps = {n: c for n, c, _ in verify.plan("quick", set(), [])}
        self.assertIn("test_verify", steps["python-tools"])
        self.assertFalse(any(arg.endswith("process") for arg in steps["python-tools"]))


class LocalMouseWaiver(unittest.TestCase):
    def test_exact_failure_is_waivable_only_with_local_opt_in(self):
        name = verify.LOCAL_MOUSE_TEST
        method = name.rsplit(".", 1)[1]
        text = f"FAIL: {method} ({name})\nRan 3 tests in 1s\nFAILED (failures=1)\n"
        command = ["python", "-m", "unittest"]
        self.assertTrue(verify.waivable_mouse_failure(1, command, text, True, False))
        for enabled, ci in [(False, False), (True, True)]:
            self.assertFalse(verify.waivable_mouse_failure(1, command, text, enabled, ci))
        self.assertFalse(verify.waivable_mouse_failure(101, command, text, True, False))
        self.assertFalse(verify.waivable_mouse_failure(1, ["cargo", "test"], text, True, False))
        mixed = text.replace("FAILED (failures=1)", "FAIL: other (test_other.Tests.other)\nFAILED (failures=2)")
        self.assertFalse(verify.waivable_mouse_failure(1, command, mixed, True, False))
        self.assertFalse(verify.waivable_mouse_failure(1, command, text.replace(name, "test_other.Tests.other"), True, False))


class LocalMouseGate(unittest.TestCase):
    def test_cli_cannot_enable_local_waiver_in_ci(self):
        with patch.dict(verify.os.environ, {"CI": "true"}), redirect_stderr(StringIO()):
            with self.assertRaises(SystemExit) as error:
                verify.main(["push", "--waive-local-mouse"])
        self.assertEqual(error.exception.code, 2)
        with patch.dict(verify.os.environ, {}, clear=True), redirect_stderr(StringIO()):
            with self.assertRaises(SystemExit) as error:
                verify.main(["full", "--ci-profile", "debug", "--waive-local-mouse"])
        self.assertEqual(error.exception.code, 2)

    def test_step_preserves_failure_and_reports_exact_local_waiver(self):
        failure = ("FAIL: test_native_underscore_and_mouse_click "
                   "(test_travel_process.TravelProcesses.test_native_underscore_and_mouse_click)\n"
                   "FAILED (failures=1)\n")
        def failed(command, **kwargs):
            kwargs["stdout"].write(failure)
            return type("Result", (), {"returncode": 1})()
        with tempfile.TemporaryDirectory() as directory, patch.object(verify.subprocess, "run", failed):
            with patch.dict(verify.os.environ, {}, clear=True):
                enabled = verify.run_step("mouse", ["python", "-m", "unittest"],
                    {"TOR_LOCAL_MOUSE_WAIVER":"1"}, Path(directory), 1, False)
                self.assertEqual(enabled["code"], 0)
                self.assertTrue(enabled["note"].startswith("WAIVED local failure:"))
                blocked = verify.run_step("mouse", ["python", "-m", "unittest"],
                    {}, Path(directory), 1, False)
                self.assertEqual(blocked["code"], 1)
            with patch.dict(verify.os.environ, {"CI":"true"}):
                ci = verify.run_step("mouse", ["python", "-m", "unittest"],
                    {"TOR_LOCAL_MOUSE_WAIVER":"1"}, Path(directory), 1, False)
                self.assertEqual(ci["code"], 1)


    def test_actual_cli_rejects_local_mouse_waiver_before_ci_runs(self):
        result = subprocess.run([sys.executable,str(Path(verify.__file__)),"full",
            "--ci-profile","debug","--waive-local-mouse"],capture_output=True,text=True)
        self.assertEqual(result.returncode,2)
        self.assertIn("local only",result.stderr)
        self.assertNotIn("packages:",result.stdout)



class CiPythonEnvironment(unittest.TestCase):
    def test_workflow_keeps_virtual_environment_interpreter_on_path(self):
        workflow = (verify.ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        prefix = '"$task_python" -c "'
        commands = [line.strip()[len(prefix):-1] for line in workflow.splitlines()
                    if line.strip().startswith(prefix) and line.strip().endswith('"')]
        commands = [command for command in commands if "GITHUB_PATH" in command]
        self.assertEqual(len(commands), 1, "Exercise the workflow's actual PATH publication command")
        local = verify.ROOT / ".local"
        local.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="ci-python-path-", dir=local) as directory:
            root = Path(directory)
            base = root / "base"
            base.mkdir()
            virtual = root / "virtual"
            virtual.mkdir()
            if os.name == "nt":
                # A directory junction models interpreter alias resolution
                # without requiring Windows symbolic-link privileges.
                link = virtual / "Scripts"
                result = subprocess.run(["cmd.exe", "/c", "mklink", "/J", str(link), str(base)],
                                        capture_output=True, text=True, timeout=10,
                                        creationflags=subprocess.CREATE_NO_WINDOW)
                self.assertEqual(result.returncode, 0, result.stderr)
                (base / "python.exe").touch()
                invocation = link / "python.exe"
            else:
                link = virtual / "bin"
                link.mkdir()
                target = base / "python"
                target.touch()
                invocation = link / "python"
                invocation.symlink_to(target)
            try:
                expected = invocation.parent.absolute()
                self.assertNotEqual(invocation.resolve().parent, expected)
                output = root / "github-path.txt"
                result = subprocess.run(
                    [sys.executable, "-c", "import sys; sys.executable = sys.argv[1]; exec(sys.argv[2])",
                     str(invocation), commands[0]],
                    env={**os.environ, "GITHUB_PATH": str(output)},
                    capture_output=True, text=True, encoding="utf-8", timeout=10,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(output.read_text(encoding="utf-8").splitlines(), [str(expected)])
            finally:
                if os.name == "nt":
                    link.rmdir()
                else:
                    invocation.unlink()

if __name__ == "__main__":
    unittest.main()
