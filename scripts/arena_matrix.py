"""Generate curated arena recipes and paired plans with ordinary scenario validation."""
import argparse
import subprocess
import sys
import json
from pathlib import Path
import re
import tomllib

HD_LEVELS = (1, 2, 4, 8, 16)
ATTRIBUTES = ("strength", "speed", "intellect", "willpower", "awareness", "presence")
SKILLS = ("athletics", "heavy_weaponry", "agility", "light_weaponry", "stealth", "thievery",
          "crafting", "deduction", "lore", "medicine", "discipline", "intimidation", "insight",
          "perception", "survival", "deception", "leadership", "persuasion", "spellcasting")
WARRIOR = ("power_strike", "hardiness", "heavy_blows", "toughness", "guard", "endurance",
           "impact_ward", "unyielding", "greater_guard", "mighty_blows", "deep_endurance",
           "indomitable", "iron_guard", "tireless", "crushing_blows", "perfected_blows")
RACIAL = ("hardiness", "power_strike", "heavy_blows", "toughness", "guard", "endurance",
          "impact_ward", "unyielding", "greater_guard", "mighty_blows", "deep_endurance",
          "indomitable", "iron_guard", "tireless", "crushing_blows", "perfected_blows")
MAGE = ("magic_bolt", "fear", "arcane_reserve", "potent_bolt", "empowered_bolt", "resolve",
        "hardiness", "greater_bolt", "endurance", "toughness", "guard", "master_bolt",
        "fear_mastery", "unyielding", "energy_ward", "indomitable")
MIXED = ("power_strike", "magic_bolt", "fear", "arcane_reserve", "potent_bolt", "endurance",
         "hardiness", "empowered_bolt", "guard", "resolve", "toughness", "impact_ward",
         "unyielding", "fear_mastery", "energy_ward", "greater_bolt")
FAMILIES = ("racial", "strength_heavy", "speed_light", "mage_intellect", "mage_willpower",
            "mage_awareness", "mage_presence", "mixed", "warrior_mage_dip", "mage_warrior_dip",
            "sustain", "fear", "resistance", "occluded_terrain")


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
    if family not in FAMILIES or type(hd) is not int or hd not in HD_LEVELS:
        raise ValueError("Unknown curated family/HD")
    if family in ("mixed", "warrior_mage_dip", "mage_warrior_dip") and hd == 1:
        raise ValueError("A mixed or dip build requires at least two HD")
    mage = family.startswith("mage_") and family != "mage_warrior_dip"
    caster = mage or family in ("sustain", "fear")
    binding = family[5:] if mage else "intellect"
    style = "light_weaponry" if family == "speed_light" else "heavy_weaponry"
    attributes = dict(zip(ATTRIBUTES, [2, 2, 2, 2, 1, 1]))
    if family in ("strength_heavy", "speed_light") or caster:
        primary = "speed" if family == "speed_light" else binding if caster else "strength"
        attributes = {attribute: 5 if attribute == primary else 1 for attribute in ATTRIBUTES}
    if family == "racial":
        sources, talents = ["racial"] * hd, RACIAL[:hd]
    elif family == "mixed":
        sources, talents = ["warrior" if i % 2 == 0 else "mage" for i in range(hd)], MIXED[:hd]
    elif family == "warrior_mage_dip":
        sources, talents = ["warrior"] * (hd - 1) + ["mage"], WARRIOR[:hd - 1] + ("magic_bolt",)
    elif family == "mage_warrior_dip":
        sources, talents = ["mage"] * (hd - 1) + ["warrior"], MAGE[:hd - 1] + ("power_strike",)
    elif caster:
        sources, talents = ["mage"] * hd, MAGE[:hd]
        if family == "fear":
            talents = ("fear", "magic_bolt")[:hd] + MAGE[2:hd]
        elif family == "sustain":
            talents = ("magic_bolt", "resolve", "arcane_reserve", "endurance", "deep_endurance",
                       "potent_bolt", "hardiness", "empowered_bolt", "tireless", "toughness",
                       "greater_bolt", "master_bolt", "fear", "fear_mastery", "unyielding", "indomitable")[:hd]
    else:
        sources, talents = ["warrior"] * hd, WARRIOR[:hd]
    current = attributes.copy()
    ranks = {skill: 0 for skill in SKILLS}
    entries = []
    for ordinal, (source, talent) in enumerate(zip(sources, talents), 1):
        priorities = (("spellcasting", "intimidation", "discipline", "lore") if source == "mage"
                      else (style, "discipline", "athletics", "perception"))
        priorities += tuple(skill for skill in SKILLS if skill not in priorities)
        entry = {"source": source, "talent": talent, "training": []}
        for _ in range(1 if source == "racial" else 2):
            skill = next(skill for skill in priorities if ranks[skill] < 5 and skill not in entry["training"])
            ranks[skill] += 1
            entry["training"].append(skill)
        if ordinal % 4 == 0:
            attribute = next(attribute for attribute in ATTRIBUTES if current[attribute] < 5)
            current[attribute] += 1
            entry["attribute"] = attribute
        entries.append(entry)
    return {"species": "subject", "name": "subject", "faction": "blue",
            "binding": binding, "attributes": attributes, "hit_dice": entries}, style


def encounter(family, hd, candidate, mirrored, ruleset):
    subject, style = recipe(family if candidate else "strength_heavy", hd)
    opponent_family = "fear" if family == "resistance" else "strength_heavy"
    opponent, opponent_style = recipe(opponent_family, hd)
    opponent.update(species="opponent", name="opponent", faction="red")
    def species(skill):
        return {"kind": "humanoid", "attributes": dict(zip(ATTRIBUTES, [2, 2, 2, 2, 1, 1])),
                "melee": {"skill": skill, "bonus": 0, "wind_up": 60, "recovery": 40,
                          "damage": {"primary": {"category": "impact", "sides": 6},
                                     "components": [{"category": "impact", "amount": {
                                         "type": "rolled", "count": 1, "sides": 6, "bonus": 0}}]}}}
    catalog = {"subject": species(style), "opponent": species(opponent_style)}
    if family == "resistance" and candidate:
        catalog["subject"]["grants"] = [
            {"type": "immunity", "selector": {"type": "descriptor", "descriptor": "fear"}},
            {"type": "reduction", "selector": {"type": "category", "category": "energy"}, "amount": 2}]
    manifest = {"format": 2, "id": "arena-matrix-" + family + "-" + str(hd),
                "version": "1.0", "ruleset": ruleset, "default_character": 1,
                "arena": {"participants": [1, 2], "control": "all_ai", "ticks": 100000, "actions": 10000},
                "factions": {"blue": ["red"], "red": ["blue"]},
                "ai_profiles": {"sparring": {"memory_ticks": 1000, "flee_percent": 0}},
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
        raise ValueError("Supply the current ruleset")
    compiler = compiler.resolve(strict=True)
    compiler_hash = digest(compiler)
    output.mkdir(parents=True, exist_ok=False)
    inventory = {"format": "tor-arena-matrix-v1", "ruleset": ruleset,
                 "compiler_sha256": compiler_hash, "seeds": seeds,
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
                                                 orientation == "mirrored", ruleset)
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
    parser.add_argument("--ruleset")
    parser.add_argument("--seeds", nargs="+", default=["42"])
    parser.add_argument("--families", nargs="+", choices=FAMILIES)
    parser.add_argument("--hd", nargs="+", type=int, choices=HD_LEVELS)
    args = parser.parse_args(argv)
    try:
        ruleset = args.ruleset
        if ruleset is None:
            path = Path(__file__).resolve().parents[1] / "scenarios/mob-arena/scenario.toml"
            ruleset = tomllib.loads(path.read_text(encoding="utf-8"))["ruleset"]
        result = create_matrix(args.output.resolve(), args.compiler, ruleset, args.seeds,
                               families=args.families, levels=args.hd)
        print(f"Validated {len(result['cases'])} paired cases; {len(result['exclusions'])} explicit exclusions")
        return 0
    except (ValueError, OSError, subprocess.TimeoutExpired) as error:
        print(f"Arena matrix: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
