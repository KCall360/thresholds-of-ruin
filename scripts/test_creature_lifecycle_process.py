"""Shared creature lifecycle application through the real server and headless client."""
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import ProcessTestCase, WIZARD_TOKEN, content_ids, creature_package


class CreatureLifecycleProcesses(ProcessTestCase):
    def test_losing_a_free_grant_cancels_work_and_survives_restart_and_rewind(self):
        package = creature_package(self, "free-grant-lifecycle", hostile=True,
                                   abilities=("power_strike",))
        rules_path = package / "rules.toml"
        rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
        next(ability for ability in rules["abilities"] if ability["id"] == "fear").pop("cost")
        rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('characters = [{ ',
                                'characters = [{ unselected = "ai", ai = "practice", ')
        source = source.replace('creature = { species = "human", name = "trainee"',
                                'creature = { species = "human", templates = ["borrowed_grant"], name = "trainee"')
        source = source.replace('neutral = [], foe = ["neutral"]',
                                'neutral = ["foe"], foe = ["neutral"]')
        source = source.replace('ai_profiles = ',
                                'arena = { participants = [1, 2], control = "all_ai", start_paused = true, ticks = 200, actions = 100 }\nai_profiles = ')
        source += ('\n[creatures.templates.borrowed_grant]\npriority = 10\n'
                   'modifiers = [{ scope = "always", op = { kind = "grant_ability", ability = "fear" } }]\n')
        manifest.write_text(source, encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server(scenario=package, wizard=True)
        wizard = self.wizard()
        observer, initial = self.client(WIZARD_TOKEN, observe=True)
        self.wizard_command(wizard, "arena step")
        prepared = self.request(observer, {"type": "snapshot"})
        self.assertTrue(prepared["state"]["observation"]["combat"]["preparation_active"])
        self.wizard_command(wizard, "creature template 1 borrowed_grant off")
        removed = self.request(observer, {"type": "snapshot"})
        combat = removed["state"]["observation"]["combat"]
        self.assertNotIn("fear", content_ids(combat["own_stats"]["abilities"]))
        self.assertIsNone(combat["preparation_remaining"])
        self.assertFalse(combat["preparation_active"])
        self.assertEqual(removed["state"]["observation"]["tick"], initial["state"]["observation"]["tick"])
        observer.stop()
        wizard.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        observer, restored = self.client(WIZARD_TOKEN, observe=True)
        self.assertEqual(restored["state"], removed["state"])
        wizard = self.wizard()
        self.wizard_command(wizard, "rewind initial")
        rewound = self.request(observer, {"type": "snapshot"})
        self.assertEqual(rewound["state"]["observation"]["combat"]["own_stats"],
                         initial["state"]["observation"]["combat"]["own_stats"])
