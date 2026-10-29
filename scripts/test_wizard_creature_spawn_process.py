"""Named authored creatures, private discovery, ordinary stock and source-free saves."""
import json
import re
import shutil
import subprocess
import tomllib
import unittest

from arena_matrix import toml_source
from process_harness import (ProcessTestCase, ROOT, WIZARD_TOKEN, SPECTATOR_TOKEN,
                             ascii_stats_viewport_text, non_graphical)


class WizardCreatureSpawnProcesses(ProcessTestCase):
    graphical = True

    def package(self, catalog_extras=0):
        package = self.directory / "named-creatures"
        package.mkdir()
        (package / "regions").mkdir()
        source_root = ROOT / "scenarios/tests/generated-filler"
        source = tomllib.loads((source_root / "scenario.toml").read_text(encoding="utf-8"))
        source["id"] = "named-creatures"
        for index in range(catalog_extras):
            source["archetypes"][f"rat{index:02}"] = source["archetypes"]["rat"]
        source["creatures"]["species"]["cave_rat"]["behavior"] = {
            "disposition": "passive", "idle": "wait", "memory_ticks": 1000}
        source["ai_profiles"]["idle"] = {
            "care": False, "equipment": False, "loot": False, "flee_percent": 0}
        source["archetypes"]["tonic"] = {"name": "birth tonic", "class": "potion", "stackable": True,
            "consumable": {"effects": [{"kind": "heal", "amount": {"kind": "fixed", "amount": 1}}]}}
        source["floor_activation"] = {"version": 1, "protagonist": 1, "families": {},
            "carried_item_pools": {"supplies": {"version": 1, "choices": [
                {"archetype": "tonic", "weight": 1, "quantity": 3}]}},
            "profiles": {"baseline": {"version": 1, "stages": [{"id": "foes", "version": 1,
                "kind": "actors", "count": [0, 0], "ai": "idle",
                "depth_growth": {"kind": "racial_after", "depth": 2}, "bands": [{
                    "depth": {"kind": "open", "minimum": 2}, "choices": [{"archetype": "rat", "weight": 1,
                        "carried_loot": {"version": 1, "pool": "supplies", "chance": {
                            "numerator": 1, "denominator": 1}}}]}]}]}}}
        (package / "scenario.toml").write_text(toml_source(source), encoding="utf-8", newline="\n")
        (package / "regions/1.toml").write_text(
            'id = 1\nname = "Spawn hall"\nsize = [20, 5, 1]\nanchors = { start = [2, 2, 0] }\n',
            encoding="utf-8", newline="\n")
        shutil.copy2(source_root / "rules.toml", package / "rules.toml")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
            capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        return package

    def query(self, wizard, operation, kind, field):
        start = len(wizard.transcript)
        if kind == "creature_catalog":
            # Catalog continuations retain the already issued read basis. They
            # are not setup mutations and must not demand unrelated pictures.
            frame = self.command(wizard, {"type": "wizard", "command": operation})
            self.assertIsNone(frame["error"])
        else:
            frame = self.wizard_command(wizard, operation)
        messages = [json.loads(line).get("message") for line in wizard.transcript[start:] if line.startswith("{")]
        replies = [message[field] for message in messages if message and message["type"] == kind]
        self.assertEqual(len(replies), 1)
        if kind == "creature_inspection":
            self.assertEqual(replies[0]["revision"], frame["state"]["revision"])
        return frame, replies[0]

    def born_actor(self, frame):
        summaries = [entry["content"]["summary"] for entry in frame["history"]
            if entry["content"]["type"] == "wizard"]
        return int(re.search(r"Spawned actor (\d+)", summaries[-1]).group(1))

    @non_graphical
    def test_named_standalone_creature_uses_authored_build(self):
        self.server(wizard=True, scenario="generated-filler")
        wizard = self.wizard()
        frame = self.wizard_command(wizard, "creature spawn archetype:rat 1 4 2 0")
        actor = self.born_actor(frame)
        _, report = self.query(wizard, f"creature inspect {actor}", "creature_inspection", "report")
        self.assertEqual(report["species"]["id"], "cave_rat")
        self.assertEqual(len(report["hit_dice"]), 1)
        self.assertIn("no population AI or stock", frame["history"][-1]["content"]["summary"])

    @non_graphical
    def test_named_creature_catalog_lists_authored_selectors(self):
        self.server("--outbound-frame-bytes", 4096, wizard=True,
                    scenario=self.package(catalog_extras=40))
        wizard = self.wizard()
        before = self.request(wizard, {"type": "snapshot"})
        after = None
        selectors = []
        for _ in range(43):
            command = "creature list depth 3" + (f" after {after}" if after else "")
            frame, page = self.query(wizard, command, "creature_catalog", "page")
            self.assertTrue(page["entries"])
            self.assertLessEqual(len(page["entries"]), 32)
            selectors.extend(row["selector"] for row in page["entries"])
            self.assertEqual(frame["state"], before["state"])
            self.assertEqual(frame["history"], before["history"])
            after = page["next"]
            if after is None:
                break
        self.assertIsNone(after)
        self.assertEqual(len(selectors), 42)
        self.assertEqual(selectors, sorted(set(selectors)))
        self.assertIn("archetype:rat", selectors)
        self.assertIn("population:baseline/foes/rat", selectors)
        text, _ = self.text_client(WIZARD_TOKEN, observe=True)
        output = text.command("wizard creature list")
        self.assertIn("archetype:rat", output)
        self.assertIn("no population AI or birth stock", output)

    @non_graphical
    def test_population_birth_stock_depth_rejects_atomically_and_recovers_without_sources(self):
        package = self.package()
        server = self.server("--checkpoint-interval", 1, wizard=True, scenario=package)
        wizard = self.wizard()
        _, page = self.query(wizard, "creature list depth 3", "creature_catalog", "page")
        self.assertTrue(any(row["selector"] == "population:baseline/foes/rat" for row in page["entries"]))
        for command in ["creature spawn missing 1 4 2 0", "creature spawn rat 1 2 2 0 depth 3",
                        "creature spawn rat 1 4 2 0 depth 258"]:
            before = self.request(wizard, {"type": "snapshot"})
            rejected = self.wizard_command(wizard, command, succeeds=False)
            self.assertEqual(rejected["state"], before["state"])
            self.assertEqual(rejected["history"], before["history"])
        born = self.wizard_command(wizard, "creature spawn rat 1 4 2 0 depth 3")
        actor = self.born_actor(born)
        _, report = self.query(wizard, f"creature inspect {actor}", "creature_inspection", "report")
        self.assertEqual(len(report["hit_dice"]), 2)
        observer, stock = self.client(WIZARD_TOKEN, observe=True, actor=actor)
        inventory = stock["state"]["observation"]["inventory"]
        self.assertEqual([(item["name"], item["quantity"]) for item in inventory], [("birth tonic", "3")])
        spectator, _ = self.client(SPECTATOR_TOKEN, observe=True)
        self.wizard_command(spectator, "creature list", succeeds=False)
        self.assertNotIn('"creature_catalog"', "".join(spectator.transcript))
        player, _ = self.client()
        waited = self.act(player, {"type": "wait"})
        def settled(frame):
            observation = frame["state"]["observation"]
            return (int(observation["tick"]) > 0 and observation["ready"]
                    and frame["readiness"]["admission"]
                    and not any(row["phase"] in ("queued", "started", "paused", "suspended")
                                for row in frame["intentions"]))
        waited = waited if settled(waited) else self.frame(player, settled)
        self.assertIsNone(waited["error"])
        self.assertGreater(int(waited["state"]["observation"]["tick"]), 0)
        # Freeze a durable mid-game state, then remove all original package inputs.
        snapshot = self.request(wizard, {"type": "snapshot"})
        _, report = self.query(wizard, f"creature inspect {actor}", "creature_inspection", "report")
        self.flush_save()
        for client in [observer, spectator, player, wizard]:
            client.stop()
        server.stop()
        shutil.rmtree(package)
        self.server(wizard=True, seed=None)
        wizard = self.wizard()
        restored = self.request(wizard, {"type": "snapshot"})
        self.assertDisclosedStateEqual(restored["state"], snapshot["state"])
        _, recovered = self.query(wizard, f"creature inspect {actor}", "creature_inspection", "report")
        self.assertDisclosedInspectionEqual(recovered, report)
        self.query(wizard, "creature list depth 3", "creature_catalog", "page")
        # New birth and rewind still resolve from the pinned package, never source paths.
        second = self.wizard_command(wizard, "creature spawn rat 1 6 2 0 depth 3")
        second_actor = self.born_actor(second)
        self.assertGreater(second_actor, actor)
        self.wizard_command(wizard, "rewind initial")
        alternate = self.wizard_command(wizard, "creature spawn rat 1 4 2 0 depth 3")
        self.assertGreater(self.born_actor(alternate), second_actor)

    def test_native_catalog_console_uses_real_keys_and_preserves_state(self):
        self.server(wizard=True, scenario=self.package(catalog_extras=24))
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--observe", "--report-frames"], token=WIZARD_TOKEN)
        initial = self.ascii_frame(window, lambda frame: frame["state"] is not None and not frame["busy"])
        key = self.native_keys(window)
        opened = self.native_press(window, key, "F7", "wizard")
        self.assertEqual(opened["wizard_command"], "")
        self.native_text(window, "creature list depth 3")
        self.ascii_frame(window, lambda frame: frame.get("wizard_command") == "creature list depth 3")
        shown = self.native_press(window, key, "Return", "enter")
        self.assertTrue(shown["screen"]["stats"])
        self.assertEqual(shown["state"], initial["state"])
        self.assertIn("Creature catalog (depth 3)", ascii_stats_viewport_text(shown))
        self.assertIn("archetype:rat", ascii_stats_viewport_text(shown))
        shown = self.native_press(window, key, "Down", "down")
        self.assertEqual(shown["screen"]["stats_scroll"], 1)
        for _ in range(len(shown["stats_rows"]) // 24 + 1):
            if "population:baseline/foes/rat" in ascii_stats_viewport_text(shown):
                break
            previous = shown["screen"]["stats_scroll"]
            # The release must reach the native pump before the next PageDown.
            shown = self.native_press(window, key, "PageDown", "recent_history")
            self.assertGreater(shown["screen"]["stats_scroll"], previous)
            self.assertEqual(shown["state"], initial["state"])
        self.assertIn("population:baseline/foes/rat", ascii_stats_viewport_text(shown))
        closed = self.native_press(window, key, "Escape", "escape")
        self.assertFalse(closed["screen"]["stats"])
        self.assertEqual(closed["state"], initial["state"])


if __name__ == "__main__":
    unittest.main()
