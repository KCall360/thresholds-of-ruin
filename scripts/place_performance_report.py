"""Validate versioned place workload coverage and summarize exclusive boundaries."""
import json
import math
from pathlib import Path
import sys

SPEC = json.loads((Path(__file__).resolve().parents[1] / "crates/server/fixtures/place-knowledge-v1.json").read_text())
METRICS = ("command_ms", "observation_ms", "encode_ms", "apply_ms", "render_ms")


def validate(rows):
    if any(r.get("version") != SPEC["version"] or r.get("kind") not in ("discovery", "sample", "recovery")
           or r.get("extra_rooms") not in SPEC["extra_rooms"] for r in rows):
        raise ValueError("Unknown workload record")
    summary = []
    for rooms in SPEC["extra_rooms"]:
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
            if any(not math.isfinite(r[m]) or r[m] < 0 for m in METRICS):
                raise ValueError("Invalid measurement")
        if len(recovery) != 1 or not recovery[0]["exact"] or recovery[0]["places"] != count or recovery[0]["checkpoint_bytes"] <= 0:
            raise ValueError("Missing exact checkpoint recovery")
        for label in ("rename", "wait"):
            values = [r for r in samples if r["label"] == label]
            result = dict(places=count, label=label, n=len(values), wire_bytes=max(r["wire_bytes"] for r in values))
            for metric in METRICS:
                ordered = sorted(r[metric] for r in values)
                result[metric] = dict(p50=ordered[math.ceil(len(ordered)*.5)-1],
                                      p95=ordered[math.ceil(len(ordered)*.95)-1], maximum=ordered[-1])
            summary.append(result)
        summary.append(recovery[0])
    return summary


def validate_client(result):
    rooms = result["extra_rooms"]
    count = 2 + rooms * len(SPEC["hints"])
    samples = result["samples"]
    if result["version"] != 1 or rooms not in SPEC["extra_rooms"] or len(result["final_places"]) != count:
        raise ValueError("Invalid real-client scale")
    if [r["sample"] for r in samples] != list(range(SPEC["samples"])):
        raise ValueError("Incomplete real-client trace")
    for r in samples:
        if r["places"] != count or r["label"] != ("rename" if r["sample"] % 2 == 0 else "wait"):
            raise ValueError("Invalid real-client sample")
        for metric in ("request_to_ack_ms", "request_to_ready_ms", "request_to_presentation_ms"):
            if not isinstance(r[metric], (float, int)) or not math.isfinite(r[metric]) or r[metric] < 0:
                raise ValueError("Invalid real-client timing")
    return result


if __name__ == "__main__":
    for result in validate([json.loads(line) for line in Path(sys.argv[1]).read_text(encoding="utf-8-sig").splitlines()]):
        print(json.dumps(result))
