"""Measure Layer 1 encounter ratios in TOR against the Rogue reference targets.

For each species, this builds an ordinary arena package from `scenarios/rogue`:
the locked Rogue adventurer, grown along its own growth schedule to the
reference level for that species' typical depth, against one ordinary actor of
the species. It runs `tor-arena --all-ai` over paired seeds and reports TOR's
hit chances, damage per action and actions to kill beside the Rogue targets
from rogue_reference_targets.py, with a ratio and an in/out-of-tolerance flag.

    python scripts/rogue_encounter_study.py --output .local/encounters --seeds 0-7
"""
import argparse
import copy
import json
from pathlib import Path
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source
import rogue_reference_targets as reference

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue"
STUDY = "tor-rogue-encounter-study-1"
# Ratio tolerance for "tuned": measured / target within [1/(1+t), 1+t].
DEFAULT_TOLERANCE = 0.35
ACTOR_ID = 3
# Study-only arrangements so every species yields a measured fight. They edit
# the study copy, never the scenario:
# - P: Rogue's phantom is invisible; the adventurer wears see invisible, as a
#   Rogue player facing one would, so it can be engaged at all.
# - O, F, X: the orc usually dies to one hit, the flytrap fights from a hold and
#   the xeroc stays disguised in about half its fights, so they get more seeds
#   for a usable count of attacks.
SEE_INVISIBLE = "rogue.ring_see_invisible"
RING_SLOT = 7  # first ring socket of the default anatomy
EXTRA_SEEDS = {"O": 4, "F": 4, "X": 4}
# The revealed xeroc waits: it engages first as well, so its own attacks are
# measured. The greedy orc needs no edit: with no Gold in the arena it acquires
# the adventurer itself, as a Rogue orc in a room without gold does.
FOE_ENGAGES = {"X": ("rogue.human", "rogue.xeroc")}
METRICS = ("player_hit", "foe_hit", "player_damage_per_action", "foe_damage_per_action",
           "player_actions_to_kill", "foe_actions_to_kill", "signature_per_action")
# The arena effect each species' signature applies. Rogue's dragon breathes as
# its own ranged action rather than as a hit rider, so the per-hit signature
# target cannot judge it.
SIGNATURE_EFFECTS = {
    "A": ("corrode_armor",), "I": ("apply_condition:rogue.frozen",),
    "R": ("lose_attribute",), "L": ("theft",), "N": ("theft",),
    "F": ("apply_linked_hold",), "V": ("lose_maximum_health",),
    "W": ("drain_latest_hit_die",), "M": ("apply_condition:rogue.confused",),
}
# Thieves need something to take: Rogue players carry gold and magic items.
STUDY_GOLD = 50
STUDY_CHARM = "rogue.healing"


def archetype(glyph):
    return "rogue." + reference.model.MONSTERS[glyph][0].replace(" ", "_")


def load_source():
    def read(name):
        return tomllib.loads((PACKAGE / name).read_text(encoding="utf-8"))
    return read("scenario.toml"), read("catalog/actors.toml")["actors"]


def leveled_character(manifest, level):
    """The locked adventurer with its first `level - 1` scheduled rows already grown."""
    character = copy.deepcopy(manifest["characters"][0])
    build = character["creature"]
    schedule = build["growth_schedule"]
    grown = min(max(level - 1, 0), len(schedule))
    build["hit_dice"] = build["hit_dice"] + schedule[:grown]
    build["growth_schedule"] = schedule[grown:]
    if not build["growth_schedule"]:
        # An exhausted schedule cannot keep authored future growth.
        build["future_growth"] = {"kind": "disabled"}
        del build["growth_schedule"]
    character.update(anchor="1/start", unselected="ai", ai="study_rogue")
    return character


def encounter(manifest, actors, glyph, level, mirrored=False):
    name = archetype(glyph)
    if name not in actors:
        raise ValueError(f"No actor archetype for {glyph}: {name}")
    character = leveled_character(manifest, level)
    if glyph == "P":
        character["loadout"].append({"archetype": SEE_INVISIBLE, "quantity": 1,
                                     "equipped_slot": RING_SLOT})
        character["known_identities"].append(SEE_INVISIBLE)
    if glyph == "L":
        character["loadout"].append({"archetype": "rogue.gold", "quantity": STUDY_GOLD})
    if glyph == "N":
        character["loadout"].append({"archetype": STUDY_CHARM, "quantity": 1})
    profiles = copy.deepcopy(manifest["ai_profiles"])
    profiles["study_rogue"] = {"idle": "explore", "doors": True, "care": True, "equipment": True,
                               "loot": True, "gold": False, "flee_percent": 0}
    study = {
        "format": manifest["format"], "id": f"rogue-encounter-{glyph.lower()}-{level}",
        "version": "1.0", "ruleset": "rules.toml", "modules": copy.deepcopy(manifest["modules"]),
        "default_character": character["id"], "characters": [character],
        "factions": copy.deepcopy(manifest["factions"]), "ai_profiles": profiles,
        "capabilities": {"npc_stair_traversal": True, "owner_advancement": False, "corpses": False},
        "themes": copy.deepcopy(manifest["themes"]), "assets": copy.deepcopy(manifest["assets"]),
        "arena": {"participants": [character["id"], ACTOR_ID], "control": "all_ai",
                  "ticks": 40000, "actions": 4000},
    }
    start, target = ([7, 2, 0], [1, 2, 0]) if mirrored else ([1, 2, 0], [7, 2, 0])
    region = {"id": 1, "name": "Rogue encounter study", "size": [9, 5, 2], "chamber": True,
              "gravity": [0, 0, -1], "anchors": {"start": start},
              "actors": [{"id": ACTOR_ID, "at": target, "archetype": name, "controller": "ai",
                          "ai": "rogue_hostile"}]}
    return study, region


def make_mean(text, species):
    start = text.index(f'[creatures.species."{species}"]')
    following = text.find("[creatures.species.", start + 1)
    end = following if following >= 0 else len(text)
    block = text[start:end].replace('disposition = "passive"', 'disposition = "mean"', 1)
    return text[:start] + block + text[end:]


def write_package(directory, study, region, compiler):
    if directory.exists():
        shutil.rmtree(directory)
    shutil.copytree(PACKAGE, directory, ignore=shutil.ignore_patterns("index.json", "validation.json"))
    (directory / "scenario.toml").write_text(toml_source(study), encoding="utf-8", newline="\n")
    # Combat ratios, not acquisition, are measured: one side engages first, so
    # passive species still fight. This edits only the study copy.
    creatures = directory / "catalog/creatures.toml"
    glyph = study["id"].split("-")[2].upper()
    text = creatures.read_text(encoding="utf-8")
    for species in FOE_ENGAGES.get(glyph, ("rogue.human",)):
        text = make_mean(text, species)
    creatures.write_text(text,
                         encoding="utf-8", newline="\n")
    (directory / "regions").mkdir()
    (directory / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    result = subprocess.run([str(compiler), "validate", str(directory)], capture_output=True,
                            text=True, encoding="utf-8", timeout=120)
    if result.returncode:
        raise RuntimeError(f"{directory.name}: {result.stderr.strip()}")


def run(arena, directory, seed):
    result = subprocess.run([str(arena), "--scenario", str(directory), "--seed", str(seed), "--all-ai"],
                            capture_output=True, text=True, encoding="utf-8", timeout=300)
    try:
        report = json.loads(result.stdout)
    except json.JSONDecodeError:
        report = None
    if result.returncode and (report is None or report.get("status") != "failure"):
        raise RuntimeError(f"{directory.name} seed {seed}: {result.stderr.strip()}")
    # An arena-reported failure is kept as an explicit outcome, never pooled.
    return report


def side(report, actor):
    return next(row for row in report["participants"] if str(row["actor"]) == str(actor))


def measure(reports, player, foe, foe_strikes, signature=()):
    """Pool encounter counts; a ratio is reported only when its denominator exists.

    The arena reports primary attack checks per strike, not attack actions, so an
    action is one strike for the adventurer's mace and `foe_strikes` for the foe's
    natural sequence (Rogue's attack components). `signature` names the arena
    effects that count as the foe's signature.
    """
    totals = {key: 0 for key in ("player_checks", "player_hits", "foe_checks", "foe_hits",
                                 "player_damage", "foe_damage", "foe_health", "player_health",
                                 "foe_signature")}
    for report in reports:
        mine, theirs = side(report, player), side(report, foe)
        for prefix, row in (("player", mine), ("foe", theirs)):
            totals[prefix + "_checks"] += int(row["attack_checks"])
            totals[prefix + "_hits"] += int(row["hits"])
            totals[prefix + "_damage"] += int(row["damage_dealt"])
        totals["foe_signature"] += sum(int(row["applications"])
                                       for name, row in theirs.get("effects", {}).items()
                                       if name in signature)
        totals["foe_health"] += int(theirs["initial"]["maximum_health"])
        totals["foe_health_minimum"] = min(totals.get("foe_health_minimum") or 10**9,
                                           int(theirs["initial"]["maximum_health"]))
        totals["player_health"] += int(mine["initial"]["maximum_health"])

    def ratio(numerator, denominator, scale=1):
        return totals[numerator] * scale / totals[denominator] if totals[denominator] else None
    player_dpa = ratio("player_damage", "player_checks")
    foe_dpa = ratio("foe_damage", "foe_checks", foe_strikes)
    count = len(reports)
    return {
        "player_hit": ratio("player_hits", "player_checks"),
        "foe_hit": ratio("foe_hits", "foe_checks"),
        "player_damage_per_action": player_dpa,
        "foe_damage_per_action": foe_dpa,
        "player_actions_to_kill": totals["foe_health"] / count / player_dpa if player_dpa else None,
        "foe_actions_to_kill": totals["player_health"] / count / foe_dpa if foe_dpa else None,
        "signature_per_action": (ratio("foe_signature", "foe_checks", foe_strikes)
                                 if signature else None),
        "counts": totals,
    }


def compare(target, measured, tolerance):
    rows = {}
    expected = {
        "player_hit": target.player_hit,
        # Rogue rolls each component with the same chance; compare the mean.
        "foe_hit": sum(target.foe_hits) / len(target.foe_hits) if target.foe_hits else None,
        "player_damage_per_action": target.player_damage_per_action,
        "foe_damage_per_action": target.foe_damage_per_action or None,
        "player_actions_to_kill": target.player_actions_to_kill,
        "foe_actions_to_kill": target.foe_actions_to_kill,
        "signature_per_action": target.signature_per_action or None,
    }
    for metric in METRICS:
        want, got = expected[metric], measured[metric]
        ratio = got / want if want and got is not None else None
        # Arena damage excludes overkill, so the adventurer's damage per action
        # reads low against small foes; it is reported but not judged.
        judged = ratio is not None and metric != "player_damage_per_action"
        rows[metric] = {"target": want, "measured": got, "ratio": ratio,
                        "within": 1 / (1 + tolerance) <= ratio <= 1 + tolerance if judged else None}
    return rows


def parse_seeds(text):
    first, _, last = text.partition("-")
    return list(range(int(first), int(last or first) + 1))


def study(output, compiler, arena, seeds, glyphs, tolerance=DEFAULT_TOLERANCE):
    manifest, actors = load_source()
    targets = {row.glyph: row for row in reference.targets()}
    results = []
    for glyph in glyphs:
        target = targets[glyph]
        reports = []
        for mirrored in (False, True):
            directory = output / f"{glyph.lower()}-{'mirrored' if mirrored else 'forward'}"
            package, region = encounter(manifest, actors, glyph, target.player_level, mirrored)
            write_package(directory, package, region, compiler)
            span = EXTRA_SEEDS.get(glyph, 1)
            species_seeds = [seed * span + offset for seed in seeds for offset in range(span)]
            reports.extend(run(arena, directory, seed) for seed in species_seeds)
        strikes = max(1, len(reference.model.MONSTERS[glyph][5]))
        completed = [report for report in reports if "termination" in report]
        measured = (measure(completed, manifest["characters"][0]["id"], ACTOR_ID, strikes,
                            SIGNATURE_EFFECTS.get(glyph, ()))
                    if completed else {metric: None for metric in METRICS} | {"counts": None})
        outcomes = [report["termination"].get("winner", report["termination"]["type"])
                    if "termination" in report else "failure: " + report["message"]
                    for report in reports]
        results.append({"glyph": glyph, "species": target.species, "depth": target.depth,
                        "target_foe_hp": target.foe_hp,
                        "player_level": target.player_level, "encounters": len(reports),
                        "outcomes": outcomes, "comparison": compare(target, measured, tolerance),
                        "counts": measured["counts"]})
    return {"study": STUDY, "targets": reference.TARGETS, "tolerance": tolerance,
            "seeds": seeds, "species": results}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/debug")
    parser.add_argument("--seeds", default="0-3", type=parse_seeds)
    parser.add_argument("--species", default=reference.model.POPULATION_ORDER)
    parser.add_argument("--tolerance", default=DEFAULT_TOLERANCE, type=float)
    args = parser.parse_args(argv)
    suffix = ".exe" if (args.bin / "tor-arena.exe").exists() else ""
    args.output.mkdir(parents=True, exist_ok=True)
    report = study(args.output, args.bin / ("tor-scenario" + suffix), args.bin / ("tor-arena" + suffix),
                   args.seeds, args.species, args.tolerance)
    (args.output / "study.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    for row in report["species"]:
        flags = "".join("." if cell["within"] else ("?" if cell["within"] is None else "x")
                        for cell in row["comparison"].values())
        print(row["glyph"], row["species"], "L" + str(row["player_level"]), flags)


if __name__ == "__main__":
    main()
