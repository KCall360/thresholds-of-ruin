"""Source utility devices with explicit accuracy and collinear fixture targets."""
import copy
import json

from utility_potions_package import fixture as potion_fixture, inventory_item, observation


STATUS_IDS = {"invisibility": "rogue.invisible", "haste": "rogue.hasted", "slow": "rogue.slowed"}


def fixture():
    data = potion_fixture()
    manifest = data["manifest"]
    manifest["id"] = "rogue-utility-wands"
    caster = manifest["characters"][0]
    target = copy.deepcopy(caster["creature"])
    target.update(name="Utility target", faction="delvers")
    rear = copy.deepcopy(target)
    rear["name"] = "Rear target"
    species = copy.deepcopy(manifest["creatures"]["species"]["rogue.human"])
    # Explicit synthetic accuracy uses an ordinary derived-attribute modifier;
    # production base attributes, utility effects and recipients stay untouched.
    species.setdefault("modifiers", []).append({"scope": "always", "op": {
        "kind": "add", "stat": {"kind": "attribute", "attribute": "intellect"}, "value": 100}})
    manifest["creatures"]["species"]["fixture.utility_caster"] = species
    caster["creature"]["species"] = "fixture.utility_caster"
    body = copy.deepcopy(manifest["creatures"]["body_packages"][caster["creature"]["body_package"]])
    body["species"] = "fixture.utility_caster"
    manifest["creatures"]["body_packages"]["fixture.body.utility_caster"] = body
    caster["creature"]["body_package"] = "fixture.body.utility_caster"
    caster["loadout"] = [{"archetype": "rogue.wand_" + kind, "quantity": 1}
                         for kind in ("haste", "slow", "invisibility")]
    caster["known_identities"] = ["rogue.wand_haste", "rogue.wand_slow"]
    data["regions"][0]["actors"] = [
        {"id": 3, "at": [3, 2, 0], "turn_ticks": 100, "controller": "external", "creature": target,
         "body": copy.deepcopy(caster["body"])},
        {"id": 4, "at": [4, 2, 0], "turn_ticks": 100, "controller": "external", "creature": rear,
         "body": copy.deepcopy(caster["body"])},
    ]
    return data


def initial_items(test, frame):
    inventory = observation(frame)["inventory"]
    test.assertEqual(len(inventory), 3)
    # Independent fixed seed42 form vector: haste and slow are staves.
    haste = next(row for row in inventory if row["name"] == "staff of haste monster")
    slow = next(row for row in inventory if row["name"] == "staff of slow monster")
    invisible = next(row for row in inventory if not row["identified"])
    test.assertEqual(invisible["name"], "yew staff")
    test.assertNotIn("invisibility", invisible["name"])
    return {"haste": haste["id"], "slow": slow["id"], "invisibility": invisible["id"]}


def persistent_conditions(frame):
    return [row for row in observation(frame)["combat"]["own_stats"]["conditions"]
            if row["condition"]["id"] in STATUS_IDS.values()]


def assert_persistent(test, frame, *kinds):
    rows = persistent_conditions(frame)
    test.assertEqual({row["condition"]["id"] for row in rows}, {STATUS_IDS[kind] for kind in kinds})
    test.assertEqual(len(rows), len(kinds), "Repeat use must retain one application")
    for row in rows:
        test.assertEqual(row["phase"], "active")
        test.assertIsNone(row["remaining_ticks"], "Persistent source status must not fabricate a deadline")


def assert_invisibility(test, caster, target, rear, invisible_id):
    test.assertFalse(any(row["name"] == "Utility target" for row in observation(caster)["visible_actors"]))
    test.assertTrue(any(row["name"] == "Rear target" for row in observation(caster)["visible_actors"]))
    test.assertTrue(any(row["condition"]["id"] == STATUS_IDS["invisibility"] for row in persistent_conditions(target)))
    assert_persistent(test, rear)
    test.assertFalse(inventory_item(caster, invisible_id)["identified"])
    offer = next(row for row in observation(caster)["interactions"]["inventory"] if row["item"] == invisible_id)
    test.assertIsNone(offer["charges"], "Unknown effective use cannot disclose instance charges")

    # Legitimate status and disappearance remain observable, but the hidden
    # source ability/type must not reappear through history or another view.
    for frame in (caster, target, rear):
        public = json.dumps(frame, sort_keys=True)
        for hidden in ("rogue.wand_invisibility", "Wand of Invisibility", "wand of invisibility", "staff of invisibility"):
            test.assertNotIn(hidden, public)


def live_combat_events(message):
    """Read actual ordered outcome facts, never pictures, history or client state."""
    if message.get("type") != "update":
        return []
    body = message["update"]["body"]
    if body["type"] != "outcome":
        return []
    return [fact["event"] for fact in body["facts"]["facts"] if fact["kind"] == "combat"]


def assert_anonymous_recipient_feedback(test, messages, recipient):
    """A real applied utility contact retains only the recipient's licensed refs."""
    view = observation(recipient)
    allowed_sources = {row["id"] for row in view["visible_actors"]}
    found = [(message, event) for message in messages
             if message.get("type") == "update" and message["update"]["actor"] == "3"
             for event in live_combat_events(message) if event["type"] == "anonymous_ability"]
    test.assertTrue(found, "The actual recipient must receive live anonymous utility feedback")
    for message, event in found:
        test.assertEqual(set(event), {"type", "caster", "target", "outcome"})
        test.assertEqual(event["target"], view["self_target"])
        test.assertIn(event["caster"], allowed_sources)
        test.assertEqual(event["outcome"], "applied")
        encoded = json.dumps(message, sort_keys=True)
        for hidden in ("rogue.wand_invisibility", "Wand of Invisibility", "wand of invisibility", "staff of invisibility"):
            test.assertNotIn(hidden, encoded)
    return found[0][0]
