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
    if "previous_report_encode_ms" in profile or "previous_report_write_ms" in profile:
        for key in ("previous_report_encode_ms", "previous_report_write_ms"):
            value = profile.get(key)
            if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
                raise ValueError("Invalid diagnostic report phase duration")


def validate(rows, version=1):
    if version not in (1, 2):
        raise ValueError('Unsupported client workload version')
    expected = [(cells, burst, sample) for cells in (64, 20956)
                for burst in (1, 64) for sample in range(20)]
    if [(r["cells"], r["burst"], r["sample"]) for r in rows] != expected:
        raise ValueError("Missing, duplicated or reordered client workload samples")
    groups = {}
    for row in rows:
        if row["version"] != version or row["memory"] != row["cells"] or row["chart"] != min(4096, row["cells"]):
            raise ValueError("Wrong workload version or memory/chart coverage")
        if version == 2:
            expected_count = 1 if row['burst'] == 1 and row['sample'] == 0 else 2
            if type(row.get('narration_count')) is not int or row['narration_count'] != expected_count:
                raise ValueError('Missing semantic narration coverage')
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
    parser.add_argument('--narration', action='store_true', help='Require version 2 semantic workload')
    args = parser.parse_args()
    rows = [json.loads(line) for line in args.samples.read_text(encoding="utf-8-sig").splitlines()]
    print(json.dumps(validate(rows, version=2 if args.narration else 1), indent=2))


if __name__ == "__main__":
    main()
