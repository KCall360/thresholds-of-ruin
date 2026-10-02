"""Validate and summarize items workload v1 without dumping raw samples."""
from workload_report import main, positive_counts, require, summarize_matrix

CASES = {(16, 8), (1000, 256)}
COUNTS = ["disclosed_bytes", "saved_bytes", "observations", "scenes", "item_candidates", "stack_candidates", "knowledge_checks"]


def check_row(row):
    require(row["transfers"] == len(row["transfer_ms"]) == 20, "each sample times 20 transfers")
    require(len(row["client_apply_ms"]) == len(row["client_render_ms"]) == 20, "the client applies and renders each transfer")
    positive_counts(row, COUNTS)


def summarize(rows):
    return summarize_matrix(
        rows, matches=lambda r: r.get("workload_version") == 1, key_fields=["items", "identities"],
        cases=CASES, check_row=check_row, sample_ids=range(20),
        timings=["construction_ms", "knowledge_ms", "transfer_ms", "client_apply_ms", "client_render_ms", "save_ms", "resume_ms"],
        counts=COUNTS, extend=lambda report, samples: report.update(samples=len(samples)))


if __name__ == "__main__":
    main(summarize)
