"""Ordinary authored one-cell invocation fixture for real applications."""
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import creature_package


def one_cell_package(test, name="one-cell", *, push=False, blocked=False, primary_only=False):
    identity = "push_probe" if push else "step_probe"
    package = creature_package(test, name, abilities=(identity,),
        ability_aliases={identity: ("basic_melee", "Push Probe" if push else "Step Probe")})
    path = package / "rules.toml"
    rules = tomllib.loads(path.read_text(encoding="utf-8"))
    definition = next(row for row in rules["abilities"] if row["id"] == identity)
    definition.update(tags=[], invocation="actor_target" if push else "self_step",
        cost={"resource": "stamina", "start": 1, "resolution": 2 if push else 1},
        reach={"kind": "melee" if push else "self_only"})
    definition["timing"] = {"source": (
        {"kind": "selected_weapon_adjusted", "preparation": 50, "recovery": 0} if push else
        {"kind": "fixed", "prepare": 100, "recover": 100}), "scaled_by": "speed"}
    definition["ai"]["hostile"] = push
    strike = {"offset": 0, "on_failure": [], "descriptors": [], "riders": []}
    if push:
        strike["on_success"] = [{"kind": "damage", "damage": {"kind": "selected_weapon", "impact_bonus": 0}}]
        strike["riders"] = [{"trigger": "success", "descriptors": [], "effects": [
            {"kind": "push_target_one_cell", "direction": "away_from_source"}]}]
    else:
        strike["on_success"] = [{"kind": "step_self_one_cell"},
            {"kind": "apply_condition", "condition": "braced", "duration": 200}]
    if primary_only:
        strike["riders"] = []
    definition["strikes"] = {"kind": "authored", "strikes": [strike]}
    path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    human = manifest["creatures"]["species"]["human"]
    human["attributes"]["speed"] = 0
    human["melee"]["strikes"][0]["on_success"] = [{"kind": "damage", "damage": {"kind": "components", "components": [
        {"category": "impact", "amount": {"kind": "fixed", "amount": 4}}]}}]
    manifest["creatures"]["species"]["human"]["displacement"] = {"size": "medium", "anchored": False}
    path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    if blocked:
        region["walls"].extend([[3, 1, 0], [3, 1, 1]])
    region["actors"][0]["controller"] = "external"
    region["actors"][0].pop("ai", None)
    region["actors"][0]["turn_ticks"] = 1000
    path.write_text(toml_source(region), encoding="utf-8", newline="\n")
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
        capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stderr)
    return package
