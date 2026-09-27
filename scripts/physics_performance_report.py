"""Validate the complete physics workload v1 and summarize retained raw samples."""
import json
import math
from pathlib import Path
import sys

CASES = {(actors, items, cells, falling)
         for actors, items, cells in [(1, 1, 2), (8, 128, 2), (1, 1, 8), (8, 128, 8)]
         for falling in [False, True]}


def summarize(rows):
    groups = {}
    for row in rows:
        assert row['workload'] == 'physics' and row['version'] == 1
        key = tuple(row[f] for f in ['actors', 'items', 'cells', 'falling'])
        assert key in CASES
        assert len(row['command_ms']) == row['actors'] * 8
        assert 0 < len(row['client_apply_ms']) <= len(row['command_ms'])
        assert len(row['client_draw_ms']) == len(row['client_apply_ms'])
        for field in ['saved_bytes', 'disclosed_bytes', 'body_cells', 'scenes']:
            assert isinstance(row[field], int) and row[field] > 0
        assert isinstance(row['physics_steps'], int) and row['physics_steps'] >= 0
        assert (row['physics_steps'] > 0) == row['falling']
        groups.setdefault(key, []).append(row)
    assert set(groups) == CASES
    result = []
    for key, samples in sorted(groups.items()):
        assert len(samples) == 3 and {s['sample'] for s in samples} == {0, 1, 2}
        report = dict(zip(['actors', 'items', 'cells', 'falling'], key))
        report['samples'] = len(samples)
        for field in ['command_ms', 'client_apply_ms', 'client_draw_ms', 'save_ms', 'resume_ms']:
            values = sorted(v for s in samples for v in (s[field] if isinstance(s[field], list) else [s[field]]))
            assert all(isinstance(v, (float, int)) and math.isfinite(v) and v >= 0 for v in values)
            report[field] = dict(n=len(values), p50=values[math.ceil(len(values)*.5)-1],
                                 p95=values[math.ceil(len(values)*.95)-1], maximum=max(values))
        for field in ['saved_bytes', 'disclosed_bytes', 'physics_steps', 'body_cells', 'scenes']:
            report[field] = dict(min=min(s[field] for s in samples), max=max(s[field] for s in samples))
        result.append(report)
    return result


if __name__ == '__main__':
    rows = [json.loads(line) for line in Path(sys.argv[1]).read_text(encoding='utf-8-sig').splitlines() if line.startswith('{')]
    print(json.dumps(summarize(rows), indent=2))
