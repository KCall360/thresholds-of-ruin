"""Validate combat workload v1; counters include warmup, timings exclude it."""
import json
import math
from pathlib import Path
import sys

def summarize(rows):
    groups = {}
    for row in rows:
        assert row['workload'] == 'combat' and row['version'] == 1
        key = (row['actors'], row['history'])
        assert key in {(2,0),(2,1000),(8,0),(8,1000)}
        assert len(row['command_ms']) == len(row['decision_ms']) == 64
        assert 0 < len(row['client_apply_ms']) <= 64
        assert len(row['client_apply_ms']) == len(row['client_draw_ms'])
        for field in ['saved_bytes','disclosed_bytes','scenes','body_cells']:
            assert isinstance(row[field],int) and row[field] > 0
        phases = row['phase_totals_ms']
        assert set(phases) == {'simulation','perception','navigation','revision','checkpoint_capture'}
        assert all(isinstance(v,(int,float)) and math.isfinite(v) and v>=0 for v in phases.values())
        assert isinstance(row['navigation_refreshes'],int) and row['navigation_refreshes']>=0
        groups.setdefault(key,[]).append(row)
    assert set(groups) == {(2,0),(2,1000),(8,0),(8,1000)}
    reports=[]
    for (actors,history), samples in sorted(groups.items()):
        assert len(samples)==3 and {s['sample'] for s in samples}=={0,1,2}
        report=dict(actors=actors,history=history)
        for field in ['command_ms','decision_ms','client_apply_ms','client_draw_ms','save_ms','resume_ms']:
            values=sorted(v for s in samples for v in (s[field] if isinstance(s[field],list) else [s[field]]))
            assert all(isinstance(v,(int,float)) and math.isfinite(v) and v>=0 for v in values)
            report[field]=dict(n=len(values),p50=values[math.ceil(len(values)*.5)-1],p95=values[math.ceil(len(values)*.95)-1],maximum=max(values))
        for field in ['saved_bytes','disclosed_bytes','scenes','body_cells']:
            report[field]=dict(min=min(s[field] for s in samples),max=max(s[field] for s in samples))
        report['phase_mean_ms']={k:sum(s['phase_totals_ms'][k] for s in samples)/192 for k in samples[0]['phase_totals_ms']}
        report['navigation_refreshes']={'min':min(s['navigation_refreshes'] for s in samples),'max':max(s['navigation_refreshes'] for s in samples)}
        reports.append(report)
    return reports

if __name__=='__main__':
    rows=[json.loads(line) for line in Path(sys.argv[1]).read_text(encoding='utf-8-sig').splitlines() if line.startswith('{')]
    print(json.dumps(summarize(rows),indent=2))
