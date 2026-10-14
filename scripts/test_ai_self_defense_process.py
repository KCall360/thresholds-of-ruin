"""Ordinary autonomous self-defense, paid preparation and actual saved replay."""
import json
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import ProcessTestCase, WIZARD_TOKEN, creature_package, resource


class AiSelfDefenseProcesses(ProcessTestCase):
    def package(self, name):
        package = creature_package(self, name, hostile=True,
                                   abilities=("probe_defense",),
                                   ability_aliases={"probe_defense": ("basic_melee", "Probe Defense")})
        rules_path = package / "rules.toml"
        rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
        # Keep both actual payments observable throughout this 300-tick
        # lifecycle probe; natural recovery has separate kernel coverage.
        next(pool for pool in rules["resources"]
             if pool["resource"] == "stamina")["recovery_period"] = 1000
        rules["conditions"].append({
            "id": "probe_ward", "family": "probe_ward", "name": "Probe Ward",
            "lifecycle": "timed", "stacking": {"kind": "longest"},
            "source_scoped": False, "descriptors": [], "capacity": "reject",
            "modifiers": [{"scope": "always", "op": {
                "kind": "talent_reduction", "category": "impact", "amount": 2}}]})
        ability = next(entry for entry in rules["abilities"] if entry["id"] == "probe_defense")
        ability.update(invocation="actor_target", reach={"kind": "self_only"},
                       cost={"resource": "stamina", "start": 1, "resolution": 1},
                       timing={"source": {"kind": "fixed", "prepare": 100, "recover": 100}},
                       ai={"priority": 30, "hostile": False,
                           "minimum_resource_percent": 100, "range_preference": "any"},
                       cooldown=1000,
                       strikes={"kind": "authored", "strikes": [{"offset": 0,
                           "on_success": [{"kind": "apply_condition", "condition": "probe_ward", "duration": 1000}],
                           "on_failure": [], "descriptors": [], "riders": []}]})
        rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('characters = [{ ', 'characters = [{ unselected = "ai", ai = "practice", ')
        source = source.replace('ai_profiles = ', 'arena = { participants = [1, 2], control = "all_ai", start_paused = true, ticks = 300, actions = 100 }\nai_profiles = ')
        manifest.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        return package

    def test_autonomous_defense_uses_paid_self_condition_and_keeps_it_across_restart(self):
        package = self.package("paid-self-defense")
        server = self.server("--checkpoint-interval", 1, scenario=package, wizard=True)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=2)
        stamina = resource(initial, "stamina")["balance"]
        self.assertNotIn("Server error", wizard.command("wizard arena step 2"))
        prepared = self.frame(observer, lambda frame: frame.get("state") is not None
                              and resource(frame, "stamina")["reserved"] == 1)
        self.assertEqual(resource(prepared, "stamina")["balance"], stamina - 1)
        self.assertTrue(prepared["state"]["observation"]["combat"]["preparation_active"])
        self.assertIsNone(self.request(observer, {"type": "save"})["error"])
        wizard.stop()
        observer.stop()
        server.stop()
        self.server("--checkpoint-interval", 1, scenario=package, wizard=True)
        observer, restored = self.client(WIZARD_TOKEN, observe=True, actor=2)
        self.assertEqual(resource(restored, "stamina"), resource(prepared, "stamina"))
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard arena resume"))
        finished = self.frame(observer, lambda frame: frame.get("state") is not None
                             and int(frame["state"]["observation"]["tick"]) == 300)
        self.assertEqual(resource(finished, "stamina")["reserved"], 0)
        self.assertEqual(resource(finished, "stamina")["balance"], stamina - 2)
        report_client, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        ready = self.command(report_client, {"type": "wizard", "command": "creature inspect 2"})
        self.assertFalse(ready.get("error"), ready.get("error"))
        messages = [json.loads(line).get("message") for line in report_client.transcript if line.startswith("{")]
        report = next(message["report"] for message in reversed(messages)
                      if message and message["type"] == "creature_inspection")
        self.assertEqual([entry["condition"]["id"] for entry in report["conditions"]], ["probe_ward"])
