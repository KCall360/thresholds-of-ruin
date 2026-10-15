"""Focused local Rust feedback with incremental compilation; never a verification gate."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

import verify

ROOT = Path(__file__).resolve().parents[1]
SUMMARY = re.compile(
    r"^test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; "
    r"(\d+) measured; (\d+) filtered out; finished in ([\d.]+)s$", re.MULTILINE)


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("-p", "--package", required=True)
    target = result.add_mutually_exclusive_group(required=True)
    target.add_argument("--lib", action="store_true")
    target.add_argument("--test")
    target.add_argument("--bin")
    result.add_argument("--features", action="append", default=[], help="Explicit Cargo features for this selection; repeat as needed")
    result.add_argument("--no-default-features", action="store_true")
    result.add_argument("--filter", default="", help="Run matching tests within the selected target")
    result.add_argument("--exact", action="store_true", help="Match the complete test name")
    result.add_argument("--check", action="store_true", help="Compile the target without linking or running tests")
    result.add_argument("--no-incremental", action="store_true", help="Explicitly disable incremental compilation for a comparison")
    result.add_argument("--jobs", type=int, default=2, help="Compiler jobs (default: 2)")
    return result


def command(args):
    selected = ["--lib"] if args.lib else ["--test", args.test] if args.test else ["--bin", args.bin]
    result = ["cargo", "check" if args.check else "test", "-p", args.package, *selected,
              "--locked", "--message-format=json-render-diagnostics"]
    for features in args.features:
        result += ["--features", features]
    if args.no_default_features:
        result.append("--no-default-features")
    if not args.check and (args.filter or args.exact):
        result += ["--"]
        if args.filter:
            result.append(args.filter)
        if args.exact:
            result.append("--exact")
    return result


def environment(root, inherited, jobs, incremental=True):
    # Keep local feedback artifacts separate from the normal qualification target.
    # Override setup/CI-derived incremental=0 only in this subprocess environment.
    return {**inherited, "CARGO_INCREMENTAL": "1" if incremental else "0", "CARGO_BUILD_JOBS": str(jobs),
            "CARGO_PROFILE_DEV_DEBUG": "0", "CARGO_TARGET_DIR": str(root / ".local" / "target-feedback"),
            "PYTHONUTF8": "1", "PYTHONIOENCODING": "utf-8"}


def snapshot(root):
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=root).decode().split("\0")
    rows = []
    for name in sorted(set(names) - {""}):
        path = root / name
        rows.append({"path": name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()
                     if path.is_file() else None})
    return rows


def outcome(log, returncode, check=False):
    rows = [{"status": row[0], "passed": int(row[1]), "failed": int(row[2]), "ignored": int(row[3]),
             "measured": int(row[4]), "filtered_out": int(row[5]), "seconds": float(row[6])}
            for row in SUMMARY.findall(log)]
    if returncode:
        return False, rows, "Cargo failed"
    if check:
        return True, rows, "compiled; behavioral tests not run"
    if len(rows) != 1 or rows[0]["passed"] < 1 or rows[0]["failed"] or rows[0]["status"] != "ok":
        return False, rows, "expected one successful, nonempty test target"
    return True, rows, f"{rows[0]['passed']} passed; {rows[0]['ignored']} ignored; {rows[0]['filtered_out']} filtered out"


def artifact_rows(log):
    rows = []
    for line in log.splitlines():
        try:
            item = json.loads(line)
        except ValueError:
            continue
        if (not isinstance(item, dict) or item.get("reason") != "compiler-artifact"
                or not item.get("executable") or not item.get("profile", {}).get("test")):
            continue
        path = Path(item["executable"])
        rows.append({"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                     "features": item["features"], "profile": item["profile"], "target": item["target"]})
    return rows


def main(argv=None):
    cli = parser()
    args = cli.parse_args(argv)
    if args.jobs < 1:
        cli.error("--jobs must be positive")
    if args.filter.startswith("-"):
        cli.error("--filter must be a test name, not a test-runner flag")
    if args.exact and not args.filter:
        cli.error("--exact requires --filter")
    if args.check and (args.filter or args.exact):
        cli.error("--check compiles a target, not a test-name selection")
    if os.environ.get("CI", "").lower() not in ("", "0", "false"):
        cli.error("local feedback is not a CI or merge gate; use verify.py")
    env = environment(ROOT, os.environ, args.jobs, incremental=not args.no_incremental)
    selected = command(args)
    print("Focused local feedback only; does not satisfy quick, push, full or CI.", flush=True)
    with verify.Lock(ROOT / ".local" / "verify.lock"):
        folder = ROOT / ".local" / "feedback" / str(time.time_ns())
        folder.mkdir(parents=True)
        before = snapshot(ROOT)
        (folder / "inputs-before.json").write_text(json.dumps(before, indent=2) + "\n", encoding="utf-8")
        (folder / "source.patch").write_bytes(subprocess.check_output(["git", "diff", "--binary", "HEAD"], cwd=ROOT))
        receipt = {"kind": "local-feedback", "gate": False, "command": selected,
                   "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT).decode().strip(),
                   "rustc": subprocess.check_output(["rustc", "-Vv"], cwd=ROOT, env=env).decode(),
                   "cargo": subprocess.check_output(["cargo", "-V"], cwd=ROOT, env=env).decode(),
                   "environment": {key: env[key] for key in sorted(env)
                                   if key in ("CARGO_TARGET_DIR", "CARGO_INCREMENTAL", "CARGO_BUILD_JOBS",
                                              "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTUP_TOOLCHAIN")
                                   or key.startswith("CARGO_PROFILE_")}}
        log_path = folder / "output.log"
        started = time.monotonic()
        with log_path.open("w", encoding="utf-8") as log:
            result = subprocess.run(selected, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        receipt["seconds"] = time.monotonic() - started
        after = snapshot(ROOT)
        (folder / "inputs-after.json").write_text(json.dumps(after, indent=2) + "\n", encoding="utf-8")
        text = log_path.read_text(encoding="utf-8", errors="replace")
        success, summaries, note = outcome(text, result.returncode, args.check)
        receipt.update(exit_code=result.returncode, inputs_unchanged=before == after,
                       test_summaries=summaries, artifacts=artifact_rows(text))
        if success and not args.check and len(receipt["artifacts"]) != 1:
            success, note = False, "expected one identifiable compiled test executable"
        if before != after:
            success, note = False, "source inputs changed during the run"
        receipt.update(success=success, note=note)
        (folder / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        print(f"{'ok' if success else 'FAILED'}: {note} ({receipt['seconds']:.2f}s); log: {log_path}")
        if not success:
            print(text[-4000:])
        return 0 if success else 1


if __name__ == "__main__":
    sys.exit(main())
