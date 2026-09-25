"""Validate versioned client-memory/burst samples and summarize matched phases."""
import argparse
import json
import math
from pathlib import Path


def distribution(values):
    values = sorted(values)
    return {"n": len(values), "p50_ms": values[math.ceil(len(values)*.5)-1],
            "p95_ms": values[math.ceil(len(values)*.95)-1], "max_ms": values[-1]}


def validate_presentation_profile(profile):
    if profile["version"] != 1 or type(profile["network_events"]) is not int or not 0 <= profile["network_events"] <= 16:
        raise ValueError("Invalid native profile version or event budget")
    for key in ("apply_ms", "draw_ms", "native_ms", "capture_ms", "previous_report_ms", "turn_interval_ms"):
        value = profile[key]
        if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
            raise ValueError("Invalid native phase duration")


def validate(rows):
    expected = [(cells, burst, sample) for cells in (64, 20956)
                for burst in (1, 64) for sample in range(20)]
    if [(r["cells"], r["burst"], r["sample"]) for r in rows] != expected:
        raise ValueError("Missing, duplicated or reordered client workload samples")
    groups = {}
    for row in rows:
        if row["version"] != 1 or row["memory"] != row["cells"] or row["chart"] != min(4096, row["cells"]):
            raise ValueError("Wrong workload version or memory/chart coverage")
        for phase in ("apply_ms", "render_ms"):
            value = row[phase]
            if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
                raise ValueError("Invalid phase duration")
            key = f"cells-{row['cells']}-burst-{row['burst']}-{phase}"
            groups.setdefault(key, []).append(value)
    return {key: distribution(values) for key, values in groups.items()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("samples", type=Path)
    args = parser.parse_args()
    rows = [json.loads(line) for line in args.samples.read_text(encoding="utf-8-sig").splitlines()]
    print(json.dumps(validate(rows), indent=2))


if __name__ == "__main__":
    main()
