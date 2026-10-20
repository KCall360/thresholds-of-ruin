"""Independent finite magic-stock expectations for private population audits.

These constants are reviewed test policy, never loaded from production content.
Receipts establish owned stock; they must not guide ordinary play or navigation.
"""

MAGIC_ITEM_STAGES = (
    ("rogue.magic_potions", ("rogue.healing", "rogue.confusion", "rogue.blindness", "rogue.poison", "rogue.see_invisible", "rogue.extra_healing")),
    ("rogue.magic_scrolls", ("rogue.identify_potion", "rogue.identify_scroll",
                             "rogue.identify_wand", "rogue.sleep")),
    ("rogue.magic_devices", ("rogue.wand_fire", "rogue.wand_cold", "rogue.wand_lightning", "rogue.wand_invisibility", "rogue.wand_haste", "rogue.wand_slow")),
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


EQUIPMENT_ITEM_STAGES = (
    ("rogue.equipment", ("rogue.generated_mace", "rogue.long_sword", "rogue.dagger",
                         "rogue.two_handed_sword", "rogue.spear", "rogue.leather_armor",
                         "rogue.generated_ring_mail", "rogue.studded_leather", "rogue.scale_mail",
                         "rogue.chain_mail", "rogue.splint_mail", "rogue.banded_mail", "rogue.plate_mail",
                         "rogue.shortbow")),
    ("rogue.equipment_scrolls", ("rogue.enchant_armor", "rogue.protect_armor", "rogue.enchant_weapon",
                                 "rogue.remove_curse", "rogue.identify_weapon", "rogue.identify_armor")),
    ("rogue.ammunition", ("rogue.arrow",)),
)
EQUIPMENT_ITEM_STAGE_IDS = tuple(identity for identity, _ in EQUIPMENT_ITEM_STAGES)


def assert_finite_item_identity(test, item, content):
    """Birth archetypes may intentionally share a neutral equipment identity."""
    test.assertEqual(item["spec"]["archetype"], content)
    identity = {"rogue.generated_mace": "rogue.mace",
                "rogue.generated_ring_mail": "rogue.ring_mail"}.get(content, content)
    test.assertEqual(item["spec"]["identity"], identity)


def assert_equipment_item_receipts(test, stages, *, require_placed=False):
    """Require finite equipment/ammunition attempts and return placed item IDs."""
    test.assertEqual([stage["id"] for stage in stages], list(EQUIPMENT_ITEM_STAGE_IDS))
    identities = []
    for stage, (identity, choices) in zip(stages, EQUIPMENT_ITEM_STAGES, strict=True):
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
            test.assertFalse(require_placed, "The seed must place each finite equipment-stock stage")
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
        test.assertEqual(entity["quantity"], 6 if identity == "rogue.ammunition" else 1)
        identities.append(("item", entity["id"]))
    test.assertEqual(len(identities), len(set(identities)))
    return identities
