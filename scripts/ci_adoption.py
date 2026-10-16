"""Optionally reuse one exact-tree premerge cell; missing evidence means full CI.

This consumer trusts reviewed CI/test code like a fresh run. Observations describe
bounded inputs, not adversarial attestation or every volatile machine detail.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import io
import json
import math
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile

import ci_observation
import verify
from ci_qualification import EvidenceError, GitHub, PREFIX, REPOSITORY, REPOSITORY_ID, pages, require, run_identity, run_snapshot, timestamp, unique_object

MAX_ARCHIVE = 32 * 1024 * 1024
MAX_RECORD = 16 * 1024 * 1024
MAX_TOTAL = 128 * 1024 * 1024


class NoFollow(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def remaining(deadline):
    seconds = deadline - time.monotonic()
    require(seconds > 0, "artifact deadline exceeded")
    return min(15, seconds)


def read_bounded(response, limit, deadline):
    data = bytearray()
    while True:
        remaining(deadline)
        chunk = response.read1(min(65536, limit + 1 - len(data)))
        remaining(deadline)
        if not chunk:
            return bytes(data)
        data.extend(chunk)
        require(len(data) <= limit, "artifact byte budget exceeded")


def download_artifact(api, artifact, *, deadline):
    """API credentials are used on one fixed origin, never on storage requests."""
    remaining(deadline)
    identity, size, digest = artifact["id"], artifact["size_in_bytes"], artifact["digest"]
    require(type(identity) is int and identity > 0 and type(size) is int and 0 < size <= MAX_ARCHIVE
            and isinstance(digest, str) and re.fullmatch(r"sha256:[0-9a-f]{64}", digest), "invalid artifact metadata")
    request = urllib.request.Request(f"https://api.github.com{PREFIX}/actions/artifacts/{identity}/zip", headers={
        "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28",
        "Authorization": "Bearer " + api.token})
    try:
        with urllib.request.build_opener(NoFollow()).open(request, timeout=remaining(deadline)):
            raise EvidenceError("artifact API did not redirect")
    except urllib.error.HTTPError as error:
        try:
            require(error.code == 302, "artifact API download unavailable")
            location = error.headers.get("Location", "")
        finally:
            error.close()
    url = urllib.parse.urlsplit(location)
    require(url.scheme == "https" and url.hostname and url.username is None and url.password is None
            and url.port in (None, 443) and not url.fragment
            and (url.hostname == "results-receiver.actions.githubusercontent.com"
                 or url.hostname.endswith(".blob.core.windows.net")), "unexpected artifact storage origin")
    storage = urllib.request.Request(location, headers={"Accept-Encoding": "identity"})
    with urllib.request.build_opener(NoFollow()).open(storage, timeout=remaining(deadline)) as response:
        require(response.status == 200 and response.headers.get("Content-Encoding", "identity") == "identity",
                "unexpected artifact storage response")
        length = response.headers.get("Content-Length")
        require(length is None or length == str(size), "artifact transport length mismatch")
        data = read_bounded(response, size, deadline)
    require(len(data) == size and hashlib.sha256(data).hexdigest() == digest[7:], "artifact length or digest mismatch")
    remaining(deadline)
    return data


def artifact_records(data, *, deadline):
    """Read bounded regular members into memory; never extract archive paths."""
    remaining(deadline)
    require(0 < len(data) <= MAX_ARCHIVE, "artifact archive budget exceeded")
    records, seen = {}, set()
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        remaining(deadline)
        require(0 < len(entries) <= 64 and sum(row.file_size for row in entries) <= MAX_TOTAL,
                "artifact member budget exceeded")
        for row in entries:
            remaining(deadline)
            name = row.filename
            parts = name.split("/")
            require(row.orig_filename == name and re.fullmatch(r"[A-Za-z0-9_./-]{1,512}", name)
                    and all(part not in ("", ".", "..") for part in parts)
                    and name.casefold() not in seen, "ambiguous artifact member path")
            seen.add(name.casefold())
            mode = stat.S_IFMT(row.external_attr >> 16)
            require(not row.flag_bits & 1 and row.compress_type in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED)
                    and not row.is_dir() and mode in (0, stat.S_IFREG), "unsupported artifact member")
            limit = 2 * 1024 * 1024 if name.endswith(".json") else MAX_RECORD
            require(row.file_size <= limit, "artifact record budget exceeded")
            with archive.open(row) as source:
                value = read_bounded(source, limit, deadline)
            require(len(value) == row.file_size, "artifact member length mismatch")
            records[name] = value
    remaining(deadline)
    return records


def bounded_worker(command, *, timeout=90):
    """Supervise the one Python-only worker; cancellation is never an adoption."""
    with tempfile.TemporaryFile() as output:
        try:
            result = subprocess.run(command, stdin=subprocess.DEVNULL, stdout=output, stderr=subprocess.DEVNULL,
                                    timeout=timeout, check=False)
        except (OSError, subprocess.TimeoutExpired):
            return False
        if result.returncode in (-2, 130, -15, 143):
            raise SystemExit(130 if result.returncode in (-2, 130) else 143)
        output.seek(0)
        return result.returncode == 0 and output.read(9) == b"adopted\n"


def document(raw):
    value = json.loads(raw.decode("utf-8"), object_pairs_hook=unique_object,
                       parse_constant=lambda _: require(False, "nonfinite artifact JSON"),
                       parse_float=lambda value: float(value) if math.isfinite(float(value)) else require(False, "nonfinite artifact JSON"))
    require(isinstance(value, dict), "artifact JSON is not an object")
    return value


def validate_cell(records, identity, job, target, *, profile, partition, jobs, workers):
    """Pure comparison under reviewed CI trust; caller owns API/cache eligibility."""
    require(profile in ("debug", "release") and partition in ("rust", "python")
            and type(jobs) is int and 1 <= jobs <= 4 and type(workers) is int and 1 <= workers <= 6, "unsupported cell sizing")
    require(target["execution_limits"] == {"jobs": jobs, "test_workers": workers}
            and all(type(value) is int for value in target["execution_limits"].values()), "target sizing mismatch")
    source = identity["source"]
    before, after = (document(records[f"observations/{phase}.json"]) for phase in ("before", "after"))
    platform = {"Linux": "ubuntu-latest", "Windows": "windows-latest"}[target["runner"]["os"]]
    require(identity["candidate"] is True and source["attempt"] == 1
            and job["name"] == f"Rust ({platform}, {profile}, {partition})"
            and job["run_id"] == source["run_id"] and job["run_attempt"] == 1
            and job["status"] == "completed" and job["conclusion"] == "success", "source cell identity mismatch")
    for observation, phase in ((before, "before"), (after, "after"), (target, "before")):
        require(type(observation["schema_version"]) is int and observation["schema_version"] == 1
                and observation["status"] == "untrusted_observation" and observation["complete"] is True
                and observation["issues"] == [] and observation["phase"] == phase
                and observation["labels"] == {"profile": profile, "partition": partition}
                and observation["checkout"]["tracked_dirty"] is False
                and observation["checkout"]["untracked_present"] is False, "incomplete or dirty observation")
    stable = ("checkout", "github", "runner", "tools", "file_hashes", "build_test_environment", "native_packages", "graphical")
    require(all(before[key] == after[key] for key in stable), "source inputs changed during verification")
    require(all(before[key] == target[key] for key in stable if key not in ("checkout", "github")), "resolved inputs differ")
    require(before["checkout"]["sha"] == source["checkout_sha"] and before["checkout"]["tree"] == source["tree"]
            and target["checkout"]["sha"] == identity["target_sha"] and target["checkout"]["tree"] == source["tree"]
            and before["checkout"]["parents"] == target["checkout"]["parents"]
            and len(before["checkout"]["parents"]) == 2 and before["checkout"]["parents"][1] == job["head_sha"], "checkout binding mismatch")
    for observation, event, sha in ((before, "pull_request", source["checkout_sha"]), (target, "push", identity["target_sha"])):
        github = observation["github"]
        require(github["repository"] == REPOSITORY and github["job"] == "profiles" and github["event"] == event
                and github["reported_sha"] == github["workflow_sha"] == sha
                and type(github["run_attempt"]) is int and github["run_attempt"] == 1
                and type(github["run_id"]) is int and github["run_id"] > 0
                and github["workflow_ref"] == f"{REPOSITORY}/.github/workflows/ci.yml@{github['ref']}", "workflow observation mismatch")
    require(before["github"]["run_id"] == source["run_id"] and re.fullmatch(r"refs/pull/[1-9][0-9]*/merge", before["github"]["ref"])
            and target["github"]["ref"] == "refs/heads/main" and target["github"]["run_id"] == identity["target_run"]
            and target["github"]["run_id"] != source["run_id"], "run observation mismatch")
    require(timestamp(job["started_at"]) <= timestamp(before["recorded_at"]) <= timestamp(after["recorded_at"])
            <= timestamp(job["completed_at"]), "observations outside source job")
    require(set(target["file_hashes"]) == set(ci_observation.CONFIG_FILES)
            and target["build_test_environment"] == ci_observation.build_test_environment(os.environ, []), "current configuration mismatch")
    require(all(target["build_test_environment"]["variables"][name] == {"status": "absent"}
                for name in ("TOR_TEST_BINARIES", "TOR_SCENARIO_COMPILER", "TOR_LOCAL_MOUSE_WAIVER")), "unsupported external runtime or waiver")
    native = target["native_packages"]
    require((platform == "windows-latest" and native == {"status": "not_observed"}) or
            (platform == "ubuntu-latest" and native["status"] == "present" and type(native["count"]) is int
             and 0 < native["count"] <= ci_observation.MAX_NATIVE_PACKAGES and re.fullmatch(r"[0-9a-f]{64}", native["sha256"])), "native context unavailable")
    graphical_issues = []
    require(target["graphical"] == ci_observation.graphical_mode(target["runner"]["os"], partition, os.environ, graphical_issues)
            and not graphical_issues, "unsupported graphical context")
    names = [name for name in records if name.endswith("/summary.json")]
    require(len(names) == 1 and re.fullmatch(rf"[^/]+-full-ci-{profile}-{partition}/summary.json", names[0]), "ambiguous full cell summary")
    summary, prefix = document(records[names[0]]), names[0].rsplit("/", 1)[0]
    require(type(summary["schema_version"]) is int and summary["schema_version"] == 1 and summary["tier"] == "full" and summary["scope"] == "ci-partition"
            and summary["ci_profile"] == profile and summary["ci_partition"] == partition
            and summary["clean_passed"] is True and summary["gate_passed"] is True and summary["graphical_waiver"] is None
            and summary["waived_steps"] == summary["not_run_steps"] == []
            and type(summary["jobs"]) is int and summary["jobs"] == jobs
            and type(summary["test_workers"]) is int and summary["test_workers"] == workers, "incomplete full cell summary")
    plan = verify.plan("full", set(), [], xvfb=target["graphical"]["mode"] == "xvfb",
                       ci_profile=profile, ci_partition=partition, workers=workers)
    require(summary["planned_steps"] == [name for name, _, _ in plan]
            == [row["step"] for row in summary["steps"]], "planned stage mismatch")
    with tempfile.TemporaryDirectory() as directory:
        copied = []
        for (name, command, overrides), row in zip(plan, summary["steps"]):
            environment = ci_observation.build_test_environment({**os.environ, **verify.verification_environment(
                jobs, {key: value for key, value in overrides.items() if not key.startswith("_")})}, [])
            environment["scope"] = "verifier_stage"
            require(row["status"] == "passed" and row["environment"] == environment, "stage outcome or effective environment mismatch")
            filename = f"{name}-tests.jsonl" if verify.python_test_prefix(command) is not None else f"{name}.log"
            path = Path(directory) / filename
            path.write_bytes(records[f"{prefix}/{filename}"])
            copied.append({**row, "log": str(path)})
        complete = verify.receipt_completeness(plan, copied, directory, jobs)
    require(complete["complete"] is True and json.dumps(complete, sort_keys=True) ==
            json.dumps(summary["receipt_completeness"], sort_keys=True), "incomplete or changed invocation evidence")


def source_artifact(api, identity, platform, profile, partition, *, now):
    """Bind the unique artifact to an actual successful first-attempt API job."""
    source = identity['source']
    require(identity['candidate'] is True and source['attempt'] == 1, 'source metadata is ineligible')
    run = api.get(f"{PREFIX}/actions/runs/{source['run_id']}")
    run_identity(run, source['run_id'], 'pull_request')
    require(run['status'] == 'completed' and run['conclusion'] == 'success', 'source run no longer successful')
    jobs = pages(api, f"{PREFIX}/actions/runs/{source['run_id']}/attempts/1/jobs", 'jobs')
    selected = [job for job in jobs if job['name'] == f'Rust ({platform}, {profile}, {partition})']
    require(len(selected) == 1, 'missing or duplicate source cell')
    job = selected[0]
    require(job['run_id'] == source['run_id'] and job['run_attempt'] == 1 and job['head_sha'] == run['head_sha']
            and job['status'] == 'completed' and job['conclusion'] == 'success', 'source job relationship mismatch')
    artifacts = pages(api, f"{PREFIX}/actions/runs/{source['run_id']}/artifacts", 'artifacts')
    selected = [row for row in artifacts if row['name'] == f'verification-{platform}-{profile}-{partition}']
    require(len(selected) == 1, 'missing or duplicate source artifact')
    artifact = selected[0]
    origin = artifact['workflow_run']
    require(origin['id'] == source['run_id'] and origin['repository_id'] == origin['head_repository_id'] == REPOSITORY_ID
            and origin['head_sha'] == run['head_sha'] and artifact['expired'] is False, 'artifact run relationship mismatch')
    require(timestamp(job['started_at']) <= timestamp(artifact['created_at']) <= timestamp(artifact['updated_at'])
            <= timestamp(job['completed_at']) and now < timestamp(artifact['expires_at']), 'artifact outside source job or expired')
    return artifact, job, run


def attempt(args, proof):
    """One descendant-free worker; only the trusted workflow supplies the handoff."""
    deadline = time.monotonic() + 90
    require(os.environ.get("GITHUB_EVENT_NAME") == "push" and os.environ.get("GITHUB_REF") == "refs/heads/main"
            and os.environ.get("GITHUB_REPOSITORY") == REPOSITORY and os.environ.get("CI_RUST_CACHE_OUTCOME") == "success"
            and os.environ.get("CI_RUST_CACHE_HIT") == "true" and re.fullmatch(r"[0-9a-f]{64}", os.environ.get("CI_BUILD_RECIPE", "")),
            "not an eligible main push with an exact dependency-cache hit")
    handoff = os.environ.get("CI_QUALIFICATION", "")
    require(len(handoff) <= 4096, "qualification handoff exceeds budget")
    identity = document(handoff.encode())
    require(identity["target_sha"] == os.environ["GITHUB_SHA"] and identity["target_run"] == int(os.environ["GITHUB_RUN_ID"]),
            "qualification handoff targets a different run")
    with (verify.ROOT / ".local/verify/observations/before.json").open("rb") as handle:
        target = document(read_bounded(handle, 2 * 1024 * 1024, deadline))
    api = GitHub(os.environ["GH_TOKEN"])
    artifact, job, run = source_artifact(api, identity, args.platform, args.profile, args.partition, now=datetime.now(timezone.utc))
    records = artifact_records(download_artifact(api, artifact, deadline=deadline), deadline=deadline)
    validate_cell(records, identity, job, target, profile=args.profile, partition=args.partition,
                  jobs=target["execution_limits"]["jobs"], workers=target["execution_limits"]["test_workers"])
    require(run_snapshot(api.get(f"{PREFIX}/actions/runs/{run['id']}")) == run_snapshot(run)
            and api.get(f"{PREFIX}/git/ref/heads/main")["object"]["sha"] == identity["target_sha"], "source attempt or main changed")
    current = api.get(f"{PREFIX}/actions/runs/{identity['target_run']}")
    run_identity(current, identity['target_run'], 'push')
    require(current['status'] == 'in_progress' and current['conclusion'] is None
            and current['head_sha'] == identity['target_sha'], 'target run no longer active')
    remaining(deadline)
    proof.write_text(json.dumps({"source": identity["source"], "target_sha": identity["target_sha"], "target_run": identity["target_run"],
                                 "target_url": f"https://github.com/{REPOSITORY}/actions/runs/{identity['target_run']}/attempts/1",
                                 "artifact_id": artifact["id"], "artifact_digest": artifact["digest"],
                                 "cell": [args.platform, args.profile, args.partition]}), encoding="utf-8")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    parser.add_argument("--profile", choices=("debug", "release"), required=True)
    parser.add_argument("--partition", choices=("rust", "python"), required=True)
    parser.add_argument("--platform", choices=("ubuntu-latest", "windows-latest"), required=True)
    parser.add_argument("--worker-proof", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    try:
        if args.worker_proof:
            attempt(args, args.worker_proof)
        else:
            with tempfile.TemporaryDirectory() as directory:
                proof = Path(directory) / "proof.json"
                command = [sys.executable, __file__, "--profile", args.profile, "--partition", args.partition,
                           "--platform", args.platform, "--worker-proof", str(proof)]
                require(bounded_worker(command), "optional adoption worker unavailable or ineligible")
                receipt = document(proof.read_bytes())
            receipt.update(adopted=True, tests_executed_here=False)
            output = verify.ROOT / ".local/verify/adoption.json"
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        sys.stdout.buffer.write(b"adopted\n")
        return 0
    except Exception:  # Fixed diagnostic only: never emit signed URLs or raw network exceptions.
        if not args.worker_proof:
            output = verify.ROOT / ".local/verify/adoption.json"
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text('{"adopted":false,"reason":"optional evidence unavailable or ineligible; run full verification"}\n', encoding="utf-8")
        return 1  # The workflow must call this optional helper in a shell conditional.


if __name__ == "__main__":
    raise SystemExit(main())
