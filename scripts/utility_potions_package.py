"""Production potion content in a bounded, explicitly authored encounter.

Only geometry, finite fixture stock, nutrition enrollment and external Phantom
control/timing differ from production. No effect, condition or species is edited.
The resolved catalog is used by both the Service and actual-process witnesses.
"""
import copy
import tomllib

from rogue_roster_package import PRODUCTION, encounter_sources


def fixture():
    source = encounter_sources()
    catalog = tomllib.loads((PRODUCTION / "catalog/items.toml").read_text(encoding="utf-8"))
    manifest = {key: copy.deepcopy(source["manifest"][key])
                for key in ("format", "version", "ruleset", "factions", "ai_profiles", "themes", "assets")}
    character = copy.deepcopy(source["manifest"]["characters"][0])
    character.update(anchor="1/start", loadout=[
        {"archetype": "rogue.poison", "quantity": 2},
        {"archetype": "rogue.see_invisible", "quantity": 3},
        {"archetype": "rogue.extra_healing", "quantity": 5},
        {"archetype": "rogue.blindness", "quantity": 2},
        {"archetype": "rogue.restore_strength", "quantity": 1}],
        known_identities=["rogue.blindness", "rogue.restore_strength"])
    character["creature"]["nutrition"] = {"kind": "exempt"}
    phantom = {"archetype": "rogue.phantom", "id": 3, "at": [3, 2, 0],
               "turn_ticks": 1000000, "controller": "external"}
    manifest.update(id="rogue-utility-potions", default_character=1, characters=[character],
                    archetypes={**source["actors"], **source["items"]},
                    creatures={"species": source["species"]}, appearance_pools=catalog["appearance_pools"])
    region = {"id": 1, "name": "Lit utility potion encounter", "size": [7, 5, 2],
              "chamber": True, "gravity": [0, 0, -1], "anchors": {"start": [1, 2, 0]},
              "actors": [phantom]}
    return {"manifest": manifest, "regions": [region],
            "rules": (PRODUCTION / "rules.toml").read_text(encoding="utf-8"),
            "seed": 42, "wizard": False, "frame_bytes": 65536,
            "client_bytes": 262144, "total_bytes": 524288}


def observation(frame):
    return frame["state"]["observation"]


def condition(frame, identity):
    return next((row for row in observation(frame)["combat"]["own_stats"]["conditions"]
                 if row["condition"]["id"] == identity), None)


def deadline(frame, identity="rogue.see_invisible"):
    return int(observation(frame)["tick"]) + int(condition(frame, identity)["remaining_ticks"])


def inventory_item(frame, identity):
    return next(row for row in observation(frame)["inventory"] if row["id"] == identity)


def initial_items(test, frame):
    """Select fixture doses solely by disclosed quantity/name, not private IDs."""
    rows = observation(frame)["inventory"]
    poison = next(row for row in rows if row["quantity"] == "2" and not row["identified"])
    sight = next(row for row in rows if row["quantity"] == "3")
    extra = next(row for row in rows if row["quantity"] == "5")
    blind = next(row for row in rows if row["name"] == "potion of blindness")
    restore = next(row for row in rows if row["name"] == "potion of restore strength")
    test.assertFalse(poison["identified"])
    test.assertFalse(sight["identified"])
    test.assertFalse(extra["identified"])
    test.assertNotEqual(poison["appearance"], sight["appearance"])
    test.assertFalse(any(row["name"] == "phantom" for row in observation(frame)["visible_actors"]))
    return {"poison": poison["id"], "sight": sight["id"], "extra": extra["id"],
            "blind": blind["id"], "restore": restore["id"]}


def assert_poison(test, frame, identity):
    deficit = observation(frame)["combat"]["own_stats"]["restorable_losses"]["attributes"]["strength"]
    test.assertIn(deficit, (1, 2, 3))
    item = inventory_item(frame, identity)
    test.assertEqual((item["name"], item["quantity"], item["identified"]), ("potion of poison", "1", True))
    return deficit


def assert_sight(test, frame, identity):
    test.assertIsNone(condition(frame, "rogue.blindness"))
    test.assertIsNotNone(condition(frame, "rogue.see_invisible"))
    test.assertTrue(any(row["name"] == "phantom" for row in observation(frame)["visible_actors"]))
    test.assertFalse(inventory_item(frame, identity)["identified"], "The effective cure/reveal must not identify")
    test.assertNotEqual(inventory_item(frame, identity)["name"], "potion of see invisible")


def assert_extra_healing(test, before, after, identity):
    test.assertIsNotNone(condition(before, "rogue.blindness"))
    test.assertIsNone(condition(after, "rogue.blindness"))
    item = inventory_item(after, identity)
    test.assertEqual((item["name"], item["quantity"], item["identified"]),
                     ("potion of extra healing", "4", True))
    test.assertEqual(observation(after)["combat"]["max_hp"], observation(before)["combat"]["max_hp"],
                     "The sole first HD is already at its immutable acquisition cap")
