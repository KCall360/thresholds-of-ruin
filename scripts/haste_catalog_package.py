"""Exact production Haste potion in the qualified single-owner source fixture."""
import subprocess
import tomllib
from arena_matrix import toml_source
from nothing_catalog_package import prepare as prepare_nothing, PRODUCTION


def prepare(test, *, diagnostic_ability=False):
    package = prepare_nothing(test)
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest["id"] = "imported-haste-dose"
    test.assertEqual(manifest["creatures"]["species"]["figure"]["attributes"]["speed"], 0)
    test.assertEqual(manifest["characters"][0]["turn_ticks"], 100)
    path.write_text(toml_source(manifest), encoding="utf-8")
    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    test.assertFalse(region.get("actors", []))
    region["items"] = [{"id": 30, "at": [1, 2, 0], "archetype": "rogue.haste", "quantity": 2}]
    path.write_text(toml_source(region), encoding="utf-8")
    for relative in ("catalog/items.toml", "rules.toml"):
        test.assertEqual((package / relative).read_bytes(), (PRODUCTION / relative).read_bytes())
    if diagnostic_ability:
        # Distinct, actual self ability for supported privileged outcome capture.
        # The production potion and its condition definitions remain untouched.
        rules_path = package / "rules.toml"
        rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
        original_overrides = list(rules["overrides"])
        rules["overrides"].append({"op": "add", "path": ["abilities", "fixture.haste_dose"], "value": {
            "id": "fixture.haste_dose", "name": "Haste dose witness", "interaction": "non_hostile",
            "tags": [], "basic_attack": False, "invocation": "actor_target", "reach": {"kind": "self_only"},
            "timing": {"source": {"kind": "fixed", "prepare": 100, "recover": 0}},
            "strikes": {"kind": "authored", "strikes": [{"offset": 0,
                "on_success": [{"kind": "haste_dose", "condition": "rogue.hasted", "rest": "rogue.haste_exhaustion"}],
                "on_failure": [], "descriptors": [], "riders": []}]},
            "ai": {"priority": 0, "hostile": False, "minimum_resource_percent": 0, "range_preference": "any"}}})
        test.assertEqual(rules["overrides"][:-1], original_overrides)
        rules_path.write_text(toml_source(rules), encoding="utf-8")
        manifest["creatures"]["species"]["figure"].setdefault("modifiers", []).append({
            "scope": "always", "op": {"kind": "grant_ability", "ability": "fixture.haste_dose"}})
        (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8")
    checked = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                             capture_output=True, text=True, timeout=30)
    test.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
    return package
