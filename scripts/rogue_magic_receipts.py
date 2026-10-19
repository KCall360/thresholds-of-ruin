"""Independent finite magic-stock expectations for private population audits.

These constants are reviewed test policy, never loaded from production content.
Receipts establish owned stock; they must not guide ordinary play or navigation.
"""

MAGIC_ITEM_STAGES = (
    ("rogue.magic_potions", ("rogue.healing", "rogue.confusion", "rogue.blindness")),
    ("rogue.magic_scrolls", ("rogue.identify_potion", "rogue.identify_scroll",
                             "rogue.identify_wand", "rogue.sleep")),
    ("rogue.magic_devices", ("rogue.wand_fire", "rogue.wand_cold", "rogue.wand_lightning")),
)
MAGIC_ITEM_STAGE_IDS = tuple(identity for identity, _ in MAGIC_ITEM_STAGES)


def assert_magic_item_receipts(test, stages, *, require_placed=False):
    """Require exactly one finite attempt per stage and return placed item IDs."""
    test.assertEqual([stage["id"] for stage in stages], list(MAGIC_ITEM_STAGE_IDS))
    identities = []
    for stage, (identity, choices) in zip(stages, MAGIC_ITEM_STAGES, strict=True):
        test.assertEqual(set(stage), {"id", "version", "result"})
        test.assertIs(type(stage["version"]), int)
        test.assertEqual((stage["id"], stage["version"]), (identity, 1))
        result = stage["result"]
        test.assertEqual(set(result), {"kind", "requested", "attempts"})
        test.assertEqual(result["kind"], "evaluated")
        test.assertIs(type(result["requested"]), int)
        test.assertEqual(result["requested"], 1)
        test.assertEqual(len(result["attempts"]), 1)
        attempt = result["attempts"][0]
        test.assertIn(attempt["content"], choices)
        if attempt["kind"] == "skipped":
            test.assertEqual(attempt, {"kind": "skipped", "content": attempt["content"],
                                       "reason": "no_eligible_cell"})
            test.assertFalse(require_placed, "The seed must place each finite magic-stock stage")
            continue
        test.assertEqual(set(attempt), {"kind", "content", "location", "entity"})
        test.assertEqual(attempt["kind"], "placed")
        location = attempt["location"]
        test.assertEqual(set(location), {"region", "position"})
        test.assertIs(type(location["region"]), int)
        test.assertGreater(location["region"], 0)
        test.assertLess(location["region"], 1 << 64)
        test.assertEqual(set(location["position"]), {"x", "y", "z"})
        for coordinate in location["position"].values():
            test.assertIs(type(coordinate), int)
        entity = attempt["entity"]
        test.assertEqual(set(entity), {"kind", "id", "quantity"})
        test.assertEqual(entity["kind"], "item")
        test.assertIs(type(entity["id"]), int)
        test.assertGreater(entity["id"], 0)
        test.assertLess(entity["id"], 1 << 64)
        test.assertIs(type(entity["quantity"]), int)
        test.assertEqual(entity["quantity"], 1)
        identities.append(("item", entity["id"]))
    test.assertEqual(len(identities), len(set(identities)))
    return identities
