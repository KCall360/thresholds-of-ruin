"""Validate and summarize items workload v1 without dumping raw samples."""
from workload_report import is_duration, main, nonnegative_counts, positive_counts, require, summarize_matrix

CASES = {(16, 8), (1000, 256)}
COUNTS = ["disclosed_bytes", "saved_bytes", "observations", "scenes", "item_candidates", "stack_candidates", "knowledge_checks"]


def check_row(row):
    require(row["transfers"] == len(row["transfer_ms"]) == 20, "each sample times 20 transfers")
    require(len(row["client_apply_ms"]) == len(row["client_render_ms"]) == 20, "the client applies and renders each transfer")
    if "outcome_apply_ms" in row:
        require(len(row["outcome_apply_ms"]) == 20, "one causal outcome is applied per transfer")
        require(all(is_duration(v) for v in row["outcome_apply_ms"]), "outcome application durations are valid")
    positive_counts(row, [field for field in COUNTS if field != "scenes"])
    nonnegative_counts(row, ["scenes"])


def summarize(rows, *, selected_case=None):
    require(selected_case is None or (type(selected_case) is tuple and len(selected_case) == 2
            and all(type(value) is int for value in selected_case) and selected_case in CASES),
            "unknown or malformed selected case")
    cases = CASES if selected_case is None else {selected_case}
    outcome_timings = ["outcome_apply_ms"] if any("outcome_apply_ms" in row for row in rows) else []
    return summarize_matrix(
        rows, matches=lambda r: r.get("workload_version") == 1, key_fields=["items", "identities"],
        cases=cases, check_row=check_row, sample_ids=range(20),
        timings=["construction_ms", "knowledge_ms", "transfer_ms", "client_apply_ms", "client_render_ms", "save_ms", "resume_ms"] + outcome_timings,
        counts=COUNTS, extend=lambda report, samples: report.update(samples=len(samples)))


if __name__ == "__main__":
    main(summarize)
