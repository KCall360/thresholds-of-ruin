"""Independent exact oracle for the reviewed twenty-one-species activation input.

This is an input adapter contract, not a candidate population or a live-package
loader. Expected selector mass, ordering and policies never come from supplied
content. This pins population policy, not arbitrary supplied catalog/rules
bytes. The generator retains and fingerprints the full declared source closure,
resolved package rules and dependency identities, while still emitting its
retained eighteen-species population.
"""
import copy
import math


# Pinned Rogue selector c7c119f893bd2f8255f2ad78d58a32664ddb9a97.
# Retain historical choice order, appending O/L/N rather than reordering streams.
SELECTOR = "KEBSHIROZLCQANYFTWPXUMVGJD"
ORDER = tuple(zip("KEBSHIRZCQYFTWUVGJOLN", (
    "kestrel", "emu", "bat", "snake", "hobgoblin", "ice_monster",
    "rattlesnake", "zombie", "centaur", "quagga", "yeti", "venus_flytrap",
    "troll", "wraith", "black_unicorn", "vampire", "griffin", "jabberwock",
    "orc", "leprechaun", "nymph"), strict=True))


def reviewed_twenty_one_activation():
    """Build fresh expected values without importing the candidate's band table."""
    stock = {glyph: {"version": 1, "pool": "rogue.vampire_supplies",
                     "chance": {"numerator": numerator, "denominator": denominator}}
             for glyph, numerator, denominator in (
                 ("C", 3, 20), ("Y", 3, 10), ("T", 1, 2),
                 ("V", 1, 5), ("G", 1, 5), ("J", 7, 10))}
    stock["L"] = {"version": 1, "pool": "rogue.leprechaun_gold",
                  "chance": {"numerator": 1, "denominator": 1}}

    def choices(depth):
        # Fifty equiprobable outcomes: a normal index owns five, each of the
        # five low/high boundary replacements owns one. Normalize exactly.
        mass = dict.fromkeys((glyph for glyph, _ in ORDER), 0)
        for offset in range(10):
            index = depth + offset - 6
            indices = range(5) if index < 0 else range(21, 26) if index > 25 else (index,)
            for selected in indices:
                glyph = SELECTOR[selected]
                if glyph in mass:
                    mass[glyph] += 5 // len(indices)
        divisor = math.gcd(*mass.values())
        result = []
        for glyph, name in ORDER:
            if mass[glyph]:
                choice = {"archetype": "rogue." + name, "weight": mass[glyph] // divisor}
                if glyph in stock:
                    choice["carried_loot"] = copy.deepcopy(stock[glyph])
                result.append(choice)
        return result

    bands = []
    for depth in range(1, 31):
        authored = choices(depth)
        if bands and bands[-1]["choices"] == authored:
            bands[-1]["depth"]["maximum"] = depth
        else:
            bands.append({"depth": {"kind": "finite", "minimum": depth, "maximum": depth},
                          "choices": authored})
    bands.append({"depth": {"kind": "open", "minimum": 31}, "choices": choices(31)})

    def singleton(archetype):
        return [{"depth": {"kind": "open", "minimum": 1},
                 "choices": [{"archetype": archetype, "weight": 1}]}]

    quantity = {"kind": "depth_scaled_uniform", "minimum": 2,
                "maximum_base": 51, "per_depth": 10}
    stages = [{"id": "rogue.eight_species_foes", "version": 1, "kind": "actors",
               "count": [4, 4], "bands": bands, "ai": "rogue_hostile",
               "depth_growth": {"kind": "racial_after", "depth": 26},
               "avoid_protagonist_sight": True}]
    stages.extend({"id": identity, "version": 1, "kind": "items", "count": [1, 1],
                   "bands": singleton(archetype), "quantity": count}
                  for identity, archetype, count in (
                      ("rogue.bandages", "rogue.clean_bandage", 2),
                      ("rogue.rations", "rogue.ration", 1),
                      ("rogue.restore_strength", "rogue.restore_strength", 1)))
    stages.append({"id": "rogue.room_gold", "version": 1, "kind": "room_items",
                   "chance": {"numerator": 1, "denominator": 2},
                   "bands": singleton("rogue.gold"), "quantity": copy.deepcopy(quantity)})
    return {"version": 1, "protagonist": "selected_character",
            "families": {"dungeon": "rogue.low_depth"},
            "profiles": {"rogue.low_depth": {"version": 1, "stages": stages}},
            "carried_item_pools": {
                "rogue.vampire_supplies": {"version": 1, "choices": [
                    {"archetype": "rogue.ration", "weight": 1, "quantity": 1},
                    {"archetype": "rogue.clean_bandage", "weight": 1, "quantity": 1}]},
                "rogue.leprechaun_gold": {"version": 1, "choices": [
                    {"archetype": "rogue.gold", "weight": 1, "quantity": quantity}]}}}
