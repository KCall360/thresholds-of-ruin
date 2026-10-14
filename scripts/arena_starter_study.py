"""Prepare a bounded, paired Warrior/Mage allocation study, separate from diagnostics.

This writes validated packages and existing arena_evaluation plans. It does not
run battles or assert calibration, useful choices, resource pressure or escape.
"""
import argparse
import itertools
import subprocess
import sys
import tomllib
from pathlib import Path

from arena_evaluation import digest, files, seed, timeout_seconds, write_json
from arena_matrix import REQUIRED_HD_LEVELS, STARTER_FAMILIES, encounter, recipe, toml_source
from ruleset_sources import resolve_with_dependencies

FORMAT = "tor-arena-starter-study-v1"
WARRIORS = tuple(name for name in STARTER_FAMILIES if name.startswith("warrior_"))
MAGES = tuple(name for name in STARTER_FAMILIES if name.startswith("mage_"))
DEFAULT_SEEDS = tuple(str(value) for value in range(32))
SOURCE_FILES = ("arena_starter_study.py", "arena_matrix.py", "arena_recipes.json",
                "arena_evaluation.py", "ruleset_sources.py")


def study_cases():
    """27 matchup cells; each class is compared separately with its mixed reference."""
    cases = []
    for warrior, mage, hd, evaluated in itertools.product(WARRIORS, MAGES, REQUIRED_HD_LEVELS,
                                                          ("warrior", "mage")):
        pair = {"warrior": warrior, "mage": mage}
        matchup = f"{warrior}-vs-{mage}-hd{hd}"
        cases.append({"id": matchup + "-evaluate-" + evaluated, "matchup": matchup,
                      **pair, "hit_dice": hd, "evaluated_class": evaluated,
                      "candidate": pair[evaluated], "reference": evaluated + "_mixed",
                      "opponent": pair["mage" if evaluated == "warrior" else "warrior"]})
    return cases


def study_encounter(case, variant, orientation, ruleset):
    if case not in study_cases() or variant not in ("baseline", "candidate"):
        raise ValueError("Unknown starter study case or variant")
    if orientation not in ("forward", "mirrored"):
        raise ValueError("Unknown starter study orientation")
    family = case["reference"] if variant == "baseline" else case["candidate"]
    manifest, region = encounter(family, case["hit_dice"], True, orientation == "mirrored", ruleset)
    opponent, _ = recipe(case["opponent"], case["hit_dice"])
    opponent.update(species="opponent", name="opponent", faction="red")
    manifest["archetypes"]["opponent"]["creature"] = opponent
    manifest["id"] = "starter-study-" + case["id"]
    # Identical natural weapons, geometry and species protection in both arms.
    # No free defensive protection, extra points or synthetic resource capacity.
    manifest["arena"].update(ticks=20000, actions=2000)
    region["name"] = "Starter allocation study"
    return manifest, region


def allocation_spend():
    """Authored portfolio counts, not inferred player-choice preferences."""
    portfolios = []
    for family, hd in itertools.product(STARTER_FAMILIES, REQUIRED_HD_LEVELS):
        build, _ = recipe(family, hd)
        selections = [row["purchases"][0]["selection"] for row in build["hit_dice"]]
        portfolios.append({"family": family, "hit_dice": hd, "shared_points_spent": len(selections),
                           "free_starting_ranks": list(build["starting_skills"]),
                           "paid_ranks": sum(choice["kind"] == "skill_rank" for choice in selections),
                           "paid_talents": sum(choice["kind"] in ("talent", "talent_form") for choice in selections),
                           "selections": selections})
    return portfolios


def run_manifest(seeds=DEFAULT_SEEDS, timeout=30, case_ids=None):
    values = [seed(value) for value in seeds]
    if not 1 <= len(values) <= 1000 or len(values) != len(set(values)):
        raise ValueError("Supply 1..1000 distinct seeds")
    timeout = timeout_seconds(timeout)
    available = study_cases()
    if case_ids is None:
        selected = available
    else:
        requested = list(case_ids)
        known = {case["id"] for case in available}
        if not requested or len(requested) != len(set(requested)) or not set(requested) <= known:
            raise ValueError("Unknown, duplicate or empty starter case selection")
        selected = [case for case in available if case["id"] in requested]
    return {
        "format": FORMAT, "seeds": values, "timeout_seconds": timeout,
        "required_bands": list(REQUIRED_HD_LEVELS), "required_matchups": 27,
        "required_class_relative_comparisons": 54,
        "selected_matchups": len({case["matchup"] for case in selected}),
        "selected_class_relative_comparisons": len(selected),
        "complete_design": len(selected) == len(available),
        "planned_seed_count": len(values), "default_seed_count": len(DEFAULT_SEEDS),
        "pilot": len(selected) < len(available) or len(values) < len(DEFAULT_SEEDS),
        "battle_evidence": "not_run", "calibration": "not_assessed",
        "package_count": len(selected) * 4,
        "expected_runs": len(selected) * len(values) * 4,
        "maximum_run_timeout_seconds": len(selected) * len(values) * 4 * timeout,
        "maximum_replay_timeout_seconds": len(selected) * len(values) * 4 * timeout,
        "caps": {"ticks": 20000, "actions": 2000},
        "sampling_unit": "seed within a matchup and class-relative comparison",
        "nonindependent": ["forward and mirrored orientations",
                           "reused mixed-reference baselines", "overlapping matchup views"],
        "authored_allocation_spend": allocation_spend(),
        "controlled_inputs": {
            "attributes": "same balanced ten-point base and HD attribute choices",
            "weapon": "same natural heavy-weaponry 1d6 Impact, 60/40 base phases",
            "protection": "same humanoid species, no equipment or extra species modifiers",
            "resources": "ordinary fresh creature balances; retain initial report state",
            "nutrition": "no nutrition treatment in this recipe; match any later campaign policy",
            "distance_geometry": "six-cell separation, same open 9x5x2 room",
            "ai": {"memory_ticks": 1000, "flee_percent": 0},
        },
        "evidence_requirements": {
            "existing_report": ["termination types separately", "exact replay", "injury conservation",
                                "User primary check success, not necessarily damaging hit",
                                "positive applied damage", "natural and explicit healing",
                                "per-ability resolutions and applications", "initial and final resources"],
            "pending_instrumentation": ["Target and rider check attempts and roller-relative outcomes",
                                        "causal resource spending, refunds and recovery",
                                        "decision opportunities and affordable-but-unused options",
                                        "action versus strike-resolution concentration"],
            "ordinary_process": ["useful-action contrasts under controlled pressure",
                                 "bounded safe-cell escape and unsafe-route controls",
                                 "ordinary earned-HD growth and paid acquisition"],
            "not_inferred": ["Target success is a damaging hit", "net balance is spending",
                             "ability application proves useful player choice",
                             "all-AI no-flee duel proves escape"],
        },
        "cases": [dict(case, plan=case["id"] + "/plan.json") for case in selected],
    }


def create_study(output, compiler, ruleset, seeds=DEFAULT_SEEDS, timeout=30, case_ids=None):
    inventory = run_manifest(seeds, timeout, case_ids)
    ruleset = Path(ruleset)
    source_hash = digest(ruleset)
    raw_rules = ruleset.read_bytes()
    authored = tomllib.loads(raw_rules.decode("utf-8"))
    compiler = compiler.resolve(strict=True)
    compiler_hash = digest(compiler)
    scripts = Path(__file__).resolve().parent
    generator_inputs = {name: digest(scripts / name) for name in SOURCE_FILES}
    inventory.update(compiler_sha256=compiler_hash, rules_source_sha256=source_hash,
                     generator_inputs=generator_inputs, validated_packages=0,
                     generation_status="resolving_rules")
    output.mkdir(parents=True, exist_ok=False)
    # The partial inventory remains inspectable if compilation times out or rejects a package.
    write_json(output / "partial.json", inventory)
    rules, dependencies = resolve_with_dependencies(authored, compiler)
    materialized = toml_source(rules).encode("utf-8") if dependencies else raw_rules
    inventory.update(ruleset=rules["id"], rules_dependencies=dependencies,
                     generation_status="preparing")
    write_json(output / "partial.json", inventory)
    for case in inventory["cases"]:
        identity = {key: value for key, value in case.items() if key != "plan"}
        plan = {"seeds": inventory["seeds"], "faction": "blue", "timeout_seconds": timeout}
        package_inputs = {}
        case["package_inputs"] = package_inputs
        for variant, orientation in itertools.product(("baseline", "candidate"), ("forward", "mirrored")):
            name = variant + "-" + orientation
            package = output / case["id"] / name
            (package / "regions").mkdir(parents=True)
            manifest, region = study_encounter(identity, variant, orientation, "rules.toml")
            (package / "rules.toml").write_bytes(materialized)
            for path, document in (("scenario.toml", manifest), ("regions/1.toml", region)):
                (package / path).write_text(toml_source(document), encoding="utf-8", newline="\n")
            result = subprocess.run([str(compiler), "validate", str(package)], capture_output=True,
                                    text=True, encoding="utf-8", timeout=30)
            if result.returncode:
                raise ValueError("Invalid starter package " + case["id"] + "/" + name + ": "
                                 + (result.stdout + result.stderr)[-2000:])
            plan.setdefault(variant, {})[orientation] = name
            package_inputs[name] = files(package)
            inventory["validated_packages"] += 1
            write_json(output / "partial.json", inventory)
        write_json(output / case["plan"], plan)
    if (digest(compiler) != compiler_hash or digest(ruleset) != source_hash
            or generator_inputs != {name: digest(scripts / name) for name in SOURCE_FILES}):
        raise ValueError("Starter study source or compiler changed during generation")
    inventory["generation_status"] = "validated"
    write_json(output / "study.json", inventory)
    (output / "partial.json").unlink()
    return inventory


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--ruleset", type=Path,
                        default=Path(__file__).resolve().parents[1] / "scenarios/mob-arena/rules.toml")
    parser.add_argument("--seeds", nargs="+", default=DEFAULT_SEEDS)
    parser.add_argument("--timeout-seconds", type=float, default=30)
    parser.add_argument("--cases", nargs="+", help="Exact comparison IDs for an explicitly incomplete pilot")
    args = parser.parse_args(argv)
    try:
        inventory = create_study(args.output.resolve(), args.compiler, args.ruleset, args.seeds,
                                 args.timeout_seconds, args.cases)
        print(f"Validated {inventory['selected_matchups']}/27 matchup cells, "
              f"{inventory['selected_class_relative_comparisons']}/54 comparisons; "
              f"{inventory['expected_runs']} planned runs, no battles run")
        return 0
    except (ValueError, OSError, subprocess.TimeoutExpired) as error:
        print(f"Starter allocation study: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
