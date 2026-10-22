"""Execute and reconcile selected PR/main contract assurance, never a full run.

Uses the existing verifier's command execution, timeouts and timing runner. Every
consumer recomputes the Git-bound plan; the aggregate independently checks the
exact lane/test union. No cache or prior run qualifies selected execution.
"""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

import ci_test_plan as planner
import ci_example_prerequisites
import verify

ROOT = Path(__file__).resolve().parent.parent


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_json(path):
    raw = Path(path).read_bytes()
    require(len(raw) <= 16 * 1024 * 1024, "receipt exceeds size bound")
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "duplicate JSON key")
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=unique)


def validate_python(rows, expected, profile=None):
    require(expected and len(expected) == len(set(expected)), "empty/duplicate required selection")
    require(len(rows) >= 3 and rows[0].get("kind") == "selection" and rows[-1].get("kind") == "suite",
            "incomplete test lifecycle")
    require(Counter(rows[0].get("test_ids", [])) == Counter(expected), "runner selection differs from independent plan")
    require(all(row.get("kind") in ("test", "build") for row in rows[1:-1]), "unexpected test lifecycle row")
    if profile is not None:
        require(all(row.get("profile") == profile and row.get("schema_version") == 1 for row in rows),
                "wrong test profile or receipt schema")
    tests = [row for row in rows[1:-1] if row["kind"] == "test"]
    require(Counter(row.get("test_id") for row in tests) == Counter(expected), "missing/duplicate/unexpected test result")
    zero = lambda row, keys: all(type(row.get(key)) is int and row[key] == 0 for key in keys)
    require(all(row.get("outcome") == "passed" and zero(row, ("failures", "errors", "subtests_failed", "subtests_skipped"))
                for row in tests), "test did not pass cleanly")
    require(all(row.get("outcome") == "passed" and row.get("returncode") == 0
                for row in rows[1:-1] if row["kind"] == "build"), "failed application build")
    end = rows[-1]
    require(end.get("successful") is True and type(end.get("tests_run")) is int and end["tests_run"] == len(expected)
            and zero(end, ("failures", "errors", "skipped", "waived", "expected_failures", "unexpected_successes"))
            and not any(end.get(key) for key in ("waived_tests", "not_started", "batch_errors")), "incomplete terminal test result")


def steps_for(lane, logs, workers, binaries=None):
    profile = lane["profile"]
    if lane["partition"] == "build":
        return []  # The producer command is appended by run(), in this same target.
    if lane["partition"] == "rust":
        return verify.plan("full", set(), [], ci_profile="debug", ci_partition="rust", workers=workers)
    if lane["partition"] == "consumer":
        manifest = "crates/client-3d/Cargo.toml"
        return [("consumer-check", ["cargo", "check", "--manifest-path", manifest, "--bin", "tor-client-3d", "--locked"], {}),
                ("consumer-model", ["cargo", "test", "--manifest-path", manifest, "-p", "tor-client-3d-model", "--locked"], {})]
    require(lane["partition"] in ("python", "tooling") and profile in ("debug", "release"), "unknown selected lane")
    tooling = lane["partition"] == "tooling"
    steps = []
    if binaries is None and not tooling:
        steps.append(("build-applications", ["cargo", "build", "--workspace", "--bins", "--locked",
                                            *(["--release"] if profile == "release" else [])], {}))
    if lane["id"] == "windows-debug-python":
        steps.append(("platform-codecs", ["cargo", "test", "-p", "tor-protocol", "-p", "tor-client-common", "--locked"], {}))
    if profile == "release":
        steps.append(("release-sentinels", ["cargo", "test", "-p", "tor-world", "-p", "tor-protocol", "--locked", "--release"], {}))
    command = [sys.executable, str(ROOT / "scripts/test_runner.py"), "--workers", str(workers),
               "--from-file", str(logs / "selected-ids.txt"), "-v"]
    if not tooling and sys.platform.startswith("linux") and not os.environ.get("DISPLAY"):
        command = ["xvfb-run", "-a", "-s", "-screen 0 1280x1024x24", *command]
    environment = {"_cwd": "scripts", "TOR_TEST_PROFILE": profile}
    if binaries is not None:
        environment["TOR_TEST_BINARIES"] = str(binaries)
        environment["TOR_SCENARIO_COMPILER"] = str(binaries / ("tor-scenario.exe" if os.name == "nt" else "tor-scenario"))
    elif not tooling:
        target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
        environment["TOR_SCENARIO_COMPILER"] = str(target / profile / ("tor-scenario.exe" if os.name == "nt" else "tor-scenario"))
    prerequisite = ci_example_prerequisites.stage(lane['test_ids'], profile)
    require(not tooling or prerequisite is None, 'tooling lane contains a compiled example owner')
    if prerequisite:
        steps.append(prerequisite)
    steps.append(("python-selected", command, environment))
    return steps


def required_stages(lane, test_ids=()):
    """Independent ordered role contract, never derived from uploaded receipts."""
    stages = {
        "linux-debug-rust": ["fmt", "clippy", "simulation-isolated", "architecture", "rustdoc", "rust-debug", "publish-applications"],
        "linux-debug-build": ["publish-applications"],
        "linux-tooling": ["python-selected"],
        "windows-tooling": ["python-selected"],
        "linux-debug-python": ["validate-applications", "python-selected"],
        "windows-debug-python": ["build-applications", "platform-codecs", "python-selected"],
        "linux-release-python": ["build-applications", "release-sentinels", "python-selected"],
        "linux-3d-consumer": ["consumer-check", "consumer-model"],
    }[lane]
    if 'python-selected' in stages:
        prerequisite = ci_example_prerequisites.stage(test_ids, 'release' if lane == 'linux-release-python' else 'debug')
        require(not lane.endswith('-tooling') or prerequisite is None, 'tooling lane contains a compiled example owner')
        if prerequisite:
            stages.insert(stages.index('python-selected'), prerequisite[0])
    return stages


def binding(path):
    raw = path.read_bytes()
    require(len(raw) <= 64 * 1024 * 1024, "stage evidence exceeds bound")
    return {"file": path.name, "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def bound_bytes(directory, record):
    require(isinstance(record, dict) and set(record) == {"file", "bytes", "sha256"}
            and isinstance(record["file"], str) and Path(record["file"]).name == record["file"]
            and record["file"] not in (".", "..") and type(record["bytes"]) is int
            and 0 <= record["bytes"] <= 64 * 1024 * 1024, "invalid evidence binding")
    path = directory / record["file"]
    require(binding(path) == record, "evidence bytes do not match receipt")
    return path.read_bytes()


def validate_files(plan, receipt, directory):
    """Read uploaded evidence independently of runner-reported completion."""
    lane = next(row for row in plan["lanes"] if row["id"] == receipt["lane"])
    for step in receipt["steps"]:
        log = bound_bytes(directory, step["record"]).decode("utf-8", errors="strict")
        if step["step"] == "rust-debug":
            outcomes = [json.loads(line.removeprefix("SERVICE_CONTRACT_RESULT ")) for line in log.splitlines()
                        if line.startswith("SERVICE_CONTRACT_RESULT ")]
            require(len(outcomes) == 1, "missing production Service evidence")
            result = outcomes[0]
            require(type(result.get("selected")) is int and result["selected"] > 0
                    and result.get("run") == result["selected"]
                    and all(type(result.get(key)) is int and result[key] == 0 for key in ("failures", "errors", "skipped")),
                    "incomplete production Service execution")
    if lane["test_ids"]:
        raw = bound_bytes(directory, receipt["python_record"])
        validate_python([json.loads(line) for line in raw.decode("utf-8").splitlines()], lane["test_ids"], lane["profile"])
    if lane["partition"] == "tooling":
        import ci_no_build
        for record in receipt["no_build_records"].values():
            bound_bytes(directory / "no-build", record)
        ci_no_build.validate_audit(directory / "no-build", check_tools=False)
    if receipt["lane"] in ("linux-debug-rust", "linux-debug-build", "linux-debug-python"):
        manifest = json.loads(bound_bytes(directory, receipt["application_record"]))
        context = manifest.get("context", {})
        require(manifest.get("version") == 1 and context.get("source_sha") == plan["source_sha"]
                and context.get("tree") == plan["source_tree"] and context.get("profile") == "debug"
                and context.get("workflow") == {"repository": os.environ.get("GITHUB_REPOSITORY", ""),
                    "run": os.environ.get("GITHUB_RUN_ID", ""), "attempt": os.environ.get("GITHUB_RUN_ATTEMPT", "")},
                "application artifact identity differs from current run")


def validate_receipts(plan, receipts):
    require(plan.get("mode") == "selected" and plan.get("policy_version") == planner.POLICY_VERSION,
            "aggregate requires current selected policy")
    expected = {lane["id"]: lane for lane in plan["lanes"]}
    require(expected and Counter(row.get("lane") for row in receipts) == Counter(expected.keys()), "missing/duplicate/unexpected lane")
    for row in receipts:
        require(row.get("policy_version") == planner.POLICY_VERSION and row.get("assurance") == "selected-pr-contracts"
                and row.get("source_sha") == plan["source_sha"] and row.get("source_tree") == plan["source_tree"]
                and row.get("plan_sha256") == planner.digest(plan) and row.get("passed") is True
                and row.get("evidence_origin") == "fresh"
                and row.get("run_id") == os.environ.get("GITHUB_RUN_ID", "")
                and row.get("run_attempt") == os.environ.get("GITHUB_RUN_ATTEMPT", ""), "stale, failed or nonselected evidence")
        require(row.get("test_ids") == expected[row["lane"]]["test_ids"], "lane test identities differ")
        require(row.get("steps") and [step.get("step") for step in row["steps"]] == required_stages(row["lane"], expected[row["lane"]]["test_ids"])
                and all(type(step.get("code")) is int and step["code"] == 0 for step in row["steps"]),
                "missing, extra or failed canonical lane stage")
        if row["lane"] in ("linux-tooling", "windows-tooling"):
            require(isinstance(row.get("no_build_records"), dict) and set(row["no_build_records"]) == {"manifest", "audit"},
                    "missing no-build tooling audit")
        if row["lane"] in ("linux-debug-rust", "linux-debug-build", "linux-debug-python"):
            require(isinstance(row.get("application_record"), dict), "missing validated application evidence")
    applications = [row["application_record"] for row in receipts if row["lane"] in ("linux-debug-rust", "linux-debug-build", "linux-debug-python")]
    if applications:
        require(len(applications) == 2 and applications[0] == applications[1], "producer/consumer application identity differs")


def clean_source():
    subprocess.run(["git", "diff", "--quiet", "HEAD", "--"], cwd=ROOT, check=True)
    # A checked-in plan never permits untracked executable inputs to be imported.
    untracked = subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT)
    require(not untracked.strip(), "untracked source inputs")


def run(plan, lane_id, directory, binary_artifact=None, produce_binaries=None):
    require(plan["mode"] == "selected", "current scope requires full qualification, not selected execution")
    lanes = [lane for lane in plan["lanes"] if lane["id"] == lane_id]
    require(len(lanes) == 1, "lane absent from independently selected plan")
    lane = lanes[0]
    expected_os = "windows-latest" if sys.platform == "win32" else "ubuntu-latest"
    require(lane["os"] == expected_os, "lane executed on wrong operating system")
    require((lane_id == "linux-debug-python") == (binary_artifact is not None), "selected consumer requires its validated application artifact")
    require((lane_id in ("linux-debug-rust", "linux-debug-build")) == (produce_binaries is not None), "deterministic owner must produce applications")
    clean_source()
    require(not verify.running_builds(), "another Cargo target owner is active")
    binaries = None
    if binary_artifact is not None:
        import ci_binary_artifact
        binaries = ci_binary_artifact.validate(binary_artifact, lane["profile"], plan["source_sha"])
    directory.mkdir(parents=True, exist_ok=False)
    (directory / "selected-ids.txt").write_text("\n".join(lane["test_ids"]) + "\n", encoding="utf-8")
    jobs = verify.jobs_for_memory(verify.free_memory_gb(), os.cpu_count())
    workers = verify.test_workers(verify.free_memory_gb(), os.cpu_count())
    steps = steps_for(lane, directory, workers, binaries)
    if lane["partition"] == "tooling":
        import ci_no_build
        guarded = ci_no_build.prepare(directory / "no-build")
        ci_no_build.probe(directory / "no-build", guarded)
        steps = [(name, command, {**env, **guarded}) for name, command, env in steps]
    if binary_artifact is not None:
        steps.insert(0, ("validate-applications", [sys.executable, str(ROOT / "scripts/ci_binary_artifact.py"),
            "validate", str(binary_artifact.resolve()), "--profile", lane["profile"], "--source-sha", plan["source_sha"], "--smoke"], {}))
    if produce_binaries is not None:
        require(lane_id in ("linux-debug-rust", "linux-debug-build") and binary_artifact is None, "only deterministic build owner may publish applications")
        steps.append(("publish-applications", [sys.executable, str(ROOT / "scripts/ci_binary_artifact.py"),
                      "build", "--profile", "debug", "--output", str(produce_binaries.resolve())], {}))
    require([name for name, _, _ in steps] == required_stages(lane_id, lane["test_ids"]), "execution plan lacks a required role")
    results = []
    failure = None
    with verify.Lock(ROOT / ".local/verify.lock"):
        for name, command, env in steps:
            result = verify.run_step(name, command, dict(env), directory, jobs, False)
            results.append(result)
            print(f"{lane_id}/{name}: {'passed' if result['code'] == 0 else 'FAILED'} ({result['seconds']:.1f}s)", flush=True)
            if result["code"] != 0 or result["note"].startswith("WAIVED"):
                failure = "a required stage failed"
                break
    try:
        require(len(results) == len(steps) and failure is None, "incomplete selected lane")
        if lane["test_ids"]:
            rows = [json.loads(line) for line in (directory / "python-selected-tests.jsonl").read_text(encoding="utf-8").splitlines()]
            validate_python(rows, lane["test_ids"], lane["profile"])
        # Reuse existing validation for Cargo/Service production artifacts and
        # stage/environment accounting, but now require it, not just report it.
        completeness = verify.receipt_completeness(steps, results, directory, jobs)
        require(completeness["complete"], "incomplete verifier stage evidence: " + "; ".join(completeness["issues"]))
        if lane["partition"] == "tooling":
            ci_no_build.validate_audit(directory / "no-build")
        clean_source()
        require(planner.current_plan() == plan, "checked inputs or plan changed during execution")
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        failure = str(error)
    receipt = {"policy_version": planner.POLICY_VERSION, "assurance": "selected-pr-contracts", "lane": lane_id,
               "source_sha": plan["source_sha"], "source_tree": plan["source_tree"], "plan_sha256": planner.digest(plan),
               "passed": failure is None, "evidence_origin": "fresh", "run_id": os.environ.get("GITHUB_RUN_ID", ""),
               "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT", ""), "test_ids": lane["test_ids"], "failure": failure,
               "steps": [{"step": row["step"], "code": row["code"], "seconds": row["seconds"],
                          "record": binding(row["log"])} for row in results]}
    if lane["test_ids"] and (directory / "python-selected-tests.jsonl").is_file():
        receipt["python_record"] = binding(directory / "python-selected-tests.jsonl")
    if lane["partition"] == "tooling":
        receipt["no_build_records"] = {"manifest": binding(directory / "no-build/manifest.json"),
                                      "audit": binding(directory / "no-build/audit.jsonl")}
    application_source = produce_binaries or binary_artifact
    if application_source is not None and (application_source / "manifest.json").is_file():
        destination = directory / "application-manifest.json"
        destination.write_bytes((application_source / "manifest.json").read_bytes())
        receipt["application_record"] = binding(destination)
    (directory / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    if failure:
        print(f"Selected assurance failed: {failure}", file=sys.stderr)
    return 1 if failure else 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    execute = commands.add_parser("run")
    execute.add_argument("--lane", required=True)
    execute.add_argument("--output", type=Path, required=True)
    execute.add_argument("--binaries", type=Path)
    execute.add_argument("--produce-binaries", type=Path)
    aggregate = commands.add_parser("aggregate")
    aggregate.add_argument("--receipts", type=Path, required=True)
    args = parser.parse_args(argv)
    plan = planner.current_plan()
    if args.command == "run":
        return run(plan, args.lane, args.output.resolve(), args.binaries, args.produce_binaries)
    paths = sorted(args.receipts.glob("*/receipt.json"))
    receipts = [read_json(path) for path in paths]
    validate_receipts(plan, receipts)
    for receipt, path in zip(receipts, paths):
        validate_files(plan, receipt, path.parent)
    print("Selected contract assurance complete; exhaustive matrix coverage is a distinct scheduled/on-demand result.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
