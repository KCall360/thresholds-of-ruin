"""Hostile API fixtures for reporting-only same-tree qualification."""
import copy
from contextlib import redirect_stdout
from datetime import datetime, timezone
from http.client import IncompleteRead
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import ci_qualification as qualification


BASE, HEAD, MERGE, SYNTHETIC, TREE = (character * 40 for character in "abcde")
BASE_TREE = "f" * 40
HEAD_TREE = "3" * 40
REPOSITORY = {"id": 1375221902, "full_name": "KCall360/thresholds-of-ruin"}
PREFIX = "/repos/KCall360/thresholds-of-ruin"
NOW = datetime(2026, 10, 16, 12, 0, tzinfo=timezone.utc)
SOURCE_NAME = f"Qualification source ({SYNTHETIC})"
MATRIX = [(os, profile, part) for os in ("windows-latest", "ubuntu-latest")
          for profile in ("debug", "release") for part in ("rust", "python")]


class ApiFixture:
    def __init__(self):
        self.calls = []
        reference = lambda sha: {"sha": sha, "ref": "main", "repo": REPOSITORY.copy()}
        pull = {"number": 9, "state": "closed", "merged": True, "merged_at": "2026-10-16T11:50:00Z",
                "merge_commit_sha": MERGE, "base": reference(BASE), "head": reference(HEAD)}
        source = {"id": 101, "run_attempt": 1, "event": "pull_request", "status": "completed",
                  "conclusion": "success", "workflow_id": 360984778, "path": ".github/workflows/ci.yml",
                  "repository": REPOSITORY.copy(), "head_repository": REPOSITORY.copy(), "head_sha": HEAD,
                  "run_started_at": "2026-10-16T11:00:00Z", "updated_at": "2026-10-16T11:40:00Z",
                  "pull_requests": [{"number": 9, "base": reference(BASE), "head": reference(HEAD)}]}
        target = {**source, "id": 102, "event": "push", "status": "in_progress", "conclusion": None,
                  "head_sha": MERGE, "head_branch": "main", "created_at": "2026-10-16T11:50:01Z"}
        names = [f"Rust ({os}, {profile}, {part})" for os, profile, part in MATRIX]
        names += ["Repository tooling lint", "Dependencies (advisories)", "Dependencies (bans licenses sources)",
                  "Rust (windows-latest)", "Rust (ubuntu-latest)", "Change scope and documentation", SOURCE_NAME]
        jobs = []
        for index, name in enumerate(names):
            steps = [{"name": title, "status": "completed", "conclusion": "success"} for title in
                     ("Checkout exact source", "Verify complete profile partition")]
            jobs.append({"id": index + 1000, "run_id": 101, "run_attempt": 1, "head_sha": HEAD,
                         "status": "completed", "conclusion": "success", "name": name, "steps": steps,
                         "labels": ["windows-latest" if "windows-latest," in name else "ubuntu-latest"],
                         "started_at": "2026-10-16T11:01:00Z", "completed_at": "2026-10-16T11:39:00Z"})
        paths = (".github/workflows/ci.yml", "scripts/verify.py", "scripts/test_runner.py",
                 "scripts/parallel_tests.py", "scripts/ci_qualification.py", "scripts/ci_scope.py", "scripts/ci_observation.py", "scripts/ci_adoption.py")
        tree = {"sha": TREE, "truncated": False,
                "tree": [{"path": path, "type": "blob", "mode": "100644", "sha": "1" * 40} for path in paths]}
        self.data = {
            f"{PREFIX}/actions/runs/102": target,
            f"{PREFIX}/git/ref/heads/main": {"object": {"sha": MERGE}},
            f"{PREFIX}/git/commits/{MERGE}": {"sha": MERGE, "tree": {"sha": TREE},
                                              "parents": [{"sha": BASE}, {"sha": HEAD}]},
            f"{PREFIX}/git/commits/{BASE}": {"sha": BASE, "tree": {"sha": BASE_TREE}},
            f"{PREFIX}/git/commits/{HEAD}": {"sha": HEAD, "tree": {"sha": HEAD_TREE}},
            f"{PREFIX}/git/commits/{SYNTHETIC}": {"sha": SYNTHETIC, "tree": {"sha": TREE},
                                                  "parents": [{"sha": BASE}, {"sha": HEAD}]},
            f"{PREFIX}/commits/{MERGE}/pulls?per_page=100&page=1": [pull],
            f"{PREFIX}/pulls/9": pull,
            f"{PREFIX}/actions/workflows/360984778/runs?event=pull_request&head_sha={HEAD}&per_page=100&page=1":
                {"total_count": 1, "workflow_runs": [source]},
            f"{PREFIX}/actions/runs/101/attempts/1": source,
            f"{PREFIX}/actions/runs/101": source,
            f"{PREFIX}/actions/runs/101/attempts/1/jobs?per_page=100&page=1": {"total_count": len(jobs), "jobs": jobs},
            f"{PREFIX}/git/trees/{TREE}?recursive=1": tree,
            f"{PREFIX}/git/trees/{BASE_TREE}?recursive=1": {**copy.deepcopy(tree), "sha": BASE_TREE},
            f"{PREFIX}/git/trees/{HEAD_TREE}?recursive=1": {**copy.deepcopy(tree), "sha": HEAD_TREE},
        }

    def get(self, path):
        self.calls.append(path)
        result = self.data[path]
        if isinstance(result, Exception):
            raise result
        return copy.deepcopy(result)

    def source(self):
        return self.data[f"{PREFIX}/actions/runs/101/attempts/1"]

    def jobs(self):
        return self.data[f"{PREFIX}/actions/runs/101/attempts/1/jobs?per_page=100&page=1"]


class QualificationDecisions(unittest.TestCase):
    def setUp(self):
        self.api = ApiFixture()

    def report(self):
        result = qualification.inspect(self.api, MERGE, 102, NOW)
        self.assertIs(result["reporting_only"], True)
        self.assertIs(result["full_required"], True)
        return result

    def rejects(self):
        report = self.report()
        self.assertIs(report["candidate"], False, report)
        self.assertTrue(report["reason"])

    def test_exact_synthetic_tree_reports_a_candidate_but_still_requires_full(self):
        result = self.report()
        self.assertIs(result["candidate"], True, result)
        self.assertEqual(result["source"], {"run_id": 101, "attempt": 1, "checkout_sha": SYNTHETIC,
                                          "tree": TREE, "url": "https://github.com/KCall360/thresholds-of-ruin/actions/runs/101/attempts/1"})

    def test_equal_head_is_insufficient_when_tested_merge_tree_differs(self):
        self.api.data[f"{PREFIX}/git/commits/{SYNTHETIC}"]["tree"]["sha"] = "2" * 40
        self.rejects()

    def test_documentation_only_jobs_never_qualify_as_a_full_run(self):
        for shape in ("expanded_skips", "unexpanded_skips"):
            with self.subTest(shape=shape):
                self.api = ApiFixture()
                inventory = self.api.jobs()
                for job in inventory["jobs"]:
                    if job["name"].count(",") == 2 or job["name"].startswith("Dependencies ("):
                        job["conclusion"] = "skipped"
                if shape == "unexpanded_skips":
                    inventory["jobs"] = [job for job in inventory["jobs"] if job["conclusion"] != "skipped"]
                    inventory["total_count"] = len(inventory["jobs"])
                self.rejects()

    def test_missing_synthetic_identity_never_falls_back_to_head(self):
        self.api.jobs()["jobs"][-1]["name"] = "Qualification source"
        self.rejects()

    def test_source_and_target_parents_must_match_exactly(self):
        for sha in (SYNTHETIC, MERGE):
            with self.subTest(sha=sha):
                self.api = ApiFixture()
                self.api.data[f"{PREFIX}/git/commits/{sha}"]["parents"][0]["sha"] = "2" * 40
                self.rejects()

    def test_missing_failed_cancelled_skipped_duplicate_and_wrong_attempt_cells_reject(self):
        for variation in ("missing", "failure", "cancelled", "skipped", "duplicate", "attempt", "run", "head", "label"):
            with self.subTest(variation=variation):
                self.api = ApiFixture()
                jobs = self.api.jobs()["jobs"]
                if variation == "missing": jobs.pop(0)
                elif variation == "duplicate": jobs[-2] = copy.deepcopy(jobs[0])
                elif variation in ("failure", "cancelled", "skipped"): jobs[0]["conclusion"] = variation
                elif variation == "attempt": jobs[0]["run_attempt"] = 2
                elif variation == "run": jobs[0]["run_id"] = 99
                elif variation == "head": jobs[0]["head_sha"] = "2" * 40
                else: jobs[0]["labels"] = ["ubuntu-latest"]
                self.rejects()

    def test_successful_job_cannot_hide_failed_or_absent_verification_step(self):
        for variation in ("missing", "failure", "skipped", "duplicate"):
            with self.subTest(variation=variation):
                self.api = ApiFixture()
                steps = self.api.jobs()["jobs"][0]["steps"]
                if variation == "missing": steps.pop()
                elif variation == "duplicate": steps.append(copy.deepcopy(steps[-1]))
                else: steps[-1]["conclusion"] = variation
                self.rejects()

    def test_forks_wrong_workflow_wrong_event_reruns_and_wrong_head_reject(self):
        changes = [("head_repository", {"id": 7}), ("workflow_id", 99), ("event", "workflow_dispatch"),
                   ("path", ".github/workflows/other.yml"), ("run_attempt", 2), ("head_sha", "2" * 40)]
        for key, value in changes:
            with self.subTest(key=key):
                self.api = ApiFixture()
                self.api.source()[key] = value
                self.rejects()

    def test_reversed_future_and_after_merge_source_runs_reject(self):
        for value in ("2026-10-16T10:40:00Z", "2026-10-16T12:01:00Z", "2026-10-16T11:51:00Z"):
            with self.subTest(value=value):
                self.api = ApiFixture()
                self.api.source()["updated_at"] = value
                self.rejects()

    def test_consistent_old_success_is_rejected_by_freshness_guard(self):
        source = self.api.source()
        source["run_started_at"] = "2026-10-14T11:00:00Z"
        source["updated_at"] = "2026-10-14T11:40:00Z"
        for job in self.api.jobs()["jobs"]:
            job["started_at"] = "2026-10-14T11:01:00Z"
            job["completed_at"] = "2026-10-14T11:39:00Z"
        result = self.report()
        self.assertFalse(result["candidate"])
        self.assertEqual(result["reason"], "source evidence is stale or outside the merge interval")

    def test_superseded_main_unsupported_merge_and_wrong_pr_association_reject(self):
        for variation in ("tip", "squash", "pull"):
            with self.subTest(variation=variation):
                self.api = ApiFixture()
                if variation == "tip": self.api.data[f"{PREFIX}/git/ref/heads/main"]["object"]["sha"] = HEAD
                elif variation == "squash": self.api.data[f"{PREFIX}/git/commits/{MERGE}"]["parents"].pop()
                else: self.api.source()["pull_requests"][0]["number"] = 10
                self.rejects()

    def test_modified_head_workflow_cannot_be_hidden_by_restoring_final_merge(self):
        for associations in ([], self.api.source()["pull_requests"]):
            with self.subTest(associations=bool(associations)):
                self.api = ApiFixture()
                self.api.source()["pull_requests"] = associations
                self.api.data[f"{PREFIX}/git/trees/{HEAD_TREE}?recursive=1"]["tree"][0]["sha"] = "2" * 40
                self.rejects()

    def test_executable_mode_changes_are_qualification_control_changes(self):
        for tree in (TREE, HEAD_TREE):
            with self.subTest(tree=tree):
                self.api = ApiFixture()
                self.api.data[f"{PREFIX}/git/trees/{tree}?recursive=1"]["tree"][0]["mode"] = "100755"
                self.rejects()

    def test_changed_missing_or_symlinked_qualification_controls_reject(self):
        for variation in ("changed", "missing", "symlink", "new"):
            with self.subTest(variation=variation):
                self.api = ApiFixture()
                tree = self.api.data[f"{PREFIX}/git/trees/{TREE}?recursive=1"]["tree"]
                if variation == "changed": tree[0]["sha"] = "2" * 40
                elif variation == "missing": tree.pop()
                elif variation == "symlink": tree[0]["mode"] = "120000"
                else: tree.append({"path": ".github/workflows/extra.yml", "type": "blob", "mode": "100644", "sha": "2" * 40})
                self.rejects()

    def test_partial_api_lists_and_truncated_git_trees_reject(self):
        for variation in ("jobs", "tree"):
            with self.subTest(variation=variation):
                self.api = ApiFixture()
                if variation == "jobs": self.api.jobs()["total_count"] += 1
                else: self.api.data[f"{PREFIX}/git/trees/{TREE}?recursive=1"]["truncated"] = True
                self.rejects()

    def test_api_failure_and_malformed_responses_fall_back_without_raising(self):
        for value in (OSError("API unavailable"), IncompleteRead(b"truncated"), {}, None):
            with self.subTest(value=value):
                self.api = ApiFixture()
                self.api.data[f"{PREFIX}/actions/runs/102"] = value
                self.rejects()

    def test_uploaded_observations_are_not_requested_or_used(self):
        self.api.data["forged-observations"] = {"complete": True, "candidate": True, "tree": TREE}
        self.api.jobs()["jobs"][0]["conclusion"] = "skipped"
        self.rejects()
        self.assertTrue(all("artifacts" not in path and "observations" not in path for path in self.api.calls))

    def test_rerun_started_after_first_read_is_not_hidden_by_old_attempt_endpoint(self):
        get = self.api.get
        def racing_get(path):
            value = get(path)
            if path == f"{PREFIX}/actions/runs/101":
                value["run_attempt"] = 2
            return value
        self.api.get = racing_get
        self.rejects()

    def test_endpoint_specific_incidental_fields_do_not_create_false_mismatches(self):
        self.api.data[f"{PREFIX}/actions/runs/101/attempts/1"] = {**self.api.source(), "incidental": "attempt only"}
        self.assertTrue(self.report()["candidate"])

    def test_untrusted_api_error_text_is_not_reported(self):
        self.api.data[f"{PREFIX}/actions/runs/102"] = ValueError("secret diagnostic sentinel")
        self.assertNotIn("sentinel", json.dumps(self.report()))

    def test_new_unknown_job_is_not_silently_ignored(self):
        self.api.jobs()["jobs"].append({**self.api.jobs()["jobs"][0], "id": 9999, "name": "New qualification"})
        self.api.jobs()["total_count"] += 1
        self.rejects()

    def test_empty_run_pr_association_uses_independent_merge_and_synthetic_parents(self):
        self.api.source()["pull_requests"] = []
        self.assertTrue(self.report()["candidate"])
        self.api.data[f"{PREFIX}/git/commits/{SYNTHETIC}"]["parents"][0]["sha"] = "2" * 40
        self.rejects()

    def test_success_filter_cannot_hide_a_conflicting_or_newer_source_run(self):
        path = f"{PREFIX}/actions/workflows/360984778/runs?event=pull_request&head_sha={HEAD}&per_page=100&page=1"
        for state in ("failure", "cancelled", "in_progress", "success"):
            with self.subTest(state=state):
                self.api = ApiFixture()
                self.api.data[path]["workflow_runs"].append({**self.api.source(), "id": 103, "conclusion": state})
                self.api.data[path]["total_count"] += 1
                self.rejects()

    def test_new_source_run_during_inspection_invalidates_candidate(self):
        get, reads = self.api.get, 0
        def racing_get(path):
            nonlocal reads
            value = get(path)
            if "/actions/workflows/" in path:
                reads += 1
                if reads == 2:
                    value["workflow_runs"].append({**self.api.source(), "id": 103})
                    value["total_count"] += 1
            return value
        self.api.get = racing_get
        self.rejects()


class ApiBoundary(unittest.TestCase):
    def test_real_cli_process_without_actions_context_emits_safe_negative_report(self):
        environment = {key: value for key, value in os.environ.items() if key not in ("GH_TOKEN", "GITHUB_REPOSITORY")}
        result = subprocess.run([sys.executable, qualification.__file__, "--sha", MERGE, "--run-id", "102"],
                                env=environment, capture_output=True, text=True, timeout=10, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertIs(report["candidate"], False)
        self.assertIs(report["full_required"], True)
        self.assertIs(report["reporting_only"], True)

    def test_reader_limits_origin_response_size_time_and_redirects(self):
        client = qualification.GitHub("credential sentinel")
        with mock.patch.object(client.opener, "open") as opened:
            opened.return_value.__enter__.return_value.read.return_value = b'{"ok":true}'
            self.assertEqual(client.get(PREFIX + "/actions/runs/1"), {"ok": True})
            request = opened.call_args.args[0]
            self.assertEqual(request.full_url, "https://api.github.com" + PREFIX + "/actions/runs/1")
            self.assertEqual(request.get_method(), "GET")
            self.assertEqual(opened.call_args.kwargs["timeout"], 15)
            opened.return_value.__enter__.return_value.read.assert_called_once_with(2 * 1024 * 1024 + 1)
            opened.reset_mock()
            with self.assertRaises(qualification.EvidenceError):
                client.get("https://other.example/repo")
            opened.assert_not_called()
        with self.assertRaisesRegex(qualification.EvidenceError, "redirected"):
            qualification.NoRedirect().redirect_request(None, None, 302, "redirect", {}, "https://other.example/")

    def test_reader_rejects_oversized_duplicate_and_malformed_json(self):
        client = qualification.GitHub("credential sentinel")
        for body in (b"x" * (2 * 1024 * 1024 + 1), b'{"result":true,"result":false}', b"invalid"):
            with self.subTest(size=len(body)), mock.patch.object(client.opener, "open") as opened:
                opened.return_value.__enter__.return_value.read.return_value = body
                with self.assertRaises(ValueError):
                    client.get(PREFIX + "/actions/runs/1")

    def test_pagination_collects_complete_lists_and_bounds_excessive_results(self):
        api = mock.Mock()
        api.get.side_effect = [{"jobs": list(range(100)), "total_count": 101}, {"jobs": [100], "total_count": 101}]
        self.assertEqual(qualification.pages(api, PREFIX + "/jobs", "jobs"), list(range(101)))
        self.assertEqual(api.get.call_args.args[0], PREFIX + "/jobs?per_page=100&page=2")
        api = mock.Mock()
        api.get.return_value = {"jobs": list(range(100)), "total_count": 400}
        with self.assertRaisesRegex(qualification.EvidenceError, "budget"):
            qualification.pages(api, PREFIX + "/jobs", "jobs")
        self.assertEqual(api.get.call_count, 3)

    def test_only_validated_candidate_exports_target_bound_handoff(self):
        api = ApiFixture()
        report = qualification.inspect(api, MERGE, 102, NOW)
        self.assertEqual(report['target_run'], 102)
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'github-output'
            for candidate in (False, True):
                with self.subTest(candidate=candidate), mock.patch.dict(os.environ, {
                        'GITHUB_REPOSITORY': qualification.REPOSITORY, 'GH_TOKEN': 'fixture', 'GITHUB_OUTPUT': str(output)}), \
                        mock.patch.object(qualification, 'inspect', return_value={**report, 'candidate': candidate}), \
                        redirect_stdout(io.StringIO()):
                    self.assertEqual(qualification.main(['--sha', MERGE, '--run-id', '102']), 0)
                if not candidate:
                    self.assertFalse(output.exists())
                else:
                    line, = output.read_text().splitlines()
                    self.assertTrue(line.startswith('source='))
                    handoff = json.loads(line[len('source='):])
                    self.assertEqual(handoff, {key: report[key] for key in ('candidate', 'target_sha', 'target_run', 'source')})

    def test_cli_without_context_emits_full_required_and_never_calls_network(self):
        output = io.StringIO()
        with mock.patch.dict(os.environ, {}, clear=True), mock.patch.object(qualification.GitHub, "get") as get, redirect_stdout(output):
            self.assertEqual(qualification.main(["--sha", MERGE, "--run-id", "102"]), 0)
        report = json.loads(output.getvalue())
        self.assertFalse(report["candidate"])
        self.assertTrue(report["full_required"])
        get.assert_not_called()

    def test_cli_api_failure_emits_report_without_exposing_credentials(self):
        output = io.StringIO()
        with (mock.patch.dict(os.environ, {"GITHUB_REPOSITORY": qualification.REPOSITORY, "GH_TOKEN": "credential sentinel"}),
              mock.patch.object(qualification.GitHub, "get", side_effect=OSError("credential sentinel")), redirect_stdout(output)):
            self.assertEqual(qualification.main(["--sha", MERGE, "--run-id", "102"]), 0)
        report = json.loads(output.getvalue())
        self.assertFalse(report["candidate"])
        self.assertTrue(report["full_required"])
        self.assertNotIn("sentinel", output.getvalue())


class ReportingWorkflow(unittest.TestCase):
    def setUp(self):
        self.workflow = (Path(__file__).resolve().parents[1] / ".github/workflows/ci.yml").read_text()

    def test_every_checkout_explicitly_uses_the_github_evaluated_source_sha(self):
        checkouts = [block for block in re.findall(r"(?ms)^      - .*?(?=^      - |\Z)", self.workflow)
                     if "uses: actions/checkout@" in block]
        self.assertGreaterEqual(len(checkouts), 4)
        for block in checkouts:
            self.assertIn("name: Checkout exact source", block)
            self.assertIn("          ref: ${{ github.sha }}\n", block)
            self.assertIn("          persist-credentials: false\n", block)

    def test_source_identity_is_github_evaluated_and_handoff_is_separately_validated(self):
        block = self.workflow.split("  source_identity:\n", 1)[-1]
        self.assertIn("    name: Qualification source (${{ github.sha }})\n", block)
        self.assertIn("    continue-on-error: true\n", block)
        self.assertIn("      actions: read\n", block)
        self.assertIn("python scripts/ci_qualification.py", block)
        self.assertIn("source: ${{ steps.qualification.outputs.source }}", block)
        profiles = re.search(r"(?ms)^  profiles:\n(.*?)(?=^  [a-zA-Z_]+:|\Z)", self.workflow)[1]
        self.assertIn("needs: [scope, source_identity]", profiles)
        self.assertIn("python scripts/ci_adoption.py", profiles)
        aggregate = re.search(r"(?ms)^  check:\n(.*?)(?=^  [a-zA-Z_]+:|\Z)", self.workflow)[1]
        self.assertNotIn("source_identity", aggregate)
        self.assertNotIn("ci_qualification", aggregate)
        self.assertIn('    - cron: "17 6 * * 1"', self.workflow)

    def test_report_runs_only_on_main_push_without_changing_required_full_command(self):
        block = self.workflow.split("  source_identity:\n", 1)[-1]
        self.assertIn("        if: github.event_name == 'push' && github.ref == 'refs/heads/main'\n", block)
        self.assertIn("run: python scripts/verify.py full --ci-profile ${{ matrix.profile }} --ci-partition ${{ matrix.partition }}", self.workflow)


if __name__ == "__main__":
    unittest.main()
