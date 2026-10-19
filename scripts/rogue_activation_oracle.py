"""Independent exact oracles for reviewed 21-, 24- and 26-species input.

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


def reviewed_twenty_four_activation():
    """Add M/P/X from the pinned selector, preserving the exact21 contract.

    Only the independently authored policy above is reused, never a supplied
    package. M is in the upper-clamp replacement set and retains its open tail.
    """
    activation = reviewed_twenty_one_activation()
    stage = activation["profiles"]["rogue.low_depth"]["stages"][0]
    order = (*ORDER, ("M", "medusa"), ("P", "phantom"), ("X", "xeroc"))
    stock = {choice["archetype"]: copy.deepcopy(choice["carried_loot"])
             for band in stage["bands"] for choice in band["choices"] if "carried_loot" in choice}
    for name, numerator, denominator in (("medusa", 2, 5), ("xeroc", 3, 10)):
        stock["rogue." + name] = {"version": 1, "pool": "rogue.vampire_supplies",
                                  "chance": {"numerator": numerator, "denominator": denominator}}
    bands = []
    for depth in range(1, 32):
        mass = dict.fromkeys((glyph for glyph, _ in order), 0)
        for offset in range(10):
            index = depth + offset - 6
            indices = range(5) if index < 0 else range(21, 26) if index > 25 else (index,)
            for selected in indices:
                glyph = SELECTOR[selected]
                if glyph in mass:
                    mass[glyph] += 5 // len(indices)
        divisor = math.gcd(*mass.values())
        choices = []
        for glyph, name in order:
            if mass[glyph]:
                identity = "rogue." + name
                choice = {"archetype": identity, "weight": mass[glyph] // divisor}
                if identity in stock:
                    choice["carried_loot"] = copy.deepcopy(stock[identity])
                choices.append(choice)
        if depth < 31 and bands and bands[-1]["choices"] == choices:
            bands[-1]["depth"]["maximum"] = depth
        else:
            bounds = ({"kind": "open", "minimum": depth} if depth == 31 else
                      {"kind": "finite", "minimum": depth, "maximum": depth})
            bands.append({"depth": bounds, "choices": choices})
    stage["bands"] = bands
    return activation


def reviewed_twenty_six_activation():
    """Add A/D without changing the historical conditional input contracts.

    All 26 source choices are now present, so no probability mass is discarded.
    The depth-31 Dragon outcome is still direct; the uniform tail starts at 32.
    Current finite magic and equipment stock appends after the five unchanged legacy stages.
    """
    activation = reviewed_twenty_four_activation()
    stage = activation["profiles"]["rogue.low_depth"]["stages"][0]
    order = (*ORDER, ("M", "medusa"), ("P", "phantom"), ("X", "xeroc"),
             ("A", "aquator"), ("D", "dragon"))
    stock = {choice["archetype"]: copy.deepcopy(choice["carried_loot"])
             for band in stage["bands"] for choice in band["choices"] if "carried_loot" in choice}
    stock["rogue.dragon"] = {"version": 1, "pool": "rogue.vampire_supplies",
                             "chance": {"numerator": 1, "denominator": 1}}
    # Provisional TOR tuning: ordinary paid ranks 3/4/5, not automatic HD ranks.
    advancement = [{"ordinal": ordinal, "purchases": [{"pool": "advancement", "selection": {
        "kind": "skill_rank", "skill": "breath"}}]} for ordinal in (15, 20, 25)]
    bands = []
    for depth in range(1, 33):
        mass = dict.fromkeys(SELECTOR, 0)
        for offset in range(10):
            index = depth + offset - 6
            indices = range(5) if index < 0 else range(21, 26) if index > 25 else (index,)
            for selected in indices:
                mass[SELECTOR[selected]] += 5 // len(indices)
        divisor = math.gcd(*mass.values())
        choices = []
        for glyph, name in order:
            if mass[glyph]:
                identity = "rogue." + name
                choice = {"archetype": identity, "weight": mass[glyph] // divisor}
                if identity in stock:
                    choice["carried_loot"] = copy.deepcopy(stock[identity])
                if glyph == "D":
                    choice["advancement"] = copy.deepcopy(advancement)
                choices.append(choice)
        bounds = ({"kind": "open", "minimum": depth} if depth == 32 else
                  {"kind": "finite", "minimum": depth, "maximum": depth})
        bands.append({"depth": bounds, "choices": choices})
    stage["bands"] = bands
    # Independently reviewed provisional TOR stock, not Rogue's probability
    # table. Keep these last so the existing deterministic streams stay owned.
    activation["profiles"]["rogue.low_depth"]["stages"].extend(
        {"id": identity, "version": 1, "kind": "items", "count": [1, 1],
         "bands": [{"depth": {"kind": "open", "minimum": 1},
                    "choices": [{"archetype": "rogue." + name, "weight": 1}
                                for name in names]}], "quantity": 1}
        for identity, names in (
            ("rogue.magic_potions", ("healing", "confusion", "blindness")),
            ("rogue.magic_scrolls", ("identify_potion", "identify_scroll", "identify_wand", "sleep")),
            ("rogue.magic_devices", ("wand_fire", "wand_cold", "wand_lightning")),
            ("rogue.equipment", ("generated_mace", "long_sword", "dagger", "two_handed_sword", "spear",
                                 "leather_armor", "generated_ring_mail", "studded_leather", "scale_mail",
                                 "chain_mail", "splint_mail", "banded_mail", "plate_mail", "shortbow")),
            ("rogue.equipment_scrolls", ("enchant_armor", "protect_armor", "enchant_weapon", "remove_curse",
                                        "identify_weapon", "identify_armor"))))
    # One finite stack attempt; six arrows is explicit provisional TOR tuning.
    activation["profiles"]["rogue.low_depth"]["stages"].append({
        "id": "rogue.ammunition", "version": 1, "kind": "items", "count": [1, 1],
        "bands": [{"depth": {"kind": "open", "minimum": 1},
                   "choices": [{"archetype": "rogue.arrow", "weight": 1}]}], "quantity": 6})
    return activation
