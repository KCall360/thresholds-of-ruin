"""Persisted compiled inputs work after source removal and artifact relocation."""
import json
import shutil
import sqlite3
import subprocess
import unittest

from process_harness import ProcessTestCase, ROOT


class CompiledInputProcesses(ProcessTestCase):
    def compile(self, *arguments):
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), *arguments],
                                capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result

    def package(self):
        package = self.directory / "authored"
        shutil.copytree(ROOT / "scenarios/tests/streaming-corridor", package)
        self.compile("validate", str(package))
        return package

    def test_unvisited_halls_use_relocated_compiled_inputs_without_source(self):
        package = self.package()
        exported = self.directory / "portable"
        self.compile("export", str(package), str(exported))
        self.assertFalse((exported / "scenario.toml").exists())
        self.assertTrue((exported / "package.json").is_file())
        server = self.server(scenario=package, seed=5)
        player, initial = self.client()
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        with sqlite3.connect(self.save) as db:
            saved = db.execute("SELECT region,source FROM region_sources ORDER BY region").fetchall()
        self.assertLess(len(saved), 7, "unvisited authored chunks stay lazy")
        for region, text in saved:
            compiled = json.loads(text)
            self.assertEqual(compiled["format"], 9)
            self.assertEqual(compiled["region"]["id"], region)
        shutil.rmtree(package)
        server = self.server(scenario=exported, seed=None)
        player, restored = self.client()
        self.assertEqual(restored["state"], initial["state"])
        for _ in range(76):
            moved = self.act(player, {"type": "move", "direction": "east"})
            self.assertIsNone(moved.get("error"), moved.get("error"))
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            self.assertGreater(db.execute("SELECT count(*) FROM region_sources").fetchone()[0], len(saved))
        expected = self.request(player, {"type": "snapshot"})["state"]
        player.stop()
        server.stop()
        self.server(scenario=exported, seed=None)
        _, resumed = self.client()
        self.assertEqual(resumed["state"], expected)

    def test_live_saved_game_continues_after_authoring_sources_are_removed(self):
        package = self.package()
        server = self.server(scenario=package, seed=5)
        player, _ = self.client()
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            initial = db.execute("SELECT count(*) FROM region_sources").fetchone()[0]
        self.assertLess(initial, 7)
        # The server remains running throughout removal and unvisited travel.
        # Only the immutable compiled artifact is left in the package directory.
        shutil.rmtree(package / "regions")
        for name in ("scenario.toml", "rules.toml"):
            (package / name).unlink()
        for _ in range(76):
            moved = self.act(player, {"type": "move", "direction": "east"})
            self.assertIsNone(moved.get("error"), moved.get("error"))
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            self.assertGreater(db.execute("SELECT count(*) FROM region_sources").fetchone()[0], initial)
        expected = self.request(player, {"type": "snapshot"})["state"]
        player.stop()
        server.stop()
        self.server(seed=None)
        _, resumed = self.client()
        self.assertEqual(resumed["state"], expected)

    def test_metadata_over_save_capacity_is_rejected_before_publication(self):
        package = self.directory / "oversized"
        shutil.copytree(ROOT / "scenarios/two-room", package)
        manifest = package / "scenario.toml"
        text = manifest.read_text(encoding="utf-8")
        # Each asset is individually valid; only aggregate compiled metadata
        # exceeds the base-frame admission budget.
        assets = ["texture." + "x" * 55 + f"{index:05d}" for index in range(15000)]
        text += "\n[assets]\nstone = " + json.dumps(assets) + "\n"
        manifest.write_text(text, encoding="utf-8")
        before = {p.relative_to(package): p.read_bytes() for p in package.rglob("*") if p.is_file()}
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", str(package)],
                                capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("save metadata limit", result.stderr)
        self.assertEqual(before, {p.relative_to(package): p.read_bytes() for p in package.rglob("*") if p.is_file()})

    def test_corrupt_saved_compiled_input_fails_closed_without_rewriting_save(self):
        package = self.package()
        server = self.server(scenario=package, seed=5)
        player, _ = self.client()
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        with sqlite3.connect(self.save) as db:
            row = db.execute("SELECT region,source FROM region_sources ORDER BY region LIMIT 1").fetchone()
            value = json.loads(row[1])
            value["region"]["name"] = "corrupted compiled input"
            db.execute("UPDATE region_sources SET source=? WHERE region=?", (json.dumps(value), row[0]))
        before = self.save.read_bytes()
        rejected = self.launch("tor-server", ["--listen", "127.0.0.1:0", "--save", self.save])
        self.assertNotEqual(rejected.child.wait(timeout=15), 0)
        self.assertEqual(self.save.read_bytes(), before)

    def test_previous_saved_region_envelope_fails_closed_without_rewriting_save(self):
        package = self.package()
        server = self.server(scenario=package, seed=5)
        player, _ = self.client()
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        with sqlite3.connect(self.save) as db:
            row = db.execute("SELECT region,source FROM region_sources ORDER BY region LIMIT 1").fetchone()
            value = json.loads(row[1])
            self.assertEqual(value["format"], 9)
            current_format = value["format"]
            value["format"] = 8
            self.assertNotEqual(value["format"], current_format)
            db.execute("UPDATE region_sources SET source=? WHERE region=?", (json.dumps(value), row[0]))
        before = self.save.read_bytes()
        rejected = self.launch("tor-server", ["--listen", "127.0.0.1:0", "--save", self.save])
        self.assertNotEqual(rejected.child.wait(timeout=15), 0)
        self.assertEqual(self.save.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
