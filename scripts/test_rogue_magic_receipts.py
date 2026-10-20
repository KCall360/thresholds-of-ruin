"""Pure finite-stock receipt contracts run in every tooling tier, without apps.

Stage identities and eligible subtypes below are independent reviewed expectations,
not generated from production content or the receipt validator's own tables.
"""
import copy
import unittest

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
                ("rogue.healing", "rogue.confusion", "rogue.blindness", "rogue.poison", "rogue.see_invisible", "rogue.extra_healing"),
                ("rogue.identify_potion", "rogue.identify_scroll", "rogue.identify_wand", "rogue.sleep"),
                ("rogue.wand_fire", "rogue.wand_cold", "rogue.wand_lightning", "rogue.wand_invisibility", "rogue.wand_haste", "rogue.wand_slow"))):
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
                ("rogue.arrow",), ('rogue.ring_protection', 'rogue.ring_add_strength', 'rogue.ring_sustain_strength', 'rogue.ring_see_invisible', 'rogue.ring_adornment', 'rogue.ring_dexterity', 'rogue.ring_increase_damage', 'rogue.ring_regeneration', 'rogue.ring_slow_digestion', 'rogue.ring_maintain_armor', 'rogue.ring_searching'))):
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


if __name__ == "__main__":
    unittest.main()
