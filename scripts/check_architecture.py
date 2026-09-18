"""Enforce reviewed workspace edges, including optional/build/dev dependencies."""

import argparse
import json
from pathlib import Path
import subprocess
import sys


ALLOWED = {
    "tor-world": set(),
    "tor-simulation": {"tor-world"},
    "tor-protocol": set(),
    "tor-server": {"tor-world", "tor-simulation", "tor-protocol"},
    "tor-client-common": {"tor-protocol"},
    "tor-client-ascii": {"tor-client-common", "tor-protocol"},
    "tor-client-text": {"tor-client-common", "tor-protocol"},
    "tor-test-support": {
        "tor-world", "tor-simulation", "tor-protocol", "tor-server",
        "tor-client-common", "tor-client-ascii", "tor-client-text",
    },
}


def violations(metadata):
    members = set(metadata["workspace_members"])
    packages = [p for p in metadata["packages"] if p["id"] in members]
    names = {p["name"] for p in packages}
    errors = []
    for package in packages:
        name = package["name"]
        if name not in ALLOWED:
            errors.append(f"{name} has no dependency policy")
            continue
        for dependency in package["dependencies"]:
            target = dependency["name"]  # Cargo preserves the real name for aliases.
            if target in names or dependency.get("path") is not None:
                if target not in ALLOWED[name]:
                    errors.append(f"{name} -> {target} is forbidden")
    return sorted(set(errors))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo", default="cargo", help="Path to the Cargo executable")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    result = subprocess.run(
        [args.cargo, "metadata", "--format-version", "1", "--no-deps", "--locked"],
        cwd=root, check=True, capture_output=True, text=True, encoding="utf-8",
    )
    errors = violations(json.loads(result.stdout))
    if errors:
        print("Dependency boundary violations:\n" + "\n".join(errors), file=sys.stderr)
        return 1
    print("Workspace dependency boundaries passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
