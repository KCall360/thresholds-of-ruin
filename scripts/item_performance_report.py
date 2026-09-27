"""Validate and summarize items workload v1 without dumping raw samples."""
import json
import math
from pathlib import Path
import sys


def summarize(rows):
    groups = {}
    for row in rows:
        assert row['workload_version'] == 1
        key = (row['items'], row['identities'])
        assert key in [(16, 8), (1000, 256)]
        assert row['transfers'] == len(row['transfer_ms']) == 20
        assert len(row['client_apply_ms']) == len(row['client_render_ms']) == 20
        for field in ['disclosed_bytes', 'saved_bytes', 'observations', 'scenes', 'item_candidates', 'stack_candidates', 'knowledge_checks']:
            assert isinstance(row[field], int) and row[field] > 0
        groups.setdefault(key, []).append(row)
    assert set(groups) == {(16, 8), (1000, 256)}
    result = []
    for key, samples in sorted(groups.items()):
        assert {s['sample'] for s in samples} == set(range(20)) and len(samples) == 20
        report = dict(items=key[0], identities=key[1], samples=len(samples))
        for field in ['construction_ms', 'knowledge_ms', 'transfer_ms', 'client_apply_ms', 'client_render_ms', 'save_ms', 'resume_ms']:
            values = sorted(v for s in samples for v in (s[field] if isinstance(s[field], list) else [s[field]]))
            assert all(isinstance(v, (float, int)) and math.isfinite(v) and v >= 0 for v in values)
            report[field] = dict(n=len(values), p50=values[math.ceil(len(values)*.5)-1], p95=values[math.ceil(len(values)*.95)-1], maximum=max(values))
        for field in ['disclosed_bytes', 'saved_bytes', 'observations', 'scenes', 'item_candidates', 'stack_candidates', 'knowledge_checks']:
            report[field] = dict(min=min(s[field] for s in samples), max=max(s[field] for s in samples))
        result.append(report)
    return result


if __name__ == '__main__':
    rows = [json.loads(line) for line in Path(sys.argv[1]).read_text(encoding='utf-8-sig').splitlines() if line.startswith('{')]
    print(json.dumps(summarize(rows), indent=2))
