"""Small owned loss/restoration operands for acceptance, never production loot."""
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import ROOT
from ruleset_sources import load_rules


LOSS_STRENGTH = "fixture_lose_strength"
LOSS_HEALTH = "fixture_lose_health"
WOUND = "fixture_wound"
DRAIN_HD = "fixture_drain_hit_die"
ZERO_WORD_SEED = 0x61C8864680B583EB


def prepare(test, *, peer=False, consume_turns=1, hit_dice=False):
    package = test.directory / "restorable-loss-fixture"
    shutil.copytree(ROOT / "scenarios/two-room", package)
    compiler = test.bin / ("tor-scenario" + test.suffix)
    rules = load_rules(package / "rules.toml", compiler)
    rules["constants"]["item_timing"]["consume_turns"] = consume_turns
    rules["health_recovery"] = {"base_period": 1000000, "minimum_period": 1000000,
                                "reduction_per_hd": 0}

    def strike(effects):
        return {"offset": 0, "on_success": effects, "on_failure": [],
                "descriptors": [], "riders": []}

    effects = {
        LOSS_STRENGTH: {"kind": "lose_attribute", "attribute": "strength",
                        "amount": {"kind": "fixed", "amount": 1}},
        LOSS_HEALTH: {"kind": "lose_maximum_health",
                      "amount": {"kind": "rolled", "dice": {"count": 1, "sides": 3, "bonus": 0}}},
        WOUND: {"kind": "damage", "damage": {"kind": "components", "components": [
            {"category": "vital", "amount": {"kind": "fixed", "amount": 2}}]}},
    }
    if hit_dice:
        effects[DRAIN_HD] = {"kind": "drain_latest_hit_die"}
    for identity, effect in effects.items():
        rules["abilities"].append({
            "interaction": "hostile_to_target",
            "id": identity, "name": identity.replace("_", " "), "tags": [], "basic_attack": False,
            "timing": {"source": {"kind": "fixed", "prepare": 1, "recover": 1}},
            "reach": {"kind": "self_only"},
            "strikes": {"kind": "authored", "strikes": [strike([effect])]},
            "ai": {"priority": 0, "hostile": False, "minimum_resource_percent": 0,
                   "range_preference": "any"},
        })
    rules["talents"].append({
        "id": "fixture_loss_operations", "name": "Fixture loss operations", "prerequisites": [],
        "requirement": {"kind": "total_hd", "minimum": 1},
        "modifiers": [{"scope": "always", "op": {"kind": "grant_ability", "ability": identity}}
                      for identity in effects],
    })
    (package / "rules.toml").write_text(toml_source(rules), encoding="utf-8", newline="\n")
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest["id"] = "restorable-loss-fixture"
    manifest["factions"] = {"subjects": []}
    manifest["archetypes"] = {"restoration": {
        "name": "fixture restoration", "identity": "fixture_restoration", "class": "potion",
        "stackable": True, "consumable": {"effects": [
            {"kind": "restore_attribute", "attribute": "strength", "amount": {"kind": "fixed", "amount": 1}},
            {"kind": "restore_maximum_health", "amount": {"kind": "fixed", "amount": 100}},
        ]}}}
    if hit_dice:
        manifest["archetypes"]["restoration"]["consumable"]["effects"] = [{"kind": "restore_hit_die"}]
    manifest["creatures"] = {"species": {"subject": {
        "kind": "humanoid",
        "behavior": {"disposition": "passive", "idle": "wait", "memory_ticks": 1000},
        "attributes": {"strength": 3, "speed": 1, "intellect": 1,
                                               "willpower": 1, "awareness": 1, "presence": 1},
        "melee": {"skill": "heavy_weaponry", "bonus": 0, "wind_up": 100, "recovery": 100,
                  "strikes": [strike([{"kind": "heal", "amount": {"kind": "fixed", "amount": 0}}])]},
        "modifiers": [{"scope": "always", "op": {"kind": "add", "stat": {"kind": "health"}, "value": 20}}],
    }}}

    def creature(name):
        value = {"species": "subject", "name": name, "faction": "subjects", "binding": "intellect",
                "starting_skills": ["heavy_weaponry", "athletics"], "restoration": "authored",
                "nutrition": {"kind": "exempt"}, "hit_dice": [{"source": "racial", "purchases": [
                    {"pool": "advancement", "selection": {"kind": "talent", "talent": "fixture_loss_operations"}},
                ]}]}
        if hit_dice:
            value["hit_dice"].extend([{"source": "racial"}, {"source": "racial", "purchases": [
                {"pool": "advancement", "selection": {"kind": "skill_rank", "skill": "athletics"}},
            ]}])
        return value

    manifest["characters"][0].update(creature=creature("loss subject"), turn_ticks=100)
    path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    region["actors"] = ([{"id": 2, "at": [2, 1, 0], "turn_ticks": 100,
                         "controller": "external", "creature": creature("loss peer")}] if peer else [])
    region["items"] = [{"id": 10, "at": [1, 1, 0], "carried_by": 1,
                        "archetype": "restoration", "quantity": 2}]
    path.write_text(toml_source(region), encoding="utf-8", newline="\n")
    path = package / "regions/2.toml"
    gallery = tomllib.loads(path.read_text(encoding="utf-8"))
    gallery["items"] = []
    path.write_text(toml_source(gallery), encoding="utf-8", newline="\n")
    validated = subprocess.run([compiler, "validate", package], capture_output=True,
                               text=True, encoding="utf-8", timeout=30)
    test.assertEqual(validated.returncode, 0, validated.stderr)
    return package
