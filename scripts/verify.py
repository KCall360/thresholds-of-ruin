"""Run the testing policy's checks in tiers, log each step, and print a compact summary.

Tiers (see docs/testing.md#running-the-checks):

  quick  at stable checkpoints: formatting, clippy and debug tests for affected
         packages, the Python tool tests, and the process tests the change maps to
  push   before pushing: every debug check CI runs; release coverage runs in CI
  full   exactly what CI runs on one platform, debug and release; also covers
         the push gate for unchanged inputs, toolchain and test configuration

Affected packages include every workspace package that depends on a changed one.
Anything the mapping doesn't recognize selects everything, so a tier can run more
than it needs but never less. CI's full Windows and Linux matrix remains the
required gate before merging. CI partitions the full plan by debug/release profile;
a single partition is never a full or merge gate. The debug partition has the
same checks as local push; CI evidence does not certify a local run.

Tiers only choose which existing tests run. Every feature and bug fix must add
its tests to the suite (docs/testing.md), and this script warns when code
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

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
TIERS = ("quick", "push", "full")

# Paths whose change can affect every package or every process test.
SHARED_PREFIXES = ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/", "scenarios/", "deny.toml")
# Paths that never affect Rust packages or process behavior.
INERT_PREFIXES = ("docs/", "perf/", ".github/", "README.md", "CONTRIBUTING.md", "AGENTS.md", "LICENSE", "ruff.toml", ".gitignore")
# Clients launched by process tests. Any other runtime package change selects every process test.
# Each client package, and the text by which a process test can launch it: the
# binary name or a `process_harness` helper for that client. The harness offers
# helpers for every client, so its own text is not searched. Every server stop
# saves through the headless client, so any test that starts a server uses it.
HARNESS = "process_harness"
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
    return {name for name in names if (path.parent / f"{name}.py").exists()}


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
            cache[name] = script_imports(scripts_dir / f"{name}.py")
        pending.extend(cache[name])
    return seen


def process_tests(scripts_dir):
    return sorted(p.stem for p in scripts_dir.glob("test_*process.py"))


def select_process_tests(paths, affected, everything, scripts_dir):
    """Process tests that could observe the change. Over-selects rather than under-selects."""
    all_tests = process_tests(scripts_dir)
    runtime = affected - set(CLIENT_BINARIES) - {"tor-test-support"}
    if everything or runtime:
        return all_tests
    cache, texts = {}, {}
    closures = {t: script_closure(t, scripts_dir, cache) for t in all_tests}

    def text(module):
        if module not in texts:
            texts[module] = (scripts_dir / f"{module}.py").read_text(encoding="utf-8")
        return texts[module]

    changed_modules = {Path(p).stem for p in paths if p.startswith("scripts/") and p.endswith(".py")}
    selected = set()
    for test, closure in closures.items():
        if closure & changed_modules:
            selected.add(test)
        for package in affected & set(CLIENT_BINARIES):
            if any(marker in text(m) for m in closure - {HARNESS} for marker in CLIENT_BINARIES[package]):
                selected.add(test)
    return sorted(selected)


UNTESTED_WARNING = (
    "WARNING: code changed but no tests were added or changed. Features need tests at every layer they touch "
    "and a process test; bug fixes need a regression test that fails first (docs/testing.md)."
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


def plan(tier, affected, process, xvfb=False, ci_profile=None):
    """Ordered steps; CI may partition only the complete full plan by profile."""
    if ci_profile is not None and (tier != "full" or ci_profile not in ("debug", "release")):
        raise ValueError("CI profile partitions require full and a debug/release profile")
    unittest = [sys.executable, "-m", "unittest"]
    if xvfb:
        unittest = ["xvfb-run", "-a", "-s", "-screen 0 1280x1024x24", *unittest]
    discover_all = [*unittest, "discover", "-s", "scripts", "-p", "test_*.py", "-v"]
    # The Python tool tests are cheap and every tier runs them; process tests are selected.
    tool_modules = sorted(p.stem for p in SCRIPTS.glob("test_*.py") if not p.stem.endswith("process"))
    release = {"TOR_TEST_PROFILE": "release"}
    doc = {"RUSTDOCFLAGS": "-D warnings"}
    selected = packages_args(affected)
    steps = [("fmt", ["cargo", "fmt", "--all", "--check"], {})]
    if tier == "quick":
        if affected:
            steps.append(("clippy", ["cargo", "clippy", *selected, "--all-targets", "--locked", "--", "-D", "warnings"], {}))
        steps.append(("architecture", [sys.executable, "scripts/check_architecture.py"], {}))
        steps.append(("python-tools", [*unittest, *tool_modules], {"_cwd": "scripts"}))
        if affected:
            steps.append(("rust-debug", ["cargo", "test", *selected, "--locked"], {}))
        if process:
            steps.append(("process-debug", [*unittest, "-v", *process], {"_cwd": "scripts"}))
        return steps
    steps += [
        ("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"], {}),
        ("python-debug", discover_all, {}),
        ("architecture", [sys.executable, "scripts/check_architecture.py"], {}),
        ("rustdoc", ["cargo", "doc", "--workspace", "--no-deps", "--document-private-items", "--locked"], doc),
        ("rust-debug", ["cargo", "test", "--workspace", "--locked"], {}),
    ]
    if tier == "full":
        release_steps = [
            ("rust-release", ["cargo", "test", "--workspace", "--release", "--locked"], {}),
            ("process-release", [*unittest, "discover", "-s", "scripts", "-p", "test_*process.py", "-v"], release),
        ]
        if ci_profile == "debug":
            return steps
        if ci_profile == "release":
            return release_steps
        return steps + release_steps
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


def waivable_mouse_failure(code, command, text, enabled, ci):
    """An explicit local exception for one test, never a suite/build exception."""
    return (enabled and not ci and code == 1 and "unittest" in command
            and python_failures(text) == [LOCAL_MOUSE_TEST]
            and any(line in ("FAILED (failures=1)", "FAILED (errors=1)")
                    for line in text.splitlines()))


def run_step(name, command, env, logs, jobs, rerun):
    cwd = ROOT / env.pop("_cwd", ".")
    environment = {**os.environ, "CARGO_BUILD_JOBS": str(jobs), "CARGO_PROFILE_DEV_DEBUG": "0",
                   "PYTHONUTF8": "1", "PYTHONIOENCODING": "utf-8", **env}
    log = logs / f"{name}.log"
    started = time.monotonic()
    with open(log, "w", encoding="utf-8") as handle:
        handle.write("$ " + " ".join(command) + "\n")
        handle.flush()
        code = subprocess.run(command, cwd=cwd, env=environment, stdout=handle, stderr=subprocess.STDOUT).returncode
    result = {"step": name, "code": code, "seconds": time.monotonic() - started, "log": log, "note": ""}
    if code == 0:
        return result
    text = log.read_text(encoding="utf-8", errors="replace")
    if waivable_mouse_failure(code, command, text,
            env.get("TOR_LOCAL_MOUSE_WAIVER") == "1", bool(environment.get("CI"))):
        result["code"] = 0
        result["note"] = "WAIVED local failure: " + LOCAL_MOUSE_TEST
        return result
    notes = []
    hint = environment_hint(text)
    if hint:
        notes.append(f"environment? {hint}")
    failed = python_failures(text) if "unittest" in command else []
    if failed:
        notes.append("failed: " + ", ".join(failed))
        if rerun:
            unittest = command[: command.index("unittest") + 1]
            rerun_log = logs / f"{name}-rerun.log"
            with open(rerun_log, "w", encoding="utf-8") as handle:
                again = subprocess.run([*unittest, "-v", *failed], cwd=SCRIPTS, env=environment,
                                       stdout=handle, stderr=subprocess.STDOUT).returncode
            # The step stays failed either way: a pass on rerun marks a flake, not a pass.
            notes.append("rerun passed (intermittent)" if again == 0 else "rerun failed too")
    result["note"] = "; ".join(notes)
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("tier", nargs="?", default="push", choices=TIERS)
    parser.add_argument("--ci-profile", choices=("debug", "release"),
                        help="CI-only partition of full; both profiles remain required")
    parser.add_argument("--base", default="origin/main", help="Compare against this ref to find changes")
    parser.add_argument("--jobs", type=int, help="CARGO_BUILD_JOBS; chosen from free memory by default")
    parser.add_argument("--rerun-failed", action="store_true", help="Rerun failed Python tests once to show whether they are intermittent")
    parser.add_argument("--keep-going", action="store_true", help="Run later steps after a failure")
    parser.add_argument("--allow-concurrent", action="store_true", help="Start even though other cargo/rustc processes are running")
    parser.add_argument("--dry-run", action="store_true", help="Print the selection and steps without running them")
    parser.add_argument("--waive-local-mouse", action="store_true",
                        help="Record only the native travel mouse test failure as locally waived; forbidden in CI")
    args = parser.parse_args(argv)
    if args.waive_local_mouse and (args.ci_profile or os.environ.get("CI")):
        parser.error("--waive-local-mouse is local only")
    if args.ci_profile and args.tier != "full":
        parser.error("--ci-profile requires the full tier")
    label = f"full-ci-{args.ci_profile}" if args.ci_profile else args.tier

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
    steps = plan(args.tier, affected, process,
                 xvfb=sys.platform.startswith("linux") and not os.environ.get("DISPLAY"),
                 ci_profile=args.ci_profile)

    if args.waive_local_mouse:
        steps = [(name, command, {**env, "TOR_LOCAL_MOUSE_WAIVER": "1"})
                 for name, command, env in steps]
    selection = "complete workspace" if args.ci_profile else f"{len(paths)} changed paths since {args.base}"
    print(f"tier {label}; {selection}; jobs {jobs} ({free:.1f} GB free)")
    if unknown:
        print("unmapped paths select everything: " + ", ".join(unknown[:5]) + (" ..." if len(unknown) > 5 else ""))
    print("packages: " + (", ".join(sorted(affected)) or "none"))
    print(f"process tests: {len(process)} of {len(process_tests(SCRIPTS))}")
    if untested:
        print(UNTESTED_WARNING)
    if args.dry_run:
        for name, command, env in steps:
            print(f"  {name}: {' '.join(command)}")
        return 0
    others = running_builds()
    if others and not args.allow_concurrent:
        print("Other builds are running (" + ", ".join(others) + "); wait for them or pass --allow-concurrent.", file=sys.stderr)
        return 2

    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    logs = ROOT / ".local" / "verify" / f"{stamp}-{label}"
    logs.mkdir(parents=True)
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
        lines.append(f"CI {args.ci_profile} partition only; both profiles on both platforms are required. This partition is not a full run or a merge gate.")
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
    print(summary)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
