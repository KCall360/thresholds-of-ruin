"""Check the ratified Rogue tuning tolerances against measured evidence.

Tolerances ratified on 29 October 2026 (docs/plans/rogue-tuning.md):

- level on arrival within 1 of the reference model at depths 1-10, within 2 at
  11-26 (pacing model, and campaign levels where runs reached a depth);
- food found against food spent within x1.5 of the model's ratio, per
  five-floor band reached (the model's ratio at the campaign's measured turns
  per floor: pace is play behavior, so only food generation is judged);
- starvation at most 10% of deaths;
- at least 75% of completed runs reach depth 4;
- every species with at least 20 measured attacks within 35% on every judged
  encounter ratio.

    python scripts/rogue_acceptance.py --campaign .local/run-a .local/run-b \\
        --study .local/encounters/study.json
"""
import argparse
import json
from pathlib import Path

import rogue_pacing_model as pacing
import rogue_reference_model as model

MEAL = (1100 + 1499) / 2
MINIMUM_ATTACKS = 20


def level_band(depth):
    return 1 if depth <= 10 else 2


def check_levels(levels, reference):
    misses = {depth: (level, reference[depth]) for depth, level in levels.items()
              if depth in reference and abs(level - reference[depth]) > level_band(depth)}
    return {"pass": not misses, "misses": misses}


def model_food_ratio(turns_per_floor=900):
    food, _ = model.food_per_floor()
    return float(food) * MEAL / turns_per_floor


def check_food(runs):
    """Pooled per five-floor band: food units placed against units spent.

    The model's ratio is taken at the band's measured turns per floor, since a
    slower player spends more food per floor in Rogue as well.
    """
    bands = {}
    for run in runs:
        generation = run.get("item_generation") or {}
        if not generation.get("floors"):
            continue
        for depth, ticks in run["ticks_per_floor"].items():
            band = (int(depth) - 1) // 5
            spent = ticks / 100
            entry = bands.setdefault(band, {"found": 0.0, "spent": 0.0, "floors": 0})
            entry["spent"] += spent
            entry["floors"] += 1
            entry["found"] += generation["food_per_floor"] * MEAL
    results = {}
    for band, entry in sorted(bands.items()):
        pace = entry["spent"] / entry["floors"]
        expected = model_food_ratio(pace) if pace else None
        ratio = entry["found"] / entry["spent"] if entry["spent"] else None
        relative = ratio / expected if ratio is not None else None
        results[f"{band * 5 + 1}-{band * 5 + 5}"] = {
            "ratio": ratio, "turns_per_floor": pace, "model_ratio": expected,
            "relative_to_model": relative,
            "pass": relative is not None and 1 / 1.5 <= relative <= 1.5}
    return {"pass": bool(results) and all(row["pass"] for row in results.values()),
            "bands": results, "model_ratio_at_900_turns": model_food_ratio()}


def starved_to_death(run):
    """The server's named cause; older runs fall back to the hunger evidence."""
    if run.get("cause"):
        return run["cause"] == "starvation"
    # Runs recorded before the cause was named: dying with an empty reserve, or
    # before the hunger at death was kept, any empty spell.
    if "hunger_at_death" in run:
        return run["hunger_at_death"] == "empty"
    return bool(run.get("samples_by_hunger", {}).get("empty"))


def check_starvation(runs):
    deaths = [run for run in runs if run.get("died")]
    starved = [run for run in deaths if starved_to_death(run)]
    share = len(starved) / len(deaths) if deaths else 0.0
    return {"pass": share <= 0.10, "deaths": len(deaths), "starved": len(starved)}


def check_depth(runs):
    completed = [run for run in runs if run.get("outcome") != "harness_error"]
    reached = [run for run in completed if (run.get("deepest_depth") or 0) >= 4]
    share = len(reached) / len(completed) if completed else 0.0
    return {"pass": bool(completed) and share >= 0.75, "completed": len(completed),
            "reached_depth_4": len(reached), "share": share}


def check_species(study):
    failures = {}
    for row in study["species"]:
        counts = row["counts"] or {}
        if counts.get("foe_checks", 0) + counts.get("player_checks", 0) < MINIMUM_ATTACKS:
            continue
        bad = [metric for metric, cell in row["comparison"].items() if cell["within"] is False]
        if bad:
            failures[row["glyph"]] = bad
    return {"pass": not failures, "failures": failures,
            "species": len(study["species"])}


def campaign_levels(runs):
    levels = {}
    for run in runs:
        for depth, level in (run.get("level_on_arrival") or {}).items():
            levels.setdefault(int(depth), []).append(level)
    return {depth: round(sum(values) / len(values)) for depth, values in levels.items()}


def evaluate(runs, study):
    reference = pacing.reference_levels()
    report = {
        "pacing_model_levels": check_levels(pacing.levels_by_depth(pacing.load_growth()), reference),
        "campaign_levels": check_levels(campaign_levels(runs), reference),
        "food": check_food(runs),
        "starvation": check_starvation(runs),
        "depth_4": check_depth(runs),
        "species": check_species(study),
    }
    return report


def apply_waivers(report, waived):
    """Maintainer-waived checks keep their own result and are listed; `pass`
    covers the rest, and `pass_without_waivers` covers every check."""
    unknown = set(waived) - set(report)
    if unknown:
        raise ValueError(f"Unknown checks to waive: {sorted(unknown)}")
    checks = {name: row for name, row in report.items() if isinstance(row, dict)}
    for name in waived:
        checks[name] = {**checks[name], "waived": True}
    return {**checks, "waived": sorted(waived),
            "pass": all(row["pass"] for name, row in checks.items() if name not in waived),
            "pass_without_waivers": all(row["pass"] for row in checks.values())}


def load_runs(directories):
    runs = []
    for directory in directories:
        runs += [json.loads(path.read_text(encoding="utf-8"))
                 for path in sorted(Path(directory).glob("run-*.json"))]
    return runs


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--campaign", nargs="+", type=Path, required=True)
    parser.add_argument("--study", type=Path, required=True)
    parser.add_argument("--waive", nargs="*", default=[],
                        help="Checks the maintainer waived; each keeps its own result")
    args = parser.parse_args(argv)
    study = json.loads(args.study.read_text(encoding="utf-8"))
    report = apply_waivers(evaluate(load_runs(args.campaign), study), args.waive)
    print(json.dumps(report, indent=1, default=str))


if __name__ == "__main__":
    main()
