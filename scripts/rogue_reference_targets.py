"""Per-species Rogue encounter targets against the reference adventurer.

Layer 1 of the tuning plan (docs/plans/rogue-tuning.md). For each of the 26
species, at its typical depth, this derives from Rogue's combat rules (R05):
hit chance both ways, expected damage per action both ways, actions to kill
both ways, and the per-action rate of its signature effect. Targets are ratios
of actions, so they survive TOR's tick timing. The reference adventurer is the
model's expected level at that depth with Rogue's starting kit.

    python scripts/rogue_reference_targets.py > targets.json
"""
from dataclasses import asdict, dataclass
from fractions import Fraction
import json

import rogue_reference_model as model

TARGETS = "rogue-reference-targets-1"
# Rogue start (R01): +1 hit / +1 damage mace 2d4, Strength 16 (+1 damage),
# +1 ring mail giving AC 6. Found armor is not modeled; this is the kit baseline.
PLAYER_WEAPON = "2d4"
PLAYER_HIT_BONUS = 1
PLAYER_DAMAGE_BONUS = 1 + 1
PLAYER_AC = 6
SAVE_POISON, SAVE_BREATH, SAVE_MAGIC = 0, 2, 3


def hit_chance(attacker_level, defender_ac, bonus=0):
    """swing(): rnd(20) + bonus >= 20 - level - AC, with rnd(20) in 0..19."""
    return Fraction(min(20, max(0, attacker_level + defender_ac + bonus)), 20)


def save_chance(level, category):
    """1d20 >= 14 + category - floor(level / 2)."""
    need = 14 + category - level // 2
    return Fraction(min(20, max(0, 21 - need)), 20)


def signature_per_hit(glyph, player_level):
    """Chance that one landed hit applies the species' distinguishing effect."""
    fails = {category: 1 - save_chance(player_level, category)
             for category in (SAVE_POISON, SAVE_BREATH, SAVE_MAGIC)}
    return {
        "A": ("rust armor", Fraction(1)),
        "I": ("freeze", Fraction(1)),
        "R": ("strength poison", fails[SAVE_POISON]),
        "L": ("steal gold", fails[SAVE_MAGIC]),
        "N": ("steal magic item", Fraction(1)),
        "F": ("hold", Fraction(1)),
        "V": ("maximum-Health drain", Fraction(3, 10)),
        "W": ("level drain", Fraction(15, 100)),
        "M": ("gaze confusion", fails[SAVE_MAGIC]),
        "D": ("flame breath", fails[SAVE_BREATH]),
    }.get(glyph)


def typical_depth(glyph):
    """Center of the ten depths that can draw this species' population index."""
    index = model.POPULATION_ORDER.index(glyph)
    return min(26, max(1, index + 2))


@dataclass
class SpeciesTarget:
    glyph: str
    species: str
    depth: int
    player_level: int
    player_hp: float
    foe_level: int
    foe_hp: float
    player_hit: float
    foe_hits: list
    player_damage_per_action: float
    foe_damage_per_action: float
    player_actions_to_kill: float | None
    foe_actions_to_kill: float | None
    signature: str
    signature_per_action: float


def attack_distribution(hits, dice, bonus=0):
    """Damage of one attack action: independent components, misses deal 0."""
    total = {0: Fraction(1)}
    for chance, text in zip(hits, dice):
        count, sides = (int(part) for part in text.split("d"))
        damage = {0: 1 - chance}
        if sides:
            for amount, weight in model.dice_distribution(count, sides).items():
                value = max(0, amount + bonus)
                damage[value] = damage.get(value, 0) + chance * weight
        else:
            damage[0] += chance
        following = {}
        for already, first in total.items():
            for amount, second in damage.items():
                following[already + amount] = following.get(already + amount, 0) + first * second
        total = following
    return total


def expected_actions_to_kill(health, attack, limit=400):
    """E[attacks until cumulative damage >= Health], Health given as a distribution.

    Kills are granular: a 4-Health kestrel dies to any single hit, so this is
    not Health divided by mean damage. Truncated at `limit` attacks.
    """
    if not any(amount for amount in attack if attack[amount]):
        return None
    attack = {amount: float(chance) for amount, chance in attack.items()}
    expected = 0.0
    for hp, chance in health.items():
        # Distribution of damage still below `hp`, attack by attack.
        chance, alive, survived = float(chance), {0: 1.0}, 1.0
        attacks = 0
        while survived > 1e-9 and attacks < limit:
            expected += chance * survived
            following = {}
            for done, weight in alive.items():
                for amount, probability in attack.items():
                    total = done + amount
                    if total < hp:
                        following[total] = following.get(total, 0) + weight * probability
            alive = following
            survived = sum(alive.values())
            attacks += 1
    return expected


def target(glyph, levels):
    name, _, _, _, _, components = model.MONSTERS[glyph]
    depth = typical_depth(glyph)
    stats = model.monster_stats(glyph, depth)
    player_level = levels[depth]
    player_hp = model.START_HP + (player_level - 1) * model.LEVEL_HP_MEAN
    player_hit = hit_chance(player_level, stats["armor"], PLAYER_HIT_BONUS)
    player_damage = player_hit * (model.dice_mean(PLAYER_WEAPON) + PLAYER_DAMAGE_BONUS)
    foe_hits = [hit_chance(stats["level"], PLAYER_AC) for _ in components]
    foe_damage = sum(chance * model.dice_mean(dice) for chance, dice in zip(foe_hits, components))
    miss = Fraction(1)
    for chance in foe_hits:
        miss *= 1 - chance
    # Zero-damage contact species (aquator, ice monster, nymph) still need a hit;
    # the flytrap's special hold is treated as one ordinary contact roll.
    any_hit = 1 - miss if foe_hits else hit_chance(stats["level"], PLAYER_AC)
    signature = signature_per_hit(glyph, player_level)
    foe_health = model.dice_distribution(stats["level"], 8)
    player_attack = attack_distribution([player_hit], [PLAYER_WEAPON], PLAYER_DAMAGE_BONUS)
    foe_attack = attack_distribution(foe_hits, components)
    player_health = {round(player_hp): Fraction(1)}
    player_kill = expected_actions_to_kill(foe_health, player_attack)
    foe_kill = expected_actions_to_kill(player_health, foe_attack)
    return SpeciesTarget(
        glyph=glyph, species=name, depth=depth, player_level=player_level,
        player_hp=float(player_hp), foe_level=stats["level"], foe_hp=float(stats["hp"]),
        player_hit=float(player_hit), foe_hits=[float(chance) for chance in foe_hits],
        player_damage_per_action=float(player_damage),
        foe_damage_per_action=float(foe_damage),
        player_actions_to_kill=None if player_kill is None else float(player_kill),
        foe_actions_to_kill=None if foe_kill is None else float(foe_kill),
        signature=signature[0] if signature else "",
        signature_per_action=float(any_hit * signature[1]) if signature else 0.0,
    )


def targets(turns_per_floor=900):
    rows = model.expectations(set(range(1, 27)), turns_per_floor)
    levels = {row.depth: row.level_on_arrival for row in rows}
    return [target(glyph, levels) for glyph in model.POPULATION_ORDER]


def main(argv=None):
    import argparse
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--turns-per-floor", default=900, type=int)
    args = parser.parse_args(argv)
    print(json.dumps({"targets": TARGETS, "model": model.MODEL,
                      "turns_per_floor": args.turns_per_floor,
                      "species": [asdict(row) for row in targets(args.turns_per_floor)]},
                     indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
