"""Shared rules overrides and catalog-only persistence through real applications."""
import json
import shutil
import subprocess

from process_harness import ProcessTestCase, ROOT
from ruleset_sources import load_rules


class RulesetProcesses(ProcessTestCase):
    def test_import_override_validation_rejects_ambiguous_or_invalid_content(self):
        package = self.directory / "invalid-import-overrides"
        shutil.copytree(ROOT / "scenarios/two-room", package)
        rules = package / "rules.toml"
        certificate = package / "validation.json"
        original = certificate.read_bytes()
        for source in [
            'import = "../general.toml"\n',
            'import = "interactions-v27"\n[[overrides]]\nop="set"\n'
            'path=["talents","keen_ward","id"]\nvalue="renamed"\n',
            'import = "interactions-v27"\n[[overrides]]\nop="set"\n'
            'path=["resources","mana","missing"]\nvalue=1\n',
            'import = "interactions-v27"\n[[overrides]]\nop="remove"\n'
            'path=["abilities","magic_bolt"]\n',
            'import = "interactions-v27"\n[[overrides]]\nop="set"\n'
            'path=["constants","base_defense"]\nvalue=12\n[[overrides]]\nop="set"\n'
            'path=["constants","base_defense"]\nvalue=13\n',
        ]:
            with self.subTest(source=source):
                rules.write_text(source, encoding="utf-8", newline="\n")
                result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                        capture_output=True, text=True, encoding="utf-8", timeout=20)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(certificate.read_bytes(), original)

    def test_data_only_talent_survives_restart_without_original_rules_file(self):
        package = self.directory / "data-rules"
        shutil.copytree(ROOT / "scenarios/two-room", package)
        rules = package / "rules.toml"
        rules.write_text(rules.read_text(encoding="utf-8") + '''
[[overrides]]
op = "add"
path = ["talents", "fixture_resilience"]
value = { id = "fixture_resilience", name = "Fixture resilience", prerequisites = [], requirement = { kind = "total_hd", minimum = 1 }, modifiers = [{ scope = "always", op = { kind = "add", stat = { kind = "health" }, value = 11 } }] }
''', encoding="utf-8", newline="\n")
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        build = ('creature = { species = "subject", name = "rules subject", '
                 'faction = "neutral", binding = "intellect", '
                 'hit_dice = [{ source = "racial", talent = "fixture_resilience" }] }, ')
        self.assertIn('characters = [{ ', source)
        source = source.replace('characters = [{ ', 'characters = [{ ' + build, 1)
        source += '''
[creatures.species.subject]
kind = "humanoid"
attributes = { strength = 1, speed = 1, intellect = 1, willpower = 1, awareness = 1, presence = 1 }
melee = { skill = "heavy_weaponry", bonus = 0, wind_up = 60, recovery = 40, damage = { primary = { category = "impact" }, components = [{ category = "impact", amount = { type = "fixed", value = 1 } }] } }
'''
        manifest.write_text(source, encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=20)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server(scenario=package)
        player, initial = self.client()
        stats = initial["state"]["observation"]["combat"]["own_stats"]
        self.assertIn("fixture_resilience", json.dumps(stats))
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        rules.unlink()
        # A supplied package configures a fresh save only. The existing save owns
        # its pinned rules and catalog, independent of this ordinary fallback.
        self.server(scenario=ROOT / "scenarios/two-room", seed=None)
        player, restored = self.client()
        self.assertEqual(restored["state"]["observation"]["combat"]["own_stats"], stats)
        player.stop()

    def test_data_only_condition_is_applied_inspected_and_restored(self):
        import tomllib
        from arena_matrix import toml_source
        from process_harness import WIZARD_TOKEN, creature_package
        package = creature_package(self, "marked", abilities=("mark_target",),
                                   ability_aliases={"mark_target": ("magic_bolt", "Mark Target")})
        rules_path = package / "rules.toml"
        rules = load_rules(rules_path, self.bin / ("tor-scenario" + self.suffix))
        rules["conditions"].append({
            "id": "marked", "name": "Marked", "modifiers": [
                {"scope": "versus_source", "op": {"kind": "add", "stat": {"kind": "edge"}, "value": -1}}],
            "stacking": {"kind": "longest"}, "source_scoped": True, "descriptors": [], "capacity": "oldest"})
        ability = next(value for value in rules["abilities"] if value["id"] == "mark_target")
        ability.pop("check", None)
        ability["on_success"] = [{"kind": "apply_condition", "condition": "marked", "duration": 1000}]
        ability["on_failure"] = []
        rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        manifest_path = package / "scenario.toml"
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["creatures"]["species"]["human"]["modifiers"].append({
            "scope": "always", "op": {"kind": "add", "stat": {
                "kind": "ability", "ability": "mark_target", "stat": "range"}, "value": 3}})
        manifest_path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=20)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server(scenario=package, wizard=True)
        player, initial = self.client()
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        disclosed = next(value for value in initial["state"]["observation"]["combat"]["own_stats"]["abilities"]
                         if value["id"] == "mark_target")
        self.assertEqual(disclosed["reach"], {"kind": "sight", "range": ability["reach"]["range"] + 3})
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                      if actor["name"] == "practice target")
        self.assertIsNone(self.act(player, {"type": "use_ability", "id": "mark_target", "target": target})["error"])
        self.frame(player, lambda frame: frame.get("state") is not None
                   and frame["state"]["observation"]["ready"]
                   and int(frame["state"]["observation"]["tick"]) > 0
                   and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)

        def inspect(client):
            offset = len(client.transcript)
            self.assertFalse(self.command(client, {"type": "wizard", "command": "creature inspect 2"}).get("error"))
            messages = [json.loads(line).get("message") for line in client.transcript[offset:] if line.startswith("{")]
            return next(message["report"] for message in reversed(messages)
                        if message and message["type"] == "creature_inspection")

        before = inspect(wizard)
        marked = next(value for value in before["conditions"] if value["condition"]["id"] == "marked")
        self.assertEqual(marked["condition"]["name"], "Marked")
        self.assertEqual(marked["source"], "1")
        self.assertGreater(int(marked["remaining_ticks"]), 0)
        self.assertTrue(any(source["category"] == "condition" and source["source"]["id"] == "marked"
                            for source in before["sources"]))
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        wizard.stop()
        server.stop()
        self.server(scenario=package, wizard=True, seed=None)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        self.assertEqual(inspect(wizard)["conditions"], before["conditions"])

    def test_data_only_item_timing_controls_all_preparation_durations(self):
        from arena_matrix import toml_source
        package = self.directory / "item-timing"
        shutil.copytree(ROOT / "scenarios/tests/interactions", package)
        path = package / "rules.toml"
        document = load_rules(path, self.bin / ("tor-scenario" + self.suffix))
        document["constants"]["item_timing"]["consume_turns"] = 2
        for slot in document["equipment_slots"]:
            slot["equip_turns"] = 4 if slot["id"] == "body_armor" else 2
            slot["unequip_turns"] = 5 if slot["id"] == "body_armor" else 3
        path.write_text(toml_source(document), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=20)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.server(scenario=package)
        player, initial = self.client()
        items = initial["state"]["observation"]["inventory"]
        sword = next(item["id"] for item in items if item["name"] == "sword")
        mail = next(item["id"] for item in items if item["name"] == "mail")
        potion = next(item["id"] for item in items if item["class"] == "potion")
        for action, tick in [
            ({"type": "unequip", "item": sword}, 300),
            ({"type": "equip", "item": sword, "slot": 0}, 500),
            ({"type": "equip", "item": mail, "slot": 1}, 900),
            ({"type": "unequip", "item": mail}, 1400),
            ({"type": "drink", "item": potion}, 1600),
        ]:
            with self.subTest(action=action):
                result = self.act(player, action)
                self.assertIsNone(result["error"])
                self.assertEqual(int(result["state"]["observation"]["tick"]), tick)
                self.assertEqual(result["intentions"], [])

    def test_data_only_weapon_tag_inherits_talent_damage_and_discloses_provenance(self):
        import tomllib
        from arena_matrix import toml_source
        from process_harness import WIZARD_TOKEN, creature_package
        package = creature_package(self, "tagged-weapon", abilities=("fixture_sweep", "untagged_sweep"),
                                   ability_aliases={"fixture_sweep": ("basic_melee", "Fixture sweep"),
                                                    "untagged_sweep": ("basic_melee", "Untagged sweep")})
        path = package / "rules.toml"
        rules = load_rules(path, self.bin / ("tor-scenario" + self.suffix))
        next(ability for ability in rules["abilities"] if ability["id"] == "untagged_sweep")["tags"] = []
        path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        manifest["characters"][0]["creature"]["hit_dice"][0]["talent"] = "heavy_blows"
        melee = manifest["creatures"]["species"]["human"]["melee"]
        melee["bonus"] = 1000
        melee["damage"] = {"primary": {"category": "impact"}, "components": [
            {"category": "impact", "amount": {"type": "fixed", "value": 4}}]}
        path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=20)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.server(scenario=package, wizard=True)
        player, initial = self.client()
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)

        def report(command, kind):
            offset = len(wizard.transcript)
            self.assertIsNone(self.command(wizard, {"type": "wizard", "command": command})["error"])
            messages = [json.loads(line).get("message") for line in wizard.transcript[offset:] if line.startswith("{")]
            return next(message["report"] for message in reversed(messages) if message and message["type"] == kind)

        def fields(step):
            return {field["id"]: field["value"]["value"] for field in step["fields"]}

        inspection = report("creature inspect 1", "creature_inspection")
        profiles = {fields(step)["ability"]["id"]: fields(step)
                    for step in inspection["selected_attack"]["abilities"]}
        self.assertEqual(int(profiles["fixture_sweep"]["flat_bonus"]), 1)
        self.assertEqual(int(profiles["untagged_sweep"]["flat_bonus"]), 0)
        source = next(source for source in inspection["sources"]
                      if source["category"] == "talent" and source["source"]["id"] == "heavy_blows")
        self.assertEqual(fields(source["rows"][0])["ability_tag"],
                         {"id": "weapon_attack", "name": "Weapon attack"})
        report("combat capture on", "combat_diagnostics")
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                      if actor["name"] == "practice target")
        self.assertIsNone(self.act(player, {"type": "use_ability", "id": "fixture_sweep", "target": target})["error"])
        self.frame(player, lambda frame: frame.get("state") is not None
                   and frame["state"]["observation"]["ready"]
                   and int(frame["state"]["observation"]["tick"]) > 0
                   and frame["state"]["observation"]["combat"]["preparation_remaining"] is None)
        record = next(record for record in report("combat inspect", "combat_diagnostics")["records"]
                      if record["ability"]["id"] == "fixture_sweep")
        self.assertEqual(sum(effect["amount"] for effect in record["outcome"]["effects"]
                             if effect["type"] == "damage"), 5)

    def test_data_only_custom_slot_equips_in_text_client_and_survives_restore(self):
        import tomllib
        from arena_matrix import toml_source
        package = self.directory / "tentacle-slots"
        shutil.copytree(ROOT / "scenarios/tests/interactions", package)
        rules_path = package / "rules.toml"
        rules = load_rules(rules_path, self.bin / ("tor-scenario" + self.suffix))
        rules["equipment_slots"].append({
            "id": "tentacle", "name": "Tentacle", "equipped_phrase": "held by a tentacle",
            "compatible_classes": ["tool"], "requires_attack": True,
            "equip_turns": 2, "unequip_turns": 4})
        rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        manifest["creatures"]["species"]["figure"]["anatomy"]["slots"][0] = "tentacle"
        manifest["archetypes"]["sword"]["class"] = "tool"
        manifest["archetypes"]["sword"]["equipment"]["slot"] = "tentacle"
        path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=20)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server("--checkpoint-interval", 1, scenario=package)
        probe, initial = self.client(observe=True)
        slot = initial["state"]["observation"]["interactions"]["slots"][0]
        self.assertEqual(slot, {"id": "tentacle", "name": "Tentacle",
                               "equipped_phrase": "held by a tentacle", "requires_attack": True})
        text, _ = self.text_client()
        self.assertIn("ItemStarted", text.command("remove sword"))
        self.assertIn("ItemStarted", text.command("wear sword"))
        snapshot = self.request(probe, {"type": "snapshot"})
        view = snapshot["state"]["observation"]
        self.assertEqual(int(view["tick"]), 600)
        sword = next(item["id"] for item in view["inventory"] if item["name"] == "sword")
        equipped = next(item for item in view["interactions"]["inventory"] if item["item"] == sword)
        self.assertEqual(equipped["slot"], slot)
        self.assertEqual(equipped["equipped_slot"], 0)
        self.assertIsNotNone(equipped["known_equipment"]["attack"])
        self.assertIsNone(self.request(probe, {"type": "save"})["error"])
        probe.stop()
        text.stop()
        server.stop()
        rules_path.unlink()
        self.server(scenario=ROOT / "scenarios/two-room", seed=None)
        _, restored = self.client(observe=True)
        self.assertEqual(restored["state"], snapshot["state"])

    def test_forged_ground_item_reference_rejects_restart_without_rewriting_save(self):
        import os
        import sqlite3
        import zlib
        from checkpoint_payload import HEADER, decode_checkpoint_payload
        from process_harness import TOKEN
        server = self.server("--checkpoint-interval", 1, scenario="interactions")
        player, initial = self.client()
        mail = next(item["id"] for item in initial["state"]["observation"]["inventory"]
                    if item["name"] == "mail")
        self.assertIsNone(self.act(player, {"type": "drop", "item": mail})["error"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        with sqlite3.connect(self.save) as db:
            payload = db.execute("SELECT payload FROM checkpoint").fetchone()[0]
            checkpoint = json.loads(decode_checkpoint_payload(payload))
            items = checkpoint["shared"]["items"][checkpoint["game"]["items"]]
            ground = next(item for item in items.values() if item["spec"]["name"] == "mail")
            self.assertIn("Ground", ground["location"])
            ground["spec"]["equipment"]["modifiers"].append({
                "scope": "always", "op": {"kind": "grant_ability", "ability": "missing_ground_ability"}})
            raw = json.dumps(checkpoint, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
            payload = HEADER + zlib.compress(raw, 1) + len(raw).to_bytes(4, "little")
            db.execute("UPDATE checkpoint SET payload = ?", (payload,))
        before = self.save.read_bytes()
        result = subprocess.run([self.bin / ("tor-server" + self.suffix), "--listen", "127.0.0.1:0",
                                 "--scenario", ROOT / "scenarios/tests/interactions", "--save", self.save],
                                capture_output=True, text=True, encoding="utf-8", timeout=15,
                                env={**os.environ, "TOR_SERVER_TOKEN": TOKEN})
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('"address"', result.stdout)
        self.assertIn("InvalidArchive", result.stderr)
        self.assertEqual(self.save.read_bytes(), before)
