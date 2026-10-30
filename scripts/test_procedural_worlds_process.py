"""Compact structural families through the real compiler, server and client."""
import json
import shutil
import sqlite3
import subprocess
from contextlib import closing

from process_harness import ProcessTestCase
from grid_scenario_authoring import build_compact


class ProceduralWorldProcesses(ProcessTestCase):
    def compact(self, name, domain):
        package = self.directory / name
        build_compact(package, domain)
        return package

    def test_large_compact_world_compiles_and_restarts_without_any_region_sources(self):
        package = self.compact("compact", 'kind = "open"\nmax_depth = 1000000')
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        certificate = json.loads(result.stdout)
        self.assertEqual(certificate["regions"], 0)
        self.assertFalse((package / "regions").exists())
        index = json.loads((package / "index.json").read_text(encoding="utf-8"))
        self.assertEqual(index["regions"], [])
        server = self.server(scenario=package)
        client, ready = self.client()
        self.assertIsNone(ready.get("error"))
        self.assertIsNone(self.act(client, {"type": "wait"}).get("error"))
        self.assertIsNone(self.request(client, {"type": "save"}).get("error"))
        client.stop()
        server.stop()
        shutil.rmtree(package)
        self.server(seed=None)
        client, ready = self.client()
        self.assertIsNone(ready.get("error"))
        self.assertIsNone(self.act(client, {"type": "wait"}).get("error"))

    def declared_start(self, depth, port):
        from checkpoint_payload import decode_checkpoint_payload
        from process_harness import explore_to_stairs, underfoot

        package = self.compact("declared-start", 'kind = "open"\nmax_depth = 1000000')
        manifest = package / "scenario.toml"
        manifest.write_text(manifest.read_text(encoding="utf-8").replace(
            'anchor = "caves/1/start"', f'anchor = "caves/{depth}/{port}"'), encoding="utf-8")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["regions"], 0)
        self.assertEqual(json.loads((package / "index.json").read_text(encoding="utf-8"))["regions"], [])
        server = self.server("--checkpoint-interval", 1, scenario=package)
        player, ready = self.client()
        self.assertIsNone(ready.get("error"))
        self.assertTrue(underfoot(ready["state"])["stairs_" + port])
        saved = self.act(player, {"type": "wait"})
        self.assertIsNone(saved["error"])
        self.flush_save()
        with closing(sqlite3.connect(self.save)) as db:
            payload = db.execute("SELECT payload FROM checkpoint").fetchone()[0]
            checkpoint = json.loads(decode_checkpoint_payload(payload))
            actors = checkpoint["shared"]["actors"][checkpoint["game"]["actors"]]
            region = actors["1"]["location"]["region"]
            self.assertEqual((region - 1) // 4 + 1, depth)
            initial_records = db.execute("SELECT count(*) FROM regions").fetchone()[0]
            self.assertLessEqual(initial_records, 16)
            self.assertEqual(db.execute("SELECT count(*) FROM region_sources").fetchone()[0], 0)
        player.stop()
        server.stop()
        shutil.rmtree(package)
        # Use a source-free fallback scenario: reopening must retain the saved
        # package rather than read the repository's default adventure sources.
        self.server("--regions", 1, "--checkpoint-interval", 1, seed=None)
        player, resumed = self.client()
        self.assertDisclosedStateEqual(resumed["state"], saved["state"])
        traveled = self.act(player, {"type": "move", "direction": port})
        self.assertIsNone(traveled["error"])
        self.assertNotEqual(underfoot(traveled["state"])["key"], underfoot(resumed["state"])["key"])
        self.assertTrue(underfoot(traveled["state"])["stairs_" + ("down" if port == "up" else "up")])
        if port == "up":
            # Three further ordinary ascents leave the initial two-hop horizon.
            # The original sources and compiled bundle are already gone.
            for _ in range(3):
                at_stair = explore_to_stairs(self, player, traveled["state"], direction="up")
                self.assertTrue(underfoot(at_stair)["stairs_up"])
                traveled = self.act(player, {"type": "move", "direction": "up"})
                self.assertIsNone(traveled["error"])
            self.flush_save()
            with closing(sqlite3.connect(self.save)) as db:
                self.assertGreater(db.execute("SELECT count(*) FROM regions").fetchone()[0], initial_records)
                payload = db.execute("SELECT payload FROM checkpoint").fetchone()[0]
                checkpoint = json.loads(decode_checkpoint_payload(payload))
                actors = checkpoint["shared"]["actors"][checkpoint["game"]["actors"]]
                self.assertEqual((actors["1"]["location"]["region"] - 1) // 4 + 1, depth - 4)

    def test_declared_depth_26_starts_on_existing_up_port_and_resumes_source_free(self):
        self.declared_start(26, "up")

    def test_declared_depth_999999_starts_on_existing_down_port_and_resumes_source_free(self):
        self.declared_start(999999, "down")

    def test_invalid_operational_domain_fails_without_derived_outputs(self):
        package = self.compact("invalid", 'kind = "seeded_finite"\nmin = 7\nmax = 2')
        before = {p.name: p.read_bytes() for p in package.iterdir()}
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, timeout=15)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(before, {p.name: p.read_bytes() for p in package.iterdir()})
        self.assertIn("domain", result.stderr)

    def test_physical_portal_to_generated_family_reports_named_connection_contract(self):
        package = self.compact("unsupported-portal", 'kind = "finite"\ncount = 2')
        (package / "regions").mkdir()
        (package / "regions/1000.toml").write_text(
            'id=1000\nname="Hub"\nsize=[5,5,2]\nchamber=true\n'
            '[[portals]]\nat=[2,2,0]\ndirection="down"\nto="caves/2/up"\n',
            encoding="utf-8")
        before = {str(p.relative_to(package)): p.read_bytes()
                  for p in package.rglob("*") if p.is_file()}
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "check", package],
                                capture_output=True, text=True, timeout=30)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("use stair_pairs", json.loads(result.stderr)["error"]["message"])
        self.assertEqual(before, {str(p.relative_to(package)): p.read_bytes()
                                for p in package.rglob("*") if p.is_file()})

    def test_authored_hub_generated_ports_and_chamber_traverse_and_resume(self):
        from process_harness import explore_to_stairs, underfoot
        package = self.compact("composed", 'kind = "finite"\ncount = 1')
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8").replace('anchor = "caves/1/start"', 'anchor = "1000/stair"')
        source += ('\n[stair_pairs.entry]\nupper = "1000/stair"\nlower = "caves/1/up"\n'
                   '[stair_pairs.exit]\nupper = "caves/last/down"\nlower = "1001/stair"\n')
        manifest.write_text(source, encoding="utf-8")
        (package / "regions").mkdir()
        for region, name in [(1000, "Hub"), (1001, "Chamber")]:
            (package / "regions" / f"{region}.toml").write_text(
                f'id={region}\nname="{name}"\nsize=[5,5,2]\nchamber=true\nanchors={{stair=[2,2,0]}}\n',
                encoding="utf-8")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        server = self.server(scenario=package, wizard=True)
        player, start = self.client()
        origin = underfoot(start["state"])["key"]
        entry = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNone(entry["error"])
        self.assertTrue(underfoot(entry["state"])["stairs_up"])
        returned = self.act(player, {"type": "move", "direction": "up"})
        self.assertEqual(underfoot(returned["state"])["key"], origin)
        entry = self.act(player, {"type": "move", "direction": "down"})
        departure = explore_to_stairs(self, player, entry["state"])
        exit_cell = underfoot(departure)["key"]
        chamber = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNone(chamber["error"])
        wizard = self.wizard()
        self.wizard_command(wizard, "wall 1001 3 2 0 closed")
        changed = self.request(player, {"type": "snapshot"})
        self.flush_save()
        player.stop()
        wizard.stop()
        server.stop()
        shutil.rmtree(package)
        self.server(seed=None, wizard=True)
        player, resumed = self.client()
        self.assertDisclosedStateEqual(resumed["state"], changed["state"])
        back = self.act(player, {"type": "move", "direction": "up"})
        self.assertEqual(underfoot(back["state"])["key"], exit_cell)
        again = self.act(player, {"type": "move", "direction": "down"})
        east = next(cell for cell in again["state"]["observation"]["visible_cells"]
                    if cell["position"] == {"x": 1, "y": 0, "z": 0})
        self.assertTrue(east["wall"])

    def test_open_cap_is_reachable_and_failed_descent_preserves_state(self):
        from process_harness import explore_to_stairs, underfoot
        package = self.compact("operational-limit", 'kind = "open"\nmax_depth = 2')
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.server(scenario=package)
        player, welcome = self.client()
        first_stair = explore_to_stairs(self, player, welcome["state"])
        descended = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNone(descended["error"])
        at_stair = explore_to_stairs(self, player, descended["state"])
        self.assertTrue(underfoot(at_stair)["stairs_down"])
        before = self.request(player, {"type": "snapshot"})
        blocked = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNotNone(blocked["error"])
        self.assertIn("unavailable", blocked["error"].lower())
        after = self.request(player, {"type": "snapshot"})
        self.assertDisclosedStateEqual(after["state"], before["state"])
        waited = self.act(player, {"type": "wait"})
        self.assertIsNone(waited["error"])
        self.assertEqual(underfoot(waited["state"])["key"], underfoot(at_stair)["key"])
        at_up = explore_to_stairs(self, player, waited["state"], direction="up")
        self.assertTrue(underfoot(at_up)["stairs_up"])
        ascended = self.act(player, {"type": "move", "direction": "up"})
        self.assertIsNone(ascended["error"])
        self.assertEqual(underfoot(ascended["state"])["key"], underfoot(first_stair)["key"])

    def test_unvisited_generation_and_generated_mutations_survive_source_deletion(self):
        self.unvisited_generation_and_mutations(False)

    def test_omitted_unvisited_floor_and_mutations_survive_source_deletion(self):
        self.unvisited_generation_and_mutations(True)

    def unvisited_generation_and_mutations(self, omissions):
        from process_harness import underfoot
        package = self.compact("pinned-future", 'kind = "open"\nmax_depth = 1000000')
        if omissions:
            baseline = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                      capture_output=True, text=True, timeout=60)
            self.assertEqual(baseline.returncode, 0, baseline.stdout + baseline.stderr)
            import tomllib
            from arena_matrix import toml_source
            path = package / "scenario.toml"
            manifest = tomllib.loads(path.read_text(encoding="utf-8"))
            recipe = manifest["generation_recipes"]["exploration"]
            recipe["version"] = 4
            recipe["stages"].insert(6, {"id": "room_omissions", "version": 1,
                "operation": {"kind": "room_omissions", "config": {"count": [1, 1]}}})
            path.write_text(toml_source(manifest), encoding="utf-8")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        server = self.server("--checkpoint-interval", 0, scenario=package, wizard=True)
        player, _ = self.client()
        self.flush_save()
        player.stop()
        server.stop()
        with closing(sqlite3.connect(self.save)) as db:
            self.assertEqual(db.execute("SELECT count(*) FROM checkpoint").fetchone()[0], 0)
        shutil.rmtree(package)
        server = self.server("--checkpoint-interval", 0, seed=None, wizard=True)
        player, _ = self.client()
        wizard = self.wizard()
        # Depth101 is far outside initial horizons. Its first slot's north
        # exterior row is guaranteed stone before these ordinary mutations.
        for x in [0, 1]:
            for z in [0, 1]:
                self.wizard_command(wizard, f"wall 401 {x} 0 {z} open")
        teleported = self.wizard_command(wizard, "teleport 1 401 0 0 0")
        before = self.request(player, {"type": "snapshot"})
        self.assertIsNone(before["error"])
        self.assertDisclosedStateEqual(before["state"], teleported["state"])
        moved = self.act(player, {"type": "move", "direction": "east"})
        self.assertIsNone(moved["error"])
        self.assertNotEqual(underfoot(before["state"])["key"], underfoot(moved["state"])["key"])
        changed_cell = underfoot(moved["state"])["key"]
        self.flush_save()
        # Initial entry belongs to a seeded slot and cell, not a fixed center.
        # This explicit return clearing lets the test leave depth 101 without
        # depending on undisclosed generated terrain or the entry selection.
        for z in [0, 1]:
            self.wizard_command(wizard, f"wall 1 0 0 {z} open")
        teleported = self.wizard_command(wizard, "teleport 1 1 0 0 0")
        # A wizard reply cannot synchronize the player's separate socket.
        returned = self.request(player, {"type": "snapshot"})
        self.assertIsNone(returned["error"])
        self.assertDisclosedStateEqual(returned["state"], teleported["state"])
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        self.flush_save()
        player.stop()
        wizard.stop()
        server.stop()
        with closing(sqlite3.connect(self.save)) as db:
            self.assertEqual(db.execute("SELECT count(*) FROM checkpoint").fetchone()[0], 0)
        self.server("--checkpoint-interval", 0, seed=None, wizard=True)
        player, _ = self.client()
        wizard = self.wizard()
        teleported = self.wizard_command(wizard, "teleport 1 401 0 0 0")
        restored = self.request(player, {"type": "snapshot"})
        self.assertIsNone(restored["error"])
        self.assertDisclosedStateEqual(restored["state"], teleported["state"])
        again = self.act(player, {"type": "move", "direction": "east"})
        self.assertIsNone(again["error"])
        self.assertEqual(underfoot(again["state"])["key"], changed_cell)
