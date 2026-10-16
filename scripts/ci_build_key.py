"""Key the explicit CI build recipe, not verifier/workflow source text.

Cargo and rust-cache remain responsible for dependencies and artifact freshness.
Known Python checks are opaque: builds hidden in their bodies are not discovered.
Unknown top-level composition disables caching; full verification still runs.
"""
import argparse
import hashlib
import json
import sys
from pathlib import Path

import service_contract_ci
import verify


def normalize(value):
    """Remove only the current checkout prefix, preserving flags and suffixes."""
    if not isinstance(value, str):
        raise ValueError("non-string build input")
    root = str(verify.ROOT)
    if value == root:
        return "$WORKSPACE"
    if value.startswith(root + "/") or value.startswith(root + "\\"):
        return "$WORKSPACE" + value[len(root):]
    return value


def recipe(profile, partition):
    rows = []
    steps = verify.plan("full", set(), [], ci_profile=profile, ci_partition=partition)
    for _, command, overrides in steps:
        command = list(command)
        opaque = False
        if len(command) < 2:
            raise ValueError("missing build command")
        if command[0] == sys.executable:
            script = Path(command[1])
            if not script.is_absolute():
                script = verify.ROOT / script
            if script == verify.SCRIPTS / "service_contract_ci.py":
                if command[2:] != ["--profile", profile, "--workspace"]:
                    raise ValueError("unknown Service wrapper composition")
                command = service_contract_ci.cargo_command(profile, workspace=True)
            elif script in (verify.SCRIPTS / "test_runner.py", verify.SCRIPTS / "check_architecture.py"):
                # These are not interpreted as build recipes. Current process
                # apps/tools already have explicit Cargo prerequisites in plan().
                command = ["opaque-python", script.name]
                opaque = True
            else:
                raise ValueError("unknown Python wrapper")
        if not opaque and (command[0] != "cargo" or command[1] not in
                           ("build", "test", "check", "clippy", "doc", "fmt")):
            raise ValueError("unknown build composition")
        # Match run_step's actual fixed/step overrides, without copying inherited
        # credentials. rust-cache already keys its inherited compiler env prefixes.
        environment = verify.verification_environment(1, overrides)
        environment.pop("CARGO_BUILD_JOBS")  # Scheduling, not a compiled variant.
        cwd = verify.ROOT / environment.pop("_cwd", ".")
        rows.append({"command": [normalize(arg) for arg in command], "cwd": normalize(str(cwd)),
                     "environment": {key: normalize(value) for key, value in environment.items()}})
    if not rows:
        raise ValueError("empty build recipe")
    return rows


def cache_key(profile, partition):
    if profile not in ("debug", "release") or partition not in verify.CI_PARTITIONS:
        raise ValueError("unknown CI profile or partition")
    payload = {"version": 1, "profile": profile, "partition": partition,
               # Virtual workspace root is omitted by pinned rust-cache's member
               # hashing. Keep profiles and inherited workspace settings explicit.
               "root_manifest": hashlib.sha256((verify.ROOT / "Cargo.toml").read_bytes()).hexdigest(),
               "recipe": recipe(profile, partition)}
    return hashlib.sha256(json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("debug", "release"), required=True)
    parser.add_argument("--partition", choices=verify.CI_PARTITIONS, required=True)
    args = parser.parse_args(argv)
    try:
        key = cache_key(args.profile, args.partition)
    except (OSError, ValueError, TypeError, KeyError, IndexError):
        print("Dependency cache disabled: build recipe unavailable or unsupported.", file=sys.stderr)
        return 0  # No output key means no restore/save; it never skips verification.
    print("key=" + key)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
