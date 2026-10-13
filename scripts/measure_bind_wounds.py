"""Record provisional care outcomes from the ordinary Engine acceptance fixture.

This is a deterministic gameplay measurement, not a latency benchmark. It keeps
zero-heal interrupted/death cases and the zero-bandage controls. The Rust fixture
uses the unchanged bundled Bind Wounds and Health recovery rules at HD 1/6/11;
initial injury is an authored consumable resolved through the ordinary queue.
Run only while holding the shared development host's execution lock.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
TEST = "supply_lifecycle::bind_wounds_provisional_measurements_use_ordinary_engine_actions"


def output_path(raw):
    path = (Path(raw) if raw else ROOT / ".local" / "bind-wounds" /
            datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")).resolve()
    if not path.is_relative_to((ROOT / ".local").resolve()):
        raise ValueError("Measurement artifacts must be kept below .local/")
    if path.exists():
        raise ValueError("Choose a new artifact directory; previous measurements are preserved")
    return path


def capture(command):
    return subprocess.check_output(command, cwd=ROOT, text=True, encoding="utf-8").strip()


def validate(records):
    common = {"case", "hd", "initial_bandages", "initial_hp", "maximum_hp", "finite_uses",
              "explicit_heal", "natural_heal", "rows"}
    rest = {"natural_period", "active_ticks_to_full"}
    pressure = {"final_hp", "active_ticks", "horizon_ticks", "attempts", "suspended_attempts",
                "dead", "damage_received"}
    rest_boundary = {"tick", "hp", "bandages", "natural_heal", "explicit_heal", "spent", "cooldown"}
    pressure_boundary = {"tick", "hp", "bandages", "preparation_active", "cooldown",
                         "recovery_elapsed_before", "active_tick_delta", "recovery_period",
                         "recovery_upper_bound"}

    def unsigned_fields(value, fields):
        if any(type(value[name]) is not int or not 0 <= value[name] <= 2**64 - 1
               for name in fields):
            raise ValueError("Measurement counters must be unsigned integers, not booleans")

    if type(records) is not list:
        raise ValueError("Measurements must be a list of complete records")
    for row in records:
        if type(row) is not dict or row.get("case") not in ("uncontested_rest", "hostile_pressure"):
            raise ValueError("Unknown measurement record")
        is_rest = row["case"] == "uncontested_rest"
        fields = common | (rest if is_rest else pressure)
        if set(row) != fields:
            raise ValueError("Missing or unknown measurement fields")
        unsigned_fields(row, fields - {"case", "rows", "dead"})
        if not is_rest and type(row["dead"]) is not bool:
            raise ValueError("Death evidence must be a boolean")
        if row["maximum_hp"] == 0 or row["initial_hp"] > row["maximum_hp"]:
            raise ValueError("Invalid initial Health evidence")
        if is_rest and row["natural_period"] == 0:
            raise ValueError("Invalid natural recovery period")
        if type(row["rows"]) is not list or not 1 <= len(row["rows"]) <= (2000 if is_rest else 1000):
            raise ValueError("Missing or unbounded boundary observations")
        for boundary in row["rows"]:
            fields = rest_boundary if is_rest else pressure_boundary
            if type(boundary) is not dict or set(boundary) != fields:
                raise ValueError("Missing or unknown boundary fields")
            unsigned_fields(boundary, fields - {"preparation_active"})
            if not is_rest and type(boundary["preparation_active"]) is not bool:
                raise ValueError("Preparation evidence must be a boolean")
            if boundary["hp"] > row["maximum_hp"] or boundary["bandages"] > row["initial_bandages"]:
                raise ValueError("Boundary exceeds physical Health or stock")
    expected = {(kind, hd, stock) for kind in ("uncontested_rest", "hostile_pressure")
                for hd in (1, 6, 11) for stock in (0, 3)}
    actual = {(row["case"], row["hd"], row["initial_bandages"]) for row in records}
    if actual != expected or len(records) != len(expected):
        raise ValueError("Measurement fixture did not retain every HD, pressure and no-bandage case")
    for row in records:
        if not 0 <= row["finite_uses"] <= row["initial_bandages"]:
            raise ValueError("Invalid finite-use accounting")
        if row["initial_bandages"] == 0 and (row["finite_uses"] or row["explicit_heal"]):
            raise ValueError("No-bandage control spent or explicitly healed")
        if row["case"] == "uncontested_rest":
            if row["explicit_heal"] + row["natural_heal"] != row["maximum_hp"] - row["initial_hp"]:
                raise ValueError("Rest-to-full healing did not reconcile")
        else:
            if row["damage_received"] <= 0:
                raise ValueError("Hostile-pressure case did not exercise hostile AI")
            for boundary in row["rows"]:
                names = ("recovery_elapsed_before", "active_tick_delta", "recovery_period", "recovery_upper_bound")
                if any(type(boundary.get(name)) is not int or boundary[name] < 0 for name in names):
                    raise ValueError("Missing or invalid pressure recovery-bound evidence")
                elapsed, delta, period, upper = (boundary[name] for name in names)
                if upper != elapsed + delta or upper >= period:
                    raise ValueError("Pressure interval could conceal natural recovery")
            if row["natural_heal"] != 0:
                raise ValueError("Pressure recovery contradicts its zero-boundary evidence")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("debug", "release"), default="release")
    parser.add_argument("--output", help="New artifact directory below .local/")
    args = parser.parse_args()
    destination = output_path(args.output)
    destination.mkdir(parents=True)
    records_path = destination / "outcomes.jsonl"
    command = ["cargo", "test", "-p", "tor-server", "--test", "it", "--locked"]
    if args.profile == "release":
        command.append("--release")
    command += [TEST, "--", "--exact", "--nocapture"]
    metadata = {"schema_version": 1, "workload": "bind-wounds-ordinary-engine-v1",
                "profile": args.profile, "command": command,
                "commit": capture(["git", "rev-parse", "HEAD"]),
                "status": capture(["git", "status", "--short"]),
                "rustc": capture(["rustc", "--version"]), "host": platform.platform(),
                "boundary": "Deterministic actor-active simulation ticks; excludes wall-clock timing",
                "limitations": "Authored fixed-damage pressure, finite three-bandage stock, one seed (42); no balance claim"}
    metadata_path = destination / "metadata.json"
    metadata_path.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    environment = dict(os.environ, TOR_BIND_WOUNDS_MEASUREMENTS=str(records_path))
    with (destination / "test.log").open("w", encoding="utf-8") as log:
        result = subprocess.run(command, cwd=ROOT, env=environment, stdout=log, stderr=subprocess.STDOUT)
    metadata["returncode"] = result.returncode
    if result.returncode == 0:
        records = [json.loads(line) for line in records_path.read_text(encoding="utf-8").splitlines()]
        validate(records)
        metadata["outcomes_sha256"] = hashlib.sha256(records_path.read_bytes()).hexdigest()
        summary = [{key: value for key, value in row.items() if key != "rows"} for row in records]
        (destination / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
        for row in summary:
            ticks = row.get("active_ticks_to_full", row.get("active_ticks"))
            print(f"{row['case']} HD{row['hd']} stock={row['initial_bandages']}: "
                  f"ticks={ticks} uses={row['finite_uses']} explicit={row['explicit_heal']} "
                  f"natural={row['natural_heal']} dead={row.get('dead', False)}")
    metadata_path.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    print(f"{'PASS' if result.returncode == 0 else 'FAIL'}: {destination}")
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
