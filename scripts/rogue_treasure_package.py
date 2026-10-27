"""One opt-in generated treasure room, with unchanged production catalogs.

Fixed chance/counts and neutral waiting AI isolate entry/receipt behavior;
these are acceptance operands, not a new live Rogue distribution or balance pass.
"""
import copy
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source
from rogue_roster_package import PRODUCTION


def fixture():
    production = tomllib.loads((PRODUCTION / "scenario.toml").read_text(encoding="utf-8"))
    manifest = {key: copy.deepcopy(production[key]) for key in
                ("format", "version", "ruleset", "modules", "factions", "ai_profiles", "themes", "assets")}
    manifest.update(id="rogue-treasure-acceptance", default_character=1,
                    characters=[copy.deepcopy(production["characters"][0])])
    manifest["generation_recipes"] = {"room": {"kind": "grid_rooms", "version": 1,
        "room_width": [7, 7], "room_height": [5, 5], "extra_connections": [0, 0],
        "lighting": {"darkness_roll": 10, "darkness_start": 100}}}
    manifest["procedural_worlds"] = {"dungeon": {"kind": "grid_chain", "version": 1,
        "base_region": 1, "grid": [1, 1], "size": [9, 7, 2], "recipe": "room",
        "domain": {"kind": "open", "max_depth": 4}}}
    manifest["capabilities"] = {"npc_stair_traversal": False}
    fragment = tomllib.loads((PRODUCTION / "examples/treasure-stage.toml").read_text(encoding="utf-8"))
    stage, = fragment["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
    stage = copy.deepcopy(stage)
    stage.update(chance={"numerator": 1, "denominator": 1}, item_count=[2, 2],
                 actor_count=[1, 1], quantity=2)
    # Newly generated actors are not preauthorized player attachments. Keep
    # real autonomous scheduling without changing authentication or catalogs.
    manifest["factions"] = {name: [] for name in production["factions"]}
    manifest["ai_profiles"]["acceptance_wait"] = {
        "idle": "wait", "doors": False, "care": False, "equipment": False,
        "loot": False, "gold": False, "flee_percent": 0}
    stage["ai"] = "acceptance_wait"
    manifest["characters"][0]["anchor"] = "dungeon/1/down"
    manifest["floor_activation"] = {"version": 1, "protagonist": "selected_character",
        "families": {"dungeon": "treasure"}, "profiles": {"treasure": {
            "version": 1, "stages": [stage]}}}
    return manifest


def prepare(test):
    manifest = fixture()
    package = test.directory / "rogue-treasure-acceptance"
    (package / "catalog").mkdir(parents=True)
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    for relative in (*manifest["modules"], "rules.toml"):
        test.assertEqual((package / relative).read_bytes(), (PRODUCTION / relative).read_bytes())
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stdout + result.stderr)
    return package
