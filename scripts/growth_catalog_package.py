"""Exact opt-in growth catalog with lawful ordinary allocation and blocked growth."""
import subprocess
import tomllib
from arena_matrix import toml_source
from nothing_catalog_package import prepare as prepare_nothing, PRODUCTION
from ruleset_sources import load_rules


def prepare(test, kind, *, diagnostic_abilities=False):
    test.assertIn(kind, ("gain_strength", "raise_level"))
    package = prepare_nothing(test)
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest["id"] = "imported-" + kind.replace("_", "-")
    owner = manifest["characters"][0]
    owner["creature"]["attributes"] = {"strength": 5, "speed": 0, "intellect": 2,
                                        "willpower": 2, "awareness": 1, "presence": 1}
    owner["creature"]["future_growth"] = {"kind": "disabled"}
    test.assertEqual(owner["turn_ticks"], 100)
    test.assertEqual(owner["creature"]["hit_dice"], [{"source": "racial", "purchases": []}])
    test.assertEqual(manifest["creatures"]["species"]["figure"]["attributes"]["strength"], 0)
    path.write_text(toml_source(manifest), encoding="utf-8")
    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    test.assertFalse(region.get("actors", []))
    region["items"] = [{"id": 30, "at": [1, 2, 0], "archetype": "rogue." + kind,
                        "quantity": 3 if kind == "gain_strength" else 2}]
    path.write_text(toml_source(region), encoding="utf-8")
    for relative in ("catalog/items.toml", "rules.toml"):
        test.assertEqual((package / relative).read_bytes(), (PRODUCTION / relative).read_bytes())
    if diagnostic_abilities:
        rules_path = package / "rules.toml"
        rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
        existing = list(rules["overrides"])
        for effect in ("gain_strength", "raise_level"):
            ability = "fixture." + effect
            rules["overrides"].append({"op": "add", "path": ["abilities", ability], "value": {
                "id": ability, "name": "Growth diagnostic " + effect, "interaction": "non_hostile",
                "tags": [], "basic_attack": False, "invocation": "actor_target", "reach": {"kind": "self_only"},
                "timing": {"source": {"kind": "fixed", "prepare": 100, "recover": 100}},
                "strikes": {"kind": "authored", "strikes": [{"offset": 0, "on_success": [{"kind": effect}],
                    "on_failure": [], "descriptors": [], "riders": []}]},
                "ai": {"priority": 0, "hostile": False, "minimum_resource_percent": 0, "range_preference": "any"}}})
            manifest["creatures"]["species"]["figure"]["modifiers"].append({
                "scope": "always", "op": {"kind": "grant_ability", "ability": ability}})
        test.assertEqual(rules["overrides"][:-2], existing)
        rules_path.write_text(toml_source(rules), encoding="utf-8")
        (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8")
    compiler = test.bin / ("tor-scenario" + test.suffix)
    checked = subprocess.run([compiler, "validate", package], capture_output=True, text=True, timeout=30)
    test.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
    rules = load_rules(package / "rules.toml", compiler)
    test.assertEqual(rules["constants"]["item_timing"]["consume_turns"], 1)
    return package, rules["growth"]
