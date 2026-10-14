"""Small authored probes for transport tests, independent of shipped talents.

These abilities pin payment, timing, check and range contracts. They are explicit
fixture content, not aliases or compatibility definitions for retired rules.
Tests of acquisition must instead buy the production root and exact form.
"""
from copy import deepcopy


TRANSPORT_ABILITIES = ("fixture_stamina_strike", "fixture_mana_bolt", "fixture_focus_fear")


def transport_ability(identity):
    """Return one fresh, narrowly named payment/check probe."""
    common = {"id": identity, "basic_attack": False, "tags": [],
              "timing": {"source": {"kind": "fixed", "prepare": 100, "recover": 100},
                         "scaled_by": "speed"},
              "reach": {"kind": "sight", "range": 6}}
    strike = {"offset": 0, "on_failure": [], "descriptors": [], "riders": []}
    ai = {"priority": 10, "hostile": True, "minimum_resource_percent": 0,
          "range_preference": "any"}
    if identity == "fixture_stamina_strike":
        common.update(name="Fixture Stamina Strike", tags=["weapon_attack"],
                      timing={"source": {"kind": "selected_weapon"}, "scaled_by": "speed"},
                      reach={"kind": "melee"})
        pool, skill, defense = "stamina", {"kind": "selected_weapon"}, "physical"
        strike["on_success"] = [{"kind": "damage", "damage": {
            "kind": "selected_weapon", "impact_bonus": 3}}]
        ai["priority"] = 20
    elif identity == "fixture_mana_bolt":
        common["name"] = "Fixture Mana Bolt"
        pool, skill, defense = "mana", {"kind": "named", "skill": "spellcasting"}, "cognitive"
        strike["on_success"] = [{"kind": "damage", "damage": {"kind": "components", "components": [
            {"category": "energy", "amount": {"kind": "rolled", "dice": {
                "count": 1, "sides": 6, "bonus": 1}}}]}}]
        ai["range_preference"] = "outside_melee"
    elif identity == "fixture_focus_fear":
        common["name"] = "Fixture Focus Fear"
        pool, skill, defense = "focus", {"kind": "named", "skill": "intimidation"}, "spiritual"
        strike["on_success"] = [{"kind": "apply_condition", "condition": "fear", "duration": 300}]
        strike["descriptors"] = ["fear", "mind_affecting"]
        ai.update(priority=30, minimum_resource_percent=100)
    else:
        raise ValueError(f"unknown transport probe: {identity}")
    strike["check"] = {"actor": "user", "skill": skill,
                       "against": {"kind": "defense", "defense": defense}}
    common.update(cost={"resource": pool, "start": 1, "resolution": 1},
                  strikes={"kind": "authored", "strikes": [strike]}, ai=ai)
    return common


def add_transport_abilities(rules, identities):
    """Add only explicitly requested probes; leave production rows untouched."""
    existing = {row["id"] for row in rules["abilities"]}
    for identity in dict.fromkeys(identities):
        if identity in TRANSPORT_ABILITIES and identity not in existing:
            rules["abilities"].append(transport_ability(identity))
            existing.add(identity)


def ability_alias(rules, identity, template, display_name):
    """Author a test-specific name over an explicitly selected definition."""
    definition = deepcopy(next(row for row in rules["abilities"] if row["id"] == template))
    definition.update(id=identity, name=display_name, basic_attack=False)
    rules["abilities"].append(definition)
