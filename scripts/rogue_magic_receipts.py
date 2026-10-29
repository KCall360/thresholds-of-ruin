"""Independent finite magic-stock expectations for private population audits.

These constants are reviewed test policy, never loaded from production content.
Receipts establish owned stock; they must not guide ordinary play or navigation.
"""

MAGIC_ITEM_STAGES = (
    ("rogue.magic_potions", ("rogue.healing", "rogue.confusion", "rogue.blindness", "rogue.poison", "rogue.see_invisible", "rogue.extra_healing", "rogue.levitation")),
    ("rogue.magic_scrolls", ("rogue.identify_potion", "rogue.identify_scroll",
                             "rogue.identify_wand", "rogue.sleep", "rogue.teleportation")),
    ("rogue.magic_devices", ("rogue.wand_fire", "rogue.wand_cold", "rogue.wand_lightning", "rogue.wand_invisibility", "rogue.wand_haste", "rogue.wand_slow", "rogue.wand_teleport_away", "rogue.wand_teleport_to")),
)
MAGIC_ITEM_STAGE_IDS = tuple(identity for identity, _ in MAGIC_ITEM_STAGES)


def assert_magic_item_receipts(test, stages, *, require_placed=False):
    """Require exactly one finite attempt per stage and return placed item IDs."""
    return _assert_finite_item_receipts(test, stages, MAGIC_ITEM_STAGES, require_placed=require_placed)


def _assert_finite_item_receipts(test, stages, expected, *, require_placed=False, quantities=None):
    """Exact finite-stage schema; callers supply independent reviewed policy."""
    quantities = {} if quantities is None else quantities
    test.assertEqual([stage["id"] for stage in stages], [identity for identity, _ in expected])
    identities = []
    for stage, (identity, choices) in zip(stages, expected, strict=True):
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
            test.assertFalse(require_placed, "The seed must place each finite stock stage")
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
        test.assertEqual(entity["quantity"], quantities.get(identity, 1))
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
    ("rogue.rings", ('rogue.ring_protection', 'rogue.ring_add_strength', 'rogue.ring_sustain_strength', 'rogue.ring_see_invisible', 'rogue.ring_adornment', 'rogue.ring_dexterity', 'rogue.ring_increase_damage', 'rogue.ring_regeneration', 'rogue.ring_slow_digestion', 'rogue.ring_maintain_armor', 'rogue.ring_searching', 'rogue.ring_teleportation', 'rogue.ring_stealth', 'rogue.ring_aggravation')),
)
EQUIPMENT_ITEM_STAGE_IDS = tuple(identity for identity, _ in EQUIPMENT_ITEM_STAGES)


def assert_finite_item_identity(test, item, content):
    """Birth archetypes may intentionally share a neutral equipment identity."""
    test.assertEqual(item["spec"]["archetype"], content)
    identity = {"rogue.generated_mace": "rogue.mace",
                "rogue.generated_ring_mail": "rogue.ring_mail"}.get(content, content)
    test.assertEqual(item["spec"]["identity"], identity)


def assert_equipment_item_receipts(test, stages, *, require_placed=False):
    """Require finite equipment/ammunition/ring attempts and return placed item IDs."""
    return _assert_finite_item_receipts(test, stages, EQUIPMENT_ITEM_STAGES,
        require_placed=require_placed, quantities={"rogue.ammunition": 6})


CONTROL_ITEM_STAGES = (
    ("rogue.control_scrolls", ("rogue.aggravate_monster", "rogue.create_monster",
                             "rogue.hold_monster", "rogue.monster_confusion", "rogue.scare_monster")),
)
CONTROL_ITEM_STAGE_IDS = tuple(identity for identity, _ in CONTROL_ITEM_STAGES)


def assert_control_item_receipts(test, stages, *, require_placed=False):
    """Require the one finite control-scroll attempt and return its owned ID."""
    return _assert_finite_item_receipts(test, stages, CONTROL_ITEM_STAGES, require_placed=require_placed)


LIVE_CONTENT_STAGE_IDS = ("rogue.expanded_stock", "rogue.treasure_room")
EXPANDED_ITEM_CHOICES = tuple("rogue." + name for name in (
    "haste", "gain_strength", "raise_level", "hallucination", "monster_detection",
    "magic_detection", "food_detection", "magic_mapping", "wand_nothing", "wand_light",
    "wand_drain_life", "wand_cancellation", "wand_magic_missile", "wand_polymorph",
    "fruit", "dart", "shuriken"))
TREASURE_ITEM_CHOICES = tuple("rogue." + name for name in ("gold", "healing", "fruit", "dart", "shuriken"))


def _bounded_integer(test, value, minimum, maximum):
    test.assertIs(type(value), int)
    test.assertGreaterEqual(value, minimum)
    test.assertLessEqual(value, maximum)


def _assert_live_attempt(test, attempt, choices, kind, depth):
    test.assertIn(attempt["content"], choices)
    if attempt["kind"] == "skipped":
        test.assertEqual(attempt, {"kind": "skipped", "content": attempt["content"],
                                   "reason": "no_eligible_cell"})
        return []
    test.assertEqual(set(attempt), {"kind", "content", "location", "entity"})
    test.assertEqual(attempt["kind"], "placed")
    location = attempt["location"]
    test.assertEqual(set(location), {"region", "position"})
    _bounded_integer(test, location["region"], 1, (1 << 64) - 1)
    test.assertEqual(set(location["position"]), {"x", "y", "z"})
    for coordinate in location["position"].values():
        _bounded_integer(test, coordinate, -(1 << 31), (1 << 31) - 1)
    entity = attempt["entity"]
    test.assertEqual(set(entity), {"kind", "id", "quantity" if kind == "item" else "carried_loot"})
    test.assertEqual(entity["kind"], kind)
    _bounded_integer(test, entity["id"], 1, (1 << 64) - 1)
    identities = [(kind, entity["id"])]
    if kind == "item":
        _bounded_integer(test, entity["quantity"], 1, 1)
        return identities
    policy = choices[attempt["content"]].get("carried_loot")
    stock = entity["carried_loot"]
    if policy is None:
        test.assertIsNone(stock)
        return identities
    test.assertIsInstance(stock, dict)
    if stock["kind"] == "not_carried":
        test.assertEqual(stock, {"kind": "not_carried"})
        test.assertLess(policy["chance"]["numerator"], policy["chance"]["denominator"])
        return identities
    test.assertEqual(set(stock), {"kind", "content", "quantity", "item"})
    test.assertEqual(stock["kind"], "carried")
    _bounded_integer(test, stock["item"], 1, (1 << 64) - 1)
    if policy["pool"] == "rogue.leprechaun_gold":
        test.assertEqual(stock["content"], "rogue.gold")
        _bounded_integer(test, stock["quantity"], 2, 51 + 10 * depth)
    else:
        test.assertEqual(policy["pool"], "rogue.vampire_supplies")
        test.assertIn(stock["content"], ("rogue.ration", "rogue.clean_bandage"))
        _bounded_integer(test, stock["quantity"], 1, 1)
    identities.append(("item", stock["item"]))
    return identities


def assert_live_content_receipts(test, stages, depth):
    """Strict appended-stage policy; return all births, including carried stock.

    Actor eligibility comes from the independent historical roster oracle, not
    the supplied manifest or a receipt. This does not assert occurrence rates.
    """
    from rogue_activation_oracle import reviewed_twenty_six_activation

    _bounded_integer(test, depth, 1, 1_000_000)
    test.assertEqual([stage["id"] for stage in stages], list(LIVE_CONTENT_STAGE_IDS))
    for stage in stages:
        test.assertEqual(set(stage), {"id", "version", "result"})
        _bounded_integer(test, stage["version"], 1, 1)
    stock = stages[0]["result"]
    test.assertEqual(set(stock), {"kind", "requested", "attempts"})
    test.assertEqual(stock["kind"], "evaluated")
    _bounded_integer(test, stock["requested"], 0, 1)
    test.assertIsInstance(stock["attempts"], list)
    test.assertEqual(len(stock["attempts"]), stock["requested"])
    identities = []
    for attempt in stock["attempts"]:
        identities.extend(_assert_live_attempt(test, attempt, EXPANDED_ITEM_CHOICES, "item", depth))
    cluster = stages[1]["result"]
    test.assertEqual(set(cluster), {"kind", "selection"})
    test.assertEqual(cluster["kind"], "room_cluster")
    selected = cluster["selection"]
    if selected["kind"] in ("chance_miss", "no_eligible_room"):
        test.assertEqual(set(selected), {"kind"})
        return identities
    test.assertEqual(set(selected), {"kind", "room", "room_area", "actor_depth", "item_requested",
                                    "actor_requested", "items", "actors"})
    test.assertEqual(selected["kind"], "selected")
    _bounded_integer(test, selected["room"], 1, 64)
    _bounded_integer(test, selected["room_area"], 1, 1024)
    _bounded_integer(test, selected["actor_depth"], depth + 1, depth + 1)
    _bounded_integer(test, selected["item_requested"], 2, 4)
    _bounded_integer(test, selected["actor_requested"], 1, 3)
    actor_depth = selected["actor_depth"]
    bands = reviewed_twenty_six_activation()["profiles"]["rogue.low_depth"]["stages"][0]["bands"]
    actors = {choice["archetype"]: choice for band in bands
              if band["depth"]["minimum"] <= actor_depth
              and (band["depth"]["kind"] == "open" or actor_depth <= band["depth"]["maximum"])
              for choice in band["choices"]}
    test.assertTrue(actors)
    for role, choices, kind in (("items", TREASURE_ITEM_CHOICES, "item"), ("actors", actors, "actor")):
        test.assertIsInstance(selected[role], list)
        requested = selected["item_requested" if kind == "item" else "actor_requested"]
        test.assertEqual(len(selected[role]), min(requested, selected["room_area"]))
        for attempt in selected[role]:
            identities.extend(_assert_live_attempt(test, attempt, choices, kind, actor_depth))
    test.assertEqual(len(identities), len(set(identities)))
    return identities
