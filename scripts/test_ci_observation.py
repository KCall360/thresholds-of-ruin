"""Bounded, explicitly untrusted observations from a real checkout."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
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

    def probe(self, command, root, issues, scope):
        if "--show-toplevel" in command: return str(root).encode() + b"\n"
        if "--verify" in command: return SHA.encode() + b"\n"
        if "cat-file" in command: return f"tree {TREE}\nparent {PARENT}\nauthor Test <test@example.invalid> 1 +0000\n\nmessage\n".encode()
        if "status" in command: return self.status
        if command[0] == "rustc": return RUSTC
        if command[0] == "cargo": return b"cargo 1.90.0 (840b83a10 2025-07-30)\n"
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
        def probe(command, root, issues, scope):
            if "--verify" in command: return b"short-sha\n"
            self.assertNotIn("cat-file", command)
            return original(command, root, issues, scope)
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
                def probe(command, root, issues, scope):
                    return output if "cat-file" in command else original(command, root, issues, scope)
                with patch.object(ci_observation, "probe", side_effect=probe):
                    observed = ci_observation.checkout(self.root, [])
                self.assertIsNone(observed["sha"])
                self.assertIsNone(observed["tree"])
                self.assertIsNone(observed["parents"])
        def probe(command, root, issues, scope):
            if "cat-file" in command:
                self.assertEqual(command[-1], SHA)
                return f"tree {TREE}\ngpgsig opaque\n parent not-a-real-header\n\nparent misleading-message\n".encode()
            return original(command, root, issues, scope)
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
        def probe(command, root, issues, scope):
            return b"secret credential\n" if command[0] in ("rustc", "cargo") else original(command, root, issues, scope)
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
