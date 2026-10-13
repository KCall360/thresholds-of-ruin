"""Validate head-only ordered-strikes v1; never accept a partial matrix silently."""
import argparse
import json
import math

from workload_report import (is_duration, nonnegative_counts, positive_counts,
                             read_rows, require, summarize_matrix)

KEYS = ("strikes", "riders", "actors", "regions", "mode")
CASES = {(s, q, a, r, m) for s, q in ((1, 0), (3, 2), (16, 8))
         for a in (2, 256) for r in (16, 256) for m in ("checkpoint", "tail")}
TIMINGS = ("command_ms", "sequence_ms", "admission_start_ms", "advancement_ms",
           "setup_ms", "plan_encoding_ms", "checkpoint_encoding_ms", "save_ms",
           "resume_ms", "final_save_ms", "final_resume_ms", "comparison_projection_ms",
           "simulation_ms", "perception_ms", "navigation_ms", "revision_ms", "checkpoint_capture_ms")
BYTES = ("plan_json_bytes", "checkpoint_json_bytes", "saved_bytes", "final_saved_bytes", "disclosed_bytes")
COUNTERS = ("loaded_actors", "loaded_regions", "setup_commands", "setup_checkpoint_sequence",
            "checkpoint_interval", "prefix_completed", "command_count", "prefix_commands",
            "continuation_commands", "primary_hits", "target_health_before", "target_health_after",
            "ability_plans", "game_clones", "preparation_candidates", "actor_table_copies",
            "observations", "scenes", "body_cells", "actor_candidates", "health_derivations",
            "strike_resolutions", "rider_resolutions", "revision_comparisons", "actors_observed",
            "prefix_records_loaded", "prefix_checkpoint_sequence", "prefix_records_replayed",
            "final_records_loaded", "final_checkpoint_sequence", "final_records_replayed")
FLAGS = ("prefix_state_equal", "prefix_journal_equal", "final_state_equal", "final_journal_equal")
FIELDS = set(KEYS + TIMINGS + BYTES + COUNTERS + FLAGS + ("workload", "version", "sample", "seed"))


def check_row(row):
    positive_counts(row, BYTES)
    nonnegative_counts(row, COUNTERS + ("seed",))
    require(type(row["sample"]) is int and row["sample"] in range(3), "unknown sample ID")
    require(row["seed"] == 42 + row["sample"], "each sample has its fixed seed")
    require(all(row[field] is True for field in FLAGS), "reopen state and journal equality must be verified")
    require(row["loaded_actors"] == row["actors"] and row["loaded_regions"] == row["regions"],
            "loaded dimensions must match the declared case")
    require(row["setup_commands"] == row["actors"] == row["setup_checkpoint_sequence"],
            "each actor waits once before the real setup checkpoint")
    require(row["prefix_completed"] == int(row["strikes"] > 1), "wrong saved strike prefix")
    waits = (row["strikes"] * 100 + 149) // 150
    require(row["command_count"] == 1 + waits, "wrong measured command count")
    require(row["prefix_commands"] == 1 + int(row["strikes"] > 1), "wrong prefix command count")
    require(row["continuation_commands"] == row["command_count"] - row["prefix_commands"],
            "wrong continuation command count")
    require(isinstance(row["command_ms"], list) and len(row["command_ms"]) == row["command_count"],
            "each ordinary command needs one interval")
    for field in TIMINGS:
        values = row[field] if field == "command_ms" else [row[field]]
        require(all(is_duration(value) for value in values), f"invalid {field} duration")
    for field, expected in (("sequence_ms", sum(row["command_ms"])),
                            ("admission_start_ms", row["command_ms"][0]),
                            ("advancement_ms", sum(row["command_ms"][1:]))):
        require(math.isclose(row[field], expected, rel_tol=1e-9, abs_tol=1e-9),
                f"{field} must sum only the corresponding measured commands")
    require(row["strike_resolutions"] == row["strikes"], "every primary must resolve exactly once")
    require(row["rider_resolutions"] <= row["strikes"] * row["riders"], "impossible rider count")
    require(row["primary_hits"] <= row["strikes"], "impossible primary hit count")
    require(0 < row["target_health_after"] <= row["target_health_before"], "target must survive")
    checkpoint = row["mode"] == "checkpoint"
    require(row["checkpoint_interval"] == (1 if checkpoint else 4096), "wrong persistence policy")
    for stage, commands in (("prefix", row["prefix_commands"]), ("final", row["command_count"])):
        loaded = row["setup_commands"] + commands
        require(row[f"{stage}_records_loaded"] == loaded, "reopen must load the exact accepted prefix")
        require(row[f"{stage}_checkpoint_sequence"] == (loaded if checkpoint else row["setup_checkpoint_sequence"]),
                "reopen must select the actual checkpoint, not full replay")
        require(row[f"{stage}_records_replayed"] == (0 if checkpoint else commands),
                "reopen must replay only the bounded measured tail")


def extend(summary, samples):
    summary["seeds"] = sorted(sample["seed"] for sample in samples)
    summary["caution"] = "Head-only, small n: three seed samples; sample-level p95 is the maximum, not a tail estimate."


def summarize(rows, *, smoke=False):
    require(type(smoke) is bool, "smoke selection must be explicit")
    rows = list(rows)
    for row in rows:
        require(isinstance(row, dict), "workload rows must be objects")
        require(set(row) == FIELDS, "missing or unknown fields; frozen plan operands are never report fields")
        require(all(type(row[field]) is int for field in KEYS[:-1]) and type(row["mode"]) is str,
                "case dimensions and persistence mode have strict types")
    cases = {case for case in CASES if case[:4] == (3, 2, 2, 16)} if smoke else CASES
    return summarize_matrix(rows, matches=lambda r: r["workload"] == "ordered-strikes" and r["version"] == 1,
                            key_fields=KEYS, cases=cases, check_row=check_row, sample_ids=range(3),
                            timings=TIMINGS, counts=BYTES + COUNTERS, extend=extend)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("samples")
    parser.add_argument("--smoke", action="store_true", help="require only the complete s3/q2/a2/r16 seed/mode subset")
    args = parser.parse_args()
    print(json.dumps(summarize(read_rows(args.samples), smoke=args.smoke), indent=2))
