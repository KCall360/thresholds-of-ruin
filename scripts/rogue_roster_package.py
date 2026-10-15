"""New reconstruction of isolated, normally compiled Rogue roster encounters.

Copies production species, recipes, rules and actual selected-character kit.
Only encounter geometry/control changes. This is not a balance-study runner.
"""
import copy
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import ROOT

PRODUCTION = ROOT / "scenarios/rogue-exploration"
SPECIES = ("emu", "snake", "hobgoblin", "zombie", "quagga", "centaur", "yeti",
           "black_unicorn", "ice_monster")


def encounter_sources():
    def read(name):
        return tomllib.loads((PRODUCTION / name).read_text(encoding="utf-8"))
    return {"manifest": read("scenario.toml"),
            "actors": read("catalog/actors.toml")["actors"],
            "species": read("catalog/creatures.toml")["creatures"]["species"],
            "items": read("catalog/items.toml")["items"]}


def encounter(species, *, character=1, autonomous=False):
    if species not in SPECIES or type(character) is not int or character not in (1, 2):
        raise ValueError("Choose an authored roster species and Warrior/Mage preset")
    source = encounter_sources()
    selected = copy.deepcopy(next(row for row in source["manifest"]["characters"] if row["id"] == character))
    selected["anchor"] = "1/start"
    manifest = {key: copy.deepcopy(source["manifest"][key]) for key in
                ("format", "version", "ruleset", "modules", "factions", "ai_profiles")}
    manifest.update(id="rogue-roster-" + species, default_character=character, characters=[selected])
    foe = {"id": 3, "at": [2, 2, 0], "archetype": "rogue." + species,
           "controller": "ai" if autonomous else "external"}
    if autonomous:
        foe["ai"] = "rogue_hostile"
    region = {"id": 1, "name": "Isolated Rogue content encounter", "size": [7, 5, 2],
              "chamber": True, "gravity": [0, 0, -1], "anchors": {"start": [1, 2, 0]},
              "actors": [foe]}
    return manifest, region


def roster_package(test, species, *, character=1, autonomous=False):
    manifest, region = encounter(species, character=character, autonomous=autonomous)
    package = test.directory / ("roster-" + species)
    (package / "regions").mkdir(parents=True)
    (package / "catalog").mkdir()
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    compiler = test.bin / ("tor-scenario" + test.suffix)
    result = subprocess.run([compiler, "validate", package], capture_output=True,
                            text=True, encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stderr)
    return package


def unrelated_fallback(test):
    """Validate unrelated startup content after the actual saved source is gone."""
    package = test.directory / "unrelated-fallback"
    shutil.copytree(ROOT / "scenarios/two-room", package)
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stderr)
    return package
