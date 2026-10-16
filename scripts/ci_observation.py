"""Record bounded, untrusted CI observations; never decide whether tests may skip.

Run before and after full verification; redirect JSON into the verification log
artifact. An exit of zero means a report was emitted, not that verification or
reuse passed. Missing metadata/probes/files produce issues and complete=false.
Even complete=true means only that these observations were collected.

PR-controlled code, environment and tools can forge this entire document. It is
not authenticated provenance, a complete input manifest, or a compatibility
identity. Git tracked_dirty/untracked_present are observations, not clean-input
proof: ignored build output is expected and is not inspected. Collection is not
an atomic snapshot, even for reviewed code. Python's executable path is diagnostic
only. Allowlisted environment digests describe this observer process, not the
verifier's per-stage overrides or environment equivalence. Installed distribution
versions describe metadata, not imported code, package bytes or provenance. Native
libraries/display state and non-allowlisted configuration remain unobserved. This producer
never reads event payloads, credentials, arbitrary environment/configuration or
network services, and never installs tools. External validation would be needed
before any evidence-reuse proposal; this module implements no reuse decision.
"""

import argparse
from datetime import datetime, timezone
import hashlib
from importlib import metadata
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import threading


MAX_OUTPUT_BYTES = 64 * 1024
MAX_FILE_BYTES = 2 * 1024 * 1024
PROBE_TIMEOUT = 5
MAX_DISTRIBUTIONS = 256
# Fixed names only: never discover arbitrary environment keys or expose their values.
BUILD_TEST_ENVIRONMENT = (
    "CARGO_BUILD_TARGET", "CARGO_BUILD_JOBS", "CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_RELEASE_DEBUG",
    "CARGO_INCREMENTAL", "CARGO_TARGET_DIR", "CARGO_HOME", "RUSTUP_TOOLCHAIN", "RUSTUP_HOME",
    "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTDOCFLAGS",
    "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "RUSTDOC", "CC", "CXX", "AR",
    "CFLAGS", "CXXFLAGS", "LDFLAGS", "PATH", "PYTHONPATH", "PYTHONHOME", "PYTHONHASHSEED",
    "PYTHONUTF8", "PYTHONIOENCODING", "VIRTUAL_ENV",
    "TOR_TEST_PROFILE", "TOR_SCENARIO_COMPILER", "TOR_TEST_BINARIES", "TOR_LOCAL_MOUSE_WAIVER", "TOR_TEST_PRIORITY",
)
# Bounded explicit inputs, not recursive discovery or a claim of completeness.
CONFIG_FILES = (
    ".github/workflows/ci.yml", ".github/requirements-arena-search.txt", ".github/requirements-lint.txt",
    "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config", ".cargo/config.toml",
    "deny.toml", "ruff.toml", "AGENTS.md", "CONTRIBUTING.md", "docs/development/testing.md",
    "scripts/verify.py", "scripts/test_runner.py", "scripts/parallel_tests.py",
    "scripts/graphical_waiver.py", "scripts/ci_observation.py",
    "scripts/ci_scope.py", "scripts/ci_build_key.py", "scripts/service_contract_ci.py",
    "scripts/service_contract_host.py", "scripts/process_harness.py", "scripts/ruleset_sources.py",
    "scripts/ci_policy.py", "scripts/ci_coverage_manifest.json", "scripts/ci_shadow.py",
)
OPTIONAL_FILES = frozenset((".cargo/config", ".cargo/config.toml", "scripts/ci_policy.py",
                            "scripts/ci_coverage_manifest.json", "scripts/ci_shadow.py"))
SHA = r"(?:[0-9a-f]{40}|[0-9a-f]{64})"
TOKEN = r"[A-Za-z0-9_.+-]{1,128}"
ENVIRONMENT_FIELDS = {
    "github": {
        "repository": ("GITHUB_REPOSITORY", r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+"),
        "run_id": ("GITHUB_RUN_ID", r"[1-9][0-9]{0,19}"),
        "run_attempt": ("GITHUB_RUN_ATTEMPT", r"[1-9][0-9]{0,9}"),
        "job": ("GITHUB_JOB", TOKEN), "event": ("GITHUB_EVENT_NAME", TOKEN),
        "ref": ("GITHUB_REF", r"refs/[A-Za-z0-9/_.+@-]{1,240}"),
        "reported_sha": ("GITHUB_SHA", SHA),
        "workflow_ref": ("GITHUB_WORKFLOW_REF", r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/\.github/workflows/[A-Za-z0-9_.-]+@[A-Za-z0-9/_.+@-]+"),
        "workflow_sha": ("GITHUB_WORKFLOW_SHA", SHA),
    },
    "runner": {
        "os": ("RUNNER_OS", r"Linux|Windows|macOS"), "architecture": ("RUNNER_ARCH", r"X86|X64|ARM|ARM64"),
        "image_os": ("ImageOS", TOKEN), "image_version": ("ImageVersion", r"[0-9]+(?:\.[0-9]+){1,3}"),
    },
}


def issue(issues, scope, code):
    # Deliberately never include command stderr or invalid raw values.
    issues.append({"scope": scope, "code": code})


def probe(command, root, issues, scope):
    """Bound wall time AND captured stdout; kill overflow and retain no partial identity."""
    try:
        process = subprocess.Popen(command, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, bufsize=0,
                                   env={**os.environ, "RUSTUP_AUTO_INSTALL": "0", "RUSTUP_NO_UPDATE_CHECK": "1",
                                        "GIT_OPTIONAL_LOCKS": "0", "GIT_NO_LAZY_FETCH": "1"})
    except OSError:
        issue(issues, scope, "unavailable")
        return None
    output = bytearray()
    overflow = threading.Event()

    def read():
        while True:
            try:
                chunk = process.stdout.read(min(4096, MAX_OUTPUT_BYTES + 1 - len(output)))
            except (OSError, ValueError):
                break  # Main thread may close a pipe held open by a descendant.
            if not chunk: break
            output.extend(chunk)
            if len(output) > MAX_OUTPUT_BYTES:
                overflow.set()
                process.kill()
                break

    reader = threading.Thread(target=read, daemon=True)
    reader.start()
    try:
        process.wait(timeout=PROBE_TIMEOUT)
        reader.join(timeout=1)
        if overflow.is_set(): issue(issues, scope, "output_limit")
        elif reader.is_alive(): issue(issues, scope, "incomplete_output")
        elif process.returncode: issue(issues, scope, "command_failed")
        else: return bytes(output)
    except subprocess.TimeoutExpired:
        issue(issues, scope, "timeout")
    finally:
        if process.poll() is None: process.kill()
        process.wait()
        reader.join(timeout=1)
        process.stdout.close()
    return None


def validated(value, pattern, issues, scope, *, optional=False):
    if optional and value is None: return None
    if not isinstance(value, str) or len(value) > 512 or not re.fullmatch(pattern, value):
        issue(issues, scope, "missing_or_invalid")
        return None
    return value


def hash_file(root, name, issues):
    path = root / name
    try:
        if path.resolve() != path or path.is_symlink():
            raise ValueError("Symlinked inputs are not followed")
        if not path.exists():
            if name not in OPTIONAL_FILES: issue(issues, name, "missing")
            return {"status": "missing"}
        if not path.is_file() or path.stat().st_size > MAX_FILE_BYTES:
            raise ValueError("Not a bounded regular file")
        with path.open("rb") as source:
            contents = source.read(MAX_FILE_BYTES + 1)
        if len(contents) > MAX_FILE_BYTES: raise ValueError("File grew beyond bound")
        return {"status": "present", "sha256": hashlib.sha256(contents).hexdigest(), "bytes": len(contents)}
    except (OSError, ValueError, RuntimeError):
        issue(issues, name, "unavailable_or_oversized")
        return {"status": "unavailable"}


def checkout(root, issues):
    result = {"sha": None, "tree": None, "parents": None, "tracked_dirty": None, "untracked_present": None}
    git = ["git", "--no-replace-objects", "-c", "core.fsmonitor=false", "-c", "core.untrackedCache=false"]
    top = probe([*git, "rev-parse", "--show-toplevel"], root, issues, "checkout.root")
    try:
        if top is None or Path(os.fsdecode(top).strip()).resolve() != root:
            issue(issues, "checkout.root", "not_repository_root")
            return result
    except (ValueError, OSError):
        issue(issues, "checkout.root", "invalid_output")
        return result
    head = probe([*git, "rev-parse", "--verify", "HEAD^{commit}"], root, issues, "checkout.commit")
    try:
        sha = head.decode("ascii").strip() if head is not None else ""
        if not re.fullmatch(SHA, sha): raise ValueError("Invalid HEAD identity")
        # Traversal-aware %P hides parents at a shallow boundary. Read the raw
        # object for the resolved SHA so a moving HEAD cannot mix identities.
        output = probe([*git, "cat-file", "-p", sha], root, issues, "checkout.commit")
        headers, separator, _ = output.partition(b"\n\n") if output is not None else (b"", b"", b"")
        lines = headers.splitlines()
        trees = [line[5:].decode("ascii") for line in lines if line.startswith(b"tree ")]
        parents = [line[7:].decode("ascii") for line in lines if line.startswith(b"parent ")]
        if (not separator or len(trees) != 1 or not lines[0].startswith(b"tree ") or len(parents) > 16
                or len(parents) != len(set(parents))
                or any(not re.fullmatch(SHA, value) for value in [*trees, *parents])):
            raise ValueError("Invalid commit observation")
        result.update(sha=sha, tree=trees[0], parents=parents)
    except (UnicodeError, ValueError):
        issue(issues, "checkout.commit", "invalid_output")
    status = probe([*git, "status", "--porcelain=v1", "-z", "--untracked-files=normal"], root, issues, "checkout.status")
    if status is None:
        issue(issues, "checkout.status", "unavailable")
        return result
    try:
        if status and not status.endswith(b"\0"): raise ValueError("Truncated status")
        entries = iter(status.split(b"\0")[:-1])
        tracked, untracked = False, False
        for entry in entries:
            if len(entry) < 4 or entry[2:3] != b" ": raise ValueError("Malformed status")
            flag = entry[:2]
            if flag == b"??": untracked = True
            elif all(character in b" MADRCUT" for character in flag) and flag != b"  ":
                tracked = True
                if b"R" in flag or b"C" in flag:
                    if not next(entries): raise ValueError("Empty rename source")
            else: raise ValueError("Unknown status")
        result.update(tracked_dirty=tracked, untracked_present=untracked)
    except (ValueError, StopIteration):
        issue(issues, "checkout.status", "invalid_or_truncated")
    return result


def tool_versions(root, issues):
    rustc = probe(["rustc", "-Vv"], root, issues, "tools.rustc")
    cargo = probe(["cargo", "-V"], root, issues, "tools.cargo")
    version = r" \d+\.\d+\.\d+(?:-[A-Za-z0-9.]+)? \([0-9a-f]{7,40} \d{4}-\d{2}-\d{2}\)"
    resolved = None
    try:
        lines = rustc.decode("ascii").splitlines() if rustc else []
        parts = dict(line.split(": ", 1) for line in lines[1:])
        expected = {"binary", "commit-hash", "commit-date", "host", "release", "LLVM version"}
        if (not lines or not re.fullmatch("rustc" + version, lines[0]) or set(parts) != expected
                or len(lines) != len(expected) + 1 or parts["binary"] != "rustc"
                or not re.fullmatch(SHA, parts["commit-hash"])
                or not re.fullmatch(r"\d{4}-\d{2}-\d{2}", parts["commit-date"])
                or any(not re.fullmatch(TOKEN, parts[key]) for key in ("host", "release", "LLVM version"))):
            raise ValueError("Unresolved rustc identity")
        resolved = {"version": lines[0], **{key.replace("-", "_").replace("LLVM version", "llvm_version"): value
                                          for key, value in parts.items() if key != "binary"}}
    except (UnicodeError, ValueError):
        issue(issues, "tools.rustc", "invalid_or_unavailable_version")
    try:
        cargo = cargo.decode("ascii").strip() if cargo else None
    except UnicodeError:
        cargo = None
    cargo = validated(cargo, "cargo" + version, issues, "tools.cargo")
    return {"rustc": resolved, "cargo": cargo,
            "python": {"implementation": platform.python_implementation(), "version": platform.python_version(),
                       "executable_diagnostic": sys.executable, "distributions": python_distributions(issues)}}


def build_test_environment(environment, issues):
    values = {}
    for name in BUILD_TEST_ENVIRONMENT:
        if name not in environment:
            values[name] = {"status": "absent"}
            continue
        try:
            value = environment[name]
            if not isinstance(value, str) or len(value) > MAX_OUTPUT_BYTES: raise ValueError("Unbounded value")
            encoded = value.encode("utf-8")
            if len(encoded) > MAX_OUTPUT_BYTES: raise ValueError("Unbounded bytes")
            values[name] = {"status": "present", "sha256": hashlib.sha256(encoded).hexdigest()}
        except ValueError:
            values[name] = {"status": "unavailable"}
            issue(issues, f"environment.{name}", "invalid_or_oversized")
    digest = None if any(value["status"] == "unavailable" for value in values.values()) else hashlib.sha256(
        json.dumps(values, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return {"scope": "observer_process", "variables": values, "sha256": digest}


def python_distributions(issues):
    versions = {}
    try:
        for number, distribution in enumerate(metadata.distributions()):
            if number >= MAX_DISTRIBUTIONS: raise ValueError("Unbounded inventory")
            name, version = distribution.metadata.get("Name"), distribution.version
            if (not isinstance(name, str) or not re.fullmatch(r"[A-Za-z0-9](?:[A-Za-z0-9._-]{0,126}[A-Za-z0-9])?", name)
                    or not isinstance(version, str) or not re.fullmatch(r"[0-9][A-Za-z0-9_.+!-]{0,127}", version)):
                raise ValueError("Invalid distribution metadata")
            name = re.sub(r"[-_.]+", "-", name).lower()
            if name in versions: raise ValueError("Ambiguous distribution identity")
            versions[name] = version
        return {"status": "present", "versions": versions, "sha256": hashlib.sha256(
            json.dumps(versions, sort_keys=True, separators=(",", ":")).encode()).hexdigest()}
    except Exception:  # Installed metadata finders may fail with arbitrary exceptions; omit their messages.
        issue(issues, "tools.python.distributions", "invalid_or_unavailable_inventory")
        return {"status": "unavailable"}


def collect(root, phase, *, environment=None, profile=None, partition=None):
    """Collect observations only; even an issue-free report has no authority."""
    root = Path(root).resolve()
    environment = os.environ if environment is None else environment
    issues = []
    report = {"schema_version": 1, "status": "untrusted_observation", "phase": phase,
              "recorded_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")}
    report["phase"] = validated(phase, r"before|after", issues, "phase")
    for section, fields in ENVIRONMENT_FIELDS.items():
        values = {key: validated(environment.get(variable), pattern, issues, f"{section}.{key}")
                  for key, (variable, pattern) in fields.items()}
        if section == "github":
            for key in ("run_id", "run_attempt"):
                if values[key] is not None: values[key] = int(values[key])
        report[section] = values
    report["labels"] = {key: validated(value, TOKEN, issues, f"labels.{key}", optional=True)
                        for key, value in (("profile", profile), ("partition", partition))}
    report["checkout"] = checkout(root, issues)
    report["file_hashes"] = {name: hash_file(root, name, issues) for name in CONFIG_FILES}
    report["tools"] = tool_versions(root, issues)
    report["build_test_environment"] = build_test_environment(environment, issues)
    report["omissions"] = ["non_allowlisted_environment_and_configuration", "verifier_stage_overrides",
                           "native_libraries_and_display", "package_contents_and_provenance"]
    report.update(complete=not issues, issues=issues)
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--phase", choices=("before", "after"), required=True)
    parser.add_argument("--profile", help="Optional diagnostic label; never selects tests")
    parser.add_argument("--partition", help="Optional diagnostic label; never selects tests")
    args = parser.parse_args(argv)
    print(json.dumps(collect(args.repo, args.phase, profile=args.profile, partition=args.partition), indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
