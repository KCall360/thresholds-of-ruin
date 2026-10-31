"""Write the Rogue-only package's item stages from Rogue's generation tables.

Rogue makes nine item attempts per new floor, each succeeding 36% of the time,
and draws each success from category weights times subtype weights
(docs/research/rogue-5.4.4/content.md). This script renders that table as one
`chance_items` stage plus the food-drought guarantee, replacing the starter
package's one-attempt-per-category stages in `scenarios/rogue/scenario.toml`.
Rerun it after changing the table, then validate the package.

    python scripts/rogue_item_table.py
"""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "scenarios/rogue/scenario.toml"
BEGIN = "# BEGIN generated Rogue item table (scripts/rogue_item_table.py)\n"
END = "# END generated Rogue item table\n"
# Rogue groups arrows, darts and shuriken: rnd(8) + 8.
MISSILES = '{ kind = "uniform", minimum = 8, maximum = 15 }'

CATEGORIES = {
    "potion": (26, {
        "confusion": 7, "hallucination": 8, "poison": 8, "gain_strength": 13,
        "see_invisible": 3, "healing": 13, "monster_detection": 6, "magic_detection": 6,
        "raise_level": 2, "extra_healing": 5, "haste": 5, "restore_strength": 13,
        "blindness": 5, "levitation": 6}),
    "scroll": (36, {
        "monster_confusion": 7, "magic_mapping": 4, "hold_monster": 2, "sleep": 3,
        "enchant_armor": 7, "identify_potion": 10, "identify_scroll": 10,
        "identify_weapon": 6, "identify_armor": 7, "identify_wand": 10, "scare_monster": 3,
        "food_detection": 2, "teleportation": 5, "enchant_weapon": 8, "create_monster": 4,
        "remove_curse": 7, "aggravate_monster": 3, "protect_armor": 2}),
    "food": (16, {"ration": 90, "fruit": 10}),
    "weapon": (7, {
        "generated_mace": 11, "long_sword": 11, "shortbow": 12, "arrow": 12, "dagger": 8,
        "two_handed_sword": 10, "dart": 12, "shuriken": 12, "spear": 12}),
    "armor": (7, {
        "leather_armor": 20, "generated_ring_mail": 15, "studded_leather": 15,
        "scale_mail": 13, "chain_mail": 12, "splint_mail": 10, "banded_mail": 10,
        "plate_mail": 5}),
    "ring": (4, {
        "ring_protection": 9, "ring_add_strength": 9, "ring_sustain_strength": 5,
        "ring_searching": 10, "ring_see_invisible": 10, "ring_adornment": 1,
        "ring_aggravation": 10, "ring_dexterity": 8, "ring_increase_damage": 8,
        "ring_regeneration": 4, "ring_slow_digestion": 9, "ring_teleportation": 5,
        "ring_stealth": 7, "ring_maintain_armor": 5}),
    "wand": (4, {
        "wand_light": 12, "wand_invisibility": 6, "wand_lightning": 3, "wand_fire": 3,
        "wand_cold": 3, "wand_polymorph": 15, "wand_magic_missile": 10, "wand_haste": 10,
        "wand_slow": 11, "wand_drain_life": 9, "wand_nothing": 1, "wand_teleport_away": 6,
        "wand_teleport_to": 6, "wand_cancellation": 5}),
}
FOOD = ["rogue.fruit", "rogue.ration"]


# A population band holds at most 64 choices, so ordinary placement is two
# independent groups. Each group's chance is 36% x its category share, keeping
# every item type's expected count per floor equal to Rogue's.
GROUPS = {"rogue.items": ("potion", "scroll"),
          "rogue.items_other": ("food", "weapon", "armor", "ring", "wand")}
# Treasure rooms draw one band; rings and wands are omitted to fit 64 choices.
TREASURE = ("potion", "scroll", "food", "weapon", "armor")


def choices(categories=tuple(CATEGORIES)):
    """(archetype, weight, quantity) with weight = category % x subtype %."""
    rows = []
    for name in categories:
        category, subtypes = CATEGORIES[name]
        for subtype, weight in subtypes.items():
            quantity = MISSILES if subtype in ("arrow", "dart", "shuriken") else None
            rows.append(("rogue." + subtype, category * weight, quantity))
    return rows


def group_chance(categories):
    """(numerator, denominator) of 36% x the categories' share of all items."""
    share = sum(CATEGORIES[name][0] for name in categories)
    return 36 * share, 100 * sum(weight for weight, _ in CATEGORIES.values())


def foe_bands(text):
    """The raw actor band list of the ordinary foe stage, for treasure rooms."""
    start = text.index('id = "rogue.eight_species_foes"')
    bands = text.index("bands = [", start)
    end = text.index("\n]\n", bands) + 3
    return text[bands + len("bands = "):end]


def choice_rows(categories):
    rows = []
    for archetype, weight, quantity in choices(categories):
        extra = f", quantity = {quantity}" if quantity else ""
        rows.append(f'{{ archetype = "{archetype}", weight = {weight}{extra} }}')
    return rows


def render(text):
    lines = [BEGIN,
             "# Rogue's ordinary item placement: nine attempts at 36%, weights are\n",
             "# category percent x subtype percent. Values match the research tables.\n"]
    for stage, categories in GROUPS.items():
        numerator, denominator = group_chance(categories)
        lines += ['[[floor_activation.profiles."rogue.low_depth".stages]]\n',
                  f'id = "{stage}"\n', "version = 1\n", 'kind = "chance_items"\n',
                  "attempts = 9\n",
                  f"chance = {{ numerator = {numerator}, denominator = {denominator} }}\n",
                  "quantity = 1\n",
                  'bands = [{ depth = { kind = "open", minimum = 1 }, choices = [\n']
        lines += [f"  {row},\n" for row in choice_rows(categories)]
        lines += ["] }]\n", "\n"]
    lines += ["# Rogue forces food on the fourth new floor in a row without any: its\n",
              "# counter includes the new floor, while this threshold counts earlier floors.\n",
              '[[floor_activation.profiles."rogue.low_depth".stages]]\n',
              'id = "rogue.food_drought"\n', "version = 1\n", 'kind = "food_drought"\n',
              "threshold = 2\n",
              "food = [" + ", ".join(f'"{name}"' for name in FOOD) + "]\n",
              "quantity = 1\n",
              'bands = [{ depth = { kind = "open", minimum = 1 }, choices = '
              '[{ archetype = "rogue.ration", weight = 9 }, { archetype = "rogue.fruit", weight = 1 }] }]\n',
              "\n",
              "# Rogue's treasure room: 1 in 20 floors, monsters drawn as if one level\n",
              "# deeper. Counts are provisional tuning; rings and wands are omitted.\n",
              '[[floor_activation.profiles."rogue.low_depth".stages]]\n',
              'id = "rogue.treasure_room"\n', "version = 1\n", 'kind = "room_cluster"\n',
              "chance = { numerator = 1, denominator = 20 }\n",
              "item_count = [2, 9]\n", "actor_count = [4, 11]\n", "quantity = 1\n",
              "actor_depth_offset = 1\n", 'ai = "rogue_hostile"\n',
              'depth_growth = { kind = "racial_after", depth = 26 }\n',
              "avoid_protagonist_sight = false\n",
              'item_bands = [{ depth = { kind = "open", minimum = 1 }, choices = ['
              + ", ".join(choice_rows(TREASURE)) + "] }]\n",
              "actor_bands = " + foe_bands(text),
              END]
    return "".join(lines)


REPLACED = ("rogue.rations", "rogue.restore_strength", "rogue.magic_potions", "rogue.magic_scrolls",
            "rogue.magic_devices", "rogue.equipment", "rogue.equipment_scrolls",
            "rogue.ammunition", "rogue.rings", "rogue.control_scrolls")


def blocks(text):
    """Split into top-level blocks; leading comments belong to the next table."""
    lines, result, current = text.splitlines(keepends=True), [], []
    pending = []
    for line in lines:
        if line.startswith("#"):
            pending.append(line)
            continue
        if line.startswith("["):
            result.append(current)
            current = pending
            pending = []
        elif pending:
            current += pending
            pending = []
        current.append(line)
    result.append(current + pending)
    return ["".join(block) for block in result if block]


def stage_id(block):
    if '[[floor_activation.profiles."rogue.low_depth".stages]]' not in block:
        return None
    match = re.search(r'^id = "([^"]+)"', block, flags=re.M)
    return match.group(1) if match else None


def rewrite(text):
    if BEGIN in text:
        start, end = text.index(BEGIN), text.index(END) + len(END)
        return text[:start] + render(text) + text[end:]
    kept, inserted = [], False
    for block in blocks(text):
        if stage_id(block) in REPLACED:
            if not inserted:
                kept.append(render(text) + "\n")
                inserted = True
            continue
        kept.append(block)
    if not inserted:
        raise ValueError("No replaceable item stages found")
    return "".join(kept)


def main():
    MANIFEST.write_text(rewrite(MANIFEST.read_text(encoding="utf-8")), encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
