"""Ordinary validated W05 process fixture, ready for combined phase6 integration."""
import copy
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import ROOT, resource
from ruleset_sources import load_rules


def paid_preparation(test, player, initial, subject, subject_initial):
    selected = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                    if actor["name"] == "subject")
    test.assertIsNone(test.act(player, {"type": "use_ability", "id": "read_the_rhythm",
                                      "target": selected})["error"])
    target = next(actor["id"] for actor in subject_initial["state"]["observation"]["visible_actors"]
                  if actor["name"] == "observer")
    test.assertIsNone(test.act(subject, {"type": "use_ability", "id": "slow_plan",
                                       "target": target})["error"])
    observed = test.frame(player, lambda frame: frame.get("state") is not None
                         and frame["state"]["observation"]["combat"]["public_plan"] is not None)
    record = observed["state"]["observation"]["combat"]["public_plan"]
    test.assertEqual(record["observed_at"], "100")
    test.assertEqual(record["state"]["remaining_work"], "200")
    test.assertEqual(resource(observed, "focus")["balance"], resource(initial, "focus")["balance"] - 2)
    test.assertEqual(resource(observed, "focus")["reserved"], 0)
    return observed


def public_plan_package(test, *, known=False):
    package = test.directory / ("public-plan-known" if known else "public-plan-unknown")
    shutil.copytree(ROOT / "scenarios/two-room", package)
    compiler = test.bin / ("tor-scenario" + test.suffix)
    rules = load_rules(package / "rules.toml", compiler)
    if not any(tag["id"] == "attack" for tag in rules["ability_tags"]):
        rules["ability_tags"].append({"id": "attack", "name": "Attack"})
    for ability in rules["abilities"]:
        if ability["id"] == "basic_melee" and "attack" not in ability["tags"]:
            ability["tags"].append("attack")

    def strike(effects):
        return {"offset": 0, "on_success": effects, "on_failure": [],
                "descriptors": [], "riders": []}

    observe = {"id": "read_the_rhythm", "name": "Read the Rhythm", "tags": [],
               "basic_attack": False,
               "cost": {"resource": "focus", "start": 1, "resolution": 1},
               "timing": {"source": {"kind": "fixed", "prepare": 100, "recover": 100},
                          "scaled_by": "speed"},
               "reach": {"kind": "sight", "range": 6},
               "strikes": {"kind": "authored", "strikes": [strike([
                   {"kind": "observe_public_plan"},
                   {"kind": "apply_qualified_condition_to_source", "condition": "readied",
                    "duration": 300, "ability_tag": "attack"}])]},
               "ai": {"priority": 40, "hostile": True, "minimum_resource_percent": 100,
                      "range_preference": "any"}}
    slow = copy.deepcopy(observe)
    slow.update(id="slow_plan", name="Slow Plan")
    slow.pop("cost")
    slow["timing"]["source"]["prepare"] = 300
    slow["strikes"]["strikes"] = [strike([{"kind": "heal", "amount": {"kind": "fixed", "amount": 0}}])]
    # This standalone capability fixture supplies the proposed authored operands.
    # Grants isolate transport from the separately integrated talent prerequisites.
    rules["abilities"] = [ability for ability in rules["abilities"]
                          if ability["id"] not in ("read_the_rhythm", "slow_plan")]
    rules["abilities"].extend([observe, slow])
    (package / "rules.toml").write_text(toml_source(rules), encoding="utf-8", newline="\n")
    manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
    manifest["id"] = package.name
    species = {"kind": "humanoid", "attributes": dict.fromkeys(
        ("strength", "speed", "intellect", "willpower", "awareness", "presence"), 0),
        "melee": {"skill": "heavy_weaponry", "bonus": 0, "wind_up": 100, "recovery": 100,
                  "strikes": [strike([{"kind": "damage", "damage": {"kind": "components",
                      "components": [{"category": "impact", "amount": {"kind": "fixed", "amount": 1}}]}}])]},
        "modifiers": [{"scope": "always", "op": {"kind": "grant_ability", "ability": "read_the_rhythm"}},
                      {"scope": "always", "op": {"kind": "set",
                          "stat": {"kind": "resource_max", "resource": "focus"}, "value": 10}}]}
    manifest["creatures"] = {"species": {"reader": species}, "templates": {
        "known_plan": {"priority": 0, "modifiers": [
            {"scope": "always", "op": {"kind": "grant_ability", "ability": "slow_plan"}}]}}}

    def creature(name, knows):
        return {"species": "reader", "name": name, "faction": "neutral", "binding": "intellect",
                "starting_skills": ["heavy_weaponry", "athletics"], "restoration": "authored",
                "hit_dice": [{"source": "racial"}], "templates": ["known_plan"] if knows else []}

    manifest["characters"][0]["creature"] = creature("observer", known)
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    region["items"] = []
    region["actors"] = [{"id": 2, "at": [2, 1, 0], "turn_ticks": 100,
                         "controller": "external", "creature": creature("subject", True)}]
    path.write_text(toml_source(region), encoding="utf-8", newline="\n")
    validated = subprocess.run([compiler, "validate", package], capture_output=True,
                               text=True, encoding="utf-8", timeout=30)
    test.assertEqual(validated.returncode, 0, validated.stderr)
    return package
