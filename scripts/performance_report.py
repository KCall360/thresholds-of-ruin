"""Validate exact diagnostic coverage and summarize retained JSONL samples."""
import argparse
from collections import defaultdict
import gzip
import itertools
import json
import math
from pathlib import Path
from workload_report import require, is_duration, nonnegative_counts, positive_counts

ROOT = Path(__file__).resolve().parents[1]
SPEC = json.loads((ROOT/"crates/server/fixtures/performance-v1.json").read_text())


def expected_actions(regions, actors, history, cycles):
    # Independent scheduler oracle: equal 100-tick turns, diagonal cost 142,
    # rejected attempts free, identity breaks ties. No server state is consulted.
    ready = [(history//actors + (i < history%actors))*100 for i in range(actors)]
    result = []
    secondary = 0
    def record(actor, step):
        action = step["action"]
        result.append((actor,step["label"],action,step["expected"]))
        if step["expected"] != "blocked":
            ready[actor-1] += 142 if "_" in action.get("direction","") else 100
    for _ in range(cycles):
        for step in SPEC["steps"]:
            if step.get("min_regions",1) > regions:
                continue
            while (actor := min(range(actors),key=lambda i:(ready[i],i))+1) != 1:
                if actor == 2:
                    other = SPEC["secondary"][secondary % len(SPEC["secondary"])]
                    secondary += 1
                else:
                    other = {"label":"wait_same_region","action":{"type":"wait"},"expected":"waited"}
                record(actor,other)
            record(1,step)
    return result


STREAM_CASES = {"stream-r16-memory", "stream-r16-durable", "stream-r256-memory", "stream-r256-durable"}
# Per-command bounds for the streaming workload: one reference point, radii
# 1 and 2, and at most the character and the guard loaded. None may depend
# on the corridor's length.
STREAM_BOUNDS = {"horizon_regions_expanded": 8, "horizon_links_examined": 16, "pinned_actors": 2,
                 "reach_lookups": 2, "region_records_read": 4, "regions_built": 4, "region_changes": 1}
# What the preloader is asked for after each command: one hop beyond the
# loaded regions. Recorded when the run preloads.
PRELOAD_BOUNDS = {"preload_jobs": 4, "preload_regions_expanded": 8, "preload_links_examined": 16}

# Native Duration fields in command, recovery and acquisition profiles and
# Rogue records. Historical profiles may omit newer fields; supplied fields
# must retain their schema rather than being mistaken for scalar counters.
NATIVE_DURATION_FIELDS = {
    "authoritative_total", "rollback_capture", "checkpoint_capture", "simulation_transition",
    "navigation_refresh", "perception", "revision_detection", "rollback_snapshot",
    "journal_serialization", "journal_write", "journal_sync", "journal_replace", "publication",
    "region_transition", "group_wait", "group_build", "fallback_read", "fallback_build",
    "total", "elapsed", "barrier",
}



def validate_profile_measurements(profile):
    """Every supplied native count/duration is typed; old optional fields stay optional."""
    require(isinstance(profile, dict) and profile, "Profile must be a nonempty object")
    for field, value in profile.items():
        if field in NATIVE_DURATION_FIELDS or isinstance(value, dict) and ("secs" in value or "nanos" in value):
            require(isinstance(value, dict) and type(value.get("secs")) is int and value["secs"] >= 0
                    and type(value.get("nanos")) is int and 0 <= value["nanos"] < 1_000_000_000,
                    f"Invalid {field} duration")
        elif isinstance(value, dict):
            validate_profile_measurements(value)
        else:
            nonnegative_counts(profile, (field,))


def validate_measurements(rows):
    """Check recorded timings before a report can become performance evidence.

    Only authoritative_total is universal for successful samples. Optional
    phases (including historical wire v1 versus v2) are validated when present.
    Rejected commands may have no authoritative phase because they did no work.
    """
    def durations(value):
        if isinstance(value, dict):
            for field, measurement in value.items():
                if field == "phases_ms":
                    require(isinstance(measurement, dict), "Invalid phases_ms measurement map")
                    require(all(is_duration(item) for item in measurement.values()), "Invalid phases_ms measurement")
                elif field.endswith("_ms"):
                    require(is_duration(measurement), f"Invalid {field} measurement")
                else:
                    durations(measurement)
        elif isinstance(value, list):
            for item in value:
                durations(item)

    for row in rows:
        require(isinstance(row, dict), "Report rows must be objects")
        durations(row)
        count_fields = ("version", "trace_version", "profile_version", "wire_profile_version", "regions",
                        "actors", "actor", "history_start", "history_end", "client_memory", "rewind_count",
                        "cycle", "cycles", "step", "steps", "sample", "samples", "attempt", "depth", "records",
                        "records_per_floor", "floors", "prepared_limit", "groups", "n", "observers", "cells",
                        "final_save_bytes", "checkpoint_json_bytes", "observation_bytes")
        nonnegative_counts(row, (field for field in count_fields if field in row))
        for field in ("elapsed", "barrier"):
            if field in row:
                validate_profile_measurements({field: row[field]})
        status = row.get("save_status")
        if status is not None:
            require(isinstance(status, dict), "Save status must be an object")
            nonnegative_counts(status, (field for field in status if field not in ("error", "saving", "overdue")))
        if row.get("kind") == "sample":
            phases = row.get("phases_ms")
            require(isinstance(phases, dict), "Missing phases_ms measurements")
            require(row.get("expected") == "blocked" or is_duration(phases.get("authoritative_total")),
                    "Missing authoritative_total measurement")
            if row.get("profile") is not None:
                validate_profile_measurements(row["profile"])
        for field in ("recovery",):
            if row.get(field) is not None:
                validate_profile_measurements(row[field])


def validate_region_acquisition(profile, required=False):
    """Nested acquisition metrics partition successful reads/builds only."""
    validate_profile_measurements(profile)
    acquisition = profile.get('region_acquisition')
    if acquisition is None:
        require(not required, 'Missing region acquisition profile')
        return
    require(isinstance(acquisition, dict), 'Invalid evidence: isinstance(acquisition, dict)')
    for name in ('fallback_reads', 'fallback_builds', 'prepared_reads', 'prepared_builds'):
        require(type(acquisition.get(name)) is int and acquisition[name] >= 0, name)
    groups = ('groups_committed', 'prepared_groups', 'demand_groups')
    has_groups = any(name in acquisition for name in groups)
    if has_groups:
        for name in groups:
            require(type(acquisition.get(name)) is int and acquisition[name] >= 0, name)
        require(acquisition['groups_committed'] == acquisition['prepared_groups'] + acquisition['demand_groups'], "Invalid evidence: acquisition['groups_committed'] == acquisition['prepared_groups'] + acquisition['demand_groups']")
    committed = acquisition['groups_committed'] if has_groups else 0
    prepared = acquisition['prepared_groups'] if has_groups else 0
    require(acquisition['fallback_reads'] + acquisition['prepared_reads'] == profile['region_records_read'], "Invalid evidence: acquisition['fallback_reads'] + acquisition['prepared_reads'] == profile['region_records_read']")
    require(acquisition['fallback_builds'] + acquisition['prepared_builds'] + 9*committed == profile['regions_built'], "Invalid evidence: acquisition['fallback_builds'] + acquisition['prepared_builds'] + 9*committed == profile['regions_built']")
    require(acquisition['prepared_reads'] + acquisition['prepared_builds'] + 9*prepared == profile['regions_prepared'], "Invalid evidence: acquisition['prepared_reads'] + acquisition['prepared_builds'] + 9*prepared == profile['regions_prepared']")

    def nanos(value):
        require(isinstance(value, dict), 'Invalid evidence: isinstance(value, dict)')
        require(type(value.get('secs')) is int and value['secs'] >= 0, "Invalid evidence: type(value.get('secs')) is int and value['secs'] >= 0")
        require(type(value.get('nanos')) is int and 0 <= value['nanos'] < 1000000000, "Invalid evidence: type(value.get('nanos')) is int and 0 <= value['nanos'] < 1000000000")
        return value['secs']*1000000000 + value['nanos']

    read = nanos(acquisition.get('fallback_read'))
    build = nanos(acquisition.get('fallback_build'))
    group_wait = nanos(acquisition.get('group_wait')) if has_groups else 0
    group_build = nanos(acquisition.get('group_build')) if has_groups else 0
    require(read + build + group_wait + group_build <= nanos(profile.get('region_transition')), 'Nested timings exceed transition')
    require(acquisition['fallback_reads'] or read == 0, "Invalid evidence: acquisition['fallback_reads'] or read == 0")
    require(acquisition['fallback_builds'] or build == 0, "Invalid evidence: acquisition['fallback_builds'] or build == 0")
    require(committed or group_wait == 0, 'Invalid evidence: committed or group_wait == 0')
    require(not has_groups or acquisition['demand_groups'] or group_build == 0, "Invalid evidence: not has_groups or acquisition['demand_groups'] or group_build == 0")


def validate_rogue(rows):
    """Complete floor generation and streaming diagnostics, with work bounds."""
    validate_measurements(rows)
    metadata = [row for row in rows if row['kind'] == 'rogue']
    require(len(metadata) == 1, 'Invalid evidence: len(metadata) == 1')
    meta = metadata[0]
    require(meta['version'] == 1 and meta['floors'] == 26, "Invalid evidence: meta['version'] == 1 and meta['floors'] == 26")
    require(meta['records_per_floor'] == 9 and meta['prepared_limit'] == 32, "Invalid evidence: meta['records_per_floor'] == 9 and meta['prepared_limit'] == 32")
    require(type(meta['samples']) is int and meta['samples'] > 0, "Invalid evidence: type(meta['samples']) is int and meta['samples'] > 0")
    expected = set(itertools.product(range(meta['samples']), range(1, 27)))
    generation = [row for row in rows if row['kind'] == 'generation']
    require(len(generation) == len(expected), 'Invalid evidence: len(generation) == len(expected)')
    require({(row['sample'], row['depth']) for row in generation} == expected, "Invalid evidence: {(row['sample'], row['depth']) for row in generation} == expected")
    for row in generation:
        require(isinstance(row.get("elapsed"), dict), "Missing generation elapsed measurement")
    require(all(row['records'] == 9 for row in generation), "Invalid evidence: all(row['records'] == 9 for row in generation)")
    ends = [row for row in rows if row['kind'] == 'rogue_end']
    cases = set(itertools.product(range(meta['samples']), ('demand', 'prepared', 'racing'), (False, True)))
    key = lambda row: (row['sample'], row['mode'], row['durable'])
    require(len(ends) == len(cases) and {key(row) for row in ends} == cases, 'Invalid evidence: len(ends) == len(cases) and {key(row) for row in ends} == cases')
    acquisitions = [row for row in rows if row['kind'] == 'acquisition']
    require(all(key(row) in cases for row in acquisitions), 'Invalid evidence: all(key(row) in cases for row in acquisitions)')
    for end in ends:
        require(isinstance(end.get("barrier"), dict), "Missing durable barrier measurement")
        samples = [row for row in acquisitions if key(row) == key(end)]
        require([row['step'] for row in samples] == list(range(end['steps'])), "Invalid evidence: [row['step'] for row in samples] == list(range(end['steps']))")
        require(end['groups'] == 1, "Invalid evidence: end['groups'] == 1")
        require(sum(row['profile']['region_acquisition']['groups_committed'] for row in samples) == 1, "Invalid evidence: sum(row['profile']['region_acquisition']['groups_committed'] for row in samples) == 1")
        for row in samples:
            validate_region_acquisition(row['profile'], required=True)
            counts = row['residency']
            nonnegative_counts(counts, ("active", "frozen"))
            require(counts['active'] + counts['frozen'] < 18, "Invalid evidence: counts['active'] + counts['frozen'] < 18")


def validate_stream(rows, case):
    """A streaming run walks the same number of steps east then west per
    cycle, with any AI turns in between, and bounded transition work per
    command."""
    meta = [r for r in rows if r["kind"] == "stream" and r["case"] == case]
    ends = [r for r in rows if r["kind"] == "stream_end" and r["case"] == case]
    require(len(meta) == 1 and len(ends) == 1, "Missing or duplicate streaming metadata or completion")
    meta, end = meta[0], ends[0]
    require(meta["workload"] == "streaming-v1", 'Invalid evidence: meta["workload"] == "streaming-v1"')
    require(meta.get('region_acquisition_version', 1) == 1, 'Unknown acquisition profile version')
    samples = [r for r in rows if r["kind"] == "sample"]
    require(all(s["case"] == case for s in samples), "Unexpected sample case")
    walked = [s["label"] for s in samples if s["actor"] == 1]
    leg = meta["steps_per_cycle"] // 2
    expected = (["walk_east"]*leg + ["walk_west"]*leg) * meta["cycles"]
    require(walked == expected, (case, "walk coverage"))
    require(all(s["label"] == "ai_turn" for s in samples if s["actor"] != 1), (case, "unexpected actor"))
    history = 0
    for sample in samples:
        require(sample["history_start"] == history, (case, "history"))
        history += 1
        profile = sample["profile"]
        validate_region_acquisition(profile, 'region_acquisition_version' in meta)
        require(profile["simulation_transitions"] == 1, 'Invalid evidence: profile["simulation_transitions"] == 1')
        require(profile["candidate_captures"] == 1 and profile["rollback_snapshots"] == 1, 'Invalid evidence: profile["candidate_captures"] == 1 and profile["rollback_snapshots"] == 1')
        bounds = STREAM_BOUNDS | (PRELOAD_BOUNDS if meta.get("preloading") else {})
        for name, bound in bounds.items():
            require(0 <= profile[name] <= bound, (case, name, profile[name]))
        require(sample["history_end"] == history and sample["rewind_count"] <= 128, 'Invalid evidence: sample["history_end"] == history and sample["rewind_count"] <= 128')
        if meta["storage"] == "background_sqlite_journal":
            status = sample["save_status"]
            require(status["accepted_sequence"] == history and status["error"] is None, 'Invalid evidence: status["accepted_sequence"] == history and status["error"] is None')
    require(any(s["profile"]["region_changes"] for s in samples), (case, "nothing streamed"))
    if meta.get("preloading"):
        require(any(s["profile"]["preload_jobs"] for s in samples), (case, "nothing preloaded"))
    require(end["history_end"] == history, 'Invalid evidence: end["history_end"] == history')
    require(end["recovery"]["records_loaded"] == history, 'Invalid evidence: end["recovery"]["records_loaded"] == history')
    if meta["storage"] == "background_sqlite_journal":
        status = end["save_status"]
        require(status["accepted_sequence"] == status["durable_sequence"] == history, 'Invalid evidence: status["accepted_sequence"] == status["durable_sequence"] == history')
        require(status["pending_bytes"] == 0 and status["error"] is None, 'Invalid evidence: status["pending_bytes"] == 0 and status["error"] is None')
    return {case: meta}, {case: samples}, {case: end}


def validate_wire_profiles(rows):
    """Version 2 measures complete envelopes and the shared encode/decode path.

    Historical unversioned sizes cover state/update DTOs only. Keep those raw
    diagnostics readable without interpreting them as complete-message results.
    """
    metadata = {}
    samples, summaries = defaultdict(list), defaultdict(list)
    for row in rows:
        kind, case = row.get("kind"), row.get("case")
        if kind in ("case", "traversal", "stream"):
            require(case not in metadata, "Duplicate wire case metadata")
            version = row.get("wire_profile_version", 1)
            require(type(version) is int and version in (1, 2), "Unknown wire profile version")
            metadata[case] = version
        elif kind == "sample":
            samples[case].append(row)
        elif kind == "wire":
            summaries[case].append(row)
    require(set(samples) | set(summaries) <= set(metadata), "Wire measurements lack case metadata")
    for case, version in metadata.items():
        observations = []
        for sample in samples[case]:
            phases = sample.get("phases_ms", {})
            wire = sample.get("observation_wire")
            if version == 1:
                require(wire is None and not ({"wire_encoding", "wire_decoding"} & phases.keys()), "Mixed wire versions")
                continue
            require("observation_wire" in sample, "Missing observation measurement field")
            require("delta_encoding" not in phases, "Construction-only timing in complete wire profile")
            if wire is None:
                require(not ({"wire_encoding", "wire_decoding"} & phases.keys()), "Missing observation sizes")
                continue
            require(isinstance(wire, dict) and type(wire.get("version")) is int and wire["version"] == 2, 'Invalid evidence: isinstance(wire, dict) and type(wire.get("version")) is int and wire["version"] == 2')
            full, sent, delta = wire.get("full_bytes"), wire.get("sent_bytes"), wire.get("delta")
            require(type(full) is int and type(sent) is int and 0 < sent <= full, 'Invalid evidence: type(full) is int and type(sent) is int and 0 < sent <= full')
            require(sent <= 16 * 1024 * 1024, "Selected envelope exceeds response ceiling")
            require(type(delta) is bool and (sent < full if delta else sent == full), 'Invalid evidence: type(delta) is bool and (sent < full if delta else sent == full)')
            for name in ("wire_encoding", "wire_decoding"):
                value = phases.get(name)
                require(type(value) in (int, float) and math.isfinite(value) and value >= 0, name)
            observations.append((full, sent, delta))
        if version == 1:
            require(all(r.get("wire_profile_version", 1) == 1 for r in summaries[case]), "Mixed wire summaries")
            continue
        require(len(summaries[case]) == bool(observations), "Missing or duplicate wire summary")
        if not observations:
            continue
        summary = summaries[case][0]
        require(type(summary.get("wire_profile_version")) is int and summary["wire_profile_version"] == 2, 'Invalid evidence: type(summary.get("wire_profile_version")) is int and summary["wire_profile_version"] == 2')
        sent_sizes = sorted(item[1] for item in observations)
        expected = dict(n=len(observations), deltas=sum(item[2] for item in observations),
                        full_bytes_total=sum(item[0] for item in observations),
                        sent_bytes_total=sum(sent_sizes),
                        sent_p50_bytes=sent_sizes[math.ceil(len(sent_sizes)*0.50)-1],
                        sent_p95_bytes=sent_sizes[math.ceil(len(sent_sizes)*0.95)-1],
                        sent_max_bytes=sent_sizes[-1])
        for name, value in expected.items():
            require(type(summary.get(name)) is int and summary[name] == value, (case, name))


def validate(rows, quick=False, phase_b=False, phase_c=False, selected_case=None, phase_d=False, discovery_only=False, saved_discovery=False):
    validate_measurements(rows)
    validate_wire_profiles(rows)
    if selected_case in STREAM_CASES:
        return validate_stream(rows, selected_case)
    discovery_only = discovery_only or saved_discovery
    cases, samples, ends = {}, defaultdict(list), {}
    traversals, traversal_ends = {}, {}
    tables = {"case": cases, "case_end": ends, "traversal": traversals, "traversal_end": traversal_ends}
    for row in rows:
        require(row["kind"] != "failure", "Benchmark recorded a failed command")
        if row["kind"] in tables:
            table = tables[row["kind"]]
            require(row["case"] not in table, "Duplicate case metadata or completion")
            table[row["case"]] = row
        elif row["kind"] == "sample": samples[row["case"]].append(row)
    matrix = itertools.product([1,8] if quick else [1,8,64,256],[1,8],[0,100] if quick else [0,100,1000,10000],["memory","durable"])
    expected_cases = {f"r{r}-a{a}-h{h}-{storage}" for r,a,h,storage in matrix}
    if phase_b or phase_c or phase_d:
        expected_cases = {f"r8-a{a}-h{h}-{s}" for a,h,s in itertools.product([1,8],[100,10000],["memory","durable"])}
        expected_cases.add("r256-a8-h10000-durable")
    if phase_c:
        expected_cases = {case for case in expected_cases if case.endswith("-durable")}
    if selected_case is not None:
        require(selected_case in expected_cases, "Unknown selected case")
        expected_cases = {selected_case}
    if discovery_only:
        expected_cases = set()
    require(set(cases) == set(ends) == expected_cases, "Missing or unexpected completed matrix case")
    for case, meta in cases.items():
        expected = expected_actions(meta["regions"],meta["actors"],meta["history_start"],meta["cycles"])
        actual = samples[case]
        require(len(expected) == len(actual), (case,"sample count",len(expected),len(actual)))
        history = meta["history_start"]
        last_bytes = 0
        for (actor,label,action,outcome),sample in zip(expected,actual):
            require((sample["actor"],sample["label"],sample["expected"]) == (actor,label,outcome), (case,label))
            resolved = sample["action"]
            if action["type"] == "door": require(resolved["type"] == "set_door" and resolved["open"] == action["open"], 'Invalid evidence: resolved["type"] == "set_door" and resolved["open"] == action["open"]')
            else: require(resolved == action, 'Invalid evidence: resolved == action')
            require(sample["history_start"] == history, 'Invalid evidence: sample["history_start"] == history')
            if outcome != "blocked":
                history += 1
                profile = sample["profile"]
                require(profile["simulation_transitions"] == 1, 'Invalid evidence: profile["simulation_transitions"] == 1')
                require(profile["candidate_captures"] <= 1 and profile["rollback_snapshots"] <= 1, 'Invalid evidence: profile["candidate_captures"] <= 1 and profile["rollback_snapshots"] <= 1')
                require(profile["actors_observed"] <= 2*meta["actors"], 'Invalid evidence: profile["actors_observed"] <= 2*meta["actors"]')
                require(profile["revision_comparisons"] <= meta["actors"], 'Invalid evidence: profile["revision_comparisons"] <= meta["actors"]')
                if meta.get("profile_version", 1) >= 2:
                    validate_work(profile, resolved, meta["actors"])
                require(profile["records_serialized"] <= history, 'Invalid evidence: profile["records_serialized"] <= history')
                if meta["storage"] == "background_sqlite_journal":
                    require(profile["records_serialized"] == 1, 'Invalid evidence: profile["records_serialized"] == 1')
                    require(all(profile[k] == 0 for k in ("bytes_written","file_writes","file_flushes","file_syncs","file_replacements")), 'Invalid evidence: all(profile[k] == 0 for k in ("bytes_written","file_writes","file_flushes","file_syncs","file_replacements"))')
                    status = sample["save_status"]
                    require(status["accepted_sequence"] == history, 'Invalid evidence: status["accepted_sequence"] == history')
                    require(status["durable_sequence"] <= history and status["error"] is None, 'Invalid evidence: status["durable_sequence"] <= history and status["error"] is None')
                    require(status["pending_bytes"] <= meta["save_policy"]["queue_bytes"], 'Invalid evidence: status["pending_bytes"] <= meta["save_policy"]["queue_bytes"]')
                elif meta["storage"] != "memory":
                    require(profile["file_syncs"] >= 1 and profile["bytes_written"] > 0, 'Invalid evidence: profile["file_syncs"] >= 1 and profile["bytes_written"] > 0')
                    last_bytes = profile["bytes_written"]
            require(sample["history_end"] == history and sample["rewind_count"] <= 128, 'Invalid evidence: sample["history_end"] == history and sample["rewind_count"] <= 128')
        require(ends[case]["history_end"] == history, 'Invalid evidence: ends[case]["history_end"] == history')
        if last_bytes: require(last_bytes == ends[case]["final_save_bytes"], 'Invalid evidence: last_bytes == ends[case]["final_save_bytes"]')
        if meta["storage"] == "background_sqlite_journal":
            status = ends[case]["save_status"]
            require(status["accepted_sequence"] == status["durable_sequence"] == history, 'Invalid evidence: status["accepted_sequence"] == status["durable_sequence"] == history')
            require(status["pending_bytes"] == 0 and status["error"] is None, 'Invalid evidence: status["pending_bytes"] == 0 and status["error"] is None')
            require(status["batches"] > 0 and status["journal_bytes"] > 0, 'Invalid evidence: status["batches"] > 0 and status["journal_bytes"] > 0')
            final = actual[-1]["save_status"]
            require(status["journal_bytes"] == final["journal_bytes"] + final["pending_bytes"], 'Invalid evidence: status["journal_bytes"] == final["journal_bytes"] + final["pending_bytes"]')
        if phase_c or phase_d:
            recovery = ends[case]["recovery"]
            require(recovery["records_loaded"] == history, 'Invalid evidence: recovery["records_loaded"] == history')
            require(recovery["records_replayed"] == history - recovery["checkpoint_sequence"], 'Invalid evidence: recovery["records_replayed"] == history - recovery["checkpoint_sequence"]')
            captured = [sample["history_end"] for sample in actual
                        if sample.get("profile") and sample["profile"]["checkpoint_captures"]]
            require(recovery["checkpoint_sequence"] == (captured[-1] if captured else 0), 'Invalid evidence: recovery["checkpoint_sequence"] == (captured[-1] if captured else 0)')
            interval = meta["save_policy"]["checkpoint_interval"]
            if interval and captured:
                require(0 < recovery["checkpoint_sequence"] <= history, 'Invalid evidence: 0 < recovery["checkpoint_sequence"] <= history')
                require(recovery["records_replayed"] < interval, 'Invalid evidence: recovery["records_replayed"] < interval')
                require(status["checkpoint_sequence"] == recovery["checkpoint_sequence"], 'Invalid evidence: status["checkpoint_sequence"] == recovery["checkpoint_sequence"]')
                require(status["checkpoint_bytes"] <= 64*1024*1024, 'Invalid evidence: status["checkpoint_bytes"] <= 64*1024*1024')
            else:
                require(recovery["checkpoint_sequence"] == 0, 'Invalid evidence: recovery["checkpoint_sequence"] == 0')
                require(recovery["records_replayed"] == history, 'Invalid evidence: recovery["records_replayed"] == history')
                if interval and meta["storage"] == "background_sqlite_journal":
                    require(history < interval, 'Invalid evidence: history < interval')
    discovery_regions = (8,256) if phase_d or discovery_only else (8,64,256)
    required_discovery = () if (phase_b or phase_c or selected_case) and not discovery_only else discovery_regions
    expected_traversals = {f"traversal-r{r}" for r in required_discovery}
    require(set(samples) == expected_cases | expected_traversals, "Unexpected sample case")
    require(set(traversal_ends) == expected_traversals, "Missing or unexpected discovery completion")
    if traversals:
        require(set(traversals) == expected_traversals, "Missing or unexpected discovery metadata")
    for regions in required_discovery:
        case = f"traversal-r{regions}"
        actual = samples[case]
        cycles = 2 if quick else regions-1
        require(len(actual) == cycles*len(SPEC["traversal"]), (case,"discovery coverage"))
        require(sum(s["label"] == "cross_region_boundary" for s in actual) == cycles, 'Invalid evidence: sum(s["label"] == "cross_region_boundary" for s in actual) == cycles')
        require(actual[-1]["client_memory"] > actual[0]["client_memory"], (case,"memory must grow"))
        require(actual[-1]["history_end"] == len(actual), 'Invalid evidence: actual[-1]["history_end"] == len(actual)')
        expected = SPEC["traversal"] * cycles
        metadata = traversals.get(case, {})
        end = traversal_ends[case]
        require(end["history_end"] == len(actual) and end["client_memory"] == actual[-1]["client_memory"], 'Invalid evidence: end["history_end"] == len(actual) and end["client_memory"] == actual[-1]["client_memory"]')
        if metadata:
            require(metadata["cycles"] == cycles and metadata["regions"] == regions and metadata["trace_version"] == SPEC["version"], 'Invalid evidence: metadata["cycles"] == cycles and metadata["regions"] == regions and metadata["trace_version"] == SPEC["version"]')
        if saved_discovery:
            require(metadata["storage"] == "background_sqlite_journal", 'Invalid evidence: metadata["storage"] == "background_sqlite_journal"')
            validate_saved_discovery(metadata, actual, end)
        for index, (sample, step) in enumerate(zip(actual, expected)):
            require(sample["actor"] == 1, 'Invalid evidence: sample["actor"] == 1')
            if step["action"]["type"] == "door":
                require(sample["action"]["type"] == "set_door" and sample["action"]["open"] == step["action"]["open"], 'Invalid evidence: sample["action"]["type"] == "set_door" and sample["action"]["open"] == step["action"]["open"]')
            else:
                require(sample["action"] == step["action"], 'Invalid evidence: sample["action"] == step["action"]')
            require(sample["label"] == step["label"] and sample["expected"] == step["expected"], 'Invalid evidence: sample["label"] == step["label"] and sample["expected"] == step["expected"]')
            require(sample["history_start"] == index and sample["history_end"] == index + 1, 'Invalid evidence: sample["history_start"] == index and sample["history_end"] == index + 1')
            if metadata.get("profile_version", 1) >= 2:
                validate_work(sample["profile"], sample["action"], 1)
    return cases, samples, ends


def validate_saved_discovery(meta, samples, end):
    """A saved traversal must commit its entire prefix and reload the same boundary."""
    nonnegative_counts(meta, ("checkpoint_interval",))
    persistence = end["persistence"]
    positive_counts(persistence, ("final_save_bytes", "checkpoint_json_bytes"))
    status, recovery = persistence["save_status"], persistence["recovery"]
    nonnegative_counts(status, ("pending_bytes", "accepted_sequence", "durable_sequence", "checkpoint_sequence", "checkpoint_bytes"))
    nonnegative_counts(recovery, ("records_loaded", "records_replayed", "checkpoint_sequence"))
    count = len(samples)
    require(status["error"] is None and status["pending_bytes"] == 0, 'Invalid evidence: status["error"] is None and status["pending_bytes"] == 0')
    require(status["accepted_sequence"] == status["durable_sequence"] == count, 'Invalid evidence: status["accepted_sequence"] == status["durable_sequence"] == count')
    require(recovery["records_loaded"] == count, 'Invalid evidence: recovery["records_loaded"] == count')
    captured = [s["history_end"] for s in samples if s["profile"]["checkpoint_captures"]]
    sequence = captured[-1] if captured else 0
    require(recovery["checkpoint_sequence"] == status["checkpoint_sequence"] == sequence, 'Invalid evidence: recovery["checkpoint_sequence"] == status["checkpoint_sequence"] == sequence')
    require(recovery["records_replayed"] == count - sequence, 'Invalid evidence: recovery["records_replayed"] == count - sequence')
    interval = meta["checkpoint_interval"]
    require(sequence == ((count // interval) * interval if interval else 0), 'Invalid evidence: sequence == ((count // interval) * interval if interval else 0)')
    if sequence:
        require(0 < status["checkpoint_bytes"] <= 64*1024*1024, 'Invalid evidence: 0 < status["checkpoint_bytes"] <= 64*1024*1024')
    else:
        require(status["checkpoint_bytes"] == 0, 'Invalid evidence: status["checkpoint_bytes"] == 0')
    require(persistence["final_save_bytes"] > 0, 'Invalid evidence: persistence["final_save_bytes"] > 0')
    require(type(persistence["checkpoint_json_bytes"]) is int and persistence["checkpoint_json_bytes"] > 0, 'Invalid evidence: type(persistence["checkpoint_json_bytes"]) is int and persistence["checkpoint_json_bytes"] > 0')
    for key in ("checkpoint_diagnostic_ms", "final_flush_ms", "restart_replay_ms"):
        value = persistence[key]
        require(type(value) in (int, float) and math.isfinite(value) and value >= 0, 'Invalid evidence: type(value) in (int, float) and math.isfinite(value) and value >= 0')
    for sample in samples:
        current = sample["save_status"]
        require(current["error"] is None, 'Invalid evidence: current["error"] is None')
        require(current["accepted_sequence"] == sample["history_end"], 'Invalid evidence: current["accepted_sequence"] == sample["history_end"]')
        require(current["durable_sequence"] <= current["accepted_sequence"], 'Invalid evidence: current["durable_sequence"] <= current["accepted_sequence"]')
        require(sample["profile"]["records_serialized"] == 1, 'Invalid evidence: sample["profile"]["records_serialized"] == 1')


def validate_work(profile, action, actors):
    """Version 2 contracts describe actual calls, separate from fixture version 1."""
    validate_profile_measurements(profile)
    nonnegative_counts(profile, ("actors_observed", "scene_calls", "perception_calls", "candidate_captures", "rollback_snapshots"))
    for field, work in (("scene_calls", "fresh scene computations"),
                        ("perception_calls", "fresh observation disclosures")):
        require(type(profile[field]) is int and profile[field] >= 0, f"{work} must be a nonnegative integer count")
    if action["type"] == "wait":
        require((profile["actors_observed"], profile["perception_calls"], profile["scene_calls"], profile["navigation_refreshes"]) == (0,0,0,0), 'Invalid evidence: (profile["actors_observed"], profile["perception_calls"], profile["scene_calls"], profile["navigation_refreshes"]) == (0,0,0,0)')
        require(profile["revision_comparisons"] == actors, 'Invalid evidence: profile["revision_comparisons"] == actors')
    else:
        require(profile["actors_observed"] == 2*actors, 'Invalid evidence: profile["actors_observed"] == 2*actors')
        limit = 2*actors + (action["type"] == "set_door")
        require(profile["scene_calls"] <= limit, "repeated fresh scene computations")
        require(actors <= profile["perception_calls"] <= limit, "each actor needs fresh post-action disclosure within the work bound")
    require(profile["candidate_captures"] == 1 and profile["rollback_snapshots"] == 1, 'Invalid evidence: profile["candidate_captures"] == 1 and profile["rollback_snapshots"] == 1')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input",type=Path)
    parser.add_argument("--quick",action="store_true")
    parser.add_argument("--phase-b",action="store_true")
    parser.add_argument("--phase-c",action="store_true")
    parser.add_argument("--phase-d",action="store_true")
    parser.add_argument("--discovery-only",action="store_true")
    parser.add_argument("--saved-discovery",action="store_true")
    parser.add_argument("--case")
    parser.add_argument("--summary",type=Path)
    args = parser.parse_args()
    opener = gzip.open if args.input.suffix == ".gz" else open
    with opener(args.input,"rt",encoding="utf-8-sig") as stream:
        rows = [json.loads(line) for line in stream if line.strip()]
    cases,samples,ends = validate(rows,args.quick,args.phase_b,args.phase_c,args.case,args.phase_d,args.discovery_only,args.saved_discovery)
    summaries = [r for r in rows if r["kind"] != "sample"]
    if args.summary:
        args.summary.write_text("\n".join(json.dumps(r) for r in summaries)+"\n",encoding="utf-8")
    print(f"Verified {len(cases)} complete cases, {sum(len(values) for values in samples.values())} ordered samples, history and byte accounting.")
    for row in rows:
        if row["kind"] == "summary" and row["label"] == "mixed" and row["phase"] == "authoritative_total":
            print(f"{row['case']}: n={row['n']} mean={row['mean_ms']:.3f} p50={row['p50_ms']:.3f} p95={row['p95_ms']:.3f} max={row['max_ms']:.3f} ms")


if __name__ == "__main__":
    main()
