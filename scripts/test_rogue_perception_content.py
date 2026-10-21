"""Approved M/P/X source contracts; runtime behavior has independent owners.

These authoring tests are source-only. Catalog presence does not prove ordinary
interaction disclosure, save/replay, gaze checks, ambush behavior or balance.
"""
from fractions import Fraction
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue-exploration"


def source(name):
    return tomllib.loads((PACKAGE / name).read_text(encoding="utf-8"))


def owned_rows(count, purchases):
    return [{"source": "racial", "purchases": [{"pool": "advancement", "selection": {
        "kind": "talent", "talent": purchases[ordinal]}}] if ordinal in purchases else []}
        for ordinal in range(1, count + 1)]


class RoguePerceptionContent(unittest.TestCase):
    def test_exact_racial_rows_type_subtype_natural_dice_and_disposition(self):
        actors = source("catalog/actors.toml")["actors"]
        species = source("catalog/creatures.toml")["creatures"]["species"]
        for name, kind, subtype, hd, purchases, disposition, dice in (
                ("medusa", "monstrous_humanoid", "reptilian", 8,
                 {1: "rogue.confusing_gaze", 2: "second_strike", 4: "third_strike"},
                 "mean", [(3, 4), (3, 4), (2, 5)]),
                ("phantom", "undead", "incorporeal", 8, {}, "passive", [(4, 4)]),
                ("xeroc", "aberration", "shapechanger", 7,
                 {1: "rogue.object_guise", 6: "rogue.ambush_from_guise"}, "passive", [(4, 4)])):
            with self.subTest(species=name):
                identity = "rogue." + name
                build, definition = actors[identity]["creature"], species[identity]
                self.assertEqual(build["species"], identity)
                self.assertEqual(build["hit_dice"], owned_rows(hd, purchases))
                self.assertEqual(build["restoration"], "authored")
                self.assertEqual(build["nutrition"], {"kind": "exempt"})
                self.assertEqual(build["future_growth"], {"kind": "disabled"})
                self.assertEqual(definition["kind"], kind)
                self.assertEqual(definition["subtypes"], [subtype])
                self.assertEqual(definition["behavior"]["disposition"], disposition)
                self.assertEqual(definition["behavior"]["idle"], "wait" if name == "xeroc" else "explore")
                strikes = definition["melee"]["strikes"]
                self.assertEqual(len(strikes), len(dice))
                for index, (strike, (count, sides)) in enumerate(zip(strikes, dice, strict=True)):
                    self.assertEqual(strike["check"], {"actor": "user", "skill": {
                        "kind": "natural_attack"}, "against": {"kind": "defense", "defense": "physical"}})
                    self.assertEqual(strike.get("unlock"), (None, "second_strike", "third_strike")[index])
                    self.assertEqual(strike["on_success"][0]["damage"]["components"][0]["amount"],
                                     {"kind": "rolled", "dice": {"count": count, "sides": sides, "bonus": 0}})
                    self.assertEqual(strike["on_failure"], [])
                    self.assertEqual(strike["riders"], [])
                self.assertNotIn("loadout", actors[identity], "Birth stock belongs to population choices")

    def test_medusa_binds_existing_spellcasting_to_presence_and_buys_gaze(self):
        build = source("catalog/actors.toml")["actors"]["rogue.medusa"]["creature"]
        self.assertEqual(build["binding"], "presence")
        self.assertIn("spellcasting", build["starting_skills"])
        ops = [row["op"] for row in source("catalog/creatures.toml")["creatures"]["species"]["rogue.medusa"]["modifiers"]]
        self.assertEqual({op["flag"] for op in ops if op["kind"] == "flag"},
                         {"gaze_capable", "natural_sequence"})
        self.assertFalse(any(op["kind"] == "grant_ability" for op in ops), "GZ1 must be purchased")

    def test_phantom_uses_typed_invisibility_and_existing_one_in_five_erratic(self):
        phantom = source("catalog/creatures.toml")["creatures"]["species"]["rogue.phantom"]
        self.assertEqual(phantom["modifiers"], [
            {"scope": "always", "op": {"kind": "perception", "value": {"kind": "invisible", "enabled": True}}},
            {"scope": "always", "op": {"kind": "locomotion", "value": {"kind": "erratic",
                "chance": {"numerator": 1, "denominator": 5}}}}])
        # Declaring the subtype is not permission for immunity or wall passing.
        rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text(encoding="utf-8"))
        for identity in ("incorporeal", "shapechanger"):
            subtype = next(row for row in rules["subtypes"] if row["id"] == identity)
            self.assertTrue(subtype["mechanics_deferred"])
            self.assertEqual(subtype["modifiers"], [])

    def test_mimic_talents_own_one_mundane_guise_and_advantage_capability(self):
        overrides = {tuple(row["path"]): row["value"] for row in source("rules.toml")["overrides"]}
        guise = overrides[("talents", "rogue.object_guise")]
        ambush = overrides[("talents", "rogue.ambush_from_guise")]
        for talent, hd in ((guise, 1), (ambush, 6)):
            self.assertEqual(talent["requirement"], {"kind": "all", "requirements": [
                {"kind": "total_hd", "minimum": hd}, {"kind": "subtype_in", "subtypes": ["shapechanger"]}]})
            self.assertEqual(talent["form_choices"], [])
        self.assertEqual(guise["prerequisites"], [])
        self.assertEqual(ambush["prerequisites"], ["rogue.object_guise"])
        mace = source("catalog/items.toml")["items"]["rogue.mace"]
        self.assertFalse(mace.get("magical", False))
        self.assertEqual(guise["modifiers"], [{"scope": "always", "op": {"kind": "perception", "value": {
            "kind": "object_guise", "skill": "stealth", "edge": 0, "appearance": {"name": mace["name"], "description": "",
                "class": mace["class"], "asset": mace["asset"]}}}},
            {"scope": "always", "op": {"kind": "perception", "value": {
                "kind": "readied_touch_ambush", "enabled": True}}}])
        self.assertEqual(ambush["modifiers"], [{"scope": "always", "op": {"kind": "perception", "value": {
            "kind": "ambush_from_guise", "enabled": True}}}])
        xeroc = source("catalog/creatures.toml")["creatures"]["species"]["rogue.xeroc"]
        self.assertEqual(xeroc.get("modifiers", []), [], "Guise/ambush must come from exact paid HD rows")
        self.assertEqual(xeroc["behavior"]["idle"], "wait")
        self.assertFalse(any(xeroc["behavior"][name] for name in ("doors", "care", "equipment", "loot")))

    def test_gaze_confusion_and_search_keep_the_approved_shared_contract(self):
        overrides = {tuple(row["path"]): row["value"] for row in source("rules.toml")["overrides"]}
        confused = overrides[("conditions", "rogue.confused")]
        self.assertEqual(confused["descriptors"], ["mind_affecting"])
        self.assertEqual(confused["timing"], {"kind": "actor_active", "on_expiry": {"kind": "protect", "duration": 400}})
        self.assertEqual(confused["stacking"], {"kind": "keep"})
        self.assertEqual(confused["restrictions"], [])
        self.assertEqual(confused["preparation"], "preserve")
        gaze = overrides[("abilities", "rogue.confusing_gaze")]
        self.assertEqual(gaze["cooldown"], 700)
        self.assertEqual(gaze["reach"], {"kind": "mutual_sight", "range": 4})
        self.assertEqual(gaze["timing"]["source"], {"kind": "fixed", "prepare": 100, "recover": 100})
        self.assertEqual(gaze["strikes"]["strikes"][0]["on_success"], [{"kind": "apply_condition", "condition": "rogue.confused", "duration": 300}])
        search = overrides[("abilities", "rogue.search")]
        self.assertEqual(search["timing"]["source"], {"kind": "fixed", "prepare": 50, "recover": 50})
        self.assertEqual(search["reach"], {"kind": "self_only"})
        self.assertIn("rogue.search", overrides[("constants", "default_abilities")])

    def test_assets_are_authored_and_guise_uses_existing_item_asset(self):
        actors = source("catalog/actors.toml")["actors"]
        assets = source("scenario.toml")["assets"]["dungeon"]
        for name, expected in (("medusa", "creature.medusa"), ("phantom", "creature.undead.phantom"),
                               ("xeroc", "creature.xeroc")):
            asset = actors["rogue." + name]["asset"]
            self.assertEqual(asset, expected)
            self.assertLessEqual(len(asset), 80)
            self.assertRegex(asset, r"\A[a-z0-9-]+(?:\.[a-z0-9-]+)*\Z")
            self.assertIn(asset, assets)
        self.assertIn("item.food.ration", assets)

    def test_population_has_exact_m_p_x_boundaries_and_finite_birth_chances(self):
        manifest = source("scenario.toml")
        activation = manifest["floor_activation"]
        stages = activation["profiles"]["rogue.low_depth"]["stages"]
        self.assertEqual([row["id"] for row in stages], ["rogue.eight_species_foes", "rogue.bandages",
                         "rogue.rations", "rogue.restore_strength", "rogue.room_gold", "rogue.magic_potions", "rogue.magic_scrolls",
            "rogue.magic_devices", "rogue.equipment", "rogue.equipment_scrolls", "rogue.ammunition", "rogue.rings", "rogue.control_scrolls"])
        self.assertEqual(stages[0]["count"], [4, 4])
        self.assertEqual(stages[0]["depth_growth"], {"kind": "racial_after", "depth": 26})
        bands = stages[0]["bands"]
        self.assertEqual(len({row["archetype"] for band in bands for row in band["choices"]}), 26)
        for name, first, last, chance in (("medusa", 18, None, (2, 5)),
                                         ("phantom", 15, 24, None), ("xeroc", 16, 25, (3, 10))):
            for depth in (*range(1, 36), 267, 268, 275, 1_000_000):
                with self.subTest(species=name, depth=depth):
                    matches = [row for row in bands if row["depth"]["minimum"] <= depth and
                               (row["depth"]["kind"] == "open" or depth <= row["depth"]["maximum"])]
                    self.assertEqual(len(matches), 1)
                    choices = {row["archetype"]: row for row in matches[0]["choices"]}
                    self.assertEqual("rogue." + name in choices, first <= depth and (last is None or depth <= last))
                    if not (first <= depth and (last is None or depth <= last)):
                        continue
                    stock = choices["rogue." + name].get("carried_loot")
                    self.assertEqual(stock, {"version": 1, "pool": "rogue.vampire_supplies", "chance": {
                        "numerator": chance[0], "denominator": chance[1]}} if chance else None)
        # At depth 27 M shares the upper-boundary mass of V/G/J/D equally.
        band = next(row for row in bands if row["depth"]["minimum"] == 27)
        weights = {row["archetype"]: row["weight"] for row in band["choices"]}
        total = sum(weights.values())
        self.assertEqual({key: Fraction(value, total) for key, value in weights.items()}, {
            "rogue.medusa": Fraction(1, 5), "rogue.vampire": Fraction(1, 5),
            "rogue.griffin": Fraction(1, 5), "rogue.jabberwock": Fraction(1, 5),
            "rogue.dragon": Fraction(1, 5)})


    def test_medusa_retains_upper_clamp_mass_and_exact_shared_growth_boundary(self):
        bands = source("scenario.toml")["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]["bands"]
        for depth, expected in ((27, (1, 1, 1, 1, 1)), (28, (6, 11, 11, 11, 11)),
                                (29, (7, 7, 12, 12, 12)), (30, (8, 8, 8, 13, 13)),
                                (31, (9, 9, 9, 9, 14)), (32, (1, 1, 1, 1, 1)),
                                (274, (1, 1, 1, 1, 1)), (275, (1, 1, 1, 1, 1))):
            band = next(row for row in bands if row["depth"]["minimum"] <= depth and
                        (row["depth"]["kind"] == "open" or depth <= row["depth"]["maximum"]))
            self.assertEqual({row["archetype"]: row["weight"] for row in band["choices"]},
                             dict(zip(("rogue.medusa", "rogue.vampire", "rogue.griffin", "rogue.jabberwock", "rogue.dragon"),
                                      expected, strict=True)))
        # The shared racial_after policy appends actual rows; no medusa cap/filter.
        build = source("catalog/actors.toml")["actors"]["rogue.medusa"]["creature"]
        self.assertEqual(len(build["hit_dice"]) + 274 - 26, 256)
        self.assertEqual(len(build["hit_dice"]) + 275 - 26, 257)


if __name__ == "__main__":
    unittest.main()
