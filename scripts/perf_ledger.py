"""Maintain the committed performance ledger (perf/ledger.jsonl).

The ledger holds one JSON line per accepted headline measurement: the source
commit, workload name and version, case, a machine fingerprint, the sample count,
p50/p95/max, key operation and byte counts, and the URL and SHA-256 of the raw
data. Timings are only comparable between lines with the same fingerprint,
build profile, and workload version. The checks here cover format only; they
never apply timing thresholds.

    python scripts/perf_ledger.py check
    python scripts/perf_ledger.py fingerprint --path SAVE_DIRECTORY
    python scripts/perf_ledger.py add --comparison RUN/comparison.json \\
        --unit latency:r8-a1-h100-memory --group r8-a1-h100-memory \\
        --metric authoritative_total --raw-url URL
"""
import argparse
import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
LEDGER = ROOT / "perf" / "ledger.jsonl"
SCHEMA = 1
REPOSITORY_URL = "https://github.com/KCall360/thresholds-of-ruin/"
RAW_URL = re.compile(re.escape(REPOSITORY_URL) + r"(releases/download|blob|raw)/[^\s?#]+$")
STORAGE_TYPES = {"HDD", "SSD", "NVMe", "RAM", "network", "unknown"}
BUILD_PROFILES = {"release", "debug"}
ENTRY_KEYS = {"schema", "date", "commit", "dirty", "workload", "case", "metric", "machine",
              "build", "n", "p50_ms", "p95_ms", "max_ms", "counts", "raw", "command", "note"}
OPTIONAL_KEYS = {"command", "note"}
MACHINE_KEYS = {"fingerprint", "cpu", "logical_cpus", "ram_gib", "os", "os_build", "storage"}
STORAGE_KEYS = {"type", "filesystem", "model", "bus"}


def nearest_rank(ordered, percent):
    """Nearest-rank percentile, matching the Rust benchmark summaries."""
    if not ordered:
        raise ValueError("No samples")
    return ordered[max(0, (len(ordered) * percent + 99) // 100 - 1)]


def distribution(values):
    ordered = sorted(values)
    return {"n": len(ordered), "p50_ms": nearest_rank(ordered, 50),
            "p95_ms": nearest_rank(ordered, 95), "max_ms": ordered[-1]}


def machine_fingerprint(machine):
    """Short stable hash of the identifying machine fields (everything but itself)."""
    identity = {key: value for key, value in machine.items() if key != "fingerprint"}
    encoded = json.dumps(identity, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()[:12]


def with_fingerprint(machine):
    machine = dict(machine)
    machine["fingerprint"] = machine_fingerprint(machine)
    return machine


def storage_type(media_type=None, rotational=None, bus=None):
    """Classify a device from Windows MediaType/BusType or Linux rotational/transport."""
    bus = (bus or "").lower()
    if bus == "nvme":
        return "NVMe"
    media = (media_type or "").upper()
    if media in ("HDD", "SSD"):
        return media
    if rotational is not None:
        return "HDD" if rotational else "SSD"
    return "unknown"


def parse_windows_machine(info):
    """Build a machine record from the CIM JSON emitted by WINDOWS_PROBE."""
    processor = info["processor"]
    if isinstance(processor, list):
        processor = processor[0]
    system, os_info = info["system"], info["os"]
    disk = info.get("disk") or {}
    ram = system.get("TotalPhysicalMemory")
    return with_fingerprint({
        "cpu": processor["Name"].strip(),
        "logical_cpus": int(processor["NumberOfLogicalProcessors"]),
        "ram_gib": round(int(ram) / 2**30, 1) if ram else None,
        "os": os_info["Caption"].strip(),
        "os_build": os_info["Version"],
        "storage": {
            "type": storage_type(disk.get("MediaType"), bus=disk.get("BusType")),
            "filesystem": info.get("filesystem") or "unknown",
            "model": (disk.get("FriendlyName") or "unknown").strip(),
            "bus": disk.get("BusType") or "unknown",
        },
    })


def parse_cpuinfo(text):
    for line in text.splitlines():
        key, _, value = line.partition(":")
        if key.strip() == "model name":
            return value.strip()
    return "unknown"


def parse_meminfo(text):
    for line in text.splitlines():
        if line.startswith("MemTotal:"):
            return round(int(line.split()[1]) * 1024 / 2**30, 1)
    return None


def parse_os_release(text):
    for line in text.splitlines():
        if line.startswith("PRETTY_NAME="):
            return line.split("=", 1)[1].strip().strip('"')
    return "Linux"


# MediaType and BusType come from Get-PhysicalDisk; Get-Disk alone lacks MediaType.
WINDOWS_PROBE = r"""
$ErrorActionPreference = 'SilentlyContinue'
$letter = '{letter}'
$partition = Get-Partition -DriveLetter $letter
$disk = if ($partition) {{ Get-PhysicalDisk | Where-Object DeviceId -eq ([string]$partition.DiskNumber) | Select-Object -First 1 }}
[pscustomobject]@{{
  processor = Get-CimInstance Win32_Processor | Select-Object Name, NumberOfLogicalProcessors
  system = Get-CimInstance Win32_ComputerSystem | Select-Object TotalPhysicalMemory
  os = Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version
  filesystem = (Get-Volume -DriveLetter $letter).FileSystem
  disk = if ($disk) {{ [pscustomobject]@{{ FriendlyName = $disk.FriendlyName; MediaType = [string]$disk.MediaType; BusType = [string]$disk.BusType }} }}
}} | ConvertTo-Json -Depth 4 -Compress
"""


def probe_machine(path):
    """Describe this machine and the storage volume holding `path`."""
    path = Path(path).resolve()
    if sys.platform == "win32":
        letter = path.drive.rstrip(":") or "C"
        output = subprocess.run(
            ["powershell", "-NoProfile", "-NonInteractive", "-Command", WINDOWS_PROBE.format(letter=letter)],
            capture_output=True, text=True, check=True).stdout
        return parse_windows_machine(json.loads(output))
    return probe_linux_machine(path)


def probe_linux_machine(path):
    def read(name):
        try:
            return Path(name).read_text(encoding="utf-8")
        except OSError:
            return ""

    def run(*command):
        try:
            return subprocess.run(command, capture_output=True, text=True, check=True).stdout.strip()
        except (OSError, subprocess.CalledProcessError):
            return ""

    source, _, filesystem = run("findmnt", "-no", "SOURCE,FSTYPE", "-T", str(path)).partition(" ")
    parent = run("lsblk", "-no", "PKNAME", source).splitlines()
    device = f"/dev/{parent[0]}" if parent and parent[0] else source
    rota, _, rest = run("lsblk", "-ndo", "ROTA,TRAN,MODEL", device).partition(" ")
    transport, _, model = rest.strip().partition(" ")
    return with_fingerprint({
        "cpu": parse_cpuinfo(read("/proc/cpuinfo")),
        "logical_cpus": os.cpu_count() or 0,
        "ram_gib": parse_meminfo(read("/proc/meminfo")),
        "os": parse_os_release(read("/etc/os-release")),
        "os_build": platform.release(),
        "storage": {
            "type": storage_type(rotational=(rota.strip() == "1") if rota.strip() in ("0", "1") else None,
                                 bus=transport),
            "filesystem": filesystem.strip() or "unknown",
            "model": model.strip() or "unknown",
            "bus": transport or "unknown",
        },
    })


def _finite(value):
    return type(value) in (int, float) and math.isfinite(value) and value >= 0


def validate_entry(entry):
    """Return a list of format problems in one ledger entry (empty when valid)."""
    if not isinstance(entry, dict):
        return ["entry is not an object"]
    problems = []
    missing = ENTRY_KEYS - OPTIONAL_KEYS - set(entry)
    unknown = set(entry) - ENTRY_KEYS
    if missing:
        problems.append(f"missing keys {sorted(missing)}")
    if unknown:
        problems.append(f"unknown keys {sorted(unknown)}")
    if missing:
        return problems
    if entry["schema"] != SCHEMA:
        problems.append(f"schema must be {SCHEMA}")
    try:
        datetime.date.fromisoformat(entry["date"])
        if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", entry["date"]):
            raise ValueError
    except (TypeError, ValueError):
        problems.append("date must be YYYY-MM-DD")
    if not (isinstance(entry["commit"], str) and re.fullmatch(r"[0-9a-f]{40}", entry["commit"])):
        problems.append("commit must be a full 40-character hash")
    if type(entry["dirty"]) is not bool:
        problems.append("dirty must be a boolean")
    workload = entry["workload"]
    if not (isinstance(workload, dict) and set(workload) == {"name", "version"}
            and isinstance(workload["name"], str) and re.fullmatch(r"[a-z][a-z0-9_]*", workload["name"])
            and type(workload["version"]) is int and workload["version"] >= 1):
        problems.append("workload must be {name: lowercase identifier, version: positive integer}")
    for key in ("case", "metric"):
        if not (isinstance(entry[key], str) and entry[key].strip()):
            problems.append(f"{key} must be a non-empty string")
    problems += [f"machine: {p}" for p in validate_machine(entry["machine"])]
    build = entry["build"]
    if not (isinstance(build, dict) and set(build) <= {"profile", "rustc"} and build.get("profile") in BUILD_PROFILES
            and (("rustc" not in build) or isinstance(build["rustc"], str))):
        problems.append("build must be {profile: release|debug, optional rustc string}")
    if not (type(entry["n"]) is int and entry["n"] > 0):
        problems.append("n must be a positive integer")
    stats = [entry[k] for k in ("p50_ms", "p95_ms", "max_ms")]
    if not all(_finite(v) for v in stats):
        problems.append("p50_ms, p95_ms and max_ms must be finite nonnegative numbers")
    elif not stats[0] <= stats[1] <= stats[2]:
        problems.append("percentiles must satisfy p50 <= p95 <= max")
    counts = entry["counts"]
    if not (isinstance(counts, dict) and all(isinstance(k, str) and k and type(v) is int and v >= 0
                                             for k, v in counts.items())):
        problems.append("counts must map names to nonnegative integers")
    raw = entry["raw"]
    if not (isinstance(raw, dict) and set(raw) == {"url", "sha256"}):
        problems.append("raw must be {url, sha256}")
    else:
        if not (isinstance(raw["url"], str) and RAW_URL.match(raw["url"])):
            problems.append("raw.url must be a release asset or tagged file in the project repository")
        if not (isinstance(raw["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", raw["sha256"])):
            problems.append("raw.sha256 must be 64 lowercase hex characters")
    for key in OPTIONAL_KEYS & set(entry):
        if not (isinstance(entry[key], str) and entry[key].strip()):
            problems.append(f"{key} must be a non-empty string")
    return problems


def validate_machine(machine):
    if not isinstance(machine, dict) or set(machine) != MACHINE_KEYS:
        return [f"keys must be {sorted(MACHINE_KEYS)}"]
    problems = []
    for key in ("cpu", "os", "os_build"):
        if not (isinstance(machine[key], str) and machine[key].strip()):
            problems.append(f"{key} must be a non-empty string")
    if not (type(machine["logical_cpus"]) is int and machine["logical_cpus"] > 0):
        problems.append("logical_cpus must be a positive integer")
    ram = machine["ram_gib"]
    if ram is not None and not (_finite(ram) and ram > 0):
        problems.append("ram_gib must be a positive number or null")
    storage = machine["storage"]
    if not (isinstance(storage, dict) and set(storage) == STORAGE_KEYS):
        problems.append(f"storage keys must be {sorted(STORAGE_KEYS)}")
    else:
        if storage["type"] not in STORAGE_TYPES:
            problems.append(f"storage.type must be one of {sorted(STORAGE_TYPES)}")
        if not all(isinstance(storage[k], str) and storage[k].strip() for k in ("filesystem", "model", "bus")):
            problems.append("storage.filesystem, model and bus must be non-empty strings ('unknown' if unrecorded)")
    if not problems and machine["fingerprint"] != machine_fingerprint(machine):
        problems.append("fingerprint does not match the machine fields")
    return problems


def entry_key(entry):
    workload = entry["workload"]
    return (entry["commit"], workload["name"], workload["version"], entry["case"], entry["metric"],
            entry["machine"]["fingerprint"], entry["build"]["profile"])


def validate_ledger(text):
    """Return format problems for the whole ledger text, with line numbers."""
    problems, keys, previous_date = [], set(), ""
    if text and not text.endswith("\n"):
        problems.append("ledger must end with a newline")
    for number, line in enumerate(text.splitlines(), 1):
        if not line.strip():
            problems.append(f"line {number}: blank line")
            continue
        try:
            entry = json.loads(line)
        except json.JSONDecodeError as error:
            problems.append(f"line {number}: invalid JSON ({error.msg})")
            continue
        entry_problems = validate_entry(entry)
        problems += [f"line {number}: {p}" for p in entry_problems]
        if entry_problems:
            continue
        key = entry_key(entry)
        if key in keys:
            problems.append(f"line {number}: duplicate entry for {key[1]} {key[3]} {key[4]}")
        keys.add(key)
        if entry["date"] < previous_date:
            problems.append(f"line {number}: entries must be appended in date order")
        previous_date = max(previous_date, entry["date"])
    return problems


def entry_from_comparison(comparison, unit, group, metric, side, raw_url, raw_sha256, date=None, note=None):
    """Build a ledger entry from a perf_compare.py comparison.json."""
    result = comparison["results"][unit][group]
    measured = result[side]
    stats = measured["timings"][metric]
    counts = {name: value for name, value in measured["counts"].items() if type(value) is int}
    source = comparison["sides"][side]
    workload = result["workload"][side]
    # Streaming benchmarks report a versioned identifier rather than the
    # ledger's separate name and numeric version. Keep validation strict for
    # unknown identifiers and leave already structured workloads unchanged.
    if isinstance(workload.get("version"), str):
        identifier = re.fullmatch(r"([a-z][a-z0-9_]*)-v([1-9][0-9]*)", workload["version"])
        if identifier:
            workload = {"name": identifier.group(1), "version": int(identifier.group(2))}
    entry = {
        "schema": SCHEMA,
        "date": date or comparison["created"][:10],
        "commit": source["commit"],
        "dirty": source["dirty"],
        "workload": workload,
        "case": group,
        "metric": metric,
        "machine": comparison["machine"],
        "build": {"profile": "release", "rustc": comparison["rustc"]},
        "n": stats["n"],
        "p50_ms": stats["p50_ms"],
        "p95_ms": stats["p95_ms"],
        "max_ms": stats["max_ms"],
        "counts": counts,
        "raw": {"url": raw_url, "sha256": raw_sha256},
        "command": " ".join(comparison["units"][unit]["command"]),
    }
    if note:
        entry["note"] = note
    return entry


def append_entry(path, entry):
    text = path.read_text(encoding="utf-8") if path.exists() else ""
    line = json.dumps(entry, separators=(",", ":")) + "\n"
    problems = validate_ledger(text + line)
    if problems:
        raise ValueError("\n".join(problems))
    with path.open("a", encoding="utf-8", newline="\n") as stream:
        stream.write(line)


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    check = commands.add_parser("check", help="validate the ledger format")
    check.add_argument("ledger", type=Path, nargs="?", default=LEDGER)
    probe = commands.add_parser("fingerprint", help="print this machine's fingerprint record")
    probe.add_argument("--path", type=Path, default=ROOT, help="a path on the storage volume used for saves")
    add = commands.add_parser("add", help="append an entry from a perf_compare.py comparison.json")
    add.add_argument("--ledger", type=Path, default=LEDGER)
    add.add_argument("--comparison", type=Path, required=True)
    add.add_argument("--unit", required=True, help="unit id, for example latency:r8-a1-h100-memory")
    add.add_argument("--group", required=True, help="case within the unit's results")
    add.add_argument("--metric", required=True)
    add.add_argument("--side", choices=("base", "head"), default="head")
    add.add_argument("--raw-url", required=True, help="release asset URL of the uploaded bundle")
    add.add_argument("--raw-file", type=Path, help="local copy of the uploaded bundle (default: the run bundle)")
    add.add_argument("--note")
    args = parser.parse_args(argv)
    if args.command == "check":
        problems = validate_ledger(args.ledger.read_text(encoding="utf-8"))
        for problem in problems:
            print(problem, file=sys.stderr)
        if problems:
            return 1
        print(f"{args.ledger}: {len(args.ledger.read_text(encoding='utf-8').splitlines())} valid entries")
        return 0
    if args.command == "fingerprint":
        print(json.dumps(probe_machine(args.path), indent=2))
        return 0
    comparison = json.loads(args.comparison.read_text(encoding="utf-8"))
    raw_file = args.raw_file or args.comparison.parent / comparison["bundle"]
    entry = entry_from_comparison(comparison, args.unit, args.group, args.metric, args.side,
                                  args.raw_url, sha256_file(raw_file), note=args.note)
    append_entry(args.ledger, entry)
    print(json.dumps(entry, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
