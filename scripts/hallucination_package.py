"""Production Hallucination in a bounded lit encounter, not production stock.

Only geometry, finite fixture stock and two speed-zero external actors differ.
All item effects, appearance pools and production actor catalog rows are copied
unchanged. The source-deleted checkpoint witness lives in the Rust Service owner.
"""
import copy

from utility_potions_package import fixture as utility_fixture


def fixture():
    result = utility_fixture()
    manifest = result["manifest"]
    manifest["id"] = "rogue-hallucination-service"
    figure = copy.deepcopy(manifest["creatures"]["species"]["rogue.human"])
    figure["modifiers"] = [{"scope": "always", "op": {"kind": "set",
        "stat": {"kind": "attribute", "attribute": "speed"}, "value": 0}}]
    manifest["creatures"]["species"]["fixture.hallucination"] = figure
    owner = manifest["characters"][0]
    owner.update(anchor="1/start", turn_ticks=100, loadout=[], known_identities=["rogue.gold"])
    owner["creature"].update(species="fixture.hallucination", nutrition={"kind": "exempt"},
                             future_growth={"kind": "disabled"})
    body = copy.deepcopy(manifest["creatures"]["body_packages"][owner["creature"]["body_package"]])
    body["species"] = "fixture.hallucination"
    manifest["creatures"]["body_packages"]["fixture.body.hallucination"] = body
    owner["creature"]["body_package"] = "fixture.body.hallucination"
    peer = copy.deepcopy(owner["creature"])
    peer["name"] = "actual visible peer"
    result["regions"] = [{"id": 1, "name": "Lit Hallucination laboratory", "size": [7, 5, 2],
        "chamber": True, "lit": True, "anchors": {"start": [2, 2, 0]},
        "actors": [{"id": 2, "at": [4, 2, 0], "turn_ticks": 100, "controller": "external",
                    "creature": peer, "body": copy.deepcopy(owner["body"])}],
        "items": [*[{"id": identity, "at": [2, 2, 0], "archetype": "rogue." + kind,
                    "quantity": quantity, "carried_by": 1}
                   for identity, kind, quantity in ((30, "hallucination", 6), (31, "extra_healing", 3),
                       (32, "blindness", 4), (33, "healing", 2), (34, "see_invisible", 5))],
                  {"id": 40, "at": [3, 2, 0], "archetype": "rogue.gold", "quantity": 7}]}]
    return result


def initial_items(test, frame):
    """Select finite fixture doses using independent disclosed quantities."""
    inventory = frame["state"]["observation"]["inventory"]
    expected = {"hallucination": "6", "extra": "3", "blind": "4", "healing": "2", "sight": "5"}
    result = {}
    for key, quantity in expected.items():
        rows = [row for row in inventory if row["quantity"] == quantity]
        test.assertEqual(len(rows), 1)
        test.assertFalse(rows[0]["identified"])
        result[key] = rows[0]["id"]
    return result


def assert_sample(test, observation, phase):
    # Frozen values independently evaluated from the documented selector recipe
    # and canonical production tuple roster: seed42, observer1, peer2/item40.
    # The opt-in Polymorph maple-wand label makes81 object tuples, not80;
    # no combat RNG or actor-HD operand participates in this selector.
    expected = {0: ("emu", "creature.bird.emu", "smoky potion", "item.potion", "potion"),
                1: ("bat", "creature.bat", "ivory ring", "item.ring", "ring")}[phase]
    name, asset, object_name, object_asset, cosmetic_class = expected
    peers = [row for row in observation["visible_actors"] if row["id"] != observation["self_target"]]
    # One creature may have several genuinely disclosed body-cell occurrences.
    # Keep and check every row; occurrence count is not an actor population count.
    test.assertEqual(len({row["id"] for row in peers}), 1)
    for peer in peers:
        test.assertEqual((peer["name"], peer.get("asset"), peer["description"]),
                         (name, asset, ""))
        test.assertEqual(peer["appearance_seed"], "p_" + "00" * 32)
    test.assertEqual(len(observation["ground_items"]), 1)
    ground = observation["ground_items"][0]
    test.assertEqual(ground["presentation"], {"name": object_name, "asset": object_asset,
        "kind": {"kind": "item", "class": cosmetic_class}})
    item = ground["item"]
    test.assertEqual((item["name"], item["appearance"], item["description"]), ("object", "object", ""))
    test.assertIsNone(item.get("asset"))
    test.assertIsNone(item.get("corpse"))
    test.assertEqual((item["class"], item["quantity"], item["known_currency"]), ("coin", "7", "gold"))
    test.assertTrue(all(not row["stairs_up"] and not row["stairs_down"] for row in observation["visible_cells"]))
