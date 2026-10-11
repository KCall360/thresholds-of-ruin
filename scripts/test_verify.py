"""Tests for verify.py's change selection, which must never select too little."""
from pathlib import Path
import tempfile
import json
import os
import re
import subprocess
import sys
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
from unittest.mock import patch
import unittest

import verify


def metadata():
    def package(name, deps):
        return {
            "id": name, "name": name,
            "manifest_path": f"/repo/crates/{name.removeprefix('tor-')}/Cargo.toml",
            "dependencies": [{"name": d, "kind": None} for d in deps] + [{"name": "serde", "kind": None}],
        }

    packages = [
        package("tor-world", []),
        package("tor-simulation", ["tor-world"]),
        package("tor-protocol", []),
        package("tor-client-common", ["tor-protocol"]),
        package("tor-client-ascii", ["tor-client-common", "tor-protocol"]),
        package("tor-server", ["tor-world", "tor-simulation", "tor-protocol", "tor-client-ascii"]),
    ]
    # A client is used only by server dev tests; the build edge is distinct.
    server = packages[-1]
    next(dependency for dependency in server["dependencies"]
         if dependency["name"] == "tor-client-ascii")["kind"] = "dev"
    server["dependencies"].append({"name": "tor-client-common", "kind": "build"})
    server["dependencies"].append({"name": "not-a-member", "kind": "dev"})
    members = [p["id"] for p in packages]
    packages.append(package("not-a-member", []))
    return {"workspace_root": "/repo", "workspace_members": members, "packages": packages}


class Selection(unittest.TestCase):
    def setUp(self):
        self.dirs, self.deps = verify.workspace(metadata())

    def test_workspace_keeps_only_member_dependencies_including_dev(self):
        self.assertEqual(self.dirs["tor-client-ascii"], "crates/client-ascii")
        self.assertEqual(self.deps["tor-server"], {"tor-world", "tor-simulation", "tor-protocol", "tor-client-ascii", "tor-client-common"})

    def test_dev_only_client_dependency_selects_server_tests(self):
        self.assertEqual(verify.dependents({"tor-client-ascii"}, self.deps),
                         {"tor-client-ascii", "tor-server"})
        self.assertNotIn("not-a-member", self.dirs)

    def test_protocol_document_selects_its_real_decoder_tests(self):
        direct, everything, unknown = verify.classify(["docs/reference/protocol.md"], self.dirs)
        self.assertEqual((direct, everything, unknown), ({"tor-protocol"}, False, []))
        affected = verify.dependents(direct, self.deps)
        steps = {n: c for n, c, _ in verify.plan("quick", affected, [])}
        self.assertIn("tor-protocol", steps["rust-debug"])

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
        direct, everything, unknown = verify.classify(["docs/development/testing.md", "scripts/verify.py"], self.dirs)
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

    def test_data_fixture_and_unknown_script_input_select_all_process_tests(self):
        for path in ("scripts/fixtures/perception-memory.json", "scripts/fixtures/new.json",
                     "scripts/fixtures/deleted.json", "scripts/unknown-data.dat",
                     "scripts/test_runner.py", "scripts/verify.py"):
            with self.subTest(path=path):
                self.assertEqual(self.select([path], []), verify.process_tests(self.scripts))

    def test_transitive_and_cyclic_imports_select_their_consumers(self):
        (self.scripts / "test_text_process.py").write_text("import helper_a\n", encoding="utf-8")
        (self.scripts / "helper_a.py").write_text("import helper_b\n", encoding="utf-8")
        (self.scripts / "helper_b.py").write_text("import helper_a\n", encoding="utf-8")
        self.assertEqual(self.select(["scripts/helper_b.py"], []), ["test_text_process"])

    def test_deleted_transitive_helper_still_selects_its_consumers(self):
        (self.scripts / "test_text_process.py").write_text("import helper\n", encoding="utf-8")
        (self.scripts / "helper.py").write_text("from deleted_helper import support\n", encoding="utf-8")
        self.assertEqual(self.select(["scripts/deleted_helper.py"], []), ["test_text_process"])

    def test_docs_only_change_selects_no_process_tests(self):
        self.assertEqual(self.select(["docs/development/testing.md"], []), [])


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
        self.assertFalse(verify.missing_tests(["docs/development/testing.md", "AGENTS.md"], "+text\n"))


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
            ["fmt", "clippy", "simulation-isolated", "python-debug", "architecture", "rustdoc", "rust-debug", "rust-release", "process-release"],
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
                for package in metadata()["workspace_members"]:
                    self.assertIn(package, output.getvalue())
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
                                     ["fmt", "clippy", "simulation-isolated", "python-debug", "architecture", "rustdoc", "rust-debug"])
                    for _, command, environment in push:
                        self.assertNotIn("--release", command)
                        self.assertNotEqual(environment.get("TOR_TEST_PROFILE"), "release")

    def test_quick_limits_rust_to_affected_packages(self):
        steps = {n: c for n, c, _ in verify.plan("quick", {"tor-world"}, [])}
        self.assertEqual(list(steps), ["fmt", "clippy", "architecture", "python-tools", "rust-debug"])
        self.assertIn("tor-world", steps["rust-debug"])
        self.assertNotIn("--workspace", steps["rust-debug"])

    def test_simulation_fixture_contract_has_an_isolated_debug_invocation(self):
        for tier in ("quick", "push", "full"):
            steps = {n: c for n, c, _ in verify.plan(tier, {"tor-simulation"}, [])}
            self.assertEqual(steps["simulation-isolated"], ["cargo", "test", "-p", "tor-simulation",
                             "--test", "it", "--locked", "fixture_contract::"])
        self.assertNotIn("simulation-isolated", self.names("quick", {"tor-protocol"}))
        release = verify.plan("full", set(), [], ci_profile="release")
        self.assertNotIn("simulation-isolated", [name for name, _, _ in release])

    def test_python_stages_use_the_timing_runner_without_changing_discovery(self):
        steps = {name: command for name, command, _ in verify.plan("push", set(), [])}
        self.assertIn(str(verify.SCRIPTS / "test_runner.py"), steps["python-debug"])
        self.assertIn("discover", steps["python-debug"])
        self.assertIn("test_*.py", steps["python-debug"])

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


class ExecutionEvidence(unittest.TestCase):
    def test_actual_failure_then_passing_rerun_stays_failed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "test_flake.py").write_text(
                "from pathlib import Path\nimport unittest\n"
                "class Flake(unittest.TestCase):\n"
                "    def test_once(self):\n"
                "        marker = Path('ran-once')\n"
                "        seen = marker.exists()\n"
                "        marker.touch()\n"
                "        self.assertTrue(seen, 'original failure')\n", encoding="utf-8")
            with patch.object(verify, "ROOT", root), patch.object(verify, "SCRIPTS", root):
                result = verify.run_step("flaky", [sys.executable, "-m", "unittest", "-v", "test_flake"],
                                         {}, root, 1, True)
            self.assertEqual(result["code"], 1, "A successful retry cannot erase the original failure")
            self.assertIn("rerun passed (intermittent)", result["note"])
            self.assertIn("original failure", (root / "flaky.log").read_text())
            self.assertIn("OK", (root / "flaky-rerun.log").read_text())

    def test_timed_runner_retry_keeps_distinct_timings_and_failure_artifact_paths(self):
        runner = Path(verify.__file__).with_name("test_runner.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "test_retry.py").write_text(
                "from pathlib import Path\nimport unittest,os\n"
                "class Retry(unittest.TestCase):\n"
                "    def test_once(self):\n"
                "        marker = Path('seen')\n"
                "        seen = marker.exists()\n"
                "        with marker.open('a') as out: out.write(os.environ['TOR_TEST_ARTIFACTS'] + '\\n')\n"
                "        self.assertTrue(seen, 'original failure')\n", encoding="utf-8")
            with patch.object(verify, "ROOT", root), patch.object(verify, "SCRIPTS", root):
                result = verify.run_step("retry", [sys.executable, str(runner), "-v", "test_retry"],
                                         {}, root, 1, True)
            self.assertEqual(result["code"], 1)
            self.assertIn("rerun passed (intermittent)", result["note"])
            first = [json.loads(line) for line in (root / "retry-tests.jsonl").read_text().splitlines()]
            again = [json.loads(line) for line in (root / "retry-rerun-tests.jsonl").read_text().splitlines()]
            self.assertEqual(next(row["outcome"] for row in first if row["kind"] == "test"), "failed")
            self.assertEqual(next(row["outcome"] for row in again if row["kind"] == "test"), "passed")
            self.assertEqual((root / "seen").read_text().splitlines(),
                             [str(root / "retry-process-failures"), str(root / "retry-rerun-process-failures")])

    def test_stop_and_keep_going_preserve_failure_and_summary(self):
        for keep_going in (False, True):
            with self.subTest(keep_going=keep_going), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                def run(name, *_args):
                    return {"step": name, "code": int(name == "first"), "seconds": 0,
                            "log": None, "note": ""}
                with patch.object(verify, "ROOT", root), \
                        patch.object(verify.subprocess, "run") as commands, \
                        patch.object(verify, "changes", return_value=([], "")), \
                        patch.object(verify, "free_memory_gb", return_value=8), \
                        patch.object(verify, "running_builds", return_value=[]), \
                        patch.object(verify, "plan", return_value=[(n, [n], {}) for n in ("first", "second", "third")]), \
                        patch.object(verify, "run_step", side_effect=run) as steps, redirect_stdout(StringIO()):
                    commands.return_value.stdout = json.dumps(metadata())
                    args = ["push", "--jobs", "1"] + (["--keep-going"] if keep_going else [])
                    self.assertEqual(verify.main(args), 1)
                self.assertEqual([call.args[0] for call in steps.call_args_list],
                                 ["first", "second", "third"] if keep_going else ["first"])
                summary = next((root / ".local/verify").glob("*/summary.txt")).read_text()
                self.assertIn("FAILED", summary)
                data = json.loads(next((root / ".local/verify").glob("*/summary.json")).read_text())
                self.assertFalse(data["gate_passed"])
                self.assertFalse(data["clean_passed"])
                self.assertEqual([step["code"] for step in data["steps"]],
                                 [1, 0, 0] if keep_going else [1, None, None])
                self.assertEqual([step["command"] for step in data["steps"]], [[n] for n in ("first", "second", "third")])
                self.assertEqual(len(re.findall(r"(?m)^(?:second|third)\s+not run", summary)),
                                 0 if keep_going else 2)
                self.assertFalse((root / ".local/verify.lock").exists())

    def test_machine_summary_distinguishes_a_waived_full_gate_from_clean_evidence(self):
        for waived in (False, True):
            with self.subTest(waived=waived), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                note = "WAIVED local failure: " + verify.LOCAL_MOUSE_TEST if waived else ""
                with patch.object(verify, "ROOT", root), \
                        patch.object(verify.subprocess, "run") as commands, \
                        patch.dict(verify.os.environ, {"CI":""}), \
                        patch.object(verify, "changes", return_value=([], "")), \
                        patch.object(verify, "free_memory_gb", return_value=8), \
                        patch.object(verify, "running_builds", return_value=[]), \
                        patch.object(verify, "plan", return_value=[("python-debug", ["tests"], {})]), \
                        patch.object(verify, "run_step", return_value={"step":"python-debug", "code":0,
                            "seconds":1, "log":None, "note":note}), redirect_stdout(StringIO()):
                    commands.return_value.stdout = json.dumps(metadata())
                    self.assertEqual(verify.main(["full", "--jobs", "1"] +
                                                 (["--waive-local-mouse"] if waived else [])), 0)
                evidence = json.loads(next((root / ".local/verify").glob("*/summary.json")).read_text())
                self.assertTrue(evidence["gate_passed"])
                self.assertEqual(evidence["clean_passed"], not waived)
                self.assertEqual(evidence["waived_steps"], ["python-debug"] if waived else [])
                self.assertEqual(evidence["not_run_steps"], [])
                self.assertEqual(evidence["tier"], "full")
                self.assertEqual(evidence["scope"], "local")
                self.assertIsNone(evidence["ci_profile"])
                self.assertEqual(evidence["steps"][0]["status"], "waived" if waived else "passed")

    def test_step_exception_releases_the_verification_lock(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with patch.object(verify, "ROOT", root), \
                    patch.object(verify.subprocess, "run") as commands, \
                    patch.object(verify, "changes", return_value=([], "")), \
                    patch.object(verify, "free_memory_gb", return_value=8), \
                    patch.object(verify, "running_builds", return_value=[]), \
                    patch.object(verify, "plan", return_value=[("first", ["first"], {})]), \
                    patch.object(verify, "run_step", side_effect=RuntimeError("launch failed")), \
                    redirect_stdout(StringIO()), self.assertRaisesRegex(RuntimeError, "launch failed"):
                commands.return_value.stdout = json.dumps(metadata())
                verify.main(["push", "--jobs", "1"])
            self.assertFalse((root / ".local/verify.lock").exists())
            with verify.Lock(root / ".local/verify.lock"):
                self.assertTrue((root / ".local/verify.lock").exists())


class CiWorkflowContract(unittest.TestCase):
    def job(self, name):
        workflow = (verify.ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        match = re.search(r"(?ms)^  " + re.escape(name) + r":\n(.*?)(?=^  [a-zA-Z_]+:|\Z)", workflow)
        self.assertIsNotNone(match, f"Missing required job {name}")
        return match.group(1)

    def test_complete_matrix_invokes_unwaived_full_profile_partitions(self):
        profiles = self.job("profiles")
        self.assertRegex(profiles, r"(?m)^        os: \[windows-latest, ubuntu-latest\]$")
        self.assertRegex(profiles, r"(?m)^        profile: \[debug, release\]$")
        self.assertRegex(profiles, r"(?m)^      fail-fast: false$")
        self.assertRegex(profiles, r"(?m)^        run: python scripts/verify\.py full --ci-profile \$\{\{ matrix\.profile \}\}$")
        self.assertNotIn("--waive-local-mouse", profiles)
        self.assertNotIn("continue-on-error", profiles)

    def test_required_platform_names_depend_on_every_profile_even_after_failure(self):
        gate = self.job("check")
        self.assertIn("    name: Rust (${{ matrix.os }})\n", gate)
        self.assertIn("        os: [windows-latest, ubuntu-latest]\n", gate)
        self.assertIn("    needs: profiles\n", gate)
        self.assertIn("    if: always()\n", gate)
        self.assertIn("          PROFILES_RESULT: ${{ needs.profiles.result }}\n", gate)
        self.assertNotIn("continue-on-error", gate)
        # Execute the actual Ubuntu gate, rather than a copied success predicate.
        commands = re.findall(r"(?m)^        run: (.+)$", gate)
        self.assertEqual(len(commands), 1)
        self.assertEqual(commands[0], 'test "$PROFILES_RESULT" = success')
        for outcome in ("success", "failure", "cancelled", "skipped", ""):
            with self.subTest(outcome=outcome):
                result = subprocess.run(["bash", "-c", commands[0]],
                                        env={**os.environ, "PROFILES_RESULT": outcome},
                                        capture_output=True, text=True, timeout=10)
                self.assertEqual(result.returncode == 0, outcome == "success", result.stderr)

if __name__ == "__main__":
    unittest.main()
