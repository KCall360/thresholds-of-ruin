"""Validate versioned place workload coverage and summarize exclusive boundaries."""
import json
import math
from pathlib import Path
import sys
from workload_report import is_duration, require, nonnegative_counts, positive_counts

SPEC = json.loads((Path(__file__).resolve().parents[1] / "crates/server/fixtures/place-knowledge-v1.json").read_text())
OUTCOME_METRICS = ("outcome_encode_ms", "outcome_apply_ms")
METRICS = ("command_ms", "observation_ms", "encode_ms", "apply_ms", "render_ms")


def validate(rows, *, selected_rooms=None):
    if selected_rooms is not None and (type(selected_rooms) is not int or selected_rooms not in SPEC["extra_rooms"]):
        raise ValueError("Unknown selected scale")
    scales = SPEC["extra_rooms"] if selected_rooms is None else [selected_rooms]
    for row in rows:
        require(isinstance(row, dict), "Place rows must be objects")
        positive_counts(row, ("version", "places"))
        nonnegative_counts(row, ("extra_rooms",))
        if row.get("kind") == "sample":
            nonnegative_counts(row, ("sample", "navigation_refreshes"))
            positive_counts(row, ("records", "wire_bytes"))
            if any(field in row for field in (*OUTCOME_METRICS, "outcome_wire_bytes")):
                positive_counts(row, ("outcome_wire_bytes",))
                require(all(is_duration(row.get(field)) for field in OUTCOME_METRICS),
                        "Causal delivery measurements must be complete durations")
        elif row.get("kind") == "discovery":
            nonnegative_counts(row, ("room",))
        elif row.get("kind") == "recovery":
            positive_counts(row, ("checkpoint_bytes",))
            require(row.get("exact") is True, "Recovery must be exact")
    if any(r.get("version") != SPEC["version"] or r.get("kind") not in ("discovery", "sample", "recovery")
           or r.get("extra_rooms") not in scales for r in rows):
        raise ValueError("Unknown workload record")
    summary = []
    for rooms in scales:
        case = [r for r in rows if r["extra_rooms"] == rooms]
        discoveries = [r for r in case if r["kind"] == "discovery"]
        samples = [r for r in case if r["kind"] == "sample"]
        recovery = [r for r in case if r["kind"] == "recovery"]
        count = 2 + rooms * len(SPEC["hints"])
        if [r["room"] for r in discoveries] != list(range(rooms)):
            raise ValueError("Incomplete discovery trace")
        for r in discoveries:
            if r["places"] != 2 + (r["room"] + 1) * len(SPEC["hints"]):
                raise ValueError("Incorrect discovery count")
        if [r["sample"] for r in samples] != list(range(SPEC["samples"])):
            raise ValueError("Incomplete sample trace")
        for r in samples:
            if (r["places"] != count or r["label"] != ("rename" if r["sample"] % 2 == 0 else "wait")
                    or r["records"] != 1 or r["navigation_refreshes"] != 0 or r["wire_bytes"] <= 0):
                raise ValueError("Invalid place sample")
            if any(not is_duration(r.get(m)) for m in METRICS):
                raise ValueError("Invalid measurement")
        if len(recovery) != 1 or not recovery[0]["exact"] or recovery[0]["places"] != count or recovery[0]["checkpoint_bytes"] <= 0:
            raise ValueError("Missing exact checkpoint recovery")
        for label in ("rename", "wait"):
            values = [r for r in samples if r["label"] == label]
            result = dict(places=count, label=label, n=len(values), wire_bytes=max(r["wire_bytes"] for r in values))
            outcome_metrics = OUTCOME_METRICS if any("outcome_apply_ms" in row for row in values) else ()
            require(all(all(field in row for field in outcome_metrics) for row in values),
                    "Causal delivery samples must be complete")
            for metric in (*METRICS, *outcome_metrics):
                ordered = sorted(r[metric] for r in values)
                result[metric] = dict(p50=ordered[math.ceil(len(ordered)*.5)-1],
                                      p95=ordered[math.ceil(len(ordered)*.95)-1], maximum=ordered[-1])
            summary.append(result)
        summary.append(recovery[0])
    return summary


def validate_client(result):
    positive_counts(result, ("version",))
    nonnegative_counts(result, ("extra_rooms",))
    rooms = result["extra_rooms"]
    count = 2 + rooms * len(SPEC["hints"])
    samples = result["samples"]
    if result["version"] != 1 or rooms not in SPEC["extra_rooms"] or len(result["final_places"]) != count:
        raise ValueError("Invalid real-client scale")
    if [r["sample"] for r in samples] != list(range(SPEC["samples"])):
        raise ValueError("Incomplete real-client trace")
    for r in samples:
        nonnegative_counts(r, ("sample",))
        positive_counts(r, ("places",))
        if r["places"] != count or r["label"] != ("rename" if r["sample"] % 2 == 0 else "wait"):
            raise ValueError("Invalid real-client sample")
        for metric in ("request_to_ack_ms", "request_to_ready_ms", "request_to_presentation_ms"):
            if not is_duration(r.get(metric)):
                raise ValueError("Invalid real-client timing")
    return result


if __name__ == "__main__":
    for result in validate([json.loads(line) for line in Path(sys.argv[1]).read_text(encoding="utf-8-sig").splitlines()]):
        print(json.dumps(result))
