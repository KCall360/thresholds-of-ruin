"""Creature abilities through authored packages and real client processes."""
import shutil
import subprocess

from process_harness import ProcessTestCase, ROOT, WIZARD_TOKEN, ascii_input_completion, creature_package, resource



class CreatureAbilityProcesses(ProcessTestCase):
    def test_default_dungeon_build_and_equipped_source_survive_restart(self):
        import json
        server = self.server(scenario="first-dungeon", wizard=True)
        player, initial = self.client(observe=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        stats = initial["state"]["observation"]["combat"]["own_stats"]
        self.assertEqual(stats["hit_dice"], ["warrior"] * 4)
        self.assertEqual(set(stats["active_talents"]),
                         {"power_strike", "guard", "heavy_blows", "mighty_blows"})
        self.assertIn("power_strike", stats["abilities"])

        def weapon(frame):
            view = frame["state"]["observation"]
            blade = next(item["id"] for item in view["inventory"] if item["name"] == "iron greatsword")
            return next(item["known_equipment"]["attack"]
                        for item in view["interactions"]["inventory"] if item["item"] == blade)

        declared = weapon(initial)
        self.assertEqual(declared["skill"], "heavy_weaponry")
        self.assertEqual(declared["damage"]["primary"]["category"], "keen")
        self.assertEqual(declared["damage"]["components"][0]["amount"],
                         {"type": "rolled", "count": 2, "sides": 6, "bonus": 0})

        def inspect():
            ready = self.command(wizard, {"type": "wizard", "command": "creature inspect 1"})
            self.assertFalse(ready.get("error"), ready.get("error"))
            messages = [json.loads(line).get("message") for line in wizard.transcript if line.startswith("{")]
            return next(message["report"] for message in reversed(messages)
                        if message and message["type"] == "creature_inspection")

        source = inspect()
        self.assertEqual(source["species"]["id"], "human")
        self.assertEqual(source["species"]["melee"]["damage"]["primary"]["category"], "impact")
        self.assertEqual(len(source["hit_dice"]), 4)
        self.assertEqual(source["hit_dice"][3]["attribute"], "strength")
        self.assertEqual(self.request(player, {"type": "snapshot"})["state"]["observation"]["tick"],
                         initial["state"]["observation"]["tick"])
        player.stop()
        wizard.stop()
        server.stop()
        self.server(scenario="first-dungeon", wizard=True)
        player, restored = self.client(observe=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        self.assertEqual(restored["state"]["observation"]["combat"]["own_stats"], stats)
        self.assertEqual(weapon(restored), declared)
        self.assertEqual(inspect(), source)

    def test_all_ai_arena_stops_at_exact_tick_and_survives_server_restart(self):
        package = creature_package(self, "arena")
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('characters = [{ ', 'characters = [{ unselected = "ai", ai = "practice", ')
        source = source.replace('ai_profiles = ', 'arena = { participants = [1, 2], control = "all_ai", ticks = 7, actions = 100 }\nai_profiles = ')
        manifest.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        server = self.server(scenario=package, wizard=True)
        observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=1)
        stopped = initial if int(initial["state"]["observation"]["tick"]) == 7 else self.frame(
            observer, lambda frame: frame.get("state") is not None
            and int(frame["state"]["observation"]["tick"]) == 7)
        self.assertEqual(int(stopped["state"]["observation"]["tick"]), 7)
        observer.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        observer, restored = self.client(WIZARD_TOKEN, observe=True, actor=1)
        self.assertEqual(int(restored["state"]["observation"]["tick"]), 7)

    def test_remote_arena_pin_survives_checkpoint_restart_without_observing_the_mob(self):
        import json
        import sqlite3
        from checkpoint_payload import decode_checkpoint_payload
        package = creature_package(self, "remote-arena", abilities=())
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('characters = [{ ', 'characters = [{ unselected = "ai", ai = "practice", ')
        source = source.replace('ai_profiles = ', 'arena = { participants = [1, 2], control = "all_ai", start_paused = true, ticks = 100, actions = 1000 }\nai_profiles = ')
        manifest.write_text(source, encoding="utf-8", newline="\n")
        entry = package / "regions/1.toml"
        gallery = package / "regions/2.toml"
        source = entry.read_text(encoding="utf-8")
        actor = source[source.index("\n[[actors]]"):].replace('controller = "ai"',
            'controller = "ai"\nturn_ticks = 10\nbody = { cells = [[0,0,0],[0,0,1]], eye = [0,0,1], mass = 80 }')
        actor = actor.replace('at = [2, 1, 0]', 'at = [1, 1, 0]')
        # Disconnected, walled cells allow only Wait: neither wandering, sight,
        # reach nor ordinary region-neighbor loading can activate the remote mob.
        walls = [[x, y, z] for x in range(3) for y in range(3) for z in range(2) if (x, y) != (1, 1)]
        for region, path in ((1, entry), (2, gallery)):
            source = (f'id = {region}\nname = "arena cell {region}"\nsize = [3,3,2]\n'
                      'chamber = true\ngravity = [0,0,-1]\nanchors = { start = [1,1,0] }\n'
                      + 'walls = ' + json.dumps(walls) + '\n')
            path.write_text(source + (actor if region == 2 else ""), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server("--checkpoint-interval", 1, scenario=package, wizard=True)
        wizard, initial = self.client(WIZARD_TOKEN, observe=True, actor=1)
        self.assertFalse(any(actor["name"] == "practice target"
                             for actor in initial["state"]["observation"]["visible_actors"]))

        def checkpoint():
            self.assertIsNone(self.request(wizard, {"type": "save"})["error"])
            with sqlite3.connect(self.save) as db:
                sequence, payload = db.execute("SELECT sequence, payload FROM checkpoint").fetchone()
                self.assertEqual(db.execute("SELECT count(*) FROM journal WHERE sequence > ?", (sequence,)).fetchone()[0], 0)
            saved = json.loads(decode_checkpoint_payload(payload))
            actors = saved["shared"]["actors"][saved["game"]["actors"]]
            lifecycle = saved["shared"]["lifecycles"][saved["game"]["lifecycle"]]
            remote_points = [point for point in lifecycle["points"].values() if point["target"] == {"actor": 2}]
            self.assertEqual(remote_points, [{"target": {"actor": 2}, "active_radius": 0,
                                             "load_radius": 0, "observes": False}])
            self.assertEqual(set(actors), {"1", "2"})
            self.assertEqual(actors["2"]["location"]["region"], 2)
            return saved, actors

        self.assertFalse(self.command(wizard, {"type": "wizard", "command": "arena step 2"}).get("error"))
        for _ in range(10):
            saved, actors = checkpoint()
            if actors["2"]["ready_at"] == 10:
                break
        self.assertEqual(actors["1"]["ready_at"], 100)
        self.assertEqual(actors["2"]["ready_at"], 10)
        self.assertEqual(saved["game"]["tick"], 0)
        wizard.stop()
        server.stop()
        server = self.server("--checkpoint-interval", 1, scenario=package, wizard=True, seed=None)
        wizard, restored = self.client(WIZARD_TOKEN, observe=True, actor=1)
        self.assertEqual(int(restored["state"]["observation"]["tick"]), 0)
        restored_saved, restored_actors = checkpoint()
        self.assertEqual(restored_actors, actors)
        self.assertEqual(restored_saved["game"]["tick"], saved["game"]["tick"])
        self.assertFalse(self.command(wizard, {"type": "wizard", "command": "arena step"}).get("error"))
        # Remote-only work need not emit an observer delta. Inspect the saved
        # authoritative clock instead of adding a view that would pin region 2.
        for _ in range(10):
            saved, actors = checkpoint()
            if saved["game"]["tick"] == 10 and actors["2"]["ready_at"] == 20:
                break
        self.assertEqual(saved["game"]["tick"], 10)
        self.assertEqual(actors["1"]["ready_at"], 100)
        self.assertEqual(actors["2"]["ready_at"], 20)
        self.assertFalse(self.command(wizard, {"type": "wizard", "command": "arena resume"}).get("error"))
        stopped = self.frame(wizard, lambda frame: frame.get("state") is not None
                             and frame["state"]["observation"]["combat"]["terminal"])
        self.assertEqual(int(stopped["state"]["observation"]["tick"]), 100)
        _, actors = checkpoint()
        self.assertEqual(actors["2"]["ready_at"], 100)

    def test_manual_arena_survivors_continue_after_selected_death_and_restart(self):
        package = creature_package(self, "arena-selected-death")
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('archetypes = { ', 'archetypes = { poison = { class = "potion", name = "test poison", consumable = { effects = [{ type = "damage", components = { vital = 100000 } }] } }, ')
        source = source.replace('ai_profiles = ', 'arena = { participants = [1, 2, 3], control = "manual", ticks = 700, actions = 100 }\nfactions = { neutral = ["foe"], foe = ["neutral"] }\nai_profiles = ')
        manifest.write_text(source, encoding="utf-8", newline="\n")
        region = package / "regions/1.toml"
        source = region.read_text(encoding="utf-8")
        source += ('\n[[actors]]\nid = 3\nat = [3, 1, 0]\ncontroller = "ai"\nai = "practice"\n'
                   'creature = { species = "human", name = "arena foe", faction = "foe", '
                   'binding = "intellect", hit_dice = [{ source = "racial" }] }\n'
                   '\n[[items]]\nid = 3\nat = [1, 1, 0]\ncarried_by = 1\narchetype = "poison"\n')
        region.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        server = self.server(scenario=package, wizard=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        player, initial = self.client()
        potion = next(item["id"] for item in initial["state"]["observation"]["inventory"] if item["class"] == "potion")
        self.assertIsNone(self.act(player, {"type": "drink", "item": potion})["error"])
        stopped = self.frame(observer, lambda frame: frame.get("state") is not None
                             and int(frame["state"]["observation"]["tick"]) == 700)
        self.assertTrue(stopped["state"]["observation"]["combat"]["terminal"])
        self.assertFalse(stopped["state"]["observation"]["combat"]["dead"])
        selected = self.request(player, {"type": "snapshot"})
        self.assertTrue(selected["state"]["observation"]["combat"]["dead"])
        self.assertEqual(selected["state"]["observation"]["combat"]["hp"], 0)
        observer.stop()
        player.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        observer, restored = self.client(WIZARD_TOKEN, observe=True, actor=2)
        self.assertEqual(int(restored["state"]["observation"]["tick"]), 700)
        self.assertTrue(restored["state"]["observation"]["combat"]["terminal"])
        self.assertFalse(restored["state"]["observation"]["combat"]["dead"])
        selected, state = self.client(WIZARD_TOKEN, observe=True, actor=1)
        self.assertTrue(state["state"]["observation"]["combat"]["dead"])

    def test_bundled_mob_arena_discloses_build_and_executes_paid_fear(self):
        package = ROOT / "scenarios/mob-arena"
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.server(scenario=package)
        player, initial = self.client()
        combat = initial["state"]["observation"]["combat"]
        self.assertEqual(len(combat["own_stats"]["hit_dice"]), 3)
        self.assertTrue({"power_strike", "magic_bolt", "fear"}.issubset(combat["own_stats"]["abilities"]))
        enemies = initial["state"]["observation"]["visible_actors"]
        target = next(actor["id"] for actor in enemies if actor["name"] == "red sentinel")
        initial_focus = resource(initial, "focus")["balance"]
        started = self.act(player, {"type": "use_ability", "ability": "fear", "target": target})
        self.assertIsNone(started["error"])
        self.assertEqual(resource(started, "focus")["balance"], initial_focus - 1)
        self.assertEqual(resource(started, "focus")["reserved"], 1)
        completed = self.frame(player, lambda frame: frame.get("state") is not None
                               and int(frame["state"]["observation"]["tick"]) >= 100
                               and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)
        self.assertEqual(resource(completed, "focus")["reserved"], 0)
        self.assertEqual(resource(completed, "focus")["balance"], initial_focus - 2)

    def test_wizard_arena_steps_preserve_paid_preparation_across_restart(self):
        package = creature_package(self, "arena-step", hostile=True, abilities=("fear",))
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('characters = [{ ', 'characters = [{ unselected = "ai", ai = "practice", ')
        source = source.replace('ai_profiles = ', 'arena = { participants = [1, 2], control = "all_ai", start_paused = true, ticks = 200, actions = 100 }\nai_profiles = ')
        manifest.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        server = self.server("--checkpoint-interval", 1, scenario=package, wizard=True)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=2)
        clock, _ = self.client(WIZARD_TOKEN, observe=True, actor=1)
        player, public = self.client(observe=True, actor=1)
        denied = self.command(player, {"type": "wizard", "command": "arena resume"})
        self.assertIn("Wizard authority", denied["error"])
        initial_focus = resource(initial, "focus")["balance"]
        self.assertEqual(int(initial["state"]["observation"]["tick"]), 0)
        self.assertNotIn("Server error", wizard.command("wizard arena pause"))
        self.assertNotIn("Server error", wizard.command("wizard arena step"))
        self.frame(clock, lambda frame: any(
            entry.get("content", {}).get("type") == "action"
            for entry in frame.get("history", [])))
        unchanged = self.request(observer, {"type": "snapshot"})
        self.assertEqual(resource(unchanged, "focus")["balance"], initial_focus)
        public = self.request(player, {"type": "snapshot"})
        self.assertFalse(any(entry["content"]["type"] == "wizard" for entry in public["history"]))
        self.assertNotIn("Server error", wizard.command("wizard arena step 1"))
        prepared = self.frame(observer, lambda frame: frame.get("state") is not None
                              and resource(frame, "focus")["reserved"] == 1)
        self.assertEqual(int(prepared["state"]["observation"]["tick"]), 0)
        self.assertTrue(prepared["state"]["observation"]["combat"]["preparation_active"])
        self.assertEqual(resource(prepared, "focus")["balance"], initial_focus - 1)
        for _ in range(3):
            paused = self.request(observer, {"type": "snapshot"})
            self.assertEqual(int(paused["state"]["observation"]["tick"]), 0)
            self.assertEqual(resource(paused, "focus")["reserved"], 1)
        # Force the pending paid preparation into an actual checkpoint, rather
        # than proving only replay of a journal tail after the initial checkpoint.
        self.assertIsNone(self.request(observer, {"type": "save"})["error"])
        import sqlite3
        from checkpoint_payload import decode_checkpoint_payload
        with sqlite3.connect(self.save) as db:
            sequence, payload = db.execute("SELECT sequence, payload FROM checkpoint").fetchone()
            self.assertGreater(sequence, 0)
            self.assertEqual(db.execute("SELECT count(*) FROM journal WHERE sequence > ?", (sequence,)).fetchone()[0], 0)
        import json
        checkpoint = json.loads(decode_checkpoint_payload(payload))
        self.assertTrue(any(actors.get("2", {}).get("pending") is not None
                            for actors in checkpoint["shared"]["actors"]))
        wizard.stop()
        clock.stop()
        player.stop()
        observer.stop()
        server.stop()
        self.server("--checkpoint-interval", 1, scenario=package, wizard=True)
        observer, restored = self.client(WIZARD_TOKEN, observe=True, actor=2)
        self.assertEqual(int(restored["state"]["observation"]["tick"]), 0)
        self.assertTrue(restored["state"]["observation"]["combat"]["preparation_active"])
        self.assertEqual(resource(restored, "focus")["balance"], initial_focus - 1)
        self.assertEqual(resource(restored, "focus")["reserved"], 1)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard arena resume"))
        stopped = self.frame(observer, lambda frame: frame.get("state") is not None
                            and int(frame["state"]["observation"]["tick"]) == 200)
        self.assertEqual(resource(stopped, "focus")["balance"], initial_focus - 2)
        self.assertEqual(resource(stopped, "focus")["reserved"], 0)

    def test_wizard_removes_owned_hit_dice_and_restores_them_after_rewind(self):
        package = self.directory / "arena-hd"
        shutil.copytree(ROOT / "scenarios/mob-arena", package)
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('control = "manual"', 'control = "all_ai", start_paused = true')
        manifest.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        server = self.server(scenario=package, wizard=True)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        observer, original = self.client(WIZARD_TOKEN, observe=True)
        initial = original["state"]["observation"]["combat"]["own_stats"]
        self.assertEqual(len(initial["hit_dice"]), 3)
        initial_focus = resource(original, "focus")["balance"]
        self.assertNotIn("Server error", wizard.command("wizard arena step"))
        prepared = self.frame(observer, lambda frame: frame.get("state") is not None
                              and resource(frame, "focus")["reserved"] == 1)
        self.assertTrue(prepared["state"]["observation"]["combat"]["preparation_active"])
        for remaining in (2, 1, 0):
            output = wizard.command("wizard creature remove-hd 1")
            self.assertNotIn("Server error", output)
            changed = self.request(observer, {"type": "snapshot"})
            combat = changed["state"]["observation"]["combat"]
            self.assertEqual(len(combat["own_stats"]["hit_dice"]), remaining)
            self.assertEqual(int(changed["state"]["observation"]["tick"]), 0)
            if remaining == 2:
                self.assertNotIn("fear", combat["own_stats"]["abilities"])
                self.assertFalse(combat["preparation_active"])
                self.assertEqual(resource(changed, "focus")["reserved"], 0)
                self.assertEqual(resource(changed, "focus")["balance"], initial_focus - 1)
        self.assertTrue(combat["dead"])
        wizard.stop()
        observer.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        observer, restored = self.client(WIZARD_TOKEN, observe=True)
        self.assertTrue(restored["state"]["observation"]["combat"]["dead"])
        self.assertEqual(restored["state"]["observation"]["combat"]["own_stats"]["hit_dice"], [])
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard rewind initial"))
        rewound = self.request(observer, {"type": "snapshot"})
        self.assertEqual(rewound["state"]["observation"]["combat"]["own_stats"], initial)
        self.assertFalse(rewound["state"]["observation"]["combat"]["dead"])

    def test_ai_prepares_and_pays_for_each_granted_technique_through_normal_execution(self):
        for ability, pool in [("power_strike", "stamina"), ("magic_bolt", "mana"), ("fear", "focus")]:
            with self.subTest(ability=ability):
                package = creature_package(self, "ai-" + ability, hostile=True, abilities=(ability,))
                self.save = self.directory / (ability + ".db")
                server = self.server(scenario=package, wizard=True)
                observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=2)
                player, _ = self.client()
                self.assertIsNone(self.act(player, {"type": "wait"})["error"])
                started = self.frame(observer, lambda frame: (
                    frame.get("state") is not None
                    and frame["state"]["observation"]["combat"]["preparation_active"]))
                self.assertEqual(resource(started, pool)["balance"], resource(initial, pool)["balance"] - 1)
                self.assertEqual(resource(started, pool)["reserved"], 1)
                # Short player waits expose an actual preparation frame before
                # the simulation fast-forwards through the AI's completion.
                for _ in range(4 if ability == "power_strike" else 9):
                    self.assertIsNone(self.act(player, {"type": "wait"})["error"])
                completed = self.frame(observer, lambda frame: (
                    frame.get("state") is not None
                    and int(frame["state"]["observation"]["tick"]) >= (50 if ability == "power_strike" else 100)
                    and frame["state"]["observation"]["combat"]["preparation_remaining"] is None))
                self.assertEqual(resource(completed, pool)["reserved"], 0)
                self.assertEqual(resource(completed, pool)["balance"], resource(initial, pool)["balance"] - 2)
                observer.stop()
                player.stop()
                server.stop()

    def check_ability(self, ability, pool):
        package = creature_package(self, ability)
        self.server(scenario=package)
        player, initial = self.client()
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                      if actor["name"] == "practice target")
        started = self.act(player, {"type": "use_ability", "ability": ability, "target": target})
        self.assertIsNone(started["error"])
        self.assertTrue(started["state"]["observation"]["combat"]["preparation_active"])
        self.assertEqual(resource(started, pool)["balance"], resource(initial, pool)["balance"] - 1)
        self.assertEqual(resource(started, pool)["reserved"], 1)
        completed = self.frame(player, lambda frame: (
            frame.get("state") is not None
            and frame["state"]["observation"]["ready"]
            and int(frame["state"]["observation"]["tick"]) > 0
            and frame["state"]["observation"]["combat"]["preparation_remaining"] is None))
        self.assertEqual(resource(completed, pool)["reserved"], 0)
        elapsed = int(completed["state"]["observation"]["tick"])
        period = {"stamina": 100, "focus": 300, "mana": 1000}[pool]
        self.assertEqual(resource(completed, pool)["balance"],
                         resource(initial, pool)["balance"] - 2 + elapsed // period)
        self.assertEqual(completed["state"]["observation"]["position"],
                         initial["state"]["observation"]["position"])

    def test_power_strike_pays_both_halves_and_recovers_normally(self):
        self.check_ability("power_strike", "stamina")

    def test_magic_bolt_pays_both_halves_and_recovers_normally(self):
        self.check_ability("magic_bolt", "mana")

    def test_fear_pays_both_halves_and_recovers_normally(self):
        self.check_ability("fear", "focus")

    def test_text_commands_prepare_and_resolve_all_granted_abilities(self):
        package = creature_package(self, "text-creatures")
        self.server(scenario=package)
        observer, _ = self.client(observe=True)
        text, _ = self.adventure()
        for command, ability, pool in [
            ("power strike", "power strike", "stamina"),
            ("magic bolt", "magic bolt", "mana"),
            ("fear", "fear", "focus"),
        ]:
            before = self.request(observer, {"type": "snapshot"})
            tick = int(before["state"]["observation"]["tick"])
            narration = self.say(text, command + " practice target")
            self.assertIn("You prepare to use " + ability + ".", narration)
            current = self.request(observer, {"type": "snapshot"})

            def completed(frame):
                observation = (frame.get("state") or {}).get("observation") or {}
                return (int(observation.get("tick", 0)) > tick
                        and observation.get("ready")
                        and observation["combat"]["preparation_remaining"] is None)

            after = current if completed(current) else self.frame(observer, completed)
            self.assertEqual(resource(after, pool)["reserved"], 0)
            elapsed = int(after["state"]["observation"]["tick"]) - tick
            period = {"stamina": 100, "focus": 300, "mana": 1000}[pool]
            self.assertEqual(resource(after, pool)["balance"],
                             resource(before, pool)["balance"] - 2 + elapsed // period)


class NativeCreatureAbilityProcesses(ProcessTestCase):
    graphical = True

    def test_native_bump_attack_uses_owned_melee_without_paid_technique_costs(self):
        import json
        package = creature_package(self, "native-bump", hostile=True, abilities=())
        manifest = package / "scenario.toml"
        manifest.write_text(manifest.read_text(encoding="utf-8").replace(
            'factions = { neutral = [], foe = ["neutral"] }',
            'factions = { neutral = ["foe"], foe = ["neutral"] }'), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.server(scenario=package, wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        self.assertFalse(self.command(wizard, {"type": "wizard", "command": "combat capture on"}).get("error"))
        capture = self.directory / "native-bump.ppm"
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames", "--capture", capture])
        initial = self.ascii_frame(window, lambda frame: frame["has_control"] and not frame["busy"])
        self.assertTrue(initial["window_open"])
        native = self.native_keys(window)
        native("Right", True)
        acknowledged = self.ascii_frame(window, ascii_input_completion("right"))
        native("Right", False)
        def resolved(frame):
            observation = (frame.get("state") or {}).get("observation")
            return (observation is not None
                    and int(observation["tick"]) > int(initial["state"]["observation"]["tick"])
                    and observation["combat"]["preparation_remaining"] is None
                    and observation["ready"])
        completed = acknowledged if resolved(acknowledged) else self.ascii_frame(window, resolved)
        self.assertTrue(completed["window_open"])
        self.assertTrue(capture.exists())
        self.assertEqual(completed["state"]["observation"]["position"], initial["state"]["observation"]["position"])
        self.assertGreater(int(completed["state"]["observation"]["tick"]), int(initial["state"]["observation"]["tick"]))
        for pool in ("stamina", "focus", "mana"):
            self.assertEqual(resource(completed, pool)["balance"], resource(initial, pool)["balance"])
            self.assertEqual(resource(completed, pool)["reserved"], 0)
        offset = len(wizard.transcript)
        self.assertFalse(self.command(wizard, {"type": "wizard", "command": "combat inspect"}).get("error"))
        messages = [json.loads(line).get("message") for line in wizard.transcript[offset:] if line.startswith("{")]
        reports = [message["report"] for message in messages if message and message["type"] == "combat_diagnostics"]
        self.assertEqual(len(reports), 1)
        attacks = [record for record in reports[0]["records"] if record["actor"] == "1"]
        self.assertEqual(len(attacks), 1)
        self.assertEqual(attacks[0]["ability"], "basic_melee")
        self.assertIsNone(attacks[0]["charge"])
        self.assertTrue(any(step["type"] == "check" and step["check"]["skill"] == "heavy_weaponry"
                            for step in attacks[0]["trace"]["steps"]))

    def test_presented_stats_and_ability_target_selection_execute_shared_combat(self):
        package = creature_package(self, "native-creatures")
        self.server(scenario=package)
        window, initial = self.window()
        stats = self.key(window, "stats")
        self.assertTrue(stats["window_open"])
        self.assertIn("Strength 2", "\n".join(stats["stats_rows"]))
        self.assertEqual(stats["state"], initial["state"])
        self.key(window, "escape")
        for index, ability in enumerate(["power_strike", "magic_bolt", "fear"]):
            menu = self.key(window, "abilities")
            pool = ["stamina", "mana", "focus"][index]
            balance = resource(menu, pool)["balance"]
            self.assertEqual(menu["ability_choices"], ["power_strike", "magic_bolt", "fear"])
            tick = menu["state"]["observation"]["tick"]
            for _ in range(index):
                self.key(window, "down")
            selected = self.key(window, "enter")
            self.assertEqual(selected["selected_ability"], ability)
            self.assertEqual(selected["state"]["observation"]["tick"], tick)
            accepted = self.key(window, "enter")
            completed = accepted if (
                int(accepted["state"]["observation"]["tick"]) > int(tick)
                and accepted["state"]["observation"]["ready"]
                and accepted["state"]["observation"]["combat"]["preparation_remaining"] is None
            ) else self.ascii_frame(window, lambda frame: (
                int(frame["state"]["observation"]["tick"]) > int(tick)
                and frame["state"]["observation"]["ready"]
                and frame["state"]["observation"]["combat"]["preparation_remaining"] is None))
            self.assertTrue(completed["window_open"])
            elapsed = int(completed["state"]["observation"]["tick"]) - int(tick)
            period = {"stamina": 100, "focus": 300, "mana": 1000}[pool]
            self.assertEqual(resource(completed, pool)["reserved"], 0)
            self.assertEqual(resource(completed, pool)["balance"], balance - 2 + elapsed // period)
            self.assertEqual(completed["state"]["observation"]["position"],
                             initial["state"]["observation"]["position"])
            self.assertIn(ability, str(completed["history"]))
