"""Validate the complete physics workload v1 and summarize retained raw samples."""
from workload_report import main, positive_counts, require, summarize_matrix

CASES = {(actors, items, cells, falling)
         for actors, items, cells in [(1, 1, 2), (8, 128, 2), (1, 1, 8), (8, 128, 8)]
         for falling in [False, True]}


def check_row(row):
    require(len(row["command_ms"]) == row["actors"] * 8, "each sample times eight commands per actor")
    require(0 < len(row["client_apply_ms"]) <= len(row["command_ms"]), "the client applies at most one update per command")
    require(len(row["client_draw_ms"]) == len(row["client_apply_ms"]), "every applied update is drawn")
    positive_counts(row, ["saved_bytes", "disclosed_bytes", "body_cells", "scenes"])
    require(isinstance(row["physics_steps"], int) and row["physics_steps"] >= 0, "physics steps must be a count")
    require((row["physics_steps"] > 0) == row["falling"], "only falling cases run physics steps")


def summarize(rows):
    return summarize_matrix(
        rows, matches=lambda r: r.get("workload") == "physics" and r.get("version") == 1,
        key_fields=["actors", "items", "cells", "falling"], cases=CASES, check_row=check_row, sample_ids=range(3),
        timings=["command_ms", "client_apply_ms", "client_draw_ms", "save_ms", "resume_ms"],
        counts=["saved_bytes", "disclosed_bytes", "physics_steps", "body_cells", "scenes"],
        extend=lambda report, samples: report.update(samples=len(samples)))


if __name__ == "__main__":
    main(summarize)
