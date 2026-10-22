"""Offline, unweighted Actions execution accounting; never qualification or billing.

Input: {"runs": [{"run": <workflow-run JSON>, "jobs": [<job JSON>, ...],
                  "jobs_scope": "all", "total_count": N}]}. Merge paginated jobs
before use; jobs_scope=all declares that all attempts were requested. Repeated
--input files may contain overlapping runs/jobs. No network access is performed.
"""
import argparse
from datetime import datetime
import json
import math
from pathlib import Path


def timestamp(value):
    """Return an aware timestamp or None, never guess a local timezone."""
    if not isinstance(value, str):
        return None
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
        return parsed.timestamp() if parsed.tzinfo else None
    except (ValueError, OverflowError):
        return None


def platform(job):
    labels = " ".join(str(label).lower() for label in job.get("labels", []))
    name = str(job.get("name", "")).lower()
    for os_name, tokens in (("windows", ("windows",)), ("linux", ("ubuntu", "linux")),
                            ("macos", ("macos",))):
        if any(token in labels or token in name for token in tokens):
            return os_name
    return "unknown"


def reconcile_job_ids(jobs):
    """Merge progress snapshots; reject contradictory identities before accounting.

    Null fields may be filled by a later snapshot in either input order. Two
    non-null name/start/end/attempt/conclusion values must agree. In particular,
    an altered completion time is not another execution and is never charged.
    """
    by_id = {}
    unidentified = []
    duplicates = 0
    for job in jobs:
        identifier = job.get("id")
        if identifier is None:
            unidentified.append(job)
            continue
        if identifier not in by_id:
            by_id[identifier] = dict(job)
            continue
        previous = by_id[identifier]
        for field in ("name", "started_at", "completed_at", "run_attempt", "conclusion"):
            old, new = previous.get(field), job.get(field)
            if old is not None and new is not None:
                if field.endswith("_at") and timestamp(old) is not None and timestamp(new) is not None:
                    same = timestamp(old) == timestamp(new)
                else:
                    same = old == new
                if not same:
                    raise ValueError(f"conflicting snapshots for job ID {identifier}: {field}")
            if old is None and new is not None:
                previous[field] = new
        # Runner labels normally do not change; retain available attribution.
        if not previous.get("labels") and job.get("labels"):
            previous["labels"] = job["labels"]
        duplicates += 1
    return list(by_id.values()) + unidentified, duplicates


def report(bundles, summaries=()):
    """Account completed intervals; incomplete/invalid data yields lower bounds."""
    grouped = {}
    for bundle in bundles:
        for entry in bundle["runs"]:
            run = entry["run"]
            key = (run.get("repository", {}).get("full_name", ""), run["id"])
            grouped.setdefault(key, []).append(entry)
    rows = []
    for key, entries in sorted(grouped.items()):
        run = max((entry["run"] for entry in entries), key=lambda r: r.get("run_attempt", 1))
        issues = set()
        if not any(entry.get("jobs_scope") == "all" for entry in entries):
            issues.add("all_attempts_not_declared")
        jobs = [job for entry in entries for job in entry["jobs"]]
        ids = {job.get("id") for job in jobs if job.get("id") is not None}
        if any(not isinstance(entry.get("total_count"), int) or isinstance(entry.get("total_count"), bool)
               or entry["total_count"] < 0 for entry in entries):
            issues.add("job_count_not_declared")
        expected = max((entry.get("total_count", 0) for entry in entries
                        if isinstance(entry.get("total_count", 0), int)), default=0)
        if expected > len(ids):
            issues.add("job_pages_incomplete")
        # The API may assign NEW IDs to carried-forward successes on partial rerun.
        # Exact name/start/end intervals, not IDs alone, identify those copies.
        jobs, reconciled = reconcile_job_ids(jobs)
        seen = set()
        deduplicated = reconciled
        seconds = 0.0
        os_seconds = dict.fromkeys(("linux", "windows", "macos", "unknown"), 0.0)
        starts, ends = [], []
        incomplete = 0
        executed = 0
        for job in jobs:
            start, end = timestamp(job.get("started_at")), timestamp(job.get("completed_at"))
            identity = (job.get("name"), start, end)
            if start is None or end is None:
                identity = ("id", job.get("id"), job.get("run_attempt"))
            if identity in seen:
                deduplicated += 1
                continue
            seen.add(identity)
            if job.get("conclusion") == "skipped":
                continue
            if start is None or end is None or end < start:
                incomplete += 1
                issues.add("job_interval_incomplete_or_invalid")
                continue
            duration = end - start
            executed += 1
            seconds += duration
            os_seconds[platform(job)] += duration
            starts.append(start)
            ends.append(end)
        if run.get("status") != "completed":
            issues.add("workflow_not_completed")
        created = timestamp(run.get("created_at"))
        terminal = timestamp(run.get("updated_at"))
        if created is None or (starts and created > min(starts)):
            issues.add("workflow_creation_invalid")
            created = None
        rows.append({"repository": key[0], "run_id": run["id"], "url": run.get("html_url"),
                     "workflow": run.get("name"), "event": run.get("event"),
                     "conclusion": run.get("conclusion"), "attempts": run.get("run_attempt", 1),
                     "runner_seconds": seconds, "runner_hours": seconds / 3600,
                     "os_runner_seconds": os_seconds, "executed_jobs": executed,
                     "duplicate_records_removed": deduplicated, "incomplete_jobs": incomplete,
                     "is_lower_bound": bool(issues), "issues": sorted(issues),
                     "execution_span_seconds": max(ends) - min(starts) if starts else None,
                     "created_to_execution_end_seconds": max(ends) - created if ends and created is not None else None,
                     "first_job_delay_seconds": min(starts) - created if starts and created is not None else None,
                     "github_terminal_span_seconds": terminal - created if terminal is not None and created is not None and terminal >= created else None})
    stages = []
    for summary in summaries:
        for step in summary.get("steps", []):
            value = step.get("seconds")
            if not isinstance(value, (int, float)) or isinstance(value, bool) or not math.isfinite(value) or value < 0:
                raise ValueError("stage seconds must be finite and nonnegative")
            stages.append({"step": step["step"], "seconds": value})
    return {"schema_version": 1, "units": "unweighted runner time, not billing or CPU time",
            "runner_seconds": sum(row["runner_seconds"] for row in rows),
            "runner_hours": sum(row["runner_hours"] for row in rows),
            "is_lower_bound": not rows or any(row["is_lower_bound"] for row in rows),
            "issues": [] if rows else ["no_workflow_runs_supplied"],
            "runs": rows, "optional_stage_timings": stages,
            "stage_note": "Informational only; may overlap runner time and are never added to totals."}


def human_readable(result):
    lines = [f"Runner-hours: {'at least ' if result['is_lower_bound'] else ''}{result['runner_hours']:.4f}",
             "Unweighted execution time; not billing. Spans include dependency/retry waits, not just queueing.",
             "Run | runner h | creation→last execution min | incomplete jobs | duplicate records"]
    for row in result["runs"]:
        span = row["created_to_execution_end_seconds"]
        lines.append(f"{row['run_id']} | {row['runner_hours']:.4f} | {span / 60 if span is not None else 'unknown'} | {row['incomplete_jobs']} | {row['duplicate_records_removed']}")
        if row["issues"]:
            lines.append("  Limitations: " + ", ".join(row["issues"]))
    return "\n".join(lines) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, action="append", required=True)
    parser.add_argument("--summary", type=Path, action="append", default=[])
    parser.add_argument("--json-output", type=Path, required=True)
    parser.add_argument("--text-output", type=Path)
    args = parser.parse_args(argv)
    try:
        result = report([json.loads(path.read_text(encoding="utf-8")) for path in args.input],
                        [json.loads(path.read_text(encoding="utf-8")) for path in args.summary])
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.error(str(error))
    args.json_output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    text = human_readable(result)
    if args.text_output:
        args.text_output.write_text(text, encoding="utf-8")
    else:
        print(text, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
