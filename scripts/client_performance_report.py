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


def validate_ownership(rows):
    """Validate paired clone costs and distinct serialized contents, not RSS."""
    expected = [(cells, readers, sample, method)
                for cells in (64, 4096, 20956) for readers in (1, 8, 32) for sample in range(100)
                for method in (('owned_clone', 'shared_handle') if sample % 2 == 0
                               else ('shared_handle', 'owned_clone'))]
    if len(rows) != len(expected):
        raise ValueError('Incomplete observation ownership workload')
    fields = {'diagnostic', 'version', 'cells', 'readers', 'method', 'sample',
              'state_bytes', 'retained_objects', 'distinct_state_serialized_bytes', 'clone_ms', 'verified'}
    groups, payloads = {}, {}
    for row, identity in zip(rows, expected):
        if not isinstance(row, dict) or set(row) != fields:
            raise ValueError('Invalid ownership diagnostic fields')
        for key in ('version', 'cells', 'readers', 'sample', 'state_bytes',
                    'retained_objects', 'distinct_state_serialized_bytes'):
            if type(row[key]) is not int:
                raise ValueError('Invalid ownership diagnostic integer')
        if (row['diagnostic'] != 'observation_ownership' or row['version'] != 1
                or row['verified'] is not True
                or (row['cells'], row['readers'], row['sample'], row['method']) != identity):
            raise ValueError('Wrong version, order or verification of ownership samples')
        objects = row['readers'] if row['method'] == 'owned_clone' else 1
        if (row['state_bytes'] <= 0 or row['retained_objects'] != objects
                or row['distinct_state_serialized_bytes'] != row['state_bytes'] * objects):
            raise ValueError('Wrong distinct observation content counts')
        if payloads.setdefault(row['cells'], row['state_bytes']) != row['state_bytes']:
            raise ValueError('Observation payload changed between ownership methods')
        elapsed = row['clone_ms']
        if type(elapsed) not in (int, float) or not math.isfinite(elapsed) or elapsed < 0:
            raise ValueError('Invalid ownership phase duration')
        key = f"cells-{row['cells']}-readers-{row['readers']}-{row['method']}"
        group = groups.setdefault(key, {'samples': [], 'state_bytes': row['state_bytes'],
                                       'retained_objects': objects,
                                       'distinct_state_serialized_bytes': row['distinct_state_serialized_bytes']})
        group['samples'].append(elapsed)
    result = {}
    for key, group in groups.items():
        result[key] = {**distribution(group['samples']), 'state_bytes': group['state_bytes'],
                       'retained_objects': group['retained_objects'],
                       'distinct_state_serialized_bytes': group['distinct_state_serialized_bytes']}
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("samples", type=Path)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--narration', action='store_true', help='Require version 2 semantic workload')
    mode.add_argument('--ownership', action='store_true', help='Require paired immutable observation ownership samples')
    args = parser.parse_args()
    rows = [json.loads(line) for line in args.samples.read_text(encoding="utf-8-sig").splitlines()]
    result = validate_ownership(rows) if args.ownership else validate(rows, version=2 if args.narration else 1)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
