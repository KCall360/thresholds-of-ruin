"""Active-time Health recovery through real server, headless and text clients."""
import json
import shutil
import subprocess
import tomllib

from process_harness import ProcessTestCase, ROOT, WIZARD_TOKEN, creature_package


class HealthRecoveryProcesses(ProcessTestCase):
    def package(self):
        package = creature_package(self, "health-recovery", abilities=())
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('archetypes = { ', 'archetypes = { injury = { class = "potion", name = "injury potion", consumable = { effects = [{ kind = "damage", damage = { kind = "components", components = [{ category = "vital", amount = { kind = "fixed", amount = 3 } }] } }] } }, ', 1)
        source = source.replace('bonus = 0, wind_up = 60', 'bonus = 100, wind_up = 60')
        manifest.write_text(source, encoding="utf-8", newline="\n")
        region = package / "regions/1.toml"
        source = region.read_text(encoding="utf-8").replace('controller = "ai"', 'controller = "ai"\nturn_ticks = 10000')
        source += '\n[[items]]\nid = 3\nat = [1, 1, 0]\ncarried_by = 1\narchetype = "injury"\nquantity = 1\n'
        region.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        return package

    @staticmethod
    def combat(frame):
        return frame["state"]["observation"]["combat"]

    def completed(self, player, frame):
        self.assertIsNone(frame["error"])

        def ready(candidate):
            observation = (candidate.get("state") or {}).get("observation", {})
            return observation.get("ready") and observation["combat"]["preparation_remaining"] is None

        return frame if ready(frame) else self.frame(player, ready)

    def test_authored_species_template_and_equipment_recovery_survives_source_free_reopen(self):
        from arena_matrix import toml_source
        package = self.package()
        path = package / "scenario.toml"
        document = tomllib.loads(path.read_text(encoding="utf-8"))

        def modifier(kind, value):
            return {"scope": "always", "op": {"kind": "add", "stat": {"kind": kind}, "value": value}}

        document["creatures"]["species"]["human"]["modifiers"].append(modifier("health_recovery_period", 40))
        document["creatures"].setdefault("templates", {})["health_care"] = {
            "priority": 0, "modifiers": [modifier("health_recovery_period_percent", -25)]}
        document["characters"][0]["creature"]["templates"] = ["health_care"]
        document["archetypes"]["recovery_cap"] = {
            "class": "armor", "name": "recovery cap", "equipment": {
                "slot": "head_armor", "modifiers": [modifier("health_recovery_reset_percent", -50)]}}
        path.write_text(toml_source(document), encoding="utf-8", newline="\n")
        region = package / "regions/1.toml"
        source = region.read_text(encoding="utf-8")
        source += '\n[[items]]\nid = 4\nat = [1, 1, 0]\ncarried_by = 1\nequipped_slot = 3\narchetype = "recovery_cap"\n'
        region.write_text(source, encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=20)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server("--checkpoint-interval", 1, scenario=package, wizard=True)
        player, initial = self.client()
        recovery = self.combat(initial)["own_stats"]["health_recovery"]
        self.assertEqual(recovery, {"period_ticks": "900", "elapsed_ticks": "0",
                                    "stopped": False, "reset_percent": 50})
        potion = next(item["id"] for item in initial["state"]["observation"]["inventory"] if item["class"] == "potion")
        self.completed(player, self.act(player, {"type": "drink", "item": potion}))
        for _ in range(4):
            before = self.completed(player, self.act(player, {"type": "wait"}))
        progress = int(self.combat(before)["own_stats"]["health_recovery"]["elapsed_ticks"])
        self.assertGreater(progress, 0)
        self.assertLess(progress, 900)
        self.assertEqual(self.combat(before)["max_hp"] - self.combat(before)["hp"], 3)
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        shutil.rmtree(package)
        self.server(scenario=ROOT / "scenarios/two-room", seed=None, wizard=True)
        player, restored = self.client()
        self.assertEqual(restored["state"], before["state"])
        start = int(restored["state"]["observation"]["tick"])
        for _ in range(30):
            rested = self.completed(player, self.act(player, {"type": "wait"}))
            tick = int(rested["state"]["observation"]["tick"])
            recovered = min(3, (progress + tick - start) // 900)
            self.assertEqual(self.combat(rested)["hp"], self.combat(before)["hp"] + recovered)
            if recovered == 3:
                break
        self.assertEqual(self.combat(rested)["hp"], self.combat(rested)["max_hp"])

    def test_rest_recovery_and_damage_reset_are_disclosed_and_survive_reconnect_restart(self):
        package = self.package()
        server = self.server("--checkpoint-interval", 1, scenario=package, wizard=True)
        player, initial = self.client()
        wizard = self.wizard()
        period = int(self.combat(initial)["own_stats"]["health_recovery"]["period_ticks"])
        self.assertEqual(period, 1160)
        self.assertNotIn("health", [pool["resource"]["id"] for pool in self.combat(initial)["own_stats"]["resources"]])
        potion = next(item["id"] for item in initial["state"]["observation"]["inventory"] if item["class"] == "potion")
        injured = self.completed(player, self.act(player, {"type": "drink", "item": potion}))
        self.assertIsNone(injured["error"])
        health = self.combat(injured)["hp"]
        self.assertEqual(self.combat(injured)["max_hp"] - health, 3)
        for _ in range(5):
            rested = self.completed(player, self.act(player, {"type": "wait"}))
            self.assertIsNone(rested["error"])
        before = self.combat(rested)["own_stats"]["health_recovery"]
        self.assertGreater(int(before["elapsed_ticks"]), 400)
        self.wizard_command(wizard, "teleport 2 1 2 1 0")
        ready = self.request(player, {"type": "snapshot"})
        target = next(actor["id"] for actor in ready["state"]["observation"]["visible_actors"] if actor["name"] == "practice target")
        attacked = self.completed(player, self.act(player, {"type": "use_ability", "id": "basic_melee", "target": target}))
        self.assertIsNone(attacked["error"])
        after = self.combat(attacked)["own_stats"]["health_recovery"]
        self.assertLess(int(after["elapsed_ticks"]), int(before["elapsed_ticks"]))
        self.assertEqual(self.combat(attacked)["hp"], health)
        offset = len(wizard.transcript)
        self.wizard_command(wizard, "creature inspect 1")
        messages = [json.loads(line).get("message") for line in wizard.transcript[offset:]
                    if line.startswith("{")]
        reports = [message["report"] for message in messages
                   if message and message["type"] == "creature_inspection"]
        self.assertEqual(len(reports), 1)
        self.assertEqual(reports[0]["stats"]["health_recovery"], after)
        self.assertEqual(reports[0]["injury"], 3)
        self.assertEqual(self.combat(self.request(player, {"type": "snapshot"})), self.combat(attacked))
        text, _ = self.text_client(WIZARD_TOKEN, observe=True)
        output = text.command("stats")
        self.assertIn(f"Health recovery: 1 per {period} active ticks", output)
        text.stop()
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        player, reconnected = self.client()
        self.assertEqual(self.combat(reconnected), self.combat(attacked))
        player.stop()
        wizard.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        player, restarted = self.client()
        self.assertEqual(self.combat(restarted), self.combat(attacked))
        elapsed = int(after["elapsed_ticks"])
        start = int(restarted["state"]["observation"]["tick"])
        for _ in range(40):
            rested = self.completed(player, self.act(player, {"type": "wait"}))
            self.assertIsNone(rested["error"])
            tick = int(rested["state"]["observation"]["tick"])
            expected = min(3, (elapsed + tick - start) // period)
            self.assertEqual(self.combat(rested)["hp"], health + expected)
            if expected == 3:
                break
        self.assertEqual(self.combat(rested)["hp"], self.combat(rested)["max_hp"])
        self.assertEqual(self.combat(rested)["own_stats"]["health_recovery"]["elapsed_ticks"], "0")
