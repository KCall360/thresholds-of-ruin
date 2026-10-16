"""Run selected Rust tests and catalog contracts from their exact Cargo artifact.

One Cargo invocation: integration-target selection also builds the feature-gated
test=false host binary. Never guess a target path or launch a fallback build.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parent.parent
HOST_NAME = "tor-service-contract-host"
HOST_FEATURE = "service-contract-host"
HOST_SOURCE = ROOT / "crates/server/src/bin/tor-service-contract-host.rs"


def cargo_command(profile, packages=(), *, workspace=False):
    if profile not in ("debug", "release"):
        raise ValueError("Service contracts require debug or release")
    if workspace == bool(packages) or (not workspace and "tor-server" not in packages):
        raise ValueError("Select the workspace or an explicit package set containing tor-server")
    selection = ["--workspace"] if workspace else [part for name in sorted(set(packages)) for part in ("-p", name)]
    return ["cargo", "test", *selection, "--locked", "--features", "tor-server/service-contract-host",
            "--message-format=json-render-diagnostics", *(["--release"] if profile == "release" else [])]


def select_artifact(rows):
    if len(rows) != 1:
        raise ValueError(f"Expected exactly one Service host Cargo artifact, received {len(rows)}")
    row = rows[0]
    target = row.get("target", {})
    if (row.get("reason") != "compiler-artifact" or target.get("name") != HOST_NAME
            or target.get("kind") != ["bin"] or row.get("profile", {}).get("test") is not False
            or Path(target.get("src_path", "")).resolve() != HOST_SOURCE.resolve()
            or HOST_FEATURE not in row.get("features", [])
            or HOST_FEATURE not in target.get("required-features", [])):
        raise ValueError("Cargo artifact is not the selected production-Service host binary")
    executable = Path(row.get("executable") or "")
    if not executable.is_absolute() or not executable.is_file():
        raise ValueError("Cargo did not identify an existing absolute Service host executable")
    return executable


def run_suite(executable, digest, *, directory=None, pattern="service_contract_tests.py"):
    """A missing, empty, skipped or partially executed suite cannot qualify a gate."""
    directory = ROOT / "scripts" if directory is None else Path(directory)
    # Independent discovery must not inherit an outer suite's retained root.
    suite = unittest.TestLoader().discover(str(directory), pattern=pattern)
    expected = suite.countTestCases()
    if expected == 0:
        print("Service contract suite is empty; no behavior was verified", file=sys.stderr)
        return 1
    environment = {"TOR_SERVICE_CONTRACT_HOST": str(executable), "TOR_SERVICE_CONTRACT_SHA256": digest}
    prior = {key: os.environ.get(key) for key in environment}
    os.environ.update(environment)
    try:
        result = unittest.TextTestRunner(verbosity=2).run(suite)
    finally:
        for key, value in prior.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
    print("SERVICE_CONTRACT_RESULT " + json.dumps({"selected": expected, "run": result.testsRun,
        "failures": len(result.failures), "errors": len(result.errors), "skipped": len(result.skipped)}), flush=True)
    return 0 if result.wasSuccessful() and result.testsRun == expected and not result.skipped else 1


def run(profile, packages=(), *, workspace=False):
    command = cargo_command(profile, packages, workspace=workspace)
    artifacts = []
    print("Service contracts Cargo command: " + " ".join(command), flush=True)
    with subprocess.Popen(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                          text=True, encoding="utf-8", errors="replace") as cargo:
        for line in cargo.stdout:
            print(line, end="", flush=True)
            try:
                row = json.loads(line)
            except ValueError:
                continue  # Ordinary libtest output is interleaved with Cargo JSON.
            if (isinstance(row, dict) and row.get("reason") == "compiler-artifact"
                    and row.get("target", {}).get("name") == HOST_NAME):
                artifacts.append(row)
        status = cargo.wait()
    if status:
        return status  # Never mask a failed Rust suite or run host tests after it.
    try:
        executable = select_artifact(artifacts)
    except ValueError as error:
        print(f"Service contract artifact error: {error}", file=sys.stderr)
        return 1
    digest = hashlib.sha256(executable.read_bytes()).hexdigest()
    print("SERVICE_CONTRACT_ARTIFACT " + json.dumps({"executable": str(executable),
        "sha256": digest, "profile": profile, "features": artifacts[0]["features"]}), flush=True)
    return run_suite(executable, digest)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("debug", "release"), required=True)
    selected = parser.add_mutually_exclusive_group(required=True)
    selected.add_argument("--workspace", action="store_true")
    selected.add_argument("--package", action="append", default=[])
    args = parser.parse_args(argv)
    try:
        return run(args.profile, args.package, workspace=args.workspace)
    except (OSError, ValueError) as error:
        print(f"Service contract runner error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
