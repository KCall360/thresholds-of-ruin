"""Validate exact diagnostic coverage and summarize retained JSONL samples."""
import argparse
from collections import defaultdict
import gzip
import itertools
import json
import math
from pathlib import Path

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


def validate_region_acquisition(profile, required=False):
    """Nested acquisition metrics partition successful reads/builds only."""
    acquisition = profile.get('region_acquisition')
    if acquisition is None:
        assert not required, 'Missing region acquisition profile'
        return
    assert isinstance(acquisition, dict)
    for name in ('fallback_reads', 'fallback_builds', 'prepared_reads', 'prepared_builds'):
        assert type(acquisition.get(name)) is int and acquisition[name] >= 0, name
    assert acquisition['fallback_reads'] + acquisition['prepared_reads'] == profile['region_records_read']
    assert acquisition['fallback_builds'] + acquisition['prepared_builds'] == profile['regions_built']
    assert acquisition['prepared_reads'] + acquisition['prepared_builds'] == profile['regions_prepared']

    def nanos(value):
        assert isinstance(value, dict)
        assert type(value.get('secs')) is int and value['secs'] >= 0
        assert type(value.get('nanos')) is int and 0 <= value['nanos'] < 1000000000
        return value['secs']*1000000000 + value['nanos']

    read = nanos(acquisition.get('fallback_read'))
    build = nanos(acquisition.get('fallback_build'))
    assert read + build <= nanos(profile.get('region_transition')), 'Nested timings exceed transition'
    assert acquisition['fallback_reads'] or read == 0
    assert acquisition['fallback_builds'] or build == 0


def validate_stream(rows, case):
    """A streaming run walks the same number of steps east then west per
    cycle, with any AI turns in between, and bounded transition work per
    command."""
    meta = [r for r in rows if r["kind"] == "stream" and r["case"] == case]
    ends = [r for r in rows if r["kind"] == "stream_end" and r["case"] == case]
    assert len(meta) == 1 and len(ends) == 1, "Missing or duplicate streaming metadata or completion"
    meta, end = meta[0], ends[0]
    assert meta["workload"] == "streaming-v1"
    assert meta.get('region_acquisition_version', 1) == 1, 'Unknown acquisition profile version'
    samples = [r for r in rows if r["kind"] == "sample"]
    assert all(s["case"] == case for s in samples), "Unexpected sample case"
    walked = [s["label"] for s in samples if s["actor"] == 1]
    leg = meta["steps_per_cycle"] // 2
    expected = (["walk_east"]*leg + ["walk_west"]*leg) * meta["cycles"]
    assert walked == expected, (case, "walk coverage")
    assert all(s["label"] == "ai_turn" for s in samples if s["actor"] != 1), (case, "unexpected actor")
    history = 0
    for sample in samples:
        assert sample["history_start"] == history, (case, "history")
        history += 1
        profile = sample["profile"]
        validate_region_acquisition(profile, 'region_acquisition_version' in meta)
        assert profile["simulation_transitions"] == 1
        assert profile["candidate_captures"] == 1 and profile["rollback_snapshots"] == 1
        bounds = STREAM_BOUNDS | (PRELOAD_BOUNDS if meta.get("preloading") else {})
        for name, bound in bounds.items():
            assert 0 <= profile[name] <= bound, (case, name, profile[name])
        assert sample["history_end"] == history and sample["rewind_count"] <= 128
        if meta["storage"] == "background_sqlite_journal":
            status = sample["save_status"]
            assert status["accepted_sequence"] == history and status["error"] is None
    assert any(s["profile"]["region_changes"] for s in samples), (case, "nothing streamed")
    if meta.get("preloading"):
        assert any(s["profile"]["preload_jobs"] for s in samples), (case, "nothing preloaded")
    assert end["history_end"] == history
    assert end["recovery"]["records_loaded"] == history
    if meta["storage"] == "background_sqlite_journal":
        status = end["save_status"]
        assert status["accepted_sequence"] == status["durable_sequence"] == history
        assert status["pending_bytes"] == 0 and status["error"] is None
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
            assert case not in metadata, "Duplicate wire case metadata"
            version = row.get("wire_profile_version", 1)
            assert type(version) is int and version in (1, 2), "Unknown wire profile version"
            metadata[case] = version
        elif kind == "sample":
            samples[case].append(row)
        elif kind == "wire":
            summaries[case].append(row)
    assert set(samples) | set(summaries) <= set(metadata), "Wire measurements lack case metadata"
    for case, version in metadata.items():
        observations = []
        for sample in samples[case]:
            phases = sample.get("phases_ms", {})
            wire = sample.get("observation_wire")
            if version == 1:
                assert wire is None and not ({"wire_encoding", "wire_decoding"} & phases.keys()), "Mixed wire versions"
                continue
            assert "observation_wire" in sample, "Missing observation measurement field"
            assert "delta_encoding" not in phases, "Construction-only timing in complete wire profile"
            if wire is None:
                assert not ({"wire_encoding", "wire_decoding"} & phases.keys()), "Missing observation sizes"
                continue
            assert isinstance(wire, dict) and type(wire.get("version")) is int and wire["version"] == 2
            full, sent, delta = wire.get("full_bytes"), wire.get("sent_bytes"), wire.get("delta")
            assert type(full) is int and type(sent) is int and 0 < sent <= full
            assert sent <= 16 * 1024 * 1024, "Selected envelope exceeds response ceiling"
            assert type(delta) is bool and (sent < full if delta else sent == full)
            for name in ("wire_encoding", "wire_decoding"):
                value = phases.get(name)
                assert type(value) in (int, float) and math.isfinite(value) and value >= 0, name
            observations.append((full, sent, delta))
        if version == 1:
            assert all(r.get("wire_profile_version", 1) == 1 for r in summaries[case]), "Mixed wire summaries"
            continue
        assert len(summaries[case]) == bool(observations), "Missing or duplicate wire summary"
        if not observations:
            continue
        summary = summaries[case][0]
        assert type(summary.get("wire_profile_version")) is int and summary["wire_profile_version"] == 2
        sent_sizes = sorted(item[1] for item in observations)
        expected = dict(n=len(observations), deltas=sum(item[2] for item in observations),
                        full_bytes_total=sum(item[0] for item in observations),
                        sent_bytes_total=sum(sent_sizes),
                        sent_p50_bytes=sent_sizes[math.ceil(len(sent_sizes)*0.50)-1],
                        sent_p95_bytes=sent_sizes[math.ceil(len(sent_sizes)*0.95)-1],
                        sent_max_bytes=sent_sizes[-1])
        for name, value in expected.items():
            assert type(summary.get(name)) is int and summary[name] == value, (case, name)


def validate(rows, quick=False, phase_b=False, phase_c=False, selected_case=None, phase_d=False, discovery_only=False, saved_discovery=False):
    validate_wire_profiles(rows)
    if selected_case in STREAM_CASES:
        return validate_stream(rows, selected_case)
    discovery_only = discovery_only or saved_discovery
    cases, samples, ends = {}, defaultdict(list), {}
    traversals, traversal_ends = {}, {}
    tables = {"case": cases, "case_end": ends, "traversal": traversals, "traversal_end": traversal_ends}
    for row in rows:
        assert row["kind"] != "failure", "Benchmark recorded a failed command"
        if row["kind"] in tables:
            table = tables[row["kind"]]
            assert row["case"] not in table, "Duplicate case metadata or completion"
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
        assert selected_case in expected_cases, "Unknown selected case"
        expected_cases = {selected_case}
    if discovery_only:
        expected_cases = set()
    assert set(cases) == set(ends) == expected_cases, "Missing or unexpected completed matrix case"
    for case, meta in cases.items():
        expected = expected_actions(meta["regions"],meta["actors"],meta["history_start"],meta["cycles"])
        actual = samples[case]
        assert len(expected) == len(actual), (case,"sample count",len(expected),len(actual))
        history = meta["history_start"]
        last_bytes = 0
        for (actor,label,action,outcome),sample in zip(expected,actual):
            assert (sample["actor"],sample["label"],sample["expected"]) == (actor,label,outcome), (case,label)
            resolved = sample["action"]
            if action["type"] == "door": assert resolved["type"] == "set_door" and resolved["open"] == action["open"]
            else: assert resolved == action
            assert sample["history_start"] == history
            if outcome != "blocked":
                history += 1
                profile = sample["profile"]
                assert profile["simulation_transitions"] == 1
                assert profile["candidate_captures"] <= 1 and profile["rollback_snapshots"] <= 1
                assert profile["actors_observed"] <= 2*meta["actors"]
                assert profile["revision_comparisons"] <= meta["actors"]
                if meta.get("profile_version", 1) >= 2:
                    validate_work(profile, resolved, meta["actors"])
                assert profile["records_serialized"] <= history
                if meta["storage"] == "background_sqlite_journal":
                    assert profile["records_serialized"] == 1
                    assert all(profile[k] == 0 for k in ("bytes_written","file_writes","file_flushes","file_syncs","file_replacements"))
                    status = sample["save_status"]
                    assert status["accepted_sequence"] == history
                    assert status["durable_sequence"] <= history and status["error"] is None
                    assert status["pending_bytes"] <= meta["save_policy"]["queue_bytes"]
                elif meta["storage"] != "memory":
                    assert profile["file_syncs"] >= 1 and profile["bytes_written"] > 0
                    last_bytes = profile["bytes_written"]
            assert sample["history_end"] == history and sample["rewind_count"] <= 128
        assert ends[case]["history_end"] == history
        if last_bytes: assert last_bytes == ends[case]["final_save_bytes"]
        if meta["storage"] == "background_sqlite_journal":
            status = ends[case]["save_status"]
            assert status["accepted_sequence"] == status["durable_sequence"] == history
            assert status["pending_bytes"] == 0 and status["error"] is None
            assert status["batches"] > 0 and status["journal_bytes"] > 0
            final = actual[-1]["save_status"]
            assert status["journal_bytes"] == final["journal_bytes"] + final["pending_bytes"]
        if phase_c or phase_d:
            recovery = ends[case]["recovery"]
            assert recovery["records_loaded"] == history
            assert recovery["records_replayed"] == history - recovery["checkpoint_sequence"]
            captured = [sample["history_end"] for sample in actual
                        if sample.get("profile") and sample["profile"]["checkpoint_captures"]]
            assert recovery["checkpoint_sequence"] == (captured[-1] if captured else 0)
            interval = meta["save_policy"]["checkpoint_interval"]
            if interval and captured:
                assert 0 < recovery["checkpoint_sequence"] <= history
                assert recovery["records_replayed"] < interval
                assert status["checkpoint_sequence"] == recovery["checkpoint_sequence"]
                assert status["checkpoint_bytes"] <= 64*1024*1024
            else:
                assert recovery["checkpoint_sequence"] == 0
                assert recovery["records_replayed"] == history
                if interval and meta["storage"] == "background_sqlite_journal":
                    assert history < interval
    discovery_regions = (8,256) if phase_d or discovery_only else (8,64,256)
    required_discovery = () if (phase_b or phase_c or selected_case) and not discovery_only else discovery_regions
    expected_traversals = {f"traversal-r{r}" for r in required_discovery}
    assert set(samples) == expected_cases | expected_traversals, "Unexpected sample case"
    assert set(traversal_ends) == expected_traversals, "Missing or unexpected discovery completion"
    if traversals:
        assert set(traversals) == expected_traversals, "Missing or unexpected discovery metadata"
    for regions in required_discovery:
        case = f"traversal-r{regions}"
        actual = samples[case]
        cycles = 2 if quick else regions-1
        assert len(actual) == cycles*len(SPEC["traversal"]), (case,"discovery coverage")
        assert sum(s["label"] == "cross_region_boundary" for s in actual) == cycles
        assert actual[-1]["client_memory"] > actual[0]["client_memory"], (case,"memory must grow")
        assert actual[-1]["history_end"] == len(actual)
        expected = SPEC["traversal"] * cycles
        metadata = traversals.get(case, {})
        end = traversal_ends[case]
        assert end["history_end"] == len(actual) and end["client_memory"] == actual[-1]["client_memory"]
        if metadata:
            assert metadata["cycles"] == cycles and metadata["regions"] == regions and metadata["trace_version"] == SPEC["version"]
        if saved_discovery:
            assert metadata["storage"] == "background_sqlite_journal"
            validate_saved_discovery(metadata, actual, end)
        for index, (sample, step) in enumerate(zip(actual, expected)):
            assert sample["actor"] == 1
            if step["action"]["type"] == "door":
                assert sample["action"]["type"] == "set_door" and sample["action"]["open"] == step["action"]["open"]
            else:
                assert sample["action"] == step["action"]
            assert sample["label"] == step["label"] and sample["expected"] == step["expected"]
            assert sample["history_start"] == index and sample["history_end"] == index + 1
            if metadata.get("profile_version", 1) >= 2:
                validate_work(sample["profile"], sample["action"], 1)
    return cases, samples, ends


def validate_saved_discovery(meta, samples, end):
    """A saved traversal must commit its entire prefix and reload the same boundary."""
    persistence = end["persistence"]
    status, recovery = persistence["save_status"], persistence["recovery"]
    count = len(samples)
    assert status["error"] is None and status["pending_bytes"] == 0
    assert status["accepted_sequence"] == status["durable_sequence"] == count
    assert recovery["records_loaded"] == count
    captured = [s["history_end"] for s in samples if s["profile"]["checkpoint_captures"]]
    sequence = captured[-1] if captured else 0
    assert recovery["checkpoint_sequence"] == status["checkpoint_sequence"] == sequence
    assert recovery["records_replayed"] == count - sequence
    interval = meta["checkpoint_interval"]
    assert sequence == ((count // interval) * interval if interval else 0)
    if sequence:
        assert 0 < status["checkpoint_bytes"] <= 64*1024*1024
    else:
        assert status["checkpoint_bytes"] == 0
    assert persistence["final_save_bytes"] > 0
    assert type(persistence["checkpoint_json_bytes"]) is int and persistence["checkpoint_json_bytes"] > 0
    for key in ("checkpoint_diagnostic_ms", "final_flush_ms", "restart_replay_ms"):
        value = persistence[key]
        assert type(value) in (int, float) and math.isfinite(value) and value >= 0
    for sample in samples:
        current = sample["save_status"]
        assert current["error"] is None
        assert current["accepted_sequence"] == sample["history_end"]
        assert current["durable_sequence"] <= current["accepted_sequence"]
        assert sample["profile"]["records_serialized"] == 1


def validate_work(profile, action, actors):
    """Version 2 contracts describe actual calls, separate from fixture version 1."""
    if action["type"] == "wait":
        assert (profile["actors_observed"], profile["perception_calls"], profile["scene_calls"], profile["navigation_refreshes"]) == (0,0,0,0)
        assert profile["revision_comparisons"] == actors
    else:
        assert profile["actors_observed"] == 2*actors
        assert profile["scene_calls"] == profile["perception_calls"]
        assert profile["scene_calls"] <= 2*actors + (action["type"] == "set_door")
    assert profile["candidate_captures"] == 1 and profile["rollback_snapshots"] == 1


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
