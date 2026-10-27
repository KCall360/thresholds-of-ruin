"""Opt-in production torch in a dark, single-owner ordinary fixture."""
import subprocess
import tomllib

from arena_matrix import toml_source
from nothing_catalog_package import prepare as prepare_nothing, PRODUCTION


def prepare(test):
    package = prepare_nothing(test)
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest["id"] = "imported-torch"
    anatomy = manifest["creatures"]["species"]["figure"]["anatomy"]
    test.assertEqual(anatomy["capacities"], {"grip": 2})
    test.assertEqual(anatomy["slots"], ["weapon", "body_armor", "ring", "ring", "weapon"])
    anatomy["slots"].append("held_light")
    path.write_text(toml_source(manifest), encoding="utf-8")
    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    test.assertEqual(region["size"], [7, 5, 2])
    test.assertEqual(region["anchors"]["start"], [1, 2, 0])
    test.assertFalse(region.get("actors", []))
    region["lit"] = False
    region["items"] = [{"id": 30, "at": [1, 2, 0], "archetype": "rogue.torch"}]
    path.write_text(toml_source(region), encoding="utf-8")
    for relative in ("catalog/items.toml", "rules.toml"):
        test.assertEqual((package / relative).read_bytes(), (PRODUCTION / relative).read_bytes())
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, timeout=30)
    test.assertEqual(result.returncode, 0, result.stdout + result.stderr)
    return package
