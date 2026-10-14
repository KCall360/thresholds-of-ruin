"""Shared ordinary finite-supply package for process acceptance and measurements."""
import copy
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import ROOT
from ruleset_sources import load_rules


def prepare(test, *, bandages=1, hit_dice=1, hostile=False, autonomous=False):
    """Two carers flank one exact known ground stack; no wizard or private holds.

    Actors 1 and 2 start at [1,1,0] and [3,1,0]. Both are externally controlled
    unless autonomous=True makes actor 2 an AI. Item 10 is the clean bandage at
    [2,1,0], omitted when bandages=0. Bundled Bind Wounds is unmodified. A hostile
    partner belongs to the explicit foe faction; ordinary allies share carers.
    """
    if type(bandages) is not int or not 0 <= bandages <= 1_000_000:
        raise ValueError("Bandage quantity must be a bounded nonnegative integer")
    if type(hit_dice) is not int or not 1 <= hit_dice <= 256:
        raise ValueError("Hit dice must be from 1 through 256")
    name = f"supplies-hd{hit_dice}-n{bandages}-{'ai' if autonomous else 'controlled'}-{'foe' if hostile else 'ally'}"
    package = test.directory / name
    shutil.copytree(ROOT / "scenarios/two-room", package)
    compiler = test.bin / ("tor-scenario" + test.suffix)
    rules = load_rules(package / "rules.toml", compiler)
    (package / "rules.toml").write_text(toml_source(rules), encoding="utf-8", newline="\n")
    manifest_path = package / "scenario.toml"
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    manifest["id"] = name
    manifest["factions"] = {"carers": [], "foe": ["carers"]}
    manifest["ai_profiles"] = {"partner": {"memory_ticks": 1000, "flee_percent": 0}}
    manifest["archetypes"]["clean_bandage"] = {
        "name": "clean bandage", "identity": "clean_bandage", "class": "misc",
        "stackable": True, "supply": "bandage"}
    strike = {"offset": 0, "on_success": [{"kind": "damage", "damage": {
        "kind": "components", "components": [{"category": "impact", "amount": {
            "kind": "fixed", "amount": 1}}]}}], "on_failure": [], "descriptors": [], "riders": []}
    manifest["creatures"] = {"species": {"carer": {
        "kind": "humanoid", "attributes": dict.fromkeys(
            ("strength", "speed", "intellect", "willpower", "awareness", "presence"), 0),
        "melee": {"skill": "heavy_weaponry", "bonus": 0, "wind_up": 100,
                  "recovery": 100, "strikes": [copy.deepcopy(strike)]}}}}

    def creature(name, faction):
        return {"species": "carer", "name": name, "faction": faction,
                "binding": "intellect", "starting_skills": ["heavy_weaponry", "athletics"],
                "restoration": "authored", "nutrition": {"kind":"exempt"}, "hit_dice": [{"source": "racial"} for _ in range(hit_dice)]}

    manifest["characters"][0]["creature"] = creature("first carer", "carers")
    manifest["characters"][0]["turn_ticks"] = 100
    manifest_path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    region["items"] = ([{"id": 10, "at": [2, 1, 0], "archetype": "clean_bandage",
                         "quantity": bandages}] if bandages else [])
    partner = {"id": 2, "at": [3, 1, 0], "turn_ticks": 100,
               "controller": "ai" if autonomous else "external",
               "creature": creature("second carer", "foe" if hostile else "carers")}
    if autonomous:
        partner["ai"] = "partner"
    region["actors"] = [partner]
    path.write_text(toml_source(region), encoding="utf-8", newline="\n")
    validated = subprocess.run([compiler, "validate", package], capture_output=True,
                               text=True, encoding="utf-8", timeout=30)
    test.assertEqual(validated.returncode, 0, validated.stderr)
    return package
