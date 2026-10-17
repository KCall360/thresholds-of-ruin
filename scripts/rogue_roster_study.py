"""New reconstruction of bounded Rogue reference-study preparation.

The missing original study base was not recovered. This implementation uses the
retained playable-mode contract and the existing starter-study/matrix helpers.
It writes ordinary validated packages and arena_evaluation plans, never battles,
production population edits, numeric balance claims or earned-route evidence.
"""
import argparse
import copy
import hashlib
import itertools
from pathlib import Path
import subprocess
import sys
import tomllib

from arena_evaluation import digest, files, participant_contract, seed, timeout_seconds, write_json
from arena_matrix import REQUIRED_HD_LEVELS, encounter, toml_source
from rogue_roster_package import PLAYABLE_SPECIES, SPECIES
from ruleset_sources import resolve_with_dependencies

ROOT = Path(__file__).resolve().parents[1]
PRODUCTION = "scenarios/rogue-exploration/"
DEFAULT_RULES = "crates/simulation/data/default-ruleset.toml"
FORMAT = "tor-rogue-roster-study-v1"
REFERENCES = ("arena-diagnostic", "playable-unspent", "playable-first-purchase")
ARMS = {"baseline": "warrior_mixed", "candidate": "mage_mixed"}
ORIENTATIONS = ("forward", "mirrored")
DEFAULT_SEEDS = ("0", "1")
# Concrete tor_simulation::ai::AiProfile::general() policy. Authoring's partial
# general() inherits species idle behavior, so the study specifies every field.
# The reconstructed production package has no explicit "normal" profile.
PLAYABLE_AI_NAME = "study_general"
PLAYABLE_AI = {"idle": "explore", "doors": True, "care": True, "equipment": True,
               "loot": True, "flee_percent": 25}
AUTHORING_AI_SOURCE = "crates/server/src/scenario_authoring.rs"
RUNTIME_AI_SOURCE = "crates/simulation/src/ai.rs"
# Pin authoring's resolution semantics as well as the concrete runtime policy.
AI_SOURCES = (AUTHORING_AI_SOURCE, RUNTIME_AI_SOURCE)
INPUTS = (
    PRODUCTION + "scenario.toml", PRODUCTION + "rules.toml",
    PRODUCTION + "catalog/creatures.toml", PRODUCTION + "catalog/actors.toml",
    PRODUCTION + "catalog/items.toml", DEFAULT_RULES,
    "scripts/rogue_roster_study.py", "scripts/rogue_roster_package.py",
    "scripts/arena_matrix.py", "scripts/arena_recipes.json",
    "scripts/arena_evaluation.py", "scripts/ruleset_sources.py", *AI_SOURCES,
)


def encounter_sources(snapshot=None):
    def read(name):
        relative = PRODUCTION + name
        raw = (ROOT / relative).read_bytes() if snapshot is None else snapshot[relative]
        return tomllib.loads(raw.decode("utf-8"))
    return {"manifest": read("scenario.toml"), "actors": read("catalog/actors.toml")["actors"],
            "species": read("catalog/creatures.toml")["creatures"]["species"],
            "items": read("catalog/items.toml")["items"]}


def source_cases(source, reference="arena-diagnostic"):
    if reference not in REFERENCES:
        raise ValueError("Unknown Rogue reference mode")
    species = SPECIES if reference == "arena-diagnostic" else PLAYABLE_SPECIES
    levels = REQUIRED_HD_LEVELS if reference == "arena-diagnostic" else (1,)
    return [{"id": f"{name}-hd{hd}", "species": "rogue." + name, "reference_hd": hd,
             "foe_hd": len(source["actors"]["rogue." + name]["creature"]["hit_dice"])}
            for name in species for hd in levels]


def study_cases(reference="arena-diagnostic"):
    return source_cases(encounter_sources(), reference)


def playable_character(source, arm, reference):
    """Copy every starter operand; the only build change is an explicit purchase."""
    if reference not in REFERENCES[1:] or arm not in ARMS:
        raise ValueError("Unknown playable reference or class arm")
    selected = 1 if arm == "baseline" else 2
    character = copy.deepcopy(next(row for row in source["manifest"]["characters"]
                                   if row["id"] == selected))
    build = character["creature"]
    class_id = "warrior" if arm == "baseline" else "mage"
    if build["hit_dice"] != [{"source": class_id, "purchases": []}]:
        raise ValueError("Playable reference requires the actual unspent one-HD start")
    if reference == "playable-first-purchase":
        choice = ({"kind": "talent", "talent": "rattling_blow"} if arm == "baseline" else
                  {"kind": "talent_form", "talent": "evocation_initiate", "form": "spark_lance_cold"})
        build["hit_dice"][0]["purchases"] = [{"pool": "advancement", "selection": choice}]
    return character


def reference_builds(source, reference):
    if reference == "arena-diagnostic":
        return {}
    spent = int(reference == "playable-first-purchase")
    return {arm: {"character": character["id"], "creature": character["creature"],
                  "species": copy.deepcopy(source["species"][character["creature"]["species"]]),
                  "body": character["body"], "turn_ticks": character["turn_ticks"],
                  "loadout": character["loadout"], "known_identities": character["known_identities"],
                  "item_definitions": {row["archetype"]: copy.deepcopy(source["items"][row["archetype"]])
                                       for row in character["loadout"]},
                  "ai_profile": copy.deepcopy(PLAYABLE_AI),
                  "points": {"earned": 1, "spent": spent, "unspent": 1 - spent},
                  "growth_evidence": "authored_hd_one_not_earned_route"}
            for arm in ARMS for character in [playable_character(source, arm, reference)]}


def run_manifest(seeds=DEFAULT_SEEDS, timeout=30, case_ids=None, *, reference="arena-diagnostic"):
    values = [seed(value) for value in seeds]
    if not 1 <= len(values) <= 1000 or len(values) != len(set(values)):
        raise ValueError("Supply 1..1000 distinct seeds")
    timeout = timeout_seconds(timeout)
    source = encounter_sources()
    available = source_cases(source, reference)
    requested = [case["id"] for case in available] if case_ids is None else list(case_ids)
    known = {case["id"] for case in available}
    if not requested or len(requested) != len(set(requested)) or not set(requested) <= known:
        raise ValueError("Unknown, duplicate or empty roster case selection")
    selected = [case for case in available if case["id"] in requested]
    runs = len(selected) * len(values) * 4
    playable = reference != "arena-diagnostic"
    return {
        "format": FORMAT, "seeds": values, "timeout_seconds": timeout, "reference": reference,
        "implementation": "new_reconstruction_with_retained_playable_contract",
        "faction": "delvers" if playable else "blue",
        "arms": {"baseline": "warrior", "candidate": "mage"} if playable else dict(ARMS),
        "arm_interpretation": "separate_class_observations",
        "reference_builds": reference_builds(source, reference),
        "required_cases": len(available),
        "required_reference_hd": [1] if playable else list(REQUIRED_HD_LEVELS),
        "complete_design": len(selected) == len(available),
        "missing_cases": [case["id"] for case in available if case["id"] not in requested],
        "planned_seed_count": len(values), "default_seed_count": len(DEFAULT_SEEDS),
        "pilot": True, "battle_evidence": "not_run", "calibration": "not_assessed",
        "production_activation": "not_selected", "package_count": len(selected) * 4,
        "expected_runs": runs, "maximum_run_timeout_seconds": runs * timeout,
        "maximum_replay_timeout_seconds": runs * timeout,
        "caps": {"ticks": 20000, "actions": 2000},
        "sampling_unit": "seed within species and reference mode; class arms are separate observations",
        "nonindependent": ["forward and mirrored orientations", "class arms share seeds"],
        "controller_policy": ({"profile": PLAYABLE_AI_NAME, "values": copy.deepcopy(PLAYABLE_AI),
            "provenance": "tor_simulation::ai::AiProfile::general()",
            "source_files": [RUNTIME_AI_SOURCE],
            "adaptation": "explicit study AI control; production starters are externally controlled",
            "foe_profile": "rogue_hostile", "human_route_evidence": False,
            "opening_action": "none", "species_disposition": "unchanged"} if playable else
            {"profile": "sparring", "flee_percent": 0, "foe_profile": "rogue_hostile",
             "human_route_evidence": False}),
        "limitations": [
            *(["reference PC natural weapons are not the selected-character starter kit",
               "no-flee duels do not establish escape or food pressure"] if not playable else
              ["general-profile autonomous decisions are a proxy, not ordinary human route choices",
               "AI does not inspect hidden immunity; a cold-immune stalemate is not a human escape result",
               "one authored class HD is not an earned depth-appropriate build",
               "authored first purchases require separate ordinary purchase acceptance"]),
            "reference HD is not dungeon depth",
            "isolated authored foes have no population birth stock; candidate stock requires entry-process evidence",
            "birth stock is absent from isolated duels; wake cadence and finite memory are TOR adaptations",
            "unchanged passive player and passive foe may never acquire each other; no contact is not combat survival",
            "net resources are not gross spending or recovery",
            "earned progression and coupled explore/fight/rest nutrition and stock flows remain unmeasured"],
        "cases": [dict(case, plan=case["id"] + "/plan.json") for case in selected],
    }


def study_encounter(case, arm, orientation, source, *, reference="arena-diagnostic"):
    if case not in source_cases(source, reference) or arm not in ARMS or orientation not in ORIENTATIONS:
        raise ValueError("Unknown roster case, class arm or orientation")
    manifest, region = encounter(ARMS[arm], case["reference_hd"], True,
                                 orientation == "mirrored", "rules.toml")
    if reference != "arena-diagnostic":
        character = playable_character(source, arm, reference)
        character.update(anchor="1/start", unselected="ai", ai=PLAYABLE_AI_NAME)
        profiles = copy.deepcopy(source["manifest"]["ai_profiles"])
        if PLAYABLE_AI_NAME in profiles:
            raise ValueError("Study AI profile collides with production content")
        profiles[PLAYABLE_AI_NAME] = copy.deepcopy(PLAYABLE_AI)
        manifest = {"format": source["manifest"]["format"],
                    "id": "rogue-roster-" + reference + "-" + case["id"], "version": "1.0",
                    "ruleset": "rules.toml", "modules": copy.deepcopy(source["manifest"]["modules"]),
                    "default_character": character["id"], "characters": [character],
                    "factions": copy.deepcopy(source["manifest"]["factions"]), "ai_profiles": profiles,
                    "arena": {"participants": [character["id"], 3], "control": "all_ai",
                              "ticks": 20000, "actions": 2000}}
        region["name"] = "Actual Rogue starter kit reference encounter"
        region["actors"][0].update(id=3, archetype=case["species"], ai="rogue_hostile")
    else:
        manifest["id"] = "rogue-roster-arena-diagnostic-" + case["id"]
        manifest["modules"] = ["catalog/creatures.toml", "catalog/actors.toml"]
        manifest["creatures"]["species"].pop("opponent")
        del manifest["archetypes"]
        foe_faction = source["actors"][case["species"]]["creature"]["faction"]
        manifest["factions"] = {"blue": [foe_faction], foe_faction: ["blue"]}
        manifest["ai_profiles"].update(copy.deepcopy(source["manifest"]["ai_profiles"]))
        manifest["arena"].update(ticks=20000, actions=2000)
        region["name"] = "Diagnostic reference against fixed Rogue content"
        region["actors"][0].update(archetype=case["species"], ai="rogue_hostile")
    # These copied recipes retain their source's declared presentation forecast.
    manifest["themes"] = copy.deepcopy(source["manifest"]["themes"])
    manifest["assets"] = copy.deepcopy(source["manifest"]["assets"])
    return manifest, region


def encounter_roster(manifest, region, source):
    """Keep actual per-arm actor identities, never renumber to satisfy pairing."""
    characters = {row["id"]: row["creature"]["faction"] for row in manifest["characters"]}
    actors = {row["id"]: source["actors"][row["archetype"]]["creature"]["faction"]
              for row in region["actors"]}
    return [{"actor": str(actor), "faction": {**characters, **actors}[actor]}
            for actor in manifest["arena"]["participants"]]


def create_study(output, compiler, seeds=DEFAULT_SEEDS, timeout=30, case_ids=None, *, reference="arena-diagnostic"):
    inventory = run_manifest(seeds, timeout, case_ids, reference=reference)
    snapshot = {relative: (ROOT / relative).read_bytes() for relative in INPUTS}
    source = encounter_sources(snapshot)
    if inventory["reference_builds"] != reference_builds(source, reference):
        raise ValueError("Playable reference changed during source snapshot")
    compiler = compiler.resolve(strict=True)
    compiler_hash = digest(compiler)
    hashes = {relative: hashlib.sha256(raw).hexdigest() for relative, raw in snapshot.items()}
    inventory.update(compiler_sha256=compiler_hash, source_inputs=hashes, validated_packages=0,
                     generation_status="resolving_rules")
    output.mkdir(parents=True, exist_ok=False)
    # Interrupted/rejected preparation keeps its inventory and exact source bytes.
    write_json(output / "partial.json", inventory)
    try:
        for relative, raw in snapshot.items():
            retained = output / "sources" / relative
            retained.parent.mkdir(parents=True, exist_ok=True)
            retained.write_bytes(raw)
        raw_rules = snapshot[PRODUCTION + "rules.toml"].decode("utf-8")
        rules, dependencies = resolve_with_dependencies(raw_rules, compiler)
        imported = tomllib.loads(raw_rules)["import"]
        bundled = hashlib.sha256(snapshot[DEFAULT_RULES].replace(b"\r\n", b"\n")).hexdigest()
        if dependencies != {imported: bundled}:
            raise ValueError("Compiler bundled rules differ from the pinned production source")
        inventory.update(rules_dependencies=dependencies, ruleset=rules["id"], generation_status="preparing")
        write_json(output / "partial.json", inventory)
        for case in inventory["cases"]:
            identity = {key: value for key, value in case.items() if key != "plan"}
            plan = {"seeds": inventory["seeds"], "faction": inventory["faction"],
                    "timeout_seconds": inventory["timeout_seconds"], "participant_rosters": {}}
            package_inputs = case["package_inputs"] = {}
            for arm, orientation in itertools.product(ARMS, ORIENTATIONS):
                name = arm + "-" + orientation
                inventory["pending_package"] = case["id"] + "/" + name
                write_json(output / "partial.json", inventory)
                package = output / case["id"] / name
                (package / "regions").mkdir(parents=True)
                manifest, region = study_encounter(identity, arm, orientation, source, reference=reference)
                for module in manifest["modules"]:
                    path = package / module
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(snapshot[PRODUCTION + module])
                (package / "rules.toml").write_text(toml_source(rules), encoding="utf-8", newline="\n")
                for path, document in (("scenario.toml", manifest), ("regions/1.toml", region)):
                    (package / path).write_text(toml_source(document), encoding="utf-8", newline="\n")
                result = subprocess.run([str(compiler), "validate", str(package)], capture_output=True,
                                        text=True, encoding="utf-8", timeout=30)
                if result.returncode:
                    raise ValueError("Invalid roster package " + inventory["pending_package"] + ": "
                                     + (result.stdout + result.stderr)[-2000:])
                roster = encounter_roster(manifest, region, source)
                if plan["participant_rosters"].setdefault(arm, roster) != roster:
                    raise ValueError("Mirrored participant identities differ")
                plan.setdefault(arm, {})[orientation] = name
                package_inputs[name] = files(package)
                inventory["validated_packages"] += 1
                write_json(output / "partial.json", inventory)
            participant_contract(plan)
            write_json(output / case["plan"], plan)
        if digest(compiler) != compiler_hash or hashes != {name: digest(ROOT / name) for name in INPUTS}:
            raise ValueError("Roster study source or compiler changed during generation")
        inventory.pop("pending_package", None)
        inventory["generation_status"] = "validated"
        write_json(output / "study.json", inventory)
        (output / "partial.json").unlink()
    except (ValueError, OSError, KeyError, subprocess.TimeoutExpired) as error:
        inventory.update(generation_status="failed", failure={"type": type(error).__name__, "message": str(error)[-2000:]})
        write_json(output / "partial.json", inventory)
        raise
    return inventory


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seeds", nargs="+", default=DEFAULT_SEEDS)
    parser.add_argument("--timeout-seconds", type=float, default=30)
    parser.add_argument("--reference", choices=REFERENCES, default="arena-diagnostic")
    parser.add_argument("--case", dest="case_ids", action="append", help="Exact case ID; repeat for a partial pilot")
    args = parser.parse_args(argv)
    try:
        inventory = create_study(args.output.resolve(), args.compiler, args.seeds,
                                 args.timeout_seconds, args.case_ids, reference=args.reference)
        print(f"Validated {len(inventory['cases'])}/{inventory['required_cases']} roster cases; "
              f"{inventory['expected_runs']} planned runs, no battles run")
        return 0
    except (ValueError, OSError, KeyError, subprocess.TimeoutExpired) as error:
        print(f"Rogue roster study: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
