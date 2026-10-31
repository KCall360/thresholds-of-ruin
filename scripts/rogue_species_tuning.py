"""Tune Rogue species values against an encounter study (milestone 3).

Reads a rogue_encounter_study.py result and adjusts only authored numbers in
`scenarios/rogue/catalog/creatures.toml`; no mechanic changes:

- Health modifier, so mean Health matches Rogue's `level d8` (4.5 per level).
- Physical defense bonus, toward the adventurer's target hit chance.
- Accuracy, toward the foe's target hit chance: the melee bonus for strikes
  checked with the selected weapon, or an attack-attribute modifier (without
  defense) for natural-attack checks, which take attribute plus skill rank.
- Damage-dice bonus per strike, toward the foe's target damage per action.

Steps are one point per pass (Health moves straight to its target), so repeated
study/tune passes converge without overshooting on noisy samples. Edits replace
single lines inside each species' blocks, preserving the catalog's comments.

    python scripts/rogue_species_tuning.py .local/encounters/study.json --apply
"""
import argparse
import json
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / "scenarios/rogue/catalog/creatures.toml"
HIGH, LOW = 1.15, 1 / 1.15
FAR = 1.4
# Rare attackers (greedy or fleeing species) give noisy ratios; leave them alone.
MINIMUM_CHECKS = 20


def step(ratio):
    """+1/-1 outside the tolerance band, doubled when far outside it."""
    if ratio > HIGH:
        return 2 if ratio > FAR else 1
    if ratio < LOW:
        return -2 if ratio < 1 / FAR else -1
    return 0
DEFENSE = {"kind": "defense", "defense": "physical"}
GOVERNING = {"heavy_weaponry": "strength", "light_weaponry": "speed"}
HEALTH = {"kind": "health"}


def inline(value):
    """Render a TOML inline value in the catalog's style."""
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, str):
        return json.dumps(value)
    if isinstance(value, list):
        return "[" + ", ".join(inline(item) for item in value) + "]"
    return "{ " + ", ".join(f"{key} = {inline(item)}" for key, item in value.items()) + " }"


def species_block(text, name, suffix=""):
    """(start, end) of a table whose header is exactly the species' (sub)table."""
    header = f'[creatures.species."{name}"{suffix}]\n'
    start = text.index(header)
    following = re.search(r"^\[", text[start + len(header):], flags=re.M)
    end = start + len(header) + (following.start() if following else len(text))
    return start, end


def natural_accuracy(species):
    """True when strikes are checked with the natural-attack selector."""
    return any(strike.get("check", {}).get("skill", {}).get("kind") == "natural_attack"
               for strike in species.get("melee", {}).get("strikes", []))


def accuracy_value(modifiers):
    for row in modifiers:
        if row["op"].get("kind") == "attribute_without_defense":
            return row["op"]["value"]
    return 0


def with_accuracy(modifiers, attribute, value):
    rows = [row for row in modifiers if row["op"].get("kind") != "attribute_without_defense"]
    if value:
        rows.append({"scope": "always", "op": {"kind": "attribute_without_defense",
                                               "attribute": attribute, "defense": "physical",
                                               "value": value}})
    return rows


def modifier_value(modifiers, stat):
    for row in modifiers:
        if row["op"].get("kind") == "add" and row["op"].get("stat") == stat:
            return row["op"]["value"]
    return 0


def with_modifier(modifiers, stat, value):
    rows = [row for row in modifiers
            if not (row["op"].get("kind") == "add" and row["op"].get("stat") == stat)]
    if value:
        rows.append({"scope": "always", "op": {"kind": "add", "stat": stat, "value": value}})
    return rows


def set_modifiers(text, name, modifiers):
    start, end = species_block(text, name)
    block = text[start:end]
    line = "modifiers = " + inline(modifiers) + "\n"
    if re.search(r"^modifiers = .*\n", block, flags=re.M):
        block = re.sub(r"^modifiers = .*\n", lambda _: line, block, count=1, flags=re.M)
    else:
        block = block.rstrip("\n") + "\n" + line + ("\n" if block.endswith("\n\n") else "")
    return text[:start] + block + text[end:]


def set_melee_bonus(text, name, bonus):
    start, end = species_block(text, name, ".melee")
    block = re.sub(r"^bonus = -?\d+\n", f"bonus = {bonus}\n", text[start:end], count=1, flags=re.M)
    return text[:start] + block + text[end:]


def set_dice_bonus(text, name, bonus):
    """Every rolled strike die of the species carries the same flat bonus."""
    header = f'[[creatures.species."{name}".melee.strikes]]\n'
    position = 0
    while (start := text.find(header, position)) >= 0:
        following = re.search(r"^\[", text[start + len(header):], flags=re.M)
        end = start + len(header) + (following.start() if following else len(text))
        block = re.sub(r"(dice = \{ count = \d+, sides = \d+, bonus = )-?\d+",
                       lambda match: match.group(1) + str(bonus), text[start:end])
        text = text[:start] + block + text[end:]
        position = start + len(block)
    return text


def set_inline_melee(text, name, melee_bonus, dice_bonus):
    """Edit a one-line `melee = { ... }` table inside the species block."""
    start, end = species_block(text, name)
    block = text[start:end]
    match = re.search(r"^melee = \{.*\}\n", block, flags=re.M)
    if not match:
        return text
    line = match.group(0)
    if melee_bonus is not None:
        line = re.sub(r"^(melee = \{ skill = \"[a-z_]+\", bonus = )-?\d+",
                      lambda found: found.group(1) + str(melee_bonus), line)
    if dice_bonus is not None:
        line = re.sub(r"(dice = \{ count = \d+, sides = \d+, bonus = )-?\d+",
                      lambda found: found.group(1) + str(dice_bonus), line)
    block = block[:match.start()] + line + block[match.end():]
    return text[:start] + block + text[end:]


def current(catalog, name):
    species = catalog["creatures"]["species"][name]
    strikes = species.get("melee", {}).get("strikes", [])
    bonuses = [component["amount"]["dice"]["bonus"]
               for strike in strikes for effect in strike.get("on_success", [])
               if effect.get("kind") == "damage"
               for component in effect["damage"].get("components", [])
               if component["amount"].get("kind") == "rolled"]
    return {"modifiers": species.get("modifiers", []),
            "natural": natural_accuracy(species),
            "attribute": GOVERNING.get(species.get("melee", {}).get("skill")),
            "accuracy": accuracy_value(species.get("modifiers", [])),
            "melee_bonus": species.get("melee", {}).get("bonus", 0),
            "dice_bonus": max(bonuses, default=None)}


def reference_hp(glyph):
    import rogue_reference_targets
    return next(row.foe_hp for row in rogue_reference_targets.targets() if row.glyph == glyph)


def proposals(study, catalog):
    """One damped adjustment per species from its measured/target ratios."""
    changes = []
    for row in study["species"]:
        name = "rogue." + row["species"].replace(" ", "_")
        counts, comparison = row["counts"], row["comparison"]
        if not counts or name not in catalog["creatures"]["species"]:
            continue
        values = current(catalog, name)
        change = {"species": name, "glyph": row["glyph"]}
        encounters = sum(1 for outcome in row["outcomes"] if not outcome.startswith("failure"))
        measured_hp = counts["foe_health"] / encounters if encounters else None
        target_hp = row.get("target_foe_hp") or reference_hp(row["glyph"])
        health = modifier_value(values["modifiers"], HEALTH)
        if measured_hp and target_hp:
            proposed = health + round(target_hp - measured_hp)
            # The smallest observed Health must stay at least 2 after the change.
            minimum = counts.get("foe_health_minimum") or measured_hp
            change["health"] = max(proposed, health - int(minimum) + 2)
        if any("Invalid actor creature build" in outcome for outcome in row["outcomes"]):
            # A rejected birth means some rolls reached zero Health: back off.
            change["health"] = health + 2

        def ratio(metric):
            return comparison[metric]["ratio"]

        defense = modifier_value(values["modifiers"], DEFENSE)
        if (value := ratio("player_hit")) is not None and counts["player_checks"] >= MINIMUM_CHECKS:
            change["defense"] = defense + step(value)
        enough = counts["foe_checks"] >= MINIMUM_CHECKS
        if (value := ratio("foe_hit")) is not None and enough:
            if values["natural"] and values["attribute"]:
                change["accuracy"] = values["accuracy"] - step(value)
                change["melee_bonus"] = 0  # not read by natural-attack checks
            else:
                change["melee_bonus"] = values["melee_bonus"] - step(value)
        damage, hit = ratio("foe_damage_per_action"), ratio("foe_hit")
        if damage is not None and hit and values["dice_bonus"] is not None and enough:
            per_hit = damage / hit
            # Damage moves one point at a time: dice bonuses overshoot easily.
            change["dice_bonus"] = max(0, values["dice_bonus"] - max(-1, min(1, step(per_hit))))
        changes.append(change)
    return changes


def apply(changes, text, catalog):
    for change in changes:
        name = change["species"]
        modifiers = current(catalog, name)["modifiers"]
        if "health" in change:
            modifiers = with_modifier(modifiers, HEALTH, change["health"])
        if "defense" in change:
            modifiers = with_modifier(modifiers, DEFENSE, max(0, change["defense"]))
        if "accuracy" in change:
            attribute = current(catalog, name)["attribute"]
            modifiers = with_accuracy(modifiers, attribute, change["accuracy"])
        text = set_modifiers(text, name, modifiers)
        if f'[creatures.species."{name}".melee]\n' not in text:
            text = set_inline_melee(text, name, change.get("melee_bonus"), change.get("dice_bonus"))
            continue
        if "melee_bonus" in change:
            text = set_melee_bonus(text, name, change["melee_bonus"])
        if change.get("dice_bonus") is not None:
            text = set_dice_bonus(text, name, change["dice_bonus"])
    tomllib.loads(text)
    return text


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("study", type=Path)
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args(argv)
    study = json.loads(args.study.read_text(encoding="utf-8"))
    text = CATALOG.read_text(encoding="utf-8")
    catalog = tomllib.loads(text)
    changes = proposals(study, catalog)
    print(json.dumps(changes, indent=1))
    if args.apply:
        CATALOG.write_text(apply(changes, text, catalog), encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
