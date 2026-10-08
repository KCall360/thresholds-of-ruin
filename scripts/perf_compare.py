"""Compare release benchmarks between a base Git ref and the working tree.

Builds the base ref's benchmark examples in a Git worktree and the working tree's
examples in place, then runs them interleaved (base, head, base, head, ...) on
this machine. Every run is validated by its own tree's report validator. The
tool prints pooled n/p50/p95/max per metric and the operation and byte counts
side by side, and writes comparison.json plus a raw-data bundle to a new run
directory. Wall-clock timings are diagnostic; counts are deterministic.

    python scripts/perf_compare.py main --case r8-a1-h100-memory --case r64-a8-h100-memory
    python scripts/perf_compare.py HEAD~1 --case combat:a8-h1000 --rounds 4
    python scripts/perf_compare.py main --case r8-a8-h100-durable -- --save-target-ms 10

Cases are latency_bench case names (rN-aN-hN-memory|durable, or stream-rN-memory|durable
for region streaming) or WORKLOAD[:GROUP]
for combat, physics, items, client, and places. Those workloads always run
their complete matrix because their validators require it; GROUP only selects
what is shown. Arguments after `--` are passed to every benchmark invocation.
"""
import argparse
from collections import defaultdict
from dataclasses import dataclass
import datetime
import gzip
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
import perf_ledger  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / ".local" / "perf-compare"
EXE = ".exe" if sys.platform == "win32" else ""
LATENCY_CASE = re.compile(r"(r\d+-a\d+-h\d+|stream-r\d+)-(memory|durable)")
COMPETING = {"cargo", "rustc", "cargo.exe", "rustc.exe", "link.exe", "clippy-driver", "clippy-driver.exe"}


def _combat_group(row):
    return f"a{row['actors']}-h{row['history']}"


def _physics_group(row):
    return f"a{row['actors']}-i{row['items']}-c{row['cells']}-{'falling' if row['falling'] else 'static'}"


def _items_group(row):
    return f"i{row['items']}-id{row['identities']}"


def _client_group(row):
    return f"c{row['cells']}-b{row['burst']}"


def _places_group(row):
    return f"rooms{row['extra_rooms']}-{row['label'] if row['kind'] == 'sample' else row['kind']}"


@dataclass(frozen=True)
class Workload:
    name: str             # ledger workload name
    package: str
    example: str
    validator: str        # path relative to the tree root
    validator_flags: tuple = ()   # benchmark flags that the validator also needs
    group: object = None  # row -> group name, for the generic row extractor
    keys: tuple = ()      # row fields that identify a group rather than count work
    version_field: str = "version"


WORKLOADS = {
    "latency": Workload("performance", "tor-server", "latency_bench", "scripts/performance_report.py",
                        ("--quick", "--phase-b", "--phase-c", "--phase-d", "--discovery-only", "--saved-discovery")),
    "combat": Workload("combat", "tor-server", "combat_bench", "scripts/combat_performance_report.py",
                       group=_combat_group, keys=("actors", "history", "sample", "version")),
    "physics": Workload("physics", "tor-server", "physics_bench", "scripts/physics_performance_report.py",
                        group=_physics_group, keys=("actors", "items", "cells", "sample", "version")),
    "items": Workload("items", "tor-server", "item_bench", "scripts/item_performance_report.py",
                      group=_items_group, keys=("items", "identities", "sample", "workload_version", "transfers"),
                      version_field="workload_version"),
    "client": Workload("client", "tor-client-ascii", "client_bench", "scripts/client_performance_report.py",
                       ("--narration",), group=_client_group, keys=("cells", "burst", "sample", "version")),
    "places": Workload("places", "tor-server", "place_bench", "scripts/place_performance_report.py",
                       group=_places_group, keys=("extra_rooms", "sample", "version", "room")),
}


@dataclass(frozen=True)
class Unit:
    """One benchmark invocation, repeated for each side and round."""
    id: str
    workload: str
    args: tuple
    groups: tuple  # groups to display; empty means all

    @property
    def spec(self):
        return WORKLOADS[self.workload]

    def validator_args(self, extra):
        flags = [flag for flag in extra if flag in self.spec.validator_flags]
        if self.workload == "latency":
            return list(self.args[:2]) + flags
        return flags


def parse_cases(cases, cycles):
    """Turn --case values into benchmark units, merging groups of one workload."""
    units, groups = {}, defaultdict(list)
    for case in cases:
        if LATENCY_CASE.fullmatch(case):
            unit_id = f"latency:{case}"
            units[unit_id] = Unit(unit_id, "latency", ("--case", case, "--cycles", str(cycles)), (case,))
            continue
        workload, _, group = case.partition(":")
        if workload not in WORKLOADS or workload == "latency":
            raise ValueError(f"Unknown case {case!r}: use a latency_bench case name or one of "
                             f"{sorted(set(WORKLOADS) - {'latency'})}[:GROUP]")
        groups[workload].append(group)
    for workload, selected in groups.items():
        shown = () if "" in selected else tuple(dict.fromkeys(selected))
        units[workload] = Unit(workload, workload, (), shown)
    return list(units.values())


def schedule(units, rounds):
    """Interleaved ABAB order: within each round, every unit runs base then head."""
    return [(round_index, unit, side) for round_index in range(1, rounds + 1)
            for unit in units for side in ("base", "head")]


def _add_count(counts, name, value):
    if type(value) is int:
        counts[name] = counts.get(name, 0) + value


def extract_latency(rows):
    """Timings and deterministic counts per latency_bench case or traversal."""
    timings = defaultdict(lambda: defaultdict(list))
    counts = defaultdict(dict)
    version = None
    for row in rows:
        kind, case = row.get("kind"), row.get("case")
        if kind in ("case", "traversal", "stream"):
            version = row.get("trace_version", row.get("workload", version))
        elif kind == "sample":
            if row["expected"] != "blocked":
                timings[case]["authoritative_total"].append(row["phases_ms"]["authoritative_total"])
            for metric in ("command_call", "disclosure_projection", "wire_encoding", "wire_decoding"):
                if metric in row["phases_ms"]:
                    timings[case][metric].append(row["phases_ms"][metric])
            for name, value in (row.get("profile") or {}).items():
                _add_count(counts[case], f"profile.{name}", value)
            counts[case]["client_memory"] = row["client_memory"]
        elif kind == "wire":
            complete = row.get("wire_profile_version", 1) == 2
            names = ("full_envelope_bytes", "sent_envelope_bytes") if complete else (
                "legacy_full_state_bytes", "legacy_update_bytes")
            for name, field in zip(names, ("full_bytes_total", "sent_bytes_total")):
                _add_count(counts[case], f"wire.{name}", row.get(field))
            _add_count(counts[case], "wire.observations", row.get("n"))
            _add_count(counts[case], "wire.deltas", row.get("deltas"))
        elif kind in ("case_end", "traversal_end", "stream_end"):
            end = row.get("persistence", row)
            for metric in ("restart_replay_ms", "final_flush_ms"):
                if metric in end:
                    timings[case][metric].append(end[metric])
            for name in ("history_end", "rewind_count"):
                _add_count(counts[case], name, row.get(name))
            for name in ("final_save_bytes", "checkpoint_json_bytes"):
                _add_count(counts[case], name, end.get(name))
            for name in ("records_loaded", "records_replayed", "checkpoint_sequence"):
                _add_count(counts[case], f"recovery.{name}", (end.get("recovery") or {}).get(name))
            for name in ("journal_bytes", "checkpoints", "checkpoint_bytes"):
                _add_count(counts[case], f"save_status.{name}", (end.get("save_status") or {}).get(name))
    return _plain(timings), dict(counts), version


def extract_rows(rows, spec):
    """Generic extractor: *_ms fields are timings, other integers are summed counts."""
    timings = defaultdict(lambda: defaultdict(list))
    counts = defaultdict(dict)
    version = None
    for row in rows:
        group = spec.group(row)
        version = row.get(spec.version_field, version)
        for name, value in row.items():
            if name.endswith("_ms"):
                if isinstance(value, list):
                    timings[group][name].extend(value)
                elif type(value) in (int, float):
                    timings[group][name].append(value)
            elif name not in spec.keys:
                _add_count(counts[group], name, value)
    return _plain(timings), dict(counts), version


def _plain(timings):
    return {group: dict(metrics) for group, metrics in timings.items()}


def extract(unit, rows):
    if unit.workload == "latency":
        return extract_latency(rows)
    return extract_rows(rows, unit.spec)


def pool(measurements):
    """Combine per-round extractions for one side into pooled statistics.

    `measurements` is a list of (timings, counts) per round. Timings are pooled
    before percentiles are taken; per-round p95 values show run-to-run spread.
    A count that differs between rounds is reported as {min, max}.
    """
    result = {}
    groups = sorted({g for timings, counts in measurements for g in (*timings, *counts)})
    for group in groups:
        metrics = sorted({m for timings, _ in measurements for m in timings.get(group, {})})
        pooled = {}
        for metric in metrics:
            per_round = [timings.get(group, {}).get(metric, []) for timings, _ in measurements]
            values = [v for r in per_round for v in r]
            if not values:
                continue
            stats = perf_ledger.distribution(values)
            stats["round_p95_ms"] = [perf_ledger.distribution(r)["p95_ms"] for r in per_round if r]
            pooled[metric] = stats
        names = sorted({n for _, counts in measurements for n in counts.get(group, {})})
        combined = {}
        for name in names:
            observed = [counts.get(group, {}).get(name) for _, counts in measurements]
            distinct = sorted({v for v in observed if v is not None})
            if len(distinct) == 1 and None not in observed:
                combined[name] = distinct[0]
            else:
                combined[name] = {"min": distinct[0] if distinct else None,
                                  "max": distinct[-1] if distinct else None,
                                  "missing_rounds": observed.count(None)}
        result[group] = {"timings": pooled, "counts": combined}
    return result


def _ms(value):
    return f"{value:.3f}" if value < 100 else f"{value:.1f}"


def _change(base, head):
    if abs(head - base) < 0.001:
        return "="  # below the benchmarks' reporting resolution
    if base == 0:
        return "n/a" if head else "0%"
    return f"{(head - base) / base * 100:+.1f}%"


def _count_text(value):
    if isinstance(value, dict):
        return f"varies {value['min']}..{value['max']}"
    return "-" if value is None else str(value)


def format_group(title, base, head):
    """Side-by-side text tables for one group's timings and counts."""
    lines = [title]
    header = f"  {'metric':<22} {'side':<5} {'n':>6} {'p50':>9} {'p95':>9} {'max':>9}  {'round p95 range':<19} {'dp50':>7} {'dp95':>7}"
    lines.append(header)
    for metric in sorted(set(base["timings"]) | set(head["timings"])):
        b, h = base["timings"].get(metric), head["timings"].get(metric)
        for side, stats in (("base", b), ("head", h)):
            if stats is None:
                lines.append(f"  {metric:<22} {side:<5} {'-':>6}")
                continue
            spread = f"{_ms(min(stats['round_p95_ms']))}..{_ms(max(stats['round_p95_ms']))}"
            delta = ""
            if side == "head" and b is not None:
                delta = f" {_change(b['p50_ms'], h['p50_ms']):>7} {_change(b['p95_ms'], h['p95_ms']):>7}"
            lines.append(f"  {metric if side == 'base' else '':<22} {side:<5} {stats['n']:>6} {_ms(stats['p50_ms']):>9} "
                         f"{_ms(stats['p95_ms']):>9} {_ms(stats['max_ms']):>9}  {spread:<19}{delta}")
    names = sorted(set(base["counts"]) | set(head["counts"]))
    if names:
        lines.append(f"  {'count':<34} {'base':>16} {'head':>16}  change")
        for name in names:
            b, h = base["counts"].get(name), head["counts"].get(name)
            if type(b) is int and type(h) is int:
                change = "=" if b == h else f"{h - b:+d}"
            else:
                change = "?"
            lines.append(f"  {name:<34} {_count_text(b):>16} {_count_text(h):>16}  {change}")
    return "\n".join(lines)


def parse_process_names(text):
    """Process names from `tasklist /FO CSV /NH` or `ps -eo comm=` output."""
    names = []
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        names.append(line.split('","')[0].strip('"') if line.startswith('"') else Path(line).name)
    return names


def competing_processes():
    command = ["tasklist", "/FO", "CSV", "/NH"] if sys.platform == "win32" else ["ps", "-eo", "comm="]
    try:
        output = subprocess.run(command, capture_output=True, text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    return sorted({n for n in parse_process_names(output) if n.lower() in COMPETING})


def git(*args, cwd=ROOT):
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True, check=True).stdout.strip()


def prepare_worktree(commit):
    path = WORK / "worktrees" / commit[:12]
    if path.exists():
        if git("rev-parse", "HEAD", cwd=path) != commit:
            raise SystemExit(f"{path} exists at a different commit; remove it with `git worktree remove`")
        return path
    path.parent.mkdir(parents=True, exist_ok=True)
    git("worktree", "add", "--detach", str(path), commit)
    return path


def build(tree, units, log, target):
    packages = defaultdict(set)
    for unit in units:
        packages[unit.spec.package].add(unit.spec.example)
    for package, examples in sorted(packages.items()):
        command = ["cargo", "build", "--release", "--locked", "--target-dir", str(target), "-p", package]
        for example in sorted(examples):
            command += ["--example", example]
        print(f"Building {', '.join(sorted(examples))} in {tree}", flush=True)
        with open(log, "a", encoding="utf-8") as stream:
            stream.write("$ " + " ".join(command) + "\n")
            stream.flush()
            code = subprocess.run(command, cwd=tree, stdout=stream, stderr=subprocess.STDOUT).returncode
        if code:
            raise SystemExit(f"Build failed in {tree} (exit {code}); see {log}")


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    extra = []
    if "--" in argv:
        split = argv.index("--")
        argv, extra = argv[:split], argv[split + 1:]
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("base", help="Git ref to compare against (built in a worktree)")
    parser.add_argument("--case", action="append", required=True, dest="cases")
    parser.add_argument("--rounds", type=int, default=3, help="interleaved base/head pairs (default 3)")
    parser.add_argument("--cycles", type=int, default=5, help="latency_bench --cycles (default 5)")
    parser.add_argument("--output", type=Path, help="new run directory")
    parser.add_argument("--temp-dir", type=Path,
                        help="TMP/TEMP for benchmark saves; choose the storage volume deliberately")
    parser.add_argument("--no-build", action="store_true", help="reuse previously built binaries")
    parser.add_argument("--allow-competing", action="store_true",
                        help="run even if cargo or rustc processes are active")
    args = parser.parse_args(argv)
    if args.rounds < 1 or args.cycles < 1:
        parser.error("--rounds and --cycles must be positive")
    try:
        units = parse_cases(args.cases, args.cycles)
    except ValueError as error:
        parser.error(str(error))

    base_commit = git("rev-parse", "--verify", f"{args.base}^{{commit}}")
    head_commit = git("rev-parse", "HEAD")
    head_dirty = bool(git("status", "--porcelain"))  # untracked sources count too
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    run_dir = (args.output or WORK / f"{stamp}-{base_commit[:8]}-{head_commit[:8]}").resolve()
    if run_dir.exists():
        parser.error(f"{run_dir} already exists; choose a new --output")
    (run_dir / "raw").mkdir(parents=True)
    (run_dir / "bin").mkdir()
    base_tree = prepare_worktree(base_commit)
    trees = {"base": base_tree, "head": ROOT}
    # An ambient target belongs to the head. The baseline must never inherit
    # it and overwrite outputs that Cargo may later consider fresh for the head.
    targets = {"base": (base_tree / "target").resolve(),
               "head": (ROOT / (os.environ.get("CARGO_TARGET_DIR") or "target")).resolve()}
    if targets["base"] == targets["head"]:
        raise SystemExit("Baseline and head need distinct build target directories")
    if not args.no_build:
        for side, tree in trees.items():
            build(tree, units, run_dir / f"build-{side}.log", targets[side])

    # Copy binaries so later builds cannot change what is measured.
    binaries = {}
    for side, tree in trees.items():
        binaries[side] = {}
        for example in sorted({u.spec.example for u in units}):
            source = targets[side] / "release" / "examples" / f"{example}{EXE}"
            if not source.exists():
                raise SystemExit(f"Missing {source}; the {side} tree may predate this benchmark")
            target = run_dir / "bin" / f"{side}-{example}{EXE}"
            shutil.copy2(source, target)
            binaries[side][example] = {"path": str(target), "sha256": perf_ledger.sha256_file(target)}

    busy = competing_processes()
    if busy and not args.allow_competing:
        raise SystemExit(f"Competing build processes are running ({', '.join(busy)}); "
                         "wait for them or pass --allow-competing and record it")
    temp_parent = (args.temp_dir or run_dir / "tmp").resolve()
    temp_parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="tor-perf-", dir=temp_parent) as owned_temp:
        temp_root = Path(owned_temp)
        machine = perf_ledger.probe_machine(temp_root)
        rustc = subprocess.run(["rustc", "--version"], capture_output=True, text=True).stdout.strip()
        print(f"Machine {machine['fingerprint']}: {machine['cpu']}, {machine['ram_gib']} GiB, "
              f"{machine['os']} {machine['os_build']}, saves on {machine['storage']['type']} "
              f"({machine['storage']['model']})", flush=True)

        runs, extracted, failures = [], defaultdict(lambda: defaultdict(list)), []
        versions = defaultdict(dict)
        for round_index, unit, side in schedule(units, args.rounds):
            name = f"{unit.id.replace(':', '-')}-r{round_index}-{side}"
            output = run_dir / "raw" / f"{name}.jsonl"
            stderr = run_dir / "raw" / f"{name}.stderr.log"
            temp = temp_root / side
            temp.mkdir(parents=True, exist_ok=True)
            env = dict(os.environ, TMP=str(temp), TEMP=str(temp), TMPDIR=str(temp))
            command = [binaries[side][unit.spec.example]["path"], *unit.args, *extra]
            print(f"[round {round_index}] {side:<4} {unit.id}", flush=True)
            with open(output, "w", encoding="utf-8") as out, open(stderr, "w", encoding="utf-8") as err:
                code = subprocess.run(command, cwd=trees[side], env=env, stdout=out, stderr=err).returncode
            record = {"unit": unit.id, "round": round_index, "side": side, "exit_code": code,
                      "raw": f"raw/{name}.jsonl.gz", "stderr": f"raw/{name}.stderr.log"}
            validator = [sys.executable, str(trees[side] / unit.spec.validator), str(output),
                         *unit.validator_args(extra)]
            if code == 0:
                check = subprocess.run(validator, capture_output=True, text=True, encoding="utf-8")
                (run_dir / "raw" / f"{name}.validation.log").write_text(check.stdout + check.stderr, encoding="utf-8")
                record["validated"] = check.returncode == 0
                record["validator"] = " ".join(validator[1:])
            else:
                record["validated"] = False
            if record["validated"]:
                rows = [json.loads(line) for line in output.read_text(encoding="utf-8-sig").splitlines() if line.strip()]
                timings, counts, version = extract(unit, rows)
                extracted[unit.id][side].append((timings, counts))
                versions[unit.id][side] = {"name": unit.spec.name, "version": version}
            else:
                failures.append(f"{name}: exit {code}, validation {'failed' if code == 0 else 'not run'}")
                print(f"  FAILED: {failures[-1]} (kept in {output.name})", flush=True)
            with open(output, "rb") as source, gzip.open(str(output) + ".gz", "wb") as target:
                shutil.copyfileobj(source, target)
            output.unlink()
            record["sha256"] = perf_ledger.sha256_file(str(output) + ".gz")
            runs.append(record)

        results = {}
        for unit in units:
            sides = {side: pool(extracted[unit.id][side]) for side in ("base", "head")}
            groups = unit.groups or tuple(sorted(set(sides["base"]) | set(sides["head"])))
            results[unit.id] = {}
            for group in groups:
                empty = {"timings": {}, "counts": {}}
                results[unit.id][group] = {"workload": versions[unit.id],
                                           "base": sides["base"].get(group, empty),
                                           "head": sides["head"].get(group, empty)}

        bundle = f"perf-{stamp}-{base_commit[:8]}-{head_commit[:8]}.tar.gz"
        comparison = {
            "tool_version": 1,
            "created": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
            "order": "ABAB", "rounds": args.rounds, "machine": machine, "rustc": rustc,
            "temp_dir": str(temp_root), "competing_processes": busy,
            "build_targets": {side: str(target) for side, target in targets.items()},
            "sides": {"base": {"ref": args.base, "commit": base_commit, "dirty": False},
                      "head": {"ref": "working tree", "commit": head_commit, "dirty": head_dirty}},
            "binaries": {side: {k: v["sha256"] for k, v in examples.items()} for side, examples in binaries.items()},
            "units": {u.id: {"workload": u.workload, "command": [u.spec.example, *u.args, *extra],
                             "validator_args": u.validator_args(extra)} for u in units},
            "runs": runs, "failures": failures, "results": results, "bundle": bundle,
        }
        (run_dir / "comparison.json").write_text(json.dumps(comparison, indent=2) + "\n", encoding="utf-8")
        with tarfile.open(run_dir / bundle, "w:gz") as archive:
            archive.add(run_dir / "comparison.json", arcname="comparison.json")
            archive.add(run_dir / "raw", arcname="raw")

    print()
    print(f"base {args.base} {base_commit[:12]}  vs  head {head_commit[:12]}{' (dirty)' if head_dirty else ''}; "
          f"{args.rounds} ABAB rounds; timings in ms, pooled over rounds")
    for unit in units:
        for group, result in results[unit.id].items():
            workload = result["workload"].get("head") or result["workload"].get("base") or {}
            title = f"\n{unit.workload} {group} ({workload.get('name')} v{workload.get('version')})"
            if result["workload"].get("base") != result["workload"].get("head"):
                title += "  WARNING: base and head workload versions differ; timings are not comparable"
            print(format_group(title, result["base"], result["head"]))
    print(f"\nRun directory: {run_dir}\nBundle: {bundle}")
    if failures:
        print("\nFailed runs (retained, excluded from the tables):\n  " + "\n  ".join(failures))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
