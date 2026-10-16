"""Bounded, explicitly untrusted observations from a real checkout."""
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import ci_observation


SHA, TREE, PARENT = (character * 40 for character in "abc")
GITHUB = {
    "GITHUB_REPOSITORY": "owner/repo", "GITHUB_RUN_ID": "123", "GITHUB_RUN_ATTEMPT": "2",
    "GITHUB_JOB": "profiles", "GITHUB_EVENT_NAME": "pull_request", "GITHUB_REF": "refs/pull/4/merge",
    "GITHUB_SHA": SHA, "GITHUB_WORKFLOW_REF": "owner/repo/.github/workflows/ci.yml@refs/pull/4/merge",
    "GITHUB_WORKFLOW_SHA": SHA, "RUNNER_OS": "Linux", "RUNNER_ARCH": "X64",
    "ImageOS": "ubuntu24", "ImageVersion": "20261001.1.0",
}
RUSTC = ("rustc 1.90.0 (1159e78c4 2025-09-14)\nbinary: rustc\ncommit-hash: " + "b" * 40 +
         "\ncommit-date: 2025-09-14\nhost: x86_64-unknown-linux-gnu\nrelease: 1.90.0\nLLVM version: 20.1.8\n").encode()


class Observation(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name).resolve()
        for name in ci_observation.CONFIG_FILES:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"fixture\n")
        self.status = b""
        self.native_output = b"libx11-6\t2:1.8.7-1build1\tamd64\tinstall ok installed\n"
        self.memory_output = b"12582912\r\n"
        self.memory = self.enterContext(patch("verify.free_memory_gb", return_value=12.0))
        self.cpus = self.enterContext(patch.object(ci_observation.os, "cpu_count", return_value=8))
        self.distributions = self.enterContext(patch("importlib.metadata.distributions", return_value=[
            SimpleNamespace(metadata={"Name": "NumPy"}, version="1.2.3")]))

    def probe(self, command, root, issues, scope, **kwargs):
        if "--show-toplevel" in command: return str(root).encode() + b"\n"
        if "--verify" in command: return SHA.encode() + b"\n"
        if "cat-file" in command: return f"tree {TREE}\nparent {PARENT}\nauthor Test <test@example.invalid> 1 +0000\n\nmessage\n".encode()
        if "status" in command: return self.status
        if command[0] == "rustc": return RUSTC
        if command[0] == "cargo": return b"cargo 1.90.0 (840b83a10 2025-07-30)\n"
        if command[0] == "powershell":
            self.assertEqual(command, ["powershell", "-NoProfile", "-Command", "(Get-CimInstance Win32_OperatingSystem).FreeVirtualMemory"])
            self.assertEqual(kwargs, {"max_output_bytes": 128})
            return self.memory_output
        if command[0] == "dpkg-query":
            self.assertEqual(command, ["dpkg-query", "-W", "-f=${Package}\t${Version}\t${Architecture}\t${Status}\n"])
            self.assertEqual(kwargs, {"max_output_bytes": 512 * 1024})
            return self.native_output
        self.fail(f"Unexpected command: {command}")

    def collect(self, **kwargs):
        with patch.object(ci_observation, "probe", side_effect=self.probe):
            return ci_observation.collect(self.root, "before", environment=GITHUB, **kwargs)

    def test_complete_report_is_explicitly_untrusted_and_has_no_reuse_verdict(self):
        result = self.collect(profile="debug", partition="python")
        self.assertEqual(result["schema_version"], 1)
        self.assertEqual(result["status"], "untrusted_observation")
        self.assertTrue(result["complete"], result["issues"])
        self.assertEqual(result["checkout"], {"sha": SHA, "tree": TREE, "parents": [PARENT],
                                            "tracked_dirty": False, "untracked_present": False})
        self.assertEqual(result["github"]["run_attempt"], 2)
        self.assertEqual(result["labels"], {"profile": "debug", "partition": "python"})
        self.assertNotIn("decision", result)
        self.assertNotIn("reusable", result)
        self.assertNotIn("compatible", result)
        self.assertIn("executable_diagnostic", result["tools"]["python"])
        self.assertEqual(result["tools"]["rustc"]["commit_hash"], "b" * 40)

    def test_hashes_are_exact_file_bytes_and_do_not_discover_arbitrary_inputs(self):
        (self.root / "secret.env").write_text("SECRET=never-record-me")
        result = self.collect()
        self.assertEqual(set(result["file_hashes"]), set(ci_observation.CONFIG_FILES))
        entry = result["file_hashes"]["Cargo.lock"]
        self.assertEqual(entry, {"status": "present", "sha256": hashlib.sha256(b"fixture\n").hexdigest(), "bytes": 8})
        self.assertNotIn("never-record-me", json.dumps(result))

    def test_environment_is_allowlisted_and_invalid_values_are_not_echoed(self):
        bad = {**GITHUB, "GITHUB_TOKEN": "secret-value", "GITHUB_RUN_ID": "secret-value",
               "GITHUB_REF": "refs/heads/main\nsecret-value", "AWS_SECRET_ACCESS_KEY": "secret-value"}
        with patch.object(ci_observation, "probe", side_effect=self.probe):
            result = ci_observation.collect(self.root, "after", environment=bad)
        self.assertFalse(result["complete"])
        self.assertIsNone(result["github"]["run_id"])
        self.assertIsNone(result["github"]["ref"])
        self.assertNotIn("secret-value", json.dumps(result))

    def test_absent_metadata_still_produces_an_incomplete_report(self):
        with patch.object(ci_observation, "probe", side_effect=self.probe):
            result = ci_observation.collect(self.root, "before", environment={})
        self.assertFalse(result["complete"])
        self.assertEqual(result["status"], "untrusted_observation")
        self.assertIsNone(result["runner"]["image_version"])
        self.assertEqual(result["checkout"]["sha"], SHA)

    def test_build_test_environment_is_allowlisted_hashed_and_scoped(self):
        environment = {**GITHUB, "RUSTFLAGS": "--remap-path-prefix=/private/build=.",
                       "CARGO_TARGET_DIR": "/private/target", "PYTHONHASHSEED": "",
                       "TOR_SCENARIO_COMPILER": "/private/scenario", "TOR_TEST_PROFILE": "release",
                       "TOR_LOCAL_MOUSE_WAIVER": "1", "TOR_TEST_PRIORITY": "below_normal",
                       "GITHUB_TOKEN": "private-token", "UNRECOGNIZED_OPTION": "private-option"}
        with patch.object(ci_observation, "probe", side_effect=self.probe):
            result = ci_observation.collect(self.root, "before", environment=environment)
        self.assertIn("build_test_environment", result)
        observed = result["build_test_environment"]
        self.assertEqual(observed["scope"], "observer_process")
        values = observed["variables"]
        self.assertEqual(set(values), set(ci_observation.BUILD_TEST_ENVIRONMENT))
        for name in ("RUSTFLAGS", "CARGO_TARGET_DIR", "PYTHONHASHSEED", "TOR_SCENARIO_COMPILER",
                     "TOR_TEST_PROFILE", "TOR_LOCAL_MOUSE_WAIVER", "TOR_TEST_PRIORITY"):
            self.assertEqual(values[name], {"status": "present", "sha256": hashlib.sha256(environment[name].encode()).hexdigest()})
        self.assertEqual(values["CARGO_BUILD_TARGET"], {"status": "absent"})
        expected = hashlib.sha256(json.dumps(values, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        self.assertEqual(observed["sha256"], expected)
        self.assertTrue(result["complete"], result["issues"])
        for private in ("/private", "private-token", "private-option", "GITHUB_TOKEN", "UNRECOGNIZED_OPTION"):
            self.assertNotIn(private, json.dumps(result))
        self.assertIn("volatile_display_state", result["omissions"])
        self.assertIn("verifier_stage_overrides", result["omissions"])
        self.assertIn("package_contents_and_provenance", result["omissions"])

    def test_environment_fingerprint_changes_for_value_and_presence_not_mapping_order(self):
        fingerprints = []
        for environment in (GITHUB, {**GITHUB, "RUSTFLAGS": ""}, {**GITHUB, "RUSTFLAGS": "-Copt-level=1"}):
            with patch.object(ci_observation, "probe", side_effect=self.probe):
                result = ci_observation.collect(self.root, "before", environment=environment)
                reordered = ci_observation.collect(self.root, "after", environment=dict(reversed(list(environment.items()))))
            self.assertIn("build_test_environment", result)
            self.assertEqual(result["build_test_environment"], reordered["build_test_environment"])
            fingerprints.append(result["build_test_environment"]["sha256"])
        self.assertEqual(len(set(fingerprints)), 3)

    def test_external_runtime_binary_override_is_present_and_value_sensitive_without_paths(self):
        observed = [ci_observation.build_test_environment(environment, []) for environment in (
            {}, {"TOR_TEST_BINARIES": "/private/first"}, {"TOR_TEST_BINARIES": "/private/second"})]
        self.assertEqual(len({row["sha256"] for row in observed}), 3)
        self.assertEqual(observed[0]["variables"]["TOR_TEST_BINARIES"], {"status": "absent"})
        self.assertEqual(observed[1]["variables"]["TOR_TEST_BINARIES"]["status"], "present")
        self.assertNotIn("/private", json.dumps(observed))

    def test_fixed_workload_controls_hash_presence_and_values_without_disclosure(self):
        for name in ("TOR_ARENA_MATRIX", "TOR_SAVED_EXPLORATION_REGIONS", "TOR_PERFORMANCE_REGIONS", "TOR_PERFORMANCE_CYCLES"):
            with self.subTest(name=name):
                observed = [ci_observation.build_test_environment(environment, []) for environment in
                            ({}, {name: ""}, {name: "private-first"}, {name: "private-second"})]
                self.assertIn(name, observed[0]["variables"])
                self.assertEqual(observed[0]["variables"][name], {"status": "absent"})
                self.assertEqual(observed[1]["variables"][name],
                                 {"status": "present", "sha256": hashlib.sha256(b"").hexdigest()})
                self.assertEqual(len({row["sha256"] for row in observed}), 4)
                self.assertNotIn("private-", json.dumps(observed))

    def test_execution_limits_use_existing_memory_and_cpu_sizing_on_both_platforms(self):
        for runner_os in ("Linux", "Windows"):
            for memory, cpus, expected in ((0.5, 8, (1, 1)), (5, 8, (2, 3)), (10, 16, (4, 6)),
                                           (12, 2, (2, 1)), (3, None, (1, 1))):
                with self.subTest(runner_os=runner_os, memory=memory, cpus=cpus):
                    self.memory.return_value, self.cpus.return_value = memory, cpus
                    self.memory_output = f"{int(memory * 1024 * 1024)}\r\n".encode()
                    self.memory.reset_mock()
                    with patch.object(ci_observation, "probe", side_effect=self.probe) as commands:
                        result = ci_observation.collect(self.root, "before", environment={**GITHUB, "RUNNER_OS": runner_os})
                    self.assertIn("execution_limits", result)
                    self.assertEqual(result["execution_limits"], dict(zip(("jobs", "test_workers"), expected)))
                    self.assertTrue(result["complete"], result["issues"])
                    self.assertEqual(self.memory.call_count, int(runner_os == "Linux"))
                    self.assertEqual("powershell" in [call.args[0][0] for call in commands.call_args_list], runner_os == "Windows")

    def test_execution_limits_reject_missing_nonfinite_and_unusable_memory(self):
        for memory in (None, True, 0, -1, float("nan"), float("inf"), "private-memory"):
            with self.subTest(memory=type(memory).__name__):
                self.memory.return_value = memory
                result = self.collect()
                self.assertIn("execution_limits", result)
                self.assertEqual(result["execution_limits"], {"jobs": None, "test_workers": None})
                self.assertFalse(result["complete"])
                self.assertIn({"scope": "execution_limits", "code": "invalid_or_unavailable_limits"}, result["issues"])
                self.assertNotIn("private", json.dumps(result))
        for error in (OSError, ValueError, IndexError):
            with self.subTest(error=error.__name__):
                self.memory.side_effect = error("private-memory-error")
                result = self.collect()
                self.assertIn("execution_limits", result)
                self.assertEqual(result["execution_limits"], {"jobs": None, "test_workers": None})
                self.assertFalse(result["complete"])
                self.assertNotIn("private", json.dumps(result))

    def test_execution_limits_reject_malformed_or_unavailable_windows_memory(self):
        for output in (None, b"", b"0\r\n", b"-1\n", b"nan", b"inf", b"12_3", b"1\n2", b"\xff", b"9" * 129,
                       b"private-output"):
            with self.subTest(size=len(output) if output is not None else None):
                self.memory_output = output
                with patch.object(ci_observation, "probe", side_effect=self.probe):
                    result = ci_observation.collect(self.root, "before", environment={**GITHUB, "RUNNER_OS": "Windows"})
                self.assertIn("execution_limits", result)
                self.assertEqual(result["execution_limits"], {"jobs": None, "test_workers": None})
                self.assertFalse(result["complete"])
                self.assertIn({"scope": "execution_limits", "code": "invalid_or_unavailable_limits"}, result["issues"])
                self.assertNotIn("private", json.dumps(result))

    def test_execution_limits_require_exact_bounded_integer_sizing_results(self):
        for helper, invalid in (("jobs_for_memory", (0, 5, True, 1.0, None)),
                                ("test_workers", (0, 7, True, 1.0, None))):
            for value in invalid:
                with self.subTest(helper=helper, value=value), patch(f"verify.{helper}", return_value=value):
                    result = self.collect()
                    self.assertIn("execution_limits", result)
                    self.assertEqual(result["execution_limits"], {"jobs": None, "test_workers": None})
                    self.assertFalse(result["complete"])

    def test_graphical_mode_matches_fixed_verifier_contract_without_display_disclosure(self):
        for runner_os, partition, display, expected in (
            ("Linux", "python", None, {"mode": "xvfb", "screen": "0 1280x1024x24"}),
            ("Linux", "python", "", {"mode": "xvfb", "screen": "0 1280x1024x24"}),
            ("Linux", "python", "private-display", {"mode": "external_unsupported"}),
            ("Windows", "python", None, {"mode": "native"}),
            ("Windows", "rust", None, {"mode": "none"}),
            ("Linux", "rust", "private-display", {"mode": "none"}),
        ):
            with self.subTest(runner_os=runner_os, partition=partition, display_present=display is not None):
                environment = {**GITHUB, "RUNNER_OS": runner_os}
                if display is not None: environment["DISPLAY"] = display
                with patch.object(ci_observation, "probe", side_effect=self.probe) as commands:
                    result = ci_observation.collect(self.root, "before", environment=environment, partition=partition)
                self.assertIn("graphical", result)
                self.assertEqual(result["graphical"], expected)
                self.assertEqual(result["complete"], expected["mode"] != "external_unsupported")
                self.assertNotIn("private-display", json.dumps(result))
                self.assertIn("volatile_display_state", result["omissions"])
                if runner_os == "Windows":
                    self.assertEqual(result["native_packages"], {"status": "not_observed"})
                    self.assertEqual(result["runner"]["image_version"], GITHUB["ImageVersion"])
                    self.assertNotIn("dpkg-query", [call.args[0][0] for call in commands.call_args_list])

    def test_linux_package_metadata_is_complete_sorted_hashed_and_not_exposed(self):
        rows = [["libx11-6", "2:1.8.7-1build1", "amd64", "install ok installed"],
                ["libx11-6", "2:1.8.7-1build1", "i386", "hold ok installed"],
                ["removed", "1", "amd64", "deinstall ok config-files"]]
        def inventory(rows): return "".join("\t".join(row) + "\n" for row in rows).encode()
        self.native_output = inventory(rows)
        result = self.collect(partition="python")
        self.assertIn("native_packages", result)
        self.assertEqual(result["native_packages"], {"status": "present", "count": 3,
            "sha256": hashlib.sha256(json.dumps(sorted(rows), separators=(",", ":")).encode()).hexdigest()})
        self.assertTrue(result["complete"], result["issues"])
        self.assertNotIn("libx11-6", json.dumps(result))
        self.assertIn("package_contents_and_provenance", result["omissions"])
        self.native_output = inventory(list(reversed(rows)))
        self.assertEqual(result["native_packages"], self.collect()["native_packages"])
        original = self.native_output
        for old, new in ((b"libx11-6", b"libx11-7"), (b"1build1", b"1build2"),
                         (b"amd64", b"arm64"), (b"install ok installed", b"hold ok installed")):
            with self.subTest(field=old):
                self.native_output = original.replace(old, new)
                self.assertNotEqual(result["native_packages"]["sha256"], self.collect()["native_packages"]["sha256"])

    def test_package_inventory_bounds_duplicates_and_malformed_rows_fail_closed(self):
        valid = self.native_output
        exact = []
        for number in range(4096):
            line = f"package{number:04}\t\tamd64\tinstall ok installed\n"
            exact.append(line.replace("\t\t", "\t" + "1" * (128 - len(line)) + "\t").encode())
        self.native_output = b"".join(exact)
        self.assertEqual(len(self.native_output), 512 * 1024)
        result = self.collect()
        self.assertIn("native_packages", result)
        self.assertTrue(result["complete"], result["issues"])
        self.assertEqual(result["native_packages"]["count"], 4096)
        oversized = self.native_output + b"\n"
        too_many = b"".join(f"package{n}\t1\tamd64\tinstall ok installed\n".encode() for n in range(4097))
        for output in (None, b"", valid[:-1], valid + valid, valid + valid.replace(b"1build1", b"1build2"),
                       oversized, too_many, b"private-invalid\n", valid.replace(b"amd64", b"private\tarch"),
                       valid.replace(b"libx11-6", b"private/name"), valid.replace(b"amd64", b"\xff"),
                       valid.replace(b"2:1.8.7-1build1", b""), valid.replace(b"install ok installed", b"private-status")):
            with self.subTest(size=len(output) if output is not None else None):
                self.native_output = output
                result = self.collect()
                self.assertFalse(result["complete"])
                self.assertEqual(result["native_packages"], {"status": "unavailable"})
                self.assertIn({"scope": "native_packages", "code": "invalid_or_unavailable_inventory"}, result["issues"])
                self.assertNotIn("private", json.dumps(result))

    def test_residual_and_partial_package_states_and_versions_affect_inventory(self):
        valid = self.native_output
        fingerprints = {self.collect()["native_packages"]["sha256"]}
        for state in ("not-installed", "config-files", "half-installed", "unpacked", "half-configured",
                      "triggers-awaited", "triggers-pending"):
            for version in ("", "1", "2"):
                with self.subTest(state=state, version=version):
                    self.native_output = valid + f"removed\t{version}\tamd64\tdeinstall ok {state}\n".encode()
                    result = self.collect()
                    self.assertIn("native_packages", result)
                    self.assertTrue(result["complete"], result["issues"])
                    self.assertEqual(result["native_packages"]["count"], 2)
                    self.assertNotIn(result["native_packages"]["sha256"], fingerprints)
                    fingerprints.add(result["native_packages"]["sha256"])

    def test_unavailable_environment_values_invalidate_the_fingerprint_without_echoing(self):
        for value in ("secret" * ci_observation.MAX_OUTPUT_BYTES, "é" * ci_observation.MAX_OUTPUT_BYTES, 123, "\ud800"):
            with self.subTest(value_type=type(value).__name__):
                with patch.object(ci_observation, "probe", side_effect=self.probe):
                    result = ci_observation.collect(self.root, "before", environment={**GITHUB, "RUSTFLAGS": value})
                self.assertIn("build_test_environment", result)
                self.assertEqual(result["build_test_environment"]["variables"]["RUSTFLAGS"], {"status": "unavailable"})
                self.assertIsNone(result["build_test_environment"]["sha256"])
                self.assertFalse(result["complete"])
                self.assertIn({"scope": "environment.RUSTFLAGS", "code": "invalid_or_oversized"}, result["issues"])
                self.assertNotIn("secret", json.dumps(result))

    def test_installed_distribution_identity_is_normalized_sorted_metadata_only(self):
        self.distributions.return_value = [SimpleNamespace(metadata={"Name": "NumPy", "Location": "/private"}, version="1.2.3rc1+fixture"),
                                           SimpleNamespace(metadata={"Name": "other_package"}, version="2.0")]
        result = self.collect()
        self.assertIn("distributions", result["tools"]["python"])
        distributions = result["tools"]["python"]["distributions"]
        versions = {"numpy": "1.2.3rc1+fixture", "other-package": "2.0"}
        self.assertEqual(distributions, {"status": "present", "versions": versions,
                                        "sha256": hashlib.sha256(json.dumps(versions, sort_keys=True, separators=(",", ":")).encode()).hexdigest()})
        self.assertTrue(result["complete"], result["issues"])
        self.assertNotIn("/private", json.dumps(result))
        self.distributions.return_value.reverse()
        self.assertEqual(distributions, self.collect()["tools"]["python"]["distributions"])
        self.distributions.return_value[0].version = "2.1"
        self.assertNotEqual(distributions["sha256"], self.collect()["tools"]["python"]["distributions"]["sha256"])

    def test_invalid_or_unreadable_distribution_metadata_is_incomplete_and_not_echoed(self):
        for name, version in (("/private/package", "1"), (None, "1"), ("p" * 129, "1"),
                              ("package", "/private/version"), ("package", "1" * 129), ("package", None)):
            with self.subTest(name_type=type(name).__name__, version_type=type(version).__name__):
                self.distributions.return_value = [SimpleNamespace(metadata={"Name": name}, version=version)]
                result = self.collect()
                self.assertIn("distributions", result["tools"]["python"])
                self.assertEqual(result["tools"]["python"]["distributions"], {"status": "unavailable"})
                self.assertFalse(result["complete"])
                self.assertNotIn("private", json.dumps(result))
        for error in (OSError("private-metadata"), RuntimeError("private-finder")):
            with self.subTest(error=type(error).__name__):
                self.distributions.side_effect = error
                result = self.collect()
                self.assertFalse(result["complete"])
                self.assertEqual(result["tools"]["python"]["distributions"], {"status": "unavailable"})
                self.assertNotIn("private", json.dumps(result))

    def test_duplicate_and_oversized_distribution_inventories_have_no_partial_identity(self):
        self.distributions.return_value = [SimpleNamespace(metadata={"Name": name}, version="1")
                                           for name in ("some_package", "some.package")]
        result = self.collect()
        self.assertIn("distributions", result["tools"]["python"])
        self.assertEqual(result["tools"]["python"]["distributions"], {"status": "unavailable"})
        self.assertFalse(result["complete"])
        self.distributions.return_value = [SimpleNamespace(metadata={"Name": f"package{number}"}, version="1")
                                           for number in range(ci_observation.MAX_DISTRIBUTIONS)]
        result = self.collect()
        self.assertTrue(result["complete"], result["issues"])
        self.assertEqual(len(result["tools"]["python"]["distributions"]["versions"]), ci_observation.MAX_DISTRIBUTIONS)
        yielded = []
        def distributions():
            for number in range(ci_observation.MAX_DISTRIBUTIONS + 1):
                yielded.append(number)
                yield SimpleNamespace(metadata={"Name": f"package{number}"}, version="1")
            self.fail("Inventory enumeration exceeded its bound")
        self.distributions.side_effect = distributions
        result = self.collect()
        self.assertEqual(len(yielded), ci_observation.MAX_DISTRIBUTIONS + 1)
        self.assertEqual(result["tools"]["python"]["distributions"], {"status": "unavailable"})
        self.assertFalse(result["complete"])

    def test_stage_policy_inputs_are_hashed_when_installed_and_optional_when_absent(self):
        names = ("scripts/ci_policy.py", "scripts/ci_coverage_manifest.json", "scripts/ci_shadow.py")
        for name in names:
            (self.root / name).write_bytes(b"stage configuration\n")
        result = self.collect()
        for name in names:
            self.assertIn(name, result["file_hashes"])
            self.assertEqual(result["file_hashes"][name]["sha256"], hashlib.sha256(b"stage configuration\n").hexdigest())
            (self.root / name).unlink()
        result = self.collect()
        self.assertTrue(result["complete"], result["issues"])
        for name in names:
            self.assertEqual(result["file_hashes"][name], {"status": "missing"})

    def test_missing_required_and_optional_files_are_distinguished(self):
        (self.root / "Cargo.lock").unlink()
        (self.root / ".cargo/config.toml").unlink()
        result = self.collect()
        self.assertFalse(result["complete"])
        self.assertEqual(result["file_hashes"]["Cargo.lock"]["status"], "missing")
        self.assertEqual(result["file_hashes"][".cargo/config.toml"]["status"], "missing")
        self.assertNotIn(".cargo/config.toml", [issue["scope"] for issue in result["issues"]])

    def test_symlinked_and_oversized_files_are_not_hashed(self):
        path = self.root / "Cargo.lock"
        path.unlink()
        outside = self.root / "private.txt"
        outside.write_bytes(b"private contents")
        try: path.symlink_to(outside)
        except OSError as error: self.skipTest(f"Host cannot create symlinks: {error}")
        result = self.collect()
        self.assertFalse(result["complete"])
        self.assertEqual(result["file_hashes"]["Cargo.lock"]["status"], "unavailable")
        path.unlink()
        path.write_bytes(b"x" * (ci_observation.MAX_FILE_BYTES + 1))
        result = self.collect()
        self.assertEqual(result["file_hashes"]["Cargo.lock"]["status"], "unavailable")

    def test_dirty_and_untracked_flags_handle_rename_records_without_exposing_names(self):
        self.status = b"R  new-name\0old-name\0?? private-name\0"
        result = self.collect()
        self.assertTrue(result["checkout"]["tracked_dirty"])
        self.assertTrue(result["checkout"]["untracked_present"])
        self.assertNotIn("private-name", json.dumps(result))
        self.assertNotIn("old-name", json.dumps(result))

    def test_malformed_or_missing_status_is_unknown_never_clean(self):
        for status in (None, b" M truncated", b"broken\0", b"R  renamed\0"):
            with self.subTest(status=status):
                self.status = status
                result = self.collect()
                self.assertFalse(result["complete"])
                self.assertIsNone(result["checkout"]["tracked_dirty"])
                self.assertIsNone(result["checkout"]["untracked_present"])

    def test_invalid_head_sha_is_not_used_to_read_an_object(self):
        original = self.probe
        def probe(command, root, issues, scope, **kwargs):
            if "--verify" in command: return b"short-sha\n"
            self.assertNotIn("cat-file", command)
            return original(command, root, issues, scope, **kwargs)
        issues = []
        with patch.object(ci_observation, "probe", side_effect=probe):
            observed = ci_observation.checkout(self.root, issues)
        self.assertIsNone(observed["sha"])
        self.assertIn({"scope": "checkout.commit", "code": "invalid_output"}, issues)

    def test_raw_commit_headers_validate_ids_and_ignore_message_parent_lines(self):
        original = self.probe
        for output in (b"tree short\n\nmessage", f"tree {TREE}\ntree {TREE}\n\nmessage".encode(),
                       f"tree {TREE}\nparent invalid\n\nmessage".encode(), f"tree {TREE}\n".encode()):
            with self.subTest(output=output):
                def probe(command, root, issues, scope, **kwargs):
                    return output if "cat-file" in command else original(command, root, issues, scope, **kwargs)
                with patch.object(ci_observation, "probe", side_effect=probe):
                    observed = ci_observation.checkout(self.root, [])
                self.assertIsNone(observed["sha"])
                self.assertIsNone(observed["tree"])
                self.assertIsNone(observed["parents"])
        def probe(command, root, issues, scope, **kwargs):
            if "cat-file" in command:
                self.assertEqual(command[-1], SHA)
                return f"tree {TREE}\ngpgsig opaque\n parent not-a-real-header\n\nparent misleading-message\n".encode()
            return original(command, root, issues, scope, **kwargs)
        with patch.object(ci_observation, "probe", side_effect=probe):
            observed = ci_observation.checkout(self.root, [])
        self.assertEqual(observed["parents"], [])
        self.assertEqual(observed["tree"], TREE)

    def test_reported_github_sha_does_not_replace_actual_checkout(self):
        with patch.object(ci_observation, "probe", side_effect=self.probe):
            result = ci_observation.collect(self.root, "before", environment={**GITHUB, "GITHUB_SHA": "f" * 40})
        self.assertEqual(result["checkout"]["sha"], SHA)
        self.assertEqual(result["github"]["reported_sha"], "f" * 40)

    def test_invalid_tool_output_is_not_recorded_as_resolved_identity(self):
        original = self.probe
        def probe(command, root, issues, scope, **kwargs):
            return b"secret credential\n" if command[0] in ("rustc", "cargo") else original(command, root, issues, scope, **kwargs)
        with patch.object(ci_observation, "probe", side_effect=probe):
            result = ci_observation.collect(self.root, "before", environment=GITHUB)
        self.assertFalse(result["complete"])
        self.assertIsNone(result["tools"]["rustc"])
        self.assertIsNone(result["tools"]["cargo"])
        self.assertNotIn("secret credential", json.dumps(result))

    def test_labels_are_optional_bounded_and_not_a_fixed_matrix(self):
        result = self.collect(profile="future_profile", partition="new-partition")
        self.assertEqual(result["labels"]["profile"], "future_profile")
        self.assertTrue(result["complete"])
        result = self.collect(profile="secret\nvalue")
        self.assertFalse(result["complete"])
        self.assertIsNone(result["labels"]["profile"])
        self.assertNotIn("secret", json.dumps(result))


class Probes(unittest.TestCase):
    def test_expanded_native_capture_limit_accepts_exact_bound_and_rejects_overflow(self):
        for size in (512 * 1024, 512 * 1024 + 1):
            with self.subTest(size=size):
                process = SimpleNamespace(stdout=io.BytesIO(b"x" * size), returncode=0,
                                          wait=lambda **kwargs: None, poll=lambda: 0, kill=lambda: None)
                issues = []
                with patch.object(ci_observation.subprocess, "Popen", return_value=process):
                    result = ci_observation.probe(["fixture"], Path.cwd(), issues, "test", max_output_bytes=512 * 1024)
                self.assertEqual(result, b"x" * size if size == 512 * 1024 else None)
                self.assertEqual(issues, [] if size == 512 * 1024 else [{"scope": "test", "code": "output_limit"}])

    def test_actual_process_output_is_bounded_and_truncation_is_explicit(self):
        issues = []
        result = ci_observation.probe([sys.executable, "-c", "print('x' * 1000000)"], Path.cwd(), issues, "test")
        self.assertIsNone(result)
        self.assertIn("output_limit", [issue["code"] for issue in issues])

    def test_actual_process_timeout_and_failure_do_not_leak_stderr(self):
        issues = []
        with patch.object(ci_observation, "PROBE_TIMEOUT", 0.05):
            result = ci_observation.probe([sys.executable, "-c", "import time; time.sleep(5)"], Path.cwd(), issues, "test")
        self.assertIsNone(result)
        self.assertIn("timeout", [issue["code"] for issue in issues])
        result = ci_observation.probe([sys.executable, "-c", "import sys; sys.stderr.write('private'); sys.exit(1)"], Path.cwd(), issues, "test")
        self.assertIsNone(result)
        self.assertNotIn("private", json.dumps(issues))

    def test_shallow_checkout_keeps_real_parents_from_the_commit_object(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, clone = root / "source", root / "shallow"
            source.mkdir()
            def git(directory, *args):
                return subprocess.run(["git", *args], cwd=directory, check=True, capture_output=True, text=True).stdout.strip()
            git(source, "init", "-q")
            (source / "tracked.txt").write_text("first\n")
            git(source, "add", "tracked.txt")
            git(source, "-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "parent")
            parent = git(source, "rev-parse", "HEAD")
            (source / "tracked.txt").write_text("second\n")
            git(source, "-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qam", "child")
            expected_sha, expected_tree = git(source, "rev-parse", "HEAD"), git(source, "rev-parse", "HEAD^{tree}")
            git(root, "clone", "--quiet", "--depth", "1", source.as_uri(), str(clone))
            self.assertEqual(git(clone, "rev-parse", "--is-shallow-repository"), "true")
            self.assertEqual(git(clone, "show", "-s", "--format=%P", "HEAD"), "")
            issues = []
            observed = ci_observation.checkout(clone.resolve(), issues)
        self.assertEqual(issues, [])
        self.assertEqual(observed["sha"], expected_sha)
        self.assertEqual(observed["tree"], expected_tree)
        self.assertEqual(observed["parents"], [parent])

    def test_actual_cli_observes_a_real_git_checkout_without_github_metadata(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def git(*args):
                return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True, text=True).stdout.strip()
            git("init", "-q")
            (root / "tracked.txt").write_text("first\n")
            git("add", "tracked.txt")
            git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "fixture")
            expected_sha, expected_tree = git("rev-parse", "HEAD"), git("rev-parse", "HEAD^{tree}")
            (root / "tracked.txt").write_text("changed\n")
            (root / "untracked.txt").write_text("private\n")
            environment = {key: value for key, value in os.environ.items() if not key.startswith("GITHUB_")}
            result = subprocess.run([sys.executable, str(Path(ci_observation.__file__)), "--repo", str(root),
                                     "--phase", "after"], text=True, capture_output=True, env=environment, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["checkout"]["sha"], expected_sha)
        self.assertEqual(report["checkout"]["tree"], expected_tree)
        self.assertEqual(report["checkout"]["parents"], [])
        self.assertTrue(report["checkout"]["tracked_dirty"])
        self.assertTrue(report["checkout"]["untracked_present"])
        self.assertFalse(report["complete"])
        self.assertEqual(report["status"], "untrusted_observation")
        self.assertNotIn("decision", report)


if __name__ == "__main__":
    unittest.main()
