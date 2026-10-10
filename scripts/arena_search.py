"""Sequential seeded arena training, independent screening and acceptance."""
import argparse
import copy
import hashlib
from pathlib import Path
import subprocess
import sys
from arena_evaluation import (FORMAT as BATCH_FORMAT, ORIENTATIONS, VARIANTS, chain_hash,
                              digest, files, read_json, resume_batch, run_batch, seed,
                              timeout_seconds, write_json)
from arena_search_parameters import materialize, publish_consistent, restore_package, validate_spec
from arena_search_sampling import (NAME, append_result, environment, finite, hash_json,
                                   initial_state, next_proposal, seed_schedule, verify_state)

FORMAT = "tor-arena-search-v1"
RESERVED = {"validation", "failures", "coverage"}
DEFAULTS = {"candidates": 50, "training_seeds": 20, "screening_seeds": 20,
            "acceptance_seeds": 200, "maximum_excluded_fraction": 0,
            "minimum_acceptance_difference": 0, "constraints": []}


def validate_config(value, space):
    required = {"format", "plan", "parameter_spec", "seed"}
    if (not isinstance(value, dict) or not required <= set(value)
            or set(value) - required - set(DEFAULTS) or value["format"] != FORMAT):
        raise ValueError("Invalid campaign configuration")
    result = {**copy.deepcopy(DEFAULTS), **copy.deepcopy(value)}
    seed(result["seed"])
    for key in ("plan", "parameter_spec"):
        if not isinstance(result[key], str) or not 0 < len(result[key]) <= 1024:
            raise ValueError("Supply campaign input paths")
    for key in ("candidates", "training_seeds", "screening_seeds", "acceptance_seeds"):
        if type(result[key]) is not int or not (3 if key == "candidates" else 1) <= result[key] <= (999 if key == "candidates" else 1000):
            raise ValueError("Invalid campaign count")
    for key, low, high in (("maximum_excluded_fraction", 0, 1), ("minimum_acceptance_difference", -1, 1)):
        if not low <= finite(result[key]) <= high:
            raise ValueError("Invalid campaign threshold")
    constraints = result["constraints"]
    if not isinstance(constraints, list) or len(constraints) > 60:
        raise ValueError("Supply at most 60 linear budget constraints")
    names = set(RESERVED)
    for constraint in constraints:
        if not isinstance(constraint, dict) or set(constraint) != {"name", "coefficients", "maximum"}:
            raise ValueError("Invalid linear budget constraint")
        name, coefficients = constraint["name"], constraint["coefficients"]
        if not isinstance(name, str) or not NAME.fullmatch(name) or name in names:
            raise ValueError("Invalid or duplicate constraint name")
        names.add(name)
        if not isinstance(coefficients, dict) or not coefficients or not set(coefficients) <= set(space):
            raise ValueError("Budget refers to unknown parameters")
        if any(space[key]["type"] != "integer" or type(value) is not int or not -1000000 <= value <= 1000000
               for key, value in coefficients.items()):
            raise ValueError("Budgets require bounded integer coefficients and parameters")
        if type(constraint["maximum"]) is not int or not -1000000 <= constraint["maximum"] <= 1000000:
            raise ValueError("Invalid budget maximum")
    return result


def trial_record(config, params, summary, validated, count):
    constraints = {"validation": int(not validated), "failures": 0, "coverage": 1}
    value = 0
    if summary is not None:
        value = summary["mean_candidate_win_difference"]
        constraints["failures"] = sum(summary["terminations"][variant].get("failure", 0) for variant in VARIANTS)
        constraints["coverage"] = (len(summary["excluded_seeds"]) / count - config["maximum_excluded_fraction"]
                                   if summary["paired_seeds"] else 1)
    for budget in config["constraints"]:
        constraints[budget["name"]] = sum(params[name] * coefficient for name, coefficient
                                         in budget["coefficients"].items()) - budget["maximum"]
    return {"params": copy.deepcopy(params), "value": value if value is not None else 0,
            "constraints": constraints}


def feasible(record):
    return all(value <= 0 for value in record["constraints"].values())


def finalists(records):
    selected, seen = [], set()
    for ordinal in sorted(range(len(records)), key=lambda i: (-records[i]["value"], i)):
        record = records[ordinal]
        identity = hash_json(record["params"])
        if feasible(record) and identity not in seen:
            selected.append(ordinal)
            seen.add(identity)
            if len(selected) == 3:
                return selected
    raise ValueError("Training needs three distinct feasible candidates")


def module_hashes():
    return {name: hashlib.sha256(Path(__file__).with_name(name).read_text(encoding="utf-8")
                                .replace("\r\n", "\n").encode("utf-8")).hexdigest()
            for name in ("arena_search.py", "arena_search_sampling.py", "arena_search_parameters.py",
                         "arena_evaluation.py", "arena_matrix.py")}


def initialize(binary, compiler, config_path, output):
    raw = read_json(config_path)
    spec, space = validate_spec(read_json((config_path.parent / raw["parameter_spec"]).resolve()))
    config = validate_config(raw, space)
    path = (config_path.parent / config["plan"]).resolve()
    plan = read_json(path)
    if (not isinstance(plan, dict) or set(plan) - {"baseline", "candidate", "seeds", "faction", "timeout_seconds"}
            or not isinstance(plan.get("faction"), str) or not plan["faction"]):
        raise ValueError("Invalid campaign paired plan")
    sources = {}
    for variant in VARIANTS:
        if not isinstance(plan[variant], dict) or set(plan[variant]) != set(ORIENTATIONS):
            raise ValueError("Supply both encounter orientations")
        for orientation in ORIENTATIONS:
            source = (path.parent / plan[variant][orientation]).resolve()
            if source == output or source in output.parents:
                raise ValueError("Campaign output must be outside source packages")
            sources[f"{variant}-{orientation}"] = {"path": str(source), "files": files(source)}
        if sources[f"{variant}-forward"]["files"] == sources[f"{variant}-mirrored"]["files"]:
            raise ValueError("Supply distinct mirrored encounter inputs")
    header = {"format": FORMAT, "config": config, "parameter_spec": spec, "space": space,
              "seeds": seed_schedule(config["seed"], training=config["training_seeds"],
                                     screening=config["screening_seeds"], acceptance=config["acceptance_seeds"]),
              "binary_sha256": digest(binary), "compiler_sha256": digest(compiler),
              "environment": environment(), "modules": module_hashes(), "sources": sources,
              "faction": plan["faction"], "timeout_seconds": timeout_seconds(plan.get("timeout_seconds", 60))}
    output.mkdir(parents=True, exist_ok=False)
    write_json(output / "inputs.json", {"header": header, "sha256": hash_json(header)})
    return header


def retained_header(binary, compiler, output):
    retained = read_json(output / "inputs.json")
    header = retained["header"]
    if (set(retained) != {"header", "sha256"} or retained["sha256"] != hash_json(header)
            or header["format"] != FORMAT or header["modules"] != module_hashes()
            or header["environment"] != environment() or header["binary_sha256"] != digest(binary)
            or header["compiler_sha256"] != digest(compiler)):
        raise ValueError("Campaign inputs, tools, implementation or environment differ")
    spec, space = validate_spec(header["parameter_spec"])
    if (space != header["space"] or spec != header["parameter_spec"]
            or validate_config(header["config"], space) != header["config"]):
        raise ValueError("Invalid retained campaign configuration")
    keys = {f"{variant}-{orientation}" for variant in VARIANTS for orientation in ORIENTATIONS}
    if set(header["sources"]) != keys:
        raise ValueError("Invalid campaign input inventory")
    expected = seed_schedule(header["config"]["seed"], training=header["config"]["training_seeds"],
                             screening=header["config"]["screening_seeds"], acceptance=header["config"]["acceptance_seeds"])
    if header["seeds"] != expected:
        raise ValueError("Campaign seed schedule differs")
    return header



def safe_directory(path, root):
    if path != root and root not in path.parents:
        raise ValueError("Campaign directory escaped its output root")
    current = path
    while True:
        if current.is_symlink():
            raise ValueError("Campaign directories cannot be symlinks")
        if current == root:
            break
        current = current.parent

def prepare_inputs(header, output):
    safe_directory(output / "inputs", output)
    ready = output / "ready.json"
    if ready.exists():
        if read_json(ready) != {"inputs_sha256": hash_json(header)}:
            raise ValueError("Campaign input readiness differs")
        for key, source in header["sources"].items():
            if files(output / "inputs" / key) != source["files"]:
                raise ValueError("Campaign snapshot differs")
    else:
        for key, source in header["sources"].items():
            restore_package(Path(source["path"]), source["files"], output / "inputs" / key, output)
        publish_consistent(ready, {"inputs_sha256": hash_json(header)})
    plan = {"faction": header["faction"], "timeout_seconds": header["timeout_seconds"],
            "seeds": header["seeds"]["training"]}
    for variant in VARIANTS:
        plan[variant] = {orientation: f"inputs/{variant}-{orientation}" for orientation in ORIENTATIONS}
    publish_consistent(output / "plan.json", plan)


def recover_unstarted_batch(binary, plan_path, output):
    """Recover initial snapshot publication before any run could be admitted."""
    safe_directory(output / "inputs", output)
    safe_directory(output / "runs", output)
    if (output / "runs").exists() and any((output / "runs").iterdir()):
        raise ValueError("Batch without checkpoint contains run records")
    plan = read_json(plan_path)
    inputs = {}
    for variant in VARIANTS:
        for orientation in ORIENTATIONS:
            key = f"{variant}-{orientation}"
            source = (plan_path.parent / plan[variant][orientation]).resolve()
            inputs[key] = files(source)
            restore_package(source, inputs[key], output / "inputs" / key, output)
    (output / "runs").mkdir(exist_ok=True)
    progress = {"format": BATCH_FORMAT, "binary_sha256": digest(binary), "inputs": inputs,
                "faction": plan["faction"], "seeds": plan["seeds"], "timeout_seconds": plan["timeout_seconds"]}
    progress.update(completed_runs=0, chain_sha256=chain_hash(progress), pending=None)
    write_json(output / "partial.json", progress)


def evaluate(binary, compiler, header, root, stage, ordinal, params):
    directory = root / stage / f"{ordinal:04d}"
    safe_directory(directory, root)
    directory.mkdir(parents=True, exist_ok=True)
    prepared = directory / "candidate"
    safe_directory(prepared, root)
    if prepared.exists() and not (prepared / "materialization.json").exists():
        prepared.rmdir()  # Only an empty directory from interrupted initial publication.
    build = materialize(compiler, root / "plan.json", header["parameter_spec"], params,
                        prepared, seeds=header["seeds"][stage], resume=prepared.exists())
    if any(failure["variant"] == "baseline" for failure in build["validation_failures"]):
        raise ValueError("Baseline compiler validation failed; fix campaign configuration")
    summary = None
    if build["status"] == "valid":
        batch_path = directory / "batch"
        safe_directory(batch_path, root)
        if batch_path.exists():
            if not (batch_path / "partial.json").exists() and not (batch_path / "batch.json").exists():
                recover_unstarted_batch(binary, prepared / "plan.json", batch_path)
            batch = resume_batch(binary, batch_path)
        else:
            batch = run_batch(binary, prepared / "plan.json", batch_path)
        summary = batch["summary"]
        if summary["terminations"]["baseline"].get("failure", 0):
            raise ValueError("Baseline Engine evaluation failed; fix campaign configuration")
    record = trial_record(header["config"], params, summary, build["status"] == "valid", len(header["seeds"][stage]))
    evidence = {"stage": stage, "ordinal": ordinal, "seeds": header["seeds"][stage], "record": record,
                "summary": summary, "validation_failures": build["validation_failures"]}
    publish_consistent(directory / "result.json", evidence)
    return record, evidence


def export_candidate(header, root, ordinal, params, acceptance):
    source = root / "acceptance" / f"{ordinal:04d}" / "candidate"
    target = root / "review"
    safe_directory(target, root)
    target.mkdir(exist_ok=True)
    evidence = {"format": FORMAT, "training_ordinal": ordinal, "params": params,
                "inputs_sha256": hash_json(header), "acceptance": acceptance,
                "files": {key: files(source / key) for key in header["sources"]},
                "promotion": "manual_review_required"}
    complete = (target / "candidate.json").exists()
    for key, fingerprint in evidence["files"].items():
        if complete:
            if files(target / key) != fingerprint:
                raise ValueError("Review export differs from retained candidate")
        else:
            restore_package(source / key, fingerprint, target / key, root)
    publish_consistent(target / "plan.json", read_json(source / "plan.json"))
    publish_consistent(target / "candidate.json", evidence)
    return "review/candidate.json"


def run_campaign(binary, compiler, output, *, config_path=None):
    safe_directory(output, output)
    header = (initialize(binary, compiler, config_path, output) if config_path is not None
              else retained_header(binary, compiler, output))
    prepare_inputs(header, output)
    config = header["config"]
    history_path = output / "history.json"
    history = read_json(history_path) if history_path.exists() else None
    if history is not None:
        verify_state(history)
        if len(history["records"]) > config["candidates"]:
            raise ValueError("History exceeds campaign trial count")
    state = initial_state(header["space"], config["seed"])
    if history is not None and {key: value for key, value in history.items() if key not in ("records", "records_sha256")} != {
            key: value for key, value in state.items() if key not in ("records", "records_sha256")}:
        raise ValueError("History configuration differs from campaign")
    for ordinal in range(config["candidates"]):
        params = next_proposal(state)
        record, _ = evaluate(binary, compiler, header, output, "training", ordinal, params)
        state = append_result(state, params, record["value"], record["constraints"])
        if history is not None and ordinal < len(history["records"]):
            if state["records"][-1] != history["records"][ordinal]:
                raise ValueError("History differs from retained training evidence")
        else:
            write_json(history_path, state)
        print(f"training {ordinal + 1}/{config['candidates']}", flush=True)
    report = {"format": FORMAT, "inputs_sha256": hash_json(header), "training_trials": len(state["records"])}
    try:
        selected = finalists(state["records"])
    except ValueError:
        report.update(status="insufficient_feasible_candidates", finalists=[])
        publish_consistent(output / "campaign.json", report)
        return report
    report["finalists"] = selected
    screening = []
    for ordinal in selected:
        record, evidence = evaluate(binary, compiler, header, output, "screening", ordinal, state["records"][ordinal]["params"])
        screening.append(evidence)
        print(f"screening trial {ordinal}", flush=True)
    report["screening"] = screening
    eligible = [evidence for evidence in screening if feasible(evidence["record"])]
    if not eligible:
        report["status"] = "screening_infeasible"
        publish_consistent(output / "campaign.json", report)
        return report
    winner = min(eligible, key=lambda evidence: (-evidence["record"]["value"], evidence["ordinal"]))
    ordinal, params = winner["ordinal"], winner["record"]["params"]
    record, acceptance = evaluate(binary, compiler, header, output, "acceptance", ordinal, params)
    accepted = feasible(record) and record["value"] >= config["minimum_acceptance_difference"]
    report.update(winner=ordinal, acceptance=acceptance, status="accepted" if accepted else "acceptance_failed")
    # Failed acceptance can still be inspected; no source definition is promoted.
    if acceptance["summary"] is not None:
        report["export"] = export_candidate(header, output, ordinal, params, acceptance)
    retained_header(binary, compiler, output)
    publish_consistent(output / "campaign.json", report)
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--compiler", type=Path, required=True)
    commands = parser.add_subparsers(dest="operation", required=True)
    start = commands.add_parser("run")
    start.add_argument("config", type=Path)
    start.add_argument("output", type=Path)
    resume = commands.add_parser("resume")
    resume.add_argument("output", type=Path)
    args = parser.parse_args(argv)
    try:
        result = run_campaign(args.binary.resolve(strict=True), args.compiler.resolve(strict=True),
                              args.output.resolve(), config_path=args.config.resolve() if args.operation == "run" else None)
        print(result["status"], flush=True)
        return 0 if result["status"] == "accepted" else 1
    except (ValueError, KeyError, TypeError, OSError, subprocess.TimeoutExpired) as error:
        print(f"Arena search: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
