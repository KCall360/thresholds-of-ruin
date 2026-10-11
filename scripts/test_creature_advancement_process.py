"""Wizard advancement through the real server, Text and headless clients."""
import subprocess

from process_harness import ProcessTestCase, WIZARD_TOKEN
from process_harness import creature_package


class CreatureAdvancementProcesses(ProcessTestCase):
    def test_added_hit_die_is_stable_across_removal_restart_and_rewind(self):
        package = creature_package(self, "advancement", abilities=("power_strike",))
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('archetypes = { ', 'archetypes = { injury = { class = "potion", name = "injury potion", consumable = { effects = [{ type = "damage", components = { vital = 3 } }] } }, ', 1)
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
        self.assertEqual(combat["own_stats"]["hit_dice"], original["own_stats"]["hit_dice"] + ["warrior"])
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

    def test_owned_training_attribute_and_talent_are_removed_with_their_die(self):
        package = creature_package(self, "owned-advancement", abilities=("power_strike",))
        self.server(scenario=package, wizard=True)
        observer, initial = self.client(WIZARD_TOKEN, observe=True)
        original = initial["state"]["observation"]["combat"]["own_stats"]
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        for command in (
            "wizard creature add-hd 1 warrior",
            "wizard creature add-hd 1 mage",
            "wizard creature train 1 4 athletics",
            "wizard creature train 1 4 discipline",
            "wizard creature attribute 1 4 strength",
            "wizard creature talent 1 4 hardiness",
        ):
            self.assertNotIn("Server error", wizard.command(command), command)
        trained = self.request(observer, {"type": "snapshot"})
        stats = trained["state"]["observation"]["combat"]["own_stats"]
        self.assertEqual(stats["attributes"]["strength"], original["attributes"]["strength"] + 1)
        self.assertIn("hardiness", stats["active_talents"])
        self.assertEqual(next(skill["rank"] for skill in stats["skills"] if skill["skill"] == "athletics"), 1)
        for command in (
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
        self.assertEqual(removed["skills"], original["skills"])
        self.assertNotIn("hardiness", removed["active_talents"])
        self.assertEqual(len(removed["hit_dice"]), 3)

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
        self.assertNotIn("Server error", wizard.command("wizard creature add-hd 1 warrior"))
        advanced = self.request(observer, {"type": "snapshot"})["state"]["observation"]["combat"]
        self.assertTrue(advanced["dead"])
        self.assertEqual(advanced["hp"], 0)
        self.assertGreater(advanced["max_hp"], 0)
        self.assertEqual(advanced["own_stats"]["hit_dice"], ["warrior"])

    def test_older_owned_talent_goes_dormant_and_reactivates_with_its_class_requirement(self):
        package = creature_package(self, "dormant-advancement", abilities=("power_strike",))
        server = self.server(scenario=package, wizard=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard creature train 1 1 spellcasting"))
        self.assertNotIn("Server error", wizard.command("wizard creature talent 1 1 magic_bolt"))
        active = self.request(observer, {"type": "snapshot"})["state"]["observation"]["combat"]["own_stats"]
        self.assertIn("magic_bolt", active["active_talents"])
        self.assertIn("magic_bolt", active["abilities"])
        self.assertNotIn("Server error", wizard.command("wizard creature remove-hd 1"))
        dormant = self.request(observer, {"type": "snapshot"})
        stats = dormant["state"]["observation"]["combat"]["own_stats"]
        self.assertEqual(stats["hit_dice"], ["warrior"])
        self.assertIn("magic_bolt", stats["dormant_talents"])
        self.assertNotIn("magic_bolt", stats["active_talents"])
        self.assertNotIn("magic_bolt", stats["abilities"])
        self.assertEqual(next(skill["rank"] for skill in stats["skills"] if skill["skill"] == "spellcasting"), 1)
        observer.stop()
        wizard.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        observer, restored = self.client(WIZARD_TOKEN, observe=True)
        self.assertEqual(restored["state"], dormant["state"])
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard creature add-hd 1 mage"))
        reactivated = self.request(observer, {"type": "snapshot"})["state"]["observation"]["combat"]["own_stats"]
        self.assertIn("magic_bolt", reactivated["active_talents"])
        self.assertNotIn("magic_bolt", reactivated["dormant_talents"])
        self.assertIn("magic_bolt", reactivated["abilities"])
        self.assertEqual(next(skill["rank"] for skill in reactivated["skills"] if skill["skill"] == "spellcasting"), 1)
