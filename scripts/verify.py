"""Run the testing policy's checks in tiers, log each step, and print a compact summary.

Tiers (see docs/development/testing.md#running-the-checks):

  quick  at stable checkpoints: formatting, clippy and debug tests for affected
         packages, the Python tool tests, and the process tests the change maps to
  push   before pushing: every debug check CI runs; release coverage runs in CI
  full   exactly what CI runs on one platform, debug and release; also covers
         the push gate for unchanged inputs, toolchain and test configuration

Affected packages include every workspace package that depends on a changed one.
Anything the mapping doesn't recognize selects everything, so a tier can run more
than it needs but never less. CI's full Windows and Linux matrix remains the
required gate before merging. CI partitions each debug/release profile into Rust
and Python/native checks. All partitions are required; one is never a full or
merge gate. The complete debug profile has the same checks as local push;
CI evidence does not certify a local run.

Tiers only choose which existing tests run. Every feature and bug fix must add
its tests to the suite (docs/development/testing.md), and this script warns when code
changes arrive without any.
"""

import argparse
import ast
import datetime
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

import graphical_waiver

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
RUST_TEST_INPUTS = {"docs/reference/protocol.md": {"tor-protocol"}}
# Paths that otherwise never affect Rust packages or process behavior.
INERT_PREFIXES = ("docs/", "perf/", ".github/", "README.md", "CONTRIBUTING.md", "AGENTS.md", "LICENSE", "ruff.toml", ".gitignore")
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


def select_process_tests(paths, affected, everything, scripts_dir, smoke=SMOKE_PROCESS_TESTS):
    """Process tests for the quick tier: the ones a change touches, plus the smoke set
    whenever the change can reach every test. Push and full always run them all."""
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
    selected = set()
    for test, closure in closures.items():
        if closure & changed_modules:
            selected.add(test)
        for package in affected & set(CLIENT_BINARIES):
            if any(marker in text(m) for m in closure - {HARNESS} for marker in CLIENT_BINARIES[package]):
                selected.add(test)
    if broad:
        selected.update(set(smoke) & set(all_tests))
    return sorted(selected)


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


def plan(tier, affected, process, xvfb=False, ci_profile=None, ci_partition=None, workers=1, failfast=False):
    """Ordered steps; CI may partition only an explicitly selected full profile."""
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
    isolated = ("simulation-isolated", ["cargo", "test", "-p", "tor-simulation", "--test", "it", "--locked", "fixture_contract::"], {})
    steps = [("fmt", ["cargo", "fmt", "--all", "--check"], {})]
    if tier == "quick":
        if affected:
            steps.append(("clippy", ["cargo", "clippy", *selected, "--all-targets", "--locked", "--", "-D", "warnings"], {}))
        steps.append(("architecture", [sys.executable, "scripts/check_architecture.py"], {}))
        steps.append(build_tools)
        steps.append(("python-tools", [*unittest, *options, *tool_modules], {"_cwd": "scripts", **compiler_environment}))
        if "tor-simulation" in affected:
            steps.append(isolated)
        if affected:
            steps.append(("rust-debug", ["cargo", "test", *selected, "--locked"], {}))
        if process:
            steps.append(("process-debug", [*unittest, *options, "-v", *process], {"_cwd": "scripts"}))
        return steps
    steps += [
        ("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"], {}),
        isolated,
        build_tools,
        ("python-debug", discover_all, compiler_environment),
        ("architecture", [sys.executable, "scripts/check_architecture.py"], {}),
        ("rustdoc", ["cargo", "doc", "--workspace", "--no-deps", "--document-private-items", "--locked"], doc),
        ("rust-debug", ["cargo", "test", "--workspace", "--locked"], {}),
    ]
    if tier == "full":
        release_steps = [
            ("rust-release", ["cargo", "test", "--workspace", "--release", "--locked"], {}),
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


def changed_test_modules(paths):
    """Test modules a change touched; parallel runs start them first."""
    return sorted(Path(p).stem for p in paths
                  if p.startswith("scripts/") and Path(p).name.startswith("test_") and p.endswith(".py"))


def run_step(name, command, env, logs, jobs, rerun):
    cwd = ROOT / env.pop("_cwd", ".")
    graphical = env.pop("_graphical_waiver", None)
    environment = {**os.environ, "CARGO_BUILD_JOBS": str(jobs), "CARGO_PROFILE_DEV_DEBUG": "0",
                   "PYTHONUTF8": "1", "PYTHONIOENCODING": "utf-8", **env}
    environment["TOR_TEST_TIMINGS"] = str(logs / f"{name}-tests.jsonl")
    environment["TOR_TEST_ARTIFACTS"] = str(logs / f"{name}-process-failures")
    log = logs / f"{name}.log"
    started = time.monotonic()
    with open(log, "w", encoding="utf-8") as handle:
        handle.write("$ " + " ".join(command) + "\n")
        handle.flush()
        code = subprocess.run(command, cwd=cwd, env=environment, stdout=handle, stderr=subprocess.STDOUT).returncode
    result = {"step": name, "code": code, "seconds": time.monotonic() - started,
              "log": log, "note": "", "waived_tests": []}
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
    parser.add_argument("--base", default="origin/main", help="Compare against this ref to find changes")
    parser.add_argument("--jobs", type=int, help="CARGO_BUILD_JOBS; chosen from free memory by default")
    parser.add_argument("--rerun-failed", action="store_true", help="Rerun failed Python tests once to show whether they are intermittent")
    parser.add_argument("--keep-going", action="store_true", help="Run later steps after a failure")
    parser.add_argument("--test-workers", type=int,
                        help="Parallel Python test workers; chosen from CPUs and free memory by default (1 runs serially)")
    parser.add_argument("--fail-fast", action="store_true",
                        help="Stop starting Python tests after the first failure (local feedback; never a gate result)")
    parser.add_argument("--allow-concurrent", action="store_true", help="Start even though other cargo/rustc processes are running")
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
    if args.waive_local_mouse and (args.ci_profile or os.environ.get("CI")):
        parser.error("--waive-local-mouse is local only")
    graphical_manifest = None
    if args.waive_local_graphical:
        if args.ci_profile or os.environ.get("CI"):
            parser.error("--waive-local-graphical is local only")
        if args.tier == "quick":
            parser.error("--waive-local-graphical requires push or full so all listed tests are selected")
        if args.waive_local_mouse:
            parser.error("choose one local waiver mode; list the mouse test in the graphical manifest if authorized")
        try:
            graphical_manifest = graphical_waiver.read_manifest(args.waive_local_graphical)
        except (OSError, ValueError) as error:
            parser.error(str(error))
    label = f"full-ci-{args.ci_profile}" if args.ci_profile else args.tier
    if args.ci_partition:
        label += f"-{args.ci_partition}"

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
    process = select_process_tests(paths, affected, everything, SCRIPTS)
    free = free_memory_gb()
    jobs = args.jobs or jobs_for_memory(free, os.cpu_count())
    if args.test_workers is not None and args.test_workers < 1:
        parser.error("--test-workers must be at least 1")
    workers = args.test_workers or test_workers(free, os.cpu_count())
    steps = plan(args.tier, affected, process,
                 xvfb=args.ci_partition != "rust" and sys.platform.startswith("linux")
                      and not os.environ.get("DISPLAY") and graphical_manifest is None,
                 ci_profile=args.ci_profile, ci_partition=args.ci_partition,
                 workers=workers, failfast=args.fail_fast)
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
    print(f"process tests: {len(process)} of {len(process_tests(SCRIPTS))}")
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
    if untested:
        lines.append(UNTESTED_WARNING)
    if args.ci_profile:
        partition = f"{args.ci_profile}/{args.ci_partition}" if args.ci_partition else args.ci_profile
        lines.append(f"CI {partition} partition only; all Rust and Python/native partitions of both profiles on both platforms are required. This partition is not a full run or a merge gate.")
    elif args.tier == "quick":
        lines.append("Run push or full before pushing; a successful full run covers the push gate for unchanged inputs and configuration. CI on both platforms is required before merging.")
    elif args.tier == "push":
        status = "Local debug push gate FAILED; resolve failures before pushing. " if failed else "Local debug push gate complete. "
        lines.append(status + "Full debug/release CI on both platforms is required before merging. "
                     "Run targeted local release checks for performance or release-specific behavior; use full when CI cannot run or on request.")
    else:
        lines.append("CI on both platforms is required before merging.")
    summary = "\n".join(lines)
    (logs / "summary.txt").write_text(summary + "\n", encoding="utf-8")
    waived = [result["step"] for result in results if result["note"].startswith("WAIVED")]
    not_run = [result["step"] for result in results if result["code"] is None]
    evidence = {"schema_version": 1, "tier": args.tier, "ci_profile": args.ci_profile,
                "ci_partition": args.ci_partition,
                "scope": "ci-partition" if args.ci_profile else "local", "jobs": jobs,
                "gate_passed": not failed, "clean_passed": not failed and not waived and not not_run,
                "waived_steps": waived, "not_run_steps": not_run, "total_seconds": total,
                "graphical_waiver": graphical_manifest,
                "steps": [{**result, "log": str(result["log"].relative_to(ROOT)) if result["log"] else None,
                           "status": "waived" if result["step"] in waived else "not_run" if result["code"] is None
                                     else "passed" if result["code"] == 0 else "failed",
                           "command": command} for result, (_, command, _) in zip(results, steps)]}
    (logs / "summary.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    print(summary)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
