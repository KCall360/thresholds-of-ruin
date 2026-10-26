"""Validate the CPU-only procedural construction workload; no GPU/FPS claims."""
import argparse
import json
import math
from pathlib import Path

CASES = ("human-sylvan-plate", "nymph-princess-cloak", "mage-native-boots")
COUNTS = ("cold_meshes", "warm_added_meshes", "cold_mesh_bytes", "warm_added_mesh_bytes",
          "cold_entities", "warm_entities", "motion_frames", "changed_pose_frames")
PHASES = ("pose_joints_ms", "robe_fit_ms", "drape_update_ms", "hair_fit_ms")
HAIR_PHASES = ("hair_controls_ms", "hair_deform_ms", "hair_constraints_ms", "hair_mesh_ms")
FRAME_TIMINGS = ("pose_update_ms", *PHASES, *HAIR_PHASES)
TIMINGS = ("cold_spawn_ms", "warm_spawn_ms", *FRAME_TIMINGS)


def validate(rows):
    expected = [(case, sample) for case in CASES for sample in range(3)]
    if [(r.get("case"), r.get("sample")) for r in rows] != expected:
        raise ValueError("Missing, duplicated or reordered model samples")
    counts = {}
    for row in rows:
        if set(row) != {"version", "case", "sample", "seed", "hz", *COUNTS, *TIMINGS}:
            raise ValueError("Unexpected model sample fields")
        if type(row["version"]) is not int or row["version"] != 4 or type(row["seed"]) is not int or row["seed"] != 7:
            raise ValueError("Unsupported model workload version or seed")
        if type(row["sample"]) is not int:
            raise ValueError("Invalid sample index")
        if type(row["hz"]) is not int or row["hz"] != 50 or row["motion_frames"] != 30:
            raise ValueError("Invalid fixed-step motion workload")
        if any(type(row[key]) is not list or len(row[key]) != 30 for key in FRAME_TIMINGS):
            raise ValueError("Missing motion frame timings")
        values = [row["cold_spawn_ms"], row["warm_spawn_ms"]]
        values.extend(value for key in FRAME_TIMINGS for value in row[key])
        for value in values:
            if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
                raise ValueError("Invalid model duration")
        for frame, total in enumerate(row["pose_update_ms"]):
            if sum(row[key][frame] for key in PHASES) > total + 0.000001:
                raise ValueError("Motion phases exceed their enclosing update")
            if sum(row[key][frame] for key in HAIR_PHASES) > row["hair_fit_ms"][frame] + 0.000001:
                raise ValueError("Hair subphases exceed their enclosing hair update")
        if row["case"] == "nymph-princess-cloak" and any(sum(row[key]) <= 0 for key in HAIR_PHASES):
            raise ValueError("Missing active hair-stage timing")
        if any(type(row[k]) is not int or row[k] < 0 for k in COUNTS):
            raise ValueError("Invalid model counts")
        if not 0 < row["changed_pose_frames"] <= 30:
            raise ValueError("Motion workload did not change the pose")
        if not (0 < row["cold_meshes"] and 0 < row["cold_mesh_bytes"] and 0 < row["cold_entities"] == row["warm_entities"]):
            raise ValueError("Missing or mismatched spawned models")
        if row["warm_added_meshes"] >= row["cold_meshes"] or row["warm_added_mesh_bytes"] >= row["cold_mesh_bytes"]:
            raise ValueError("Repeated spawn did not reuse mesh storage")
        current = tuple(row[k] for k in COUNTS)
        if counts.setdefault(row["case"], current) != current:
            raise ValueError("Deterministic model counts changed between samples")
    return counts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    args = parser.parse_args()
    rows = [json.loads(line) for line in args.input.read_text(encoding="utf-8-sig").splitlines() if line.strip()]
    validate(rows)
    print(f"Validated {len(rows)} CPU construction/motion samples; mesh payload is not process/GPU memory")


if __name__ == "__main__":
    main()
