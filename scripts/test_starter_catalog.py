"""The authored playable slice keeps its exact gates, actions and paid forms."""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
TALENTS = {
    "brace_for_impact", "measured_cut", "rattling_blow", "guarded_step",
    "read_the_rhythm", "dislocating_strike", "open_the_guard",
    "evocation_initiate", "binding_initiate", "warding_initiate",
    "steady_recovery", "hardy_recovery",
}
FORMS = {
    "evocation_initiate": ["spark_lance_fire", "spark_lance_cold"],
    "binding_initiate": ["falter"],
    "warding_initiate": ["shell_impact", "shell_keen", "shell_energy"],
}
LEGACY = {
    "hardiness", "toughness", "unyielding", "indomitable", "endurance",
    "deep_endurance", "tireless", "guard", "greater_guard", "iron_guard",
    "heavy_blows", "mighty_blows", "crushing_blows", "perfected_blows",
    "impact_ward", "keen_ward", "energy_ward", "resolve", "power_strike",
    "magic_bolt", "fear", "arcane_reserve", "potent_bolt", "empowered_bolt",
    "greater_bolt", "master_bolt", "fear_mastery",
}


def selected(value):
    if isinstance(value, dict):
        if value.get("kind") in ("talent", "talent_form"):
            yield value
        for child in value.values():
            yield from selected(child)
    elif isinstance(value, list):
        for child in value:
            yield from selected(child)


class StarterCatalogTests(unittest.TestCase):
    def setUp(self):
        self.rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text())
        self.talents = {row["id"]: row for row in self.rules["talents"]}
        self.forms = {row["id"]: row for row in self.rules["forms"]}
        self.abilities = {row["id"]: row for row in self.rules["abilities"]}
        self.conditions = {row["id"]: row for row in self.rules["conditions"]}

    def action(self, name):
        self.assertIn(name, self.abilities)
        return self.abilities[name]

    def strike(self, name):
        strikes = self.action(name)["strikes"]
        self.assertEqual(strikes["kind"], "authored")
        self.assertEqual(len(strikes["strikes"]), 1)
        return strikes["strikes"][0]

    def assert_cost(self, name, resource, total):
        self.assertEqual(self.action(name)["cost"],
                         {"resource": resource, "start": 1, "resolution": total - 1})

    def assert_fixed_timing(self, name):
        self.assertEqual(self.action(name)["timing"], {
            "source": {"kind": "fixed", "prepare": 100, "recover": 100}, "scaled_by": "speed"})

    def test_playable_catalog_retires_every_legacy_definition_without_runtime_aliases(self):
        self.assertEqual(set(self.talents), TALENTS)
        self.assertEqual(len(self.rules["talents"]), 12)
        self.assertEqual(len(self.rules["forms"]), 6)
        self.assertEqual(len(self.rules["abilities"]), 15)
        self.assertTrue(LEGACY.isdisjoint(self.talents))
        self.assertTrue({"power_strike", "magic_bolt", "fear"}.isdisjoint(self.abilities))
        self.assertIn("fear", self.conditions)
        self.assertTrue({"fear", "mind_affecting"}.issubset(
            {row["id"] for row in self.rules["descriptors"]}))

    def test_three_independent_mage_roots_own_only_the_explicit_acquired_form(self):
        for root, forms in FORMS.items():
            self.assertIn(root, self.talents)
            self.assertEqual(self.talents[root]["form_choices"], forms)
            self.assertEqual(self.talents[root]["modifiers"], [])
            self.assertEqual(self.talents[root]["prerequisites"], [])
            self.assertEqual(self.talents[root]["requirement"], {"kind": "all", "requirements": [
                {"kind": "total_hd", "minimum": 1},
                {"kind": "class_level", "class": "mage", "minimum": 1},
                {"kind": "skill_rank", "skill": {"kind": "named", "skill": "spellcasting"}, "minimum": 1},
            ]})
            for form in forms:
                self.assertEqual(self.forms[form]["ability"], form)

    def test_weapon_actions_use_normal_damage_and_only_the_approved_pressure_riders(self):
        damage = {"kind": "damage", "damage": {"kind": "selected_weapon", "impact_bonus": 0}}
        for action, condition in [("measured_cut", None), ("rattling_blow", "rattled"),
                                  ("open_the_guard", "exposed"), ("dislocating_strike", None)]:
            expected = [damage]
            if condition:
                expected.append({"kind": "apply_condition", "condition": condition, "duration": 200})
            self.assertEqual(self.strike(action)["on_success"], expected)
            self.assert_cost(action, "stamina", 3 if action in ("open_the_guard", "dislocating_strike") else 2)
            self.assertEqual(self.action(action)["reach"], {"kind": "melee"})
            self.assertIn("attack", self.action(action)["tags"])
        self.assertEqual(self.action("measured_cut")["outgoing_edge_policy"], {"kind": "ignore_negative"})
        self.assertEqual(self.strike("dislocating_strike")["riders"], [{
            "trigger": "success", "descriptors": [], "effects": [
                {"kind": "push_target_one_cell", "direction": "away_from_source"}]}])
        self.assertEqual(self.action("dislocating_strike")["timing"], {
            "source": {"kind": "selected_weapon_adjusted", "preparation": 50, "recovery": 0},
            "scaled_by": "speed"})

    def test_brace_and_guarded_step_use_shared_self_target_and_movement_authority(self):
        self.assertEqual(self.strike("brace_for_impact")["on_success"], [
            {"kind": "apply_condition", "condition": "braced", "duration": 300}])
        self.assertEqual(self.strike("guarded_step")["on_success"], [
            {"kind": "step_self_one_cell"},
            {"kind": "apply_condition", "condition": "braced", "duration": 200}])
        for name, invocation in [("brace_for_impact", "actor_target"), ("guarded_step", "self_step")]:
            self.assertEqual(self.action(name)["invocation"], invocation)
            self.assertEqual(self.action(name)["reach"], {"kind": "self_only"})
            self.assertFalse(self.action(name)["ai"]["hostile"])
            self.assert_fixed_timing(name)
            self.assert_cost(name, "stamina", 2)

    def test_read_the_rhythm_pays_for_only_a_qualified_public_attack_observation(self):
        self.assertEqual(self.strike("read_the_rhythm")["on_success"], [
            {"kind": "observe_public_plan"},
            {"kind": "apply_qualified_condition_to_source", "condition": "readied",
             "duration": 300, "ability_tag": "attack"},
        ])
        self.assertEqual(self.action("read_the_rhythm")["reach"], {"kind": "sight", "range": 6})
        self.assertNotIn("attack", self.action("read_the_rhythm")["tags"])
        self.assert_fixed_timing("read_the_rhythm")
        self.assert_cost("read_the_rhythm", "focus", 2)
        self.assertIn("attack", self.action("basic_melee")["tags"])

    def test_novice_evocation_and_falter_have_fixed_costs_checks_and_descriptors(self):
        for element in ("fire", "cold"):
            name = "spark_lance_" + element
            self.assertEqual(self.action(name)["reach"], {"kind": "sight", "range": 6})
            self.assertEqual(self.strike(name)["descriptors"], [element])
            self.assertEqual(self.strike(name)["on_success"], [{"kind": "damage", "damage": {
                "kind": "components", "components": [{"category": "energy", "descriptor": element,
                    "amount": {"kind": "rolled", "dice": {"count": 1, "sides": 6, "bonus": 1}}}]}}])
        self.assertEqual(self.action("falter")["reach"], {"kind": "sight", "range": 5})
        self.assertEqual(self.strike("falter")["on_success"], [
            {"kind": "apply_condition", "condition": "fear", "duration": 300}])
        self.assertEqual(self.strike("falter")["descriptors"], ["fear", "mind_affecting"])
        for name in ("spark_lance_fire", "spark_lance_cold", "falter"):
            self.assertEqual(self.strike(name)["check"], {
                "actor": "user", "skill": {"kind": "named", "skill": "spellcasting"},
                "against": {"kind": "defense", "defense": "spiritual" if name == "falter" else "cognitive"}})
            self.assert_fixed_timing(name)
            self.assert_cost(name, "mana", 2)
            self.assertIn("attack", self.action(name)["tags"])

    def test_shell_forms_use_separate_timed_strongest_reduction_categories(self):
        for category in ("impact", "keen", "energy"):
            name = "shell_" + category
            self.assertEqual(self.action(name)["reach"], {"kind": "self_only"})
            self.assertEqual(self.strike(name)["on_success"], [
                {"kind": "apply_condition", "condition": name, "duration": 400}])
            self.assertEqual(self.conditions[name], {
                "timing": {"kind": "actor_active", "on_expiry": {"kind": "remove"}},
                "restrictions": [], "preparation": "preserve",
                "id": name, "family": name, "lifecycle": "timed", "name": "Shell (" + category.title() + ")",
                "modifiers": [{"scope": "always", "op": {"kind": "talent_reduction", "category": category, "amount": 2}}],
                "stacking": {"kind": "longest"}, "source_scoped": False, "descriptors": [], "capacity": "reject"})
            self.assertFalse(self.action(name)["ai"]["hostile"])
            self.assert_fixed_timing(name)
            self.assert_cost(name, "mana", 2)

    def test_recovery_talents_modify_only_the_approved_context_and_quiet_reset(self):
        for name, scope, stat, value in [
            ("steady_recovery", "while_no_sensed_hostile", "health_recovery_period_percent", -25),
            ("hardy_recovery", "always", "health_recovery_reset_percent", -50),
        ]:
            self.assertIn(name, self.talents)
            self.assertEqual(self.talents[name]["modifiers"], [{
                "scope": scope, "op": {"kind": "add", "stat": {"kind": stat}, "value": value}}])

    def test_every_authored_scenario_uses_current_explicit_talent_and_form_purchases(self):
        purchases = []
        for path in sorted((ROOT / "scenarios").rglob("*.toml")):
            package = next(parent for parent in path.parents if (parent / "scenario.toml").is_file())
            manifest = tomllib.loads((package / "scenario.toml").read_text())
            source = tomllib.loads((package / manifest["ruleset"]).read_text())
            declared = dict(self.talents)
            # Purpose-authored graph fixtures remain local to their package.
            # Retirement forbids old production aliases, not synthetic graphs.
            for operation in source.get("overrides", []):
                target = operation["path"]
                if operation["op"] == "add" and len(target) == 2 and target[0] == "talents":
                    definition = operation["value"]
                    self.assertEqual(definition["id"], target[1])
                    declared[target[1]] = definition
            for selection in selected(tomllib.loads(path.read_text())):
                with self.subTest(path=str(path.relative_to(ROOT)), selection=selection):
                    purchases.append(selection)
                    self.assertNotIn(selection["talent"], LEGACY)
                    self.assertIn(selection["talent"], declared)
                    choices = declared[selection["talent"]].get("form_choices", [])
                    if choices:
                        self.assertEqual(selection["kind"], "talent_form")
                        self.assertIn(selection["form"], choices)
                    else:
                        self.assertEqual(selection["kind"], "talent")
        self.assertGreater(len(purchases), 0)
