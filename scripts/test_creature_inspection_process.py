"""Private creature inspection through real Text, headless and native clients."""
import shutil
import subprocess
from process_harness import ProcessTestCase, ROOT, WIZARD_TOKEN
from process_harness import creature_package, resource


class CreatureInspectionProcesses(ProcessTestCase):
    def _private_report(self, client, operation, kind="combat_diagnostics"):
        import json
        start = len(client.transcript)
        ready = self.command(client, {"type": "wizard", "command": operation})
        self.assertFalse(ready.get("error"), ready.get("error"))
        messages = [json.loads(line).get("message")
                    for line in client.transcript[start:] if line.startswith("{")]
        reports = [message["report"] for message in messages if message and message["type"] == kind]
        self.assertEqual(len(reports), 1, "the command must return a fresh private report")
        return reports[0]

    def test_fear_immunity_and_resistance_keep_rng_and_payment_contracts(self):
        for condition in ("fear", "mind_affecting", "resistance"):
            with self.subTest(condition=condition):
                package = creature_package(self, "fear-" + condition, abilities=("fear",))
                self.save = self.directory / (condition + ".db")
                manifest = package / "scenario.toml"
                source = manifest.read_text(encoding="utf-8")
                if condition == "resistance":
                    grant = '{ type = "fear_difficulty", amount = -1000 }'
                else:
                    grant = ('{ type = "immunity", selector = { type = "descriptor", descriptor = "'
                             + condition + '" } }')
                source = source.replace('grants = [', 'grants = [' + grant + ', ', 1)
                manifest.write_text(source, encoding="utf-8", newline="\n")
                result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                        capture_output=True, text=True, encoding="utf-8", timeout=15)
                self.assertEqual(result.returncode, 0, result.stderr)
                server = self.server(scenario=package, wizard=True)
                wizard, _ = self.client(WIZARD_TOKEN, observe=True)
                player, initial = self.client()

                def query(operation):
                    return self._private_report(wizard, operation)

                query("combat capture on")
                target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                              if actor["name"] == "practice target")
                started = self.act(player, {"type": "use_ability", "ability": "fear", "target": target})
                self.assertIsNone(started["error"])
                self.assertEqual(resource(started, "focus")["balance"],
                                 resource(initial, "focus")["balance"] - 1)
                self.assertEqual(resource(started, "focus")["reserved"], 1)
                completed = self.frame(player, lambda frame: frame.get("state") is not None
                                       and frame["state"]["observation"]["ready"]
                                       and int(frame["state"]["observation"]["tick"]) > 0
                                       and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)
                elapsed = int(completed["state"]["observation"]["tick"])
                self.assertEqual(resource(completed, "focus")["reserved"], 0)
                self.assertEqual(resource(completed, "focus")["balance"],
                                 resource(initial, "focus")["balance"] - 2 + elapsed // 300)
                records = query("combat inspect")["records"]
                record = next(record for record in records if record["ability"] == "fear")
                self.assertFalse(record["applied"])
                self.assertEqual(record["damage"], 0)
                self.assertEqual(record["charge"], {"resource": "focus", "start": 1, "resolution": 1})
                self.assertEqual(record["target_before"]["fear"], [])
                self.assertEqual(record["target_after"]["fear"], [])
                self.assertEqual(record["target_after"]["health"], record["target_before"]["health"])
                steps = record["trace"]["steps"]
                checks = [step["check"] for step in steps if step["type"] == "check"]
                if condition == "resistance":
                    self.assertEqual(len(checks), 1)
                    self.assertEqual(checks[0]["skill"], "discipline")
                    self.assertTrue(checks[0]["success"])
                    self.assertIsNone(checks[0]["second"])
                    self.assertNotEqual(checks[0]["rng_before"], checks[0]["rng_after"])
                    self.assertNotIn("fear_immunity", [step["type"] for step in steps])
                else:
                    self.assertEqual(checks, [], "immunity must not draw resistance dice")
                    immunity = next(step for step in steps if step["type"] == "fear_immunity")
                    self.assertEqual(immunity, {"type": "fear_immunity", "fear": condition == "fear",
                                                "mind_affecting": condition == "mind_affecting"})
                self.assertEqual(steps[-1], {"type": "fear_finished", "applied": False, "duration": "0"})
                self.assertNotIn('"type":"combat_diagnostics"', "".join(player.transcript).replace(" ", ""))
                player.stop()
                wizard.stop()
                server.stop()

    def test_fear_disadvantage_survives_restart_and_immunity_clears_it(self):
        package = creature_package(self, "fear-edge", hostile=True, abilities=("fear",))
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('grants = [', 'grants = [{ type = "fear_difficulty", amount = 1000 }, ', 1)
        source += ('\n[creatures.species.fighter]\nkind = "humanoid"\n'
                   'attributes = { strength = 0, speed = 0, intellect = 0, '
                   'willpower = 0, awareness = 0, presence = 0 }\n'
                   'melee = { skill = "heavy_weaponry", bonus = 100, wind_up = 150, recovery = 40, '
                   'damage = { primary = { category = "impact" }, components = '
                   '[{ category = "impact", amount = { type = "fixed", value = 4 } }] } }\n'
                   'grants = [{ type = "health", amount = 200 }]\n'
                   '\n[creatures.templates.fearless]\npriority = 0\n'
                   'grants = [{ type = "immunity", selector = '
                   '{ type = "descriptor", descriptor = "fear" } }]\n')
        manifest.write_text(source, encoding="utf-8", newline="\n")
        region = package / "regions/1.toml"
        source = region.read_text(encoding="utf-8")
        region.write_text(source.replace('species = "human"', 'species = "fighter"', 1),
                          encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        server = self.server(scenario=package, wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        player, initial = self.client()

        def query(operation, kind="combat_diagnostics"):
            return self._private_report(wizard, operation, kind)

        query("combat capture on")
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                      if actor["name"] == "practice target")
        started = self.act(player, {"type": "use_ability", "ability": "fear", "target": target})
        self.assertIsNone(started["error"])
        completed = self.frame(player, lambda frame: frame.get("state") is not None
                               and frame["state"]["observation"]["ready"]
                               and int(frame["state"]["observation"]["tick"]) > 0
                               and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)
        records = query("combat inspect")["records"]
        fear = next(record for record in records if record["ability"] == "fear")
        self.assertTrue(fear["applied"])
        self.assertEqual(fear["target_after"]["fear"][0]["causer"], "1")
        attack = next(record for record in records if record["actor"] == "2" and record["target"] == "1")
        self.assertEqual(attack["actor_before"]["fear"][0]["causer"], "1")
        check = next(step["check"] for step in attack["trace"]["steps"] if step["type"] == "check")
        self.assertEqual((check["input_edge"], check["edge"], check["unused_edge"]), ("-1", "-1", "0"))
        self.assertIsNotNone(check["second"])
        self.assertEqual(check["kept"], min(check["first"], check["second"]))
        self.assertEqual(attack["damage"], 4)
        original = query("creature inspect 2", "creature_inspection")
        self.assertEqual(original["fear"][0]["causer"], "1")
        self.assertGreater(int(original["fear"][0]["remaining_ticks"]), 0)
        self.assertEqual(original["tick"], completed["state"]["observation"]["tick"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        wizard.stop()
        server.stop()
        self.server(scenario=package, wizard=True, seed=None)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        player, _ = self.client()
        self.assertEqual(query("creature inspect 2", "creature_inspection"), original)
        self.assertFalse(query("combat inspect")["enabled"])
        changed = self.command(wizard, {"type": "wizard", "command": "creature template 2 fearless on"})
        self.assertFalse(changed.get("error"), changed.get("error"))
        immune = query("creature inspect 2", "creature_inspection")
        self.assertEqual(immune["fear"], [])
        for field in ("tick", "hp", "max_hp", "injury", "hit_dice"):
            self.assertEqual(immune[field], original[field])
        changed = self.command(wizard, {"type": "wizard", "command": "creature template 2 fearless off"})
        self.assertFalse(changed.get("error"), changed.get("error"))
        self.assertEqual(query("creature inspect 2", "creature_inspection")["fear"], [])
        self.assertNotIn('"type":"combat_diagnostics"', "".join(player.transcript).replace(" ", ""))

    def test_fear_refresh_does_not_stack_and_expires_with_ordinary_waits(self):
        package = creature_package(self, "fear-refresh", abilities=("fear",))
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('grants = [', 'grants = [{ type = "fear_difficulty", amount = 1000 }, ', 1)
        manifest.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.server(scenario=package, wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        player, initial = self.client()

        def query(operation, kind="combat_diagnostics"):
            return self._private_report(wizard, operation, kind)

        query("combat capture on")
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                      if actor["name"] == "practice target")
        for _ in range(2):
            started = self.act(player, {"type": "use_ability", "ability": "fear", "target": target})
            self.assertIsNone(started["error"])
            self.frame(player, lambda frame: frame.get("state") is not None
                       and frame["state"]["observation"]["ready"]
                       and int(frame["state"]["observation"]["tick"]) > 0
                       and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)
        records = [record for record in query("combat inspect")["records"] if record["ability"] == "fear"]
        self.assertEqual(len(records), 2)
        first, second = records
        self.assertTrue(first["applied"])
        self.assertTrue(second["applied"])
        first_check = next(step["check"] for step in first["trace"]["steps"] if step["type"] == "check")
        second_check = next(step["check"] for step in second["trace"]["steps"] if step["type"] == "check")
        self.assertEqual(first_check["input_edge"], "0")
        self.assertIsNone(first_check["second"])
        self.assertEqual((second_check["input_edge"], second_check["edge"]), ("-1", "-1"))
        self.assertIsNotNone(second_check["second"])
        self.assertEqual(second_check["kept"], min(second_check["first"], second_check["second"]))
        self.assertEqual(first["target_before"]["fear"], [])
        self.assertEqual(len(second["target_before"]["fear"]), 1)
        previous = int(second["target_before"]["fear"][0]["remaining_ticks"])
        duration = int(first["target_after"]["fear"][0]["remaining_ticks"])
        self.assertGreater(previous, 0)
        self.assertLess(previous, duration)
        self.assertEqual(second["target_after"]["fear"], first["target_after"]["fear"])
        affected = query("creature inspect 2", "creature_inspection")
        deadline = int(affected["tick"]) + int(affected["fear"][0]["remaining_ticks"])
        for _ in range(8):
            if int(affected["tick"]) >= deadline:
                break
            waited = self.act(player, {"type": "wait"})
            self.assertIsNone(waited["error"])
            affected = query("creature inspect 2", "creature_inspection")
            if int(affected["tick"]) < deadline:
                self.assertEqual(affected["fear"], [{"causer": "1", "remaining_ticks":
                                                   str(deadline - int(affected["tick"]))}])
        self.assertEqual(int(affected["tick"]), deadline)
        self.assertEqual(affected["fear"], [])
        self.assertEqual(affected["hp"], affected["max_hp"])

    def test_natural_one_and_twenty_do_not_override_attack_totals(self):
        # These fixed seeds yield a first combat d20 of 1 and 20, respectively.
        # Fixed damage and independent Health streams consume no combat draws before it.
        for seed, bonus, expected_die, hit in ((29, 100, 1, True), (17, -100, 20, False)):
            with self.subTest(seed=seed, expected_die=expected_die):
                package = creature_package(self, "natural-roll-" + str(seed), abilities=("power_strike",))
                self.save = self.directory / ("natural-roll-" + str(seed) + ".db")
                manifest = package / "scenario.toml"
                source = manifest.read_text(encoding="utf-8")
                attack = (f'melee = {{ skill = "heavy_weaponry", bonus = {bonus}, wind_up = 60, recovery = 40, '
                          'damage = { primary = { category = "impact" }, components = '
                          '[{ category = "impact", amount = { type = "fixed", value = 1 } }] } }')
                source = "\n".join(attack if line.startswith("melee = ") else line
                                    for line in source.splitlines()) + "\n"
                manifest.write_text(source, encoding="utf-8", newline="\n")
                region = package / "regions/1.toml"
                source = region.read_text(encoding="utf-8").replace('name = "practice target",',
                         'name = "practice target", attributes = { strength = 5, speed = 4, '
                         'intellect = 3, willpower = 2, awareness = 1, presence = 0 },', 1)
                region.write_text(source, encoding="utf-8", newline="\n")
                result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                        capture_output=True, text=True, encoding="utf-8", timeout=15)
                self.assertEqual(result.returncode, 0, result.stderr)
                server = self.server(scenario=package, wizard=True, seed=seed)
                wizard, _ = self.client(WIZARD_TOKEN, observe=True)
                player, initial = self.client()
                before = self._private_report(wizard, "creature inspect 2", "creature_inspection")
                self.assertEqual(before["stats"]["defenses"], {"physical": 19, "cognitive": 15, "spiritual": 11})
                self._private_report(wizard, "combat capture on")
                target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                              if actor["name"] == "practice target")
                self.assertIsNone(self.act(player, {"type": "attack", "target": target})["error"])
                self.frame(player, lambda frame: frame.get("state") is not None
                           and frame["state"]["observation"]["ready"]
                           and int(frame["state"]["observation"]["tick"]) > 0
                           and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)
                records = self._private_report(wizard, "combat inspect")["records"]
                self.assertEqual(len(records), 1)
                record = records[0]
                checks = [step["check"] for step in record["trace"]["steps"] if step["type"] == "check"]
                self.assertEqual(len(checks), 1)
                check = checks[0]
                self.assertEqual((check["first"], check["kept"], check["second"]), (expected_die, expected_die, None))
                self.assertEqual((check["skill"], check["threshold"], check["modifier"]), ("heavy_weaponry", 19, bonus))
                self.assertEqual(int(check["total"]), expected_die + check["attribute_value"] + check["rank"] + bonus)
                self.assertEqual(check["success"], int(check["total"]) >= check["threshold"])
                self.assertEqual((check["success"], record["applied"], record["damage"]), (hit, hit, int(hit)))
                self.assertEqual(record["target_before"]["health"] - record["target_after"]["health"], int(hit))
                components = [step for step in record["trace"]["steps"] if step["type"] == "component"]
                self.assertEqual(len(components), int(hit), "misses skip the entire damage bundle")
                self.assertIsNone(record["charge"])
                player.stop()
                wizard.stop()
                server.stop()

    def test_resource_recovery_and_fear_pause_while_frozen_or_detached(self):
        for destination in (3, 4):
            with self.subTest(destination=destination):
                package = creature_package(self, "timer-streaming-" + str(destination), abilities=("fear",))
                self.save = self.directory / ("timer-" + str(destination) + ".db")
                manifest = package / "scenario.toml"
                source = manifest.read_text(encoding="utf-8")
                source = source.replace('grants = [', 'grants = [{ type = "fear_difficulty", amount = 1000 }, ', 1)
                source += ('\n[creatures.templates.capacity]\npriority = 0\n'
                           'grants = [{ type = "stamina", amount = 2 }, '
                           '{ type = "focus", amount = 2 }, { type = "mana", amount = 2 }]\n')
                manifest.write_text(source, encoding="utf-8", newline="\n")
                # Long authored rooms prevent sight/reach pins from keeping the subject active.
                # Default radii retain hall 1 frozen from hall 3, and detach it from hall 4.
                gallery = package / "regions/2.toml"
                text = gallery.read_text(encoding="utf-8").replace("size = [5, 3, 2]", "size = [32, 3, 2]")
                text = text.replace('anchors = { "landing" = [0, 1, 0] }',
                                    'anchors = { "landing" = [0, 1, 0], "exit" = [31, 1, 0] }')
                text = text.replace('portals = [', 'portals = [{ at = [31, 1, 0], direction = "east", '
                                    'to = "3/landing", height = 2 }, ', 1)
                gallery.write_text(text, encoding="utf-8", newline="\n")
                for region in (3, 4):
                    portals = ('{ at = [0, 1, 0], direction = "west", to = "'
                               + str(region - 1) + '/exit", height = 2 }')
                    if region == 3:
                        portals += ', { at = [31, 1, 0], direction = "east", to = "4/landing", height = 2 }'
                    text = (f'id = {region}\nname = "timer room {region}"\nsize = [32, 3, 2]\n'
                            'chamber = true\ngravity = [0, 0, -1]\n'
                            'anchors = { landing = [0, 1, 0], exit = [31, 1, 0] }\n'
                            'portals = [' + portals + ']\n')
                    (package / f"regions/{region}.toml").write_text(text, encoding="utf-8", newline="\n")
                result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                        capture_output=True, text=True, encoding="utf-8", timeout=15)
                self.assertEqual(result.returncode, 0, result.stderr)
                server = self.server(scenario=package, wizard=True)
                wizard, _ = self.client(WIZARD_TOKEN, observe=True)
                player, _ = self.client()

                def operation(command):
                    ready = self.command(wizard, {"type": "wizard", "command": command})
                    self.assertFalse(ready.get("error"), ready.get("error"))
                    return ready

                def inspect():
                    return self._private_report(wizard, "creature inspect 2", "creature_inspection")

                snapshot = self.request(player, {"type": "snapshot"})
                target = next(actor["id"] for actor in snapshot["state"]["observation"]["visible_actors"]
                              if actor["name"] == "practice target")
                self.assertIsNone(self.act(player, {"type": "use_ability", "ability": "fear", "target": target})["error"])
                self.frame(player, lambda frame: frame.get("state") is not None
                           and frame["state"]["observation"]["ready"]
                           and int(frame["state"]["observation"]["tick"]) > 0
                           and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)
                operation("creature template 2 capacity on")
                before = inspect()
                self.assertEqual(len(before["fear"]), 1)
                pools = {pool["resource"]: pool for pool in before["stats"]["resources"]}
                self.assertEqual(set(pools), {"stamina", "focus", "mana"})
                self.assertTrue(all(pool["maximum"] - pool["balance"] == 2 for pool in pools.values()))
                operation(f"teleport 1 {destination} 16 1 0")
                for restart in (False, True):
                    for _ in range(20):
                        waited = self.act(player, {"type": "wait"})
                        self.assertIsNone(waited["error"])
                    self.assertGreaterEqual(int(waited["state"]["observation"]["tick"]) - int(before["tick"]), 2000)
                    if destination == 3:
                        frozen = inspect()
                        for field in ("fear", "hp", "injury", "stats"):
                            self.assertEqual(frozen[field], before[field], field)
                    else:
                        unavailable = self.command(wizard, {"type": "wizard", "command": "creature inspect 2"})
                        self.assertIn("InvalidAction", unavailable["error"])
                        self.assertIn("unavailable", unavailable["error"])
                    if not restart:
                        self.assertIsNone(self.request(wizard, {"type": "save"})["error"])
                        player.stop()
                        wizard.stop()
                        server.stop()
                        server = self.server(scenario=package, wizard=True, seed=None)
                        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
                        player, _ = self.client()
                operation("teleport 1 1 1 1 0")
                thawed = inspect()
                for field in ("fear", "hp", "injury", "stats"):
                    self.assertEqual(thawed[field], before[field], field)
                thaw_tick = int(thawed["tick"])
                for _ in range(20):
                    self.assertIsNone(self.act(player, {"type": "wait"})["error"])
                    active = inspect()
                    elapsed = int(active["tick"]) - thaw_tick
                    for pool in active["stats"]["resources"]:
                        original = pools[pool["resource"]]
                        period = {"stamina": 100, "focus": 300, "mana": 1000}[pool["resource"]]
                        self.assertEqual(pool["balance"], min(original["maximum"], original["balance"] + elapsed // period))
                        self.assertEqual((pool["reserved"], pool["available"]), (0, pool["balance"]))
                    remaining = int(before["fear"][0]["remaining_ticks"]) - elapsed
                    self.assertEqual(active["fear"], [] if remaining <= 0 else
                                     [{"causer": "1", "remaining_ticks": str(remaining)}])
                self.assertGreaterEqual(elapsed, 2000)
                self.assertEqual(active["fear"], [])
                self.assertTrue(all(pool["balance"] == pool["maximum"] for pool in active["stats"]["resources"]))
                player.stop()
                wizard.stop()
                server.stop()

    def test_mixed_damage_protection_composes_once_and_survives_restart(self):
        cases = [
            ("reduction", [], (18, 15, 8)),
            ("cold-immune", [('descriptor', 'cold')], (12, 9, 3)),
            ("energy-immune", [('category', 'energy')], (4, 4, 3)),
            ("all-immune", [('category', 'energy'), ('category', 'keen')], (0, 0, 0)),
        ]
        attack = ('melee = { skill = "heavy_weaponry", bonus = 100, wind_up = 60, recovery = 40, '
                  'damage = { primary = { category = "energy", descriptor = "fire" }, components = ['
                  '{ category = "energy", descriptor = "fire", amount = { type = "fixed", value = 4 } }, '
                  '{ category = "keen", amount = { type = "fixed", value = 4 } }, '
                  '{ category = "energy", descriptor = "cold", amount = { type = "fixed", value = 6 } }, '
                  '{ category = "energy", descriptor = "fire", amount = { type = "fixed", value = 4 } }] } }')
        for name, immunities, totals in cases:
            with self.subTest(protection=name):
                package = creature_package(self, "mixed-" + name, abilities=("power_strike",))
                self.save = self.directory / (name + ".db")
                manifest = package / "scenario.toml"
                source = manifest.read_text(encoding="utf-8")
                source = "\n".join(attack if line.startswith("melee = ") else line
                                    for line in source.splitlines()) + "\n"
                source = source.replace('archetypes = { ', 'archetypes = { "warded_mail" = '
                                        '{ class = "armor", name = "warded mail", equipment = '
                                        '{ slot = "body_armor", defense = 0, reductions = { energy = 2 } } }, ', 1)
                grants = [
                    '{ type = "health", amount = 200 }',
                    '{ type = "reduction", selector = { type = "descriptor", descriptor = "fire" }, amount = 3 }',
                    '{ type = "reduction", selector = { type = "category", category = "energy" }, amount = 4 }',
                    '{ type = "reduction", selector = { type = "category", category = "keen" }, amount = 1 }',
                ]
                grants += ('{ type = "immunity", selector = { type = "' + kind + '", '
                           + kind + ' = "' + value + '" } }' for kind, value in immunities)
                source += ('\n[creatures.species.defender]\nkind = "humanoid"\n'
                           'attributes = { strength = 0, speed = 0, intellect = 0, '
                           'willpower = 0, awareness = 0, presence = 0 }\n'
                           'melee = { skill = "heavy_weaponry", bonus = 0, wind_up = 60, recovery = 40, '
                           'damage = { primary = { category = "impact" }, components = '
                           '[{ category = "impact", amount = { type = "fixed", value = 4 } }] } }\n'
                           'grants = [' + ", ".join(grants) + ']\n')
                manifest.write_text(source, encoding="utf-8", newline="\n")
                region = package / "regions/1.toml"
                source = region.read_text(encoding="utf-8").replace('species = "human"', 'species = "defender"', 1)
                source = source.replace('\n[[actors]]', '\nitems = [{ id = 30, at = [2,1,0], '
                                        'archetype = "warded_mail", carried_by = 2, equipped_slot = 2 }]\n\n[[actors]]', 1)
                region.write_text(source, encoding="utf-8", newline="\n")
                result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                        capture_output=True, text=True, encoding="utf-8", timeout=15)
                self.assertEqual(result.returncode, 0, result.stderr)
                server = self.server(scenario=package, wizard=True)
                wizard, _ = self.client(WIZARD_TOKEN, observe=True)
                player, initial = self.client()
                summaries = []
                previous_health = None
                for continuation in range(2):
                    if continuation:
                        self.assertIsNone(self.request(player, {"type": "save"})["error"])
                        player.stop()
                        wizard.stop()
                        server.stop()
                        server = self.server(scenario=package, wizard=True, seed=None)
                        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
                        player, initial = self.client()
                    self._private_report(wizard, "combat capture on")
                    target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                                  if actor["name"] == "practice target")
                    started = self.act(player, {"type": "attack", "target": target})
                    self.assertIsNone(started["error"])
                    self.frame(player, lambda frame: frame.get("state") is not None
                               and frame["state"]["observation"]["ready"]
                               and int(frame["state"]["observation"]["tick"]) > 0
                               and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)
                    records = self._private_report(wizard, "combat inspect")["records"]
                    self.assertEqual(len(records), 1)
                    record = records[0]
                    self.assertTrue(record["applied"])
                    self.assertEqual(record["damage"], totals[2])
                    self.assertIsNone(record["charge"])
                    if previous_health is not None:
                        self.assertEqual(record["target_before"]["health"], previous_health)
                    self.assertEqual(record["target_before"]["health"] - record["target_after"]["health"], totals[2])
                    previous_health = record["target_after"]["health"]
                    steps = record["trace"]["steps"]
                    components = [step["component"] for step in steps if step["type"] == "component"]
                    self.assertEqual(len(components), 3, "split Fire damage must merge before protection")
                    fire = components[0]
                    self.assertEqual((fire["category"], fire["descriptor"], fire["raw"]), ("energy", "fire", 8))
                    for component in components:
                        category_immune = name == "all-immune" or (
                            name == "energy-immune" and component["category"] == "energy")
                        descriptor_immune = name == "cold-immune" and component["descriptor"] == "cold"
                        self.assertEqual(component["category_immune"], category_immune)
                        self.assertEqual(component["descriptor_immune"], descriptor_immune)
                        self.assertEqual(component["after_immunity"],
                                         0 if category_immune or descriptor_immune else component["raw"])
                        self.assertIsNone(component["expression"])
                        self.assertEqual(component["rng_before"], component["rng_after"])
                        self.assertEqual(component["rolled"], [])
                        self.assertEqual(component["kept"], [])
                    reductions = [step for step in steps if step["type"] == "reduction"]
                    fire_reductions = [step for step in reductions if step["selector"] ==
                                       {"type": "descriptor", "descriptor": "fire"}]
                    self.assertEqual(len(fire_reductions), 1)
                    self.assertEqual(fire_reductions[0]["capacity"], 3)
                    energy = [step for step in reductions if step["selector"] == {"type": "category", "category": "energy"}]
                    self.assertEqual(len(energy), 1, "broad reduction applies once across Fire and Cold")
                    self.assertEqual(energy[0]["capacity"], 6, "source and equipped reduction must add")
                    self.assertEqual(energy[0]["before"], totals[1] - (0 if name == "all-immune" else 4))
                    self.assertEqual(energy[0]["after"], max(0, energy[0]["before"] - 6))
                    descriptor_positions = [i for i, step in enumerate(reductions) if step["selector"]["type"] == "descriptor"]
                    category_positions = [i for i, step in enumerate(reductions) if step["selector"]["type"] == "category"]
                    self.assertLess(max(descriptor_positions), min(category_positions))
                    summary = {"components": [{key: component[key] for key in
                                               ("category", "descriptor", "raw", "category_immune", "descriptor_immune", "after_immunity")}
                                              for component in components], "reductions": reductions, "finished": steps[-1]}
                    self.assertEqual(summary["finished"], {"type": "damage_finished", "raw": 18,
                                                          "after_immunity": totals[0], "after_descriptors": totals[1],
                                                          "total": totals[2], "unused_edge": "0"})
                    summaries.append(summary)
                self.assertEqual(*summaries)
                self.assertNotIn('"type":"combat_diagnostics"', "".join(player.transcript).replace(" ", ""))
                player.stop()
                wizard.stop()
                server.stop()

    def test_inspection_is_private_and_preserves_paid_preparation(self):
        package = self.directory / "inspection-arena"
        shutil.copytree(ROOT / "scenarios/mob-arena", package)
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('control = "manual"', 'control = "all_ai", start_paused = true', 1)
        manifest.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.server(scenario=package, wizard=True)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard arena step"))
        self.frame(observer, lambda frame: frame.get("state") is not None
                   and resource(frame, "focus")["reserved"] == 1)
        before = self.request(observer, {"type":"snapshot"})
        self.assertTrue(before["state"]["observation"]["combat"]["preparation_active"])
        output = wizard.command("wizard creature inspect 1")
        self.assertNotIn("Server error", output)
        for detail in ("blue adept", "human", "arcane", "Hit die 1", "power strike"):
            self.assertIn(detail, output)
        after = self.request(observer, {"type":"snapshot"})
        self.assertEqual(after["state"], before["state"])
        target = wizard.command("wizard creature inspect 2")
        self.assertNotIn("Server error", target)
        self.assertIn("blue sentinel", target)
        self.assertIn("hardiness", target.lower())
        player, _ = self.client(observe=True)
        for actor in (2, 999):
            denied = self.command(player, {"type":"wizard", "command":f"creature inspect {actor}"})
            self.assertIn("Wizard authority", denied["error"])

    def test_headless_exports_sources_and_restart_reconstructs_the_report(self):
        server = self.server(scenario="mob-arena", wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        ready = self.command(wizard, {"type": "wizard", "command": "creature inspect 2"})
        self.assertFalse(ready.get("error"), ready.get("error"))
        import json
        messages = [json.loads(line).get("message") for line in wizard.transcript if line.startswith("{")]
        report = next(message["report"] for message in reversed(messages)
                      if message and message["type"] == "creature_inspection")
        self.assertEqual(report["actor"], "2")
        self.assertEqual(report["species"]["id"], "human")
        self.assertEqual(report["hit_dice"][1]["talent"], "hardiness")
        self.assertIsInstance(report["hit_dice"][0]["health_seed"], str)
        player, _ = self.client(observe=True)
        denied = self.command(player, {"type": "wizard", "command": "creature inspect 2"})
        self.assertIn("Wizard authority", denied["error"])
        self.assertNotIn("health_seed", "".join(player.transcript))
        self.assertNotIn("grants", "".join(player.transcript))
        wizard.stop()
        player.stop()
        server.stop()
        self.server(scenario="mob-arena", wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        ready = self.command(wizard, {"type": "wizard", "command": "creature inspect 2"})
        self.assertFalse(ready.get("error"), ready.get("error"))
        messages = [json.loads(line).get("message") for line in wizard.transcript if line.startswith("{")]
        restored = next(message["report"] for message in reversed(messages)
                        if message and message["type"] == "creature_inspection")
        self.assertEqual(restored, report)


    def test_full_natural_attack_executes_and_survives_restart(self):
        import json
        import re
        package = self.directory / "mixed-natural-arena"
        shutil.copytree(ROOT / "scenarios/mob-arena", package)
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8").replace(
            'control = "manual"', 'control = "all_ai", start_paused = true', 1)
        source = source.replace('"heavy_weaponry"', '"light_weaponry"')
        attack = ('melee = { skill = "light_weaponry", bonus = 2, wind_up = 90, recovery = 70, '
                  'damage = { primary = { category = "energy", descriptor = "fire", sides = 6 }, '
                  'components = [{ category = "energy", descriptor = "fire", amount = '
                  '{ type = "rolled", count = 2, sides = 6, bonus = -1 } }, '
                  '{ category = "keen", amount = { type = "fixed", value = 3 } }] } }')
        source, count = re.subn(r"^melee = .*", attack + '\ngrants = [{ type = "health", amount = 200 }]',
                                source, flags=re.MULTILINE)
        self.assertEqual(count, 1)
        manifest.write_text(source, encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server(scenario=package, wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)

        def query(command, kind):
            ready = self.command(wizard, {"type": "wizard", "command": command})
            self.assertFalse(ready.get("error"), ready.get("error"))
            messages = [json.loads(line).get("message") for line in wizard.transcript if line.startswith("{")]
            return next(message["report"] for message in reversed(messages)
                        if message and message["type"] == kind)

        original = query("creature inspect 2", "creature_inspection")["species"]["melee"]
        self.assertEqual((original["skill"], original["bonus"], original["wind_up"], original["recovery"]),
                         ("light_weaponry", 2, 90, 70))
        self.assertEqual(original["damage"]["primary"]["descriptor"], "fire")
        self.assertEqual(len(original["damage"]["components"]), 2)
        text, _ = self.text_client(WIZARD_TOKEN, observe=True)
        output = text.command("wizard creature inspect 2")
        self.assertIn("check +2; wind-up 90; recovery 70", output)
        self.assertIn("Energy damage (Fire) 2d6-1", output)
        self.assertIn("Keen damage 3", output)
        query("combat capture on", "combat_diagnostics")
        start = len(wizard.transcript)
        ready = self.command(wizard, {"type": "wizard", "command": "arena step 32"})
        self.assertFalse(ready.get("error"), ready.get("error"))

        def completed(frame):
            message = frame.get("message") or {}
            return (message.get("type") == "waiting" and message.get("on") in ("paused", "stopped")
                    and frame.get("state") is not None
                    and int(frame["state"]["observation"]["tick"]) > 0)

        frames = [json.loads(line) for line in wizard.transcript[start:] if line.startswith("{")]
        if not any(completed(frame) for frame in frames):
            self.frame(wizard, completed)
        report = query("combat inspect", "combat_diagnostics")
        records = list(report["records"])
        while records and int(records[0]["sequence"]) > int(report["captured"]) - report["retained"] + 1:
            older = query(f'combat inspect {int(records[0]["sequence"]) - 1}', "combat_diagnostics")
            self.assertTrue(older["records"])
            records = older["records"] + records
        matching = []
        for record in records:
            steps = record["trace"]["steps"]
            if any(step["type"] == "check" and step["check"]["skill"] == "light_weaponry" for step in steps):
                components = [step["component"] for step in steps if step["type"] == "component"]
                if components:
                    matching.append(components)
        self.assertTrue(matching, "no resolved natural attack was captured")
        for components in matching:
            fire = next(component for component in components if component["category"] == "energy")
            self.assertEqual(fire["descriptor"], "fire")
            self.assertEqual(fire["expression"], {"count": 2, "sides": 6, "bonus": -1})
            keen = next(component for component in components if component["category"] == "keen")
            self.assertIsNone(keen["expression"])
            self.assertEqual(keen["raw"], 3)
        wizard.stop()
        text.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        self.assertEqual(query("creature inspect 2", "creature_inspection")["species"]["melee"], original)


    def test_equipped_weapon_discloses_full_source_and_resolves_its_own_skill(self):
        import json
        package = creature_package(self, "equipped-creature", abilities=("power_strike",))
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        blade = ('fire_blade = { class = "weapon", name = "fire blade", '
                 'equipment = { slot = "weapon", attack = { skill = "light_weaponry", bonus = 100, '
                 'wind_up = 90, recovery = 70, damage = { primary = { category = "energy", '
                 'descriptor = "fire", sides = 6 }, components = [{ category = "energy", '
                 'descriptor = "fire", amount = { type = "rolled", count = 2, sides = 6, bonus = -1 } }, '
                 '{ category = "keen", amount = { type = "fixed", value = 3 } }] } } } }, ')
        self.assertIn('archetypes = { ', source)
        source = source.replace('archetypes = { ', 'archetypes = { ' + blade, 1)
        manifest.write_text(source, encoding="utf-8", newline="\n")
        region = package / "regions/1.toml"
        source = ('items = [{ id = 100, at = [1,1,0], archetype = "fire_blade", carried_by = 1, '
                  'equipped_slot = 0 }]\n' + region.read_text(encoding="utf-8"))
        region.write_text(source, encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server(scenario=package, wizard=True)
        observer, initial = self.client(observe=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)

        def weapon(frame):
            return next(item["known_equipment"]["attack"]
                        for item in frame["state"]["observation"]["interactions"]["inventory"]
                        if item["equipped_slot"] == 0)

        declared = weapon(initial)
        self.assertEqual((declared["skill"], declared["bonus"], declared["wind_up"], declared["recovery"]),
                         ("light_weaponry", 100, 90, 70))
        self.assertEqual(declared["damage"]["primary"]["descriptor"], "fire")
        self.assertEqual(len(declared["damage"]["components"]), 2)

        def query(command):
            ready = self.command(wizard, {"type": "wizard", "command": command})
            self.assertFalse(ready.get("error"), ready.get("error"))
            messages = [json.loads(line).get("message") for line in wizard.transcript if line.startswith("{")]
            return next(message["report"] for message in reversed(messages)
                        if message and message["type"] == "combat_diagnostics")

        query("combat capture on")
        text, _ = self.adventure()
        for command in ["attack practice target", "power strike practice target"]:
            output = self.say(text, command)
            self.assertNotIn("Server error", output)
            snapshot = self.request(observer, {"type": "snapshot"})
            if not (snapshot["state"]["observation"]["ready"]
                    and snapshot["state"]["observation"]["combat"]["preparation_remaining"] is None):
                self.frame(observer, lambda frame: frame.get("state") is not None
                           and frame["state"]["observation"]["ready"]
                           and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)
        records = query("combat inspect")["records"]
        self.assertEqual(len(records), 2)
        for record in records:
            steps = record["trace"]["steps"]
            check = next(step["check"] for step in steps if step["type"] == "check")
            self.assertEqual((check["skill"], check["modifier"], check["rank"]), ("light_weaponry", 100, 0))
            components = [step["component"] for step in steps if step["type"] == "component"]
            fire = next(component for component in components if component["category"] == "energy")
            self.assertEqual(fire["descriptor"], "fire")
            self.assertEqual(fire["expression"], {"count": 2, "sides": 6, "bonus": -1})
            keen = next(component for component in components if component["category"] == "keen")
            self.assertIsNone(keen["expression"])
            self.assertEqual(keen["raw"], 3)
        paid = next(record for record in records if record["charge"] is not None)
        impact = next(step["component"] for step in paid["trace"]["steps"]
                      if step["type"] == "component" and step["component"]["category"] == "impact")
        self.assertEqual(impact["raw"], 3)
        text.stop()
        observer.stop()
        wizard.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        observer, restored = self.client(observe=True)
        self.assertEqual(weapon(restored), declared)


class CombatDiagnosticsProcesses(ProcessTestCase):
    def test_paid_resolution_private_query_paging_and_restart(self):
        import json
        package = self.directory / "diagnostic-arena"
        shutil.copytree(ROOT / "scenarios/mob-arena", package)
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8").replace(
            'control = "manual"', 'control = "all_ai", start_paused = true', 1)
        manifest.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        server = self.server(scenario=package, wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        player, _ = self.client(observe=True)

        def query(operation):
            ready = self.command(wizard, {"type": "wizard", "command": operation})
            self.assertFalse(ready.get("error"), ready.get("error"))
            messages = [json.loads(line).get("message") for line in wizard.transcript if line.startswith("{")]
            return next(message["report"] for message in reversed(messages)
                        if message and message["type"] == "combat_diagnostics")

        self.assertFalse(query("combat inspect")["enabled"])
        self.assertTrue(query("combat capture on")["enabled"])
        def step():
            start = len(wizard.transcript)
            ready = self.command(wizard, {"type": "wizard", "command": "arena step 32"})
            self.assertFalse(ready.get("error"), ready.get("error"))
            def completed(frame):
                message = frame.get("message") or {}
                return (message.get("type") == "waiting" and message.get("on") in ("paused", "stopped")
                        and frame.get("state") is not None
                        and int(frame["state"]["observation"]["tick"]) > 0)
            frames = [json.loads(line) for line in wizard.transcript[start:] if line.startswith("{")]
            if not any(completed(frame) for frame in frames):
                self.frame(wizard, completed)
        step()
        before = self.request(wizard, {"type": "snapshot"})
        report = query("combat inspect")
        self.assertGreater(int(report["captured"]), 0)
        self.assertTrue(report["records"])
        self.assertLessEqual(len(report["records"]), 8)
        self.assertTrue(any(record["charge"] for record in report["records"]))
        self.assertTrue(any(step["type"] == "check" for record in report["records"] for step in record["trace"]["steps"]))
        after = self.request(wizard, {"type": "snapshot"})
        self.assertEqual(after["state"], before["state"])
        denied = self.command(player, {"type": "wizard", "command": "combat capture off"})
        self.assertIn("Wizard authority", denied["error"])
        self.assertTrue(query("combat inspect")["enabled"])
        self.assertNotIn('"type":"combat_diagnostics"', "".join(player.transcript).replace(" ", ""))
        first = int(report["records"][0]["sequence"])
        self.assertGreater(first, int(report["dropped"]) + 1, "fixture must exercise backwards paging")
        older = query(f"combat inspect {first - 1}")
        self.assertEqual(int(older["records"][-1]["sequence"]), first - 1)
        first = int(older["records"][0]["sequence"])
        while first > int(report["dropped"]) + 1:
            older = query(f"combat inspect {first - 1}")
            first = int(older["records"][0]["sequence"])
        text, _ = self.text_client(WIZARD_TOKEN, observe=True)
        output = text.command("wizard combat inspect")
        self.assertIn("Combat diagnostics: capture on", output)
        self.assertIn("Record", output)
        self.assertIn("RNG", output)
        import sys
        from combat_diagnostics import verify_export
        def export(name):
            transcript = self.directory / f"{name}.jsonl"
            output = self.directory / f"{name}.json"
            transcript.write_text("\n".join(wizard.transcript) + "\n", encoding="utf-8", newline="\n")
            result = subprocess.run([sys.executable, ROOT / "scripts/combat_diagnostics.py", "export", transcript, output],
                                    capture_output=True, text=True, encoding="utf-8", timeout=15)
            self.assertEqual(result.returncode, 0, result.stderr)
            document = json.loads(output.read_text(encoding="utf-8"))
            self.assertTrue(verify_export(document))
            self.assertEqual(len(document["records"]), document["retained"])
            return output, document
        baseline_path, baseline = export("baseline")
        rewound = self.command(wizard, {"type": "wizard", "command": "rewind initial"})
        self.assertFalse(rewound.get("error"), rewound.get("error"))
        fresh = query("combat inspect")
        self.assertTrue(fresh["enabled"])
        self.assertEqual(fresh["records"], [])
        step()
        replay_report = query("combat inspect")
        first = int(replay_report["records"][0]["sequence"])
        while first > int(replay_report["dropped"]) + 1:
            older = query(f"combat inspect {first - 1}")
            first = int(older["records"][0]["sequence"])
        replay_path, replay = export("replay")
        self.assertNotEqual(baseline["sha256"], replay["sha256"], "rewind uses fresh payment owners")
        self.assertEqual(baseline["numerical_sha256"], replay["numerical_sha256"])
        compared = subprocess.run([sys.executable, ROOT / "scripts/combat_diagnostics.py", "compare", baseline_path, replay_path],
                                  capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(compared.returncode, 0, compared.stderr)
        text.stop()
        wizard.stop()
        player.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        restored = query("combat inspect")
        self.assertFalse(restored["enabled"])
        self.assertEqual(restored["records"], [])
        self.assertEqual(restored["captured"], "0")


class NativeCreatureInspectionProcesses(ProcessTestCase):
    graphical = True

    def test_native_wizard_console_presents_a_scrollable_readonly_report(self):
        self.server(scenario="mob-arena", wizard=True)
        wizard = self.launch("tor-client-ascii", ["--connect", self.address, "--observe", "--report-frames"], token=WIZARD_TOKEN)
        initial = self.ascii_frame(wizard, lambda frame: frame["state"] is not None and not frame["busy"])
        key = self.native_keys(wizard)
        key("F7", True)
        self.ascii_frame(wizard, lambda frame: frame.get("wizard_command") == "")
        key("F7", False)
        self.native_text(wizard, "creature inspect 2")
        self.ascii_frame(wizard, lambda frame: frame.get("wizard_command") == "creature inspect 2")
        key("Return", True)
        shown = self.ascii_frame(wizard, lambda frame: frame["screen"]["stats"] and not frame["busy"])
        key("Return", False)
        self.assertEqual(shown["state"], initial["state"])
        self.assertIn("blue sentinel", "\n".join(shown["stats_rows"]))
        self.assertIn("Hit die 1", "\n".join(shown["stats_rows"]))
        key("Down", True)
        self.ascii_frame(wizard, lambda frame: frame["screen"]["stats_scroll"] == 1)
        key("Down", False)
        key("Escape", True)
        self.ascii_frame(wizard, lambda frame: not frame["screen"]["stats"])
        key("Escape", False)
        key("F7", True)
        self.ascii_frame(wizard, lambda frame: frame.get("wizard_command") == "")
        key("F7", False)
        self.native_text(wizard, "combat capture on")
        self.ascii_frame(wizard, lambda frame: frame.get("wizard_command") == "combat capture on")
        key("Return", True)
        diagnostic = self.ascii_frame(wizard, lambda frame: frame["screen"]["stats"] and not frame["busy"])
        key("Return", False)
        self.assertEqual(diagnostic["state"], initial["state"])
        self.assertIn("Combat diagnostics", "\n".join(diagnostic["stats_rows"]))
        self.assertIn("capture on", "\n".join(diagnostic["stats_rows"]))
        wizard.stop()
