"""Predict the locked adventurer's level by depth from the tuned values alone.

Deep floors are too slow and dangerous for the fixed campaign bot to measure
pacing, so this model applies TOR's own growth rules to the tuned package:
each foe's threat tier from its authored Hit Dice, ranks, talents and sources
(the same terms as `earned_growth::assess`), the kill reward
`base + per_tier_reward x (victim tier - recipient HD)` clamped to its bounds,
and the threshold `threshold_base + threshold_per_tier x (HD - 1)` per new Hit
Die. Kills per floor follow the package's four initial attempts, Rogue's
wanderer rate over the same time per floor as the reference model, and treasure
rooms. The result is compared with rogue_reference_model.py level by depth.

It predicts value-level pacing, not bot behavior: it assumes every monster on a
floor is killed by the adventurer, as the reference model does.

    python scripts/rogue_pacing_model.py --fit
"""
import argparse
from dataclasses import dataclass
import json
import math
from pathlib import Path
import tomllib

import rogue_reference_model as model

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue"
DEFAULT_RULES = ROOT / "crates/simulation/data/default-ruleset.toml"
FOES = "rogue.eight_species_foes"
TREASURE = "rogue.treasure_room"


@dataclass
class Growth:
    threshold_base: int
    threshold_per_tier: int
    reward_base: int
    reward_per_relative_tier: int
    minimum_reward: int
    maximum_reward: int
    threat: dict

    def reward(self, victim_tier, recipient_hd):
        amount = self.reward_base + (victim_tier - recipient_hd) * self.reward_per_relative_tier
        return max(self.minimum_reward, min(self.maximum_reward, amount))

    def threshold(self, hd):
        return self.threshold_base + self.threshold_per_tier * (max(hd, 1) - 1)


def load_growth(rules_path=PACKAGE / "rules.toml"):
    """Bundled growth values with the package's growth overrides applied."""
    growth = tomllib.loads(DEFAULT_RULES.read_text(encoding="utf-8"))["growth"]
    rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
    for row in rules.get("overrides", []):
        if row["path"][:1] == ["growth"] and len(row["path"]) == 2 and row["op"] == "set":
            growth[row["path"][1]] = row["value"]
    fields = {key: growth[key] for key in Growth.__dataclass_fields__ if key != "threat"}
    return Growth(threat=growth["threat"], **fields)


def threat_tier(actor, species, growth):
    """Tier of one authored foe: the terms `earned_growth::assess` sums."""
    threat = growth.threat
    rows = actor["creature"]["hit_dice"]
    trained = 2
    talents = set()
    classes = set()
    for row in rows:
        if row["source"] != "racial":
            classes.add(row["source"])
        for purchase in row.get("purchases", []):
            selection = purchase["selection"]
            if selection["kind"] == "skill_rank":
                trained += 1
            elif selection["kind"] in ("talent", "talent_form"):
                talents.add(selection["talent"])
    points = (threat["hit_die"] * len(rows) + threat["trained_rank"] * trained
              + threat["species"] + threat["creature_type"]
              + threat["subtype"] * len(species.get("subtypes", []))
              + threat["class"] * len(classes) + threat["talent"] * len(talents))
    return max(1, math.ceil(points / threat["points_per_tier"]))


def load_population():
    manifest = tomllib.loads((PACKAGE / "scenario.toml").read_text(encoding="utf-8"))
    actors = tomllib.loads((PACKAGE / "catalog/actors.toml").read_text(encoding="utf-8"))["actors"]
    species = tomllib.loads((PACKAGE / "catalog/creatures.toml").read_text(encoding="utf-8"))
    stages = {stage["id"]: stage for stage in
              manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]}
    return manifest, actors, species["creatures"]["species"], stages


def band_for(bands, depth):
    for band in bands:
        span = band["depth"]
        if depth >= span["minimum"] and (span["kind"] == "open" or depth <= span["maximum"]):
            return band
    return None


def expected_reward(bands, depth, recipient_hd, actors, species, growth):
    band = band_for(bands, depth)
    if band is None:
        return 0.0
    total = sum(choice["weight"] for choice in band["choices"])
    value = 0.0
    for choice in band["choices"]:
        actor = actors[choice["archetype"]]
        tier = threat_tier(actor, species[actor["creature"]["species"]], growth)
        value += choice["weight"] / total * growth.reward(tier, recipient_hd)
    return value


def kills_per_floor(stages, turns_per_floor):
    initial = stages[FOES]["count"][1]
    treasure = stages[TREASURE]
    treasure_kills = (treasure["chance"]["numerator"] / treasure["chance"]["denominator"]
                      * sum(treasure["actor_count"]) / 2)
    return initial, float(model.wanderers_per_turn() * turns_per_floor), treasure_kills


def levels_by_depth(growth, depths=26, turns_per_floor=900):
    """Level on arrival at each depth for the locked build (level = HD - 1)."""
    _, actors, species, stages = load_population()
    bands = stages[FOES]["bands"]
    initial, wanderers, treasure = kills_per_floor(stages, turns_per_floor)
    hd, held, levels = 2, 0.0, {}
    for depth in range(1, depths + 1):
        levels[depth] = hd - 1
        floor_xp = ((initial + wanderers) * expected_reward(bands, depth, hd, actors, species, growth)
                    + treasure * expected_reward(bands, depth + 1, hd, actors, species, growth))
        held += floor_xp
        while held >= growth.threshold(hd) and hd < 22:
            held -= growth.threshold(hd)
            hd += 1
    return levels


def reference_levels(depths=26, turns_per_floor=900):
    return {row.depth: row.level_on_arrival
            for row in model.expectations(set(range(1, depths + 1)), turns_per_floor)}


def error(levels, reference):
    return sum(abs(levels[depth] - reference[depth]) for depth in reference) / len(reference)


def fit(growth, reference, bases=range(20, 201, 10), per_tiers=range(0, 401, 10)):
    """Threshold values minimizing mean absolute level error over depths 1-26."""
    best = None
    for base in bases:
        for per_tier in per_tiers:
            growth.threshold_base, growth.threshold_per_tier = base, per_tier
            score = error(levels_by_depth(growth), reference)
            if best is None or score < best[0]:
                best = (score, base, per_tier)
    return best


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--fit", action="store_true", help="Search threshold values")
    args = parser.parse_args(argv)
    growth = load_growth()
    reference = reference_levels()
    current = levels_by_depth(growth)
    report = {"threshold_base": growth.threshold_base,
              "threshold_per_tier": growth.threshold_per_tier,
              "mean_level_error": error(current, reference),
              "levels": {str(depth): [current[depth], reference[depth]] for depth in reference}}
    if args.fit:
        score, base, per_tier = fit(growth, reference)
        growth.threshold_base, growth.threshold_per_tier = base, per_tier
        report["fit"] = {"threshold_base": base, "threshold_per_tier": per_tier,
                         "mean_level_error": score,
                         "levels": {str(depth): [level, reference[depth]] for depth, level
                                    in levels_by_depth(growth).items()}}
    print(json.dumps(report, indent=1))


if __name__ == "__main__":
    main()
