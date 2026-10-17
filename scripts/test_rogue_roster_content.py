"""New reconstruction: source contracts for retained ordinary Rogue recipes.

Catalog presence is not population activation or measured play/balance evidence.
"""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/rogue-exploration"
ORDINARY = {"emu": (1, [(1, 2)]), "snake": (1, [(1, 3)]), "rattlesnake": (2, [(1, 6)]),
            "hobgoblin": (1, [(1, 8)]), "zombie": (2, [(1, 8)]),
            "quagga": (3, [(1, 5), (1, 5)]),
            "centaur": (4, [(1, 2), (1, 5), (1, 5)]),
            "yeti": (4, [(1, 6), (1, 6)]),
            "troll": (6, [(1, 8), (1, 8), (2, 6)]),
            "black_unicorn": (7, [(1, 9), (1, 9), (2, 9)]),
            "wraith": (5, [(1, 6)]),
            "vampire": (8, [(1, 10)]), "jabberwock": (15, [(2, 12), (2, 4)])}


def source(name):
    return tomllib.loads((PACKAGE / name).read_text(encoding="utf-8"))


class RogueRosterContent(unittest.TestCase):
    def test_retained_ordinary_recipes_own_real_rolled_strikes_and_hd_records(self):
        actors = source("catalog/actors.toml")["actors"]
        species = source("catalog/creatures.toml")["creatures"]["species"]
        self.assertTrue({"rogue." + name for name in ORDINARY}.issubset(actors),
                        "Retained ordinary roster recipes are missing")
        for name, (hd, dice) in ORDINARY.items():
            with self.subTest(species=name):
                identity = "rogue." + name
                build = actors[identity]["creature"]
                self.assertEqual(build["species"], identity)
                self.assertEqual(len(build["hit_dice"]), hd)
                self.assertEqual(build["nutrition"], {"kind": "exempt"})
                self.assertEqual(build["future_growth"], {"kind": "disabled"})
                strikes = species[identity]["melee"]["strikes"]
                self.assertEqual(len(strikes), len(dice))
                for index, (strike, (count, sides)) in enumerate(zip(strikes, dice, strict=True)):
                    amount = strike["on_success"][0]["damage"]["components"][0]["amount"]
                    self.assertEqual(amount, {"kind": "rolled", "dice": {
                        "count": count, "sides": sides, "bonus": 0}})
                    self.assertIn("check", strike)
                    if index:
                        self.assertEqual(strike["unlock"], ("second_strike", "third_strike")[index - 1])

    def test_every_rogue_species_owns_explicit_reference_disposition(self):
        species = source("catalog/creatures.toml")["creatures"]["species"]
        mean = {"emu", "snake", "rattlesnake", "hobgoblin", "zombie", "quagga", "troll", "black_unicorn", "vampire"}
        passive = {"human", "ice_monster", "centaur", "yeti", "wraith", "jabberwock"}
        self.assertEqual(set(species), {"rogue." + name for name in mean | passive})
        for name in mean | passive:
            with self.subTest(species=name):
                self.assertEqual(species["rogue." + name].get("behavior"), {
                    "disposition": "mean" if name in mean else "passive",
                    "idle": "explore" if name == "hobgoblin" else "wait", "memory_ticks": 1000,
                })

    def test_natural_unlocks_are_explicit_eligible_purchases_not_free_attacks(self):
        rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text())
        talents = {row["id"]: row for row in rules["talents"]}
        self.assertIn("second_strike", talents, "Retained NA1 purchase is missing")
        self.assertIn("third_strike", talents, "Retained NA2 purchase is missing")
        for identity, hd in (("second_strike", 2), ("third_strike", 4)):
            self.assertEqual(talents[identity]["modifiers"], [])
            predicates = talents[identity]["requirement"]["requirements"]
            self.assertIn({"kind": "total_hd", "minimum": hd}, predicates)
            self.assertIn({"kind": "species_flag", "flag": "natural_sequence", "present": True}, predicates)
        self.assertEqual(talents["third_strike"]["prerequisites"], ["second_strike"])

    def test_centaur_yeti_keep_exact_owned_hd_purchases_and_cold_immunity(self):
        actors = source("catalog/actors.toml")["actors"]
        species = source("catalog/creatures.toml")["creatures"]["species"]
        for name, owner, unlocks in (("centaur", "warrior", {2: "second_strike", 4: "third_strike"}),
                                     ("yeti", "racial", {2: "second_strike"})):
            with self.subTest(species=name):
                build = actors["rogue." + name]["creature"]
                self.assertEqual(build["restoration"], "authored")
                self.assertEqual(build["hit_dice"], [{"source": owner, "purchases": [
                    {"pool": "advancement", "selection": {"kind": "talent", "talent": unlocks[ordinal]}}
                ] if ordinal in unlocks else []} for ordinal in range(1, 5)])
        self.assertEqual(species["rogue.yeti"]["subtypes"], ["cold"])
        rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text())
        cold = next(row for row in rules["subtypes"] if row["id"] == "cold")
        self.assertFalse(cold["mechanics_deferred"])
        self.assertEqual(cold["modifiers"], [{"scope": "always", "op": {
            "kind": "immunity", "selector": {"kind": "descriptor", "descriptor": "cold"}}}])

    def test_troll_owns_six_exact_racial_rows_and_only_rg1_na1_na2_purchases(self):
        actors = source("catalog/actors.toml")["actors"]
        self.assertIn("rogue.troll", actors, "Approved T recipe is missing")
        build = actors["rogue.troll"]["creature"]
        purchases = {1: "rogue.regeneration", 2: "second_strike", 4: "third_strike"}
        self.assertEqual(build["hit_dice"], [{"source": "racial", "purchases": [
            {"pool": "advancement", "selection": {"kind": "talent", "talent": purchases[ordinal]}}
        ] if ordinal in purchases else []} for ordinal in range(1, 7)])
        self.assertEqual(build["restoration"], "authored")
        self.assertEqual(build["nutrition"], {"kind": "exempt"})
        self.assertEqual(build["future_growth"], {"kind": "disabled"})

    def test_troll_giant_flags_are_eligibility_and_strikes_are_separate_rolled_checks(self):
        catalog = source("catalog/creatures.toml")["creatures"]["species"]
        self.assertIn("rogue.troll", catalog, "Approved T species is missing")
        troll = catalog["rogue.troll"]
        self.assertEqual(troll["kind"], "giant")
        self.assertEqual(troll["behavior"], {
            "disposition": "mean", "idle": "wait", "memory_ticks": 1000})
        ops = [row["op"] for row in troll["modifiers"]]
        self.assertEqual({op["flag"] for op in ops if op["kind"] == "flag"},
                         {"regenerative", "natural_sequence"})
        self.assertFalse(any(op["kind"] == "grant_ability" for op in ops))
        self.assertFalse(any(op.get("stat", {}).get("kind") ==
                             "health_recovery_period_percent" for op in ops))
        strikes = troll["melee"]["strikes"]
        self.assertEqual(len(strikes), 3)
        for strike, unlock, count, sides in zip(strikes, (None, "second_strike", "third_strike"),
                                               (1, 1, 2), (8, 8, 6), strict=True):
            self.assertEqual(strike.get("unlock"), unlock)
            self.assertEqual(strike["check"], {"actor": "user", "skill": {"kind": "natural_attack"},
                             "against": {"kind": "defense", "defense": "physical"}})
            self.assertEqual(strike["on_failure"], [])
            self.assertEqual(strike["riders"], [])
            effect, = strike["on_success"]
            component, = effect["damage"]["components"]
            self.assertEqual(component["amount"], {"kind": "rolled", "dice": {
                "count": count, "sides": sides, "bonus": 0}})
        # RG1's unchanged flag gate and -50 ordinary-period modifier are owned
        # by RogueVampireContent; no second recovery implementation is introduced.

    def test_jabberwock_owns_fifteen_racial_rows_and_only_na1_na3_purchases(self):
        actors = source("catalog/actors.toml")["actors"]
        self.assertIn("rogue.jabberwock", actors, "Approved J recipe is missing")
        actor = actors["rogue.jabberwock"]
        build = actor["creature"]
        purchases = {2: "second_strike", 8: "rending_pair"}
        self.assertEqual(build["hit_dice"], [{"source": "racial", "purchases": [
            {"pool": "advancement", "selection": {"kind": "talent", "talent": purchases[ordinal]}}
        ] if ordinal in purchases else []} for ordinal in range(1, 16)])
        self.assertEqual(build["restoration"], "authored")
        self.assertEqual(build["nutrition"], {"kind": "exempt"})
        self.assertEqual(build["future_growth"], {"kind": "disabled"})
        self.assertEqual(actor["asset"], "creature.jabberwock")
        self.assertEqual(actor["body"], {"cells": [[0, 0, 0], [0, 0, 1]], "eye": [0, 0, 1], "mass": 250})
        self.assertEqual(actor["turn_ticks"], 100)
        self.assertEqual(build["starting_skills"], ["heavy_weaponry", "athletics"])
        manifest = source("scenario.toml")
        self.assertIn(actor["asset"], {asset for theme in manifest["themes"]
                                     for asset in manifest["assets"][theme]})
        species = source("catalog/creatures.toml")["creatures"]["species"]["rogue.jabberwock"]
        self.assertEqual(species["kind"], "aberration")
        self.assertEqual(species["attributes"], {"strength": 2, "speed": 1, "intellect": 0,
                                               "willpower": 1, "awareness": 1, "presence": 0})
        self.assertEqual(species["displacement"], {"size": "large", "anchored": False})
        self.assertEqual({key: value for key, value in species["melee"].items() if key != "strikes"},
                         {"skill": "heavy_weaponry", "bonus": 0, "wind_up": 100, "recovery": 100})
        self.assertEqual(species.get("subtypes", []), [])
        ops = [row["op"] for row in species["modifiers"]]
        self.assertEqual({op["flag"] for op in ops if op["kind"] == "flag"}, {"natural_sequence"})
        self.assertEqual(ops, [{"kind": "flag", "flag": "natural_sequence"},
                               {"kind": "add", "stat": {"kind": "defense", "defense": "physical"}, "value": 3}])
        self.assertFalse(any(op["kind"] == "grant_ability" for op in ops))
        for strike in species["melee"]["strikes"]:
            component, = strike["on_success"][0]["damage"]["components"]
            self.assertEqual(component["category"], "keen")
            self.assertNotIn("descriptor", component)
            self.assertEqual(strike["check"], {"actor": "user", "skill": {"kind": "natural_attack"},
                             "against": {"kind": "defense", "defense": "physical"}})
            self.assertEqual(strike["riders"], [])
            self.assertEqual(strike["descriptors"], [])

    def test_wraith_owns_five_undead_rows_and_only_hd_one_energy_drain(self):
        actors = source("catalog/actors.toml")["actors"]
        self.assertIn("rogue.wraith", actors, "Approved W recipe is missing")
        actor, vampire = actors["rogue.wraith"], actors["rogue.vampire"]
        self.assertEqual(actor["body"], vampire["body"])
        self.assertEqual(actor["turn_ticks"], 100)
        self.assertEqual(actor.get("asset"), "creature.undead.wraith")
        manifest = source("scenario.toml")
        self.assertIn(actor["asset"], {asset for theme in manifest["themes"]
                                     for asset in manifest["assets"][theme]})
        self.assertEqual(actor["creature"], {
            "species": "rogue.wraith", "name": "wraith", "faction": "dungeon_foes",
            "binding": "awareness", "starting_skills": ["heavy_weaponry", "discipline"],
            "restoration": "authored", "nutrition": {"kind": "exempt"},
            "future_growth": {"kind": "disabled"}, "hit_dice": [
                {"source": "racial", "purchases": [{"pool": "advancement", "selection": {
                    "kind": "talent", "talent": "rogue.energy_drain"}}]}]
                + [{"source": "racial", "purchases": []} for _ in range(4)]})
        catalog = source("catalog/creatures.toml")["creatures"]["species"]
        wraith = catalog["rogue.wraith"]
        self.assertEqual(wraith["kind"], "undead")
        self.assertNotIn("subtypes", wraith)
        self.assertEqual(wraith["attributes"], catalog["rogue.vampire"]["attributes"])
        self.assertEqual(wraith["anatomy"], catalog["rogue.vampire"]["anatomy"])
        self.assertEqual(wraith["displacement"], {"size": "medium", "anchored": False})
        self.assertEqual(wraith["behavior"], {
            "disposition": "passive", "idle": "wait", "memory_ticks": 1000})
        self.assertEqual(wraith["modifiers"], [{"scope": "always", "op": {
            "kind": "add", "stat": {"kind": "defense", "defense": "physical"}, "value": 3}}])
        self.assertEqual(wraith["melee"], {
            "skill": "heavy_weaponry", "bonus": 0, "wind_up": 100, "recovery": 100,
            "strikes": [{"offset": 0, "check": {"actor": "user", "skill": {"kind": "natural_attack"},
                "against": {"kind": "defense", "defense": "physical"}}, "on_success": [
                {"kind": "damage", "damage": {"kind": "components", "components": [
                    {"category": "keen", "amount": {"kind": "rolled", "dice": {
                        "count": 1, "sides": 6, "bonus": 0}}}]}}],
                "on_failure": [], "descriptors": [], "riders": []}]})

    def test_energy_drain_is_parentless_hd_one_undead_purchase_with_separate_spiritual_rider(self):
        rows = {tuple(row["path"]): row for row in source("rules.toml")["overrides"]}
        identity = "rogue.energy_drain"
        self.assertIn(("talents", identity), rows, "Approved DR2 purchase is missing")
        self.assertEqual(rows[("talents", identity)], {"op": "add", "path": ["talents", identity], "value": {
            "id": identity, "name": "Energy Drain", "prerequisites": [],
            "requirement": {"kind": "all", "requirements": [
                {"kind": "total_hd", "minimum": 1}, {"kind": "type_in", "types": ["undead"]}]},
            "form_choices": [], "modifiers": [{"scope": "always", "op": {
                "kind": "grant_ability", "ability": identity}}]}})
        self.assertEqual(rows[("abilities", identity)], {"op": "add", "path": ["abilities", identity], "value": {
            "id": identity, "name": "Energy Drain", "interaction": "hostile_to_target", "tags": ["attack"],
            "basic_attack": False, "invocation": "actor_target", "timing": {
                "source": {"kind": "fixed", "prepare": 100, "recover": 100}, "scaled_by": "speed"},
            "reach": {"kind": "melee"}, "strikes": {"kind": "natural_melee", "additional_riders": [{
                "trigger": "success", "check": {"skill": {"kind": "natural_attack"}, "defense": "spiritual"},
                "effects": [{"kind": "drain_latest_hit_die"}], "descriptors": []}]},
            "ai": {"priority": 20, "hostile": True, "minimum_resource_percent": 0, "range_preference": "any"}}})

    def test_wraith_recovery_fixture_changes_only_geometry_placement_and_one_empty_warrior_row(self):
        import copy
        import tempfile
        from types import SimpleNamespace
        from unittest.mock import patch
        from rogue_wraith_package import recovery_package, recovery_sources
        manifest, region, path = recovery_sources()
        production = source("scenario.toml")
        warrior = copy.deepcopy(production["characters"][0])
        warrior["anchor"] = "1/start"
        warrior["creature"]["hit_dice"].append({"source": "warrior", "purchases": []})
        expected = {key: production[key] for key in
                    ("format", "version", "ruleset", "modules", "factions", "ai_profiles", "themes", "assets")}
        expected.update(id="rogue-wraith-earned-recovery", default_character=1, characters=[warrior])
        self.assertEqual(manifest, expected)
        self.assertEqual(region["actors"], [{"id": 3 + bay, "at": [*path[10 + 8 * bay], 0],
            "archetype": "rogue.wraith" if bay == 8 else "rogue.emu",
            "controller": "ai", "ai": "rogue_hostile"} for bay in range(10)])
        self.assertNotIn("items", region)
        self.assertEqual(len(set(path)), len(path))
        self.assertTrue(all(abs(x - nx) + abs(y - ny) == 1
                            for (x, y), (nx, ny) in zip(path, path[1:])))
        width, height, depth = region["size"]
        self.assertLessEqual(max(width, height), 32)
        self.assertEqual(set(map(tuple, region["walls"])), {
            (x, y, z) for x in range(width) for y in range(height) for z in range(depth)
            if (x, y) not in path})
        with tempfile.TemporaryDirectory() as directory:
            fake = SimpleNamespace(directory=Path(directory), bin=Path("compiler-bin"), suffix="",
                assertEqual=self.assertEqual, check_disk=lambda: None,
                work_budget=SimpleNamespace(output=SimpleNamespace(paths=())))
            with patch("rogue_wraith_package.subprocess.run", return_value=SimpleNamespace(returncode=0)) as run:
                package = recovery_package(fake)
            for relative in [*manifest["modules"], manifest["ruleset"]]:
                self.assertEqual((package / relative).read_bytes(), (PACKAGE / relative).read_bytes())
            self.assertEqual(tomllib.loads((package / "scenario.toml").read_text()), manifest)
            self.assertEqual(tomllib.loads((package / "regions/1.toml").read_text()), region)
            self.assertEqual(run.call_args.args[0], [fake.bin / "tor-scenario", "validate", package])
            self.assertEqual(run.call_args.kwargs["timeout"], 30)

    def test_wraith_recovery_expected_kill_credit_uses_unchanged_shared_threat_and_threshold(self):
        rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text())
        growth = rules["growth"]
        self.assertEqual(growth, {"threshold_base": 100, "threshold_per_tier": 100,
            "reward_base": 25, "reward_per_relative_tier": 5, "minimum_reward": 1,
            "maximum_reward": 1000, "recovery": 0, "threat": {"points_per_tier": 100,
                "hit_die": 70, "trained_rank": 5, "species": 10, "creature_type": 10,
                "subtype": 5, "class": 10, "template": 20, "talent": 10, "overrides": []}})
        self.assertFalse(any(row["path"][0] == "growth" for row in source("rules.toml")["overrides"]))
        # E: 70 HD + 10 two ranks + 10 species + 10 type = tier 1.
        # W: 350 HD + 10 ranks + 10 species + 10 type + 10 DR2 = tier 4.
        # Retained highest-earned HD2: E awards20, W awards35; regrowth costs200.
        self.assertEqual((8 * 20, 8 * 20 + 35, 9 * 20 + 35 - 200), (160, 195, 15))

    def test_wraith_corridor_choices_use_only_disclosed_forward_and_remembered_edges(self):
        from rogue_wraith_package import disclosed_corridor_step
        def cell(key, x, y):
            return {"key": key, "position": {"x": x, "y": y, "z": 0}, "wall": False}
        previous, current, forward = cell("prior", -1, 0), cell("here", 0, 0), cell("next", 1, 0)
        frame = {"state": {"observation": {"visible_cells": [forward, previous, current]}}}
        self.assertEqual(disclosed_corridor_step(frame, {"prior", "here"}), (forward, "east"))
        self.assertEqual(disclosed_corridor_step(frame, {"prior", "here", "next"}, "prior"),
                         (previous, "west"))
        with self.assertRaisesRegex(AssertionError, "disclosed corridor"):
            disclosed_corridor_step(frame, {"here"})
        with self.assertRaisesRegex(AssertionError, "disclosed corridor"):
            disclosed_corridor_step(frame, {"prior", "here"}, "undisclosed")

    def test_wraith_drain_interval_requires_separate_survived160_and_real_contact(self):
        from rogue_wraith_package import drain_interval
        contact = {"type": "ability", "caster": "w", "target": "p", "ability": {
            "id": "rogue.energy_drain", "name": "Energy Drain"}, "outcome": "applied", "strike": 0}
        death = {"type": "died", "actor": "w"}
        arguments = {"wraith": "w", "victim": "p", "prior_earned": 160, "prior_hd": 2, "hd": 1}
        self.assertEqual(drain_interval([contact], earned=160, **arguments), "separate_160_checkpoint")
        # Positive160 control executes before the former195 acceptance is rejected.
        with self.assertRaisesRegex(AssertionError, "separate160"):
            drain_interval([contact, death], earned=195, **arguments)
        for events in ([], [dict(contact, outcome="miss")], [dict(contact, caster="other")],
                       [dict(contact, target="other")], [dict(contact, strike=1)],
                       [dict(contact, ability={"id": "basic_melee", "name": "Melee"})],
                       [death], [death, contact], [contact, death]):
            with self.subTest(events=events), self.assertRaises(AssertionError):
                drain_interval(events, earned=160, **arguments)
        for key, value in (("prior_earned", 140), ("prior_hd", 1), ("hd", 2)):
            with self.subTest(field=key), self.assertRaisesRegex(AssertionError, "first survived drain"):
                drain_interval([contact], earned=160, **dict(arguments, **{key: value}))

    def test_generated_carrier_fixture_preserves_production_policies_and_recipes(self):
        from rogue_vampire_package import generated_carrier_manifest
        manifest = source("scenario.toml")
        canonical = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]
        for name, depth in (("centaur", 7), ("yeti", 11), ("troll", 13), ("vampire", 27),
                            ("jabberwock", 21)):
            with self.subTest(species=name):
                fixture = generated_carrier_manifest(name)
                self.assertEqual(fixture["modules"], manifest["modules"])
                self.assertEqual(fixture["ai_profiles"], manifest["ai_profiles"])
                self.assertEqual(fixture["floor_activation"]["carried_item_pools"],
                                 manifest["floor_activation"]["carried_item_pools"])
                stage = fixture["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]
                self.assertEqual(stage["id"], canonical["id"])
                self.assertEqual(stage["ai"], canonical["ai"])
                self.assertEqual(stage["depth_growth"], canonical["depth_growth"])
                self.assertEqual(stage["count"], [1, 1])
                choice = stage["bands"][0]["choices"][0]
                expected = next(row for band in canonical["bands"] for row in band["choices"]
                                if row["archetype"] == "rogue." + name)
                self.assertEqual(choice["carried_loot"], expected["carried_loot"])
                self.assertEqual(fixture["characters"][0]["anchor"], f"dungeon/{depth}/up")
                self.assertEqual(len(fixture["characters"][0]["creature"]["hit_dice"]), 16)
                self.assertEqual(fixture["characters"][0]["loadout"], manifest["characters"][0]["loadout"])

    def test_production_population_selects_fourteen_species_and_preserves_stock_order(self):
        manifest = source("scenario.toml")
        activation = manifest["floor_activation"]
        profile = activation["profiles"]["rogue.low_depth"]
        self.assertEqual(profile["version"], 1)
        stages = profile["stages"]
        self.assertEqual([row["id"] for row in stages],
                         ["rogue.eight_species_foes", "rogue.bandages", "rogue.rations", "rogue.restore_strength"])
        self.assertEqual([row["count"] for row in stages], [[4, 4], [1, 1], [1, 1], [1, 1]])
        actor = stages[0]
        self.assertEqual(actor["kind"], "actors")
        self.assertEqual(actor["version"], 1)
        self.assertEqual(actor["ai"], "rogue_hostile")
        self.assertTrue(actor["avoid_protagonist_sight"])
        self.assertEqual(actor["depth_growth"], {"kind": "racial_after", "depth": 26})
        # Exact band coverage, normalized weights and endpoints belong to the
        # independent pinned-selector oracle in test_rogue_activation_candidate.
        self.assertEqual({row["archetype"] for band in actor["bands"] for row in band["choices"]},
                         {"rogue." + name for name in (
                             "emu", "snake", "rattlesnake", "hobgoblin", "ice_monster", "zombie", "centaur", "quagga", "yeti", "troll", "wraith", "black_unicorn", "vampire", "jabberwock")})
        chances = {"rogue.centaur": (3, 20), "rogue.yeti": (3, 10), "rogue.troll": (1, 2), "rogue.vampire": (1, 5), "rogue.jabberwock": (7, 10)}
        for band in actor["bands"]:
            for row in band["choices"]:
                if row["archetype"] in chances:
                    numerator, denominator = chances[row["archetype"]]
                    self.assertEqual(row["carried_loot"], {"version": 1,
                        "pool": "rogue.vampire_supplies",
                        "chance": {"numerator": numerator, "denominator": denominator}})
                else:
                    self.assertNotIn("carried_loot", row)
        self.assertEqual(activation["carried_item_pools"], {"rogue.vampire_supplies": {
            "version": 1, "choices": [
                {"archetype": "rogue.ration", "weight": 1, "quantity": 1},
                {"archetype": "rogue.clean_bandage", "weight": 1, "quantity": 1}]}})
        for stage, identity, quantity in zip(stages[1:], ("clean_bandage", "ration", "restore_strength"), (2, 1, 1), strict=True):
            self.assertEqual(stage["kind"], "items")
            self.assertEqual(stage["bands"], [{"depth": {"kind": "open", "minimum": 1},
                              "choices": [{"archetype": "rogue." + identity, "weight": 1}]}])
            self.assertEqual(stage["quantity"], quantity)
        self.assertEqual(activation["protagonist"], "selected_character")
        self.assertEqual(manifest["procedural_worlds"]["dungeon"]["domain"],
                         {"kind": "open", "max_depth": 1_000_000})
        self.assertNotIn("objective", manifest)


if __name__ == "__main__":
    unittest.main()
