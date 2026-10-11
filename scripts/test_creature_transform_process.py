"""Wizard creature transformations through the real server and Text client."""
import subprocess

from process_harness import ProcessTestCase, WIZARD_TOKEN
from process_harness import creature_package, resource


class CreatureTemplateProcesses(ProcessTestCase):
    def test_template_grant_loss_cancels_paid_work_and_round_trips_restart_rewind(self):
        package = creature_package(self, "template-arena", hostile=True,
                                   abilities=("power_strike",))
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('characters = [{ ',
                                'characters = [{ unselected = "ai", ai = "practice", ')
        source = source.replace('creature = { species = "human", name = "trainee"',
                                'creature = { species = "human", templates = ["arcane_trial"], name = "trainee"')
        # Warrior training leaves the template as the sole magical source.
        source = source.replace('source = "mage"', 'source = "warrior"', 1)
        source = source.replace(', { type = "magical" }', '', 1)
        source = source.replace('neutral = [], foe = ["neutral"]',
                                'neutral = ["foe"], foe = ["neutral"]')
        source = source.replace('ai_profiles = ',
                                'arena = { participants = [1, 2], control = "all_ai", start_paused = true, ticks = 200, actions = 100 }\nai_profiles = ')
        source += ('\n[creatures.templates.arcane_trial]\npriority = 10\n'
                   'grants = [{ type = "ability", ability = "fear" }, '
                   '{ type = "magical" }, { type = "health", amount = 5 }]\n')
        manifest.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        server = self.server(scenario=package, wizard=True)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        observer, original = self.client(WIZARD_TOKEN, observe=True)
        original_combat = original["state"]["observation"]["combat"]
        original_stats = original_combat["own_stats"]
        initial_focus = resource(original, "focus")["balance"]
        initial_mana = resource(original, "mana")["maximum"]
        self.assertGreater(initial_mana, 0)
        self.assertIn("fear", original_stats["abilities"])
        self.assertNotIn("Server error", wizard.command("wizard arena step"))
        prepared = self.frame(observer, lambda frame: frame.get("state") is not None
                              and resource(frame, "focus")["reserved"] == 1)
        self.assertTrue(prepared["state"]["observation"]["combat"]["preparation_active"])
        self.assertNotIn("Server error", wizard.command("wizard creature template 1 arcane_trial off"))
        removed = self.request(observer, {"type": "snapshot"})
        combat = removed["state"]["observation"]["combat"]
        self.assertNotIn("fear", combat["own_stats"]["abilities"])
        self.assertIn("power_strike", combat["own_stats"]["abilities"])
        self.assertEqual(combat["own_stats"]["hit_dice"], original_stats["hit_dice"])
        self.assertEqual(combat["own_stats"]["skills"], original_stats["skills"])
        self.assertFalse(combat["preparation_active"])
        self.assertEqual(combat["max_hp"], original_combat["max_hp"] - 5)
        self.assertEqual(resource(removed, "focus")["reserved"], 0)
        self.assertEqual(resource(removed, "focus")["balance"], initial_focus - 1)
        self.assertEqual(resource(removed, "mana")["maximum"], 0)
        self.assertEqual(int(removed["state"]["observation"]["tick"]), 0)
        self.assertIn("Server error", wizard.command("wizard creature template 1 unknown on"))
        rejected = self.request(observer, {"type": "snapshot"})
        self.assertEqual(rejected["state"], removed["state"])
        player, _ = self.client(observe=True)
        denied = self.command(player, {"type": "wizard", "command": "creature template 999 arcane_trial on"})
        self.assertIn("Wizard authority", denied["error"])
        player.stop()
        wizard.stop()
        observer.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        observer, restored = self.client(WIZARD_TOKEN, observe=True)
        self.assertEqual(restored["state"], removed["state"])
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard creature template 1 arcane_trial on"))
        reapplied = self.request(observer, {"type": "snapshot"})
        self.assertIn("fear", reapplied["state"]["observation"]["combat"]["own_stats"]["abilities"])
        self.assertEqual(resource(reapplied, "mana")["maximum"], initial_mana)
        self.assertEqual(resource(reapplied, "mana")["balance"], 0)
        self.assertEqual(resource(reapplied, "focus")["balance"], initial_focus - 1)
        self.assertNotIn("Server error", wizard.command("wizard rewind initial"))
        rewound = self.request(observer, {"type": "snapshot"})
        self.assertEqual(rewound["state"]["observation"]["combat"]["own_stats"], original_stats)

    def test_type_changes_preserve_injury_and_rederive_only_racial_health(self):
        package = creature_package(self, "injured-type-change", abilities=("power_strike",))
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('source = "warrior"', 'source = "racial"', 1)
        source = source.replace('archetypes = { ', 'archetypes = { injury = { class = "potion", name = "injury potion", consumable = { effects = [{ type = "damage", components = { vital = 3 } }] } }, ', 1)
        source += '\n[creatures.templates.undead_form]\npriority = 10\nkind = "undead"\n'
        manifest.write_text(source, encoding="utf-8", newline="\n")
        region = package / "regions/1.toml"
        source = region.read_text(encoding="utf-8")
        source += '\n[[items]]\nid = 3\nat = [1, 1, 0]\ncarried_by = 1\narchetype = "injury"\n'
        region.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        server = self.server(scenario=package, wizard=True)
        player, initial = self.client()
        original = initial["state"]["observation"]["combat"]
        potion = next(item["id"] for item in initial["state"]["observation"]["inventory"]
                      if item["class"] == "potion")
        self.assertIsNone(self.act(player, {"type": "drink", "item": potion})["error"])
        injured = self.request(player, {"type": "snapshot"})
        before = injured["state"]["observation"]["combat"]
        self.assertEqual(before["max_hp"] - before["hp"], 3)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard creature template 1 undead_form on"))
        transformed = self.request(player, {"type": "snapshot"})
        after = transformed["state"]["observation"]["combat"]
        self.assertEqual(after["own_stats"]["kind"], "undead")
        # The first racial die changes d8 -> d12; the retained Mage die is unchanged.
        self.assertEqual(after["max_hp"], before["max_hp"] + 4)
        self.assertEqual(after["hp"], before["hp"] + 4)
        self.assertEqual(after["own_stats"]["hit_dice"], before["own_stats"]["hit_dice"])
        self.assertEqual(after["own_stats"]["skills"], before["own_stats"]["skills"])
        self.assertEqual(transformed["state"]["observation"]["inventory"],
                         injured["state"]["observation"]["inventory"])
        player.stop()
        wizard.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        player, restored = self.client()
        self.assertEqual(restored["state"], transformed["state"])
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard creature template 1 undead_form off"))
        reverted = self.request(player, {"type": "snapshot"})["state"]["observation"]["combat"]
        self.assertEqual(reverted["own_stats"]["kind"], "humanoid")
        self.assertEqual(reverted["max_hp"], before["max_hp"])
        self.assertEqual(reverted["hp"], before["hp"])
        self.assertNotIn("Server error", wizard.command("wizard rewind initial"))
        rewound = self.request(player, {"type": "snapshot"})["state"]["observation"]["combat"]
        self.assertEqual(rewound["own_stats"], original["own_stats"])
        self.assertEqual(rewound["hp"], original["hp"])
