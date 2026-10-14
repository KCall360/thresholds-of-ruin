"""Explicit tiny nutrition content for mechanics acceptance, never Rogue tuning."""
import copy
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import ROOT
from ruleset_sources import load_rules


ROLLED_MEAL = {"kind": "rolled", "dice": {"count": 1, "sides": 400, "bonus": 1099}}
ZERO_WORD_SEED = 0x61C8864680B583EB


def observed_meal_complete(frame, item, initial_reserve):
    """Observe a last-unit meal's effects without requiring its owner's lease."""
    view = (frame.get("state") or {}).get("observation")
    if view is None:
        return False
    combat = view["combat"]
    return (view["ready"] and not combat["terminal"]
            and combat["preparation_remaining"] is None
            and combat["own_stats"]["nutrition"]["reserve"] > initial_reserve
            and not any(row["id"] == item for row in view["inventory"]))


def prepare(test, *, initial=8, maximum=12, unit_ticks=100, consume_turns=1,
            meal_amount=None, spare_meal=False, peer=False, concealed=False,
            recovery_policies=None, name="nutrition-fixture"):
    package = test.directory / name
    shutil.copytree(ROOT / "scenarios/two-room", package)
    compiler = test.bin / ("tor-scenario" + test.suffix)
    rules = load_rules(package / "rules.toml", compiler)
    rules["nutrition"] = {"kind": "enabled", "initial": initial, "maximum": maximum,
                          "hungry_below": 7, "weak_below": 3, "starve_at": -3,
                          "unit_ticks": unit_ticks,
                          "hungry_recovery": {"kind": "slowed", "multiplier": 2},
                          "weak_recovery": {"kind": "stopped"},
                          "regeneration_cost": {"denominator": 1, "health": 0, "stamina": 0, "focus": 0, "mana": 0},
                          "empty_recovery": {"kind": "stopped"}}
    if recovery_policies is not None:
        rules["nutrition"].update(copy.deepcopy(recovery_policies))
    rules["constants"]["item_timing"]["consume_turns"] = consume_turns
    (package / "rules.toml").write_text(toml_source(rules), encoding="utf-8", newline="\n")
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest["id"] = name
    manifest["factions"] = {"subjects": []}
    manifest["archetypes"]["meal"] = {"name": "fixture meal", "identity": "meal", "class": "food",
        "stackable": True, "edible": {"nutrition": copy.deepcopy(
            meal_amount if meal_amount is not None else {"kind": "fixed", "amount": 6})}}
    if concealed:
        manifest["appearance_pools"] = {"meals": {"appearances": ["wrapped fixture meal"]}}
        manifest["archetypes"]["meal"]["appearance_pool"] = "meals"
    manifest["archetypes"]["stone_food"] = {"name": "stone food", "identity": "stone_food", "class": "food"}
    strike = {"offset": 0, "on_success": [{"kind": "damage", "damage": {
        "kind": "components", "components": [{"category": "impact", "amount": {"kind": "fixed", "amount": 1}}]}}],
        "on_failure": [], "descriptors": [], "riders": []}
    manifest["creatures"] = {"species": {"subject": {
        "kind": "humanoid", "attributes": dict.fromkeys(
            ("strength", "speed", "intellect", "willpower", "awareness", "presence"), 0),
        "melee": {"skill": "heavy_weaponry", "bonus": 0, "wind_up": 100,
                  "recovery": 100, "strikes": [copy.deepcopy(strike)]}}}}
    manifest["characters"][0]["creature"] = {"species": "subject", "name": "nutrition subject", "faction": "subjects",
        "binding": "intellect", "starting_skills": ["heavy_weaponry", "athletics"], "restoration": "authored",
        "nutrition": {"kind": "required"}, "hit_dice": [{"source": "racial"}]}
    manifest["characters"][0]["turn_ticks"] = 100
    path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    region["actors"] = []
    if peer:
        creature = copy.deepcopy(manifest["characters"][0]["creature"])
        creature["name"] = "nutrition peer"
        region["actors"].append({"id": 2, "at": [2, 1, 0], "turn_ticks": 1000,
                                 "controller": "external", "creature": creature})
    region["items"] = [{"id": 10, "at": [1, 1, 0], "carried_by": 1, "archetype": "meal", "quantity": 2},
                       {"id": 11, "at": [1, 1, 0], "carried_by": 1, "archetype": "stone_food"}]
    if spare_meal:
        region["items"].append({"id": 12, "at": [1, 1, 0], "carried_by": 1,
                                "archetype": "meal", "quantity": 3})
    path.write_text(toml_source(region), encoding="utf-8", newline="\n")
    result = subprocess.run([compiler, "validate", package], capture_output=True, text=True,
                            encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stderr)
    return package


def prepare_rolled(test, **options):
    """Two meals fit below cap; these deliberately artificial values are not balance policy."""
    return prepare(test, maximum=4000, unit_ticks=100000, meal_amount=ROLLED_MEAL, **options)


def prepare_regeneration(test):
    """Three ordinary one-point payments exercise shared fractional recovery debt."""
    package = prepare(test, initial=40, maximum=50, unit_ticks=100000,
                      name="nutrition-regeneration-fixture")
    compiler = test.bin / ("tor-scenario" + test.suffix)
    rules = load_rules(package / "rules.toml", compiler)
    rules["nutrition"]["regeneration_cost"] = {
        "denominator": 3, "health": 1, "stamina": 1, "focus": 1, "mana": 1}
    for resource in rules["resources"]:
        resource["recovery_period"] = 100
        resource["capacity"]["base"] = 10
    for resource in ("stamina", "focus", "mana"):
        identity = "fixture_spend_" + resource
        rules["abilities"].append({
            "id": identity, "name": "Spend " + resource, "tags": [], "basic_attack": False,
            "cost": {"resource": resource, "start": 1, "resolution": 0},
            "timing": {"source": {"kind": "fixed", "prepare": 1, "recover": 1},
                       "scaled_by": "speed"},
            "reach": {"kind": "self_only"},
            "strikes": {"kind": "authored", "strikes": [{
                "offset": 0, "on_success": [{"kind": "heal", "amount": {"kind": "fixed", "amount": 0}}],
                "on_failure": [], "descriptors": [], "riders": []}]},
            "ai": {"priority": 0, "hostile": False, "minimum_resource_percent": 0,
                   "range_preference": "any"}})
        rules["constants"]["default_abilities"].append(identity)
    (package / "rules.toml").write_text(toml_source(rules), encoding="utf-8", newline="\n")
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest["creatures"]["species"]["subject"]["modifiers"] = [
        {"scope": "always", "op": {"kind": "flag", "flag": "magical"}}]
    path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    result = subprocess.run([compiler, "validate", package], capture_output=True, text=True,
                            encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stderr)
    return package


def prepare_trigger(test, *, protection_ticks=None):
    """Ordinary automatic status content shared by acceptance and wire recording.

    ZERO_WORD_SEED makes the first depletion apply 1100 active ticks of fainting.
    An exempt peer provides 100-tick input boundaries; optional protection lets
    the recorder reach both personal phases in the same short ordinary game.
    """
    neutral = {field: {"kind": "normal"} for field in
               ("hungry_recovery", "weak_recovery", "empty_recovery")}
    package = prepare(test, initial=0, maximum=2000, peer=True, recovery_policies=neutral,
                      name="nutrition-trigger-fixture")
    compiler = test.bin / ("tor-scenario" + test.suffix)
    rules = load_rules(package / "rules.toml", compiler)
    rules["nutrition"]["starve_at"] = -852
    rules["nutrition_condition_trigger"] = {
        "kind": "on_empty_depletion", "condition": "fixture_stupor", "chance": {"one_in": 5},
        "duration": {"kind": "rolled", "dice": {"count": 1, "sides": 8, "bonus": 3}},
        "ticks_per_unit": 100}
    expiry = ({"kind": "remove"} if protection_ticks is None
              else {"kind": "protect", "duration": protection_ticks})
    rules["conditions"].append({
        "id": "fixture_stupor", "family": "fixture_stupor", "name": "Fixture Faint", "lifecycle": "timed",
        "timing": {"kind": "actor_active", "on_expiry": expiry},
        "restrictions": ["voluntary_actions"], "preparation": "pause",
        "modifiers": [], "descriptors": [], "stacking": {"kind": "keep"},
        "source_scoped": False, "capacity": "reject"})
    (package / "rules.toml").write_text(toml_source(rules), encoding="utf-8", newline="\n")
    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    region["actors"][0]["turn_ticks"] = 100
    region["actors"][0]["creature"]["nutrition"] = {"kind": "exempt"}
    path.write_text(toml_source(region), encoding="utf-8", newline="\n")
    result = subprocess.run([compiler, "validate", package], capture_output=True, text=True,
                            encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stderr)
    return package
