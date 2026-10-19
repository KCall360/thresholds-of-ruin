"""Creature abilities through authored packages and real client processes."""
from scalar_wire import u64
from process_harness import content_ids, hit_die_counts, read_capture
import shutil
import subprocess

from process_harness import ProcessTestCase, ROOT, SPECTATOR_TOKEN, WIZARD_TOKEN, ascii_input_completion, creature_package, resource, settle_rogue_action



class CreatureAbilityProcesses(ProcessTestCase):
    def test_wide_aggregate_health_remains_exact_on_wire_and_source_free_restart(self):
        import json
        from status_condition_package import actor_creature, checkpoint

        package = creature_package(self, "wide-aggregate-health", abilities=())
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        old = 'stat = { kind = "health" }, value = 200'
        self.assertEqual(source.count(old), 1)
        manifest.write_text(source.replace(old, 'stat = { kind = "health" }, value = 1000000'),
                            encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server("--checkpoint-interval", 1, scenario=package, wizard=True)
        player, initial = self.client()
        combat = initial["state"]["observation"]["combat"]
        self.assertGreater(u64(combat["max_hp"]), 1_000_000)
        self.assertEqual(combat["hp"], combat["max_hp"])
        self.assertEqual(combat["own_stats"]["hit_dice"]["total"], "2")
        text, _ = self.adventure(SPECTATOR_TOKEN)
        displayed_health = f"HP {combat['hp']}/{combat['max_hp']}"
        self.assertIn(displayed_health, self.say(text, "look"))
        text.stop()
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        start = len(wizard.transcript)
        self.wizard_command(wizard, "creature inspect 1")
        messages = [json.loads(line).get("message") for line in wizard.transcript[start:]
                    if line.startswith("{")]
        report = next(message["report"] for message in messages
                      if message and message["type"] == "creature_inspection")
        self.assertEqual((report["hp"], report["max_hp"], report["injury"]),
                         (combat["hp"], combat["max_hp"], "0"))
        self.assertEqual([die["ordinal"] for die in report["hit_dice"]], ["1", "2"])
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        before = self.request(player, {"type": "snapshot"})
        saved = checkpoint(self, player)
        self.assertIs(type(actor_creature(saved)["injury"]), int)
        self.assertEqual(actor_creature(saved)["injury"], 0)
        wizard.stop(); player.stop(); server.stop()
        shutil.rmtree(package)
        self.server("--checkpoint-interval", 1, scenario="two-room", wizard=True, seed=None)
        player, restored = self.client()
        self.assertEqual(restored["state"], before["state"])
        self.assertEqual(checkpoint(self, player), saved)
        self.assertEqual(restored["state"]["observation"]["combat"]["hp"], combat["hp"])
        for frame in (initial, before, restored):
            view = frame["state"]["observation"]["combat"]
            self.assertGreater(u64(view["hp"]), 0)
            self.assertGreaterEqual(u64(view["max_hp"]), u64(view["hp"]))
            self.assertEqual(u64(view["own_stats"]["hit_dice"]["total"]), 2)
        text, _ = self.adventure(SPECTATOR_TOKEN)
        self.assertIn(displayed_health, self.say(text, "look"))
        text.stop()

    def test_full_seeded_rows_survive_for_pc_and_mob_with_bounded_ordinary_sheets(self):
        import json
        import tomllib
        from arena_matrix import toml_source

        package = creature_package(self, "exact-character-sheets", abilities=())
        manifest = package / "scenario.toml"
        source = tomllib.loads(manifest.read_text(encoding="utf-8"))
        source["characters"][0]["creature"]["hit_dice"] = [
            {"source": "warrior" if index % 2 == 0 else "racial"}
            for index in range(256)
        ]
        source["characters"][0]["creature"]["hit_dice"][1]["purchases"] = [
            {"pool": "advancement", "selection": {"kind": "skill_rank", "skill": "intimidation"}}
        ]
        manifest.write_text(toml_source(source), encoding="utf-8", newline="\n")
        region = package / "regions/1.toml"
        source = tomllib.loads(region.read_text(encoding="utf-8"))
        source["actors"][0]["creature"]["hit_dice"] = [{"source": "racial"} for _ in range(256)]
        source["actors"][0]["creature"]["hit_dice"][1]["purchases"] = [
            {"pool": "advancement", "selection": {"kind": "skill_rank", "skill": "medicine"}}
        ]
        region.write_text(toml_source(source), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)

        def inspect(client, actor):
            start = len(client.transcript)
            response = self.command(client, {"type": "wizard", "command": f"creature inspect {actor}"})
            self.assertFalse(response.get("error"), response.get("error"))
            messages = [json.loads(line).get("message") for line in client.transcript[start:]
                        if line.startswith("{")]
            return next(message["report"] for message in messages
                        if message and message["type"] == "creature_inspection")

        server = self.server(scenario=package, wizard=True)
        records = {}
        summaries = {}
        clients = []
        for actor in [1, 2]:
            client, initial = self.client(WIZARD_TOKEN, observe=True, actor=actor)
            clients.append(client)
            stats = initial["state"]["observation"]["combat"]["own_stats"]
            summaries[actor] = stats["hit_dice"]
            self.assertEqual(u64(stats["hit_dice"]["total"]), 256)
            self.assertEqual(hit_die_counts(stats["hit_dice"]),
                             {("class", "warrior"): 128, ("racial", "racial"): 128}
                             if actor == 1 else {("racial", "racial"): 256})
            self.assertNotIn("health_seed", json.dumps(stats))
            records[actor] = inspect(client, actor)["hit_dice"]
            self.assertEqual([u64(row["ordinal"]) for row in records[actor]], list(range(1, 257)))
            self.assertTrue(all("health_seed" in row and "base_health" in row and "purchases" in row
                                for row in records[actor]))
            self.assertEqual(len({row["health_seed"] for row in records[actor]}), 256)
            self.assertEqual(len(records[actor][1]["purchases"]), 1)
            self.assertTrue(any(row["base_health"] > 0 for row in records[actor]))
        for client in clients:
            client.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        for actor in [1, 2]:
            client, restored = self.client(WIZARD_TOKEN, observe=True, actor=actor)
            self.assertEqual(restored["state"]["observation"]["combat"]["own_stats"]["hit_dice"],
                             summaries[actor])
            self.assertEqual(inspect(client, actor)["hit_dice"], records[actor])

    def test_ordinary_sheet_summarizes_sources_while_explicit_inspection_retains_every_die(self):
        import json
        self.server(scenario="first-dungeon", wizard=True)
        observer, initial = self.client(observe=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        stats = initial["state"]["observation"]["combat"]["own_stats"]
        self.assertEqual(stats["hit_dice"], {"total": "4", "sources": [
            {"source_kind": "class", "source": {"id": "warrior", "name": "Warrior"}, "count": "4"}]})
        self.assertNotIn("health_seed", json.dumps(stats))
        self.assertNotIn("purchases", json.dumps(stats))
        self.assertFalse(self.command(wizard, {"type": "wizard", "command": "creature inspect 1"})["error"])
        messages = [json.loads(line).get("message") for line in wizard.transcript if line.startswith("{")]
        report = next(message["report"] for message in reversed(messages)
                      if message and message["type"] == "creature_inspection")
        self.assertEqual(report["stats"]["hit_dice"], stats["hit_dice"])
        self.assertEqual([u64(die["ordinal"]) for die in report["hit_dice"]], [1, 2, 3, 4])
        self.assertTrue(all("health_seed" in die and "purchases" in die for die in report["hit_dice"]))
        text, _ = self.text_client(observe=True)
        self.assertIn("Hit dice: 4 (4 Warrior)", text.command("stats"))
        self.assertEqual(self.request(observer, {"type": "snapshot"})["state"], initial["state"])

    def test_default_dungeon_build_and_equipped_source_survive_restart(self):
        import json
        server = self.server(scenario="first-dungeon", wizard=True)
        player, initial = self.client(observe=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        stats = initial["state"]["observation"]["combat"]["own_stats"]
        self.assertEqual(hit_die_counts(stats["hit_dice"]), {("class", "warrior"): 4})
        self.assertEqual(set(content_ids(stats["active_talents"])),
                         {"rattling_blow", "brace_for_impact", "measured_cut"})
        self.assertIn("rattling_blow", content_ids(stats["abilities"]))

        def weapon(frame):
            view = frame["state"]["observation"]
            blade = next(item["id"] for item in view["inventory"] if item["name"] == "iron greatsword")
            return next(item["known_equipment"]["attack"]
                        for item in view["interactions"]["inventory"] if item["item"] == blade)

        blade = next(item["id"] for item in initial["state"]["observation"]["inventory"]
                     if item["name"] == "iron greatsword")
        self.assertIsNone(next(item["known_equipment"] for item in
                              initial["state"]["observation"]["interactions"]["inventory"]
                              if item["item"] == blade))
        self.wizard_command(wizard, "identify 1 101")
        initial = self.request(player, {"type": "snapshot"})
        declared = weapon(initial)
        self.assertEqual(declared["skill"]["id"], "heavy_weaponry")
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
        self.assertEqual(source["natural_attack"]["source"]["id"], "human")
        self.assertEqual(source["natural_attack"]["strike_count"], 1)
        current = self.command(wizard, {"type": "inspect"})
        start = len(wizard.transcript)
        detail = self.request(wizard, {"type": "command", "context": current["input_context"],
            "branch": current["branch"], "command": {"type": "inspect_strike_source",
                "expected_revision": current["state"]["revision"], "actor": source["actor"],
                "target_revision": source["revision"], "source": {"kind": "natural"}, "cursor": None}})
        self.assertFalse(detail.get("error"), detail.get("error"))
        messages = [json.loads(line).get("message") for line in wizard.transcript[start:] if line.startswith("{")]
        page = next(message["page"] for message in messages
                    if message and message["type"] == "creature_strike_page")
        self.assertIsNone(page["next"])
        effect = next(entry["row"] for entry in page["entries"] if entry["kind"] == "success_effect")
        self.assertEqual(effect["id"], "damage")
        component = next(field for field in effect["fields"] if field["id"] == "primary_component_0")
        self.assertTrue(component["value"]["value"].startswith("Impact: "))
        self.assertEqual(len(source["hit_dice"]), 4)
        self.assertEqual(source["hit_dice"][3]["attribute"]["id"], "strength")
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
        source = source.replace('archetypes = { ', 'archetypes = { poison = { class = "potion", name = "test poison", consumable = { effects = [{ kind = "damage", damage = { kind = "components", components = [{ category = "vital", amount = { kind = "fixed", amount = 100000 } }] } }] } }, ')
        source = source.replace('ai_profiles = ', 'arena = { participants = [1, 2, 3], control = "manual", ticks = 700, actions = 100 }\nfactions = { neutral = ["foe"], foe = ["neutral"] }\nai_profiles = ')
        manifest.write_text(source, encoding="utf-8", newline="\n")
        region = package / "regions/1.toml"
        source = region.read_text(encoding="utf-8")
        source += ('\n[[actors]]\nid = 3\nat = [3, 1, 0]\ncontroller = "ai"\nai = "practice"\n'
                   'creature = { species = "human", name = "arena foe", faction = "foe", '
                   'binding = "intellect", starting_skills = ["athletics", "lore"], restoration = "authored", nutrition = { kind = "exempt" }, hit_dice = [{ source = "racial" }] }\n'
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
        self.assertEqual(u64(selected["state"]["observation"]["combat"]["hp"]), 0)
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

    def test_bundled_mob_arena_discloses_build_and_executes_paid_falter(self):
        package = ROOT / "scenarios/mob-arena"
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.server(scenario=package)
        player, initial = self.client()
        combat = initial["state"]["observation"]["combat"]
        self.assertEqual(u64(combat["own_stats"]["hit_dice"]["total"]), 3)
        self.assertTrue({"spark_lance_fire", "falter"}.issubset(content_ids(combat["own_stats"]["abilities"])))
        self.assertIn("rattling_blow", content_ids(combat["own_stats"]["abilities"]))
        self.assertNotIn("shell_impact", content_ids(combat["own_stats"]["abilities"]))
        enemies = initial["state"]["observation"]["visible_actors"]
        target = next(actor["id"] for actor in enemies if actor["name"] == "red sentinel")
        initial_mana = resource(initial, "mana")["balance"]
        # The adept's eye is one cell above the sentinel's single-cell body:
        # five horizontal cells are six sight-range cells, outside Falter's five.
        rejected = self.act(player, {"type": "use_ability", "id": "falter", "invocation": {"kind": "actor_target", "target": target}})
        self.assertEqual(rejected["error"], "InvalidAction: Intention is unavailable")
        self.assertEqual(rejected["state"], initial["state"])
        self.assertEqual(resource(rejected, "mana")["balance"], initial_mana)
        self.assertEqual(resource(rejected, "mana")["reserved"], 0)
        # Opponents can explore before acquiring an engagement. The sentinel
        # may move behind the arena wall, so close the distance and target a
        # currently disclosed foe instead of retaining its initial position.
        for _ in range(2):
            approached = settle_rogue_action(self, player, self.act(player, {"type": "move", "direction": "east"}))
            self.assertIsNone(approached["error"])
        observation = approached["state"]["observation"]
        hostile = {actor["actor"] for actor in observation["combat"]["actors"] if actor["hostile"]}
        visible = [actor for actor in observation["visible_actors"] if actor["id"] in hostile]
        self.assertTrue(visible, "approaching the bundled arena must disclose a foe")
        target = min(visible, key=lambda actor: (
            sum(abs(value) for value in actor["position"].values()),
            actor["position"]["x"], actor["position"]["y"], actor["position"]["z"], actor["name"],
        ))["id"]
        self.assertEqual(resource(approached, "mana")["balance"], initial_mana)
        started = self.act(player, {"type": "use_ability", "id": "falter", "invocation": {"kind": "actor_target", "target": target}})
        self.assertIsNone(started["error"])
        self.assertTrue(started["state"]["observation"]["combat"]["preparation_active"])
        self.assertEqual(resource(started, "mana")["balance"], initial_mana - 1)
        self.assertEqual(resource(started, "mana")["reserved"], 1)
        completed = settle_rogue_action(self, player, started)
        self.assertEqual(resource(completed, "mana")["reserved"], 0)
        self.assertEqual(resource(completed, "mana")["balance"], initial_mana - 2)

    def test_wizard_arena_steps_preserve_paid_preparation_across_restart(self):
        package = creature_package(self, "arena-step", hostile=True, abilities=("fixture_focus_fear",))
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
        self.check_owned_hit_die_preparation("falter", start_x=2, granted_hd=3)

    def test_wizard_preserves_paid_preparation_until_its_owned_hit_die_is_removed(self):
        self.check_owned_hit_die_preparation("spark_lance_fire", start_x=1, granted_hd=2)

    def check_owned_hit_die_preparation(self, ability, *, start_x, granted_hd):
        package = self.directory / "arena-hd"
        shutil.copytree(ROOT / "scenarios/mob-arena", package)
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        source = source.replace('control = "manual"', 'control = "all_ai", start_paused = true')
        manifest.write_text(source, encoding="utf-8", newline="\n")
        # Keep the bundled grants and AI priorities. The starting eye-to-body
        # distance selects Falter at five cells and Spark Lance at six cells.
        region = package / "regions/1.toml"
        source = region.read_text(encoding="utf-8")
        source = source.replace('start = [1, 2, 0]', f'start = [{start_x}, 2, 0]')
        region.write_text(source, encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        server = self.server(scenario=package, wizard=True)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        observer, original = self.client(WIZARD_TOKEN, observe=True)
        initial = original["state"]["observation"]["combat"]["own_stats"]
        self.assertEqual(u64(initial["hit_dice"]["total"]), 3)
        initial_mana = resource(original, "mana")["balance"]
        self.assertNotIn("Server error", wizard.command("wizard arena step"))
        prepared = self.frame(observer, lambda frame: frame.get("state") is not None
                              and resource(frame, "mana")["reserved"] == 1)
        self.assertTrue(prepared["state"]["observation"]["combat"]["preparation_active"])
        remaining_preparation = prepared["state"]["observation"]["combat"]["preparation_remaining"]
        self.assertIsNotNone(remaining_preparation)
        starts = [entry["content"]["event"]["ability"]["id"] for entry in prepared["history"]
                  if entry["content"].get("event", {}).get("type") == "ability_started"]
        self.assertEqual(starts, [ability])
        self.assertEqual(resource(prepared, "mana")["balance"], initial_mana - 1)
        for remaining in (2, 1, 0):
            output = wizard.command("wizard creature remove-hd 1")
            self.assertNotIn("Server error", output)
            changed = self.request(observer, {"type": "snapshot"})
            combat = changed["state"]["observation"]["combat"]
            self.assertEqual(u64(combat["own_stats"]["hit_dice"]["total"]), remaining)
            self.assertEqual(int(changed["state"]["observation"]["tick"]), 0)
            if remaining == 2:
                self.assertNotIn("falter", content_ids(combat["own_stats"]["abilities"]))
            if remaining >= granted_hd:
                self.assertIn(ability, content_ids(combat["own_stats"]["abilities"]))
                self.assertTrue(combat["preparation_active"])
                self.assertEqual(combat["preparation_remaining"], remaining_preparation)
                self.assertEqual(resource(changed, "mana")["reserved"], 1)
                self.assertEqual(resource(changed, "mana")["balance"], initial_mana - 1)
            else:
                self.assertNotIn(ability, content_ids(combat["own_stats"]["abilities"]))
                self.assertFalse(combat["preparation_active"])
                self.assertIsNone(combat["preparation_remaining"])
                self.assertEqual(resource(changed, "mana")["reserved"], 0)
                self.assertEqual(resource(changed, "mana")["balance"], initial_mana - 1)
        self.assertTrue(combat["dead"])
        wizard.stop()
        observer.stop()
        server.stop()
        self.server(scenario=package, wizard=True)
        observer, restored = self.client(WIZARD_TOKEN, observe=True)
        restored_combat = restored["state"]["observation"]["combat"]
        self.assertTrue(restored_combat["dead"])
        self.assertEqual(restored_combat["own_stats"]["hit_dice"], {"total": "0", "sources": []})
        self.assertFalse(restored_combat["preparation_active"])
        self.assertIsNone(restored_combat["preparation_remaining"])
        self.assertEqual(resource(restored, "mana")["reserved"], 0)
        self.assertEqual(resource(restored, "mana")["balance"], initial_mana - 1)
        wizard, _ = self.text_client(WIZARD_TOKEN, observe=True)
        self.assertNotIn("Server error", wizard.command("wizard rewind initial"))
        rewound = self.request(observer, {"type": "snapshot"})
        self.assertEqual(rewound["state"]["observation"]["combat"]["own_stats"], initial)
        self.assertFalse(rewound["state"]["observation"]["combat"]["dead"])
        self.assertFalse(rewound["state"]["observation"]["combat"]["preparation_active"])
        self.assertIsNone(rewound["state"]["observation"]["combat"]["preparation_remaining"])

    def test_ai_prepares_and_pays_for_each_granted_technique_through_normal_execution(self):
        for ability, pool, target_x in [("fixture_stamina_strike", "stamina", 2),
                                        ("fixture_mana_bolt", "mana", 4),
                                        ("fixture_focus_fear", "focus", 2)]:
            with self.subTest(ability=ability):
                package = creature_package(self, "ai-" + ability, hostile=True, abilities=(ability,), target_x=target_x)
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
                for _ in range(4 if ability == "fixture_stamina_strike" else 9):
                    self.assertIsNone(self.act(player, {"type": "wait"})["error"])
                completed = self.frame(observer, lambda frame: (
                    frame.get("state") is not None
                    and int(frame["state"]["observation"]["tick"]) >= (50 if ability == "fixture_stamina_strike" else 100)
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
        started = self.act(player, {"type": "use_ability", "id": ability, "invocation": {"kind": "actor_target", "target": target}})
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
        period = {"stamina": 100, "focus": 300, "mana": 800}[pool]
        self.assertEqual(resource(completed, pool)["balance"],
                         resource(initial, pool)["balance"] - 2 + elapsed // period)
        self.assertEqual(completed["state"]["observation"]["position"],
                         initial["state"]["observation"]["position"])

    def test_stamina_probe_pays_both_halves_and_recovers_normally(self):
        self.check_ability("fixture_stamina_strike", "stamina")

    def test_mana_probe_pays_both_halves_and_recovers_normally(self):
        self.check_ability("fixture_mana_bolt", "mana")

    def test_focus_probe_pays_both_halves_and_recovers_normally(self):
        self.check_ability("fixture_focus_fear", "focus")

    def test_text_commands_prepare_and_resolve_all_granted_abilities(self):
        package = creature_package(self, "text-creatures")
        self.server(scenario=package)
        observer, _ = self.client(observe=True)
        text, _ = self.adventure()
        for command, ability, pool in [
            ("fixture stamina strike", "Fixture Stamina Strike", "stamina"),
            ("fixture mana bolt", "Fixture Mana Bolt", "mana"),
            ("fixture focus fear", "Fixture Focus Fear", "focus"),
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
            period = {"stamina": 100, "focus": 300, "mana": 800}[pool]
            self.assertEqual(resource(after, pool)["balance"],
                             resource(before, pool)["balance"] - 2 + elapsed // period)

    def test_ruleset_defined_ability_name_executes_in_the_real_text_client(self):
        package = creature_package(self, "dynamic-text", abilities=("ember_touch",),
                                   ability_aliases={"ember_touch": ("fixture_mana_bolt", "Ember Touch")})
        self.server(scenario=package)
        observer, initial = self.client(observe=True)
        abilities = initial["state"]["observation"]["combat"]["own_stats"]["abilities"]
        self.assertIn("ember_touch", content_ids(abilities))
        self.assertEqual(next(value["name"] for value in abilities if value["id"] == "ember_touch"), "Ember Touch")
        text, _ = self.adventure()
        narration = self.say(text, "ember touch practice target")
        self.assertIn("You prepare to use Ember Touch.", narration)
        current = self.request(observer, {"type": "snapshot"})
        def resolved(frame):
            observation = (frame.get("state") or {}).get("observation") or {}
            return int(observation.get("tick", 0)) > int(initial["state"]["observation"]["tick"]) and observation.get("ready") and observation["combat"]["preparation_remaining"] is None
        current = current if resolved(current) else self.frame(observer, resolved)
        self.assertEqual(resource(current, "mana")["reserved"], 0)
        self.assertLess(resource(current, "mana")["balance"], resource(initial, "mana")["balance"])


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
        self.assertEqual(attacks[0]["ability"]["id"], "basic_melee")
        self.assertIsNone(attacks[0]["charge"])
        self.assertTrue(any(step["id"] == "check" for step in attacks[0]["trace"]["steps"]))

    def test_os_stats_and_ability_keys_preserve_turn_until_target_confirmation(self):
        package = creature_package(self, "os-ability-keys", abilities=("fixture_mana_bolt",))
        self.server(scenario=package)
        capture = self.directory / "native-stats-ability.ppm"
        window = self.launch("tor-client-ascii", ["--connect", self.address,
            "--report-frames", "--capture", capture])
        initial = self.ascii_frame(window, lambda f: f["has_control"] and not f["busy"])
        observer, observed = self.client(observe=True)
        self.assertEqual(observed["state"], initial["state"])
        native = self.native_keys(window)
        def press(name, reported):
            return self.native_press(window, native, name, reported)
        def title_pixels():
            pixels = read_capture(capture).split(b"\n", 3)[3]
            return b"".join(pixels[(y * 1200 + 72) * 3:(y * 1200 + 420) * 3]
                            for y in range(96, 112))
        normal_title = title_pixels()
        native("Shift_L", True)
        try:
            stats = press("2", "stats")
        finally:
            native("Shift_L", False)
        self.assertIn("Strength 2", "\n".join(stats["stats_rows"]))
        self.assertIn("Health recovery: 1 per 1160 active ticks", "\n".join(stats["stats_rows"]))
        self.assertTrue(stats["window_open"])
        title = title_pixels()
        self.assertNotEqual(title, normal_title, "stats title must reach the captured native framebuffer")
        self.assertIn(bytes((103, 216, 189)), [title[n:n + 3] for n in range(0, len(title), 3)])
        self.assertEqual(stats["state"], initial["state"])
        press("Escape", "escape")
        menu = press("z", "abilities")
        self.assertEqual(content_ids(menu["ability_choices"]), ["fixture_mana_bolt"])
        self.assertEqual(menu["state"], initial["state"])
        selected = press("Return", "enter")
        self.assertEqual(selected["selected_ability"]["id"], "fixture_mana_bolt")
        self.assertEqual(selected["state"], initial["state"])
        # Genuine native navigation exercises target selection without a turn.
        targeted = press("Up", "up")
        self.assertEqual(targeted["selected_ability"]["id"], "fixture_mana_bolt")
        self.assertEqual(targeted["state"], initial["state"])
        self.assertEqual(self.request(observer, {"type": "snapshot"})["state"], initial["state"])
        accepted = press("Return", "enter")
        def resolved(frame):
            observation = frame["state"]["observation"]
            return (int(observation["tick"]) > int(initial["state"]["observation"]["tick"])
                    and observation["ready"] and observation["combat"]["preparation_remaining"] is None)
        completed = accepted if resolved(accepted) else self.ascii_frame(window, resolved)
        authoritative = self.request(observer, {"type": "snapshot"})
        self.assertEqual(completed["state"], authoritative["state"])
        self.assertIn("fixture_mana_bolt", str(authoritative["history"]))
        self.assertEqual(resource(completed, "mana")["reserved"], 0)
        self.assertLess(resource(completed, "mana")["balance"], resource(initial, "mana")["balance"])
        self.assertTrue(completed["window_open"])

    def test_presented_stats_and_ability_target_selection_execute_shared_combat(self):
        package = creature_package(self, "native-creatures")
        self.server(scenario=package)
        window, initial = self.window()
        stats = self.key(window, "stats")
        self.assertTrue(stats["window_open"])
        self.assertIn("Strength 2", "\n".join(stats["stats_rows"]))
        self.assertIn("Health recovery: 1 per 1160 active ticks", "\n".join(stats["stats_rows"]))
        self.assertEqual(stats["state"], initial["state"])
        self.key(window, "escape")
        for ability, pool in [("fixture_stamina_strike", "stamina"), ("fixture_mana_bolt", "mana"), ("fixture_focus_fear", "focus")]:
            menu = self.key(window, "abilities")
            balance = resource(menu, pool)["balance"]
            self.assertEqual(set(content_ids(menu["ability_choices"])), {"fixture_stamina_strike", "fixture_mana_bolt", "fixture_focus_fear"})
            index = content_ids(menu["ability_choices"]).index(ability)
            tick = menu["state"]["observation"]["tick"]
            for _ in range(index):
                self.key(window, "down")
            selected = self.key(window, "enter")
            self.assertEqual(selected["selected_ability"]["id"], ability)
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
            period = {"stamina": 100, "focus": 300, "mana": 800}[pool]
            self.assertEqual(resource(completed, pool)["reserved"], 0)
            self.assertEqual(resource(completed, pool)["balance"], balance - 2 + elapsed // period)
            self.assertEqual(completed["state"]["observation"]["position"],
                             initial["state"]["observation"]["position"])
            self.assertIn(ability, str(completed["history"]))


    def test_ruleset_defined_ability_is_selected_from_the_real_native_menu(self):
        package = creature_package(self, "dynamic-native", abilities=("ember_touch",),
                                   ability_aliases={"ember_touch": ("fixture_mana_bolt", "Ember Touch")})
        self.server(scenario=package)
        window, initial = self.window()
        menu = self.key(window, "abilities")
        self.assertEqual(content_ids(menu["ability_choices"]), ["ember_touch"])
        self.assertEqual(menu["ability_choices"][0]["name"], "Ember Touch")
        selected = self.key(window, "enter")
        self.assertEqual(selected["selected_ability"]["id"], "ember_touch")
        completed = self.key(window, "enter")
        def resolved(frame):
            observation = frame["state"]["observation"]
            return int(observation["tick"]) > int(initial["state"]["observation"]["tick"]) and observation["ready"] and observation["combat"]["preparation_remaining"] is None
        completed = completed if resolved(completed) else self.ascii_frame(window, resolved)
        self.assertEqual(resource(completed, "mana")["reserved"], 0)
        self.assertLess(resource(completed, "mana")["balance"], resource(initial, "mana")["balance"])
