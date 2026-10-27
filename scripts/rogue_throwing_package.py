"""Opt-in Rogue throwing stacks in an ordinary controlled content encounter.

The complete production item/rules catalogs are copied verbatim. Only geometry,
finite carried stock and the external emu's Health/defense are acceptance setup;
there is no fixture launcher, free expertise or live distribution modification.
"""
import copy
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source
from rogue_roster_package import PRODUCTION, encounter


MISSILES = ("rogue.dart", "rogue.shuriken")
TARGET_MODIFIERS = [
    {"scope": "always", "op": {"kind": "add", "stat": {"kind": "health"}, "value": 100}},
    {"scope": "always", "op": {"kind": "add", "stat": {"kind": "defense", "defense": "physical"}, "value": -100}},
]


def fixture():
    manifest, region = encounter("emu")
    manifest["id"] = "rogue-throwing-acceptance"
    character = manifest["characters"][0]
    # Keep the primary mace occupied: carried throws use the real secondary
    # socket and its ordinary nonzero Focus cost, not an invented launcher.
    character["known_identities"] += list(MISSILES)
    character["loadout"] += [{"archetype": identity, "quantity": 3} for identity in MISSILES]
    return manifest, region


def prepare(test):
    manifest, region = fixture()
    package = test.directory / "rogue-throwing-acceptance"
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
