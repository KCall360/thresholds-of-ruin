"""Tests for verify.py's change selection, which must never select too little."""
from pathlib import Path
import tempfile
import json
import os
import re
import shutil
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

    def test_runtime_changes_select_the_smoke_set_and_touched_tests(self):
        smoke = ("test_headless_process", "test_missing_process")
        select = lambda paths, affected, everything=False: verify.select_process_tests(
            paths, set(affected), everything, self.scripts, smoke)
        self.assertEqual(select([], ["tor-server"]), ["test_headless_process"])
        self.assertEqual(select([], [], everything=True), ["test_headless_process"])
        self.assertEqual(select(["scripts/test_text_process.py"], ["tor-server"]),
                         ["test_headless_process", "test_text_process"])
        self.assertEqual(select([], ["tor-server", "tor-client-ascii"]),
                         ["test_ascii_process", "test_headless_process", "test_items_process"])

    def test_the_summary_says_push_and_full_run_every_process_test(self):
        self.assertEqual(verify.process_summary("quick", ["a"], ["a", "b"]), "process tests: 1 of 2 (touched and smoke)")
        for tier in ("push", "full"):
            self.assertEqual(verify.process_summary(tier, ["a"], ["a", "b"]), "process tests: all 2")

    def test_the_summary_names_the_slowest_recorded_tests(self):
        with tempfile.TemporaryDirectory() as directory:
            first, second = Path(directory, "a.jsonl"), Path(directory, "b.jsonl")
            first.write_text("\n".join(json.dumps(row) for row in [
                {"kind": "test", "test_id": "m.C.test_fast", "seconds": 1.0},
                {"kind": "suite", "seconds": 99.0}]) + "\n", encoding="utf-8")
            second.write_text(json.dumps({"kind": "test", "test_id": "m.C.test_slow", "seconds": 9.0}) + "\n",
                              encoding="utf-8")
            slowest = verify.slowest_tests([first, second, Path(directory, "missing.jsonl")], count=1)
        self.assertEqual([row["test_id"] for row in slowest], ["m.C.test_slow"])

    def test_the_smoke_set_names_existing_process_tests(self):
        self.assertLessEqual(set(verify.SMOKE_PROCESS_TESTS), set(verify.process_tests(verify.SCRIPTS)))

    def test_client_change_selects_tests_that_can_launch_it_through_helpers(self):
        self.assertEqual(self.select([], ["tor-client-ascii"]), ["test_ascii_process", "test_items_process"])

    def test_harness_helpers_do_not_select_every_test_for_one_client(self):
        self.assertEqual(self.select([], ["tor-client-text"]), ["test_text_process"])
        self.assertEqual(self.select([], ["tor-client-headless"]), ["test_headless_process"])

    def test_support_script_change_selects_its_importers(self):
        self.assertEqual(self.select(["scripts/test_ascii_process.py"], []), ["test_ascii_process"])
        self.assertEqual(len(self.select(["scripts/process_harness.py"], [])), 4)

    def test_data_fixture_and_test_infrastructure_select_the_smoke_set(self):
        smoke = ("test_ascii_process", "test_text_process")
        for path in ("scripts/fixtures/perception-memory.json", "scripts/fixtures/new.json",
                     "scripts/fixtures/deleted.json", "scripts/unknown-data.dat",
                     "scripts/test_runner.py", "scripts/parallel_tests.py", "scripts/verify.py"):
            with self.subTest(path=path):
                self.assertEqual(verify.select_process_tests([path], set(), False, self.scripts, smoke), list(smoke))

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
            ["fmt", "clippy", "simulation-isolated", "build-tools", "python-debug", "architecture", "rustdoc", "rust-debug", "rust-release", "process-release"],
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
                                     ["fmt", "clippy", "simulation-isolated", "build-tools", "python-debug", "architecture", "rustdoc", "rust-debug"])
                    for _, command, environment in push:
                        self.assertNotIn("--release", command)
                        self.assertNotEqual(environment.get("TOR_TEST_PROFILE"), "release")

    def test_real_cli_debug_plan_builds_rule_compiler_before_python(self):
        result = subprocess.run([sys.executable, str(verify.SCRIPTS / "verify.py"),
                                 "full", "--ci-profile", "debug", "--dry-run"],
                                cwd=verify.ROOT, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertLess(result.stdout.index("build-tools:"), result.stdout.index("python-debug:"))
        self.assertIn("cargo build -p tor-server --bin tor-scenario --locked", result.stdout)

    def test_python_rule_tools_build_and_select_the_current_compiler(self):
        for tier, python_step in [("quick", "python-tools"), ("push", "python-debug"), ("full", "python-debug")]:
            with self.subTest(tier=tier), patch.dict(os.environ, {"CARGO_TARGET_DIR": "alternate-target"}):
                steps = verify.plan(tier, set(), [])
                names = [name for name, _, _ in steps]
                self.assertLess(names.index("build-tools"), names.index(python_step))
                build = next(command for name, command, _ in steps if name == "build-tools")
                self.assertEqual(build, ["cargo", "build", "-p", "tor-server", "--bin", "tor-scenario", "--locked"])
                environment = next(environment for name, _, environment in steps if name == python_step)
                self.assertEqual(Path(environment["TOR_SCENARIO_COMPILER"]), verify.ROOT / "alternate-target/debug" /
                                 ("tor-scenario.exe" if os.name == "nt" else "tor-scenario"))

    def test_quick_limits_rust_to_affected_packages(self):
        steps = {n: c for n, c, _ in verify.plan("quick", {"tor-world"}, [])}
        self.assertEqual(list(steps), ["fmt", "clippy", "architecture", "build-tools", "python-tools", "rust-debug"])
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
        self.assertEqual(self.names("quick"), ["fmt", "architecture", "build-tools", "python-tools"])

    def test_tool_tests_exclude_process_tests(self):
        steps = {n: c for n, c, _ in verify.plan("quick", set(), [])}
        self.assertIn("test_verify", steps["python-tools"])
        self.assertFalse(any(arg.endswith("process") for arg in steps["python-tools"]))


class CiPartitions(unittest.TestCase):
    def test_partition_union_preserves_every_profile_command_and_environment_exactly_once(self):
        ownership = {
            "debug": {"rust": ["fmt", "clippy", "simulation-isolated", "architecture", "rustdoc", "rust-debug"],
                      "python": ["build-tools", "python-debug"]},
            "release": {"rust": ["rust-release"], "python": ["process-release"]},
        }
        for profile in ("debug", "release"):
            for xvfb in (False, True):
                with self.subTest(profile=profile, xvfb=xvfb):
                    complete = verify.plan("full", set(), [], xvfb=xvfb, ci_profile=profile)
                    selected = []
                    for partition in ("rust", "python"):
                        steps = verify.plan("full", {"tor-world"}, ["test_text_process"],
                                            xvfb=xvfb, ci_profile=profile, ci_partition=partition)
                        prerequisites = [step for step in steps if step[0] == f"build-process-{profile}"]
                        self.assertEqual(len(prerequisites), int(partition == "python"))
                        original = [step for step in steps if step not in prerequisites]
                        self.assertEqual([name for name, _, _ in original], ownership[profile][partition])
                        self.assertEqual(original, [step for step in complete if step[0] in ownership[profile][partition]])
                        selected.extend(original)
                    self.assertCountEqual(selected, complete)
                    self.assertEqual(len({name for name, _, _ in selected}), len(complete))

    def test_no_partition_keeps_local_tiers_and_complete_profiles_unchanged(self):
        for tier in ("quick", "push", "full"):
            profiles = (None, "debug", "release") if tier == "full" else (None,)
            for profile in profiles:
                with self.subTest(tier=tier, profile=profile):
                    self.assertEqual(verify.plan(tier, {"tor-simulation"}, ["test_text_process"], ci_profile=profile),
                                     verify.plan(tier, {"tor-simulation"}, ["test_text_process"],
                                                 ci_profile=profile, ci_partition=None))

    def test_python_partitions_explicitly_build_all_apps_and_compiler_before_discovery(self):
        for profile in ("debug", "release"):
            for xvfb in (False, True):
                with self.subTest(profile=profile, xvfb=xvfb):
                    steps = verify.plan("full", set(), [], xvfb=xvfb,
                                        ci_profile=profile, ci_partition="python")
                    names = [name for name, _, _ in steps]
                    build_name = f"build-process-{profile}"
                    test_name = "python-debug" if profile == "debug" else "process-release"
                    self.assertLess(names.index(build_name), names.index(test_name))
                    build = next(step for step in steps if step[0] == build_name)
                    self.assertEqual(build, (build_name, ["cargo", "build", "--workspace", "--bins", "--locked"]
                                             + (["--release"] if profile == "release" else []), {}))
                    command = next(command for name, command, _ in steps if name == test_name)
                    prefix = (["xvfb-run", "-a", "-s", "-screen 0 1280x1024x24"] if xvfb else [])
                    self.assertEqual(command[:len(prefix) + 2],
                                     [*prefix, sys.executable, str(verify.SCRIPTS / "test_runner.py")])

    def test_future_unclassified_stages_fail_closed_in_both_partitions(self):
        for name in ("new-check", "rust-new-target", "python-new-suite"):
            for partition in ("rust", "python"):
                with self.subTest(name=name, partition=partition):
                    with self.assertRaisesRegex(ValueError, "Unclassified CI stage.*" + name):
                        verify.ci_partition_steps([(name, ["check"], {})], "debug", partition)

    def test_partition_requires_full_and_a_valid_explicit_profile(self):
        invalid = [(tier, profile, partition)
                   for tier in ("quick", "push", "full")
                   for profile in (None, "debug", "release", "unknown")
                   for partition in ("rust", "python", "unknown")
                   if tier != "full" or profile not in ("debug", "release") or partition == "unknown"]
        for tier, profile, partition in invalid:
            with self.subTest(tier=tier, profile=profile, partition=partition):
                with self.assertRaises(ValueError):
                    verify.plan(tier, set(), [], ci_profile=profile, ci_partition=partition)

    def test_cli_rejects_invalid_partition_selectors_before_any_workspace_command(self):
        cases = [["full", "--ci-partition", "rust"],
                 ["push", "--ci-profile", "debug", "--ci-partition", "rust"],
                 ["quick", "--ci-profile", "release", "--ci-partition", "python"],
                 ["full", "--ci-profile", "debug", "--ci-partition", "unknown"]]
        for arguments in cases:
            with self.subTest(arguments=arguments), patch.object(verify.subprocess, "run") as commands, \
                    redirect_stderr(StringIO()) as errors:
                with self.assertRaises(SystemExit) as error:
                    verify.main([*arguments, "--dry-run"])
                self.assertEqual(error.exception.code, 2)
                self.assertIn("--ci-partition" if arguments[0] == "full" else "full tier", errors.getvalue())
                commands.assert_not_called()

    def test_partitions_select_complete_coverage_without_merge_history_or_a_rust_display(self):
        for profile in ("debug", "release"):
            for partition in ("rust", "python"):
                with self.subTest(profile=profile, partition=partition), \
                        patch.object(verify.subprocess, "run") as commands, \
                        patch.object(verify, "changes", side_effect=AssertionError("No merge history in CI")) as changes, \
                        patch.object(verify, "plan", wraps=verify.plan) as planner, \
                        patch.object(verify, "free_memory_gb", return_value=8), \
                        patch.object(verify.sys, "platform", "linux"), \
                        patch.dict(os.environ, {}, clear=True), redirect_stdout(StringIO()) as output:
                    commands.return_value.stdout = json.dumps(metadata())
                    self.assertEqual(verify.main(["full", "--ci-profile", profile,
                                                  "--ci-partition", partition, "--dry-run"]), 0)
                    changes.assert_not_called()
                    self.assertEqual(planner.call_args.kwargs["xvfb"], partition == "python")
                    self.assertEqual(planner.call_args.kwargs["ci_partition"], partition)
                    self.assertIn(f"full-ci-{profile}-{partition}; complete workspace", output.getvalue())
                    for package in metadata()["workspace_members"]:
                        self.assertIn(package, output.getvalue())

    def test_ci_partitions_reject_both_local_waivers_before_inspection(self):
        for profile in ("debug", "release"):
            for partition in ("rust", "python"):
                for waiver in (["--waive-local-mouse"], ["--waive-local-graphical", "missing.json"]):
                    with self.subTest(profile=profile, partition=partition, waiver=waiver), \
                            patch.dict(os.environ, {}, clear=True), \
                            patch.object(verify.subprocess, "run") as commands, \
                            patch.object(verify.graphical_waiver, "read_manifest") as manifest, \
                            redirect_stderr(StringIO()) as errors:
                        with self.assertRaises(SystemExit) as error:
                            verify.main(["full", "--ci-profile", profile, "--ci-partition", partition, *waiver])
                        self.assertEqual(error.exception.code, 2)
                        self.assertIn("local only", errors.getvalue())
                        commands.assert_not_called()
                        manifest.assert_not_called()

    def test_actual_cli_partitions_are_standalone_complete_plans(self):
        for profile in ("debug", "release"):
            for partition in ("rust", "python"):
                with self.subTest(profile=profile, partition=partition):
                    result = subprocess.run([sys.executable, str(verify.SCRIPTS / "verify.py"), "full",
                                             "--ci-profile", profile, "--ci-partition", partition,
                                             "--base", "refs/missing-ci-partition-history", "--dry-run"],
                                            cwd=verify.ROOT, capture_output=True, text=True, timeout=30)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn("complete workspace", result.stdout)
                    if partition == "python":
                        self.assertIn(f"build-process-{profile}:", result.stdout)
                        self.assertIn("test_*.py" if profile == "debug" else "test_*process.py", result.stdout)
                    else:
                        self.assertIn(f"rust-{profile}: cargo test --workspace", result.stdout)
                        self.assertNotIn("xvfb-run", result.stdout)

    def test_actual_cli_rejects_partition_without_profile_before_workspace_inspection(self):
        result = subprocess.run([sys.executable, str(verify.SCRIPTS / "verify.py"),
                                 "full", "--ci-partition", "rust", "--dry-run"],
                                cwd=verify.ROOT, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 2, result.stdout)
        self.assertIn("--ci-partition requires full and --ci-profile", result.stderr)
        self.assertNotIn("packages:", result.stdout)

    def test_logs_and_machine_evidence_identify_every_profile_partition(self):
        def passed(name, command, env, logs, jobs, rerun):
            return {"step": name, "code": 0, "seconds": 0, "log": None, "note": ""}

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for profile in ("debug", "release"):
                for partition in ("rust", "python"):
                    with self.subTest(profile=profile, partition=partition), \
                            patch.object(verify, "ROOT", root), \
                            patch.object(verify.subprocess, "run") as commands, \
                            patch.object(verify, "changes", side_effect=AssertionError("No merge history in CI")), \
                            patch.object(verify, "free_memory_gb", return_value=8), \
                            patch.object(verify, "running_builds", return_value=[]), \
                            patch.object(verify, "run_step", side_effect=passed), \
                            redirect_stdout(StringIO()) as output:
                        commands.return_value.stdout = json.dumps(metadata())
                        self.assertEqual(verify.main(["full", "--ci-profile", profile,
                                                      "--ci-partition", partition, "--jobs", "1"]), 0)
                        paths = list((root / ".local/verify").glob(f"*-full-ci-{profile}-{partition}/summary.json"))
                        self.assertEqual(len(paths), 1)
                        evidence = json.loads(paths[0].read_text(encoding="utf-8"))
                        self.assertEqual((evidence["ci_profile"], evidence["ci_partition"]), (profile, partition))
                        self.assertEqual(evidence["scope"], "ci-partition")
                        self.assertTrue(evidence["clean_passed"])
                        self.assertIn(f"CI {profile}/{partition} partition only", output.getvalue())
                        self.assertIn("not a full run or a merge gate", output.getvalue())
            self.assertEqual(len(list((root / ".local/verify").glob("*/summary.json"))), 4)


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


def workflow_bash():
    """Use native Git's Bash on Windows, never an unrelated PATH/WSL alias."""
    if sys.platform == "win32":
        executable = shutil.which("git")
        candidates = []
        if executable:
            git = Path(executable).resolve()
            # Git for Windows exposes cmd/git.exe or bin/git.exe; its native
            # installation can also expose mingw64/bin/git.exe directly.
            for root in (git.parent.parent, git.parent.parent.parent):
                candidates.extend(root / relative for relative in ("bin/bash.exe", "usr/bin/bash.exe"))
            for candidate in candidates:
                if candidate.is_file():
                    return str(candidate)
        raise FileNotFoundError(f"Git for Windows Bash is required; git={executable!r}; "
                                f"checked={[str(path) for path in candidates]!r}")
    executable = shutil.which("bash")
    if executable is None:
        raise FileNotFoundError("Bash is required to execute the checked-in Ubuntu CI gate")
    return executable


def run_workflow_shell(script, outcome):
    command = [workflow_bash(), "--noprofile", "--norc", "-c", script]
    try:
        return subprocess.run(command, env={**os.environ, "PROFILES_RESULT": outcome},
                              capture_output=True, text=True, timeout=10)
    except subprocess.TimeoutExpired as error:
        raise AssertionError(f"command={command!r}; outcome={outcome!r}; exit=timeout; "
                             f"stdout={error.stdout!r}; stderr={error.stderr!r}") from error
    except OSError as error:
        raise AssertionError(f"command={command!r}; outcome={outcome!r}; exit=not-started; "
                             f"stdout=None; stderr={str(error)!r}") from error


def shell_diagnostic(result, outcome):
    return (f"command={result.args!r}; outcome={outcome!r}; exit={result.returncode}; "
            f"stdout={result.stdout!r}; stderr={result.stderr!r}")


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
        self.assertRegex(profiles, r"(?m)^        partition: \[rust, python\]$")
        self.assertRegex(profiles, r"(?m)^      fail-fast: false$")
        self.assertRegex(profiles, r"(?m)^        run: python scripts/verify\.py full --ci-profile \$\{\{ matrix\.profile \}\} --ci-partition \$\{\{ matrix\.partition \}\}$")
        self.assertNotIn("--waive-local-mouse", profiles)
        self.assertNotIn("--waive-local-graphical", profiles)
        self.assertNotIn("continue-on-error", profiles)
        self.assertNotRegex(profiles, r"(?m)^        (?:exclude|include):")

    def test_every_job_and_artifact_has_platform_profile_and_partition_identity(self):
        profiles = self.job("profiles")
        self.assertIn("    name: Rust (${{ matrix.os }}, ${{ matrix.profile }}, ${{ matrix.partition }})\n", profiles)
        self.assertIn("          name: verification-${{ matrix.os }}-${{ matrix.profile }}-${{ matrix.partition }}\n", profiles)
        self.assertIn("      - name: Preserve verification logs\n        if: always()\n", profiles)
        self.assertIn("          path: .local/verify/\n", profiles)

    def test_partition_budgets_bound_whole_jobs_without_changing_individual_deadlines(self):
        profiles = self.job("profiles")
        self.assertIn("    timeout-minutes: ${{ matrix.partition == 'python' && 75 || 60 }}\n", profiles)
        self.assertNotIn("TOR_TEST_TIMEOUT", profiles)
        self.assertNotIn("--timeout", profiles)

    def test_only_python_partitions_require_a_native_desktop_and_virtual_display(self):
        profiles = self.job("profiles")
        self.assertIn("      - name: Configure Linux graphical test display\n"
                      "        if: runner.os == 'Linux' && matrix.partition == 'python'\n", profiles)
        self.assertIn("      - name: Verify Windows graphical test desktop\n"
                      "        if: runner.os == 'Windows' && matrix.partition == 'python'\n", profiles)
        libraries = re.search(r"(?ms)^      - name: Install Linux Rust build libraries\n(.*?)(?=^      - |\Z)", profiles)
        self.assertIsNotNone(libraries)
        self.assertIn("        if: runner.os == 'Linux'\n", libraries[1])
        self.assertIn("libx11-dev libxcursor-dev libxrandr-dev", libraries[1])
        self.assertNotIn("matrix.partition", libraries[1])
        self.assertNotIn("xvfb", libraries[1])

    def test_split_preserves_read_only_permissions_and_main_only_cache_writes(self):
        workflow = (verify.ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertIn("permissions:\n  contents: read\n", workflow)
        self.assertNotRegex(workflow, r"(?m)^\s+\S+: write$")
        self.assertIn("          shared-key: ${{ matrix.profile }}\n", self.job("profiles"))
        self.assertIn("          save-if: ${{ github.ref == 'refs/heads/main' }}\n", self.job("profiles"))

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
                result = run_workflow_shell(commands[0], outcome)
                self.assertEqual(result.returncode == 0, outcome == "success", shell_diagnostic(result, outcome))

    def test_selected_shell_receives_environment_verbatim(self):
        # A broken interpreter or missing environment must not masquerade as
        # correct rejection of the four non-success aggregate outcomes.
        with patch.dict(os.environ, {"PROFILES_RESULT":"wrong inherited value"}):
            for value in ("ci gate probe with spaces $literal", ""):
                with self.subTest(value=value):
                    result = run_workflow_shell('printf \'%s\' "$PROFILES_RESULT"', value)
                    self.assertEqual(result.returncode, 0, shell_diagnostic(result, value))
                    self.assertEqual(result.stdout, value, shell_diagnostic(result, value))


class WorkflowShellSelection(unittest.TestCase):
    def test_windows_resolves_native_cmd_and_mingw_git_layouts(self):
        for layout in ("cmd/git.exe", "bin/git.exe", "mingw64/bin/git.exe"):
            with self.subTest(layout=layout), tempfile.TemporaryDirectory() as directory:
                root = Path(directory).resolve()
                git, bash = root / layout, root / "bin/bash.exe"
                for path in (git, bash):
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.touch()
                with patch.object(sys, "platform", "win32"), \
                        patch.object(shutil, "which", return_value=str(git)) as which:
                    self.assertEqual(workflow_bash(), str(bash))
                which.assert_called_once_with("git")

    def test_windows_missing_native_shell_fails_without_falling_back_to_path_alias(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            git, alias = root / "Git/cmd/git.exe", root / "Windows/System32/bash.exe"
            for path in (git, alias):
                path.parent.mkdir(parents=True, exist_ok=True)
                path.touch()
            with patch.object(sys, "platform", "win32"), \
                    patch.object(shutil, "which", side_effect=lambda name: str(git if name == "git" else alias)), \
                    self.assertRaisesRegex(FileNotFoundError, "Git for Windows Bash is required"):
                workflow_bash()
        with patch.object(sys, "platform", "win32"), patch.object(shutil, "which", return_value=None), \
                self.assertRaisesRegex(FileNotFoundError, "Git for Windows Bash is required"):
            workflow_bash()

    def test_non_windows_uses_resolved_bash_and_missing_shell_fails(self):
        with patch.object(sys, "platform", "linux"):
            with patch.object(shutil, "which", return_value="/native/bin/bash") as which:
                self.assertEqual(workflow_bash(), "/native/bin/bash")
                which.assert_called_once_with("bash")
            with patch.object(shutil, "which", return_value=None), \
                    self.assertRaisesRegex(FileNotFoundError, "Bash is required"):
                workflow_bash()

    def test_gate_failure_diagnostic_keeps_command_exit_and_both_output_streams(self):
        def failed(command, **_kwargs):
            return subprocess.CompletedProcess(command, 23, stdout="stdout sentinel", stderr="stderr sentinel")

        with patch(__name__ + ".workflow_bash", return_value="/native/bin/bash"), \
                patch.object(subprocess, "run", side_effect=failed), self.assertRaises(AssertionError) as failure:
            CiWorkflowContract().test_required_platform_names_depend_on_every_profile_even_after_failure()
        for detail in ("/native/bin/bash", "exit=23", "stdout sentinel", "stderr sentinel", "outcome='success'"):
            self.assertIn(detail, str(failure.exception))

    def test_launch_and_timeout_diagnostics_keep_the_shell_command_and_available_output(self):
        for error, details in [
                (OSError("launch sentinel"), ("exit=not-started", "launch sentinel")),
                (subprocess.TimeoutExpired("shell", 10, output="partial stdout", stderr="partial stderr"),
                 ("exit=timeout", "partial stdout", "partial stderr"))]:
            with self.subTest(error=type(error).__name__), \
                    patch(__name__ + ".workflow_bash", return_value="/native/bin/bash"), \
                    patch.object(subprocess, "run", side_effect=error), self.assertRaises(AssertionError) as failure:
                run_workflow_shell("test condition", "success")
            for detail in ("/native/bin/bash", "test condition", "outcome='success'", *details):
                self.assertIn(detail, str(failure.exception))

    def test_windows_gate_uses_native_git_bash_even_when_path_bash_is_an_unusable_alias(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            git = root / "Git/cmd/git.exe"
            bash = root / "Git/bin/bash.exe"
            for path in (git, bash):
                path.parent.mkdir(parents=True, exist_ok=True)
                path.touch()
            alias = root / "Windows/System32/bash.exe"
            calls = []

            def execute(command, *, env, **_kwargs):
                calls.append(command)
                if Path(command[0]) != bash:
                    return subprocess.CompletedProcess(command, 1, stdout="Wrong PATH shell selected", stderr="")
                return subprocess.CompletedProcess(command, int(env["PROFILES_RESULT"] != "success"),
                                                   stdout="", stderr="")

            with patch.object(sys, "platform", "win32"), \
                    patch.object(shutil, "which", side_effect=lambda name: str(git if name == "git" else alias)), \
                    patch.object(subprocess, "run", side_effect=execute):
                CiWorkflowContract().test_required_platform_names_depend_on_every_profile_even_after_failure()
            self.assertEqual(len(calls), 5)
            self.assertTrue(all(command[0] == str(bash) for command in calls))

if __name__ == "__main__":
    unittest.main()
