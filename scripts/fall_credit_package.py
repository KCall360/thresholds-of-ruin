"""Ordinary authored fall episode with a fixed fatal-owner XP award."""
import subprocess
import tomllib

from arena_matrix import toml_source
from one_cell_package import one_cell_package


def fall_credit_package(test, name):
    package = one_cell_package(test, name, push=True)
    rules_path = package / "rules.toml"
    rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
    rules["growth"].update(
        reward_base=25, reward_per_relative_tier=0,
        minimum_reward=25, maximum_reward=25,
    )
    push = next(row for row in rules["abilities"] if row["id"] == "push_probe")
    push["timing"] = {
        "source": {"kind": "fixed", "prepare": 110, "recover": 40},
        "scaled_by": "speed",
    }
    # Checkless primary and ordinary successful-hit rider: no random fixture hit.
    push["strikes"]["strikes"][0].pop("check", None)
    rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")

    manifest_path = package / "scenario.toml"
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    human = manifest["creatures"]["species"]["human"]
    human["modifiers"] = [
        row for row in human["modifiers"]
        if row["op"].get("stat", {}).get("kind") != "health"
    ]
    human["modifiers"].append({
        "scope": "always",
        "op": {"kind": "set", "stat": {"kind": "health"}, "value": 5},
    })
    manifest["characters"][0]["body"] = {
        "cells": [[0, 0, 0]], "eye": [0, 0, 0], "mass": 80,
    }
    manifest["characters"][0]["creature"]["future_growth"] = {"kind": "disabled"}
    manifest_path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")

    region_path = package / "regions/1.toml"
    region = tomllib.loads(region_path.read_text(encoding="utf-8"))
    region.update(
        size=[9, 9, 8], gravity=[0, 0, -1],
        walls=[[3, 3, 2], [4, 3, 2]],
        anchors={"start": [3, 3, 3], "hall": [8, 3, 3]},
        portals=[], doors=[], places=[], items=[],
    )
    region["actors"][0].update(at=[4, 3, 3], turn_ticks=10)
    region_path.write_text(toml_source(region), encoding="utf-8", newline="\n")
    remote_path = package / "regions/2.toml"
    remote = tomllib.loads(remote_path.read_text(encoding="utf-8"))
    remote["portals"] = []
    remote_path.write_text(toml_source(remote), encoding="utf-8", newline="\n")
    validated = subprocess.run(
        [test.bin / ("tor-scenario" + test.suffix), "validate", package],
        capture_output=True, text=True, encoding="utf-8", timeout=30,
    )
    test.assertEqual(validated.returncode, 0, validated.stderr)
    return package
