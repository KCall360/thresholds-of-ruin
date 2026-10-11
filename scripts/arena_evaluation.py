"""Run and replay paired arena encounters through the ordinary tor-arena Engine."""
import argparse
from collections import Counter
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

FORMAT = "tor-arena-batch-v1"
VARIANTS = ("baseline", "candidate")
ORIENTATIONS = ("forward", "mirrored")
MAX_BYTES = 64 * 1024 * 1024


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def files(package):
    """Bounded exact snapshot; symlinks cannot pull in external mutable inputs."""
    if package.is_symlink():
        raise ValueError("Scenario snapshots cannot contain symlinks")
    entries = {}
    total = 0
    for path in sorted(package.rglob("*")):
        if path.is_symlink():
            raise ValueError("Scenario snapshots cannot contain symlinks")
        if path.is_file():
            total += path.stat().st_size
            if total > MAX_BYTES or len(entries) >= 4096:
                raise ValueError("Scenario snapshot exceeds bounded size")
            entries[path.relative_to(package).as_posix()] = digest(path)
    if "scenario.toml" not in entries:
        raise ValueError("Scenario snapshot has no scenario.toml")
    return entries


def read_json(path):
    if path.is_symlink():
        raise ValueError("JSON artifacts cannot be symlinks")
    if path.stat().st_size > MAX_BYTES:
        raise ValueError("JSON artifact exceeds bounded size")
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path, value, *, temporary_path=None):
    # Atomic artifact publication; existing source packages are never changed.
    encoded = json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n"
    if len(encoded.encode("utf-8")) > MAX_BYTES:
        raise ValueError("JSON artifact exceeds bounded size")
    temporary = None
    try:
        if temporary_path is None:
            stream = tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", newline="\n",
                                                 dir=path.parent, delete=False)
        else:
            if temporary_path.parent != path.parent or temporary_path.is_symlink():
                raise ValueError("Invalid temporary artifact path")
            stream = temporary_path.open("w", encoding="utf-8", newline="\n")
        with stream:
            temporary = Path(stream.name)
            stream.write(encoded)
            stream.flush()
            os.fsync(stream.fileno())
        temporary.replace(path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def seed(value):
    if (not isinstance(value, str) or not 1 <= len(value) <= 20 or not value.isascii() or not value.isdecimal()
            or str(int(value)) != value or int(value) > 2**64 - 1):
        raise ValueError("Seeds must be canonical unsigned 64-bit decimal strings")
    return value


def summarize(runs, faction):
    """A seed contributes only if all four elimination observations exist.

    Cap, stalemate and failed runs are censored, never silently scored as losses.
    Mutual elimination contributes half a win. Orientations average within seed.
    """
    counts = {variant: Counter() for variant in VARIANTS}
    pairs = {}
    for run in runs:
        key = (run["orientation"], run["variant"])
        if key[0] not in ORIENTATIONS or key[1] not in VARIANTS:
            raise ValueError("Invalid paired run identity")
        observations = pairs.setdefault(seed(run["seed"]), {})
        if key in observations:
            raise ValueError("Duplicate paired run")
        if run["status"] not in ("ok", "failure"):
            raise ValueError("Unknown arena run status")
        termination = (run["report"]["termination"] if run["status"] == "ok"
                       else {"type": "failure"})
        kind = termination["type"]
        if kind not in ("elimination", "action_limit", "tick_limit", "stalemate", "failure"):
            raise ValueError("Unknown arena termination")
        counts[key[1]][kind] += 1
        observations[key] = (0.5 if termination.get("winner") is None
                             else float(termination["winner"] == faction)) if kind == "elimination" else None
    differences, excluded = [], []
    for value, observations in sorted(pairs.items(), key=lambda pair: int(pair[0])):
        if len(observations) != 4 or any(score is None for score in observations.values()):
            excluded.append(value)
            continue
        differences.append(sum(observations[(orientation, "candidate")]
                               - observations[(orientation, "baseline")]
                               for orientation in ORIENTATIONS) / 2)
    return {"paired_seeds": len(differences), "excluded_seeds": excluded,
            "mean_candidate_win_difference": (sum(differences) / len(differences)
                                               if differences else None),
            "terminations": {variant: dict(counts[variant]) for variant in VARIANTS}}


def execute(binary, package, value, timeout, scratch):
    """No shell; child stdout/stderr spool to bounded inspected temporary files."""
    with tempfile.TemporaryFile(dir=scratch) as stdout, tempfile.TemporaryFile(dir=scratch) as stderr:
        try:
            process = subprocess.run([str(binary), "--scenario", str(package), "--all-ai",
                                      "--seed", value], stdout=stdout, stderr=stderr, timeout=timeout)
        except subprocess.TimeoutExpired:
            return {"status": "failure", "message": "Evaluation timeout"}
        except OSError as error:
            return {"status": "failure", "message": str(error)[:1000]}
        if stdout.tell() > 2 * 1024 * 1024:
            return {"status": "failure", "message": "Evaluation report exceeds bounded size"}
        stdout.seek(0)
        try:
            report = json.load(stdout)
            if not isinstance(report, dict):
                raise ValueError("Arena report must be an object")
            if process.returncode != 0:
                return {"status": "failure", "exit_code": process.returncode,
                        "message": str(report.get("message", "Evaluation failed"))[:1000]}
            if (report["format"] != "tor-arena-run-v1" or report["input"]["seed"] != value
                    or not report["participants"]):
                raise ValueError("Unexpected arena report identity")
            return {"status": "ok", "report": report}
        except (ValueError, KeyError, TypeError, UnicodeError) as error:
            return {"status": "failure", "exit_code": process.returncode,
                    "message": f"Invalid arena report: {error}"[:1000]}


def timeout_seconds(value):
    if (isinstance(value, bool) or not isinstance(value, (int, float))
            or not math.isfinite(value) or not 0 < value <= 3600):
        raise ValueError("Evaluation timeout must be positive and at most 3600 seconds")
    return value


def run_batch(binary, plan_path, output):
    plan = read_json(plan_path)
    if set(plan) - {"baseline", "candidate", "seeds", "faction", "timeout_seconds"}:
        raise ValueError("Unknown evaluation plan field")
    values = plan["seeds"]
    if not isinstance(values, list) or not 1 <= len(values) <= 1000:
        raise ValueError("Supply between 1 and 1000 seeds")
    values = [seed(value) for value in values]
    if len(set(values)) != len(values):
        raise ValueError("Seeds must be unique")
    faction = plan["faction"]
    if not isinstance(faction, str) or not faction:
        raise ValueError("Supply the faction being evaluated")
    timeout = timeout_seconds(plan.get("timeout_seconds", 60))
    sources = {}
    for variant in VARIANTS:
        if set(plan[variant]) != set(ORIENTATIONS):
            raise ValueError("Supply exactly forward and mirrored packages")
        for orientation in ORIENTATIONS:
            source = (plan_path.parent / plan[variant][orientation]).resolve()
            if not source.is_dir() or source == output or source in output.parents:
                raise ValueError("Output must be outside every source package")
            sources[f"{variant}-{orientation}"] = (source, files(source))
    for variant in VARIANTS:
        if sources[f"{variant}-forward"][1] == sources[f"{variant}-mirrored"][1]:
            raise ValueError("Mirrored package must differ from forward package")
    binary_hash = digest(binary)
    output.mkdir(parents=True, exist_ok=False)
    inputs = {}
    for key, (source, fingerprint) in sources.items():
        target = output / "inputs" / key
        shutil.copytree(source, target)
        if files(target) != fingerprint:
            raise ValueError("Source changed during snapshot")
        inputs[key] = fingerprint
    batch = {"format": FORMAT, "binary_sha256": binary_hash, "inputs": inputs,
             "faction": faction, "seeds": values, "timeout_seconds": timeout, "runs": []}
    (output / "runs").mkdir()
    progress = {key: value for key, value in batch.items() if key != "runs"}
    progress.update(completed_runs=0, chain_sha256=chain_hash(progress), pending=None)
    write_json(output / "partial.json", progress)
    return resume_batch(binary, output)


def chain_hash(value, previous=""):
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)
    return hashlib.sha256((previous + encoded).encode("utf-8")).hexdigest()


def identities(batch):
    return [(value, orientation, variant) for value in batch["seeds"]
            for orientation in ORIENTATIONS for variant in VARIANTS]


def record_path(output, identity):
    return output / "runs" / ("-".join(identity) + ".json")


def validate_inputs(binary, batch, output):
    if any(path.is_symlink() for path in (output, output / "inputs", output / "runs")):
        raise ValueError("Batch directories cannot be symlinks")
    if batch["format"] != FORMAT or digest(binary) != batch["binary_sha256"]:
        raise ValueError("Executable fingerprint differs from batch")
    expected_keys = {f"{variant}-{orientation}" for variant in VARIANTS for orientation in ORIENTATIONS}
    if set(batch["inputs"]) != expected_keys:
        raise ValueError("Incomplete input snapshot inventory")
    for key, fingerprint in batch["inputs"].items():
        if files(output / "inputs" / key) != fingerprint:
            raise ValueError("Input snapshot differs from batch")
    timeout_seconds(batch["timeout_seconds"])
    if (not isinstance(batch["seeds"], list) or not 1 <= len(batch["seeds"]) <= 1000
            or len(set(batch["seeds"])) != len(batch["seeds"])):
        raise ValueError("Invalid batch seed inventory")
    for value in batch["seeds"]:
        seed(value)
    if not isinstance(batch["faction"], str) or not batch["faction"]:
        raise ValueError("Invalid batch faction")


def validate_record(run, identity):
    if (run.get("seed"), run.get("orientation"), run.get("variant")) != identity:
        raise ValueError("Run record differs from checkpoint identity")
    expected = {"seed", "orientation", "variant", "status"}
    if run["status"] == "ok":
        expected.add("report")
        report = run["report"]
        if (report["format"] != "tor-arena-run-v1" or report["input"]["seed"] != identity[0]
                or not report["participants"]):
            raise ValueError("Invalid checkpoint report identity")
    elif run["status"] == "failure":
        expected.add("message")
        if "exit_code" in run:
            expected.add("exit_code")
            if type(run["exit_code"]) is not int:
                raise ValueError("Invalid checkpoint exit code")
        if not isinstance(run["message"], str) or len(run["message"]) > 1000:
            raise ValueError("Invalid checkpoint failure message")
    else:
        raise ValueError("Invalid checkpoint run status")
    if set(run) != expected:
        raise ValueError("Invalid checkpoint run fields")
    summarize([run], "")


def finish_batch(batch, output):
    successful = [run for run in batch["runs"] if run["status"] == "ok"]
    rosters = [{p["actor"]: p["faction"] for p in run["report"]["participants"]}
               for run in successful]
    if rosters and (any(roster != rosters[0] for roster in rosters)
                    or batch["faction"] not in rosters[0].values()):
        raise ValueError("Paired encounters must preserve actor/faction identities")
    batch["summary"] = summarize(batch["runs"], batch["faction"])
    write_json(output / "batch.json", batch)
    (output / "partial.json").unlink(missing_ok=True)
    return batch


def load_complete(binary, output):
    batch = read_json(output / "batch.json")
    validate_inputs(binary, batch, output)
    expected = identities(batch)
    if len(batch["runs"]) != len(expected):
        raise ValueError("Incomplete paired runs")
    if summarize(batch["runs"], batch["faction"]) != batch["summary"]:
        raise ValueError("Batch summary differs from retained runs")
    for run, identity in zip(batch["runs"], expected):
        validate_record(run, identity)
        if read_json(record_path(output, identity)) != run:
            raise ValueError("Run record differs from batch")
    if {path.name for path in (output / "runs").iterdir()} != {record_path(output, i).name for i in expected}:
        raise ValueError("Unexpected run record inventory")
    return batch


def resume_batch(binary, output):
    """Recover committed or pending results without rerunning completed work.

    A write-ahead checkpoint contains the next result before its individual file
    is published. A chained checksum binds the committed prefix and input header.
    Checksums detect corruption; these local artifacts are not authenticated.
    """
    if (output / "batch.json").exists():
        return load_complete(binary, output)
    progress = read_json(output / "partial.json")
    header_fields = {"format", "binary_sha256", "inputs", "faction", "seeds", "timeout_seconds"}
    if set(progress) != header_fields | {"completed_runs", "chain_sha256", "pending"}:
        raise ValueError("Invalid resume checkpoint fields")
    validate_inputs(binary, progress, output)
    batch = {key: progress[key] for key in header_fields}
    expected = identities(batch)
    count = progress["completed_runs"]
    if type(count) is not int or not 0 <= count <= len(expected):
        raise ValueError("Invalid checkpoint completed count")
    chain = chain_hash(batch)
    runs, retained_bytes = [], 0
    for identity in expected[:count]:
        path = record_path(output, identity)
        run = read_json(path)
        validate_record(run, identity)
        retained_bytes += path.stat().st_size
        chain = chain_hash(run, chain)
        runs.append(run)
    if chain != progress["chain_sha256"]:
        raise ValueError("Run records differ from checkpoint checksum")
    pending = progress["pending"]
    if pending is not None:
        if (not isinstance(pending, dict) or set(pending) != {"run", "sha256"}
                or chain_hash(pending["run"], chain) != pending["sha256"]):
            raise ValueError("Pending result differs from checkpoint checksum")
        pending = pending["run"]
    permitted = {record_path(output, i).name for i in expected[:count]}
    if pending is not None:
        if count == len(expected):
            raise ValueError("Unexpected pending checkpoint result")
        validate_record(pending, expected[count])
        path = record_path(output, expected[count])
        permitted.update((path.name, path.with_suffix(".pending").name))
        if path.exists() and read_json(path) != pending:
            raise ValueError("Pending run differs from checkpoint")
    if any(path.is_symlink() or not path.is_file() or path.name not in permitted
           for path in (output / "runs").iterdir()):
        raise ValueError("Unexpected checkpoint run inventory")
    if retained_bytes > MAX_BYTES:
        raise ValueError("Batch report exceeds bounded size")
    for identity in expected[count:]:
        if pending is None:
            value, orientation, variant = identity
            result = execute(binary, output / "inputs" / f"{variant}-{orientation}",
                             value, progress["timeout_seconds"], output)
            if digest(binary) != progress["binary_sha256"]:
                raise ValueError("Executable changed during batch")
            pending = {"seed": value, "orientation": orientation, "variant": variant, **result}
            validate_record(pending, identity)
            progress["pending"] = {"run": pending, "sha256": chain_hash(pending, progress["chain_sha256"])}
            write_json(output / "partial.json", progress)
        path = record_path(output, identity)
        write_json(path, pending, temporary_path=path.with_suffix(".pending"))
        retained_bytes += path.stat().st_size
        if retained_bytes > MAX_BYTES:
            raise ValueError("Batch report exceeds bounded size")
        runs.append(pending)
        progress.update(completed_runs=len(runs), chain_sha256=chain_hash(pending, progress["chain_sha256"]),
                        pending=None)
        write_json(output / "partial.json", progress)
        pending = None
    validate_inputs(binary, progress, output)
    batch["runs"] = runs
    return finish_batch(batch, output)


def replay(binary, output):
    batch = load_complete(binary, output)
    timeout = timeout_seconds(batch["timeout_seconds"])
    matched = 0
    for run in batch["runs"]:
        key = f"{run['variant']}-{run['orientation']}"
        retained = read_json(output / "runs" / f"{run['seed']}-{run['orientation']}-{run['variant']}.json")
        if retained != run:
            raise ValueError("Run record differs from batch")
        result = execute(binary, output / "inputs" / key, seed(run["seed"]),
                         timeout, output)
        if result != {key: value for key, value in run.items()
                      if key not in ("seed", "orientation", "variant")}:
            return False, matched
        matched += 1
    if digest(binary) != batch["binary_sha256"]:
        raise ValueError("Executable changed during replay")
    return True, matched


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    commands = parser.add_subparsers(dest="operation", required=True)
    run = commands.add_parser("run")
    run.add_argument("plan", type=Path)
    run.add_argument("output", type=Path)
    resume = commands.add_parser("resume")
    resume.add_argument("output", type=Path)
    verify = commands.add_parser("replay")
    verify.add_argument("output", type=Path)
    args = parser.parse_args(argv)
    try:
        binary = args.binary.resolve(strict=True)
        output = args.output.resolve()
        if args.operation in ("run", "resume"):
            batch = (run_batch(binary, args.plan.resolve(), output) if args.operation == "run"
                     else resume_batch(binary, output))
            print(json.dumps(batch["summary"], sort_keys=True))
            return 0 if all(run["status"] == "ok" for run in batch["runs"]) else 1
        equal, count = replay(binary, output)
        print(f"{count} runs match" if equal else f"Replay differs after {count} runs")
        return 0 if equal else 1
    except (ValueError, KeyError, TypeError, OSError) as error:
        print(f"Arena evaluation: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
