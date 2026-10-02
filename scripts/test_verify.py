"""Tests for verify.py's change selection, which must never select too little."""
from pathlib import Path
import tempfile
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
    def names(self, tier, affected=(), process=()):
        return [name for name, _, _ in verify.plan(tier, set(affected), list(process))]

    def test_full_matches_ci(self):
        self.assertEqual(
            self.names("full", ["tor-world"], ["test_text_process"]),
            ["fmt", "clippy", "python-debug", "architecture", "rustdoc", "rust-debug", "rust-release", "process-release"],
        )

    def test_push_runs_every_debug_check_and_affected_release(self):
        names = self.names("push", ["tor-world"], ["test_text_process"])
        self.assertEqual(names[:6], ["fmt", "clippy", "python-debug", "architecture", "rustdoc", "rust-debug"])
        self.assertEqual(names[6:], ["rust-release", "process-release"])
        release = dict((n, c) for n, c, _ in verify.plan("push", {"tor-world"}, []))["rust-release"]
        self.assertEqual(release[2:5], ["-p", "tor-world", "--release"])

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


if __name__ == "__main__":
    unittest.main()
