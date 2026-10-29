"""Species value tuning edits only numbers and keeps catalog comments."""
import tomllib
import unittest

import rogue_species_tuning as tuning

CATALOG = '''# kept comment
[creatures.species."rogue.toy"]
kind = "animal"
modifiers = [{ scope = "always", op = { kind = "flag", flag = "natural_sequence" } }]

[creatures.species."rogue.toy".melee]
skill = "light_weaponry"
bonus = 0

[[creatures.species."rogue.toy".melee.strikes]]
on_success = [{ kind = "damage", damage = { kind = "components", components = [{ category = "impact", amount = { kind = "rolled", dice = { count = 1, sides = 4, bonus = 0 } } }] } }]

[[creatures.species."rogue.toy".melee.strikes]]
on_success = [{ kind = "damage", damage = { kind = "components", components = [{ category = "impact", amount = { kind = "rolled", dice = { count = 2, sides = 6, bonus = 0 } } }] } }]

[creatures.species."rogue.other"]
kind = "animal"
'''


def comparison(**ratios):
    metrics = ["player_hit", "foe_hit", "player_damage_per_action", "foe_damage_per_action",
               "player_actions_to_kill", "foe_actions_to_kill"]
    rows = {metric: {"target": 1.0, "measured": None, "ratio": None} for metric in metrics}
    rows["player_actions_to_kill"]["target"] = 4.5
    for metric, ratio in ratios.items():
        rows[metric]["ratio"] = ratio
    return rows


class SpeciesTuning(unittest.TestCase):
    def setUp(self):
        self.catalog = tomllib.loads(CATALOG)

    def study(self, **ratios):
        return {"species": [{"glyph": "T", "species": "toy", "outcomes": ["delvers", "delvers"],
                             "target_foe_hp": 4.5, "counts": {"foe_health": 16, "player_checks": 40, "foe_checks": 40}, "comparison": comparison(**ratios)}]}

    def test_proposals_move_health_to_target_and_step_other_values_by_one(self):
        [change] = tuning.proposals(self.study(player_hit=1.6, foe_hit=0.5,
                                               foe_damage_per_action=0.25), self.catalog)
        # Mean measured Health 8 against Rogue's 4.5: the modifier moves the gap.
        self.assertEqual(change["health"], -4)
        # Far outside the band (beyond x1.4) values move two points.
        self.assertEqual(change["defense"], 2)
        self.assertEqual(change["melee_bonus"], 2)
        # Damage always moves one point: dice bonuses overshoot easily.
        self.assertEqual(change["dice_bonus"], 1)
        [near] = tuning.proposals(self.study(player_hit=1.2, foe_hit=0.8,
                                             foe_damage_per_action=0.8), self.catalog)
        self.assertEqual((near["defense"], near["melee_bonus"], near["dice_bonus"]), (1, 1, 0))

    def test_ratios_inside_the_band_keep_values(self):
        [change] = tuning.proposals(self.study(player_hit=1.0, foe_hit=1.05,
                                               foe_damage_per_action=1.0), self.catalog)
        self.assertEqual((change["defense"], change["melee_bonus"], change["dice_bonus"]), (0, 0, 0))

    def test_apply_edits_values_in_place(self):
        changes = [{"species": "rogue.toy", "health": -3, "defense": 2, "melee_bonus": 1,
                    "dice_bonus": 1}]
        text = tuning.apply(changes, CATALOG, self.catalog)
        self.assertTrue(text.startswith("# kept comment\n"))
        toy = tomllib.loads(text)["creatures"]["species"]["rogue.toy"]
        self.assertEqual(toy["modifiers"][0]["op"]["flag"], "natural_sequence")
        self.assertEqual(tuning.modifier_value(toy["modifiers"], tuning.HEALTH), -3)
        self.assertEqual(tuning.modifier_value(toy["modifiers"], tuning.DEFENSE), 2)
        self.assertEqual(toy["melee"]["bonus"], 1)
        self.assertEqual(tuning.current(tomllib.loads(text), "rogue.toy")["dice_bonus"], 1)
        self.assertNotIn("modifiers", tomllib.loads(text)["creatures"]["species"]["rogue.other"])

    def test_inline_melee_tables_are_tuned_in_place(self):
        source = CATALOG + '''
[creatures.species."rogue.flat"]
kind = "animal"
melee = { skill = "light_weaponry", bonus = 0, wind_up = 100, strikes = [{ on_success = [{ kind = "damage", damage = { kind = "components", components = [{ category = "keen", amount = { kind = "rolled", dice = { count = 1, sides = 4, bonus = 0 } } }] } }] }] }
'''
        text = tuning.apply([{"species": "rogue.flat", "health": -2, "melee_bonus": 2,
                              "dice_bonus": 1}], source, tomllib.loads(source))
        flat = tomllib.loads(text)["creatures"]["species"]["rogue.flat"]
        self.assertEqual(flat["melee"]["bonus"], 2)
        self.assertEqual(tuning.current(tomllib.loads(text), "rogue.flat")["dice_bonus"], 1)
        self.assertEqual(tuning.modifier_value(flat["modifiers"], tuning.HEALTH), -2)

    def test_rejected_births_back_off_health(self):
        study = self.study(player_hit=1.0)
        study["species"][0]["outcomes"].append("failure: Invalid actor creature build")
        [change] = tuning.proposals(study, self.catalog)
        self.assertEqual(change["health"], 2)

    def test_natural_attack_accuracy_uses_an_attack_attribute_modifier(self):
        source = CATALOG.replace('skill = "light_weaponry"\nbonus = 0\n',
                                 'skill = "light_weaponry"\nbonus = 3\n').replace(
            '[[creatures.species."rogue.toy".melee.strikes]]\non_success',
            '[[creatures.species."rogue.toy".melee.strikes]]\n'
            'check = { actor = "user", skill = { kind = "natural_attack" }, against = { kind = "defense", defense = "physical" } }\n'
            'on_success', 1)
        catalog = tomllib.loads(source)
        self.assertTrue(tuning.natural_accuracy(catalog["creatures"]["species"]["rogue.toy"]))
        [change] = tuning.proposals(self.study(foe_hit=0.5), catalog)
        self.assertEqual((change["accuracy"], change["melee_bonus"]), (2, 0))
        text = tuning.apply([change], source, catalog)
        toy = tomllib.loads(text)["creatures"]["species"]["rogue.toy"]
        [boost] = [row["op"] for row in toy["modifiers"]
                   if row["op"]["kind"] == "attribute_without_defense"]
        self.assertEqual(boost, {"kind": "attribute_without_defense", "attribute": "speed",
                                 "defense": "physical", "value": 2})
        self.assertEqual(toy["melee"]["bonus"], 0)

    def test_zero_values_remove_the_modifier(self):
        text = tuning.apply([{"species": "rogue.toy", "health": 0, "defense": 0}], CATALOG,
                            self.catalog)
        toy = tomllib.loads(text)["creatures"]["species"]["rogue.toy"]
        self.assertEqual(len(toy["modifiers"]), 1)


if __name__ == "__main__":
    unittest.main()
