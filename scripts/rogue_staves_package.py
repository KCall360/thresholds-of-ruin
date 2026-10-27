"""Independent magical-implement contract values and ordinary acceptance setup.

Expected profiles are authored here, never projected from the production item
under test. The runtime fixture copies the production catalog unchanged.
"""

import copy
from pathlib import Path
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source

ROOT = Path(__file__).resolve().parents[1]
PRODUCTION = ROOT / "scenarios/rogue-exploration"

METALS = ("aluminum", "bronze", "copper", "electrum", "gold", "iron", "lead",
          "magnesium", "nickel", "platinum", "silver", "steel", "tin", "zinc")
WOODS = ("ash", "birch", "cedar", "cherry", "elm", "hazel", "mahogany", "maple",
         "oak", "pine", "teak", "walnut", "willow", "yew")


def expected_forms(effect):
    def equipment(staff):
        primary = {"category": "impact", **({"sides": 3} if staff else {})}
        amount = ({"type": "rolled", "count": 2, "sides": 3, "bonus": 0} if staff
                  else {"type": "fixed", "value": 1})
        return {"slot": "weapon", "attack": {
            "skill": "heavy_weaponry", "bonus": 0, "wind_up": 60, "recovery": 40,
            "damage": {"primary": primary, "components": [
                {"category": "impact", "amount": amount}]}}, "modifiers": []}
    return [{"name": kind + " of " + effect, "appearance_pool": "rogue." + pool,
             "equipment": equipment(kind == "staff")}
            for kind, pool in (("wand", "wands"), ("staff", "staves"))]


def fixture():
    """Synthetic speed-zero ordinary actors; every device/effect remains production."""
    manifest = tomllib.loads((ROOT / "scenarios/tests/magic-items/scenario.toml").read_text())
    production = tomllib.loads((PRODUCTION / "scenario.toml").read_text())
    manifest.update(id="rogue-staves", modules=["catalog/items.toml", "catalog/creatures.toml",
                    "catalog/bodies.toml"], assets=production["assets"], themes=production["themes"],
                    factions={"neutral": ["foe"], "foe": ["neutral"]})
    manifest.pop("archetypes")
    manifest.pop("appearance_pools")
    manifest["characters"][0]["turn_ticks"] = 100
    target = copy.deepcopy(manifest["creatures"]["species"]["figure"])
    target["modifiers"] = [
        {"scope": "always", "op": {"kind": "set", "stat": {"kind": "health"}, "value": 1000}},
        {"scope": "always", "op": {"kind": "add", "stat": {
            "kind": "defense", "defense": "physical"}, "value": -100}},
    ]
    manifest["creatures"]["species"]["practice"] = target
    creature = copy.deepcopy(manifest["characters"][0]["creature"])
    creature.update(species="practice", name="practice target", faction="foe")
    region = {"id": 1, "name": "Magical implement practice", "size": [7, 5, 2],
              "chamber": True, "anchors": {"start": [1, 2, 0]},
              "actors": [{"id": 3, "at": [2, 2, 0], "turn_ticks": 100,
                          "controller": "external", "creature": creature}],
              "items": [{"id": 20, "at": [1, 2, 0], "archetype": "rogue.identify_wand", "quantity": 2},
                        {"id": 30, "at": [1, 2, 0], "archetype": "rogue.wand_nothing"},
                        {"id": 31, "at": [1, 2, 0], "archetype": "rogue.wand_nothing"},
                        {"id": 32, "at": [1, 2, 0], "archetype": "rogue.wand_fire"}]}
    return manifest, region


def prepare(test):
    manifest, region = fixture()
    package = test.directory / "rogue-staves"
    (package / "regions").mkdir(parents=True)
    (package / "catalog").mkdir()
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    for relative in (*manifest["modules"], "rules.toml"):
        test.assertEqual((package / relative).read_bytes(), (PRODUCTION / relative).read_bytes())
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stdout + result.stderr)
    return package
