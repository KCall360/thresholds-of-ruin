"""Report possible same-tree CI qualification; never authorize skipped checks.

Only GitHub REST run/attempt/job/commit relationships are considered. Uploaded
observations, caches and test artifacts have no authority here. A candidate means
these bounded metadata checks matched, not that clean inputs, per-test coverage
or resolved environments were independently certified. reporting_only and
full_required describe this metadata-only decision: a separate cell consumer
must check artifacts and current inputs before adopting. This is limited to ordinary
same-repository PR merge commits and first-attempt full CI runs.
"""
import argparse
from datetime import datetime, timedelta, timezone
from http.client import HTTPException
import json
import os
import re
import urllib.request

REPOSITORY = "KCall360/thresholds-of-ruin"
REPOSITORY_ID = 1375221902
WORKFLOW_ID = 360984778
PREFIX = f"/repos/{REPOSITORY}"
SHA = re.compile(r"[0-9a-f]{40}\Z")
SOURCE_NAME = re.compile(r"Qualification source \(([0-9a-f]{40})\)\Z")
MATRIX = [(os, profile, part) for os in ("windows-latest", "ubuntu-latest")
          for profile in ("debug", "release") for part in ("rust", "python")
          if (os, profile, part) != ("windows-latest", "release", "python")]
MATRIX += [("windows-latest", "release", f"python shard {index}") for index in (1, 2)]
OTHER_JOBS = {"Repository tooling lint", "Dependencies (advisories)", "Dependencies (bans licenses sources)",
              "Rust (windows-latest)", "Rust (ubuntu-latest)", "Change scope and documentation"}
REQUIRED_CONTROLS = {".github/workflows/ci.yml", "scripts/verify.py", "scripts/test_runner.py",
                     "scripts/parallel_tests.py", "scripts/ci_qualification.py", "scripts/ci_scope.py",
                     "scripts/ci_observation.py", "scripts/ci_adoption.py", "scripts/ci_python_shards.py",
                     "scripts/ci_head_reuse.py", "scripts/ci_reuse_policy.py", "scripts/ci_reuse_registry.json"}
CONTROLS = REQUIRED_CONTROLS | {"scripts/service_contract_ci.py", "scripts/graphical_waiver.py",
                               "scripts/ci_policy.py", "scripts/ci_build_key.py", "scripts/service_contract_host.py",
                               "scripts/process_harness.py", "scripts/ruleset_sources.py"}


class EvidenceError(ValueError):
    """Fixed safe diagnostic, never a raw API response or credential."""


def require(condition, reason):
    if not condition:
        raise EvidenceError(reason)


def timestamp(value):
    require(isinstance(value, str) and value.endswith("Z"), "missing UTC API timestamp")
    return datetime.fromisoformat(value[:-1] + "+00:00")


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise EvidenceError("redirected GitHub API response")


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate API JSON key")
        result[key] = value
    return result


class GitHub:
    """Bounded read-only requests to one API origin; no artifact downloads."""
    def __init__(self, token):
        self.token = token
        self.opener = urllib.request.build_opener(NoRedirect())

    def get(self, path):
        require(path.startswith(PREFIX + "/"), "API request outside this repository")
        request = urllib.request.Request("https://api.github.com" + path, headers={
            "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28",
            "Authorization": "Bearer " + self.token})
        with self.opener.open(request, timeout=15) as response:
            data = response.read(2 * 1024 * 1024 + 1)
        require(len(data) <= 2 * 1024 * 1024, "API response exceeds report budget")
        return json.loads(data, object_pairs_hook=unique_object)


def pages(api, path, key=None):
    rows, total = [], None
    for page in range(1, 4):
        value = api.get(path + ("&" if "?" in path else "?") + f"per_page=100&page={page}")
        batch = value[key] if key else value
        require(isinstance(batch, list) and len(batch) <= 100, "malformed API list")
        if key:
            count = value["total_count"]
            require(type(count) is int and count >= 0 and (total is None or count == total), "unstable API count")
            total = count
        rows.extend(batch)
        if len(batch) < 100:
            require(total is None or len(rows) == total, "incomplete API pagination")
            return rows
    raise EvidenceError("API pagination exceeds report budget")


def run_identity(run, number, event):
    require(run["id"] == number and run["run_attempt"] == 1, "unsupported or changed run attempt")
    require(run["event"] == event and run["workflow_id"] == WORKFLOW_ID
            and run["path"] == ".github/workflows/ci.yml", "unexpected workflow or event")
    require(run["repository"]["id"] == REPOSITORY_ID and run["repository"]["full_name"] == REPOSITORY
            and run["head_repository"]["id"] == REPOSITORY_ID, "run repository mismatch or fork")


def run_snapshot(run):
    # List/run/attempt endpoints need not expose identical incidental metadata.
    return {key: run[key] for key in ("id", "run_attempt", "event", "status", "conclusion", "workflow_id",
                                     "path", "head_sha", "run_started_at", "updated_at", "pull_requests")}


def run_lifecycle(run):
    # Attempt and current-run endpoints can retain different update timestamps.
    # Compare those independently; same-endpoint rechecks retain full snapshots.
    return {key: value for key, value in run_snapshot(run).items() if key != "updated_at"}


def source_interval(run, merged_at, target_created_at, now):
    started, finished = timestamp(run["run_started_at"]), timestamp(run["updated_at"])
    require(started <= finished <= merged_at <= target_created_at <= now
            and now - started <= timedelta(hours=24), "source evidence is stale or outside the merge interval")
    return started, finished


def commit(api, sha):
    require(isinstance(sha, str) and SHA.fullmatch(sha), "invalid commit identity")
    value = api.get(f"{PREFIX}/git/commits/{sha}")
    require(value["sha"] == sha and SHA.fullmatch(value["tree"]["sha"]), "commit identity mismatch")
    return value


def controls(api, tree_sha):
    tree = api.get(f"{PREFIX}/git/trees/{tree_sha}?recursive=1")
    require(tree["sha"] == tree_sha and tree["truncated"] is False, "incomplete Git tree")
    result = {}
    for entry in tree["tree"]:
        path = entry["path"]
        if path in CONTROLS or path.startswith(".github/"):
            if entry["type"] == "tree":
                continue
            require(path not in result and entry["type"] == "blob" and entry["mode"] in ("100644", "100755")
                    and SHA.fullmatch(entry["sha"]), "ambiguous qualification control")
            result[path] = (entry["mode"], entry["sha"])
    require(REQUIRED_CONTROLS <= result.keys(), "missing qualification controls")
    return result


def inspect(api, target_sha, target_run, now):
    report = {"reporting_only": True, "full_required": True, "candidate": False,
              "target_sha": target_sha, "target_run": target_run,
              "limitations": ["Metadata comparison only; never permission to skip a job.",
                              "Clean inputs, individual test completeness and resolved environment equivalence are not certified.",
                              "This reporter cannot change gates; prose-only runs are not full-run evidence."]}
    try:
        require(type(target_run) is int and target_run > 0, "invalid target run identity")
        target = api.get(f"{PREFIX}/actions/runs/{target_run}")
        run_identity(target, target_run, "push")
        require(target["head_sha"] == target_sha and target["head_branch"] == "main", "target is not main push")
        require(api.get(f"{PREFIX}/git/ref/heads/main")["object"]["sha"] == target_sha, "main has advanced")
        merged = commit(api, target_sha)
        parents = [parent["sha"] for parent in merged["parents"]]
        require(len(parents) == 2 and len(set(parents)) == 2 and all(SHA.fullmatch(sha) for sha in parents),
                "only ordinary two-parent merges are supported")
        base, head = parents
        # The PR-head workflow must also be unchanged: a final merge could
        # restore baseline controls after a head workflow fabricated job names.
        baseline = controls(api, commit(api, base)["tree"]["sha"])
        require(controls(api, commit(api, head)["tree"]["sha"]) == baseline
                and controls(api, merged["tree"]["sha"]) == baseline, "qualification controls changed in the PR")
        pulls = pages(api, f"{PREFIX}/commits/{target_sha}/pulls")
        matching = [pull for pull in pulls if pull.get("merge_commit_sha") == target_sha and pull.get("merged_at")]
        require(len(matching) == 1 and type(matching[0]["number"]) is int, "ambiguous merged PR association")
        pull = api.get(f"{PREFIX}/pulls/{matching[0]['number']}")
        require(pull["number"] == matching[0]["number"] and pull["merged"] is True and pull["state"] == "closed"
                and pull["merge_commit_sha"] == target_sha and pull["head"]["sha"] == head
                and pull["base"]["ref"] == "main"
                and pull["head"]["repo"]["id"] == pull["base"]["repo"]["id"] == REPOSITORY_ID,
                "merged PR relationship mismatch")
        source_query = f"{PREFIX}/actions/workflows/{WORKFLOW_ID}/runs?event=pull_request&head_sha={head}"
        runs = pages(api, source_query, "workflow_runs")
        require(len(runs) == 1, "missing or ambiguous source runs")
        source_id = runs[0]["id"]
        require(type(source_id) is int and source_id > 0 and source_id != target_run, "invalid source run identity")
        source = api.get(f"{PREFIX}/actions/runs/{source_id}/attempts/1")
        run_identity(source, source_id, "pull_request")
        run_identity(runs[0], source_id, "pull_request")
        require(run_lifecycle(source) == run_lifecycle(runs[0]), "source run changed during inspection")
        require(source["status"] == "completed" and source["conclusion"] == "success" and source["head_sha"] == head,
                "source run did not pass")
        references = source["pull_requests"]
        # GitHub can erase this association after merge. The merged PR plus the
        # independently resolved synthetic/target parents bind the tree below,
        # but do not independently identify the originating PR when this is empty.
        require(isinstance(references, list) and (not references or (len(references) == 1 and references[0]["number"] == pull["number"]
                and references[0]["base"]["sha"] == base and references[0]["head"]["sha"] == head
                and references[0]["base"]["ref"] == "main"
                and references[0]["base"]["repo"]["id"] == references[0]["head"]["repo"]["id"] == REPOSITORY_ID)),
                "source run has a conflicting PR association")
        merged_at, target_created_at = timestamp(pull["merged_at"]), timestamp(target["created_at"])
        intervals = [source_interval(run, merged_at, target_created_at, now) for run in (source, runs[0])]
        jobs = pages(api, f"{PREFIX}/actions/runs/{source_id}/attempts/1/jobs", "jobs")
        identities = [SOURCE_NAME.fullmatch(job["name"]) for job in jobs if SOURCE_NAME.fullmatch(job["name"])]
        require(len(identities) == 1, "missing or ambiguous GitHub-evaluated source identity")
        synthetic = identities[0][1]
        tested = commit(api, synthetic)
        require([parent["sha"] for parent in tested["parents"]] == parents and tested["tree"] == merged["tree"],
                "actual synthetic source tree or parents differ")
        expected = {f"Rust ({os}, {profile}, {part})" for os, profile, part in MATRIX} | OTHER_JOBS
        expected.add(f"Qualification source ({synthetic})")
        require(len(jobs) == len(expected) and {job["name"] for job in jobs} == expected
                and len({job["id"] for job in jobs}) == len(jobs), "incomplete or duplicate source job inventory")
        job_intervals = []
        for job in jobs:
            require(job["run_id"] == source_id and job["run_attempt"] == 1 and job["head_sha"] == head
                    and job["status"] == "completed" and job["conclusion"] == "success", "source job did not pass")
            job_started, job_finished = timestamp(job["started_at"]), timestamp(job["completed_at"])
            job_intervals.append((job_started, job_finished))
            require(all(started <= job_started <= job_finished <= finished for started, finished in intervals),
                    "job outside source run interval")
            if job["name"].count(",") == 2:
                platform = "windows-latest" if job["name"].startswith("Rust (windows-latest,") else "ubuntu-latest"
                require(platform in job["labels"], "matrix runner label mismatch")
                for name in ("Checkout exact source", "Verify complete profile partition"):
                    steps = [step for step in job["steps"] if step["name"] == name]
                    require(len(steps) == 1 and steps[0]["status"] == "completed" and steps[0]["conclusion"] == "success",
                            "missing or unsuccessful required verification step")
        # Recheck mutable API relationships; artifacts or supplied local claims cannot satisfy these checks.
        latest = api.get(f"{PREFIX}/actions/runs/{source_id}")
        run_identity(latest, source_id, "pull_request")
        attempt = api.get(f"{PREFIX}/actions/runs/{source_id}/attempts/1")
        run_identity(attempt, source_id, "pull_request")
        current_runs = pages(api, source_query, "workflow_runs")
        require(len(current_runs) == 1, "API state changed during inspection")
        run_identity(current_runs[0], source_id, "pull_request")
        require(run_snapshot(current_runs[0]) == run_snapshot(runs[0])
                and run_snapshot(attempt) == run_snapshot(source)
                and run_lifecycle(latest) == run_lifecycle(source)
                and api.get(f"{PREFIX}/git/ref/heads/main")["object"]["sha"] == target_sha, "API state changed during inspection")
        intervals = [source_interval(run, merged_at, target_created_at, now) for run in (latest, attempt, current_runs[0])]
        require(all(started <= job_started <= job_finished <= finished
                    for started, finished in intervals for job_started, job_finished in job_intervals),
                "job outside source run interval")
        report.update(candidate=True, reason="same-tree first-attempt full-run metadata matched",
                      source={"run_id": source_id, "attempt": 1, "checkout_sha": synthetic, "tree": tested["tree"]["sha"],
                              "url": f"https://github.com/{REPOSITORY}/actions/runs/{source_id}/attempts/1"})
    except (OSError, HTTPException, ValueError, KeyError, TypeError, IndexError, AttributeError) as error:
        # HTTP errors and malformed API output must never weaken the normal CI path or expose response bodies/tokens.
        report["reason"] = str(error) if isinstance(error, EvidenceError) else "API evidence unavailable or malformed"
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--run-id", required=True, type=int)
    args = parser.parse_args(argv)
    if os.environ.get("GITHUB_REPOSITORY") != REPOSITORY or not os.environ.get("GH_TOKEN"):
        report = {"reporting_only": True, "full_required": True, "candidate": False,
                  "reason": "report requires this repository's authenticated GitHub Actions context"}
    else:
        report = inspect(GitHub(os.environ["GH_TOKEN"]), args.sha, args.run_id, datetime.now(timezone.utc))
    if report.get("candidate") is True and os.environ.get("GITHUB_OUTPUT"):
        handoff = {key: report[key] for key in ("candidate", "target_sha", "target_run", "source")}
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            output.write("source=" + json.dumps(handoff, separators=(",", ":")) + "\n")
    print(json.dumps(report, indent=2))
    return 0  # Report emission, never an approval to skip or an alternative required check.


if __name__ == "__main__":
    raise SystemExit(main())
