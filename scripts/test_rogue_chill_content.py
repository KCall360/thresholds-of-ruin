"""New reconstruction: retained CH1 content uses the shared status lifecycle."""
import unittest
from test_rogue_roster_content import source

ICE = "rogue.ice_monster"
CHILL = "rogue.freezing_touch"


class RogueChillContent(unittest.TestCase):
    def test_ice_has_checked_zero_damage_contact_and_one_purchased_signature(self):
        actors = source("catalog/actors.toml")["actors"]
        self.assertIn(ICE, actors, "Retained ice monster recipe is missing")
        build = actors[ICE]["creature"]
        self.assertEqual(build["hit_dice"], [{"source": "racial", "purchases": [{
            "pool": "advancement", "selection": {"kind": "talent", "talent": CHILL}}]}])
        species = source("catalog/creatures.toml")["creatures"]["species"][ICE]
        self.assertEqual(species["subtypes"], ["cold"])
        self.assertEqual(len(species["melee"]["strikes"]), 1)
        contact = species["melee"]["strikes"][0]
        self.assertIn("check", contact)
        self.assertEqual(contact["on_success"], [])
        self.assertEqual(contact["on_failure"], [])
        self.assertEqual(contact["riders"], [])

    def test_chill_is_checked_contact_then_checked_rider_with_200_400_no_refresh_policy(self):
        overrides = {tuple(row["path"]): row["value"] for row in source("rules.toml")["overrides"]}
        self.assertIn(("abilities", CHILL), overrides, "Retained CH1 signature is missing")
        ability = overrides[("abilities", CHILL)]
        self.assertFalse(ability["basic_attack"])
        self.assertEqual(ability.get("interaction"), "hostile_to_target")
        self.assertEqual(ability["strikes"]["kind"], "natural_melee",
                         "CH1 must reuse its own-species checked contact instead of duplicating it")
        self.assertNotIn("strikes", ability["strikes"])
        rider, = ability["strikes"]["additional_riders"]
        self.assertEqual(rider["trigger"], "success")
        self.assertEqual(rider["check"]["defense"], "physical")
        self.assertEqual(rider["effects"], [{"kind": "apply_condition", "condition": "rogue.frozen", "duration": 200}])
        condition = overrides[("conditions", "rogue.frozen")]
        self.assertEqual(condition["stacking"], {"kind": "keep"})
        self.assertFalse(condition["source_scoped"])
        self.assertEqual(condition["timing"], {"kind": "actor_active", "on_expiry": {"kind": "protect", "duration": 400}})
        self.assertEqual(condition["restrictions"], ["voluntary_actions"])
        self.assertEqual(condition["preparation"], "pause")
        self.assertEqual(condition["modifiers"], [])


if __name__ == "__main__":
    unittest.main()
