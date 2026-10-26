"""Actual imported Room Light, explicit authored room or passage topology."""
import subprocess
import tomllib
from arena_matrix import toml_source
from nothing_catalog_package import prepare as prepare_nothing


def prepare(test, *, passage=False):
    package = prepare_nothing(test)
    manifest_path = package / "scenario.toml"
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    manifest["id"] = "imported-room-light"
    manifest["spatial_scopes"] = {"study": {
        "members": [1],
        "rooms": {} if passage else {"1": [{"region": 1, "min": [0, 0, 0], "max": [6, 4, 1]}]},
        "passages": [{"region": 1, "z": z, "y": y, "first_x": 0, "last_x": 6}
                     for z in range(2) for y in range(5)] if passage else [],
        "doorways": [],
    }}
    manifest_path.write_text(toml_source(manifest), encoding="utf-8")
    region_path = package / "regions/1.toml"
    region = tomllib.loads(region_path.read_text(encoding="utf-8"))
    test.assertEqual(region["size"], [7, 5, 2])
    test.assertTrue(region["chamber"])
    test.assertEqual(region["anchors"]["start"], [1, 2, 0])
    region["lit"] = False
    region["items"] = [{"id": 30, "at": [1, 2, 0], "archetype": "rogue.wand_light"}]
    region_path.write_text(toml_source(region), encoding="utf-8")
    compiler = test.bin / ("tor-scenario" + test.suffix)
    result = subprocess.run([compiler, "validate", package], capture_output=True, text=True, timeout=30)
    test.assertEqual(result.returncode, 0, result.stdout + result.stderr)
    return package
