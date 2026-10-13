"""Wizard advancement through the real server, Text and headless clients."""
from process_harness import content_ids
import json
import subprocess
import tomllib

from arena_matrix import toml_source

from process_harness import ProcessTestCase, WIZARD_TOKEN
from process_harness import creature_package


class CreatureAdvancementProcesses(ProcessTestCase):
    def test_added_hit_die_is_stable_across_removal_restart_and_rewind(self):
        package = creature_package(self, "advancement", abilities=("power_strike",))
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('archetypes = { ', 'archetypes = { injury = { class = "potion", name = "injury potion", consumable = { effects = [{ kind = "damage", damage = { kind = "components", components = [{ category = "vital", amount = { kind = "fixed", amount = 3 } }] } }] } }, ', 1)
        manifest.write_text(source, encoding="utf-8", newline="\n")
        region = package / "regions/1.toml"
        source = region.read_text(encoding="utf-8")
        source += '\n[[items]]\nid = 3\nat = [1, 1, 0]\ncarried_by = 1\narchetype = "injury"\n'
        region.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        server = self.server(scenario=package, wizard=True)
        observer, initial = self.client(WIZARD_TOKEN, observe=True)
        baseline = initial["state"]["observation"]["combat"]
        player, inventory = self.client()
        potion = next(item["id"] for item in inventory["state"]["observation"]["inventory"]
                      if item["class"] == "potion")
        self.assertIsNone(self.act(player, {"type": "drink", "item": potion})["error"])
        player.stop()
        injured = self.request(observer, {"type": "snapshot"})
        original = injured["state"]["observation"]["combat"]
        self.assertEqual(original["max_hp"] - original["hp"], 3)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard creature add-hd 1 warrior"))
        advanced = self.request(observer, {"type": "snapshot"})
        combat = advanced["state"]["observation"]["combat"]
        self.assertEqual(content_ids(combat["own_stats"]["hit_dice"]), content_ids(original["own_stats"]["hit_dice"]) + ["warrior"])
        self.assertEqual(combat["own_stats"]["skills"], original["own_stats"]["skills"])
        self.assertGreater(combat["max_hp"], original["max_hp"])
        self.assertEqual(combat["max_hp"] - combat["hp"], original["max_hp"] - original["hp"])
        self.assertEqual(advanced["state"]["observation"]["tick"], injured["state"]["observation"]["tick"])
        self.assertNotIn("Server error", wizard.command("wizard creature remove-hd 1"))
        regressed = self.request(observer, {"type": "snapshot"})["state"]["observation"]["combat"]
        self.assertEqual(regressed["own_stats"], original["own_stats"])
        self.assertEqual(regressed["hp"], original["hp"])
        self.assertNotIn("Server error", wizard.command("wizard creature add-hd 1 warrior"))
        repeated = self.request(observer, {"type": "snapshot"})
        self.assertEqual(repeated["state"]["observation"]["combat"], combat)
        observer.stop()
        wizard.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        observer, restored = self.client(WIZARD_TOKEN, observe=True)
        self.assertEqual(restored["state"], repeated["state"])
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard rewind initial"))
        rewound = self.request(observer, {"type": "snapshot"})["state"]["observation"]["combat"]
        self.assertEqual(rewound["own_stats"], baseline["own_stats"])
        self.assertEqual(rewound["hp"], baseline["hp"])

    def test_shared_points_compete_while_attributes_and_starting_ranks_stay_separate(self):
        package = creature_package(self, "owned-advancement", abilities=("power_strike",))
        self.server(scenario=package, wizard=True)
        observer, initial = self.client(WIZARD_TOKEN, observe=True)
        original = initial["state"]["observation"]["combat"]["own_stats"]
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        for command in (
            "wizard creature add-hd 1 warrior",
            "wizard creature add-hd 1 mage",
            "wizard creature train 1 3 athletics",
            "wizard creature attribute 1 4 strength",
            "wizard creature talent 1 4 hardiness",
        ):
            self.assertNotIn("Server error", wizard.command(command), command)
        trained = self.request(observer, {"type": "snapshot"})
        stats = trained["state"]["observation"]["combat"]["own_stats"]
        self.assertEqual(stats["attributes"]["strength"], original["attributes"]["strength"] + 1)
        self.assertIn("hardiness", content_ids(stats["active_talents"]))
        self.assertEqual(next(skill["rank"] for skill in stats["skills"] if skill["skill"]["id"] == "athletics"), 1)
        for command in (
            "wizard creature talent 1 3 guard",
            "wizard creature train 1 4 lore",
            "wizard creature attribute 1 4 speed",
            "wizard creature attribute 1 3 speed",
            "wizard creature talent 1 4 guard",
            "wizard creature train 1 0 lore",
            "wizard creature train 1 5 lore",
        ):
            self.assertIn("Server error", wizard.command(command), command)
            self.assertEqual(self.request(observer, {"type": "snapshot"})["state"], trained["state"])
        self.assertNotIn("Server error", wizard.command("wizard creature remove-hd 1"))
        removed = self.request(observer, {"type": "snapshot"})["state"]["observation"]["combat"]["own_stats"]
        self.assertEqual(removed["attributes"], original["attributes"])
        self.assertEqual(removed["skills"], stats["skills"])
        self.assertNotIn("hardiness", content_ids(removed["active_talents"]))
        self.assertEqual(len(content_ids(removed["hit_dice"])), 3)
        # A PC chooses again: regrowing the same ordinal restores its point,
        # but neither the former talent nor the separate attribute choice.
        self.assertNotIn("Server error", wizard.command("wizard creature add-hd 1 mage"))
        self.assertNotIn("Server error", wizard.command("wizard creature train 1 4 lore"))
        regrown = self.request(observer, {"type": "snapshot"})["state"]["observation"]["combat"]["own_stats"]
        self.assertNotIn("hardiness", content_ids(regrown["active_talents"]))
        self.assertEqual(regrown["attributes"], original["attributes"])
        self.assertEqual(next(skill["rank"] for skill in regrown["skills"] if skill["skill"]["id"] == "lore"), 1)
        for _ in range(2):
            self.assertNotIn("Server error", wizard.command("wizard creature remove-hd 1"))
        reset = self.request(observer, {"type": "snapshot"})["state"]["observation"]["combat"]["own_stats"]
        self.assertEqual(reset["skills"], original["skills"])

    def test_adding_a_die_after_zero_hd_never_revives_the_actor(self):
        package = creature_package(self, "dead-advancement", abilities=("power_strike",))
        self.server(scenario=package, wizard=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        for _ in range(2):
            self.assertNotIn("Server error", wizard.command("wizard creature remove-hd 1"))
        dead = self.request(observer, {"type": "snapshot"})["state"]["observation"]["combat"]
        self.assertTrue(dead["dead"])
        self.assertEqual(dead["max_hp"], 0)
        surviving_ranks = {skill["skill"]["id"]: skill["rank"] for skill in dead["own_stats"]["skills"]}
        self.assertEqual(surviving_ranks["heavy_weaponry"], 1)
        self.assertEqual(surviving_ranks["spellcasting"], 1)
        self.assertEqual(surviving_ranks["intimidation"], 0)
        self.assertNotIn("Server error", wizard.command("wizard creature add-hd 1 warrior"))
        advanced = self.request(observer, {"type": "snapshot"})["state"]["observation"]["combat"]
        self.assertTrue(advanced["dead"])
        self.assertEqual(advanced["hp"], 0)
        self.assertGreater(advanced["max_hp"], 0)
        self.assertEqual(content_ids(advanced["own_stats"]["hit_dice"]), ["warrior"])

    def test_older_owned_talent_goes_dormant_and_reactivates_with_its_class_requirement(self):
        package = creature_package(self, "dormant-advancement", abilities=("power_strike",))
        server = self.server(scenario=package, wizard=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard creature talent 1 1 magic_bolt"))
        active = self.request(observer, {"type": "snapshot"})["state"]["observation"]["combat"]["own_stats"]
        self.assertIn("magic_bolt", content_ids(active["active_talents"]))
        self.assertIn("magic_bolt", content_ids(active["abilities"]))
        self.assertNotIn("Server error", wizard.command("wizard creature remove-hd 1"))
        dormant = self.request(observer, {"type": "snapshot"})
        stats = dormant["state"]["observation"]["combat"]["own_stats"]
        self.assertEqual(content_ids(stats["hit_dice"]), ["warrior"])
        self.assertIn("magic_bolt", content_ids(stats["dormant_talents"]))
        self.assertNotIn("magic_bolt", content_ids(stats["active_talents"]))
        self.assertNotIn("magic_bolt", content_ids(stats["abilities"]))
        self.assertEqual(next(skill["rank"] for skill in stats["skills"] if skill["skill"]["id"] == "spellcasting"), 1)
        observer.stop()
        wizard.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        observer, restored = self.client(WIZARD_TOKEN, observe=True)
        self.assertEqual(restored["state"], dormant["state"])
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard creature add-hd 1 mage"))
        reactivated = self.request(observer, {"type": "snapshot"})["state"]["observation"]["combat"]["own_stats"]
        self.assertIn("magic_bolt", content_ids(reactivated["active_talents"]))
        self.assertNotIn("magic_bolt", content_ids(reactivated["dormant_talents"]))
        self.assertIn("magic_bolt", content_ids(reactivated["abilities"]))
        self.assertEqual(next(skill["rank"] for skill in reactivated["skills"] if skill["skill"]["id"] == "spellcasting"), 1)

    def test_authored_restore_refills_owned_purchases_and_reactivates_an_or_parent_branch(self):
        package = creature_package(self, "authored-or-restoration", abilities=("power_strike",))
        rules_path = package / "rules.toml"
        rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
        for identity in ("fixture_root_a", "fixture_root_b"):
            rules["talents"].append({"id": identity, "name": identity, "prerequisites": [],
                                     "requirement": {"kind": "total_hd", "minimum": 1}, "modifiers": []})
        rules["talents"].append({
            "id": "fixture_branch", "name": "Fixture branch", "prerequisites": [],
            "requirement": {"kind": "any", "requirements": [
                {"kind": "owned_talent", "talent": "fixture_root_a"},
                {"kind": "owned_talent", "talent": "fixture_root_b"}]},
            "modifiers": [{"scope": "always", "op": {"kind": "grant_ability", "ability": "magic_bolt"}}],
        })
        rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        build = manifest["characters"][0]["creature"]
        build["restoration"] = "authored"
        build["hit_dice"] = [
            {"source": source, "purchases": [{"pool": "advancement", "selection": {
                "kind": "talent", "talent": talent}}]}
            for source, talent in (("warrior", "fixture_branch"), ("mage", "fixture_root_a"),
                                   ("warrior", "fixture_root_b"))]
        path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server(scenario=package, wizard=True)
        observer, initial = self.client(WIZARD_TOKEN, observe=True)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)

        def inspect():
            offset = len(observer.transcript)
            self.assertIsNone(self.command(observer, {"type": "wizard", "command": "creature inspect 1"})["error"])
            messages = [json.loads(line).get("message") for line in observer.transcript[offset:] if line.startswith("{")]
            return next(message["report"] for message in reversed(messages)
                        if message and message["type"] == "creature_inspection")

        original = inspect()
        self.assertEqual(original["restoration"], "authored")
        self.assertEqual(content_ids(original["starting_skills"]), ["heavy_weaponry", "spellcasting"])
        self.assertIn("magic_bolt", content_ids(original["stats"]["abilities"]))
        self.assertNotIn("Server error", wizard.command("wizard creature remove-hd 1"))
        one_parent = inspect()
        self.assertIn("fixture_branch", content_ids(one_parent["stats"]["active_talents"]))
        self.assertEqual(one_parent["dormant_talents"], [])
        self.assertNotIn("Server error", wizard.command("wizard creature remove-hd 1"))
        dormant = inspect()
        self.assertEqual(content_ids(dormant["stats"]["dormant_talents"]), ["fixture_branch"])
        self.assertNotIn("magic_bolt", content_ids(dormant["stats"]["abilities"]))
        reasons = dormant["dormant_talents"][0]["reasons"]
        self.assertTrue(reasons)
        self.assertIn("fixture_root_a", " ".join(reasons))
        self.assertIn("fixture_root_b", " ".join(reasons))
        output = wizard.command("wizard creature inspect 1")
        self.assertIn("Dormant talent: Fixture branch", output)
        self.assertIn("1 granted, 1 spent, 0 unspent", output)
        self.assertIn("Starting skills (permanent)", output)
        self.assertEqual(dormant["starting_skills"], original["starting_skills"])
        self.assertEqual(dormant["hit_dice"], original["hit_dice"][:1])
        observer.stop()
        wizard.stop()
        server.stop()
        server = self.server(scenario=package, wizard=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        self.assertEqual(inspect(), dormant)
        self.assertIn("Server error", wizard.command("wizard creature add-hd 1 warrior"))
        self.assertEqual(inspect(), dormant)
        self.assertNotIn("Server error", wizard.command("wizard creature add-hd 1 mage"))
        restored = inspect()
        self.assertEqual(restored["hit_dice"], original["hit_dice"][:2])
        self.assertEqual(restored["starting_skills"], original["starting_skills"])
        self.assertEqual(restored["dormant_talents"], [])
        self.assertIn("fixture_branch", content_ids(restored["stats"]["active_talents"]))
        self.assertIn("magic_bolt", content_ids(restored["stats"]["abilities"]))
        self.assertNotIn("Server error", wizard.command("wizard creature add-hd 1 warrior"))
        self.assertEqual(inspect(), original)
        self.assertEqual(self.request(observer, {"type": "snapshot"})["state"]["observation"]["tick"],
                         initial["state"]["observation"]["tick"])

        # The immediate editor is privileged, uses the same transaction kernel,
        # and preserves an OR branch until its final owned route is removed.
        player, _ = self.client()
        denied = self.command(player, {"type": "wizard", "command":
                                       "creature replace 1 2 1 advancement skill athletics"})
        self.assertIsNotNone(denied["error"])
        player.stop()
        self.assertEqual(inspect(), original)
        self.assertIn("Server error", wizard.command(
            "wizard creature replace 1 2 1 unknown_pool skill athletics"))
        self.assertEqual(inspect(), original)
        self.assertNotIn("Server error", wizard.command(
            "wizard creature replace 1 2 1 advancement skill athletics"))
        alternate = inspect()
        self.assertIn("fixture_branch", content_ids(alternate["stats"]["active_talents"]))
        self.assertNotIn("fixture_root_a", content_ids(alternate["stats"]["active_talents"]))
        self.assertEqual(alternate["hit_dice"][0], original["hit_dice"][0])
        self.assertNotIn("Server error", wizard.command("wizard creature replace 1 3 1 refund"))
        replaced = inspect()
        self.assertEqual(replaced["hit_dice"][0]["purchases"], [])
        self.assertEqual(replaced["hit_dice"][2]["purchases"], [])
        self.assertEqual(replaced["hit_dice"][0]["point_pools"][0]["spent"], 0)
        self.assertEqual(replaced["hit_dice"][1]["point_pools"][0]["spent"], 1)
        self.assertEqual(replaced["hit_dice"][2]["point_pools"][0]["spent"], 0)
        self.assertNotIn("fixture_branch", content_ids(replaced["stats"]["active_talents"]))
        self.assertNotIn("fixture_branch", content_ids(replaced["stats"]["dormant_talents"]))
        for field in ("starting_skills", "initial_attributes", "restoration", "injury", "dead", "tick"):
            self.assertEqual(replaced[field], original[field], field)
        self.assertEqual(replaced["stats"]["resources"], original["stats"]["resources"])
        for edited, prior in zip(replaced["hit_dice"], original["hit_dice"], strict=True):
            for field in ("source", "health_seed", "health_die", "base_health", "attribute"):
                self.assertEqual(edited[field], prior[field], field)
        observer.stop()
        wizard.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True)
        self.assertEqual(inspect(), replaced)

    def test_multi_point_authoring_enforces_total_talent_ownership_before_observation(self):
        package = creature_package(self, "ownership-boundary", abilities=("power_strike",))
        rules_path = package / "rules.toml"
        rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
        rules["constants"]["racial_point_grants"][0]["points"] = 129
        rules["talents"] = [
            {"id": f"limit_{index:03}", "name": f"Limit {index}", "prerequisites": [],
             "requirement": {"kind": "total_hd", "minimum": 1}, "modifiers": []}
            for index in range(258)]
        rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))

        def validate_count(count):
            manifest["characters"][0]["creature"]["hit_dice"] = [
                {"source": "racial", "purchases": [
                    {"pool": "advancement", "selection": {"kind": "talent", "talent": f"limit_{index:03}"}}
                    for index in range(owner * 129, min((owner + 1) * 129, count))]}
                for owner in range(2)]
            path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
            return subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                  capture_output=True, text=True, encoding="utf-8", timeout=30)

        for count in (257, 258):
            self.assertNotEqual(validate_count(count).returncode, 0, f"accepted {count} talents")
        result = validate_count(256)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.server(scenario=package, wizard=True)
        observer, initial = self.client(WIZARD_TOKEN, observe=True)
        stats = initial["state"]["observation"]["combat"]["own_stats"]
        self.assertEqual(len(stats["active_talents"]), 256)
        self.assertEqual(stats["dormant_talents"], [])
        self.assertIsNone(self.command(observer, {"type": "wizard", "command": "creature inspect 1"})["error"])
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        before = self.request(observer, {"type": "snapshot"})["state"]
        self.assertIn("Server error", wizard.command("wizard creature talent 1 2 limit_256"))
        self.assertEqual(self.request(observer, {"type": "snapshot"})["state"], before)
