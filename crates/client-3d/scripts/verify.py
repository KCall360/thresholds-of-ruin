"""Verify the independent 3D workspace, including real native process acceptance.

Run explicitly when maintaining this client. Core changes deliberately do not
invoke this script or its workflow. A successful run qualifies only the recorded
source fingerprint, profile, toolchain and command configuration.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import unittest
import json
import os
from pathlib import Path
import subprocess
import sys
import time


CLIENT = Path(__file__).resolve().parents[1]
ROOT = CLIENT.parents[1]
MANIFEST = str(CLIENT / "Cargo.toml")
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", CLIENT / "target")).resolve()

# Load the maintained core accounting implementation under an unambiguous name.
sys.path.insert(0, str(ROOT / "scripts"))
import graphical_waiver
_spec = importlib.util.spec_from_file_location("_core_verifier_for_3d", ROOT / "scripts/verify.py")
_core_verifier = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_core_verifier)


def waiver_plan(path):
    manifest = graphical_waiver.read_manifest(path)
    suite = unittest.defaultTestLoader.discover(str(CLIENT / "scripts"), pattern="test_*process.py")
    # Exact current discovery, not a count cap or a broad class exclusion.
    with graphical_waiver.apply(suite, manifest["tests"], manifest["reason"]):
        pass
    return manifest

# Explicit non-credential inputs: a DX12 run is different evidence from a
# platform-default run, as are compiler/profile overrides.
CONFIGURATION = (
    "CARGO_BUILD_JOBS", "CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_DEV_OPT_LEVEL",
    "CARGO_PROFILE_RELEASE_DEBUG", "CARGO_PROFILE_RELEASE_OPT_LEVEL", "CARGO_PROFILE_RELEASE_LTO",
    "CARGO_PROFILE_DEV_CODEGEN_UNITS", "CARGO_PROFILE_RELEASE_CODEGEN_UNITS",
    "CARGO_PROFILE_TEST_DEBUG", "CARGO_PROFILE_TEST_OPT_LEVEL",
    "CARGO_TARGET_DIR", "CARGO_BUILD_TARGET", "CARGO_INCREMENTAL", "RUSTUP_TOOLCHAIN",
    "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
    "WGPU_BACKEND", "WGPU_ADAPTER_NAME", "WGPU_POWER_PREF", "WGPU_SETTINGS_PRIO", "WGPU_DX12_COMPILER",
    "WGPU_GLES_MINOR_VERSION", "WGPU_VALIDATION", "WGPU_DEBUG", "DISPLAY", "WAYLAND_DISPLAY",
    "WGPU_DISCARD_HAL_LABELS", "WGPU_ALLOW_UNDERLYING_NONCOMPLIANT_ADAPTER", "WGPU_GPU_BASED_VALIDATION",
    "TOR_TEST_PROFILE", "TOR_TEST_BINARIES", "TOR_3D_TEST_BINARY", "TOR_TEST_ARTIFACTS",
)


def steps(profile):
    optimized = ["--release"] if profile == "release" else []
    cargo = ["--manifest-path", MANIFEST]
    return [
        ("tooling", [sys.executable, "-m", "unittest", "discover", "-s", str(CLIENT / "scripts"), "-p", "test_verification.py"]),
        ("showcase-source", [sys.executable, "-m", "unittest", "discover", "-s", str(CLIENT / "scripts"), "-p", "test_scenario.py"]),
        ("fmt", ["cargo", "fmt", *cargo, "--all", "--", "--check"]),
        ("clippy", ["cargo", "clippy", *cargo, "--workspace", "--all-targets", "--locked", *optimized, "--", "-D", "warnings"]),
        ("architecture", [sys.executable, "scripts/check_architecture.py", *cargo]),
        ("rust-tests", ["cargo", "test", *cargo, "--workspace", "--locked", *optimized]),
        ("core-binaries", ["cargo", "build", "--manifest-path", str(ROOT / "Cargo.toml"), "--target-dir", str(TARGET),
                           "-p", "tor-server", "-p", "tor-client-headless", "--bin", "tor-server", "--bin", "tor-scenario",
                           "--bin", "tor-client-headless", "--locked", *optimized]),
        ("scenario-check", [str(TARGET / profile / ("tor-scenario.exe" if os.name == "nt" else "tor-scenario")),
                            "check", str(CLIENT / "scenarios/menagerie")]),
        ("client-binary", ["cargo", "build", *cargo, "--bin", "tor-client-3d", "--locked", *optimized]),
        ("process-tests", [sys.executable, "-m", "unittest", "discover", "-s", str(CLIENT / "scripts"), "-p", "test_*process.py"]),
    ]


def source_identity():
    paths = subprocess.check_output(["git", "ls-files", "-c", "-o", "--exclude-standard", "-z"], cwd=ROOT).split(b"\0")
    digest = hashlib.sha256()
    for raw in sorted(set(paths) - {b""}):
        path = ROOT / os.fsdecode(raw)
        digest.update(raw + b"\0")
        digest.update(path.read_bytes() if path.is_file() else b"<missing>")
        digest.update(b"\0")
    return {
        "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "inputs_sha256": digest.hexdigest(),
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("debug", "release"), default="debug")
    parser.add_argument("--waive-local-graphical", metavar="MANIFEST")
    args = parser.parse_args(argv)
    graphical = None
    if args.waive_local_graphical:
        if os.environ.get("CI"):
            parser.error("--waive-local-graphical is local only")
        try:
            graphical = waiver_plan(args.waive_local_graphical)
        except (OSError, ValueError) as error:
            parser.error(str(error))
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    logs = ROOT / ".local" / "verify-3d" / f"{stamp}-{args.profile}"
    logs.mkdir(parents=True)
    graphical_path = logs / "graphical-waiver.json"
    if graphical is not None:
        graphical_path.write_text(json.dumps(graphical, indent=2) + "\n", encoding="utf-8")
    temporary = ROOT / ".local" / "client-3d-tmp"
    temporary.mkdir(parents=True, exist_ok=True)
    environment = {**os.environ, "CARGO_BUILD_JOBS": "2", "CARGO_PROFILE_DEV_DEBUG": "0", "PYTHONUTF8": "1",
                   "CARGO_TARGET_DIR": str(TARGET), "TEMP": str(temporary), "TMP": str(temporary),
                   "TOR_TEST_PROFILE": args.profile, "TOR_TEST_BINARIES": str(TARGET / args.profile),
                   "TOR_TEST_ARTIFACTS": str(logs / "process-failures"),
                   "TOR_3D_TEST_BINARY": str(TARGET / args.profile / ("tor-client-3d.exe" if os.name == "nt" else "tor-client-3d"))}
    lock = TARGET / ".verify-3d-owner"
    TARGET.mkdir(parents=True, exist_ok=True)
    try:
        owner = lock.open("x", encoding="utf-8")
    except FileExistsError:
        parser.error(f"target directory already has a verification owner: {lock}; verify its process before recovery")
    receipt = {"schema": "tor-client-3d-verification-v3", "profile": args.profile,
               "platform": sys.platform, "python": sys.version,
               "environment": {name: environment.get(name) for name in CONFIGURATION},
               "steps": [], "passed": False, "gate_passed": False, "clean_passed": False,
               "graphical_waiver": graphical, "waived_tests": []}
    try:
        with owner:
            owner.write(str(os.getpid()))
        receipt["source"] = source_identity()
        receipt["toolchain"] = {name: subprocess.check_output([name, "--version"], text=True).strip()
                                for name in ("cargo", "rustc")}
        for name, command in steps(args.profile):
            if name == "process-tests" and graphical is not None:
                command = [sys.executable, str(ROOT / "scripts/test_runner.py"),
                           "--waive-local-graphical", str(graphical_path),
                           *command[3:]]
                environment["TOR_TEST_TIMINGS"] = str(logs / "process-tests-tests.jsonl")
            started = time.monotonic()
            log = logs / f"{name}.log"
            try:
                with log.open("w", encoding="utf-8") as output:
                    result = subprocess.run(command, cwd=ROOT, env=environment, stdout=output, stderr=subprocess.STDOUT,
                                            timeout=2400 if name != "process-tests" else 1200, check=False)
                code = result.returncode
            except subprocess.TimeoutExpired:
                code = 124
            if name == "process-tests" and graphical is not None:
                try:
                    receipt["waived_tests"] = _core_verifier.graphical_waiver_evidence(
                        environment["TOR_TEST_TIMINGS"], graphical["tests"])
                except (OSError, ValueError, TypeError, KeyError) as error:
                    code = code or 2
                    print(f"FAIL: graphical waiver accounting: {error}")
            receipt["steps"].append({"name": name, "command": command, "exit_code": code,
                                     "seconds": time.monotonic() - started, "log": str(log)})
            print(f"{name}: {'PASS' if code == 0 else 'FAIL'}", flush=True)
            if code:
                print("\n".join(log.read_text(encoding="utf-8", errors="replace").splitlines()[-55:]))
                return 1
        receipt["final_source"] = source_identity()
        receipt["gate_passed"] = (receipt["source"] == receipt["final_source"]
                                  and (graphical is None or receipt["waived_tests"] == graphical["tests"]))
        receipt["clean_passed"] = receipt["gate_passed"] and graphical is None
        receipt["passed"] = receipt["clean_passed"]  # Conservative legacy readers.
        if receipt["source"] != receipt["final_source"]:
            print("FAIL: checked inputs changed during verification")
        return 0 if receipt["gate_passed"] else 1
    finally:
        (logs / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        lock.unlink()
        print(f"Verification evidence: {logs}")


if __name__ == "__main__":
    raise SystemExit(main())
