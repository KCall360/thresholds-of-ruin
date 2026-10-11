"""Apply bounded candidate parameters to preserved arena package snapshots."""
import argparse
import copy
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tomllib
from arena_evaluation import digest, files, read_json, seed, timeout_seconds, write_json
from arena_matrix import toml_source
from arena_search_sampling import hash_json, validate_record, validate_space

FORMAT = "tor-arena-search-parameters-v1"
BUILD_FIELDS = {"attributes", "binding", "species", "templates", "hit_dice"}


def permitted(file, path):
    if file == "scenario.toml":
        if len(path) >= 4 and path[0] == "creatures" and path[1] in ("species", "templates"):
            return True
        if (len(path) >= 4 and path[0] == "characters" and type(path[1]) is int
                and path[2] == "creature" and path[3] in BUILD_FIELDS):
            return True
        if (len(path) >= 4 and path[0] == "archetypes" and isinstance(path[1], str)
                and path[2] == "creature" and path[3] in BUILD_FIELDS):
            return True
        return len(path) == 3 and path[0] == "ai_profiles" and path[2] in ("memory_ticks", "flee_percent")
    return (len(path) >= 4 and path[0] == "actors" and type(path[1]) is int
            and path[2] == "creature" and path[3] in BUILD_FIELDS)


def validate_spec(value):
    if not isinstance(value, dict) or set(value) != {"format", "parameters"} or value["format"] != FORMAT:
        raise ValueError("Invalid parameter definition format")
    parameters = value["parameters"]
    if not isinstance(parameters, dict):
        raise ValueError("Invalid parameter definitions")
    if any(not isinstance(spec, dict) or set(spec) != {"space", "targets"} for spec in parameters.values()):
        raise ValueError("Supply a space and targets for every parameter")
    space = validate_space({name: spec["space"] for name, spec in parameters.items()})
    seen = []
    for spec in parameters.values():
        targets = spec["targets"]
        if not isinstance(targets, list) or not 1 <= len(targets) <= 64:
            raise ValueError("Supply 1..64 targets per parameter")
        for target in targets:
            if not isinstance(target, dict) or set(target) != {"file", "path"}:
                raise ValueError("Invalid parameter target")
            file, path = target["file"], target["path"]
            if not isinstance(file, str) or len(file) > 256:
                raise ValueError("Invalid parameter filename")
            parsed = PurePosixPath(file)
            if (parsed.is_absolute() or ".." in parsed.parts or "\\" in file
                    or str(parsed) != file or (file != "scenario.toml" and
                    not (len(parsed.parts) == 2 and parsed.parts[0] == "regions" and parsed.suffix == ".toml"))):
                raise ValueError("Target must be the manifest or a direct region TOML file")
            if (not isinstance(path, list) or not 1 <= len(path) <= 16 or any(
                not ((type(key) is int and 0 <= key <= 10000) or
                     (type(key) is str and 0 < len(key) <= 128)) for key in path)):
                raise ValueError("Invalid parameter target path")
            if not permitted(file, path):
                raise ValueError("Parameters cannot change geometry, identity, control, participants or limits")
            for previous_file, previous in seen:
                if file == previous_file and (path[:len(previous)] == previous or previous[:len(path)] == path):
                    raise ValueError("Conflicting parameter target paths")
            seen.append((file, path))
    return copy.deepcopy(value), space


def apply_document(document, file, specification, params):
    specification, space = validate_spec(specification)
    validate_record({"params": params, "value": 0, "constraints": {"validation": 0}}, space)
    result = copy.deepcopy(document)
    for name, spec in specification["parameters"].items():
        for target in spec["targets"]:
            if target["file"] != file:
                continue
            path = target["path"]
            try:
                parent = result
                for key in path[:-1]:
                    if (isinstance(parent, list) and type(key) is not int) or (isinstance(parent, dict) and type(key) is not str):
                        raise ValueError("Target container/key type mismatch")
                    parent = parent[key]
                key = path[-1]
                if (isinstance(parent, list) and type(key) is not int) or (isinstance(parent, dict) and type(key) is not str):
                    raise ValueError("Target container/key type mismatch")
                original = parent[key]
                if type(original) not in (int, str, bool) or type(original) is not type(params[name]):
                    raise ValueError("Parameters can only replace existing scalars of the same type")
                parent[key] = params[name]
            except (KeyError, IndexError, TypeError) as error:
                raise ValueError("Target path does not exist in candidate package") from error
    return result


def materialize(compiler, plan_path, specification, params, output, *, seeds=None, resume=False):
    specification, space = validate_spec(specification)
    validate_record({"params": params, "value": 0, "constraints": {"validation": 0}}, space)
    plan = read_json(plan_path)
    if (not isinstance(plan, dict) or set(plan) - {"baseline", "candidate", "seeds", "faction", "timeout_seconds"}
            or not isinstance(plan.get("faction"), str) or not plan["faction"]):
        raise ValueError("Invalid paired plan")
    values = plan["seeds"] if seeds is None else seeds
    if not isinstance(values, list) or not 1 <= len(values) <= 1000:
        raise ValueError("Supply 1..1000 seeds")
    values = [seed(value) for value in values]
    if len(set(values)) != len(values):
        raise ValueError("Duplicate seeds")
    timeout = timeout_seconds(plan.get("timeout_seconds", 60))
    sources = {}
    output = output.resolve()
    for variant in ("baseline", "candidate"):
        if not isinstance(plan[variant], dict) or set(plan[variant]) != {"forward", "mirrored"}:
            raise ValueError("Supply both orientations")
        for orientation in ("forward", "mirrored"):
            source = (plan_path.parent / plan[variant][orientation]).resolve()
            if source == output or source in output.parents:
                raise ValueError("Output must be outside every source package")
            sources[variant + "-" + orientation] = (source, files(source))
    compiler = compiler.resolve(strict=True)
    fingerprint = digest(compiler)
    result = {"format": "tor-arena-search-candidate-v1", "params": copy.deepcopy(params),
              "parameter_spec": specification, "parameter_spec_sha256": hash_json(specification),
              "compiler_sha256": fingerprint, "source_files": {key: value[1] for key, value in sources.items()},
              "resolved_files": {}, "status": "valid", "validation_failures": []}
    paired = {"seeds": values, "faction": plan["faction"], "timeout_seconds": timeout,
              "baseline": {}, "candidate": {}}
    for key in sources:
        variant, orientation = key.split("-", 1)
        paired[variant][orientation] = key
    header = {"candidate": result, "plan": paired}
    checkpoint_path = output / "materialization.json"
    if resume:
        checkpoint = read_json(checkpoint_path)
        if (set(checkpoint) != {"header", "header_sha256", "packages", "packages_sha256"}
                or checkpoint["header"] != header or checkpoint["header_sha256"] != hash_json(header)):
            raise ValueError("Materialization inputs differ from checkpoint")
        packages = checkpoint["packages"]
        if (not isinstance(packages, dict) or checkpoint["packages_sha256"] != hash_json(packages)
                or list(packages) != sorted(list(sources)[:len(packages)])):
            raise ValueError("Invalid validated package checkpoint")
        for key, completed in packages.items():
            if (set(completed) != {"resolved_files", "failure"}
                    or files(output / key) != completed["resolved_files"]):
                raise ValueError("Previously validated package differs from checkpoint")
    else:
        output.mkdir(parents=True, exist_ok=False)
        packages = {}
        checkpoint = {"header": copy.deepcopy(header), "header_sha256": hash_json(header),
                      "packages": packages, "packages_sha256": hash_json(packages)}
        write_json(checkpoint_path, checkpoint)
    for key, (source, source_files) in sources.items():
        if key in packages:
            continue
        variant = key.split("-", 1)[0]
        target = output / key
        restore_package(source, source_files, target, output)
        if variant == "candidate":
            filenames = sorted({entry["file"] for spec in specification["parameters"].values()
                                for entry in spec["targets"]})
            for filename in filenames:
                path = target / filename
                document = tomllib.loads(path.read_text(encoding="utf-8"))
                changed = apply_document(document, filename, specification, params)
                temporary = output / ".materializing"
                if temporary.is_symlink():
                    raise ValueError("Materialization temporary file cannot be a symlink")
                temporary.write_text(toml_source(changed), encoding="utf-8", newline="\n")
                temporary.replace(path)
        validation = subprocess.run([str(compiler), "validate", str(target)], capture_output=True,
                                    text=True, encoding="utf-8", timeout=30)
        if digest(compiler) != fingerprint:
            raise ValueError("Compiler changed during candidate validation")
        if files(source) != source_files:
            raise ValueError("Source changed during candidate snapshot")
        failure = ({"package": key, "variant": variant,
                    "message": (validation.stdout + validation.stderr)[-2000:]}
                   if validation.returncode else None)
        packages[key] = {"resolved_files": files(target), "failure": failure}
        checkpoint["packages_sha256"] = hash_json(packages)
        write_json(checkpoint_path, checkpoint)
    for key in sources:
        completed = packages[key]
        result["resolved_files"][key] = completed["resolved_files"]
        if completed["failure"] is not None:
            result["status"] = "invalid"
            result["validation_failures"].append(completed["failure"])
    if digest(compiler) != fingerprint:
        raise ValueError("Compiler changed during candidate validation")
    publish_consistent(output / "candidate.json", result)
    if result["status"] == "valid":
        publish_consistent(output / "plan.json", paired)
    elif (output / "plan.json").exists():
        raise ValueError("Invalid candidate must not have an executable plan")
    return result


def restore_package(source, fingerprint, target, output):
    """Reconstruct only an unfinished package from fingerprinted source files.

    Completed packages are verified and left untouched. A single owned staging
    file makes each unfinished file publication atomic without recursive deletion.
    """
    if target.is_symlink() or output.is_symlink():
        raise ValueError("Materialization directories cannot be symlinks")
    for path in target.rglob("*"):
        if path.is_symlink() or (path.is_file() and path.relative_to(target).as_posix() not in fingerprint):
            raise ValueError("Unexpected unfinished package file")
    if files(source) != fingerprint:
        raise ValueError("Source changed during candidate snapshot")
    target.mkdir(parents=True, exist_ok=True)
    temporary = output / ".materializing"
    if temporary.is_symlink():
        raise ValueError("Materialization temporary file cannot be a symlink")
    for filename in fingerprint:
        path = target / filename
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source / filename, temporary)
        temporary.replace(path)
    if files(target) != fingerprint:
        raise ValueError("Source changed during candidate snapshot")


def publish_consistent(path, value):
    if path.exists():
        if read_json(path) != value:
            raise ValueError("Published candidate evidence differs from checkpoint")
    else:
        write_json(path, value)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("compiler", "plan", "spec", "values", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--seeds", nargs="+")
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args(argv)
    try:
        result = materialize(args.compiler, args.plan.resolve(), read_json(args.spec),
                             read_json(args.values), args.output, seeds=args.seeds, resume=args.resume)
        print(result["status"])
        return 0 if result["status"] == "valid" else 1
    except (ValueError, KeyError, TypeError, OSError, subprocess.TimeoutExpired) as error:
        print(f"Arena parameters: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
