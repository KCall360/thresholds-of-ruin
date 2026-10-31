"""Rogue 5.4.4's own expectations per depth, from the pinned research notes.

This is the tuning plan's reference model (docs/plans/rogue-tuning.md). It never
runs Rogue: it evaluates the documented generation, experience, hunger, wandering,
trap and healing rules exactly where they are closed-form and states its
assumptions where play behavior matters (time spent per floor, share of monsters
killed). Outputs are targets for the TOR scenario, not TOR measurements.

    python scripts/rogue_reference_model.py --depths 1-30 --turns-per-floor 900
"""
import argparse
from dataclasses import asdict, dataclass
from fractions import Fraction
import json

MODEL = "rogue-reference-model-1"

# docs/research/rogue-5.4.4/content.md#monsters-26
# glyph: (species, level, AC, base XP, carry percent, damage dice per component)
MONSTERS = {
    "A": ("aquator", 5, 2, 20, 0, ["0d0", "0d0"]),
    "B": ("bat", 1, 3, 1, 0, ["1d2"]),
    "C": ("centaur", 4, 4, 17, 15, ["1d2", "1d5", "1d5"]),
    "D": ("dragon", 10, -1, 5000, 100, ["1d8", "1d8", "3d10"]),
    "E": ("emu", 1, 7, 2, 0, ["1d2"]),
    "F": ("venus flytrap", 8, 3, 80, 0, []),
    "G": ("griffin", 13, 2, 2000, 20, ["4d3", "3d5"]),
    "H": ("hobgoblin", 1, 5, 3, 0, ["1d8"]),
    "I": ("ice monster", 1, 9, 5, 0, ["0d0"]),
    "J": ("jabberwock", 15, 6, 3000, 70, ["2d12", "2d4"]),
    "K": ("kestrel", 1, 7, 1, 0, ["1d4"]),
    "L": ("leprechaun", 3, 8, 10, 0, ["1d1"]),
    "M": ("medusa", 8, 2, 200, 40, ["3d4", "3d4", "2d5"]),
    "N": ("nymph", 3, 9, 37, 100, ["0d0"]),
    "O": ("orc", 1, 6, 5, 15, ["1d8"]),
    "P": ("phantom", 8, 3, 120, 0, ["4d4"]),
    "Q": ("quagga", 3, 3, 15, 0, ["1d5", "1d5"]),
    "R": ("rattlesnake", 2, 3, 9, 0, ["1d6"]),
    "S": ("snake", 1, 5, 2, 0, ["1d3"]),
    "T": ("troll", 6, 4, 120, 50, ["1d8", "1d8", "2d6"]),
    "U": ("black unicorn", 7, -2, 190, 0, ["1d9", "1d9", "2d9"]),
    "V": ("vampire", 8, 1, 350, 20, ["1d10"]),
    "W": ("wraith", 5, 4, 55, 0, ["1d6"]),
    "X": ("xeroc", 7, 7, 100, 30, ["4d4"]),
    "Y": ("yeti", 4, 6, 50, 30, ["1d6", "1d6"]),
    "Z": ("zombie", 2, 8, 6, 0, ["1d8"]),
}
POPULATION_ORDER = "KEBSHIROZLCQANYFTWPXUMVGJD"
WANDERING_EXCLUDED = set("ILNFXD")
# Successive thresholds starting at level 2 (R05).
XP_THRESHOLDS = [10, 20, 40, 80, 160, 320, 640, 1300, 2600, 5200, 13000, 26000, 50000,
                 100000, 200000, 400000, 800000, 2000000, 4000000, 8000000]
START_HP = 12
LEVEL_HP_MEAN = Fraction(11, 2)  # 1d10 per gained level
ROOM_SLOTS = 9
OMITTED_ROOMS = range(4)  # rnd(4) slots omitted, uniformly
ITEM_ATTEMPTS = 9
ITEM_CHANCE = Fraction(36, 100)
CATEGORY_WEIGHTS = {"potion": 26, "scroll": 36, "food": 16, "weapon": 7, "armor": 7,
                    "ring": 4, "wand": 4}
MEAL_MEAN = Fraction(1100 + 1499, 2)
# Rogue increments no_food before placing a level's items and forces food when
# it exceeds 3, so three earlier foodless levels force the next level's food.
DROUGHT_EARLIER_FLOORS = 3
WANDER_STARTUP_TURNS = 70  # nominal spread(70): 67..73, mean 70
WANDER_ROLL_INTERVAL = 4
WANDER_CHANCE = Fraction(1, 6)


def level_for_xp(xp):
    """Experience level reached with total XP (level 1 below the first threshold)."""
    return 1 + sum(1 for threshold in XP_THRESHOLDS if xp >= threshold)


def species_distribution(depth, wandering=False):
    """Exact species probabilities for one new monster at a depth.

    Index is depth + rnd(10) - 6; negative indices become rnd(5) and indices
    above 25 become 21 + rnd(5) (R08). Wandering draws reroll excluded species.
    """
    weights = {glyph: Fraction(0) for glyph in POPULATION_ORDER}
    for roll in range(10):
        index = depth + roll - 6
        if index < 0:
            for low in range(5):
                weights[POPULATION_ORDER[low]] += Fraction(1, 50)
        elif index > 25:
            for high in range(21, 26):
                weights[POPULATION_ORDER[high]] += Fraction(1, 50)
        else:
            weights[POPULATION_ORDER[index]] += Fraction(1, 10)
    if wandering:
        for glyph in WANDERING_EXCLUDED:
            weights[glyph] = Fraction(0)
        total = sum(weights.values())
        if total == 0:
            raise ValueError(f"no wandering species at depth {depth}")
        weights = {glyph: weight / total for glyph, weight in weights.items()}
    return {glyph: weight for glyph, weight in weights.items() if weight}


def dice_distribution(count, sides):
    """Exact distribution of the sum of `count` dice with `sides` faces."""
    distribution = {0: Fraction(1)}
    for _ in range(count):
        following = {}
        for total, chance in distribution.items():
            for face in range(1, sides + 1):
                following[total + face] = following.get(total + face, 0) + chance / sides
        distribution = following
    return distribution


def dice_mean(dice):
    count, sides = (int(part) for part in dice.split("d"))
    return Fraction(count * (sides + 1), 2) if sides else Fraction(0)


def monster_stats(glyph, depth):
    """Depth-scaled level, AC and expected kill XP for one monster (R08)."""
    _, level, armor, base_xp, _, _ = MONSTERS[glyph]
    extra = max(0, depth - 26)
    level, armor = level + extra, armor - extra
    expected_adjustment = Fraction(0)
    for hp, chance in dice_distribution(level, 8).items():
        adjustment = hp // 8 if level == 1 else hp // 6
        if level > 9:
            adjustment *= 20
        elif level > 6:
            adjustment *= 4
        expected_adjustment += chance * adjustment
    return {"level": level, "armor": armor,
            "hp": Fraction(9 * level, 2),
            "xp": base_xp + 10 * extra + expected_adjustment}


def expected_kill_xp(depth, wandering=False):
    return sum(chance * monster_stats(glyph, depth)["xp"]
               for glyph, chance in species_distribution(depth, wandering).items())


def initial_monsters_per_floor():
    """Present rooms x (1/2 gold -> 80%, otherwise 25%) monster chance (R02)."""
    rooms = Fraction(sum(ROOM_SLOTS - omitted for omitted in OMITTED_ROOMS), len(OMITTED_ROOMS))
    return rooms * (Fraction(1, 2) * Fraction(80, 100) + Fraction(1, 2) * Fraction(25, 100))


def wanderers_per_turn():
    """Mean arrival rate: startup delay, then a 1/6 roll every four turns."""
    mean_wait = WANDER_STARTUP_TURNS + WANDER_ROLL_INTERVAL / WANDER_CHANCE
    return 1 / mean_wait


def items_per_floor():
    return ITEM_ATTEMPTS * ITEM_CHANCE


def food_share():
    return Fraction(CATEGORY_WEIGHTS["food"], sum(CATEGORY_WEIGHTS.values()))


def food_per_floor():
    """Steady-state food items per new floor with the drought guarantee.

    Model assumption (recorded in the tuning plan): the drought counter counts
    consecutive generated floors without food; on a floor where it exceeds the
    threshold, the first created item is food. A floor with no created items
    cannot satisfy the guarantee. Returns (mean food items, drought floor share).
    """
    attempts, chance, share = ITEM_ATTEMPTS, ITEM_CHANCE, food_share()
    nothing = (1 - chance) ** attempts
    ordinary_none = (1 - chance * share) ** attempts
    ordinary_mean = attempts * chance * share
    # Forced floor: if any item is created, the first is food; the remaining
    # attempts after it are ordinary. Expected food = P(any) + E[ordinary later].
    forced_mean = Fraction(0)
    for first in range(attempts):
        reach = (1 - chance) ** first * chance
        forced_mean += reach * (1 + (attempts - first - 1) * chance * share)
    # States are the count of earlier foodless floors; below three they are
    # ordinary, at three the floor is forced.
    # A floor without food advances the counter, food resets it. The renewal
    # chain's stationary weights are products of the preceding failure chances,
    # and the forced state repeats only when no item is created at all.
    weights = [Fraction(1)]
    for _ in range(DROUGHT_EARLIER_FLOORS):
        weights.append(weights[-1] * ordinary_none)
    # The forced state loops on itself when nothing is created.
    weights[-1] = weights[-1] / (1 - nothing)
    total = sum(weights)
    probability = [weight / total for weight in weights]
    mean = sum(probability[:-1]) * ordinary_mean + probability[-1] * forced_mean
    return mean, probability[-1]


def traps_per_floor(depth):
    """Trap chance rnd(10) < depth, count min(10, rnd(depth/4) + 1), none below 4."""
    span = depth // 4
    if span == 0:
        return Fraction(0)
    chance = Fraction(min(depth, 10), 10)
    mean_count = sum(Fraction(min(10, roll + 1), span) for roll in range(span))
    return chance * mean_count


def healing_per_turn(level):
    """Natural healing rate from the doctor daemon (R06)."""
    if level < 8:
        return Fraction(1, max(1, 21 - 2 * level))
    return Fraction(level - 6, 2) / 3


@dataclass
class DepthExpectation:
    depth: int
    level_on_arrival: int
    xp_on_arrival: float
    hp_on_arrival: float
    initial_monsters: float
    wanderers: float
    kill_xp: float
    species: dict
    items: float
    food_items: float
    food_units: float
    food_spent: float
    traps: float
    healing_per_turn: float


def expectations(depths, turns_per_floor=900, kill_share=Fraction(1)):
    """Per-depth expectations for a descent that clears `kill_share` of monsters.

    `turns_per_floor` and `kill_share` are play-behavior assumptions, not Rogue
    rules; the campaign harness measures their TOR counterparts.
    """
    food_items, _ = food_per_floor()
    rows, xp = [], Fraction(0)
    for depth in range(1, max(depths) + 1):
        initial = initial_monsters_per_floor()
        wanderers = wanderers_per_turn() * turns_per_floor
        level = level_for_xp(xp)
        row = DepthExpectation(
            depth=depth,
            level_on_arrival=level,
            xp_on_arrival=float(xp),
            hp_on_arrival=float(START_HP + (level - 1) * LEVEL_HP_MEAN),
            initial_monsters=float(initial),
            wanderers=float(wanderers),
            kill_xp=float(kill_share * (initial * expected_kill_xp(depth)
                                        + wanderers * expected_kill_xp(depth, True))),
            species={glyph: float(chance) for glyph, chance in species_distribution(depth).items()},
            items=float(items_per_floor()),
            food_items=float(food_items),
            food_units=float(food_items * MEAL_MEAN),
            food_spent=float(turns_per_floor),
            traps=float(traps_per_floor(depth)),
            healing_per_turn=float(healing_per_turn(level)),
        )
        xp += Fraction(row.kill_xp)
        if depth in depths:
            rows.append(row)
    return rows


def parse_depths(text):
    first, _, last = text.partition("-")
    return set(range(int(first), int(last or first) + 1))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--depths", default="1-30", type=parse_depths)
    parser.add_argument("--turns-per-floor", default=900, type=int)
    parser.add_argument("--kill-share", default="1", type=Fraction)
    args = parser.parse_args(argv)
    rows = expectations(args.depths, args.turns_per_floor, args.kill_share)
    print(json.dumps({"model": MODEL, "turns_per_floor": args.turns_per_floor,
                      "kill_share": str(args.kill_share),
                      "depths": [asdict(row) for row in rows]}, indent=2))


if __name__ == "__main__":
    main()
