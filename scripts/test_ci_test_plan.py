"""Independent risk-plan expectations: malformed scope must never narrow CI."""
import unittest
from unittest.mock import patch

import ci_test_plan as plan


class PlanTests(unittest.TestCase):
    def selection(self, **changes):
        value = {"full_required": False, "reasons": ["reviewed"], "contracts": ["wire"],
                 "linux_debug": ["test_wire_process.Wire.test_roundtrip"],
                 "windows_debug": ["test_native_process.Native.test_input"],
                 "linux_release": ["test_wire_process.Wire.test_roundtrip"], "consumer_3d": False, "requires_apps": True, "requires_rust": True}
        return {**value, **changes}

    def test_non_pr_events_are_exhaustive_even_when_paths_are_known(self):
        for event in ("schedule", "workflow_dispatch", "release", "unknown"):
            with self.subTest(event=event):
                result = plan.make_plan(event, "a" * 40, "b" * 40, ["crates/protocol/src/lib.rs"],
                                        ["test_wire_process.Wire.test_roundtrip"], self.selection())
                self.assertEqual(result["mode"], "full")
                self.assertEqual(result["lanes"], [])
                self.assertTrue(result["consumer_3d"])

    def test_selected_assurance_is_not_full_and_contains_platform_release_owners(self):
        result = plan.make_plan("pull_request", "a" * 40, "b" * 40,
                                ["crates/protocol/src/lib.rs"],
                                ["test_wire_process.Wire.test_roundtrip", "test_native_process.Native.test_input"], self.selection())
        self.assertEqual(result["mode"], "selected")
        self.assertEqual([row["id"] for row in result["lanes"]],
                         ["linux-debug-rust", "linux-debug-python", "windows-debug-python", "linux-release-python"])
        self.assertEqual(result["assurance"], "selected-pr-contracts")
        self.assertEqual(result["policy_version"], plan.POLICY_VERSION)

    def test_consumer_is_separate_from_full_renderer(self):
        result = plan.make_plan("pull_request", "a" * 40, "b" * 40, ["crates/protocol/src/lib.rs"],
            ["test_wire_process.Wire.test_roundtrip", "test_native_process.Native.test_input"], self.selection(consumer_3d=True))
        self.assertEqual(result["lanes"][-1]["id"], "linux-3d-consumer")
        self.assertNotIn("render", str(result))

    def test_control_and_toolchain_changes_cannot_choose_their_own_fast_gate(self):
        for path in ("scripts/ci_test_plan.py", "scripts/ci_test_contracts.py", "scripts/ci_test_inventory.json",
                     ".github/workflows/ci.yml", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml"):
            result = plan.make_plan("pull_request", "a" * 40, "b" * 40, [path], [], self.selection())
            self.assertEqual(result["mode"], "full", path)
            self.assertTrue(result["consumer_3d"])

    def test_unreviewed_or_invalid_selection_falls_back(self):
        for selection in (self.selection(full_required=True), self.selection(linux_debug=[]),
                          self.selection(linux_debug=["not.a.test"]), self.selection(consumer_3d="false")):
            result = plan.make_plan("pull_request", "a" * 40, "b" * 40, ["crates/protocol/src/lib.rs"], [], selection)
            self.assertEqual(result["mode"], "full")
            self.assertTrue(result["consumer_3d"])

    def test_raw_changes_keep_both_deleted_and_added_names(self):
        raw = b":100644 000000 " + b"a" * 40 + b" " + b"0" * 40 + b" D\0scripts/old.py\0"
        raw += b":000000 100644 " + b"0" * 40 + b" " + b"b" * 40 + b" A\0scripts/new.py\0"
        self.assertEqual(plan.diff_paths(raw), ["scripts/new.py", "scripts/old.py"])

    def test_raw_diff_rejects_symlinks_duplicates_and_truncated_input(self):
        row = b":100644 100644 " + b"a" * 40 + b" " + b"b" * 40 + b" M\0scripts/thing.py\0"
        for raw in (row[:-1], row + row, row.replace(b":100644", b":120000"), row.replace(b"scripts/thing.py", b"../escape")):
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                plan.diff_paths(raw)

    def test_missing_scope_is_full_not_empty_selected(self):
        with patch.object(plan, "scope_paths", side_effect=ValueError("bad evidence")):
            result = plan.current_plan(event_name="pull_request", event={}, source_sha="a" * 40)
        self.assertEqual(result["mode"], "full")
        self.assertIn("scope", result["reasons"][0])

    def test_digest_binds_selection_source_and_policy(self):
        value = {"policy_version": 2, "source_sha": "a" * 40, "ids": ["a", "b"]}
        self.assertEqual(plan.digest(value), plan.digest(dict(reversed(list(value.items())))))
        self.assertNotEqual(plan.digest(value), plan.digest({**value, "ids": ["a"]}))

    def test_pure_tooling_plan_has_two_no_application_lanes(self):
        selection = self.selection(requires_rust=False, requires_apps=False, linux_release=[])
        inventory = ["test_wire_process.Wire.test_roundtrip", "test_native_process.Native.test_input"]
        result = plan.make_plan("pull_request", "a" * 40, "b" * 40, ["scripts/report.py"], inventory, selection)
        self.assertEqual([lane["id"] for lane in result["lanes"]], ["linux-tooling", "windows-tooling"])
        self.assertFalse(result["requires_apps"])
        self.assertFalse(result["requires_rust"])

    def test_process_only_plan_builds_apps_without_redundant_rust_suite(self):
        selection = self.selection(requires_rust=False)
        inventory = ["test_wire_process.Wire.test_roundtrip", "test_native_process.Native.test_input"]
        result = plan.make_plan("push", "a" * 40, "b" * 40, ["scripts/test_wire_process.py"], inventory, selection)
        self.assertEqual(result["lanes"][0]["id"], "linux-debug-build")
        self.assertEqual(result["mode"], "selected")

    def test_main_scope_requires_complete_nonforced_ancestral_change(self):
        source, before = "a" * 40, "b" * 40
        raw = b":100644 100644 " + b"c" * 40 + b" " + b"d" * 40 + b" M\0scripts/report.py\0"
        def git(root, *args):
            if args == ("rev-parse", "--is-shallow-repository"):
                return b"false\n"
            if args == ("rev-parse", "HEAD"):
                return source.encode()
            if args[0] == "diff":
                return raw
            self.assertEqual(args, ("merge-base", "--is-ancestor", before, source))
            return b""
        event = {"before": before, "after": source, "created": False, "deleted": False,
                 "forced": False, "ref": "refs/heads/main"}
        with patch.object(plan.ci_scope, "git", side_effect=git):
            self.assertEqual(plan.scope_paths(plan.ROOT, "push", event, source), ["scripts/report.py"])
            for field, value in (("forced", True), ("created", True), ("deleted", True),
                                 ("before", "0" * 40), ("ref", "refs/heads/other"), ("after", "f" * 40)):
                with self.subTest(field=field), self.assertRaises(ValueError):
                    plan.scope_paths(plan.ROOT, "push", {**event, field: value}, source)


if __name__ == "__main__":
    unittest.main()
