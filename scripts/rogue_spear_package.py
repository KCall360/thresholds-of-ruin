"""Production spear and ordinary Warrior in a bounded physical encounter.

Only geometry, one carried spear and the external emu's Health/defense differ
from production. The complete item/rules catalogs remain byte-identical.
"""
import copy
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source
from rogue_roster_package import PRODUCTION, encounter
from rogue_throwing_package import TARGET_MODIFIERS


def fixture():
    manifest, region = encounter("emu")
    manifest["id"] = "rogue-spear-acceptance"
    character = manifest["characters"][0]
    character["known_identities"].append("rogue.spear")
    character["loadout"].append({"archetype": "rogue.spear", "quantity": 1})
    return manifest, region


def prepare(test):
    manifest, region = fixture()
    package = test.directory / "rogue-spear-acceptance"
    (package / "regions").mkdir(parents=True)
    (package / "catalog").mkdir()
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    species_path = package / "catalog/creatures.toml"
    species = tomllib.loads(species_path.read_text(encoding="utf-8"))
    species["creatures"]["species"]["rogue.emu"]["modifiers"] = copy.deepcopy(TARGET_MODIFIERS)
    species_path.write_text(toml_source(species), encoding="utf-8", newline="\n")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    for relative in (*manifest["modules"], "rules.toml"):
        if relative != "catalog/creatures.toml":
            test.assertEqual((package / relative).read_bytes(), (PRODUCTION / relative).read_bytes())
    expected = tomllib.loads((PRODUCTION / "catalog/creatures.toml").read_text(encoding="utf-8"))
    expected["creatures"]["species"]["rogue.emu"]["modifiers"] = copy.deepcopy(TARGET_MODIFIERS)
    test.assertEqual(tomllib.loads(species_path.read_text(encoding="utf-8")), expected)
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stdout + result.stderr)
    return package
