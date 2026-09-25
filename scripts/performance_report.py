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


def validate(rows, quick=False, phase_b=False, phase_c=False, selected_case=None, phase_d=False, discovery_only=False, saved_discovery=False):
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
