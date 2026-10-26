"""Copied generic self-device process fixture; not shipped catalog gameplay."""
import subprocess
import tomllib
from arena_matrix import toml_source
from ruleset_sources import load_rules


def prepare(test, *, peer=False, condition=False, discovery="never"):
    package = test.package(peer=peer)
    if peer:
        region_path = package / "regions/1.toml"
        region = tomllib.loads(region_path.read_text(encoding="utf-8"))
        assert region["actors"][0]["id"] == 2
        region["actors"][0]["at"] = [2, 2, 0]
        region_path.write_text(toml_source(region), encoding="utf-8")
    manifest_path = package / "scenario.toml"
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    test.assertEqual(manifest["creatures"]["species"]["figure"]["attributes"]["speed"], 0)
    device = manifest["archetypes"]["ember"]
    device["name"] = "wand of self condition" if condition else "wand of nothing"
    device["identity"] = "fixture.self_condition" if condition else "fixture.nothing"
    device["device"]["discovery"] = discovery
    manifest_path.write_text(toml_source(manifest), encoding="utf-8")
    rules_path = package / "rules.toml"
    rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
    ability = rules["overrides"][0]["value"]
    assert ability["id"] == "magic.ember_ray"
    ability.update(interaction="non_hostile", tags=[], invocation="actor_target", reach={"kind": "self_only"})
    ability["ai"]["hostile"] = False
    assert len(ability["strikes"]["strikes"]) == 1
    effects = [{"kind": "apply_condition", "condition": "fixture.self_device", "duration": 2000}] if condition else []
    ability["strikes"]["strikes"] = [{"offset": 0, "on_success": effects, "on_failure": [], "descriptors": [], "riders": []}]
    if condition:
        rules["overrides"].append({"op": "add", "path": ["conditions", "fixture.self_device"], "value": {
            "id": "fixture.self_device", "family": "fixture.self_device", "lifecycle": "timed",
            "name": "Self device", "modifiers": [], "stacking": {"kind": "keep"},
            "source_scoped": False, "descriptors": [], "capacity": "reject",
            "timing": {"kind": "actor_active", "on_expiry": {"kind": "remove"}},
            "restrictions": [], "preparation": "preserve"}})
    rules_path.write_text(toml_source(rules), encoding="utf-8")
    checked = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package], capture_output=True, text=True, timeout=30)
    test.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
    resolved = load_rules(rules_path, test.bin / ("tor-scenario" + test.suffix))
    test.assertEqual(resolved["constants"]["item_timing"]["consume_turns"], 1)
    return package
