"""Production Drain Life in an explicit, empty-room ordinary actor fixture."""
import subprocess
import tomllib
from arena_matrix import toml_source
from nothing_catalog_package import prepare as prepare_nothing, PRODUCTION


def prepare(test, *, weak=False, target=False):
    package = prepare_nothing(test)
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest["id"] = "imported-drain-life"
    manifest["spatial_scopes"] = {"study": {
        "members": [1], "rooms": {"1": [{"region": 1, "min": [0, 0, 0], "max": [6, 4, 1]}]},
        "passages": [], "doorways": [],
    }}
    # Replace the fixture's additive Health source, not a second conflicting Set.
    manifest["creatures"]["species"]["figure"]["modifiers"] = [
        {"scope": "always", "op": {"kind": "set", "stat": {"kind": "health"}, "value": 1 if weak else 21}}]
    path.write_text(toml_source(manifest), encoding="utf-8")
    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    test.assertEqual(region["size"], [7, 5, 2])
    test.assertEqual(region["anchors"]["start"], [1, 2, 0])
    test.assertTrue(region["chamber"])
    test.assertFalse(region.get("actors", []))
    if target:
        test.assertFalse(weak)
        # Real external peer: existing bounded driver advances it, never AI attacks.
        creature = dict(manifest["characters"][0]["creature"])
        creature["name"] = "drain recipient"
        region["actors"] = [{"id": 2, "at": [3, 2, 0], "turn_ticks": 100,
                             "controller": "external", "creature": creature}]
    for row in region["items"]:
        if row["id"] in (30, 31):
            row["archetype"] = "rogue.wand_drain_life"
    path.write_text(toml_source(region), encoding="utf-8")
    for relative in ("catalog/items.toml", "rules.toml"):
        test.assertEqual((package / relative).read_bytes(), (PRODUCTION / relative).read_bytes())
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, timeout=30)
    test.assertEqual(result.returncode, 0, result.stdout + result.stderr)
    return package
