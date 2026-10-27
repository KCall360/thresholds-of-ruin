"""Exact imported first-contact wands with ordinary, source-defined recipients."""
import copy
import subprocess
import tomllib

from arena_matrix import toml_source
from nothing_catalog_package import prepare as prepare_nothing, PRODUCTION


def prepare(test, *, missile, grant=False):
    package = prepare_nothing(test)
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest["id"] = "imported-first-contact"
    owner = manifest["creatures"]["species"]["figure"]
    test.assertEqual(owner["attributes"]["intellect"], 0, "validated source base remains within its attribute cap")
    owner["modifiers"] = [{"scope": "always", "op": {
        "kind": "set", "stat": {"kind": "health"}, "value": 50}}]
    recipient = copy.deepcopy(owner)
    # Species base attributes obey the source cap (5); synthetic accuracy is
    # an ordinary derived modifier, as in the existing utility-wand fixture.
    # Copy the recipient first so its own Agility/Intellect stay unchanged.
    owner["modifiers"].append({"scope": "always", "op": {
        "kind": "add", "stat": {"kind": "attribute", "attribute": "intellect"}, "value": 100}})
    if grant:
        test.assertFalse(missile)
        owner["modifiers"].append({"scope": "always", "op": {
            "kind": "grant_ability", "ability": "rogue.wand_cancellation"}})
    if not missile:
        recipient["modifiers"].append({"cancellable": True, "scope": "always", "op": {
            "kind": "perception", "value": {"kind": "invisible", "enabled": True}}})
    manifest["creatures"]["species"]["recipient"] = recipient
    path.write_text(toml_source(manifest), encoding="utf-8")
    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    test.assertEqual(region["size"], [7, 5, 2])
    test.assertEqual(region["anchors"]["start"], [1, 2, 0])
    test.assertTrue(region["chamber"])
    test.assertFalse(region.get("actors", []))
    creature = copy.deepcopy(manifest["characters"][0]["creature"])
    creature.update(species="recipient", name="contact recipient")
    region["actors"] = [{"id": 2, "at": [3, 2, 0], "turn_ticks": 100,
                         "controller": "external", "creature": creature}]
    region["items"] = [row for row in region["items"] if row["id"] != 31]
    for row in region["items"]:
        if row["id"] == 30:
            row["archetype"] = "rogue.wand_magic_missile" if missile else "rogue.wand_cancellation"
    path.write_text(toml_source(region), encoding="utf-8")
    for relative in ("catalog/items.toml", "rules.toml"):
        test.assertEqual((package / relative).read_bytes(), (PRODUCTION / relative).read_bytes())
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, timeout=30)
    test.assertEqual(result.returncode, 0, result.stdout + result.stderr)
    return package
