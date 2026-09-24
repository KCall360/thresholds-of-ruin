"""Validate exact diagnostic coverage and summarize retained JSONL samples."""
import argparse
from collections import defaultdict
import gzip
import itertools
import json
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


def validate(rows, quick=False):
    cases, samples, ends = {}, defaultdict(list), {}
    for row in rows:
        if row["kind"] == "case": cases[row["case"]] = row
        elif row["kind"] == "sample": samples[row["case"]].append(row)
        elif row["kind"] == "case_end": ends[row["case"]] = row
    matrix = itertools.product([1,8] if quick else [1,8,64,256],[1,8],[0,100] if quick else [0,100,1000,10000],["memory","durable"])
    expected_cases = {f"r{r}-a{a}-h{h}-{storage}" for r,a,h,storage in matrix}
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
                assert profile["records_serialized"] <= history
                if meta["storage"] != "memory":
                    assert profile["file_syncs"] >= 1 and profile["bytes_written"] > 0
                    last_bytes = profile["bytes_written"]
            assert sample["history_end"] == history and sample["rewind_count"] <= 128
        assert ends[case]["history_end"] == history
        if last_bytes: assert last_bytes == ends[case]["final_save_bytes"]
    for regions in (8,64,256):
        case = f"traversal-r{regions}"
        actual = samples[case]
        cycles = 2 if quick else regions-1
        assert len(actual) == cycles*len(SPEC["traversal"]), (case,"discovery coverage")
        assert sum(s["label"] == "cross_region_boundary" for s in actual) == cycles
        assert actual[-1]["client_memory"] > actual[0]["client_memory"], (case,"memory must grow")
        assert actual[-1]["history_end"] == len(actual)
    return cases, samples, ends


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input",type=Path)
    parser.add_argument("--quick",action="store_true")
    parser.add_argument("--summary",type=Path)
    args = parser.parse_args()
    opener = gzip.open if args.input.suffix == ".gz" else open
    with opener(args.input,"rt",encoding="utf-8-sig") as stream:
        rows = [json.loads(line) for line in stream if line.strip()]
    cases,samples,ends = validate(rows,args.quick)
    summaries = [r for r in rows if r["kind"] != "sample"]
    if args.summary:
        args.summary.write_text("\n".join(json.dumps(r) for r in summaries)+"\n",encoding="utf-8")
    print(f"Verified {len(cases)} complete cases, {sum(len(samples[c]) for c in cases)} ordered samples, history and byte accounting.")
    for row in rows:
        if row["kind"] == "summary" and row["label"] == "mixed" and row["phase"] == "authoritative_total":
            print(f"{row['case']}: n={row['n']} mean={row['mean_ms']:.3f} p50={row['p50_ms']:.3f} p95={row['p95_ms']:.3f} max={row['max_ms']:.3f} ms")


if __name__ == "__main__":
    main()
