"""Versioned real-headless acceptance for the active-mobs-v1 release workload.

The fixed-rate phase includes the authoritative move AND the following AI round.
Admission, execution, completion and next-ready are different timing boundaries.
A late command does not reset the next deadline or silently lower the offered rate.
No build is performed; exact artifact hashes require a build manifest. Debug
is permitted only for explicitly labeled correctness smoke, never load acceptance.
"""
import argparse
import json
import math
import os
from pathlib import Path
import re
import subprocess
import time
import uuid

import perf_ledger
from performance_driver import JsonProcess, ROOT, stop_all

VERSION = 1
WORKLOAD = "active-mobs-v1"
AI_IDS = {str(i) for i in range(2, 102)}
REGION_IDS = {str(i) for i in range(1, 21)}
TIMINGS = ("request_to_admission_ms", "request_to_execution_ms",
           "request_to_outcome_ms", "request_to_context_ms", "request_to_next_ready_ms")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def number(value):
    return type(value) in (int, float) and math.isfinite(value) and value >= 0


def distribution(values):
    require(bool(values) and all(number(value) for value in values), "Invalid timing samples")
    ordered = sorted(values)
    return {"n": len(values), "p50_ms": perf_ledger.nearest_rank(ordered, 50),
            "p95_ms": perf_ledger.nearest_rank(ordered, 95),
            "p99_ms": perf_ledger.nearest_rank(ordered, 99), "max_ms": ordered[-1]}


def deadline(origin, index, rate):
    require(type(index) is int and index >= 0 and number(rate) and rate > 0, "Invalid pacing")
    return origin + index / rate


def center(frame):
    cells = frame["state"]["observation"]["visible_cells"]
    keys = {cell["key"] for cell in cells if cell["position"] == {"x": 0, "y": 0, "z": 0}}
    require(len(keys) == 1, "Missing or ambiguous disclosed actor cell")
    return keys.pop()


def east_destination(frame):
    keys = {cell["key"] for cell in frame["state"]["observation"]["visible_cells"]
            if cell["position"] == {"x": 1, "y": 0, "z": 0} and not cell["wall"]}
    require(len(keys) == 1, "East movement lacks one disclosed traversable destination")
    return keys.pop()


def ready_for_next(frame):
    require(not frame.get("error"), f"Headless error: {frame.get('error')}")
    if frame.get("state_omitted"):
        return False
    require(frame.get("has_control") is True, "Actor control was lost")
    observation = (frame.get("state") or {}).get("observation") or {}
    combat = observation.get("combat") or {}
    require(not combat.get("dead") and not combat.get("terminal"), "Workload actor died or run ended")
    return (frame.get("synchronized") is True and observation.get("ready") is True
            and (frame.get("readiness") or {}).get("admission") is True)


def move(client, before, origin, index, scheduled):
    expected = east_destination(before)
    after, started, _, received = client.send(
        {"type": "act", "action": {"type": "move", "direction": "east"}}, wait_for_completion=True)
    timing = dict(client.action_timing or {})
    require(timing.get("outcome") == "resolved", "Move was not authoritatively resolved")
    require(all(number(timing.get(key)) for key in TIMINGS[:-1]), "Missing action timing boundary")
    # Causal readiness may arrive before the demand-driven picture. Keep the
    # original end-to-end boundary: require a newer picture, then verify its
    # exact disclosed destination below; never mistake the retained old picture
    # for a failed or completed move.
    def ready_picture(frame):
        return (ready_for_next(frame)
                and int(frame["state"]["revision"]) > int(before["state"]["revision"]))
    if not ready_picture(after):
        after, received = client.until(ready_picture)
    require(center(after) == expected and center(after) != center(before),
            "Completed move did not reach its disclosed destination")
    require(int(after["state"]["revision"]) > int(before["state"]["revision"]), "Move did not advance revision")
    timing["request_to_next_ready_ms"] = (received - started) * 1000
    row = {"sample": index, "started_seconds": started - origin,
           "next_ready_seconds": received - origin, "scheduled_seconds": scheduled - origin,
           "deadline_lateness_ms": max(0., started - scheduled) * 1000,
           "before_cell": center(before), "expected_cell": expected, "after_cell": center(after),
           "reader_work_ms": client.last_reader_work_ms, "reader_queue_delay_ms": client.last_queue_delay_ms,
           **{key: timing[key] for key in TIMINGS}}
    require([row[key] for key in TIMINGS] == sorted(row[key] for key in TIMINGS), "Reversed completion boundaries")
    return after, row


def same_disclosed_state(before, after):
    """Reset changes picture capabilities/generations, not disclosed contents."""
    def contents(frame):
        return {key: value for key, value in frame["state"].items()
                if key not in ("snapshot", "revision")}
    return (contents(before) == contents(after)
            and all(before[key] == after[key] for key in ("branch", "history")))


def rss(pid):
    """Best-effort resident bytes on Linux, not an allocation or live-heap measure."""
    try:
        for line in Path(f"/proc/{pid}/status").read_text().splitlines():
            if line.startswith("VmRSS:"):
                return int(line.split()[1]) * 1024
    except (OSError, ValueError):
        pass
    return None


def memory_summary(rows):
    result = {}
    for field in ("server_rss_bytes", "client_rss_bytes", "driver_rss_bytes"):
        present = [(row["seconds"], row.get(field)) for row in rows if type(row.get(field)) is int]
        if len(present) < 2 or present[-1][0] <= present[0][0]:
            result[field] = None
            continue
        growth = present[-1][1] - present[0][1]
        result[field] = {"n": len(present), "first_bytes": present[0][1], "last_bytes": present[-1][1],
                         "maximum_bytes": max(value for _, value in present), "growth_bytes": growth,
                         "endpoint_growth_bytes_per_second": growth / (present[-1][0] - present[0][0])}
    return result


def validate_sustained_activity(report, moves, activity):
    """Offline v1 gap proof; does not change pacing or sampled command work."""
    proof = report.get("sustained_activity")
    require(isinstance(proof, dict), "Missing sustained meaningful-activity proof")
    expected = dict(player_moves=moves, player_turn_ticks=100, player_first_tick=0,
                    player_last_tick=(moves - 1) * 100, max_gap_limit_ticks=200)
    require(all(type(proof.get(key)) is int and proof[key] == value for key, value in expected.items()),
            "Noncanonical sustained workload interval")
    actors = proof.get("actors")
    require(isinstance(actors, dict) and set(actors) == AI_IDS, "Incomplete sustained AI identities")
    for key, row in actors.items():
        require(isinstance(row, dict) and all(type(row.get(field)) is int and row[field] >= 0
                    for field in ("meaningful_actions", "first_tick", "last_tick", "max_gap_ticks")),
                f"AI {key} has invalid sustained evidence")
        require(row["meaningful_actions"] == activity["actors"][key]["moves"]
                + activity["actors"][key]["combat_starts"] > 0,
                f"AI {key} meaningful counts do not reconcile")
        require(row["first_tick"] == 0, f"AI {key} did not start meaningful activity at tick 0")
        require(row["max_gap_ticks"] <= 200, f"AI {key} meaningful action gap exceeds 200 ticks")
        require(proof["player_last_tick"] - row["last_tick"] <= 200,
                f"AI {key} stopped meaningful activity before the final window")
        require(row["last_tick"] <= activity["actors"][key]["last_tick"]
                and row["last_tick"] <= row["max_gap_ticks"] * (row["meaningful_actions"] - 1),
                f"AI {key} sustained timing does not reconcile")


def validate_activity(report, moves):
    require(report.get("workload") == WORKLOAD and type(report.get("version")) is int
            and report["version"] == VERSION and report.get("mode") == "inspect", "Wrong activity report")
    require(report.get("seed") == 42 and report.get("regions") == 20 and report.get("ai") == 100,
            "Wrong workload dimensions")
    require(report.get("all_active") is True and report.get("all_alive") is True,
            "Loaded, frozen, dead or passive actors do not establish active load")
    require({str(value) for value in report.get("actor_ids", [])} == {str(i) for i in range(1, 102)},
            "Wrong authoritative actor identity set")
    activity = report.get("activity") or {}
    actors, regions = activity.get("actors") or {}, activity.get("regions") or {}
    require(set(actors) == AI_IDS and set(regions) == REGION_IDS, "Incomplete AI or region activity")
    for key, value in actors.items():
        require(type(value.get("executions")) is int and value["executions"] > 1,
                f"AI {key} did not execute repeatedly")
        require(all(type(value.get(field)) is int and 0 <= value[field] <= value["executions"]
                    for field in ("moves", "combat_starts", "waits", "portal_crossings")),
                f"AI {key} has inconsistent action counts")
        require(value["moves"] + value["combat_starts"] > 0, f"AI {key} only waited")
        require(type(value.get("first_tick")) is int and type(value.get("last_tick")) is int
                and value["last_tick"] > value["first_tick"], f"AI {key} never advanced across ticks")
    for key, value in regions.items():
        require(type(value.get("moves")) is int and value["moves"] > 0
                and type(value.get("combat_starts")) is int and value["combat_starts"] > 0,
                f"Region {key} lacks pursuit movement and combat")
        members = [actors[str(ident)] for ident in range(2 + (int(key) - 1) * 5, 7 + (int(key) - 1) * 5)]
        require(all(value.get(field) == sum(actor[field] for actor in members)
                    for field in ("executions", "moves", "combat_starts", "waits", "portal_crossings")),
                f"Region {key} activity does not reconcile with its isolated actors")
    require(activity.get("player_moves") == moves, "Accepted requests are not exact completed moves")
    require(activity.get("player_crossings") == (moves + 1) // 8, "Incorrect authoritative portal crossing count")
    require(re.fullmatch(r"[0-9a-f]{64}", report.get("state_sha256", "")) is not None,
            "Missing authoritative replay-state digest")
    validate_sustained_activity(report, moves, activity)
    return activity


def summarize(rows, elapsed, rate):
    require(number(elapsed) and elapsed > 0 and number(rate) and rate > 0, "Invalid measurement duration")
    require(bool(rows), "No completed moves")
    for index, row in enumerate(rows):
        require(type(row.get("sample")) is int and row["sample"] == index, "Noncontiguous sample inventory")
        require(all(number(row.get(key)) for key in (*TIMINGS, "deadline_lateness_ms",
                                                    "started_seconds", "next_ready_seconds")), "Invalid sample")
        require([row[key] for key in TIMINGS] == sorted(row[key] for key in TIMINGS), "Reversed timing boundaries")
        require(row["next_ready_seconds"] >= row["started_seconds"], "Negative action interval")
        require(row.get("after_cell") == row.get("expected_cell")
                and row.get("after_cell") != row.get("before_cell"), "Unexecuted movement sample")
    window = max(1, min(25, len(rows) // 4))
    early = distribution([r["deadline_lateness_ms"] for r in rows[:window]])["p50_ms"]
    late = distribution([r["deadline_lateness_ms"] for r in rows[-window:]])["p50_ms"]
    return {"moves": len(rows), "elapsed_seconds": elapsed, "completed_hz": len(rows) / elapsed,
            "offered_hz": rate,
            "timings": {key: distribution([row[key] for row in rows]) for key in TIMINGS},
            "deadline_lateness": distribution([row["deadline_lateness_ms"] for row in rows]),
            "late_minus_early_median_ms": late - early,
            "maximum_driver_backlog_slots": max(int(row["deadline_lateness_ms"] * rate / 1000) for row in rows),
            "backlog_boundary": "Unsent fixed-rate driver deadlines, not a server mailbox-depth measurement"}


def verify_build_manifest(path, binaries, *, smoke=False):
    manifest = json.loads(path.read_text(encoding="utf-8"))
    require(type(smoke) is bool, "Smoke selection must be explicit")
    require(manifest.get("profile") == "release" or (smoke and manifest.get("profile") == "debug"),
            "Workload requires a qualified release build; debug is correctness smoke only")
    for key in ("source_commit", "source_tree"):
        require(re.fullmatch(r"[0-9a-f]{40}", manifest.get(key, "")) is not None, f"Missing {key}")
    require(type(manifest.get("dirty")) is bool and bool(manifest.get("toolchain")), "Missing source/toolchain provenance")
    for name, binary in binaries.items():
        expected = (manifest.get("binaries") or {}).get(name)
        if isinstance(expected, dict):
            expected = expected.get("sha256")
        require(expected == perf_ledger.sha256_file(binary), f"Unqualified executable bytes: {name}")
    return {"manifest": manifest, "manifest_sha256": perf_ledger.sha256_file(path),
            "paths": {name: str(binary) for name, binary in binaries.items()}}


def run_tool(binary, mode, save, output, label, env):
    command = [str(binary), mode, str(save)]
    with (output / f"{label}.stdout.jsonl").open("w", encoding="utf-8") as out, \
            (output / f"{label}.stderr.log").open("w", encoding="utf-8") as err:
        result = subprocess.run(command, cwd=ROOT, env=env, stdout=out, stderr=err, timeout=600)
    (output / f"{label}.receipt.json").write_text(json.dumps(
        {"command": command, "exit_code": result.returncode}), encoding="utf-8")
    require(result.returncode == 0, f"{label} failed; retained logs are not qualified evidence")
    lines = (output / f"{label}.stdout.jsonl").read_text(encoding="utf-8").splitlines()
    require(len(lines) == 1, f"{label} did not emit one complete report")
    return json.loads(lines[0])


def paced_completion_budget(duration, count, *, smoke):
    """Aggregate liveness cap, not a smoke throughput target.

    Smoke receives one existing 30s response allowance per planned move. A move
    can contain several independently bounded protocol waits; this aggregate
    cap intentionally remains stricter than their combined worst-case bound.
    Full acceptance retains its original duration-plus-30s cutoff.
    """
    require(number(duration) and duration > 0 and type(count) is int and count > 0
            and type(smoke) is bool, "Invalid completion-budget inputs")
    budget = duration + 30. * (count if smoke else 1)
    require(number(budget), "Nonfinite completion budget")
    return budget


def load_target_met(paced, headroom_ratio, *, smoke):
    require(type(smoke) is bool, "Smoke must be an explicit boolean")
    return (not smoke and paced["completed_within_schedule"]
            and paced["timings"]["request_to_next_ready_ms"]["p95_ms"] < 100
            and paced["timings"]["request_to_next_ready_ms"]["p99_ms"] < 200
            and paced["late_minus_early_median_ms"] < 100
            and paced["maximum_driver_backlog_slots"] == 0 and headroom_ratio > 1)


def finalize_owned(owned, output, result, run_error):
    """Keep the original failure and publish the terminal cleanup outcome."""
    previous_errors = list(result.get("diagnostics_error", []))
    cleanup_error = None
    try:
        stop_all(owned, output, result)
    except BaseException as error:
        cleanup_error = str(error)
        raise
    finally:
        errors = list(dict.fromkeys(previous_errors + result.get("diagnostics_error", [])))
        if cleanup_error is not None and not errors:
            errors.append(cleanup_error)
        result["cleanup"] = {"status": "failed" if errors else "complete", "errors": errors}
        if run_error is not None or errors:
            (output / "failure.json").write_text(json.dumps({
                "error": run_error if run_error is not None else cleanup_error or "; ".join(errors),
                "cleanup_errors": errors, "partial_result": result}, indent=2), encoding="utf-8")


def run(bin_dir, benchmark, output, build_manifest, *, duration=60., rate=5., burst=20, smoke=False):
    require(number(duration) and duration > 0 and number(rate) and rate > 0, "Invalid workload duration/rate")
    require(type(burst) is int and burst > 0, "Invalid headroom phase")
    require(smoke or (duration >= 60 and rate >= 5), "Acceptance requires at least 60 seconds at 5 Hz")
    count = math.ceil(duration * rate)
    require(count >= 16, "Even a smoke run needs at least 16 measured moves")
    completion_budget = paced_completion_budget(duration, count, smoke=smoke)
    binaries = {"tor-server": bin_dir / ("tor-server.exe" if os.name == "nt" else "tor-server"),
                "tor-client-headless": bin_dir / ("tor-client-headless.exe" if os.name == "nt" else "tor-client-headless"),
                "active_mob_bench": benchmark}
    provenance = verify_build_manifest(build_manifest, binaries, smoke=smoke)
    output.mkdir(parents=True, exist_ok=False)
    owned = []
    env = {key: value for key, value in os.environ.items()
           if not key.startswith("TOR_")}
    env.update(TOR_SERVER_TOKEN=uuid.uuid4().hex, TOR_DRIVER_DEFER_LOGS="1")
    save = output / "game.db"
    result = {"workload": WORKLOAD, "version": VERSION, "kind": "headless", "seed": 42,
              "regions": 20, "ai": 100, "smoke": smoke, "rate_hz": rate, "duration_seconds": duration,
              "planned_moves": count, "planned_burst_moves": burst, "provenance": provenance,
              "paced_completion_budget_seconds": completion_budget,
              "cleanup": {"status": "pending", "errors": None},
              "machine": perf_ledger.probe_machine(output), "samples": [], "burst_samples": [], "memory": [],
              "diagnostic_mode": "deferred; omit-memory and compact-background; full gameplay state retained by client",
              "limitations": ["RSS is host-resident memory, not allocations; unavailable hosts report null",
                              "World is fully activated before timed input; bootstrap measures cold activation separately",
                              "Per-phase simulation/navigation/storage timing comes from the matching direct Engine report",
                              "Driver deadline backlog is measured; server mailbox depth is not inferred"],
              "complete": False}

    def launch(name, args, label):
        process = JsonProcess(binaries[name], args, env, output, label)
        owned.append(process)
        return process

    def start(label):
        began = time.perf_counter()
        host = launch("tor-server", ["--listen", "127.0.0.1:0", "--save", save], f"{label}-server")
        ready, _ = host.until(lambda frame: "address" in frame)
        client = launch("tor-client-headless", ["--connect", ready["address"], "--actor", "1",
                                               "--omit-memory", "--compact-background"], f"{label}-headless")
        frame, _ = client.until(lambda value: value.get("type") == "ready")
        if not ready_for_next(frame):
            frame, _ = client.until(ready_for_next)
        return host, client, frame, (time.perf_counter() - began) * 1000

    def memory(host, client, elapsed):
        result["memory"].append({"seconds": elapsed, "server_rss_bytes": rss(host.child.pid),
                                 "client_rss_bytes": rss(client.child.pid), "driver_rss_bytes": rss(os.getpid())})

    def save_and_stop(host, client):
        saved, *_ = client.send({"type": "request", "request": {"type": "save"}})
        require(saved.get("error") is None, "Explicit save failed")
        snapshot = client.snapshot()
        stop_all([host, client], output, result)
        owned.remove(client)
        owned.remove(host)
        return snapshot

    run_error = None
    try:
        result["bootstrap"] = run_tool(benchmark, "--bootstrap", save, output, "bootstrap", env)
        bootstrap = result["bootstrap"]
        require(bootstrap.get("workload") == WORKLOAD and bootstrap.get("mode") == "bootstrap"
                and bootstrap.get("version") == VERSION and bootstrap.get("seed") == 42
                and bootstrap.get("regions") == 20 and bootstrap.get("ai") == 100
                and bootstrap.get("all_active") is True and bootstrap.get("initial_records") == 0,
                "Bootstrap does not establish an unplayed fully active workload")
        package = Path(bootstrap["package"])
        require(package.resolve().parent == output.resolve() and package.is_dir(), "Unexpected generated package location")
        retained_package = output / "source-package-retained"
        package.rename(retained_package)
        require(not package.exists(), "Original package must be absent before source-free launch")
        result["source_package_removed_from_original_path"] = True
        host, client, before, result["initial_process_startup_ms"] = start("initial")
        origin = time.perf_counter()
        memory(host, client, 0.)
        for index in range(count):
            target = deadline(origin, index, rate)
            delay = target - time.perf_counter()
            if delay > 0:
                time.sleep(delay)
            require(time.perf_counter() <= origin + completion_budget,
                    "Paced workload exceeded its finite completion budget; incomplete prefix retained")
            before, sample = move(client, before, origin, index, target)
            result["samples"].append(sample)
            memory(host, client, sample["next_ready_seconds"])
            require(sample["next_ready_seconds"] <= completion_budget,
                    "Paced workload exceeded its finite completion budget; incomplete prefix retained")
        remaining = origin + duration - time.perf_counter()
        if remaining > 0:
            time.sleep(remaining)
        elapsed = time.perf_counter() - origin
        result["paced"] = summarize(result["samples"], elapsed, rate)
        result["paced"]["completed_within_schedule"] = result["samples"][-1]["next_ready_seconds"] <= duration
        result["memory_summary"] = memory_summary(result["memory"])
        before_restart = save_and_stop(host, client)
        result["measured_inspection"] = run_tool(benchmark, "--inspect", save, output, "measured-inspect", env)
        validate_activity(result["measured_inspection"], count)
        host, client, before, result["restart_to_ready_ms"] = start("restart")
        result["restart_equal"] = same_disclosed_state(before, before_restart)
        require(result["restart_equal"], "Source-free process restart changed authoritative disclosed state")
        origin = time.perf_counter()
        for index in range(burst):
            target = time.perf_counter()
            before, sample = move(client, before, origin, index, target)
            result["burst_samples"].append(sample)
        result["burst"] = summarize(result["burst_samples"], time.perf_counter() - origin, rate)
        result["headroom_ratio"] = result["burst"]["completed_hz"] / rate
        save_and_stop(host, client)
        result["final_inspection"] = run_tool(benchmark, "--inspect", save, output, "final-inspect", env)
        validate_activity(result["final_inspection"], count + burst)
        result["provisional_load_target_met"] = load_target_met(
            result["paced"], result["headroom_ratio"], smoke=smoke)
    except BaseException as error:
        run_error = str(error)
        # Retain the incomplete prefix even if cleanup is interrupted. The
        # terminal receipt below records cleanup success or both failure causes.
        (output / "failure.json").write_text(json.dumps({
            "error": run_error, "cleanup_errors": None, "partial_result": result}, indent=2), encoding="utf-8")
        raise
    finally:
        finalize_owned(owned, output, result, run_error)
    # A completed result is written only after every owned process and reader closed.
    result["complete"] = True
    (output / "result.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, required=True)
    parser.add_argument("--benchmark", type=Path, required=True)
    parser.add_argument("--build-manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--duration", type=float, default=60.)
    parser.add_argument("--rate", type=float, default=5.)
    parser.add_argument("--burst", type=int, default=20)
    parser.add_argument("--smoke", action="store_true")
    args = parser.parse_args()
    result = run(args.bin_dir.resolve(), args.benchmark.resolve(), args.output.resolve(),
                 args.build_manifest.resolve(), duration=args.duration, rate=args.rate,
                 burst=args.burst, smoke=args.smoke)
    print(json.dumps({key: result[key] for key in ("complete", "paced", "burst", "headroom_ratio",
                                                 "provisional_load_target_met")}, indent=2))


if __name__ == "__main__":
    main()
