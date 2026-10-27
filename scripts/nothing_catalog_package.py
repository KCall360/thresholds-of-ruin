"""Production Nothing module in the proven single-owner ordinary device fixture.

Only actor/geometry and finite floor placement are fixture setup. Device and
ability definitions, generation range and appearance family remain production.
"""
import shutil
import subprocess
import tomllib
from arena_matrix import toml_source
from process_harness import ROOT
from ruleset_sources import load_rules

PRODUCTION = ROOT / "scenarios/rogue-exploration"


def prepare(test):
    package = test.package()
    manifest_path = package / "scenario.toml"
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    production = tomllib.loads((PRODUCTION / "scenario.toml").read_text(encoding="utf-8"))
    manifest["id"] = "imported-nothing"
    manifest["modules"] = ["catalog/items.toml", "catalog/creatures.toml", "catalog/bodies.toml"]
    manifest.pop("archetypes")
    manifest.pop("appearance_pools")
    manifest["assets"] = production["assets"]
    manifest["themes"] = production["themes"]
    manifest["characters"][0]["turn_ticks"] = 100
    test.assertEqual(manifest["creatures"]["species"]["figure"]["attributes"]["speed"], 0)
    (package / "catalog").mkdir()
    # Whole production rules/items now reference the opt-in body roster. Keep
    # their canonical definition closure without enrolling the synthetic figure.
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    manifest_path.write_text(toml_source(manifest), encoding="utf-8")
    region_path = package / "regions/1.toml"
    region = tomllib.loads(region_path.read_text(encoding="utf-8"))
    region["items"] = [
        {"id": 20, "at": [1, 2, 0], "archetype": "rogue.identify_wand", "quantity": 3},
        {"id": 30, "at": [1, 2, 0], "archetype": "rogue.wand_nothing"},
        {"id": 31, "at": [1, 2, 0], "archetype": "rogue.wand_nothing"},
    ]
    region_path.write_text(toml_source(region), encoding="utf-8")
    for relative in (*manifest["modules"], "rules.toml"):
        test.assertEqual((package / relative).read_bytes(), (PRODUCTION / relative).read_bytes())
    compiler = test.bin / ("tor-scenario" + test.suffix)
    checked = subprocess.run([compiler, "validate", package], capture_output=True, text=True, timeout=30)
    test.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
    rules = load_rules(package / "rules.toml", compiler)
    test.assertEqual(rules["constants"]["item_timing"]["consume_turns"], 1)
    return package
