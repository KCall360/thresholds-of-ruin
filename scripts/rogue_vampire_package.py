"""Exact Rogue Vampire content in explicitly controlled process encounters.

Geometry and the authored foe's external controller differ from production; an
optional initial ration is explicitly labeled death/drop evidence, not a birth
roll. This is never autonomous survival or earned-route evidence.
"""
import copy
import shutil
import subprocess

from arena_matrix import recipe, toml_source
from rogue_activation_candidate import CARRIER_STOCK, DEPTH_GROWTH, VAMPIRE_POOL
from rogue_roster_package import PRODUCTION, encounter, encounter_sources


VAMPIRE = "rogue.vampire"
DRAIN = "rogue.drain_vitality"
REGENERATION = "rogue.regeneration"


def vampire_package(test, *, character=1, carried_ration=False, opponent="vampire"):
    """Keep the selected character's actual identity, kit and unspent first point."""
    if opponent not in ("vampire", "emu") or (opponent != "vampire" and not carried_ration):
        raise ValueError("Only the separate finite-stock mechanism may use an unchanged emu")
    manifest, region = encounter(opponent, character=character)
    manifest["id"] = "rogue-vampire-mechanisms"
    # A four-cell lane retains cardinal melee, a west ranged escape and an
    # east corpse step without serializing an irrelevant 35-cell chamber.
    region.update(size=[4, 1, 2], anchors={"start": [1, 0, 0]})
    region["actors"][0].update(at=[2, 0, 0])
    if carried_ration:
        # Explicit initial inventory is a death/drop/use fixture, not a birth
        # roll. Generated stock is covered by candidate receipt acceptance.
        region["items"] = [{"id": 1000, "at": [2, 0, 0], "carried_by": 3,
                            "archetype": "rogue.ration", "quantity": 1}]
    package = test.directory / f"vampire-{character}"
    (package / "regions").mkdir(parents=True)
    (package / "catalog").mkdir()
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stderr)
    return package


def generated_carrier_manifest(species="vampire"):
    """One unchanged autonomous carrier against a labeled authored arena build.

    The legal HD16 recipe is a mechanism witness, never earned-route or real-kit
    evidence. Production species, stock chance/pool and AI remain unchanged.
    """
    glyph, depth = {"centaur": ("C", 7), "yeti": ("Y", 11), "troll": ("T", 13),
                    "vampire": ("V", 27), "jabberwock": ("J", 21)}[species]
    stock = CARRIER_STOCK[glyph]
    manifest = copy.deepcopy(encounter_sources()["manifest"])
    manifest["id"] = f"generated-{species}-stock-mechanism"
    character = manifest["characters"][0]
    creature, _ = recipe("warrior_rank_heavy", 16)
    creature.update(species="rogue.human", name="Authored stock witness", faction="delvers")
    character.update(anchor=f"dungeon/{depth}/up", creature=creature)
    manifest["characters"] = [character]
    manifest["default_character"] = 1
    manifest.pop("objective", None)
    manifest["generation_recipes"] = {"tiny": {"kind": "grid_rooms", "version": 1,
        "room_width": [4, 4], "room_height": [3, 3], "extra_connections": [0, 0]}}
    manifest["procedural_worlds"] = {"dungeon": {"kind": "grid_chain", "version": 1,
        "base_region": 1, "grid": [1, 1], "size": [8, 5, 2], "recipe": "tiny",
        "domain": {"kind": "open", "max_depth": 100}}}
    manifest["floor_activation"] = {"version": 1, "protagonist": "selected_character",
        "families": {"dungeon": "rogue.low_depth"},
        "carried_item_pools": {stock["pool"]: copy.deepcopy(VAMPIRE_POOL)},
        "profiles": {"rogue.low_depth": {"version": 1, "stages": [{
            "id": "rogue.eight_species_foes", "version": 1, "kind": "actors", "count": [1, 1],
            "ai": "rogue_hostile", "depth_growth": copy.deepcopy(DEPTH_GROWTH),
            "bands": [{"depth": {"kind": "open", "minimum": depth}, "choices": [{
                "archetype": "rogue." + species, "weight": 1, "carried_loot": copy.deepcopy(stock)}]}]}]}}}
    return manifest


def generated_carrier_package(test, species="vampire"):
    manifest = generated_carrier_manifest(species)
    package = test.directory / f"generated-{species}-carrier"
    (package / "catalog").mkdir(parents=True)
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stderr)
    return package
