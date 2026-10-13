"""Shared normally compiled ordered-strike process fixture."""
import copy
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import ROOT
from ruleset_sources import load_rules


def sequence_package(test, *, autonomous):
    """Authored fixed-time checkless strikes, compiled by the normal validator."""
    package = test.directory / ("sequence-ai" if autonomous else "sequence-controlled")
    shutil.copytree(ROOT / "scenarios/two-room", package)
    compiler = test.bin / ("tor-scenario" + test.suffix)
    rules = load_rules(package / "rules.toml", compiler)
    strike = {"offset": 0, "on_success": [{"kind": "damage", "damage": {
        "kind": "components", "components": [{"category": "impact", "amount": {
            "kind": "fixed", "amount": 1}}]}}], "on_failure": [], "descriptors": [], "riders": []}
    strikes = [dict(copy.deepcopy(strike), offset=offset) for offset in (0, 100, 200)]
    rules["abilities"].append({"id": "triple_strike", "name": "Triple Strike", "tags": [],
        "basic_attack": False, "timing": {"source": {"kind": "fixed", "prepare": 100, "recover": 100},
        "scaled_by": "speed"}, "reach": {"kind": "melee"}, "strikes": {"kind": "authored", "strikes": strikes},
        "ai": {"priority": 100, "hostile": True, "minimum_resource_percent": 0, "range_preference": "any"}})
    (package / "rules.toml").write_text(toml_source(rules), encoding="utf-8", newline="\n")
    manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
    manifest["id"] = package.name
    manifest["factions"] = {"neutral": [], "foe": ["neutral"]}
    manifest["ai_profiles"] = {"sequence": {"memory_ticks": 1000, "flee_percent": 0}}
    species = {"kind": "humanoid", "attributes": dict.fromkeys(
        ("strength", "speed", "intellect", "willpower", "awareness", "presence"), 0),
        "melee": {"skill": "heavy_weaponry", "bonus": 0, "wind_up": 100, "recovery": 100,
                  "strikes": [copy.deepcopy(strike)]},
        "modifiers": [{"scope": "always", "op": {"kind": "grant_ability", "ability": "triple_strike"}},
                      {"scope": "always", "op": {"kind": "add", "stat": {"kind": "health"}, "value": 200}}]}
    manifest["creatures"] = {"species": {"sequence_fighter": species}}

    def creature(name, faction):
        return {"species": "sequence_fighter", "name": name, "faction": faction, "binding": "intellect",
                "starting_skills": ["heavy_weaponry", "athletics"], "restoration": "authored",
                "hit_dice": [{"source": "racial"}]}

    manifest["characters"][0]["creature"] = creature("Escaper", "neutral")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    region["items"] = []
    sentinel = {"id": 2, "at": [2, 1, 0], "turn_ticks": 150,
                "controller": "ai" if autonomous else "external",
                "creature": creature("Sequence sentinel", "foe" if autonomous else "neutral")}
    if autonomous:
        sentinel["ai"] = "sequence"
    region["actors"] = [sentinel]
    path.write_text(toml_source(region), encoding="utf-8", newline="\n")
    validated = subprocess.run([compiler, "validate", package], capture_output=True,
                               text=True, encoding="utf-8", timeout=30)
    test.assertEqual(validated.returncode, 0, validated.stderr)
    return package
