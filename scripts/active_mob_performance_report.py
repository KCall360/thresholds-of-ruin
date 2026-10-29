"""Validate the complete active-mobs-v1 workload, not a short diagnostic prefix."""
import argparse
import json
import math
from pathlib import Path

from workload_report import is_duration, require
from performance_report import observer_profile_version, validate_demand_observer_work, ACTION_OBSERVER_COUNTS


def distribution(values):
    require(isinstance(values, list) and values and all(is_duration(v) for v in values),
            "missing or invalid duration samples")
    ordered = sorted(values)
    return {"n": len(values), **{f"p{p}_ms": ordered[math.ceil(len(values) * p / 100) - 1]
                               for p in (50, 95, 99)}, "max_ms": ordered[-1]}


def positive_int(value):
    return type(value) is int and value > 0


def validate_activity(activity, steps):
    require(isinstance(activity, dict), "missing private activity proof")
    actors, regions = activity.get("actors"), activity.get("regions")
    require(isinstance(actors, dict) and set(actors) == {str(i) for i in range(2, 102)},
            "all 100 real AI identities must execute")
    require(isinstance(regions, dict) and set(regions) == {str(i) for i in range(1, 21)},
            "all 20 regions must contain live AI work")
    for ident, row in actors.items():
        require(isinstance(row, dict), f"invalid AI {ident}")
        require(positive_int(row.get("executions")) and row["executions"] > 1,
                f"AI {ident} did not execute repeatedly")
        require(type(row.get("first_tick")) is int and type(row.get("last_tick")) is int
                and row["last_tick"] > row["first_tick"], f"AI {ident} never advanced")
        for field in ("moves", "combat_starts", "waits", "portal_crossings"):
            require(type(row.get(field)) is int and 0 <= row[field] <= row["executions"],
                    f"invalid {field} for AI {ident}")
        require(row["moves"] + row["combat_starts"] > 0, f"AI {ident} only waited")
    for ident, row in regions.items():
        members = [actors[str(i)] for i in range(2 + (int(ident) - 1) * 5, 7 + (int(ident) - 1) * 5)]
        require(positive_int(row.get("moves")) and positive_int(row.get("combat_starts")),
                f"region {ident} must exercise pursuit movement and combat")
        for field in ("executions", "moves", "combat_starts", "waits", "portal_crossings"):
            require(row.get(field) == sum(a[field] for a in members),
                    f"region {ident} activity must reconcile with its physically isolated AI owners")
    require(activity.get("player_moves") == steps, "every commanded player move must complete")
    require(positive_int(activity.get("player_crossings")), "player must traverse a real portal")
    require(activity["player_crossings"] == (steps + 1) // 8,
            "player crossings must match the exact corridor route")
    return sum(row["executions"] for row in actors.values())


def validate(row):
    require(isinstance(row, dict) and row.get("workload") == "active-mobs-v1"
            and row.get("version") == 1 and row.get("mode") == "direct", "unsupported workload")
    require(row.get("diagnostic_preloading", False) is False,
            "enabled-preloader diagnostic is not the baseline workload")
    require(row.get("seed") == 42 and row.get("regions") == 20 and row.get("ai") == 100,
            "workload dimensions or seed changed")
    require(type(row.get("steps")) is int and row["steps"] >= 8, "a probe is not a complete workload")
    require(all(row.get(f) is True for f in ("all_active", "all_alive", "activity_complete", "replay_equal")),
            "active/alive/completed/replay proof required")
    ai_commands = validate_activity(row.get("activity"), row["steps"])
    samples = row
    require(isinstance(samples, dict), "missing measurements")
    for name, count in (("command_ms", ai_commands + row["steps"]), ("ai_command_ms", ai_commands),
                        ("player_command_ms", row["steps"]), ("player_cycle_ms", row["steps"]),
                        ("projection_ms", row["steps"]), ("serialization_ms", row["steps"])):
        require(isinstance(samples.get(name), list) and len(samples[name]) == count,
                f"{name}: each completed operation needs one timing sample")
        distribution(samples[name])
    work = samples.get("work")
    require(isinstance(work, dict) and all(type(v) is int and v >= 0 for v in work.values()), "invalid work counts")
    if observer_profile_version(row) == 3:
        validate_demand_observer_work(work, positive=True)
    else:
        require(not any(field in work for field in ACTION_OBSERVER_COUNTS), "Unmarked causal observer accounting")
    require(positive_int(work.get("route_searches")) and positive_int(work.get("strike_resolutions")),
            "pathfinding and resolved combat must actually execute")
    phases = samples.get("phase_totals_ms")
    require(isinstance(phases, dict) and phases and all(is_duration(v) for v in phases.values()), "invalid phase totals")
    for name in ("package_ms", "cold_activation_ms", "workload_ms", "save_ms", "resume_ms"):
        require(is_duration(row.get(name)), f"invalid {name}")
    status = row.get("save_status")
    require(isinstance(status, dict) and status.get("pending_bytes") == 0 and status.get("error") is None
            and status.get("accepted_sequence") == status.get("durable_sequence"), "save did not reach a durable barrier")
    require(positive_int(row.get("saved_bytes")) and positive_int(row.get("retained_history")), "missing storage proof")
    require(isinstance(row.get("state_sha256"), str) and len(row["state_sha256"]) == 64
            and all(c in "0123456789abcdef" for c in row["state_sha256"]), "missing replay state hash")
    return row


def extract(rows):
    require(len(rows) == 1, "one complete deterministic cohort is required")
    row = validate(rows[0])
    group = f"r20-ai100-s{row['steps']}"
    timings = {name: values for name, values in row.items() if name.endswith("_ms") and isinstance(values, list)}
    timings.update({name: [row[name]] for name in ("package_ms", "cold_activation_ms", "workload_ms", "save_ms", "resume_ms")})
    counts = dict(row["work"], saved_bytes=row["saved_bytes"], retained_history=row["retained_history"],
                  player_moves=row["activity"]["player_moves"], player_crossings=row["activity"]["player_crossings"])
    return {group: timings}, {group: counts}, 1


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("samples", type=Path)
    args = parser.parse_args(argv)
    rows = [json.loads(line) for line in args.samples.read_text(encoding="utf-8").splitlines() if line.strip()]
    timings, counts, _ = extract(rows)
    print(json.dumps({"workload": "active-mobs-v1", "timings": {g: {m: distribution(v) for m, v in metrics.items()}
                     for g, metrics in timings.items()}, "counts": counts}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
