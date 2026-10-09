"""Validate the complete physics workload v1 and summarize retained raw samples."""
from workload_report import is_duration, main, positive_counts, require, summarize_matrix

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
    checkpoint = row.get("checkpoint_profile")
    if checkpoint is not None:
        require(isinstance(checkpoint, dict), "checkpoint profile must be an object")
        require(type(checkpoint.get("bytes")) is int and checkpoint["bytes"] > 0,
                "checkpoint size must be positive")
        require(is_duration(checkpoint.get("ms")), "checkpoint encoding must be a duration")
        require(isinstance(checkpoint.get("status"), dict), "checkpoint status must be an object")
        status = checkpoint["status"]
        if "last_commit_ms" in status:
            commit, batch = status["last_commit_ms"], status.get("last_batch_ms")
            require(type(commit) is int and type(batch) is int and 0 <= commit <= batch,
                    "commit timing must be a nonnegative part of its batch")
    profiles = row.get("profiles", [])
    require(isinstance(profiles, list), "profiles must be a list")
    if profiles:
        require(len(profiles) == len(row["command_ms"]), "one profile is required per command")
        for profile in profiles:
            require(isinstance(profile, dict), "profiles must contain objects")
            for key in ("authoritative_total", "perception", "navigation_refresh", "simulation_transition"):
                duration = profile.get(key, {})
                require(isinstance(duration, dict), "profile durations must be objects")
                seconds, nanos = duration.get("secs"), duration.get("nanos")
                require(type(seconds) is int and seconds >= 0 and type(nanos) is int
                        and 0 <= nanos < 1_000_000_000, "invalid profile duration")
            for key in ("actors_observed", "perception_calls", "scene_calls"):
                value = profile.get(key)
                require(type(value) is int and value >= 0, "invalid profile work count")


def summarize(rows):
    return summarize_matrix(
        rows, matches=lambda r: r.get("workload") == "physics" and r.get("version") == 1,
        key_fields=["actors", "items", "cells", "falling"], cases=CASES, check_row=check_row, sample_ids=range(3),
        timings=["command_ms", "client_apply_ms", "client_draw_ms", "save_ms", "resume_ms"],
        counts=["saved_bytes", "disclosed_bytes", "physics_steps", "body_cells", "scenes"],
        extend=lambda report, samples: report.update(samples=len(samples)))


if __name__ == "__main__":
    main(summarize)
