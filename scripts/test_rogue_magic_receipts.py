"""Pure finite-stock receipt contracts run in every tooling tier, without apps.

Stage identities and eligible subtypes below are independent reviewed expectations,
not generated from production content or the receipt validator's own tables.
"""
import copy
import unittest

import rogue_magic_receipts

from rogue_magic_receipts import (
    EQUIPMENT_ITEM_STAGE_IDS, MAGIC_ITEM_STAGE_IDS,
    assert_equipment_item_receipts, assert_finite_item_identity, assert_magic_item_receipts,
)


class RogueMagicReceiptTests(unittest.TestCase):
    """Pure receipt fixtures retain the finite stock and ownership boundaries."""
    def receipts(self):
        return [{"id": identity, "version": 1, "result": {
            "kind": "evaluated", "requested": 1, "attempts": [{
                "kind": "placed", "content": content,
                "location": {"region": 1, "position": {"x": index, "y": 2, "z": 0}},
                "entity": {"kind": "item", "id": 100 + index, "quantity": 1}}]}}
            for index, (identity, content) in enumerate((
                ("rogue.magic_potions", "rogue.healing"),
                ("rogue.magic_scrolls", "rogue.identify_potion"),
                ("rogue.magic_devices", "rogue.wand_fire")))]

    def test_all_three_stages_return_every_placed_identity_for_cross_stage_duplicate_checks(self):
        self.assertEqual(MAGIC_ITEM_STAGE_IDS, (
            "rogue.magic_potions", "rogue.magic_scrolls", "rogue.magic_devices"))
        self.assertEqual(assert_magic_item_receipts(self, self.receipts(), require_placed=True),
                         [("item", 100), ("item", 101), ("item", 102)])

    def test_every_reviewed_subtype_is_eligible_only_in_its_magic_stage(self):
        for index, choices in enumerate((
                ("rogue.healing", "rogue.confusion", "rogue.blindness", "rogue.poison", "rogue.see_invisible", "rogue.extra_healing", "rogue.levitation"),
                ("rogue.identify_potion", "rogue.identify_scroll", "rogue.identify_wand", "rogue.sleep", "rogue.teleportation"),
                ("rogue.wand_fire", "rogue.wand_cold", "rogue.wand_lightning", "rogue.wand_invisibility", "rogue.wand_haste", "rogue.wand_slow", "rogue.wand_teleport_away", "rogue.wand_teleport_to"))):
            for choice in choices:
                with self.subTest(stage=index, content=choice):
                    receipts = self.receipts()
                    receipts[index]["result"]["attempts"][0]["content"] = choice
                    self.assertEqual(len(assert_magic_item_receipts(self, receipts)), 3)
                for wrong_stage in range(3):
                    if wrong_stage == index:
                        continue
                    receipts = self.receipts()
                    receipts[wrong_stage]["result"]["attempts"][0]["content"] = choice
                    with self.subTest(stage=index, wrong_stage=wrong_stage, content=choice):
                        with self.assertRaises(AssertionError):
                            assert_magic_item_receipts(self, receipts)

    def test_no_eligible_cell_skip_has_no_owned_item_and_cannot_satisfy_seeded_placement(self):
        receipts = self.receipts()
        for stage in receipts:
            stage["result"]["attempts"] = [{"kind": "skipped",
                "content": stage["result"]["attempts"][0]["content"], "reason": "no_eligible_cell"}]
        self.assertEqual(assert_magic_item_receipts(self, receipts), [])
        with self.assertRaises(AssertionError):
            assert_magic_item_receipts(self, receipts, require_placed=True)
        for mutation in ({"reason": "chance_miss"}, {"entity": {"kind": "item", "id": 100, "quantity": 1}}):
            changed = copy.deepcopy(receipts)
            changed[0]["result"]["attempts"][0].update(mutation)
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                assert_magic_item_receipts(self, changed)

    def test_omitted_reordered_or_extra_magic_stages_are_rejected(self):
        receipts = self.receipts()
        for changed in (receipts[:2], receipts[::-1], receipts + [receipts[0]]):
            with self.subTest(ids=[stage["id"] for stage in changed]), self.assertRaises(AssertionError):
                assert_magic_item_receipts(self, changed)

    def test_count_quantity_identity_and_owned_location_contracts_are_strict(self):
        attempt = ("result", "attempts", 0)
        mutations = (
            (("version",), 2), (("version",), True),
            (("result", "kind"), "chance_miss"),
            (("result", "requested"), 0), (("result", "requested"), 2),
            (("result", "requested"), True), (("result", "attempts"), []),
            (("result", "attempts"), self.receipts()[0]["result"]["attempts"] * 2),
            ((*attempt, "kind"), "unknown"),
            ((*attempt, "entity", "kind"), "actor"),
            ((*attempt, "entity", "id"), 0), ((*attempt, "entity", "id"), 1 << 64),
            ((*attempt, "entity", "id"), True), ((*attempt, "entity", "id"), 101),
            ((*attempt, "entity", "quantity"), 0), ((*attempt, "entity", "quantity"), 2),
            ((*attempt, "entity", "quantity"), True),
            ((*attempt, "entity", "carried_loot"), None),
            ((*attempt, "location", "region"), 0), ((*attempt, "location", "region"), True),
            ((*attempt, "location", "position", "x"), "0"),
        )
        for path, value in mutations:
            receipts = self.receipts()
            target = receipts[0]
            for key in path[:-1]:
                target = target[key]
            target[path[-1]] = value
            with self.subTest(path=path, value=value), self.assertRaises(AssertionError):
                assert_magic_item_receipts(self, receipts)


class RogueEquipmentReceiptTests(unittest.TestCase):
    """Pure receipt fixtures retain the finite stock and ownership boundaries."""
    def receipts(self):
        return [{"id": identity, "version": 1, "result": {
            "kind": "evaluated", "requested": 1, "attempts": [{
                "kind": "placed", "content": content,
                "location": {"region": 1, "position": {"x": index, "y": 2, "z": 0}},
                "entity": {"kind": "item", "id": 100 + index, "quantity": 6 if index == 2 else 1}}]}}
            for index, (identity, content) in enumerate((
                ("rogue.equipment", "rogue.generated_mace"),
                ("rogue.equipment_scrolls", "rogue.enchant_armor"),
                ("rogue.ammunition", "rogue.arrow"),
                ("rogue.rings", "rogue.ring_protection")))]

    def test_equipment_and_ammunition_stages_preserve_cross_stage_identity_checks(self):
        self.assertEqual(EQUIPMENT_ITEM_STAGE_IDS, (
            "rogue.equipment", "rogue.equipment_scrolls", "rogue.ammunition", "rogue.rings"))
        self.assertEqual(assert_equipment_item_receipts(self, self.receipts(), require_placed=True),
                         [("item", 100), ("item", 101), ("item", 102), ("item", 103)])

    def test_every_reviewed_subtype_is_eligible_only_in_its_equipment_stage(self):
        for index, choices in enumerate((
                ("rogue.generated_mace", "rogue.long_sword", "rogue.dagger", "rogue.two_handed_sword",
                 "rogue.spear", "rogue.leather_armor", "rogue.generated_ring_mail", "rogue.studded_leather",
                 "rogue.scale_mail", "rogue.chain_mail", "rogue.splint_mail", "rogue.banded_mail", "rogue.plate_mail",
                 "rogue.shortbow"),
                ("rogue.enchant_armor", "rogue.protect_armor", "rogue.enchant_weapon", "rogue.remove_curse",
                 "rogue.identify_weapon", "rogue.identify_armor"),
                ("rogue.arrow",), ('rogue.ring_protection', 'rogue.ring_add_strength', 'rogue.ring_sustain_strength', 'rogue.ring_see_invisible', 'rogue.ring_adornment', 'rogue.ring_dexterity', 'rogue.ring_increase_damage', 'rogue.ring_regeneration', 'rogue.ring_slow_digestion', 'rogue.ring_maintain_armor', 'rogue.ring_searching', 'rogue.ring_teleportation', 'rogue.ring_stealth', 'rogue.ring_aggravation'))):
            for choice in choices:
                with self.subTest(stage=index, content=choice):
                    receipts = self.receipts()
                    receipts[index]["result"]["attempts"][0]["content"] = choice
                    self.assertEqual(len(assert_equipment_item_receipts(self, receipts)), 4)
                for wrong_stage in range(4):
                    if wrong_stage == index:
                        continue
                    receipts = self.receipts()
                    receipts[wrong_stage]["result"]["attempts"][0]["content"] = choice
                    with self.subTest(stage=index, wrong_stage=wrong_stage, content=choice):
                        with self.assertRaises(AssertionError):
                            assert_equipment_item_receipts(self, receipts)

    def test_no_eligible_cell_skip_has_no_owned_item_and_cannot_satisfy_seeded_placement(self):
        receipts = self.receipts()
        for stage in receipts:
            stage["result"]["attempts"] = [{"kind": "skipped",
                "content": stage["result"]["attempts"][0]["content"], "reason": "no_eligible_cell"}]
        self.assertEqual(assert_equipment_item_receipts(self, receipts), [])
        with self.assertRaises(AssertionError):
            assert_equipment_item_receipts(self, receipts, require_placed=True)
        for mutation in ({"reason": "chance_miss"}, {"entity": {"kind": "item", "id": 100, "quantity": 1}}):
            changed = copy.deepcopy(receipts)
            changed[0]["result"]["attempts"][0].update(mutation)
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                assert_equipment_item_receipts(self, changed)

    def test_omitted_reordered_or_extra_equipment_stages_are_rejected(self):
        receipts = self.receipts()
        for changed in (receipts[:1], receipts[::-1], receipts + [receipts[0]]):
            with self.subTest(ids=[stage["id"] for stage in changed]), self.assertRaises(AssertionError):
                assert_equipment_item_receipts(self, changed)

    def test_count_quantity_identity_and_owned_location_contracts_are_strict(self):
        attempt = ("result", "attempts", 0)
        mutations = (
            (("version",), 2), (("version",), True),
            (("result", "kind"), "chance_miss"),
            (("result", "requested"), 0), (("result", "requested"), 2),
            (("result", "requested"), True), (("result", "attempts"), []),
            (("result", "attempts"), self.receipts()[0]["result"]["attempts"] * 2),
            ((*attempt, "kind"), "unknown"),
            ((*attempt, "entity", "kind"), "actor"),
            ((*attempt, "entity", "id"), 0), ((*attempt, "entity", "id"), 1 << 64),
            ((*attempt, "entity", "id"), True), ((*attempt, "entity", "id"), 101),
            ((*attempt, "entity", "quantity"), 0), ((*attempt, "entity", "quantity"), 2),
            ((*attempt, "entity", "quantity"), True),
            ((*attempt, "entity", "carried_loot"), None),
            ((*attempt, "location", "region"), 0), ((*attempt, "location", "region"), True),
            ((*attempt, "location", "position", "x"), "0"),
        )
        for path, value in mutations:
            receipts = self.receipts()
            target = receipts[0]
            for key in path[:-1]:
                target = target[key]
            target[path[-1]] = value
            with self.subTest(path=path, value=value), self.assertRaises(AssertionError):
                assert_equipment_item_receipts(self, receipts)


    def test_ammunition_quantity_is_exactly_six_and_duplicate_identity_is_rejected(self):
        for field, invalid in (("quantity", 0), ("quantity", 1), ("quantity", 5),
                               ("quantity", 7), ("quantity", True), ("id", 100)):
            receipts = self.receipts()
            receipts[2]["result"]["attempts"][0]["entity"][field] = invalid
            with self.subTest(field=field, value=invalid), self.assertRaises(AssertionError):
                assert_equipment_item_receipts(self, receipts)

    def test_generated_aliases_require_exact_archetype_and_shared_identity(self):
        for content, identity in (("rogue.generated_mace", "rogue.mace"),
                                  ("rogue.generated_ring_mail", "rogue.ring_mail"),
                                  ("rogue.long_sword", "rogue.long_sword"),
                                  ("rogue.enchant_armor", "rogue.enchant_armor"),
                                  ("rogue.healing", "rogue.healing")):
            item = {"spec": {"archetype": content, "identity": identity}}
            assert_finite_item_identity(self, item, content)
            for field in ("archetype", "identity"):
                forged = copy.deepcopy(item)
                forged["spec"][field] = "rogue.other"
                with self.subTest(content=content, field=field), self.assertRaises(AssertionError):
                    assert_finite_item_identity(self, forged, content)


class RogueControlReceiptTests(unittest.TestCase):
    def receipts(self):
        return [{"id": "rogue.control_scrolls", "version": 1,
                 "result": {"kind": "evaluated", "requested": 1, "attempts": [{
                     "kind": "placed", "content": "rogue.aggravate_monster",
                     "location": {"region": 1, "position": {"x": 1, "y": 2, "z": 0}},
                     "entity": {"kind": "item", "id": 500, "quantity": 1}}]}}]

    def check_receipts(self, receipts, **kwargs):
        helper = getattr(rogue_magic_receipts, "assert_control_item_receipts", None)
        self.assertTrue(callable(helper), "Control stock needs its own strict receipt owner")
        return helper(self, receipts, **kwargs)

    def test_all_five_controls_have_one_finite_owned_identity(self):
        for content in ("rogue.aggravate_monster", "rogue.create_monster", "rogue.hold_monster",
                        "rogue.monster_confusion", "rogue.scare_monster"):
            with self.subTest(content=content):
                receipt = self.receipts()
                receipt[0]["result"]["attempts"][0]["content"] = content
                self.assertEqual(self.check_receipts(receipt, require_placed=True), [("item", 500)])
                for owner, validator in ((RogueMagicReceiptTests(), assert_magic_item_receipts),
                                         (RogueEquipmentReceiptTests(), assert_equipment_item_receipts)):
                    foreign = owner.receipts()
                    for index in range(len(foreign)):
                        wrong = copy.deepcopy(foreign)
                        wrong[index]["result"]["attempts"][0]["content"] = content
                        with self.subTest(foreign_stage=index), self.assertRaises(AssertionError):
                            validator(self, wrong)
        self.assertEqual(rogue_magic_receipts.CONTROL_ITEM_STAGE_IDS, ("rogue.control_scrolls",))

    def test_control_rejects_forged_shape_quantity_location_and_foreign_choices(self):
        self.assertEqual(self.check_receipts(self.receipts()), [("item", 500)])
        attempt = ("result", "attempts", 0)
        mutations = (
            (("id",), "rogue.magic_scrolls"), (("version",), 2), (("version",), True),
            (("unexpected",), 0), (("result", "requested"), 0),
            (("result", "requested"), True), (("result", "attempts"), []),
            (("result", "attempts"), self.receipts()[0]["result"]["attempts"] * 2),
            ((*attempt, "content"), "rogue.sleep"), ((*attempt, "content"), "rogue.ring_stealth"),
            ((*attempt, "kind"), "unknown"), ((*attempt, "entity", "kind"), "actor"),
            ((*attempt, "entity", "id"), 0), ((*attempt, "entity", "id"), True),
            ((*attempt, "entity", "id"), 1 << 64), ((*attempt, "entity", "quantity"), 0),
            ((*attempt, "entity", "quantity"), 2), ((*attempt, "entity", "quantity"), True),
            ((*attempt, "entity", "carried_loot"), None),
            ((*attempt, "location", "region"), 0), ((*attempt, "location", "region"), True),
            ((*attempt, "location", "position", "x"), "1"),
        )
        for path, value in mutations:
            receipt = self.receipts()
            target = receipt[0]
            for key in path[:-1]:
                target = target[key]
            target[path[-1]] = value
            with self.subTest(path=path, value=value), self.assertRaises(AssertionError):
                self.check_receipts(receipt)
        for receipt in ([], self.receipts() * 2):
            with self.assertRaises(AssertionError):
                self.check_receipts(receipt)

    def test_control_skip_has_no_identity_and_is_only_no_eligible_cell(self):
        self.assertEqual(self.check_receipts(self.receipts()), [("item", 500)])
        skipped = self.receipts()
        skipped[0]["result"]["attempts"] = [{"kind": "skipped",
            "content": "rogue.scare_monster", "reason": "no_eligible_cell"}]
        self.assertEqual(self.check_receipts(skipped), [])
        with self.assertRaises(AssertionError):
            self.check_receipts(skipped, require_placed=True)
        for mutation in ({"reason": "chance_miss"},
                         {"entity": {"kind": "item", "id": 500, "quantity": 1}}):
            changed = copy.deepcopy(skipped)
            changed[0]["result"]["attempts"][0].update(mutation)
            with self.assertRaises(AssertionError):
                self.check_receipts(changed)

    def test_control_identity_is_returned_for_cross_group_duplicate_rejection(self):
        equipment = RogueEquipmentReceiptTests().receipts()
        controls = self.receipts()
        controls[0]["result"]["attempts"][0]["entity"]["id"] = 100
        ids = assert_equipment_item_receipts(self, equipment) + self.check_receipts(controls)
        self.assertNotEqual(len(ids), len(set(ids)), "Consumers must keep the combined owner list")

    def live_receipts(self):
        def item(identity, content):
            return {"kind": "placed", "content": content,
                    "location": {"region": 1, "position": {"x": 2, "y": 2, "z": 0}},
                    "entity": {"kind": "item", "id": identity, "quantity": 1}}
        actor = {"kind": "placed", "content": "rogue.leprechaun",
                 "location": {"region": 1, "position": {"x": 3, "y": 2, "z": 0}},
                 "entity": {"kind": "actor", "id": 30, "carried_loot": {
                     "kind": "carried", "content": "rogue.gold", "quantity": 111, "item": 603}}}
        return [
            {"id": "rogue.expanded_stock", "version": 1, "result": {
                "kind": "evaluated", "requested": 1, "attempts": [item(600, "rogue.haste")]}},
            {"id": "rogue.treasure_room", "version": 1, "result": {
                "kind": "room_cluster", "selection": {"kind": "selected", "room": 2,
                    "room_area": 4, "actor_depth": 6, "item_requested": 2,
                    "actor_requested": 1, "items": [item(601, "rogue.fruit"), item(602, "rogue.dart")],
                    "actors": [actor]}}},
        ]

    def check_live(self, receipts, depth=5):
        helper = getattr(rogue_magic_receipts, "assert_live_content_receipts", None)
        self.assertTrue(callable(helper), "Live stock and clusters need a strict shared receipt owner")
        return helper(self, receipts, depth)

    def test_live_stock_and_treasure_include_every_item_actor_and_carried_owner(self):
        self.assertEqual(self.check_live(self.live_receipts()),
                         [("item", 600), ("item", 601), ("item", 602), ("actor", 30), ("item", 603)])
        choices = ("haste", "gain_strength", "raise_level", "hallucination", "monster_detection",
                   "magic_detection", "food_detection", "magic_mapping", "wand_nothing", "wand_light",
                   "wand_drain_life", "wand_cancellation", "wand_magic_missile", "wand_polymorph",
                   "fruit", "dart", "shuriken")
        for choice in choices:
            value = self.live_receipts()
            value[0]["result"]["attempts"][0]["content"] = "rogue." + choice
            self.assertEqual(len(self.check_live(value)), 5)
        for choice in ("gold", "healing", "fruit", "dart", "shuriken"):
            value = self.live_receipts()
            value[1]["result"]["selection"]["items"][0]["content"] = "rogue." + choice
            self.assertEqual(len(self.check_live(value)), 5)

    def test_live_zero_miss_skips_and_area_caps_preserve_their_distinct_boundaries(self):
        for kind in ("chance_miss", "no_eligible_room"):
            value = self.live_receipts()
            value[0]["result"].update(requested=0, attempts=[])
            value[1]["result"]["selection"] = {"kind": kind}
            self.assertEqual(self.check_live(value), [])
            value[1]["result"]["selection"]["room"] = 1
            with self.assertRaises(AssertionError):
                self.check_live(value)
        value = self.live_receipts()
        selected = value[1]["result"]["selection"]
        selected.update(room_area=1, item_requested=4, actor_requested=3)
        selected["items"] = selected["items"][:1]
        for attempts in (value[0]["result"]["attempts"], selected["items"], selected["actors"]):
            attempts[:] = [{"kind": "skipped", "content": row["content"],
                            "reason": "no_eligible_cell"} for row in attempts]
        self.assertEqual(self.check_live(value), [])
        selected["actors"][0]["entity"] = {"kind": "actor", "id": 30, "carried_loot": None}
        with self.assertRaises(AssertionError):
            self.check_live(value)

    def test_live_receipts_reject_wrong_shape_depth_counts_choices_and_duplicate_ownership(self):
        self.check_live(self.live_receipts())
        stock = (0, "result")
        selected = (1, "result", "selection")
        item = (*selected, "items", 0)
        actor = (*selected, "actors", 0)
        mutations = (
            ((0, "id"), "rogue.magic_potions"), ((1, "version"), True),
            ((0, "unexpected"), 1), ((*stock, "requested"), True),
            ((*stock, "requested"), 2), ((*stock, "attempts"), []),
            ((*stock, "attempts", 0, "content"), "rogue.torch"),
            ((*selected, "kind"), "unknown"), ((*selected, "actor_depth"), 5),
            ((*selected, "actor_depth"), True), ((*selected, "room"), 0),
            ((*selected, "room"), 65), ((*selected, "room_area"), 0),
            ((*selected, "room_area"), 1025), ((*selected, "room_area"), True),
            ((*selected, "item_requested"), 1), ((*selected, "item_requested"), 5),
            ((*selected, "actor_requested"), 0), ((*selected, "actor_requested"), 4),
            ((*selected, "actors"), []), ((*selected, "items"), []),
            ((*item, "content"), "rogue.haste"), ((*item, "entity", "quantity"), 2),
            ((*item, "entity", "id"), 600), ((*item, "entity", "id"), True),
            ((*item, "location", "region"), True), ((*item, "location", "position", "x"), "2"),
            ((*actor, "content"), "rogue.dragon"), ((*actor, "entity", "kind"), "item"),
            ((*actor, "entity", "id"), 0), ((*actor, "entity", "carried_loot"), None),
            ((*actor, "entity", "carried_loot", "kind"), "not_carried"),
            ((*actor, "entity", "carried_loot", "quantity"), 112),
            ((*actor, "entity", "carried_loot", "quantity"), True),
            ((*actor, "entity", "carried_loot", "content"), "rogue.ration"),
            ((*actor, "entity", "carried_loot", "item"), 601),
        )
        for path, invalid in mutations:
            value = self.live_receipts()
            target = value
            for key in path[:-1]:
                target = target[key]
            target[path[-1]] = invalid
            with self.subTest(path=path, invalid=invalid), self.assertRaises(AssertionError):
                self.check_live(value)
        for value in ([], self.live_receipts()[::-1], self.live_receipts() * 2):
            with self.assertRaises(AssertionError):
                self.check_live(value)

    def test_treasure_birth_stock_checks_next_depth_eligibility_and_mandatory_carriers(self):
        value = self.live_receipts()
        actor = value[1]["result"]["selection"]["actors"][0]
        actor.update(content="rogue.kestrel")
        actor["entity"]["carried_loot"] = None
        self.assertEqual(len(self.check_live(value)), 4)
        actor["entity"]["carried_loot"] = {"kind": "not_carried"}
        with self.assertRaises(AssertionError):
            self.check_live(value)
        actor["content"] = "rogue.centaur"
        value[1]["result"]["selection"]["actor_depth"] = 7
        self.assertEqual(len(self.check_live(value, depth=6)), 4)
        actor["entity"]["carried_loot"] = {"kind": "carried", "content": "rogue.ration",
                                           "quantity": 1, "item": 603}
        self.assertEqual(len(self.check_live(value, depth=6)), 5)
        actor["content"] = "rogue.dragon"
        value[1]["result"]["selection"]["actor_depth"] = 32
        self.assertEqual(len(self.check_live(value, depth=31)), 5)
        actor["entity"]["carried_loot"] = {"kind": "not_carried"}
        with self.assertRaises(AssertionError):
            self.check_live(value, depth=31)


if __name__ == "__main__":
    unittest.main()
