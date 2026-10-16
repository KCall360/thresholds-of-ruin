"""Materialize an explicitly sourced ten-species diagnostic package.

Never edits production. Bands condition the pinned Rogue source distribution on
the eligible recipes, with Vampire alone in the explicit depth-27+ tail. Checked
racial growth still refuses beyond current construction/storage capacity. Inputs
are not balance recommendations or earned progression evidence. Run only in the
qualified coordinator lane; the ordinary compiler creates candidate certificates.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tomllib

from arena_evaluation import MAX_BYTES, digest, files, write_json
from arena_matrix import toml_source
from ruleset_sources import resolve_with_dependencies


REFERENCE = "c7c119f893bd2f8255f2ad78d58a32664ddb9a97"
NAMES = {"E": "emu", "S": "snake", "H": "hobgoblin", "I": "ice_monster",
         "Z": "zombie", "C": "centaur", "Q": "quagga", "Y": "yeti",
         "U": "black_unicorn", "V": "vampire"}
# Integer proportions of unconditional source mass. Low-boundary redistribution
# matters at depths 2..5; unsupported choices are explicitly conditioned away.
BANDS = (
    (1, 1, (("E", 1), ("S", 1), ("H", 1))),
    (2, 2, (("E", 9), ("S", 9), ("H", 9), ("I", 5))),
    (3, 3, (("E", 8), ("S", 8), ("H", 8), ("I", 5))),
    (4, 4, (("E", 7), ("S", 7), ("H", 7), ("I", 5))),
    (5, 5, (("E", 6), ("S", 6), ("H", 6), ("I", 5), ("Z", 5))),
    (6, 6, (("E", 1), ("S", 1), ("H", 1), ("I", 1), ("Z", 1))),
    (7, 7, (("E", 1), ("S", 1), ("H", 1), ("I", 1), ("Z", 1), ("C", 1))),
    (8, 9, (("S", 1), ("H", 1), ("I", 1), ("Z", 1), ("C", 1), ("Q", 1))),
    (10, 10, (("H", 1), ("I", 1), ("Z", 1), ("C", 1), ("Q", 1))),
    (11, 11, (("I", 1), ("Z", 1), ("C", 1), ("Q", 1), ("Y", 1))),
    (12, 14, (("Z", 1), ("C", 1), ("Q", 1), ("Y", 1))),
    (15, 16, (("C", 1), ("Q", 1), ("Y", 1))),
    (17, 17, (("Q", 1), ("Y", 1), ("U", 1))),
    (18, 18, (("Y", 1), ("U", 1))),
    (19, 20, (("Y", 1), ("U", 1), ("V", 1))),
    (21, 22, (("U", 1), ("V", 1))),
    (23, 23, (("U", 5), ("V", 6))),
    (24, 24, (("U", 5), ("V", 7))),
    (25, 25, (("U", 5), ("V", 8))),
    (26, 26, (("U", 5), ("V", 9))),
    (27, None, (("V", 1),)),
)
# Birth policies belong to actor population choices, not canonical actor recipes.
# The equal ration/bandage pool is provisional TOR tuning, not Rogue loot equivalence.
# Keep historical stage/pool IDs so unrelated deterministic streams do not change.
VAMPIRE_STOCK = {"version": 1, "pool": "rogue.vampire_supplies",
                 "chance": {"numerator": 1, "denominator": 5}}
CARRIER_STOCK = {
    "C": {"version": 1, "pool": "rogue.vampire_supplies",
          "chance": {"numerator": 3, "denominator": 20}},
    "Y": {"version": 1, "pool": "rogue.vampire_supplies",
          "chance": {"numerator": 3, "denominator": 10}},
    "V": VAMPIRE_STOCK,
}
VAMPIRE_POOL = {"version": 1, "choices": [
    {"archetype": "rogue.ration", "weight": 1, "quantity": 1},
    {"archetype": "rogue.clean_bandage", "weight": 1, "quantity": 1},
]}
DEPTH_GROWTH = {"kind": "racial_after", "depth": 26}
ROOT = Path(__file__).resolve().parents[1]
DEFAULT_RULES = "crates/simulation/data/default-ruleset.toml"
FORMAT = "tor-rogue-activation-inputs-v2"
# Package source is always injected. These are generator dependencies only.
TOOL_INPUTS = ("scripts/rogue_activation_candidate.py", "scripts/arena_recipes.json",
               "scripts/arena_matrix.py", "scripts/arena_evaluation.py", "scripts/ruleset_sources.py")
COMPILER_DEPENDENCY_INPUTS = (DEFAULT_RULES,)


def conditional_bands():
    bands = []
    for low, high, choices in BANDS:
        depth = ({"kind": "open", "minimum": low} if high is None else
                 {"kind": "finite", "minimum": low, "maximum": high})
        authored = []
        for glyph, weight in choices:
            choice = {"archetype": "rogue." + NAMES[glyph], "weight": weight}
            if glyph in CARRIER_STOCK:
                choice["carried_loot"] = copy.deepcopy(CARRIER_STOCK[glyph])
            authored.append(choice)
        bands.append({"depth": depth, "choices": authored})
    return bands


def singleton_band(archetype):
    return [{"depth": {"kind": "open", "minimum": 1},
             "choices": [{"archetype": archetype, "weight": 1}]}]


def supply_stages():
    return [{"id": identity, "version": 1, "kind": "items", "count": [1, 1],
             "bands": singleton_band(archetype), "quantity": quantity}
            for identity, archetype, quantity in (
                ("rogue.bandages", "rogue.clean_bandage", 2),
                ("rogue.rations", "rogue.ration", 1))]


def canonical_actor_stage():
    return {"id": "rogue.eight_species_foes", "version": 1, "kind": "actors",
            "count": [4, 4], "bands": conditional_bands(), "ai": "rogue_hostile",
            "depth_growth": copy.deepcopy(DEPTH_GROWTH), "avoid_protagonist_sight": True}


def population_shape(source):
    """Accept only the two reviewed profiles, never derive an oracle from input."""
    activation = source["floor_activation"]
    legacy = [{"id": identity, "version": 1, "kind": "actors", "count": [2, 2],
               "bands": singleton_band(archetype), "ai": "rogue_hostile",
               "avoid_protagonist_sight": True}
              for identity, archetype in (("rogue.emus", "rogue.emu"), ("rogue.snakes", "rogue.snake"))]
    expected = {"version": 1, "protagonist": "selected_character",
                "families": {"dungeon": "rogue.low_depth"},
                "profiles": {"rogue.low_depth": {"version": 1, "stages": [*legacy, *supply_stages()]}}}
    # JSON comparison is type-sensitive: Python dictionary equality alone
    # would silently accept True where the reviewed version/weight is 1.
    exact = lambda value: json.dumps(value, sort_keys=True, allow_nan=False)
    if exact(activation) in (exact(expected), exact({**expected, "carried_item_pools": {}})):
        return "preactivation_es"
    expected["profiles"]["rogue.low_depth"]["stages"] = [canonical_actor_stage(), *supply_stages()]
    expected["carried_item_pools"] = {VAMPIRE_STOCK["pool"]: VAMPIRE_POOL}
    if exact(activation) == exact(expected):
        return "canonical_ten_species"
    raise ValueError("Review changed population profile or carried-item pool before generating a candidate")


def candidate_manifest(source, *, start_depth=1):
    domain = source["procedural_worlds"]["dungeon"]["domain"]
    if (domain["kind"] != "open" or type(start_depth) is not int
            or not 1 <= start_depth <= domain["max_depth"]):
        raise ValueError("Candidate diagnostic starts must be within the authored open world domain")
    population_shape(source)
    manifest = copy.deepcopy(source)
    manifest["id"] = "rogue-measured-candidate"
    activation = manifest["floor_activation"]
    activation["carried_item_pools"] = {VAMPIRE_STOCK["pool"]: copy.deepcopy(VAMPIRE_POOL)}
    activation["profiles"]["rogue.low_depth"]["stages"] = [canonical_actor_stage(), *supply_stages()]
    if start_depth != 1:
        for character in manifest["characters"]:
            character["anchor"] = f"dungeon/{start_depth}/up"
    return manifest


def source_inputs(package):
    """Read only declared package inputs, retaining their exact supplied bytes."""
    package = Path(package)
    if package.is_symlink() or (package / "scenario.toml").is_symlink():
        raise ValueError("Candidate source inputs cannot be symlinks")
    package = package.resolve(strict=True)
    manifest_bytes = (package / "scenario.toml").read_bytes()
    manifest = tomllib.loads(manifest_bytes.decode("utf-8"))
    names = ["scenario.toml", manifest["ruleset"], *manifest["modules"]]
    if any(not isinstance(name, str) for name in names) or len(names) != len(set(names)):
        raise ValueError("Invalid or duplicate declared candidate source input")
    if len(names) > 4096:
        raise ValueError("Candidate source exceeds bounded input count")
    snapshot, size = {}, 0
    for name in names:
        path = Path(name)
        if (path.is_absolute() or ".." in path.parts or path.as_posix() != name or not path.parts
                or any((package / parent).is_symlink() for parent in (path, *path.parents))):
            raise ValueError("Candidate source inputs must be ordinary package-relative files")
        raw = manifest_bytes if name == "scenario.toml" else (package / path).read_bytes()
        size += len(raw)
        if size > MAX_BYTES:
            raise ValueError("Candidate source exceeds bounded size")
        snapshot[name] = raw
    return package, manifest, snapshot


def create_candidate(output, compiler, *, source_package, source_provenance, start_depth=1):
    if (not isinstance(source_provenance, dict)
            or not isinstance(source_provenance.get("reference"), str)
            or not source_provenance["reference"].strip()):
        raise ValueError("Explicit source provenance requires a nonempty reference")
    provenance = json.loads(json.dumps(source_provenance, allow_nan=False))
    source_package, source, snapshot = source_inputs(source_package)
    output = Path(output).resolve()
    if output.is_relative_to(source_package):
        raise ValueError("Diagnostic output cannot modify its source package")
    manifest = candidate_manifest(source, start_depth=start_depth)
    shape = population_shape(source)
    tools = {relative: (ROOT / relative).read_bytes() for relative in TOOL_INPUTS}
    compiler_inputs = {relative: (ROOT / relative).read_bytes() for relative in COMPILER_DEPENDENCY_INPUTS}
    compiler = compiler.resolve(strict=True)
    compiler_hash = digest(compiler)
    def hashes(values):
        return {name: hashlib.sha256(raw).hexdigest() for name, raw in values.items()}
    package_hashes, tool_hashes, compiler_input_hashes = map(hashes, (snapshot, tools, compiler_inputs))
    output.mkdir(parents=True, exist_ok=False)
    inventory = {"format": FORMAT, "selector_reference": REFERENCE,
                 "source_provenance": provenance, "source_population_shape": shape,
                 "source_package_inputs": package_hashes, "tool_inputs": tool_hashes,
                 "compiler_dependency_inputs": compiler_input_hashes,
                 "compiler_sha256": compiler_hash, "generation_status": "resolving_rules",
                 "battle_evidence": "not_run", "calibration": "not_assessed",
                 "artifact_purpose": "diagnostic_population_package", "production_mutated": False,
                 "start_depth": start_depth,
                 "growth_evidence": "authored_hd_one_not_earned_route",
                 "population_scope": "conditional_ten_species_with_open_vampire_tail",
                 "depth_growth": copy.deepcopy(DEPTH_GROWTH),
                 "carried_stock": {"policies": {NAMES[glyph]: copy.deepcopy(policy)
                                                for glyph, policy in CARRIER_STOCK.items()},
                                   "pool": copy.deepcopy(VAMPIRE_POOL)},
                 "limitations": ["later-source species remain required, not permanently omitted",
                     "open selection bands do not bypass current HD, numeric or storage capacity refusals",
                     "four independent draws can select four unicorns or vampires; safety is unmeasured",
                     "Centaur 15%, Yeti 30%, Vampire 20% birth stock uses a provisional equal ration/bandage pool",
                     "historical eight-species stage/pool IDs remain stable for deterministic stream ownership",
                     "unchanged item stages retain their own streams, subject to changed occupancy",
                     "later-floor starts are receipt diagnostics, not earned progression"],
                 "package": "package", "validated_packages": 0}
    write_json(output / "partial.json", inventory)
    try:
        for group, values in (("package", snapshot), ("tools", tools), ("compiler", compiler_inputs)):
            for relative, raw in values.items():
                retained = output / "sources" / group / relative
                retained.parent.mkdir(parents=True, exist_ok=True)
                retained.write_bytes(raw)
        raw_rules = snapshot[source["ruleset"]].decode("utf-8")
        rules, dependencies = resolve_with_dependencies(raw_rules, compiler)
        imported = tomllib.loads(raw_rules).get("import")
        bundled = hashlib.sha256(compiler_inputs[DEFAULT_RULES].replace(b"\r\n", b"\n")).hexdigest()
        expected_dependencies = {} if imported is None else {imported: bundled}
        if dependencies != expected_dependencies:
            raise ValueError("Compiler bundled rules differ from the pinned source dependency")
        package = output / "package"
        package.mkdir()
        for module in source["modules"]:
            (package / module).parent.mkdir(parents=True, exist_ok=True)
            (package / module).write_bytes(snapshot[module])
        (package / source["ruleset"]).parent.mkdir(parents=True, exist_ok=True)
        (package / source["ruleset"]).write_text(toml_source(rules), encoding="utf-8", newline="\n")
        (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        result = subprocess.run([str(compiler), "validate", str(package)], capture_output=True,
                                text=True, encoding="utf-8", timeout=30)
        if result.returncode:
            raise ValueError("Invalid activation candidate: " + (result.stdout + result.stderr)[-2000:])
        inventory["validated_packages"] = 1
        write_json(output / "partial.json", inventory)
        if (digest(compiler) != compiler_hash
                or package_hashes != {name: digest(source_package / name) for name in snapshot}
                or tool_hashes != {name: digest(ROOT / name) for name in tools}
                or compiler_input_hashes != {name: digest(ROOT / name) for name in compiler_inputs}):
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
    parser.add_argument("--source-package", type=Path, required=True,
                        help="Explicit current package or retained snapshot package directory")
    parser.add_argument("--source-provenance", type=json.loads, required=True,
                        help='JSON object containing a nonempty reference for the supplied package')
    parser.add_argument("--start-depth", type=int, default=1,
                        help="Within authored domain; later starts are diagnostics subject to capacity refusal")
    args = parser.parse_args(argv)
    try:
        create_candidate(args.output.resolve(), args.compiler, source_package=args.source_package,
                         source_provenance=args.source_provenance, start_depth=args.start_depth)
        print("Validated diagnostic inputs only. Battles not run; source package unchanged.")
        return 0
    except (ValueError, OSError, KeyError, subprocess.TimeoutExpired) as error:
        print(f"Rogue candidate preparation: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
