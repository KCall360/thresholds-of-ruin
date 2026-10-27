"""Real-process Hallucination fixture, retaining all production cosmetic rows.

Production item effects, conditions and species are copied unchanged by the
shared fixture. Geometry, finite carried stock and a speed-zero external pair
are explicit test setup. Optional historical mode adds one ordinary paid plan
observation ability and a real door, never a wizard operation.
"""
import tomllib

from arena_matrix import toml_source
from discovery_package import write_package
from hallucination_package import fixture


def prepare(test, *, historical=False):
    data = fixture()
    data["manifest"]["id"] = "imported-hallucination-history" if historical else "imported-hallucination"
    if historical:
        rules = tomllib.loads(data["rules"])
        rules["overrides"].append({"op": "add", "path": ["abilities", "fixture.hallucination_plan"], "value": {
            "id": "fixture.hallucination_plan", "name": "Observe historical plan", "tags": [],
            "basic_attack": False, "invocation": "actor_target", "interaction": "non_hostile",
            "timing": {"source": {"kind": "fixed", "prepare": 100, "recover": 100}},
            "reach": {"kind": "sight", "range": 6},
            "strikes": {"kind": "authored", "strikes": [{"offset": 0,
                "on_success": [{"kind": "observe_public_plan"}], "on_failure": [], "descriptors": [], "riders": []}]},
            "ai": {"priority": 0, "hostile": False, "minimum_resource_percent": 0, "range_preference": "any"}}})
        data["rules"] = toml_source(rules)
        data["manifest"]["creatures"]["species"]["fixture.hallucination"]["modifiers"].append({
            "scope": "always", "op": {"kind": "grant_ability", "ability": "fixture.hallucination_plan"}})
        region = data["regions"][0]
        region["actors"][0]["at"] = [5, 2, 0]
        next(item for item in region["items"] if item["id"] == 40)["at"] = [4, 2, 0]
        region["walls"] = [[3, y, z] for y in range(5) if y != 2 for z in range(2)]
        region["doors"] = [{"id": 1, "at": [3, 2, 0], "height": 2, "open": True}]
    return write_package(test, data, data["manifest"]["id"])
