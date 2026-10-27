"""Run the testing policy's checks in tiers, log each step, and print a compact summary.

Tiers (see docs/development/testing.md#running-the-checks):

  quick  at stable checkpoints: formatting and clippy for affected packages, debug
         tests for directly changed packages, the Python tool tests, and the
         process tests the change maps to, minus extended-cost ones
  push   before pushing: quick plus workspace clippy, API docs, the isolated
         simulation fixture gate and debug tests for every affected package
  full   exhaustive checks on one platform, debug and release; also covers
         the push gate for unchanged inputs, toolchain and test configuration

Affected packages include every workspace package that depends on a changed one.
Anything the mapping doesn't recognize selects everything, so a tier can run more
than it needs but never less. quick and push defer extended-cost process tests,
listing each one; full and CI still run them. CI independently selects reviewed PR/main contracts
or exhaustive coverage; both platform aggregates remain required before merging.
Exhaustive CI partitions each debug/release profile into Rust and Python/native
checks. One partition is never a full or merge gate. Scheduled, manual and release
runs retain exhaustive coverage. CI evidence does not certify a local run.

Tiers only choose which existing tests run. Every feature and bug fix must add
its tests to the suite (docs/development/testing.md), and this script warns when code
changes arrive without any.
"""

import argparse
import ast
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

import graphical_waiver
import ci_observation
import ci_test_contracts
import ci_example_prerequisites
import parallel_tests
from ci_scope import RUST_DOCUMENTATION_INPUTS

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
TIERS = ("quick", "push", "full")
CI_PARTITIONS = ("rust", "python")
# Every full-plan stage needs an explicit owner. Unknown stages must fail CI
# partition planning rather than silently disappear from both jobs.
CI_STAGE_PARTITIONS = {
    "fmt": "rust",
    "clippy": "rust",
    "simulation-isolated": "rust",
    "build-tools": "python",
    "build-examples-debug": "python",
    "build-examples-release": "python",
    "python-debug": "python",
    "architecture": "rust",
    "rustdoc": "rust",
    "rust-debug": "rust",
    "rust-release": "rust",
    "process-release": "python",
}

# Paths whose change can affect every package or every process test.
SHARED_PREFIXES = ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/", "scenarios/", "deny.toml")
# Non-code inputs exercised by Rust integration tests. Check before inert paths.
RUST_TEST_INPUTS = dict(RUST_DOCUMENTATION_INPUTS)
# These Python inputs exercise the real opt-in Service host, not only tooling.
RUST_TEST_INPUTS.update({f"scripts/{name}.py": {"tor-server"} for name in (
    "service_contract_ci", "service_contract_host", "service_contract_tests",
    "test_service_contract_ci", "test_service_contract_host")})
# Paths that otherwise never affect Rust packages or process behavior.
INERT_PREFIXES = ("docs/", "perf/", ".github/", "README.md", "CONTRIBUTING.md", "AGENTS.md", "LICENSE", "ruff.toml", ".gitignore")
# This frontend has its own Cargo workspace, lockfile, verifier and path-scoped
# CI. Shared/core changes never acquire its checks as reverse dependencies.
INDEPENDENT_PREFIXES = ("crates/client-3d/",)
# Clients launched by process tests. Any other runtime package change selects every process test.
# Each client package, and the text by which a process test can launch it: the
# binary name or a `process_harness` helper for that client. The harness offers
# helpers for every client, so its own text is not searched. Every server stop
# saves through the headless client, so any test that starts a server uses it.
HARNESS = "process_harness"
# Fast process suites that together start every client and touch movement,
# items, doors, intentions, checkpoints and package loading. The quick tier runs
# them, plus the process tests a change touches, when a change can reach every
# process test (runtime crates, scenarios, shared data, test infrastructure).
SMOKE_PROCESS_TESTS = ("test_adventure_process", "test_ascii_process", "test_checkpoint_process",
                       "test_doors_process", "test_headless_process", "test_intention_process",
                       "test_scenario_packages_process")
# Extended-cost process tests that CI's reviewed EXTENDED set does not list, each
# measured at two minutes or more in a local debug quick run on 2026-10-26.
# quick and push defer these and EXTENDED; full and CI run them as before.
LOCAL_EXTENDED = (
    "test_fall_credit_process.FallCreditProcesses.test_immediate_fatal_fall_owner_survives_journal_only_and_checkpoint_tail",
    "test_performance_process.PerformanceProcesses.test_rogue_benchmark_preserves_floor_work_and_residency_bounds",
    "test_performance_process.PerformanceProcesses.test_small_maintained_producers_emit_complete_valid_reports",
    "test_performance_process.PerformanceProcesses.test_streaming_benchmark_emits_valid_nested_acquisition_profiles",
    "test_rogue_playable_process.RoguePlayableProcesses.test_ordinary_first_floor_foes_earn_second_hd_and_ground_supplies_are_finite",
    "test_rogue_wraith_process.RogueWraithProcesses.test_autonomous_wraith_paid_latest_row_loss_survives_source_free_restart",
    "test_wandering_population_process.WanderingPopulationProcesses.test_scheduler_restart_cap_and_paid_created_births_then_blocked_consumption",
)
CLIENT_BINARIES = {
    "tor-client-ascii": ("tor-client-ascii", ".window(", ".ascii_frame(", ".key(", ".native_keys("),
    "tor-client-text": ("tor-client-text", ".text_client(", ".adventure("),
    "tor-client-headless": ("tor-client-headless", ".server(", ".client(", ".wizard(", "save_at"),
}
# Failure text that points at the machine rather than the change. The step still fails.
ENVIRONMENT_HINTS = {
    "memory allocation of": "rustc ran out of memory: rerun with a lower --jobs",
    "SetCursorPos": "native input was denied: rerun with interactive desktop access",
    "Native click must hit the game window": "another window covered the game window during a native click",
    "A native Windows desktop is required": "no interactive desktop",
}


def workspace(metadata):
    """Return {package: relative dir} and {package: workspace dependencies, including dev}."""
    root = Path(metadata["workspace_root"]).resolve()
    members = set(metadata["workspace_members"])
    packages = [p for p in metadata["packages"] if p["id"] in members]
    names = {p["name"] for p in packages}
    dirs, deps = {}, {}
    for package in packages:
        dirs[package["name"]] = Path(package["manifest_path"]).resolve().parent.relative_to(root).as_posix()
        deps[package["name"]] = {d["name"] for d in package["dependencies"] if d["name"] in names}
    return dirs, deps


def dependents(changed, deps):
    """Close a set of packages over reverse dependencies."""
    result = set(changed)
    grew = True
    while grew:
        grew = False
        for package, uses in deps.items():
            if package not in result and uses & result:
                result.add(package)
                grew = True
    return result


def classify(paths, dirs):
    """Return (directly changed packages, whether everything is affected, unrecognized paths)."""
    direct, everything, unknown = set(), False, []
    for path in paths:
        if path.startswith(INDEPENDENT_PREFIXES):
            continue
        if path in RUST_TEST_INPUTS:
            direct.update(RUST_TEST_INPUTS[path])
            continue
        if path.startswith(SHARED_PREFIXES):
            everything = True
            continue
        owner = next((name for name, d in dirs.items() if path.startswith(d + "/")), None)
        if owner:
            direct.add(owner)
        elif path.startswith("scripts/") or path.startswith(INERT_PREFIXES):
            continue
        else:
            unknown.append(path)
            everything = True
    return direct, everything, unknown


def script_imports(path):
    """Local script modules imported by a script."""
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    names = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            names.update(alias.name.split(".")[0] for alias in node.names)
        elif isinstance(node, ast.ImportFrom) and node.module and node.level == 0:
            names.add(node.module.split(".")[0])
    # Keep missing imports too: a deleted helper must still select its users.
    return names


def script_closure(module, scripts_dir, cache=None):
    """A script module plus every local module it imports, transitively."""
    cache = {} if cache is None else cache
    seen, pending = set(), [module]
    while pending:
        name = pending.pop()
        if name in seen:
            continue
        seen.add(name)
        if name not in cache:
            path = scripts_dir / f"{name}.py"
            cache[name] = script_imports(path) if path.is_file() else set()
        pending.extend(cache[name])
    return seen


def process_tests(scripts_dir):
    return sorted(p.stem for p in scripts_dir.glob("test_*process.py"))


def select_process_tests(paths, affected, everything, scripts_dir, smoke=SMOKE_PROCESS_TESTS, direct=None):
    """Process tests for quick and push: the ones a change touches, those that can
    launch a directly changed client, plus the smoke set whenever the change can
    reach every test. A client reached only through reverse dependencies falls under
    that broad rule. Without `direct`, every affected client counts as changed.
    Full runs every process test."""
    all_tests = process_tests(scripts_dir)
    runtime = affected - set(CLIENT_BINARIES) - {"tor-test-support"}
    # Data dependencies are not represented in Python imports. Unknown data
    # beneath scripts (including added/deleted JSON fixtures) selects broadly.
    script_data = any(p.startswith("scripts/") and not p.endswith(".py") for p in paths)
    infrastructure = {"scripts/test_runner.py", "scripts/parallel_tests.py", "scripts/verify.py"} & set(paths)
    broad = everything or runtime or script_data or infrastructure
    cache, texts = {}, {}
    closures = {t: script_closure(t, scripts_dir, cache) for t in all_tests}

    def text(module):
        if module not in texts:
            path = scripts_dir / f"{module}.py"
            texts[module] = path.read_text(encoding="utf-8") if path.is_file() else ""
        return texts[module]

    changed_modules = {Path(p).stem for p in paths if p.startswith("scripts/") and p.endswith(".py")}
    clients = (affected if direct is None else set(direct)) & set(CLIENT_BINARIES)
    selected = set()
    for test, closure in closures.items():
        if closure & changed_modules:
            selected.add(test)
        for package in clients:
            if any(marker in text(m) for m in closure - {HARNESS} for marker in CLIENT_BINARIES[package]):
                selected.add(test)
    if broad:
        selected.update(set(smoke) & set(all_tests))
    return sorted(selected)


def deferred_process_tests(process, paths):
    """Extended-cost tests of the selected modules that quick and push leave to full
    and CI. A test module the change edits keeps all of its tests."""
    keep = set(process) - set(changed_test_modules(paths))
    return sorted(test for test in {*ci_test_contracts.EXTENDED, *LOCAL_EXTENDED}
                  if test.split(".")[0] in keep)


UNTESTED_WARNING = (
    "WARNING: code changed but no tests were added or changed. Features need tests at every layer they touch "
    "and a process test; bug fixes need a regression test that fails first (docs/development/testing.md)."
)
TEST_MARKERS =re.compile(r"^\+\s*#\[(?:\w+::)?test\b|^\+\s*def test_", re.MULTILINE)


def missing_tests(paths, diff):
    """True when code changed but the diff adds no test: features and fixes need tests.

    Refactors can legitimately add none, so this only warns.
    """
    code = [p for p in paths if (p.startswith("crates/") and "/src/" in p and p.endswith(".rs"))
            or (p.startswith("scripts/") and p.endswith(".py") and not Path(p).name.startswith("test_"))]
    tests = [p for p in paths if "/tests/" in p or Path(p).name.startswith("test_")]
    return bool(code) and not tests and not TEST_MARKERS.search(diff)


def test_workers(free_gb, cpus):
    """Parallel Python test workers: half the CPUs (each test starts several
    processes), at most one per 1.5 GB of free memory, between 1 and 6. Native
    graphical tests always run one at a time in an extra lane."""
    return max(1, min(6, (cpus or 1) // 2, int(free_gb / 1.5)))


def jobs_for_memory(free_gb, cpus):
    """Build jobs from free commit memory, per the maintainer machine's measured limits."""
    jobs = 4 if free_gb >= 10 else 2 if free_gb >= 5 else 1
    return max(1, min(jobs, cpus or 1))


def free_memory_gb():
    if os.name == "nt":
        out = subprocess.run(
            ["powershell", "-NoProfile", "-Command", "(Get-CimInstance Win32_OperatingSystem).FreeVirtualMemory"],
            capture_output=True, text=True, encoding="utf-8", check=True,
        ).stdout
        return int(out.strip()) / (1024 * 1024)
    with open("/proc/meminfo", encoding="utf-8") as meminfo:
        for line in meminfo:
            if line.startswith("MemAvailable:"):
                return int(line.split()[1]) / (1024 * 1024)
    return 0.0


def running_builds():
    """Other cargo/rustc processes; overlapping builds exhaust memory on the maintainer's machine."""
    if os.name == "nt":
        out = subprocess.run(["tasklist", "/FO", "CSV", "/NH"], capture_output=True, text=True, check=True).stdout
        names = [line.split(",")[0].strip('"').lower() for line in out.splitlines() if line]
    else:
        out = subprocess.run(["ps", "-eo", "comm="], capture_output=True, text=True, check=True).stdout
        names = [line.strip().lower() for line in out.splitlines()]
    return sorted({n for n in names if n in ("cargo.exe", "rustc.exe", "cargo", "rustc")})


def python_failures(log):
    """unittest ids of failed or errored tests, for a targeted rerun."""
    return sorted(set(re.findall(r"^(?:FAIL|ERROR): \S+ \(([\w.]+)\)", log, re.MULTILINE)))


def environment_hint(log):
    return next((hint for text, hint in ENVIRONMENT_HINTS.items() if text in log), None)


def packages_args(packages):
    return [arg for p in sorted(packages) for arg in ("-p", p)]


def rust_test_command(packages=None, *, release=False):
    """Preserve selection; server tests also emit and exercise the Service host."""
    if packages is None or "tor-server" in packages:
        selected = (["--workspace"] if packages is None else
                    [arg for package in sorted(packages) for arg in ("--package", package)])
        return [sys.executable, str(SCRIPTS / "service_contract_ci.py"), "--profile",
                "release" if release else "debug", *selected]
    return ["cargo", "test", *packages_args(packages), "--locked", *(["--release"] if release else [])]


def ci_partition_steps(steps, profile, partition):
    """Assign complete profile stages, adding only standalone build prerequisites."""
    unknown = sorted({name for name, _, _ in steps} - CI_STAGE_PARTITIONS.keys())
    if unknown:
        raise ValueError("Unclassified CI stage(s): " + ", ".join(unknown))
    selected = [step for step in steps if CI_STAGE_PARTITIONS[step[0]] == partition]
    if partition == "python":
        # Each Python job starts independently of Rust tests. Build every app
        # and the scenario compiler explicitly; the unchanged process harness
        # also checks this build once, normally a no-op. Keep build-tools too
        # so the complete profile's original command remains covered exactly.
        command = ["cargo", "build", "--workspace", "--bins", "--locked"]
        if profile == "release":
            command.append("--release")
        selected.insert(0, (f"build-process-{profile}", command, {}))
    return selected


def full_example_steps(steps, shard=None, inventories=None):
    """Schedule only discovered example owners before their worker stage."""
    import ci_python_shards
    result = []
    for name, command, env in steps:
        if name in ('python-debug', 'process-release'):
            profile = 'debug' if name == 'python-debug' else 'release'
            ids = (ci_python_shards.discover_inventory(profile=profile) if inventories is None
                   else inventories[profile])
            if shard is not None:
                ids = ci_python_shards.selection(ids, shard, profile=profile)
            prerequisite = ci_example_prerequisites.stage(ids, profile)
            if prerequisite:
                result.append(prerequisite)
        result.append((name, command, env))
    return result


def ci_binary_steps(steps, artifact, profile, source_sha):
    """Replace only ordinary app build prerequisites with exact-run validation.

    Tests and their existing stage identities remain unchanged. This is binary
    transport, never cached test evidence; invalid artifacts raise without a
    rebuild or guessed target-directory fallback.
    """
    import ci_binary_artifact
    artifact = Path(artifact).resolve()
    binaries = ci_binary_artifact.validate(artifact, profile, source_sha)
    validate = [sys.executable, str(SCRIPTS / "ci_binary_artifact.py"), "validate", str(artifact),
                "--profile", profile, "--source-sha", source_sha, "--smoke"]
    result = []
    for name, command, env in steps:
        if name in (f"build-process-{profile}", "build-tools"):
            command = validate
        elif python_test_prefix(command) is not None:
            env = {**env, "TOR_TEST_BINARIES": str(binaries),
                   "TOR_SCENARIO_COMPILER": str(binaries / ("tor-scenario.exe" if os.name == "nt" else "tor-scenario"))}
        result.append((name, command, env))
    return result


def plan(tier, affected, process, xvfb=False, ci_profile=None, ci_partition=None, workers=1, failfast=False,
         deferred=(), tested=None):
    """Ordered steps; CI may partition only an explicitly selected full profile.

    quick runs Rust tests for the `tested` packages (default: every affected one)
    and clippy for every affected package. quick and push leave the `deferred`
    process tests out of their run. Full ignores both arguments."""
    if ci_profile is not None and (tier != "full" or ci_profile not in ("debug", "release")):
        raise ValueError("CI profile partitions require full and a debug/release profile")
    if ci_partition is not None and (tier != "full" or ci_profile is None or ci_partition not in CI_PARTITIONS):
        raise ValueError("CI partitions require full, a debug/release profile and a rust/python partition")
    unittest = [sys.executable, str(SCRIPTS / "test_runner.py")]
    options = [*(["--workers", str(workers)] if workers > 1 else []), *(["--failfast"] if failfast else [])]
    if xvfb:
        unittest = ["xvfb-run", "-a", "-s", "-screen 0 1280x1024x24", *unittest]
    discover_all = [*unittest, *options, "discover", "-s", "scripts", "-p", "test_*.py", "-v"]
    # The Python tool tests are cheap and every tier runs them; process tests are selected.
    tool_modules = sorted(p.stem for p in SCRIPTS.glob("test_*.py") if not p.stem.endswith("process"))
    release = {"TOR_TEST_PROFILE": "release"}
    doc = {"RUSTDOCFLAGS": "-D warnings"}
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    compiler_environment = {"TOR_SCENARIO_COMPILER": str(target / "debug" /
                            ("tor-scenario.exe" if os.name == "nt" else "tor-scenario"))}
    build_tools = ("build-tools", ["cargo", "build", "-p", "tor-server", "--bin", "tor-scenario", "--locked"], {})
    selected = packages_args(affected)
    host_feature = (["--features", "tor-server/service-contract-host"]
                    if tier == "full" or "tor-server" in affected else [])
    isolated = ("simulation-isolated", ["cargo", "test", "-p", "tor-simulation", "--test", "it", "--locked", "fixture_contract::"], {})
    steps = [("fmt", ["cargo", "fmt", "--all", "--check"], {})]
    rustdoc = ("rustdoc", ["cargo", "doc", "--workspace", "--no-deps", "--document-private-items", "--locked", *host_feature], doc)
    if tier in ("quick", "push"):
        # Local gates give fast feedback on what the change can reach; CI selects
        # reviewed contracts or exhaustive coverage with separate assurance.
        # push adds the workspace-wide lint and API docs CI would reject.
        if tier == "push":
            steps.append(("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", *host_feature, "--", "-D", "warnings"], {}))
        elif affected:
            steps.append(("clippy", ["cargo", "clippy", *selected, "--all-targets", "--locked", *host_feature, "--", "-D", "warnings"], {}))
        steps.append(("architecture", [sys.executable, "scripts/check_architecture.py"], {}))
        steps.append(build_tools)
        steps.append(("python-tools", [*unittest, *options, *tool_modules], {"_cwd": "scripts", **compiler_environment}))
        if tier == "push" and "tor-simulation" in affected:
            steps.append(isolated)
        rust = affected if tier == "push" or tested is None else set(tested) & affected
        if rust:
            steps.append(("rust-debug", rust_test_command(rust), {}))
        if process:
            excluded = [arg for test in sorted(deferred) for arg in ("--exclude", test)]
            steps.append(("process-debug", [*unittest, *options, *excluded, "-v", *process], {"_cwd": "scripts"}))
        if tier == "push":
            steps.append(rustdoc)
        return steps
    steps += [
        ("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", *host_feature, "--", "-D", "warnings"], {}),
        isolated,
        build_tools,
        ("python-debug", discover_all, compiler_environment),
        ("architecture", [sys.executable, "scripts/check_architecture.py"], {}),
        rustdoc,
        ("rust-debug", rust_test_command(), {}),
    ]
    if tier == "full":
        release_steps = [
            ("rust-release", rust_test_command(release=True), {}),
            ("process-release", [*unittest, *options, "discover", "-s", "scripts", "-p", "test_*process.py", "-v"], release),
        ]
        if ci_profile == "release":
            steps = release_steps
        elif ci_profile is None:
            steps += release_steps
        if ci_partition is not None:
            return ci_partition_steps(steps, ci_profile, ci_partition)
    return steps


def changes(base):
    """Changed paths since the merge base (committed, uncommitted, and untracked) and the tracked diff."""
    def git(*args):
        return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True,
                              encoding="utf-8", errors="replace", check=True).stdout

    merge_base = git("merge-base", base, "HEAD").strip()
    paths = git("diff", "--name-only", merge_base).splitlines()
    untracked = git("ls-files", "--others", "--exclude-standard").splitlines()
    diff = git("diff", "--unified=0", merge_base)
    for path in untracked:
        try:
            diff += "".join("+" + line + "\n" for line in (ROOT / path).read_text(encoding="utf-8").splitlines())
        except (OSError, UnicodeDecodeError):
            pass
    return sorted({p for p in paths + untracked if p}), diff


class Lock:
    def __init__(self, path):
        self.path = path

    def __enter__(self):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        try:
            fd = os.open(self.path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError:
            raise SystemExit(f"Another verify run holds {self.path}. If none is running, delete it.") from None
        with os.fdopen(fd, "w") as handle:
            handle.write(str(os.getpid()))
        return self

    def __exit__(self, *exc):
        self.path.unlink(missing_ok=True)


LOCAL_MOUSE_TEST = "test_travel_process.TravelProcesses.test_native_underscore_and_mouse_click"



def python_test_prefix(command):
    """Retain wrappers and interpreter flags for both unittest entry points."""
    for index, argument in enumerate(command):
        if argument == "unittest" or Path(argument).name == "test_runner.py":
            return command[:index + 1]
    return None


def waivable_mouse_failure(code, command, text, enabled, ci):
    """An explicit local exception for one test, never a suite/build exception."""
    return (enabled and not ci and code == 1 and python_test_prefix(command) is not None
            and python_failures(text) == [LOCAL_MOUSE_TEST]
            and any(line in ("FAILED (failures=1)", "FAILED (errors=1)")
                    for line in text.splitlines()))


def graphical_waiver_evidence(path, expected):
    """A zero exit is insufficient: every authorized exclusion must be recorded."""
    records = [json.loads(line) for line in Path(path).read_text(encoding="utf-8").splitlines()]
    declared = [row for row in records if row.get("kind") == "local_graphical_waiver"]
    actual = [row["test_id"] for row in records
              if row.get("kind") == "test" and row.get("outcome") == "waived"]
    leaked = [row for row in records if row.get("outcome") == "skipped"
              and isinstance(row.get("skip_reason"), str)
              and row["skip_reason"].startswith(graphical_waiver.REASON_PREFIX)]
    suites = [row for row in records if row.get("kind") == "suite"]
    if (leaked or not records or records[-1].get("kind") != "suite"
            or len(declared) != 1 or declared[0].get("tests") != expected
            or len(actual) != len(set(actual)) or sorted(actual) != expected
            or len(suites) != 1 or suites[0].get("waived") != len(expected)
            or sorted(suites[0].get("waived_tests", [])) != expected):
        raise ValueError("graphical waiver accounting does not match the exact manifest")
    return sorted(actual)


def process_summary(tier, selected, available, deferred=()):
    """quick and push select process tests and defer extended-cost ones; full runs every one."""
    if tier in ("quick", "push"):
        summary = f"process tests: {len(selected)} of {len(available)} (touched, changed clients and smoke)"
        if deferred:
            summary += f"; {len(deferred)} extended-cost tests deferred to full and CI"
        return summary
    return f"process tests: all {len(available)}"


def slowest_tests(timing_files, count=5):
    """The slowest recorded tests, so growth in suite cost stays visible."""
    rows = []
    for path in timing_files:
        try:
            with Path(path).open("rb") as handle:
                raw = handle.read(16 * 1024 * 1024 + 1)
            if len(raw) > 16 * 1024 * 1024: continue
            candidates = [json.loads(line) for line in raw.splitlines()]
        except (OSError, ValueError, RecursionError):
            continue
        rows += [row for row in candidates if isinstance(row, dict) and row.get("kind") == "test"
                 and isinstance(row.get("test_id"), str) and len(row["test_id"]) <= 1024
                 and type(row.get("seconds")) in (int, float) and 0 <= row["seconds"] <= sys.float_info.max]
    return sorted(rows, key=lambda row: -row.get("seconds", 0))[:count]


def changed_test_modules(paths):
    """Test modules a change touched; parallel runs start them first."""
    return sorted(Path(p).stem for p in paths
                  if p.startswith("scripts/") and Path(p).name.startswith("test_") and p.endswith(".py"))


def verification_environment(jobs, overrides):
    """Explicit verifier overrides shared with the dependency-cache recipe."""
    return {"CARGO_BUILD_JOBS": str(jobs), "CARGO_PROFILE_DEV_DEBUG": "0",
            "PYTHONUTF8": "1", "PYTHONIOENCODING": "utf-8", **overrides}


def receipt_completeness(steps, results, logs, jobs):
    """Bounded reporting under the existing reviewed-runner trust model; never a gate."""
    issues, stages = [], []
    names = [name for name, _, _ in steps]
    if not names or len(names) != len(set(names)) or names != [row.get("step") for row in results]:
        issues.append("planned/executed stage mismatch")

    def require(condition):
        if not condition: raise ValueError("incomplete record")

    def unique(pairs):
        value = {}
        for key, item in pairs:
            require(key not in value)
            value[key] = item
        return value

    def decode(line):
        value = json.loads(line, object_pairs_hook=unique, parse_constant=lambda _: require(False))
        require(isinstance(value, dict))
        return value

    def zero(row, keys):
        return all(type(row.get(key)) is int and row[key] == 0 for key in keys)

    for (name, command, overrides), result in zip(steps, results):
        stage = {"step": name, "complete": False}
        stages.append(stage)
        try:
            require(type(result.get("code")) is int and result["code"] == 0
                    and not result["note"].startswith("WAIVED") and result["waived_tests"] == [])
            require(result.get("command") == command and result.get("cwd") == Path(overrides.get("_cwd", ".")).as_posix())
            environment = result["environment"]
            require(environment.get("scope") == "verifier_stage" and result["environment_issues"] == [])
            values = environment["variables"]
            require(set(values) == set(ci_observation.BUILD_TEST_ENVIRONMENT)
                    and all(value == {"status": "absent"} or (set(value) == {"status", "sha256"}
                            and value["status"] == "present" and re.fullmatch(r"[0-9a-f]{64}", value["sha256"])) for value in values.values()))
            require(environment["sha256"] == hashlib.sha256(
                json.dumps(values, sort_keys=True, separators=(",", ":")).encode()).hexdigest())
            explicit = verification_environment(jobs, {key: value for key, value in overrides.items() if not key.startswith("_")})
            expected = ci_observation.build_test_environment(explicit, [])
            require(all(key in values and values[key] == expected["variables"][key] for key in explicit))
            python = python_test_prefix(command) is not None
            service = any(Path(arg).name == "service_contract_ci.py" for arg in command)
            path = Path(logs) / f"{name}-tests.jsonl" if python else Path(result["log"])
            with path.open("rb") as handle:
                raw = handle.read(16 * 1024 * 1024 + 1)
            require(0 < len(raw) <= 16 * 1024 * 1024)
            stage["record"] = {"file": path.name, "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}
            lines = raw.decode("utf-8").splitlines()
            if python:
                rows = [decode(line) for line in lines]
                require(all(type(row.get("schema_version")) is int and row["schema_version"] == 1 for row in rows))
                profile = values["TOR_TEST_PROFILE"]
                require(all(row.get("profile") in ("debug", "release") and (
                    row["profile"] == "debug" if profile["status"] == "absent" else
                    hashlib.sha256(row["profile"].encode()).hexdigest() == profile["sha256"]) for row in rows))
                require(rows[0].get("kind") == "selection" and rows[-1].get("kind") == "suite")
                require(all(row.get("kind") in ("test", "build") for row in rows[1:-1]))
                builds = [row for row in rows[1:-1] if row["kind"] == "build"]
                require(len(builds) <= 1 and all(
                    set(row) == {"schema_version", "kind", "profile", "seconds", "outcome", "returncode"}
                    and row["outcome"] == "passed" and zero(row, ("returncode",))
                    and type(row["seconds"]) in (int, float) and 0 <= row["seconds"] <= sys.float_info.max
                    for row in builds))
                selected = rows[0].get("test_ids")
                require(isinstance(selected, list) and 0 < len(selected) <= 10000)
                require(all(isinstance(item, str) and len(item) <= 1024
                            and re.fullmatch(r"[A-Za-z_]\w*(?:\.[A-Za-z_]\w*){2,}", item) for item in selected))
                require(len(selected) == len(set(selected)))
                tests, terminal = [row for row in rows[1:-1] if row["kind"] == "test"], rows[-1]
                require(sorted(row["test_id"] for row in tests) == sorted(selected))
                require(all(row.get("outcome") == "passed"
                            and zero(row, ("failures", "errors", "subtests_failed", "subtests_skipped")) for row in tests))
                require(terminal.get("successful") is True and type(terminal.get("tests_run")) is int
                        and terminal["tests_run"] == len(selected)
                        and zero(terminal, ("failures", "errors", "skipped", "waived", "expected_failures", "unexpected_successes"))
                        and not terminal.get("waived_tests") and not terminal.get("not_started") and not terminal.get("batch_errors"))
                stage["selected_tests"] = len(selected)
            elif service:
                tagged = [[decode(line[len(prefix):]) for line in lines if line.startswith(prefix)]
                          for prefix in ("SERVICE_CONTRACT_ARTIFACT ", "SERVICE_CONTRACT_RESULT ")]
                require(all(len(records) == 1 for records in tagged))
                artifact, outcome = tagged[0][0], tagged[1][0]
                require(set(artifact) == {"executable", "sha256", "profile", "features"}
                        and set(outcome) == {"selected", "run", "failures", "errors", "skipped"})
                profile = command[command.index("--profile") + 1]
                require(artifact.get("profile") == profile and profile in ("debug", "release")
                        and isinstance(artifact.get("sha256"), str) and re.fullmatch(r"[0-9a-f]{64}", artifact["sha256"])
                        and isinstance(artifact.get("executable"), str) and 0 < len(artifact["executable"]) <= 4096
                        and isinstance(artifact.get("features"), list) and 0 < len(artifact["features"]) <= 64
                        and all(isinstance(feature, str) and re.fullmatch(r"[A-Za-z0-9_-]{1,128}", feature) for feature in artifact["features"])
                        and len(artifact["features"]) == len(set(artifact["features"])) and "service-contract-host" in artifact["features"])
                require(type(outcome.get("selected")) is int and 0 < outcome["selected"] <= 10000
                        and type(outcome.get("run")) is int and outcome["run"] == outcome["selected"]
                        and zero(outcome, ("failures", "errors", "skipped")))
                stage.update(service_artifact=artifact, service_result=outcome)
            stage["complete"] = True
        except (OSError, ValueError, TypeError, KeyError, IndexError, AttributeError, RecursionError):
            issues.append(f"{name}: missing, malformed or incomplete evidence")
    return {"schema_version": 1, "reporting_only": True, "complete": not issues, "issues": issues, "stages": stages}


def run_step(name, command, env, logs, jobs, rerun):
    cwd = ROOT / env.pop("_cwd", ".")
    graphical = env.pop("_graphical_waiver", None)
    environment = {**os.environ, **verification_environment(jobs, env)}
    environment["TOR_TEST_TIMINGS"] = str(logs / f"{name}-tests.jsonl")
    environment["TOR_TEST_ARTIFACTS"] = str(logs / f"{name}-process-failures")
    environment_issues = []
    observed_environment = ci_observation.build_test_environment(environment, environment_issues)
    observed_environment["scope"] = "verifier_stage"
    executed_command = list(command)
    log = logs / f"{name}.log"
    started = time.monotonic()
    with open(log, "w", encoding="utf-8") as handle:
        handle.write("$ " + " ".join(command) + "\n")
        handle.flush()
        code = subprocess.run(command, cwd=cwd, env=environment, stdout=handle, stderr=subprocess.STDOUT).returncode
    result = {"step": name, "code": code, "seconds": time.monotonic() - started,
              "log": log, "note": "", "waived_tests": [], "command": executed_command,
              "cwd": cwd.relative_to(ROOT).as_posix() if cwd.is_relative_to(ROOT) else None,
              "environment": observed_environment, "environment_issues": environment_issues}
    notes = []
    if graphical is not None:
        try:
            result["waived_tests"] = graphical_waiver_evidence(
                environment["TOR_TEST_TIMINGS"], graphical["tests"])
        except (OSError, ValueError, TypeError, KeyError) as error:
            result["code"] = code or 2
            notes.append(f"graphical waiver accounting FAILED: {error}")
        else:
            if code == 0:
                result["note"] = (f"WAIVED {len(result['waived_tests'])} local graphical tests (not run): "
                                  + ", ".join(result["waived_tests"]))
            else:
                notes.append(f"{len(result['waived_tests'])} local graphical tests waived; other checks failed")
    if result["code"] == 0:
        return result
    text = log.read_text(encoding="utf-8", errors="replace")
    if graphical is None and waivable_mouse_failure(code, command, text,
            env.get("TOR_LOCAL_MOUSE_WAIVER") == "1", bool(environment.get("CI"))):
        result["code"] = 0
        result["note"] = "WAIVED local failure: " + LOCAL_MOUSE_TEST
        return result
    hint = environment_hint(text)
    if hint:
        notes.append(f"environment? {hint}")
    prefix = python_test_prefix(command)
    failed = python_failures(text) if prefix else []
    if failed:
        notes.append("failed: " + ", ".join(failed))
        if rerun:
            environment["TOR_TEST_TIMINGS"] = str(logs / f"{name}-rerun-tests.jsonl")
            environment["TOR_TEST_ARTIFACTS"] = str(logs / f"{name}-rerun-process-failures")
            rerun_log = logs / f"{name}-rerun.log"
            with open(rerun_log, "w", encoding="utf-8") as handle:
                again = subprocess.run([*prefix, "-v", *failed], cwd=SCRIPTS, env=environment,
                                       stdout=handle, stderr=subprocess.STDOUT).returncode
            # The step stays failed either way: a pass on rerun marks a flake, not a pass.
            notes.append("rerun passed (intermittent)" if again == 0 else "rerun failed too")
    result["note"] = "; ".join(notes)
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("tier", nargs="?", default="push", choices=TIERS)
    parser.add_argument("--ci-profile", choices=("debug", "release"),
                        help="CI-only profile of full; both profiles remain required")
    parser.add_argument("--ci-partition", choices=CI_PARTITIONS,
                        help="CI-only Rust or Python/native stages; requires full and --ci-profile; all partitions are required")
    parser.add_argument("--ci-python-shard", type=int, choices=(1, 2),
                        help="Explicit complete Windows Python profile shard; both are required")
    parser.add_argument("--ci-binaries", type=Path, help="Exact-run ordinary application artifact for fresh full Python partitions")
    parser.add_argument("--base", default="origin/main", help="Compare against this ref to find changes")
    parser.add_argument("--jobs", type=int, help="CARGO_BUILD_JOBS; chosen from free memory by default")
    parser.add_argument("--rerun-failed", action="store_true", help="Rerun failed Python tests once to show whether they are intermittent")
    parser.add_argument("--keep-going", action="store_true", help="Run later steps after a failure")
    parser.add_argument("--test-workers", type=int,
                        help="Parallel Python test workers; chosen from CPUs and free memory by default (1 runs serially)")
    parser.add_argument("--record-durations", action="store_true",
                        help="Refresh scripts/test_durations.json (parallel ordering hints) from this run's passed tests")
    parser.add_argument("--fail-fast", action="store_true",
                        help="Stop starting Python tests after the first failure (local feedback; never a gate result)")
    parser.add_argument("--allow-concurrent", action="store_true", help="Start even though other cargo/rustc processes are running")
    parser.add_argument("--with-extended", action="store_true",
                        help="quick/push: also run the extended-cost process tests they defer to full and CI")
    parser.add_argument("--dry-run", action="store_true", help="Print the selection and steps without running them")
    parser.add_argument("--waive-local-mouse", action="store_true",
                        help="Record only the native travel mouse test failure as locally waived; forbidden in CI")
    parser.add_argument("--waive-local-graphical", metavar="MANIFEST",
                        help="Do not run exactly listed, audited graphical tests; local push/full only, forbidden in CI")
    args = parser.parse_args(argv)
    if args.ci_profile and args.tier != "full":
        parser.error("--ci-profile requires the full tier")
    if args.ci_partition and (args.tier != "full" or args.ci_profile is None):
        parser.error("--ci-partition requires full and --ci-profile")
    if args.ci_python_shard is not None and (sys.platform != "win32" or args.tier != "full" or args.ci_profile not in ("debug", "release")
            or args.ci_partition != "python" or args.fail_fast or args.rerun_failed):
        parser.error("--ci-python-shard requires fresh complete Windows profile Python verification")
    if args.ci_binaries and (args.tier != "full" or args.ci_partition != "python" or not args.ci_profile
            or args.fail_fast or args.rerun_failed or not os.environ.get("CI")):
        parser.error("--ci-binaries requires fresh full CI Python verification without evidence reuse")
    if args.waive_local_mouse and (args.ci_profile or os.environ.get("CI")):
        parser.error("--waive-local-mouse is local only")
    graphical_manifest = None
    if args.waive_local_graphical:
        if args.ci_profile or os.environ.get("CI"):
            parser.error("--waive-local-graphical is local only")
        if args.tier != "full":
            parser.error("--waive-local-graphical requires full so all listed tests are selected")
        if args.waive_local_mouse:
            parser.error("choose one local waiver mode; list the mouse test in the graphical manifest if authorized")
        try:
            graphical_manifest = graphical_waiver.read_manifest(args.waive_local_graphical)
        except (OSError, ValueError) as error:
            parser.error(str(error))
    label = f"full-ci-{args.ci_profile}" if args.ci_profile else args.tier
    if args.ci_partition:
        label += f"-{args.ci_partition}"
    if args.ci_python_shard is not None:
        label += f"-shard-{args.ci_python_shard}"

    metadata = json.loads(subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
        cwd=ROOT, capture_output=True, text=True, encoding="utf-8", check=True,
    ).stdout)
    dirs, deps = workspace(metadata)
    if args.ci_profile:
        # CI checks the complete workspace, including shallow checkouts without
        # origin/main. Change selection is only needed for local tiers.
        paths, diff = [], ""
        direct, everything, unknown = set(dirs), True, []
    else:
        paths, diff = changes(args.base)
        direct, everything, unknown = classify(paths, dirs)
    untested = missing_tests(paths, diff)
    affected = set(dirs) if everything else dependents(direct, deps)
    process = select_process_tests(paths, affected, everything, SCRIPTS, direct=direct)
    deferred = [] if args.with_extended or args.tier == "full" else deferred_process_tests(process, paths)
    # quick tests directly changed packages; push tests every affected one.
    tested = affected if everything else direct & affected
    free = free_memory_gb()
    jobs = args.jobs or jobs_for_memory(free, os.cpu_count())
    if args.test_workers is not None and args.test_workers < 1:
        parser.error("--test-workers must be at least 1")
    workers = args.test_workers or test_workers(free, os.cpu_count())
    steps = plan(args.tier, affected, process,
                 xvfb=args.ci_partition != "rust" and sys.platform.startswith("linux")
                      and not os.environ.get("DISPLAY") and graphical_manifest is None,
                 ci_profile=args.ci_profile, ci_partition=args.ci_partition,
                 workers=workers, failfast=args.fail_fast, deferred=deferred, tested=tested)
    if args.ci_python_shard is not None:
        steps = [(name, [*command[:2], "--ci-python-shard", str(args.ci_python_shard), *command[2:]],
                          {**env, "TOR_TEST_PROFILE": "debug"} if name == "python-debug" else env)
                 if name in ("process-release", "python-debug") else (name, command, env) for name, command, env in steps]
    if args.ci_python_shard is not None:
        steps = full_example_steps(steps, args.ci_python_shard)
    if args.ci_binaries:
        steps = ci_binary_steps(steps, args.ci_binaries, args.ci_profile, os.environ.get("GITHUB_SHA", ""))
    priority = changed_test_modules(paths)
    if priority:
        steps = [(name, command, {**env, "TOR_TEST_PRIORITY": ",".join(priority)}) for name, command, env in steps]

    if args.waive_local_mouse:
        steps = [(name, command, {**env, "TOR_LOCAL_MOUSE_WAIVER": "1"})
                 for name, command, env in steps]
    selection = "complete workspace" if args.ci_profile else f"{len(paths)} changed paths since {args.base}"
    print(f"tier {label}; {selection}; jobs {jobs}; test workers {workers} ({free:.1f} GB free)")
    if unknown:
        print("unmapped paths select everything: " + ", ".join(unknown[:5]) + (" ..." if len(unknown) > 5 else ""))
    print("packages: " + (", ".join(sorted(affected)) or "none"))
    if args.tier == "quick" and affected - tested:
        print("clippy only (tested by push): " + ", ".join(sorted(affected - tested)))
    print(process_summary(args.tier, process, process_tests(SCRIPTS), deferred))
    if untested:
        print(UNTESTED_WARNING)
    if args.dry_run:
        for name, command, env in steps:
            print(f"  {name}: {' '.join(command)}")
        if graphical_manifest is not None:
            print("Exact local graphical waivers: " + ", ".join(graphical_manifest["tests"]))
        return 0
    others = running_builds()
    if others and not args.allow_concurrent:
        print("Other builds are running (" + ", ".join(others) + "); wait for them or pass --allow-concurrent.", file=sys.stderr)
        return 2

    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    logs = ROOT / ".local" / "verify" / f"{stamp}-{label}"
    logs.mkdir(parents=True)
    if graphical_manifest is not None:
        snapshot = logs / "graphical-waiver.json"
        snapshot.write_text(json.dumps(graphical_manifest, indent=2) + "\n", encoding="utf-8")
        updated = []
        for name, command, env in steps:
            prefix = python_test_prefix(command)
            if prefix is not None:
                command = [*prefix, "--waive-local-graphical", str(snapshot), *command[len(prefix):]]
                env = {**env, "_graphical_waiver": graphical_manifest}
            updated.append((name, command, env))
        steps = updated
    results, failed = [], False
    with Lock(ROOT / ".local" / "verify.lock"):
        for name, command, env in steps:
            if failed and not args.keep_going:
                results.append({"step": name, "code": None, "seconds": 0, "log": None, "note": "not run"})
                continue
            result = run_step(name, command, dict(env), logs, jobs, args.rerun_failed)
            failed |= result["code"] != 0
            results.append(result)
            print(f"  {name}: {'WAIVED' if result['note'].startswith('WAIVED') else 'ok' if result['code'] == 0 else 'FAILED'} ({result['seconds']:.0f}s)", flush=True)

    lines = [f"{'step':16} {'result':8} {'time':>7}  note"]
    for r in results:
        status = "waived" if r["note"].startswith("WAIVED") else "not run" if r["code"] is None else "ok" if r["code"] == 0 else "FAILED"
        lines.append(f"{r['step']:16} {status:8} {r['seconds']:6.0f}s  {r['note']}")
    total = sum(r["seconds"] for r in results)
    lines.append(f"total {total / 60:.1f} min; logs in {logs.relative_to(ROOT).as_posix()}")
    commands = {name: command for name, command, _ in steps}
    timing_files = [logs / f"{r['step']}-tests.jsonl" for r in results
                    if r["code"] is not None and python_test_prefix(commands[r["step"]]) is not None]
    slowest = slowest_tests(timing_files)
    if slowest:
        lines.append("slowest tests: " + "; ".join(f"{row['test_id']} {row['seconds']:.0f}s" for row in slowest))
    if args.record_durations and timing_files:
        recorded = parallel_tests.record_durations([path for path in timing_files if path.exists()])
        lines.append(f"recorded {len(recorded)} test durations in scripts/test_durations.json")
    if untested:
        lines.append(UNTESTED_WARNING)
    if args.tier == "quick" and affected - tested:
        lines.append("clippy only (tested by push): " + ", ".join(sorted(affected - tested)))
    if deferred:
        lines.append(f"deferred {len(deferred)} extended-cost process tests to full and CI "
                     "(--with-extended runs them): " + ", ".join(deferred))
    if args.ci_profile:
        partition = f"{args.ci_profile}/{args.ci_partition}" if args.ci_partition else args.ci_profile
        lines.append(f"CI {partition} partition only; all Rust and Python/native partitions of both profiles on both platforms are required. This partition is not a full run or a merge gate.")
    elif args.tier == "quick":
        lines.append("Run push before pushing. Both CI platform aggregates are required; the canonical plan selects contract or exhaustive assurance.")
    elif args.tier == "push":
        status = "Local debug push gate FAILED; resolve failures before pushing. " if failed else "Local debug push gate complete. "
        lines.append(status + "Both CI platform aggregates are required; the canonical plan selects contract or exhaustive assurance. "
                     "Run targeted local release checks for performance or release-specific behavior; use full when CI cannot run or on request.")
    else:
        lines.append("CI on both platforms is required before merging.")
    summary = "\n".join(lines)
    (logs / "summary.txt").write_text(summary + "\n", encoding="utf-8")
    waived = [result["step"] for result in results if result["note"].startswith("WAIVED")]
    not_run = [result["step"] for result in results if result["code"] is None]
    evidence = {"schema_version": 1, "tier": args.tier, "ci_profile": args.ci_profile,
                "ci_partition": args.ci_partition, "ci_python_shard": args.ci_python_shard,
                "application_artifact": str(args.ci_binaries) if args.ci_binaries else None,
                "scope": "ci-partition" if args.ci_profile else "local", "jobs": jobs,
                "test_workers": workers, "planned_steps": [name for name, _, _ in steps],
                "receipt_completeness": receipt_completeness(steps, results, logs, jobs),
                "gate_passed": not failed, "clean_passed": not failed and not waived and not not_run,
                "waived_steps": waived, "not_run_steps": not_run, "total_seconds": total,
                "deferred_process_tests": deferred,
                "graphical_waiver": graphical_manifest,
                "steps": [{**result, "log": str(result["log"].relative_to(ROOT)) if result["log"] else None,
                           "status": "waived" if result["step"] in waived else "not_run" if result["code"] is None
                                     else "passed" if result["code"] == 0 else "failed",
                           "command": result.get("command", command)} for result, (_, command, _) in zip(results, steps)]}
    evidence["evidence_origin"] = "fresh"
    (logs / "summary.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    print(summary)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
