"""Validate combat workload v1; counters include warmup, timings exclude it."""
from workload_report import is_duration, main, positive_counts, require, summarize_matrix

CASES = {(2, 0), (2, 1000), (8, 0), (8, 1000)}
PHASES = {"simulation", "perception", "navigation", "revision", "checkpoint_capture"}
COUNTS = ["saved_bytes", "disclosed_bytes", "scenes", "body_cells"]


def check_row(row):
    require(len(row["command_ms"]) == len(row["decision_ms"]) == 64, "each sample times 64 commands and decisions")
    require(0 < len(row["client_apply_ms"]) <= 64, "the client applies between 1 and 64 updates")
    require(len(row["client_apply_ms"]) == len(row["client_draw_ms"]), "every applied update is drawn")
    positive_counts(row, COUNTS)
    phases = row["phase_totals_ms"]
    require(set(phases) == PHASES, "phase totals must name every phase")
    require(all(is_duration(v) for v in phases.values()), "phase totals must be durations")
    require(isinstance(row["navigation_refreshes"], int) and row["navigation_refreshes"] >= 0, "navigation refreshes must be a count")


def extend(report, samples):
    # 3 samples x 64 commands.
    report["phase_mean_ms"] = {k: sum(s["phase_totals_ms"][k] for s in samples) / 192 for k in samples[0]["phase_totals_ms"]}
    report["navigation_refreshes"] = dict(min=min(s["navigation_refreshes"] for s in samples),
                                          max=max(s["navigation_refreshes"] for s in samples))


def summarize(rows):
    return summarize_matrix(
        rows, matches=lambda r: r.get("workload") == "combat" and r.get("version") == 1,
        key_fields=["actors", "history"], cases=CASES, check_row=check_row, sample_ids=range(3),
        timings=["command_ms", "decision_ms", "client_apply_ms", "client_draw_ms", "save_ms", "resume_ms"],
        counts=COUNTS, extend=extend)


if __name__ == "__main__":
    main(summarize)
