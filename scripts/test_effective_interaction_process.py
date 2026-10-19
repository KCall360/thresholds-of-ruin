"""Effective non-hostile damage rejection through the real compiler and clients."""
import copy
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import ProcessTestCase, creature_package
from status_condition_package import actor_creature, checkpoint


def flat_modifier(value):
    return {"scope": "always", "op": {"kind": "add", "stat": {
        "kind": "ability", "ability": "basic_melee", "stat": "flat_bonus"}, "value": value}}


class EffectiveInteractionProcesses(ProcessTestCase):
    def package(self, natural, *, combined=False):
        package = creature_package(self, "effective-interaction", abilities=())
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        species = manifest["creatures"]["species"]["human"]
        species["behavior"] = {"disposition": "passive", "idle": "wait", "memory_ticks": 1000}
        amount = ({"kind": "fixed", "amount": 0} if natural else
                  {"kind": "rolled", "dice": {"count": 1, "sides": 2, "bonus": -2}})
        if combined:
            amount = {"kind": "rolled", "dice": {"count": 1, "sides": 2, "bonus": -5}}
        species["melee"]["strikes"][0]["on_success"][0]["damage"]["components"][0]["amount"] = amount
        manifest["archetypes"]["amplifier"] = {
            "class": "armor", "name": "damage amplifier", "equipment": {
                "slot": "head_armor", "modifiers": [flat_modifier(3)]}}
        manifest["ai_profiles"]["practice"] = {
            "care": False, "equipment": False, "loot": False, "flee_percent": 0}
        path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        region["items"] = [{"id": 20, "at": [1, 1, 0], "carried_by": 1, "archetype": "amplifier"}]
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        path = package / "rules.toml"
        rules = tomllib.loads(path.read_text(encoding="utf-8"))
        ability = next(row for row in rules["abilities"] if row["id"] == "basic_melee")
        ability["interaction"] = "non_hostile"
        ability["strikes"] = ({"kind": "natural_melee", "additional_riders": []} if natural else
                              {"kind": "authored", "strikes": species["melee"]["strikes"]})
        if combined:
            condition = next(row for row in rules["conditions"] if row["id"] == "fear")
            condition.update(modifiers=[flat_modifier(3)], restrictions=[], preparation="preserve")
            charge = copy.deepcopy(ability)
            charge.update(id="fixture_charge", name="fixture charge", basic_attack=False,
                          reach={"kind": "self_only"},
                          timing={"source": {"kind": "fixed", "prepare": 1, "recover": 1}},
                          strikes={"kind": "authored", "strikes": [{"offset": 0,
                              "on_success": [{"kind": "apply_condition", "condition": "fear", "duration": 100000}],
                              "on_failure": [], "descriptors": [], "riders": []}]})
            charge.pop("cost", None)
            charge.pop("cooldown", None)
            rules["abilities"].append(charge)
            rules["constants"]["default_abilities"].append("fixture_charge")
        path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        validated = self.validate(package)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        return package

    def validate(self, package):
        return subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                              capture_output=True, text=True, encoding="utf-8", timeout=30)

    def test_compiler_rejects_current_positive_damage_and_preserves_certificate(self):
        for natural in (True, False):
            with self.subTest(natural=natural):
                package = self.package(natural)
                before = (package / "validation.json").read_bytes()
                path = package / "scenario.toml"
                manifest = tomllib.loads(path.read_text(encoding="utf-8"))
                manifest["creatures"]["species"]["human"]["modifiers"].append(flat_modifier(3))
                path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
                rejected = self.validate(package)
                self.assertNotEqual(rejected.returncode, 0, rejected.stdout)
                self.assertTrue(rejected.stderr.strip(), "the compiler must explain rejection")
                self.assertEqual((package / "validation.json").read_bytes(), before)
                # Each subcase constructs a separate fresh authored package.
                package.rename(package.with_name("tested-natural" if natural else "tested-authored"))

    def test_equipment_that_turns_non_hostile_damage_positive_is_rejected(self):
        package = self.package(True)
        self.server(scenario=package)
        player, initial = self.client()
        item = next(row["id"] for row in initial["state"]["observation"]["inventory"]
                    if row["name"] == "damage amplifier")
        rejected = self.act(player, {"type": "equip", "item": item, "slot": 3})
        self.assertIsNotNone(rejected["error"])
        final = self.request(player, {"type": "snapshot"})
        before = next(row for row in initial["state"]["observation"]["inventory"] if row["id"] == item)
        after = next(row for row in final["state"]["observation"]["inventory"] if row["id"] == item)
        self.assertEqual(after, before)


    def test_combined_condition_and_gear_rejection_preserves_state_items_and_rng(self):
        package = self.package(False, combined=True)
        self.server("--checkpoint-interval", 1, scenario=package)
        player, initial = self.client()
        charged = self.act(player, {"type": "use_ability", "id": "fixture_charge",
            "invocation": {"aim": {"kind": "actor_target", "target": initial["state"]["observation"]["self_target"]}, "weapon": {"kind": "default"}}})
        self.assertIsNone(charged["error"])
        saved = checkpoint(self, player)
        self.assertTrue(any(row["condition"] == "fear" for row in actor_creature(saved)["conditions"]))
        item = next(row["id"] for row in charged["state"]["observation"]["inventory"]
                    if row["name"] == "damage amplifier")
        rejected = self.act(player, {"type": "equip", "item": item, "slot": 3})
        self.assertIsNotNone(rejected["error"])
        self.assertEqual(checkpoint(self, player), saved)
