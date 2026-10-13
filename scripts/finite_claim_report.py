"""Validate complete, isolated finite-claims v1 samples and repeat cohorts.

Logical checkpoint/plan bytes and physical SQLite file lengths are separate.
The claims_equal flags compare scalar profiles, not the private claim graph.
Only scalar private-authority diagnostics belong in a sample; no claim identities
or captured operands are accepted. Timings are diagnostic, never pass thresholds.
"""
import argparse
import json
import math
from pathlib import Path

from workload_report import InvalidWorkload, is_duration, require, summarize_matrix

KEYS = ("case", "background_actors", "mode")
SPECS = {}
for _access in ("carried", "ground"):
    for _costs, _fragments in ((1, 1), (1, 32), (8, 8), (8, 32)):
        SPECS[f"{_access}-c{_costs}-f{_fragments}"] = (_access, _costs, _fragments, 1, 1, 1)
SPECS.update({"last-unit-2": ("ground", 1, 1, 2, 1, 1),
              "shared-stack-27": ("ground", 1, 1, 27, 27, 27),
              "shared-stacks-64": ("ground", 1, 1, 64, 64, 8)})
CASES = {(case, background, mode) for case in SPECS
         for background in (0, 64) for mode in ("checkpoint", "tail")}
ARRAY_TIMINGS = ("admission_ms", "start_ms", "advancement_ms")
TIMINGS = ARRAY_TIMINGS + ("measured_ms", "setup_ms", "diagnostic_ms", "plan_encoding_ms",
                          "prefix_checkpoint_encoding_ms", "final_checkpoint_encoding_ms",
                          "prefix_save_ms", "prefix_resume_ms", "final_save_ms", "final_resume_ms", "comparison_ms")
PHYSICAL_BYTES = ("prefix_sqlite_file_bytes", "final_sqlite_file_bytes")
LOGICAL_BYTES = ("plan_max_json_bytes", "plan_total_json_bytes", "prefix_checkpoint_json_bytes",
                 "final_checkpoint_json_bytes")
COUNTERS = ("costs", "fragments_per_use", "competing_owners", "loaded_actors", "loaded_regions",
            "setup_records", "setup_checkpoint_sequence", "checkpoint_interval", "admitted_uses",
            "successful_starts", "failed_starts", "successful_resolutions", "consumed_units",
            "prefix_held_fragments", "prefix_claim_owners", "prefix_max_item_holds", "prefix_held_units",
            "final_held_fragments", "final_claim_owners", "final_max_item_holds", "final_held_units",
            "item_hold_limit", "fragment_limit", "cost_limit", "physical_units_before",
            "physical_units_at_prefix", "physical_units_after", "prefix_records", "final_records",
            "advancement_commands", "plan_count") + tuple(
                f"{phase}_{counter}" for phase in ("admission", "start", "settlement")
                for counter in ("game_clones", "scenes", "ordinary_scenes", "observations", "ability_plans",
                                "preparation_candidates", "strike_resolutions", "scene_membership_lookups",
                                "scene_membership_cells", "scene_cells_copied")) + tuple(
                f"{stage}_{counter}" for stage in ("prefix", "final")
                for counter in ("records_loaded", "checkpoint_sequence", "records_replayed"))
FLAGS = tuple(f"{stage}_{kind}_equal" for stage in ("prefix", "final") for kind in ("state", "journal", "claims"))
FIELDS = set(KEYS + TIMINGS + PHYSICAL_BYTES + LOGICAL_BYTES + COUNTERS + FLAGS
             + ("workload", "version", "sample", "seed", "access"))


def check_row(row):
    for field in COUNTERS + ("version", "sample", "seed", "background_actors"):
        require(type(row[field]) is int and 0 <= row[field] <= 10**12,
                f"{field} must be a bounded nonnegative integer")
    for field in PHYSICAL_BYTES + LOGICAL_BYTES:
        require(type(row[field]) is int and 0 < row[field] < 2**63,
                f"{field} must be a positive bounded byte count")
    require(type(row["access"]) is str, "access must be text")
    require(row["sample"] in range(3) and row["seed"] == 42 + row["sample"], "wrong fixed seed/sample")
    require(all(row[field] is True for field in FLAGS), "all reopen comparisons must be verified")
    access, costs, fragments, owners, successes, maximum = SPECS[row["case"]]
    expected = dict(access=access, costs=costs, fragments_per_use=fragments, competing_owners=owners,
                    loaded_actors=owners + 1 + row["background_actors"], loaded_regions=16,
                    admitted_uses=owners, successful_starts=successes, failed_starts=owners-successes,
                    successful_resolutions=successes, consumed_units=successes * fragments,
                    prefix_held_fragments=successes * fragments, prefix_claim_owners=successes,
                    prefix_max_item_holds=maximum, prefix_held_units=successes * fragments,
                    final_held_fragments=0, final_claim_owners=0, final_max_item_holds=0, final_held_units=0,
                    item_hold_limit=64, fragment_limit=32, cost_limit=8, plan_count=successes,
                    physical_units_before=successes * fragments, physical_units_at_prefix=successes * fragments,
                    physical_units_after=0, prefix_records=2 * owners,
                    admission_strike_resolutions=0, start_strike_resolutions=0,
                    settlement_strike_resolutions=successes)
    for field, value in expected.items():
        require(row[field] == value, f"{field} disagrees with the fixed case's actual work")
    require(row["setup_records"] == row["loaded_actors"] == row["setup_checkpoint_sequence"],
            "each loaded actor must wait before the real setup checkpoint")
    advances = row["advancement_commands"]
    require(1 <= advances <= 11 * (owners + 1), "impossible bounded settlement command count")
    require(row["final_records"] == row["prefix_records"] + advances, "wrong final accepted prefix")
    for field, count in (("admission_ms", owners), ("start_ms", owners), ("advancement_ms", advances)):
        require(type(row[field]) is list and len(row[field]) == count, f"wrong {field} interval count")
    for field in TIMINGS:
        values = row[field] if field in ARRAY_TIMINGS else [row[field]]
        require(all(is_duration(value) for value in values), f"invalid {field} duration")
    expected_ms = sum(sum(row[field]) for field in ARRAY_TIMINGS)
    require(math.isclose(row["measured_ms"], expected_ms, rel_tol=1e-9, abs_tol=1e-9),
            "measured_ms must include only ordinary admission/start/advancement intervals")
    require(row["plan_max_json_bytes"] <= 16 * 1024, "captured plan exceeds the workload's 16 KiB budget")
    require(row["plan_max_json_bytes"] <= row["plan_total_json_bytes"] <= successes * row["plan_max_json_bytes"],
            "total plan bytes disagree with actual captured plan count")
    for stage in ("prefix", "final"):
        require(row[f"{stage}_checkpoint_json_bytes"] <= 64 * 1024 * 1024,
                "logical checkpoint exceeds the workload's 64 MiB budget")
    checkpoint = row["mode"] == "checkpoint"
    require(row["checkpoint_interval"] == (1 if checkpoint else 4096), "wrong recovery policy")
    for stage in ("prefix", "final"):
        records = row[f"{stage}_records"]
        loaded = row["setup_records"] + records
        require(row[f"{stage}_records_loaded"] == loaded, "wrong reopened accepted record count")
        require(row[f"{stage}_checkpoint_sequence"] == (loaded if checkpoint else row["setup_records"]),
                "reopen must select the real nonzero checkpoint")
        require(row[f"{stage}_records_replayed"] == (0 if checkpoint else records), "wrong bounded replay tail")


def extend(summary, samples):
    summary["seeds"] = sorted(sample["seed"] for sample in samples)
    summary["caution"] = ("Head-only, small n: three fixed seeds; p95 is the maximum. "
                          "Scene-build counters do not by themselves count membership scans. "
                          "64 owners across eight stacks does not exercise 64 holds on one item.")


def summarize(rows, *, smoke=False):
    require(type(smoke) is bool, "smoke selection must be explicit")
    rows = list(rows)
    for row in rows:
        require(type(row) is dict, "workload rows must be objects")
        require(set(row) == FIELDS, "missing or unknown fields; private claim operands are never report fields")
        require(type(row["case"]) is str and type(row["mode"]) is str
                and type(row["background_actors"]) is int, "case dimensions require exact types")
    cases = {case for case in CASES if case[0] in ("ground-c8-f32", "last-unit-2") and case[1] == 0} if smoke else CASES
    return summarize_matrix(rows, matches=lambda r: r["workload"] == "finite-claims" and r["version"] == 1,
                            key_fields=KEYS, cases=cases, check_row=check_row, sample_ids=range(3),
                            timings=TIMINGS, counts=PHYSICAL_BYTES + LOGICAL_BYTES + COUNTERS, extend=extend)


def read_rows(path):
    def object_pairs(pairs):
        value = {}
        for key, item in pairs:
            require(key not in value, f"duplicate JSON key {key!r}")
            value[key] = item
        return value

    result = []
    for index, line in enumerate(Path(path).read_text(encoding="utf-8-sig").splitlines(), 1):
        if not line.strip():
            continue
        try:
            value = json.loads(line, object_pairs_hook=object_pairs)
        except (ValueError, TypeError) as error:
            raise InvalidWorkload(f"invalid JSON on line {index}: {error}") from error
        require(type(value) is dict, f"line {index} must be a JSON object")
        result.append(value)
    require(result, "no workload samples")
    return result


def compare(first, second, *, smoke=False):
    first, second = list(first), list(second)
    summarize(first, smoke=smoke)
    summarize(second, smoke=smoke)
    identity = lambda row: tuple(row[field] for field in KEYS + ("sample",))
    right = {identity(row): row for row in second}
    differences, physical_rows = [], 0
    physical = {field: [] for field in PHYSICAL_BYTES}
    ignored = set(TIMINGS + PHYSICAL_BYTES)
    for row in sorted(first, key=identity):
        match = right[identity(row)]
        changed = sorted(field for field in FIELDS - ignored if row[field] != match[field])
        if changed:
            differences.append(f"{identity(row)}: {', '.join(changed)}")
        physical_rows += any(row[field] != match[field] for field in PHYSICAL_BYTES)
        for field in PHYSICAL_BYTES:
            if row[field] != match[field]:
                physical[field].append(match[field] - row[field])
    require(not differences, "logical repeatability failed: " + "; ".join(differences))
    return dict(logical_equal=True, compared_rows=len(first), physical_difference_rows=physical_rows,
                physical_byte_deltas=physical)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("samples")
    parser.add_argument("--compare", help="a second complete cohort; logical differences fail, physical deltas remain explicit")
    parser.add_argument("--smoke", action="store_true", help="require the complete two-case, zero-background, seed/mode subset")
    args = parser.parse_args()
    rows = read_rows(args.samples)
    result = compare(rows, read_rows(args.compare), smoke=args.smoke) if args.compare else summarize(rows, smoke=args.smoke)
    print(json.dumps(result, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
