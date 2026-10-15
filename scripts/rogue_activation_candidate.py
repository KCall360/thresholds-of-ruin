"""Materialize an unselected eight-species candidate for bounded measurements.

Never edits production. Bands condition the pinned Rogue source distribution on
the eligible recipes, with Vampire alone in the explicit depth-27+ tail. Checked
racial growth still refuses beyond current construction/storage capacity. Inputs
are not balance recommendations or earned progression evidence. Run only in the
qualified coordinator lane; the ordinary compiler creates candidate certificates.
"""
import argparse
import copy
import hashlib
from pathlib import Path
import subprocess
import sys
import tomllib

from arena_evaluation import digest, files, write_json
from arena_matrix import toml_source
from rogue_roster_study import DEFAULT_RULES, PRODUCTION, ROOT
from ruleset_sources import resolve_with_dependencies


REFERENCE = "c7c119f893bd2f8255f2ad78d58a32664ddb9a97"
NAMES = {"E": "emu", "S": "snake", "H": "hobgoblin", "I": "ice_monster",
         "Z": "zombie", "Q": "quagga", "U": "black_unicorn", "V": "vampire"}
# Integer proportions of unconditional source mass. Low-boundary redistribution
# matters at depths 2..5; unsupported choices are explicitly conditioned away.
BANDS = (
    (1, 1, (("E", 1), ("S", 1), ("H", 1))),
    (2, 2, (("E", 9), ("S", 9), ("H", 9), ("I", 5))),
    (3, 3, (("E", 8), ("S", 8), ("H", 8), ("I", 5))),
    (4, 4, (("E", 7), ("S", 7), ("H", 7), ("I", 5))),
    (5, 5, (("E", 6), ("S", 6), ("H", 6), ("I", 5), ("Z", 5))),
    (6, 7, (("E", 1), ("S", 1), ("H", 1), ("I", 1), ("Z", 1))),
    (8, 9, (("S", 1), ("H", 1), ("I", 1), ("Z", 1), ("Q", 1))),
    (10, 10, (("H", 1), ("I", 1), ("Z", 1), ("Q", 1))),
    (11, 11, (("I", 1), ("Z", 1), ("Q", 1))),
    (12, 14, (("Z", 1), ("Q", 1))),
    (15, 16, (("Q", 1),)),
    (17, 17, (("Q", 1), ("U", 1))),
    (18, 18, (("U", 1),)),
    (19, 22, (("U", 1), ("V", 1))),
    (23, 23, (("U", 5), ("V", 6))),
    (24, 24, (("U", 5), ("V", 7))),
    (25, 25, (("U", 5), ("V", 8))),
    (26, 26, (("U", 5), ("V", 9))),
    (27, None, (("V", 1),)),
)
# Birth policies belong to actor population choices, not canonical actor recipes.
# This restricted available-content pool is provisional, not Rogue's full loot.
VAMPIRE_STOCK = {"version": 1, "pool": "rogue.vampire_supplies",
                 "chance": {"numerator": 1, "denominator": 5}}
VAMPIRE_POOL = {"version": 1, "choices": [
    {"archetype": "rogue.ration", "weight": 1, "quantity": 1},
    {"archetype": "rogue.clean_bandage", "weight": 1, "quantity": 1},
]}
DEPTH_GROWTH = {"kind": "racial_after", "depth": 26}
INPUTS = (PRODUCTION + "scenario.toml", PRODUCTION + "rules.toml",
          PRODUCTION + "catalog/creatures.toml", PRODUCTION + "catalog/actors.toml",
          PRODUCTION + "catalog/items.toml", DEFAULT_RULES,
          "scripts/rogue_activation_candidate.py", "scripts/rogue_roster_study.py",
          "scripts/rogue_roster_package.py",
          "scripts/arena_recipes.json", "scripts/arena_matrix.py",
          "scripts/arena_evaluation.py", "scripts/ruleset_sources.py")


def conditional_bands():
    bands = []
    for low, high, choices in BANDS:
        depth = ({"kind": "open", "minimum": low} if high is None else
                 {"kind": "finite", "minimum": low, "maximum": high})
        authored = []
        for glyph, weight in choices:
            choice = {"archetype": "rogue." + NAMES[glyph], "weight": weight}
            if glyph == "V":
                choice["carried_loot"] = copy.deepcopy(VAMPIRE_STOCK)
            authored.append(choice)
        bands.append({"depth": depth, "choices": authored})
    return bands


def candidate_manifest(source, *, start_depth=1):
    domain = source["procedural_worlds"]["dungeon"]["domain"]
    if (domain["kind"] != "open" or type(start_depth) is not int
            or not 1 <= start_depth <= domain["max_depth"]):
        raise ValueError("Candidate diagnostic starts must be within the authored open world domain")
    manifest = copy.deepcopy(source)
    manifest["id"] = "rogue-measured-candidate"
    activation = manifest["floor_activation"]
    if (activation["protagonist"] != "selected_character"
            or activation["families"] != {"dungeon": "rogue.low_depth"}):
        raise ValueError("Candidate requires the production selected-character entry policy")
    profile = activation["profiles"]["rogue.low_depth"]
    stages = profile["stages"]
    if ([stage["kind"] for stage in stages] != ["actors", "actors", "items", "items"]
            or [stage["count"] for stage in stages] != [[2, 2], [2, 2], [1, 1], [1, 1]]
            or [stage["quantity"] for stage in stages[2:]] != [2, 1]):
        raise ValueError("Review changed production encounter/supply controls before generating a candidate")
    pools = activation.setdefault("carried_item_pools", {})
    if VAMPIRE_STOCK["pool"] in pools:
        raise ValueError("Candidate carried-item pool collides with source-owned content")
    pools[VAMPIRE_STOCK["pool"]] = copy.deepcopy(VAMPIRE_POOL)
    profile["stages"] = [{"id": "rogue.eight_species_foes", "version": 1, "kind": "actors",
                          "count": [4, 4], "bands": conditional_bands(), "ai": "rogue_hostile",
                          "depth_growth": copy.deepcopy(DEPTH_GROWTH),
                          "avoid_protagonist_sight": True}, *stages[2:]]
    if start_depth != 1:
        for character in manifest["characters"]:
            character["anchor"] = f"dungeon/{start_depth}/up"
    return manifest


def create_candidate(output, compiler, *, start_depth=1):
    snapshot = {relative: (ROOT / relative).read_bytes() for relative in INPUTS}
    source = tomllib.loads(snapshot[PRODUCTION + "scenario.toml"].decode("utf-8"))
    manifest = candidate_manifest(source, start_depth=start_depth)
    compiler = compiler.resolve(strict=True)
    compiler_hash = digest(compiler)
    hashes = {relative: hashlib.sha256(raw).hexdigest() for relative, raw in snapshot.items()}
    output.mkdir(parents=True, exist_ok=False)
    inventory = {"source_reference": REFERENCE, "source_inputs": hashes,
                 "compiler_sha256": compiler_hash, "generation_status": "resolving_rules",
                 "battle_evidence": "not_run", "calibration": "not_assessed",
                 "production_activation": "not_selected", "start_depth": start_depth,
                 "growth_evidence": "authored_hd_one_not_earned_route",
                 "population_scope": "conditional_eight_species_with_open_vampire_tail",
                 "depth_growth": copy.deepcopy(DEPTH_GROWTH),
                 "carried_stock": {"policy": copy.deepcopy(VAMPIRE_STOCK),
                                   "pool": copy.deepcopy(VAMPIRE_POOL)},
                 "limitations": ["later-source species remain required, not permanently omitted",
                     "open selection bands do not bypass current HD, numeric or storage capacity refusals",
                     "four independent draws can select four unicorns or vampires; safety is unmeasured",
                     "Vampire 20% birth stock uses a provisional restricted ration/bandage pool",
                     "new foe stage identity changes its placement/build streams",
                     "unchanged item stages retain their own streams, subject to changed occupancy",
                     "later-floor starts are receipt diagnostics, not earned progression"],
                 "package": "package", "validated_packages": 0}
    write_json(output / "partial.json", inventory)
    # New reconstruction hardening: retain an explicit failed inventory.
    try:
        for relative, raw in snapshot.items():
            retained = output / "sources" / relative
            retained.parent.mkdir(parents=True, exist_ok=True)
            retained.write_bytes(raw)
        raw_rules = snapshot[PRODUCTION + "rules.toml"].decode("utf-8")
        rules, dependencies = resolve_with_dependencies(raw_rules, compiler)
        imported = tomllib.loads(raw_rules)["import"]
        bundled = hashlib.sha256(snapshot[DEFAULT_RULES].replace(b"\r\n", b"\n")).hexdigest()
        if dependencies != {imported: bundled}:
            raise ValueError("Compiler bundled rules differ from the pinned production source")
        package = output / "package"
        (package / "catalog").mkdir(parents=True)
        for module in source["modules"]:
            (package / module).write_bytes(snapshot[PRODUCTION + module])
        (package / "rules.toml").write_text(toml_source(rules), encoding="utf-8", newline="\n")
        (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        result = subprocess.run([str(compiler), "validate", str(package)], capture_output=True,
                                text=True, encoding="utf-8", timeout=30)
        if result.returncode:
            raise ValueError("Invalid activation candidate: " + (result.stdout + result.stderr)[-2000:])
        inventory["validated_packages"] = 1
        write_json(output / "partial.json", inventory)
        if digest(compiler) != compiler_hash or hashes != {relative: digest(ROOT / relative) for relative in INPUTS}:
            raise ValueError("Candidate source or compiler changed during generation")
        inventory.update(generation_status="validated", validated_packages=1,
                         rules_dependencies=dependencies, package_inputs=files(package))
        write_json(output / "inputs.json", inventory)
        (output / "partial.json").unlink()
    except (ValueError, OSError, KeyError, subprocess.TimeoutExpired) as error:
        inventory.update(generation_status="failed", failure={"type": type(error).__name__, "message": str(error)[-2000:]})
        write_json(output / "partial.json", inventory)
        raise
    return inventory


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--start-depth", type=int, default=1,
                        help="Within authored domain; later starts are diagnostics subject to capacity refusal")
    args = parser.parse_args(argv)
    try:
        create_candidate(args.output.resolve(), args.compiler, start_depth=args.start_depth)
        print("Validated candidate inputs only. Battles not run; production activation not selected.")
        return 0
    except (ValueError, OSError, KeyError, subprocess.TimeoutExpired) as error:
        print(f"Rogue candidate preparation: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
