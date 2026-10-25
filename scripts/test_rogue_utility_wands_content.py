"""Exact persistent utility wand definitions and finite stock bindings."""
import unittest

from test_rogue_utility_potions_content import source


class RogueUtilityWandsContent(unittest.TestCase):
    def test_utility_devices_are_unknown_charged_contact_rays_without_extra_costs(self):
        catalog = source("catalog/items.toml")
        overrides = {tuple(row["path"]): row["value"] for row in source("rules.toml")["overrides"]}
        for kind, name in (("invisibility", "invisibility"), ("haste", "haste monster"), ("slow", "slow monster")):
            with self.subTest(kind=kind):
                identity = "rogue.wand_" + kind
                self.assertIn(identity, set(catalog["items"]))
                self.assertEqual(catalog["items"][identity], {
                    "asset": "item.wand", "name": "wand of " + name, "identity": identity,
                    "magical": True, "class": "wand", "stackable": False, "appearance_pool": "rogue.wands",
                    "device": {"ability": identity, "initial_charges": {"minimum": 3, "maximum": 7},
                               "discovery": "never"}})
                ability = overrides[("abilities", identity)]
                self.assertEqual(ability["interaction"], "non_hostile" if kind == "invisibility" else "hostile_to_target")
                self.assertEqual(ability["tags"], [] if kind == "invisibility" else ["attack"])
                self.assertEqual(ability["invocation"], "directional_contact_ray")
                self.assertEqual(ability["reach"], {"kind": "sight", "range": 6})
                self.assertEqual(ability["timing"], {"source": {"kind": "fixed", "prepare": 100, "recover": 100}})
                for absent in ("cost", "cooldown", "supply_costs"):
                    self.assertNotIn(absent, ability)
                self.assertEqual(ability["strikes"]["kind"], "authored")
                strike, = ability["strikes"]["strikes"]
                self.assertEqual(strike["check"], {"actor": "target", "skill": {"kind": "named", "skill": "agility"},
                    "against": {"kind": "opposed", "skill": {"kind": "named", "skill": "lore"}}})
                status = {"invisibility": "rogue.invisible", "haste": "rogue.hasted", "slow": "rogue.slowed"}
                effect = {"kind": "apply_persistent_condition", "condition": status[kind]}
                if kind != "invisibility":
                    effect["cancels"] = status["slow" if kind == "haste" else "haste"]
                self.assertEqual(strike["on_failure"], [effect])
                self.assertEqual(strike["on_success"], [])
                self.assertEqual(strike["riders"], [])

    def test_persistent_conditions_keep_one_source_and_use_separate_action_duration(self):
        overrides = {tuple(row["path"]): row["value"] for row in source("rules.toml")["overrides"]}
        for kind, name in (("invisibility", "Invisible"), ("haste", "Hasted"), ("slow", "Slowed")):
            identity = {"invisibility": "rogue.invisible", "haste": "rogue.hasted", "slow": "rogue.slowed"}[kind]
            op = ({"kind": "perception", "value": {"kind": "invisible", "enabled": True}}
                  if kind == "invisibility" else {"kind": "action_duration", "value": kind})
            self.assertIn(("conditions", identity), overrides)
            self.assertEqual(overrides[("conditions", identity)], {
                "id": identity, "family": identity, "lifecycle": "timed", "name": name,
                "modifiers": [{"scope": "always", "op": op}], "stacking": {"kind": "keep"},
                "source_scoped": False, "descriptors": [], "capacity": "reject",
                "timing": {"kind": "until_removed"}, "restrictions": [], "preparation": "preserve"})

    def test_appearance_and_stock_pools_keep_distinctness_and_one_device_attempt(self):
        catalog = source("catalog/items.toml")
        identities = {identity for identity, item in catalog["items"].items()
                      if item.get("appearance_pool") == "rogue.wands"}
        self.assertEqual(identities, {"rogue.wand_" + kind for kind in
                                     ("fire", "cold", "lightning", "invisibility", "haste", "slow", "teleport_away", "teleport_to", "nothing", "light")})
        appearances = catalog["appearance_pools"]["rogue.wands"]["appearances"]
        self.assertEqual(len(appearances), len(set(appearances)))
        self.assertEqual(len(appearances), len(identities))
        manifest = source("scenario.toml")
        stage = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][7]
        self.assertEqual(stage, {"id": "rogue.magic_devices", "version": 1, "kind": "items",
            "count": [1, 1], "quantity": 1, "bands": [{"depth": {"kind": "open", "minimum": 1},
                "choices": [{"archetype": "rogue.wand_" + kind, "weight": 1}
                            for kind in ("fire", "cold", "lightning", "invisibility", "haste", "slow", "teleport_away", "teleport_to")]}]})
        for character in manifest["characters"]:
            self.assertTrue(identities.isdisjoint(character["known_identities"]))
            self.assertTrue(identities.isdisjoint(row["archetype"] for row in character["loadout"]))

    def test_service_and_process_fixture_change_only_explicit_setup_not_device_definitions(self):
        from utility_wands_package import fixture
        data = fixture()
        manifest = data["manifest"]
        for identity, item in source("catalog/items.toml")["items"].items():
            self.assertEqual(manifest["archetypes"][identity], item)
        self.assertEqual([actor["at"] for actor in data["regions"][0]["actors"]], [[3, 2, 0], [4, 2, 0]])
        self.assertTrue(all(actor["controller"] == "external" for actor in data["regions"][0]["actors"]))
        production = source("catalog/creatures.toml")["creatures"]["species"]
        human = production["rogue.human"]
        accuracy = {"scope": "always", "op": {"kind": "add",
            "stat": {"kind": "attribute", "attribute": "intellect"}, "value": 100}}
        synthetic = {**human, "modifiers": [*human.get("modifiers", []), accuracy]}
        self.assertEqual(manifest["creatures"]["species"], {**production, "fixture.utility_caster": synthetic})
        self.assertEqual(manifest["creatures"]["species"]["fixture.utility_caster"]["attributes"], human["attributes"])
        target = source("scenario.toml")["characters"][0]["creature"]
        target.update(faction="delvers", nutrition={"kind": "exempt"})
        for actor, name in zip(data["regions"][0]["actors"], ("Utility target", "Rear target")):
            self.assertEqual(actor["creature"], {**target, "name": name})
        self.assertFalse(data["wizard"])


if __name__ == "__main__":
    unittest.main()
