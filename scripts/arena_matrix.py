"""Generate curated arena recipes and paired plans with ordinary scenario validation."""
import argparse
import subprocess
import sys
import json
from pathlib import Path
import re
import tomllib
from ruleset_sources import resolve_with_dependencies

# Named point-by-point inputs are shared by the diagnostic matrix and the
# paired starter study. No engine path branches on these class or family IDs.
RECIPE_SOURCE = json.loads((Path(__file__).with_name("arena_recipes.json")).read_text(encoding="utf-8"))
STARTER_FAMILIES = tuple(RECIPE_SOURCE["allocations"])
REQUIRED_HD_LEVELS = tuple(RECIPE_SOURCE["required_bands"])
HD_LEVELS = tuple(sorted(REQUIRED_HD_LEVELS + tuple(RECIPE_SOURCE["diagnostic_bands"])))
ATTRIBUTES = ("strength", "speed", "intellect", "willpower", "awareness", "presence")
DIAGNOSTIC_FAMILIES = ("racial", "strength_heavy", "speed_light", "mage_intellect", "mage_willpower",
                       "mage_awareness", "mage_presence", "mixed", "warrior_mage_dip", "mage_warrior_dip",
                       "sustain", "binding_control", "resistance", "occluded_terrain")
FAMILIES = DIAGNOSTIC_FAMILIES + STARTER_FAMILIES


def quote_key(value):
    return value if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_-]*", value) else json.dumps(value)


def toml_value(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(item) for item in value) + "]"
    if isinstance(value, dict):
        return "{ " + ", ".join(quote_key(key) + " = " + toml_value(item)
                                  for key, item in value.items()) + " }"
    raise ValueError("Unsupported recipe TOML value")


def toml_source(value):
    return "\n".join(quote_key(key) + " = " + toml_value(item)
                     for key, item in value.items()) + "\n"


def recipe(family, hd):
    import copy
    if family not in FAMILIES or type(hd) is not int or hd not in HD_LEVELS:
        raise ValueError("Unknown curated family/HD")
    if family in ("mixed", "warrior_mage_dip", "mage_warrior_dip") and hd == 1:
        raise ValueError("A mixed or dip build requires at least two HD")
    allocations = RECIPE_SOURCE["allocations"]
    diagnostics = RECIPE_SOURCE["diagnostics"]
    mage = family.startswith("mage_") and family not in STARTER_FAMILIES and family != "mage_warrior_dip"
    caster = mage or family in ("sustain", "binding_control")
    binding = family[5:] if mage else "intellect"
    style = "light_weaponry" if family == "speed_light" else "heavy_weaponry"
    if family in STARTER_FAMILIES:
        definition = allocations[family]
    elif family in diagnostics:
        definition = diagnostics[family]
    elif family == "warrior_mage_dip":
        definition = diagnostics["warrior_dip_prefix"]
    elif caster or family == "mage_warrior_dip":
        definition = allocations["mage_mixed"]
    else:
        definition = allocations["warrior_mixed"]
    choices = copy.deepcopy(definition["selections_by_hd"][:hd])
    sources = definition.get("sources_by_hd", [definition.get("class")] * hd)[:hd]
    starting = definition["starting_skills"].copy()
    if family == "warrior_mage_dip":
        sources[-1] = "mage"
        choices[-1] = {"kind": "talent_form", "talent": "evocation_initiate", "form": "spark_lance_fire"}
    elif family == "mage_warrior_dip":
        sources[-1] = "warrior"
        choices[-1] = {"kind": "talent", "talent": "rattling_blow"}
    if style == "light_weaponry":
        starting = [style if skill == "heavy_weaponry" else skill for skill in starting]
        for choice in choices:
            if choice.get("skill") == "heavy_weaponry":
                choice["skill"] = style
    attributes = RECIPE_SOURCE["attributes"].copy()
    if family in ("strength_heavy", "speed_light", "resistance") or caster:
        primary = "speed" if family == "speed_light" else binding if caster else "strength"
        attributes = {attribute: 5 if attribute == primary else 1 for attribute in ATTRIBUTES}
    current = attributes.copy()
    entries = []
    for ordinal, (source, choice) in enumerate(zip(sources, choices, strict=True), 1):
        entry = {"source": source, "purchases": [{"pool": "advancement", "selection": choice}]}
        if ordinal % 4 == 0:
            attribute = (RECIPE_SOURCE["attribute_choices"][str(ordinal)] if family in STARTER_FAMILIES
                         else next(attribute for attribute in ATTRIBUTES if current[attribute] < 5))
            current[attribute] += 1
            entry["attribute"] = attribute
        entries.append(entry)
    return {"species": "subject", "name": "subject", "faction": "blue",
            "binding": binding, "attributes": attributes, "starting_skills": starting,
            "restoration": "authored", "nutrition": {"kind":"exempt"}, "hit_dice": entries}, style


def encounter(family, hd, candidate, mirrored, ruleset):
    subject, style = recipe(family if candidate else "strength_heavy", hd)
    opponent_family = "binding_control" if family == "resistance" else "strength_heavy"
    opponent, opponent_style = recipe(opponent_family, hd)
    opponent.update(species="opponent", name="opponent", faction="red")
    def species(skill):
        return {"kind": "humanoid", "behavior": {"disposition":"mean","idle":"explore","memory_ticks":1000}, "attributes": dict(zip(ATTRIBUTES, [2, 2, 2, 2, 1, 1])),
                "displacement": RECIPE_SOURCE["displacement"].copy(),
                "melee": {"skill": skill, "bonus": 0, "wind_up": 60, "recovery": 40,
                          "strikes": [{"offset": 0,
                                       "check": {"actor": "user", "skill": {"kind": "selected_weapon"},
                                                 "against": {"kind": "defense", "defense": "physical"}},
                                       "on_success": [{"kind": "damage", "damage": {"kind": "components",
                                           "components": [{"category": "impact", "amount": {"kind": "rolled",
                                               "dice": {"count": 1, "sides": 6, "bonus": 0}}}]}}],
                                       "on_failure": [], "descriptors": [], "riders": []}]}}
    catalog = {"subject": species(style), "opponent": species(opponent_style)}
    if family == "resistance" and candidate:
        catalog["subject"]["modifiers"] = [
            {"scope": "always", "op": {"kind": "immunity", "selector": {"kind": "descriptor", "descriptor": "fear"}}},
            {"scope": "always", "op": {"kind": "reduction", "selector": {"kind": "category", "category": "energy"}, "amount": 2}}]
    manifest = {"format": 35, "id": "arena-matrix-" + family + "-" + str(hd),
                "version": "1.0", "ruleset": ruleset, "default_character": 1,
                "arena": {"participants": [1, 2], "control": "all_ai", "ticks": 100000, "actions": 10000},
                "factions": {"blue": ["red"], "red": ["blue"]},
                "ai_profiles": {"sparring": {"care": True, "equipment": True, "loot": True, "flee_percent": 0}},
                "characters": [{"id": 1, "anchor": "1/start", "turn_ticks": 100, "unselected": "ai",
                                "ai": "sparring", "body": {"cells": [[0,0,0],[0,0,1]], "eye": [0,0,1], "mass": 80},
                                "creature": subject}],
                "creatures": {"species": catalog},
                "archetypes": {"opponent": {"creature": opponent}}}
    start, target = ([7,2,0], [1,2,0]) if mirrored else ([1,2,0], [7,2,0])
    region = {"id": 1, "name": "Curated arena", "size": [9,5,2], "chamber": True,
              "gravity": [0,0,-1], "anchors": {"start": start},
              "actors": [{"id": 2, "at": target, "archetype": "opponent", "controller": "ai", "ai": "sparring"}]}
    if family == "occluded_terrain":
        region["walls"] = [[4, y, z] for y in range(5) for z in range(2)]
    return manifest, region


def create_matrix(output, compiler, ruleset, seeds, *, families=None, levels=None):
    from arena_evaluation import digest, seed, write_json
    families = list(FAMILIES if families is None else families)
    levels = list(HD_LEVELS if levels is None else levels)
    seeds = [seed(value) for value in seeds]
    if not seeds or len(seeds) > 1000 or len(seeds) != len(set(seeds)):
        raise ValueError("Supply 1..1000 distinct seeds")
    if (not families or len(families) != len(set(families))
            or any(family not in FAMILIES for family in families)):
        raise ValueError("Unknown or duplicate curated family")
    if (not levels or len(levels) != len(set(levels))
            or any(isinstance(hd, bool) or hd not in HD_LEVELS for hd in levels)):
        raise ValueError("Unknown or duplicate curated HD level")
    if not isinstance(ruleset, str) or not ruleset:
        raise ValueError("Supply a ruleset TOML source path")
    rules_source = Path(ruleset).read_bytes()
    authored_rules = tomllib.loads(rules_source.decode("utf-8"))
    rules_document, dependencies = resolve_with_dependencies(authored_rules, compiler)
    if dependencies:
        rules_source = toml_source(rules_document).encode("utf-8")
    compiler = compiler.resolve(strict=True)
    compiler_hash = digest(compiler)
    output.mkdir(parents=True, exist_ok=False)
    inventory = {"format": "tor-arena-matrix-v1", "ruleset": rules_document["id"],
                 "compiler_sha256": compiler_hash, "rules_dependencies": dependencies, "seeds": seeds,
                 "cases": [], "exclusions": []}
    for family in families:
        for hd in levels:
            if hd == 1 and family in ("mixed", "warrior_mage_dip", "mage_warrior_dip"):
                inventory["exclusions"].append({"family": family, "hit_dice": hd,
                                                 "reason": "Two HD sources require at least two HD"})
                continue
            case_id = family + "-hd" + str(hd)
            case = output / case_id
            plan = {"seeds": seeds, "faction": "blue"}
            for variant in ("baseline", "candidate"):
                plan[variant] = {}
                for orientation in ("forward", "mirrored"):
                    package_name = variant + "-" + orientation
                    package = case / package_name
                    (package / "regions").mkdir(parents=True)
                    manifest, region = encounter(family, hd, variant == "candidate",
                                                 orientation == "mirrored", "rules.toml")
                    (package / "rules.toml").write_bytes(rules_source)
                    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
                    (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
                    result = subprocess.run([str(compiler), "validate", str(package)], capture_output=True,
                                            text=True, encoding="utf-8", timeout=30)
                    if result.returncode:
                        raise ValueError("Invalid curated package " + case_id + "/" + package_name
                                         + ": " + (result.stdout + result.stderr)[-2000:])
                    plan[variant][orientation] = package_name
            write_json(case / "plan.json", plan)
            inventory["cases"].append({"id": case_id, "family": family, "hit_dice": hd,
                                       "plan": case_id + "/plan.json"})
            write_json(output / "partial.json", inventory)
    if digest(compiler) != compiler_hash:
        raise ValueError("Scenario compiler changed during matrix generation")
    write_json(output / "matrix.json", inventory)
    (output / "partial.json").unlink(missing_ok=True)
    return inventory


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--ruleset", help="Ruleset TOML source to bundle with each generated package")
    parser.add_argument("--seeds", nargs="+", default=["42"])
    parser.add_argument("--families", nargs="+", choices=FAMILIES)
    parser.add_argument("--hd", nargs="+", type=int, choices=HD_LEVELS)
    args = parser.parse_args(argv)
    try:
        ruleset = args.ruleset
        if ruleset is None:
            path = Path(__file__).resolve().parents[1] / "scenarios/mob-arena/scenario.toml"
            ruleset = str(path.parent / tomllib.loads(path.read_text(encoding="utf-8"))["ruleset"])
        result = create_matrix(args.output.resolve(), args.compiler, ruleset, args.seeds,
                               families=args.families, levels=args.hd)
        print(f"Validated {len(result['cases'])} paired cases; {len(result['exclusions'])} explicit exclusions")
        return 0
    except (ValueError, OSError, subprocess.TimeoutExpired) as error:
        print(f"Arena matrix: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
