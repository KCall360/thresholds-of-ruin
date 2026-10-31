"""Shared real-process fixtures for owned advancement acceptance tests."""
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import ROOT
from ruleset_sources import load_rules

PACKAGE = ROOT / "scenarios/tests/advancement-owner"


def observation(frame):
    return frame["state"]["observation"]


def earned_growth_package(test):
    """A normal authored fatal-effect encounter with deterministic provisional XP."""
    package = test.directory / "earned-growth"
    shutil.copytree(PACKAGE, package)
    compiler = test.bin / ("tor-scenario" + test.suffix)
    rules = load_rules(package / "rules.toml", compiler)
    rules["growth"].update(threshold_base=100, threshold_per_tier=0, reward_base=100,
                           reward_per_relative_tier=0, minimum_reward=100, maximum_reward=100)
    (package / "rules.toml").write_text(toml_source(rules), encoding="utf-8", newline="\n")
    manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
    manifest["id"] = "earned-growth"
    manifest["factions"] = {"allies": ["opponents"], "opponents": ["allies"]}
    build = manifest["characters"][0]["creature"]
    build["hit_dice"] = [{"source": "racial", "purchases": []}]
    build["future_growth"] = {"kind": "disabled"}
    melee = manifest["creatures"]["species"]["learner"]["melee"]
    melee["strikes"][0]["on_success"][0]["damage"]["components"][0]["amount"]["amount"] = 100
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    region_file = package / "regions/1.toml"
    region = tomllib.loads(region_file.read_text(encoding="utf-8"))
    region["actors"][0]["creature"]["faction"] = "opponents"
    region_file.write_text(toml_source(region), encoding="utf-8", newline="\n")
    validated = subprocess.run([str(compiler), "validate", str(package)], capture_output=True,
                               text=True, encoding="utf-8", timeout=60)
    test.assertEqual(validated.returncode, 0, validated.stderr)
    return package
