"""Production food catalog in a tiny ordinary-actor acceptance room.

Only geometry and a finite two-fruit ground stack are fixture setup. The selected
Warrior, starter kit, nutrition rules and complete imported catalogs are unedited.
"""
import copy
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import ROOT

PRODUCTION = ROOT / "scenarios/rogue-exploration"


def fixture():
    source = tomllib.loads((PRODUCTION / "scenario.toml").read_text(encoding="utf-8"))
    manifest = {key: copy.deepcopy(source[key]) for key in
                ("format", "version", "ruleset", "modules", "factions", "ai_profiles", "themes", "assets")}
    selected = copy.deepcopy(source["characters"][0])
    selected["anchor"] = "1/start"
    manifest.update(id="rogue-fruit", default_character=1, characters=[selected])
    region = {"id": 1, "name": "Fruit acceptance room", "size": [7, 5, 2],
              "chamber": True, "gravity": [0, 0, -1], "anchors": {"start": [1, 2, 0]},
              "items": [{"id": 30, "at": [1, 2, 0], "archetype": "rogue.fruit", "quantity": 2}]}
    return manifest, region


def prepare(test):
    manifest, region = fixture()
    package = test.directory / "rogue-fruit"
    (package / "regions").mkdir(parents=True)
    (package / "catalog").mkdir()
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    for name in (*manifest["modules"], "rules.toml"):
        test.assertEqual((package / name).read_bytes(), (PRODUCTION / name).read_bytes())
    checked = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                             capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
    return package


def remove_sources(test, package):
    """Retain genuine compiled inputs, not the original authoring closure."""
    test.assertTrue((package / ".compiled").is_dir())
    for path in package.iterdir():
        if path.name != ".compiled":
            if path.is_dir():
                shutil.rmtree(path)
            else:
                path.unlink()
    test.assertFalse((package / "scenario.toml").exists())
    test.assertFalse((package / "catalog").exists())
    test.assertFalse((package / "rules.toml").exists())
