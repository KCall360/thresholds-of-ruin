"""Shadow selection never changes a gate or conceals incomplete coverage."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import ci_policy

PHYSICS_FULL = "test_performance_process.PerformanceProcesses.test_physics_benchmark_reports_completed_durable_commit_timings"
PHYSICS_SMOKE = "physics_smoke_process.PhysicsCorrectnessSmoke.test_all_shapes_preserve_actions_draw_flush_and_resume"


class ShadowPolicyTests(unittest.TestCase):
    def inventory(self):
        def row(name, kind="process", graphical=False, fixture_group=None):
            return {"test_id": name, "kind": kind, "discovery": "default", "graphical": graphical,
                    "native_lane": graphical, "fixture_group": fixture_group,
                    "source": "scripts/" + name.split(".")[0] + ".py"}
        rows = [row("test_feature_process.Feature.test_one"),
                row("test_native_process.Native.test_window", graphical=True),
                row("test_boundary_process.Boundary.test_recovery"),
                row("test_tools.Tools.test_sqlite_paths", kind="tool")]
        sources = {r["source"]: "a" * 64 for r in rows}
        return ci_policy.make_inventory(rows, sources)

    def manifest(self, inventory=None):
        inventory = inventory or self.inventory()
        deferred = {"test_feature_process.Feature.test_one": {
            "reason": "Audited concrete feature behavior; retained in Linux and exhaustive Windows.",
            "obligations": ["feature_behavior"]}}
        required = {"test_boundary_process.Boundary.test_recovery": {
            "reason": "Audited actual-process platform boundaries.",
            "obligations": ["transport_readiness", "sqlite_handles_paths", "kill_checkpoint", "source_deleted_restart"]}}
        return {"schema_version": 1, "status": "shadow_only_unqualified",
                "review_basis": "Audited fixture for policy contract tests.",
                "audit": ci_policy.audit_snapshot(inventory, set(deferred) | set(required)),
                "deferred_tests": deferred, "required_tests": required, "supplemental_tests": {}}

    def select(self, **kwargs):
        inventory = kwargs.pop("inventory", self.inventory())
        manifest = kwargs.pop("manifest") if "manifest" in kwargs else self.manifest(inventory)
        return ci_policy.select(inventory, manifest, event="pull_request",
                                changes=[{"path": "crates/simulation/src/rules.rs", "status": "M"}], **kwargs)

    def test_safe_change_proposes_linux_full_and_windows_focused_both_profiles(self):
        report = self.select()
        self.assertEqual(report["windows_mode"], "focused")
        self.assertTrue(report["shadow_only"])
        self.assertFalse(report["qualified"])
        self.assertEqual(report["mandatory_checks_changed"], False)
        for profile in ("debug", "release"):
            self.assertEqual(len(report["python"]["linux"][profile]), 4)
            selected = report["python"]["windows"][profile]
            self.assertEqual(len(selected), 3)
            self.assertIn("test_native_process.Native.test_window", selected)
            self.assertIn("test_tools.Tools.test_sqlite_paths", selected)
        self.assertEqual(report["rust"], {"linux": ["debug", "release"], "windows": ["debug"]})
        self.assertIn("test_feature_process.Feature.test_one", report["deferred_windows"])

    def test_reviewed_candidate_is_explicit_measurement_without_fabricating_pr_evidence(self):
        inventory, manifest = self.inventory(), self.manifest()
        selected = ci_policy.reviewed_candidate(inventory, manifest)
        self.assertEqual(selected, self.select()["python"]["windows"]["debug"])
        scheduled = ci_policy.select(inventory, manifest, event="workflow_dispatch", changes=None)
        self.assertEqual(scheduled["windows_mode"], "exhaustive")
        self.assertGreater(len(scheduled["python"]["windows"]["debug"]), len(selected))

    def test_measurement_candidate_rejects_invalid_audit_instead_of_labeling_full_as_focused(self):
        inventory, manifest = self.inventory(), self.manifest()
        inventory["sources"]["scripts/test_feature_process.py"] = "b" * 64
        inventory = ci_policy.make_inventory(inventory["tests"], inventory["sources"])
        with self.assertRaises(ValueError):
            ci_policy.reviewed_candidate(inventory, manifest)

    def test_scheduled_manual_release_and_unknown_events_are_exhaustive(self):
        for event in ("schedule", "nightly", "workflow_dispatch", "release", "push", "unknown"):
            with self.subTest(event=event):
                report = ci_policy.select(self.inventory(), self.manifest(), event=event,
                                          changes=[{"path": "docs/guide.md", "status": "M"}])
                self.assertEqual(report["windows_mode"], "exhaustive")
                self.assertEqual(report["rust"]["windows"], ["debug", "release"])

    def test_risk_paths_unknown_paths_and_deleted_safe_paths_are_exhaustive(self):
        for path in ("crates/server/src/storage.rs", "crates/client-common/src/transport.rs",
                     "crates/client-ascii/src/native.rs", "Cargo.lock", "Cargo.toml", "rust-toolchain.toml",
                     ".cargo/config.toml", ".github/workflows/ci.yml", "scripts/process_harness.py",
                     "scripts/test_feature_process.py", "scenarios/rogue/scenario.toml", "unknown.file",
                     "crates/simulation/Cargo.toml", "crates/simulation/build.rs",
                     "crates/world/src/platform.rs", "crates/simulation/src/windows_native.rs",
                     "crates/world/src/storage/archive.rs",
                     "docs/reference/protocol.md", "docs/../Cargo.lock", "/docs/guide.md", "docs\\guide.md"):
            with self.subTest(path=path):
                report = ci_policy.select(self.inventory(), self.manifest(), event="pull_request",
                                          changes=[{"path": path, "status": "M"}])
                self.assertEqual(report["windows_mode"], "exhaustive")
        report = ci_policy.select(self.inventory(), self.manifest(), event="pull_request",
                                  changes=[{"path": "docs/guide.md", "status": "D"}])
        self.assertEqual(report["windows_mode"], "exhaustive")

    def test_missing_change_evidence_and_malformed_status_fail_closed(self):
        for changes in (None, [], [{"path": "docs/guide.md", "status": "?"}],
                        [{"path": "docs/guide.md"}], "docs/guide.md"):
            with self.subTest(changes=changes):
                report = ci_policy.select(self.inventory(), self.manifest(), event="pull_request", changes=changes)
                self.assertEqual(report["windows_mode"], "exhaustive")

    def test_new_unrelated_tests_are_retained_without_reauditing_the_deferral(self):
        inventory, manifest = self.inventory(), self.manifest()
        inventory["tests"].append({**inventory["tests"][0], "test_id": "test_new.New.test_unknown",
                                   "source": "scripts/test_new.py"})
        inventory["sources"]["scripts/test_new.py"] = "b" * 64
        inventory = ci_policy.make_inventory(inventory["tests"], inventory["sources"])
        report = self.select(inventory=inventory, manifest=manifest)
        self.assertEqual(report["validation_errors"], [])
        self.assertEqual(report["windows_mode"], "focused")
        self.assertIn("test_new.New.test_unknown", report["python"]["windows"]["debug"])
        self.assertIn("test_new.New.test_unknown", report["python"]["windows"]["release"])

    def test_deleted_protected_test_source_or_metadata_drift_and_unknown_deferral_fail_closed(self):
        for mutation in ("deleted", "source", "metadata", "classification"):
            inventory, manifest = self.inventory(), self.manifest()
            if mutation == "deleted":
                inventory["tests"] = [row for row in inventory["tests"]
                                      if row["test_id"] != "test_feature_process.Feature.test_one"]
            elif mutation == "source":
                inventory["sources"]["scripts/test_feature_process.py"] = "b" * 64
            elif mutation == "metadata":
                inventory["tests"][0]["fixture_group"] = "new.shared.fixture"
            else:
                manifest["deferred_tests"]["test_unknown.Unknown.test_method"] = manifest["deferred_tests"]["test_feature_process.Feature.test_one"]
            inventory = ci_policy.make_inventory(inventory["tests"], inventory["sources"])
            report = self.select(inventory=inventory, manifest=manifest)
            self.assertEqual(report["windows_mode"], "exhaustive")
            self.assertTrue(report["validation_errors"])

    def test_duplicate_ids_reject_in_inventory_and_corrupt_audit_metadata_rejects(self):
        inventory = self.inventory()
        with self.assertRaises(ValueError):
            ci_policy.make_inventory(inventory["tests"] * 2, inventory["sources"])
        manifest = self.manifest()
        manifest["audit"]["tests"]["test_feature_process.Feature.test_one"]["test_id"] = "duplicated"
        self.assertTrue(self.select(manifest=manifest)["validation_errors"])

    def test_native_methods_and_required_obligations_cannot_be_deferred(self):
        for name in ("test_native_process.Native.test_window", "test_boundary_process.Boundary.test_recovery"):
            manifest = self.manifest()
            manifest["deferred_tests"][name] = {"reason": "Invalid deferral of required method", "obligations": ["feature_behavior"]}
            self.assertTrue(self.select(manifest=manifest)["validation_errors"])

    def test_classification_requires_nonempty_audit_reason_and_known_selection(self):
        for key, value in (("reason", ""), ("selection", "maybe"), ("obligations", []),
                           ("obligations", ["made_up"])):
            manifest = self.manifest()
            manifest["deferred_tests"]["test_feature_process.Feature.test_one"][key] = value
            self.assertTrue(self.select(manifest=manifest)["validation_errors"])

    def test_missing_required_boundary_obligation_fails_closed(self):
        manifest = self.manifest()
        manifest["required_tests"]["test_boundary_process.Boundary.test_recovery"]["obligations"] = ["feature_behavior"]
        self.assertTrue(self.select(manifest=manifest)["validation_errors"])

    def test_fixture_groups_promote_deferred_methods_without_splitting_shared_setup(self):
        inventory = self.inventory()
        for row in inventory["tests"]:
            if row["kind"] == "process" and not row["graphical"]:
                row["fixture_group"] = "shared.fixture"
        inventory = ci_policy.make_inventory(inventory["tests"], inventory["sources"])
        report = self.select(inventory=inventory)
        self.assertEqual(len(report["python"]["windows"]["debug"]), 4)
        self.assertEqual(report["fixture_promotions"], ["test_feature_process.Feature.test_one"])

    def test_hash_is_order_independent_but_content_sensitive(self):
        inventory = self.inventory()
        self.assertEqual(inventory, ci_policy.make_inventory(list(reversed(inventory["tests"])), inventory["sources"]))
        rows = copy.deepcopy(inventory["tests"])
        rows[0]["graphical"] = not rows[0]["graphical"]
        changed = ci_policy.make_inventory(rows, inventory["sources"])
        self.assertNotEqual(inventory["sha256"], changed["sha256"])

    def test_json_duplicate_keys_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "manifest.json"
            path.write_text('{"x":1,"x":2}', encoding="utf-8")
            with self.assertRaises(ValueError):
                ci_policy.read_json(path)

    def test_discovery_import_failure_and_empty_suite_cannot_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "scripts").mkdir()
            with self.assertRaises(ValueError):
                ci_policy.discover(root)
            (root / "scripts/test_broken.py").write_text('raise RuntimeError("broken import")\n')
            with self.assertRaises(ValueError):
                ci_policy.discover(root)

    def test_inventory_rejects_empty_duplicate_or_malformed_sources(self):
        with self.assertRaises(ValueError):
            ci_policy.make_inventory([], {})
        inventory = self.inventory()
        for sources in ({}, {**inventory["sources"], "scripts/test_feature_process.py": "invalid"}):
            with self.assertRaises(ValueError):
                ci_policy.make_inventory(inventory["tests"], sources)


class ShadowPolicyCliTests(unittest.TestCase):
    def test_real_cli_reports_discovery_and_writes_runner_ids_without_running_tests(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / "scripts"
            scripts.mkdir()
            source = scripts / "test_policy_sample.py"
            # Exercise Windows line endings even when this regression runs on Linux.
            source.write_bytes(b'import unittest\r\nclass Sample(unittest.TestCase):\r\n'
                               b' def test_must_not_run(self):\r\n  raise AssertionError("ran body")\r\n')
            script = Path(ci_policy.__file__)
            command = [sys.executable, str(script), "inventory", "--root", str(root)]
            first = subprocess.run(command, capture_output=True, text=True, check=False)
            self.assertEqual(first.returncode, 0, first.stderr)
            inventory = json.loads(first.stdout)
            self.assertEqual(inventory["tests"][0]["test_id"], "test_policy_sample.Sample.test_must_not_run")
            self.assertEqual(inventory["sources"]["scripts/test_policy_sample.py"],
                             hashlib.sha256(source.read_bytes().replace(b"\r\n", b"\n")).hexdigest())
            manifest = root / "manifest.json"
            # No process tests: required-boundary obligations cannot qualify a focus run.
            manifest.write_text(json.dumps({"schema_version": 1, "status": "shadow_only_unqualified",
                                           "review_basis": "CLI fixture", "audit": ci_policy.audit_snapshot(inventory, set()),
                                           "deferred_tests": {}, "required_tests": {}, "supplemental_tests": {}}))
            changes = root / "changes.json"
            changes.write_text('[{"path":"docs/guide.md","status":"M"}]')
            out = root / "report"
            result = subprocess.run([sys.executable, str(script), "select", "--root", str(root),
                                     "--manifest", str(manifest), "--event", "pull_request",
                                     "--changes", str(changes), "--output-dir", str(out)],
                                    capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["windows_mode"], "exhaustive")
            self.assertTrue(report["validation_errors"])
            self.assertEqual((out / "windows-debug.ids").read_text().splitlines(),
                             ["test_policy_sample.Sample.test_must_not_run"])
            self.assertEqual(report["runner"]["arguments"],
                             ["scripts/test_runner.py", "--workers", "<existing-verify-worker-count>", "--from-file", "<platform>-<profile>.ids"])

    def test_committed_manifest_retains_native_methods_and_falls_back_if_audit_is_stale(self):
        manifest = ci_policy.read_json(ci_policy.DEFAULT_MANIFEST)
        inventory = ci_policy.discover(ci_policy.ROOT, audit_inputs=manifest["audit"]["sources"])
        report = ci_policy.select(inventory, manifest, event="pull_request",
                                  changes=[{"path": "docs/guide.md", "status": "M"}])
        self.assertEqual(report["windows_mode"], "exhaustive" if report["validation_errors"] else "focused")
        if report["validation_errors"]:
            self.assertEqual(report["python"]["windows"]["debug"], ci_policy.default_test_ids(inventory))
        selected = set(report["python"]["windows"]["debug"])
        self.assertTrue(all(row["test_id"] in selected for row in inventory["tests"]
                            if row["graphical"] or row["kind"] == "tool"))


class PhysicsSupplementalPolicyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.reviewed = ci_policy.read_json(ci_policy.DEFAULT_MANIFEST)
        cls.discovered = ci_policy.discover(audit_inputs=cls.reviewed["audit"]["sources"])
        # Policy fixtures exercise a current hypothetical review. Production
        # hashes remain untouched: later helper edits use exhaustive fallback.
        cls.reviewed["audit"]["sources"] = {path: cls.discovered["sources"][path]
                                              for path in cls.reviewed["audit"]["sources"]}

    def report(self, inventory=None, manifest=None, event="pull_request"):
        return ci_policy.select(inventory or self.discovered, manifest or self.reviewed,
                                event=event, changes=[{"path": "docs/guide.md", "status": "M"}])

    def test_inventory_accounts_for_explicit_smoke_as_a_process_with_its_own_fixture(self):
        rows = {row["test_id"]: row for row in self.discovered["tests"]}
        self.assertTrue(PHYSICS_SMOKE in rows, "explicit smoke is missing from coverage inventory")
        self.assertEqual(rows[PHYSICS_SMOKE], {
            "test_id": PHYSICS_SMOKE, "kind": "process", "discovery": "supplemental",
            "graphical": False, "native_lane": False,
            "fixture_group": "physics_smoke_process.PhysicsCorrectnessSmoke",
            "source": "scripts/physics_smoke_process.py"})
        self.assertEqual(rows[PHYSICS_FULL]["discovery"], "default")
        self.assertIsNone(rows[PHYSICS_FULL]["fixture_group"])

    def test_candidate_replaces_only_audited_full_method_in_both_process_profiles(self):
        report = self.report()
        self.assertEqual(report["validation_errors"], [])
        self.assertFalse(report["qualified"])
        defaults = set(ci_policy.default_test_ids(self.discovered))
        expected = defaults - self.reviewed["deferred_tests"].keys() | {PHYSICS_SMOKE}
        for profile in ("debug", "release"):
            self.assertEqual(set(report["python"]["linux"][profile]), defaults)
            self.assertEqual(set(report["python"]["windows"][profile]), expected)
            self.assertIn(PHYSICS_FULL, report["python"]["linux"][profile])
            self.assertNotIn(PHYSICS_SMOKE, report["python"]["linux"][profile])
        self.assertEqual(set(ci_policy.reviewed_candidate(self.discovered, self.reviewed)), expected)
        self.assertEqual(report["supplemental_windows"], [PHYSICS_SMOKE])
        self.assertEqual(report["counts"]["supplemental_tests"], 1)
        self.assertEqual(report["counts"]["all_tests"], len(defaults) + 1)
        self.assertEqual(report["counts"]["default_tests"], len(defaults))
        self.assertEqual(report["runner"]["profile_environment"], "TOR_TEST_PROFILE=<profile>")

    def test_exhaustive_events_keep_original_full_method_without_duplicate_smoke(self):
        for event in ("schedule", "workflow_dispatch", "push", "unknown"):
            report = self.report(event=event)
            self.assertEqual(report["windows_mode"], "exhaustive")
            for platform in ("linux", "windows"):
                for ids in report["python"][platform].values():
                    self.assertIn(PHYSICS_FULL, ids)
                    self.assertNotIn(PHYSICS_SMOKE, ids)
            self.assertEqual(report["supplemental_windows"], [])

    def test_every_supplemental_source_and_metadata_drift_rejects_candidate(self):
        closure = self.reviewed["supplemental_tests"][PHYSICS_SMOKE]["source_closure"]
        for source in closure:
            for mutation in ("stale", "missing_hash", "missing_file"):
                with self.subTest(source=source, mutation=mutation):
                    inventory, manifest = copy.deepcopy(self.discovered), copy.deepcopy(self.reviewed)
                    if mutation == "missing_hash":
                        del manifest["audit"]["sources"][source]
                    else:
                        inventory["sources"][source] = "0" * 64
                        if mutation == "missing_file":
                            del inventory["sources"][source]
                            inventory["tests"] = [r for r in inventory["tests"] if r["source"] != source]
                        inventory = ci_policy.make_inventory(inventory["tests"], inventory["sources"])
                    self.assert_rejected(inventory, manifest)
        for field, value in (("discovery", "default"), ("kind", "tool"), ("graphical", True),
                             ("native_lane", True), ("fixture_group", None)):
            inventory = copy.deepcopy(self.discovered)
            next(r for r in inventory["tests"] if r["test_id"] == PHYSICS_SMOKE)[field] = value
            inventory = ci_policy.make_inventory(inventory["tests"], inventory["sources"])
            self.assert_rejected(inventory, self.reviewed)

    def assert_rejected(self, inventory, manifest):
        report = self.report(inventory, manifest)
        self.assertEqual(report["windows_mode"], "exhaustive")
        self.assertTrue(report["validation_errors"])
        self.assertNotIn(PHYSICS_SMOKE, report["python"]["windows"]["debug"])
        with self.assertRaises(ValueError):
            ci_policy.reviewed_candidate(inventory, manifest)

    def test_missing_malformed_or_unreviewed_supplemental_classifications_fail_closed(self):
        mutations = (None, {}, [], {PHYSICS_SMOKE: None}, {PHYSICS_SMOKE: {}},
                     {"unreviewed.Extra.test_hidden": self.reviewed["supplemental_tests"][PHYSICS_SMOKE]})
        for value in mutations:
            manifest = copy.deepcopy(self.reviewed)
            manifest["supplemental_tests"] = value
            self.assert_rejected(self.discovered, manifest)
        manifest = copy.deepcopy(self.reviewed)
        del manifest["supplemental_tests"]
        self.assert_rejected(self.discovered, manifest)
        for field, value in (("reason", ""), ("replaces", "test_unknown.Unknown.test_missing"),
                             ("source_closure", []), ("source_closure", "scripts/physics_smoke_process.py"),
                             ("source_closure", ["scripts/physics_smoke_process.py"])):
            manifest = copy.deepcopy(self.reviewed)
            manifest["supplemental_tests"][PHYSICS_SMOKE][field] = value
            self.assert_rejected(self.discovered, manifest)

    def test_missing_supplemental_method_fails_closed_but_does_not_remove_full(self):
        inventory = ci_policy.make_inventory([r for r in self.discovered["tests"]
                                              if r["test_id"] != PHYSICS_SMOKE], self.discovered["sources"])
        self.assert_rejected(inventory, self.reviewed)
        self.assertIn(PHYSICS_FULL, self.report(inventory)["python"]["windows"]["debug"])

    def test_shared_default_fixture_promotes_full_and_removes_duplicate_smoke(self):
        inventory, manifest = copy.deepcopy(self.discovered), copy.deepcopy(self.reviewed)
        sibling = "test_performance_process.PerformanceProcesses.test_small_maintained_producers_emit_complete_valid_reports"
        for row in inventory["tests"]:
            if row["test_id"] in (PHYSICS_FULL, sibling):
                row["fixture_group"] = "test_performance_process.PerformanceProcesses"
        inventory = ci_policy.make_inventory(inventory["tests"], inventory["sources"])
        manifest["audit"] = ci_policy.audit_snapshot(inventory, set(manifest["audit"]["tests"]),
                                                     manifest["audit"]["sources"])
        report = self.report(inventory, manifest)
        self.assertEqual(report["validation_errors"], [])
        self.assertIn(PHYSICS_FULL, report["fixture_promotions"])
        for ids in report["python"]["windows"].values():
            self.assertIn(PHYSICS_FULL, ids)
            self.assertIn(sibling, ids)
            self.assertNotIn(PHYSICS_SMOKE, ids)

    def test_supplemental_fixture_cannot_promote_or_share_default_setup(self):
        inventory, manifest = copy.deepcopy(self.discovered), copy.deepcopy(self.reviewed)
        smoke = next(row for row in inventory["tests"] if row["test_id"] == PHYSICS_SMOKE)
        next(row for row in inventory["tests"] if row["test_id"] == PHYSICS_FULL)["fixture_group"] = smoke["fixture_group"]
        inventory = ci_policy.make_inventory(inventory["tests"], inventory["sources"])
        manifest["audit"] = ci_policy.audit_snapshot(inventory, set(manifest["audit"]["tests"]),
                                                     manifest["audit"]["sources"])
        self.assert_rejected(inventory, manifest)

    def test_stale_manifest_cli_select_succeeds_exhaustively_but_check_and_candidate_reject(self):
        manifest = copy.deepcopy(self.reviewed)
        manifest["audit"]["sources"]["scripts/performance_process_support.py"] = "0" * 64
        self.assert_rejected(self.discovered, manifest)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "stale.json"
            path.write_text(json.dumps(manifest), encoding="utf-8")
            changes = root / "changes.json"
            changes.write_text('[{"path":"docs/guide.md","status":"M"}]', encoding="utf-8")
            command = [sys.executable, str(Path(ci_policy.__file__)), "--manifest", str(path)]
            result = subprocess.run([*command, "select", "--event", "pull_request", "--changes", str(changes),
                                     "--output-dir", str(root / "report")], capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["windows_mode"], "exhaustive")
            self.assertTrue(report["validation_errors"])
            for platform in ("windows", "linux"):
                for profile in ("debug", "release"):
                    self.assertEqual((root / "report" / f"{platform}-{profile}.ids").read_text().splitlines(),
                                     ci_policy.default_test_ids(self.discovered))
            checked = subprocess.run([*command, "check"], capture_output=True, text=True, check=False)
            self.assertEqual(checked.returncode, 1, checked.stderr)
            self.assertTrue(json.loads(checked.stdout)["validation_errors"])

    def test_new_default_method_is_retained_without_a_supplemental_audit(self):
        inventory = copy.deepcopy(self.discovered)
        inventory["tests"].append({"test_id": "test_new.New.test_unreviewed", "kind": "tool",
                                   "discovery": "default", "graphical": False, "native_lane": False,
                                   "fixture_group": None, "source": "scripts/test_new.py"})
        inventory["sources"]["scripts/test_new.py"] = "a" * 64
        inventory = ci_policy.make_inventory(inventory["tests"], inventory["sources"])
        report = self.report(inventory)
        self.assertEqual(report["validation_errors"], [])
        self.assertIn("test_new.New.test_unreviewed", report["python"]["windows"]["debug"])

    def test_existing_runner_loads_and_schedules_supplemental_process_without_native_lane(self):
        loader = unittest.TestLoader()
        tests = list(ci_policy.parallel_tests.flatten(loader.loadTestsFromName(PHYSICS_SMOKE)))
        self.assertFalse(loader.errors, loader.errors)
        self.assertEqual([test.id() for test in tests], [PHYSICS_SMOKE])
        self.assertIsInstance(tests[0], ci_policy.ProcessTestCase)
        self.assertTrue(ci_policy.parallel_tests.has_own_fixture(type(tests[0])))
        headless, native = ci_policy.parallel_tests.plan(tests, {}, workers=2)
        self.assertEqual(native, [])
        self.assertEqual([name for batch in headless for name in batch.ids], [PHYSICS_SMOKE])


if __name__ == "__main__":
    unittest.main()
