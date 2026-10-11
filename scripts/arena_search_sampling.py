"""Reproducible constrained TPE sampling for the offline arena campaign tool."""
import argparse
import copy
import hashlib
import importlib.metadata
import json
import math
import platform
import re
import sys
from pathlib import Path
from arena_evaluation import read_json, seed

FORMAT = "tor-arena-search-sampling-v1"
PINNED = {"optuna": "5.0.0", "alembic": "1.20.0", "colorlog": "6.12.0", "numpy": "2.5.3",
          "packaging": "26.3", "sqlalchemy": "2.1.4", "tqdm": "4.70.1", "PyYAML": "6.0.3",
          "Mako": "1.4.3", "MarkupSafe": "3.0.4", "typing-extensions": "4.16.0", "colorama": "0.4.6"}
NAME = re.compile(r"[a-z][a-z0-9_]{0,63}")


def hash_json(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     allow_nan=False).encode("utf-8")).hexdigest()


def environment():
    try:
        versions = {name: importlib.metadata.version(name) for name in PINNED}
    except importlib.metadata.PackageNotFoundError as error:
        raise ValueError("Install .github/requirements-arena-search.txt with --require-hashes in an isolated environment") from error
    if versions != PINNED:
        raise ValueError("Search dependency versions differ from the pinned environment")
    return {"python": sys.version, "platform": platform.system(), "machine": platform.machine(),
            "packages": versions, "sampling_code_sha256": hashlib.sha256(
                Path(__file__).read_bytes().replace(b"\r\n", b"\n")).hexdigest()}


def validate_space(space):
    if not isinstance(space, dict) or not 1 <= len(space) <= 64:
        raise ValueError("Supply 1..64 search parameters")
    result = {}
    for name, spec in sorted(space.items()):
        if not isinstance(name, str) or not NAME.fullmatch(name) or not isinstance(spec, dict):
            raise ValueError("Invalid search parameter name/definition")
        if spec.get("type") == "integer":
            if (set(spec) != {"type", "low", "high"} or type(spec["low"]) is not int
                    or type(spec["high"]) is not int or not -1000000 <= spec["low"] <= spec["high"] <= 1000000):
                raise ValueError("Invalid integer search bounds")
        elif spec.get("type") == "categorical":
            choices = spec.get("choices")
            if set(spec) != {"type", "choices"} or not isinstance(choices, list) or not 1 <= len(choices) <= 256:
                raise ValueError("Invalid categorical search choices")
            kind = type(choices[0])
            if kind not in (str, bool, int) or any(type(value) is not kind for value in choices):
                raise ValueError("Categorical choices must have one scalar type")
            if len(set(choices)) != len(choices):
                raise ValueError("Duplicate categorical choices")
            if kind is str and any(not value or len(value) > 64 or any(ord(c) < 32 for c in value) for value in choices):
                raise ValueError("Invalid categorical text")
            if kind is int and any(not -1000000 <= value <= 1000000 for value in choices):
                raise ValueError("Invalid categorical integer")
        else:
            raise ValueError("Unknown search parameter type")
        result[name] = copy.deepcopy(spec)
    return result


def finite(value):
    if type(value) not in (int, float) or not math.isfinite(value):
        raise ValueError("Objective and constraint values must be finite numbers")
    return float(value)


def validate_record(record, space):
    if not isinstance(record, dict) or set(record) != {"params", "value", "constraints"}:
        raise ValueError("Invalid trial record")
    params, constraints = record["params"], record["constraints"]
    if not isinstance(params, dict) or set(params) != set(space):
        raise ValueError("Trial parameters differ from the search space")
    for name, spec in space.items():
        value = params[name]
        if spec["type"] == "integer":
            if type(value) is not int or not spec["low"] <= value <= spec["high"]:
                raise ValueError("Trial integer is outside search bounds")
        elif not any(type(value) is type(choice) and value == choice for choice in spec["choices"]):
            raise ValueError("Trial category is outside search choices")
    if not isinstance(constraints, dict) or not 1 <= len(constraints) <= 64:
        raise ValueError("Supply named trial constraints")
    if any(not isinstance(name, str) or not NAME.fullmatch(name) for name in constraints):
        raise ValueError("Invalid constraint name")
    return {"params": copy.deepcopy(params), "value": finite(record["value"]),
            "constraints": {name: finite(value) for name, value in sorted(constraints.items())}}


def initial_state(space, root_seed):
    state = {"format": FORMAT, "seed": seed(root_seed), "space": validate_space(space),
             "environment": environment(), "records": []}
    state["header_sha256"] = hash_json({key: value for key, value in state.items() if key != "records"})
    state["records_sha256"] = hash_json(state["records"])
    return state


def validate_state(state):
    if (not isinstance(state, dict) or set(state) != {"format", "seed", "space", "environment",
                                                   "records", "header_sha256", "records_sha256"}
            or state["format"] != FORMAT):
        raise ValueError("Invalid sampling state format")
    if state["environment"] != environment():
        raise ValueError("Sampling environment differs from retained history")
    seed(state["seed"])
    space = validate_space(state["space"])
    header = {key: state[key] for key in ("format", "seed", "space", "environment")}
    if state["header_sha256"] != hash_json(header):
        raise ValueError("Sampling configuration integrity check failed")
    records = state["records"]
    if not isinstance(records, list) or len(records) > 1000:
        raise ValueError("Sampling history exceeds bounded size")
    if state["records_sha256"] != hash_json(records):
        raise ValueError("Sampling history integrity check failed")
    return space, [validate_record(record, space) for record in records]


def next_proposal(state):
    space, records = validate_state(state)
    import optuna
    optuna.logging.set_verbosity(optuna.logging.ERROR)
    ordinal = len(records)
    if ordinal >= 1000:
        raise ValueError("Sampling history is full")
    sampler_seed = int.from_bytes(hashlib.sha256(b"tor-arena-search-sampler-v1\0"
                      + int(state["seed"]).to_bytes(8, "little")
                      + ordinal.to_bytes(4, "little")).digest()[:4], "little")
    sampler = optuna.samplers.TPESampler(seed=sampler_seed, multivariate=False,
                                         constant_liar=False, n_startup_trials=10, n_ei_candidates=24)
    study = optuna.create_study(direction="maximize", sampler=sampler)
    distributions = {name: (optuna.distributions.IntDistribution(spec["low"], spec["high"])
                            if spec["type"] == "integer" else
                            optuna.distributions.CategoricalDistribution(spec["choices"]))
                     for name, spec in space.items()}
    for record in records:
        study.add_trial(optuna.trial.create_trial(params=record["params"], distributions=distributions,
                        value=record["value"], constraints=record["constraints"]))
    trial = study.ask()
    return {name: (trial.suggest_int(name, spec["low"], spec["high"]) if spec["type"] == "integer"
                   else trial.suggest_categorical(name, spec["choices"])) for name, spec in space.items()}


def append_result(state, params, value, constraints):
    space, _ = validate_state(state)
    record = validate_record({"params": params, "value": value, "constraints": constraints}, space)
    if params != next_proposal(state):
        raise ValueError("Result does not belong to the next seeded proposal")
    result = copy.deepcopy(state)
    result["records"].append(record)
    result["records_sha256"] = hash_json(result["records"])
    return result


def verify_state(state):
    _, records = validate_state(state)
    rebuilt = initial_state(state["space"], state["seed"])
    for record in records:
        rebuilt = append_result(rebuilt, record["params"], record["value"], record["constraints"])
    return next_proposal(rebuilt)


def seed_schedule(root_seed, *, training=20, screening=20, acceptance=200):
    root = int(seed(root_seed))
    result, used = {}, set()
    for phase, count in (("training", training), ("screening", screening), ("acceptance", acceptance)):
        if type(count) is not int or not 1 <= count <= 1000:
            raise ValueError("Stage seed count must be 1..1000")
        values, ordinal = [], 0
        while len(values) < count:
            value = str(int.from_bytes(hashlib.sha256(b"tor-arena-search-seeds-v1\0"
                    + root.to_bytes(8, "little") + phase.encode("ascii") + b"\0"
                    + ordinal.to_bytes(4, "little")).digest()[:8], "little"))
            ordinal += 1
            if value not in used:
                used.add(value)
                values.append(value)
        result[phase] = values
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("state", type=Path, help="Verify recorded proposals and report the next one")
    args = parser.parse_args(argv)
    try:
        print(json.dumps(verify_state(read_json(args.state)), sort_keys=True))
        return 0
    except (ValueError, KeyError, TypeError, OSError) as error:
        print(f"Arena sampling: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
