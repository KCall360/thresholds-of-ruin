"""Ordinary authored supportive-targeting fixture for real clients."""
import copy
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import creature_package


def self_ally_package(test):
    package = creature_package(test, "self-ally-targets", abilities=("mend",),
                               ability_aliases={"mend": ("fixture_mana_bolt", "Mend")})
    path = package / "rules.toml"
    rules = tomllib.loads(path.read_text(encoding="utf-8"))
    rules["constants"]["default_abilities"] = ["basic_melee"]
    ability = next(value for value in rules["abilities"] if value["id"] == "mend")
    ability.pop("cost", None)
    ability.pop("cooldown", None)
    ability["supply_costs"] = []
    ability["reach"] = {"kind": "self_or_ally", "range": {"kind": "touch"}}
    ability["timing"] = {"source": {"kind": "fixed", "prepare": 100, "recover": 100}}
    ability["ai"]["hostile"] = False
    ability["strikes"] = {"kind": "authored", "strikes": [{"offset": 0,
        "on_success": [{"kind": "heal", "amount": {"kind": "fixed", "amount": 1}}],
        "on_failure": [], "descriptors": [], "riders": []}]}
    path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest["factions"] = {"neutral": [], "outsider": []}
    path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    neutral = copy.deepcopy(region["actors"][0])
    neutral.update(id=3, at=[3, 1, 0])
    neutral["creature"].update(name="neutral stranger", faction="outsider")
    region["actors"].append(neutral)
    path.write_text(toml_source(region), encoding="utf-8", newline="\n")
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stderr)
    return package
