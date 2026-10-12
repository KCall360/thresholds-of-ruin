"""Compact structural families through the real compiler, server and client."""
import json
import shutil
import subprocess

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
        source = manifest.read_text(encoding="utf-8").replace('anchor = "1/start"', 'anchor = "1000/stair"')
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
        self.assertEqual(resumed["state"], changed["state"])
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
        self.assertEqual(after["state"], before["state"])
        waited = self.act(player, {"type": "wait"})
        self.assertIsNone(waited["error"])
        self.assertEqual(underfoot(waited["state"])["key"], underfoot(at_stair)["key"])
        at_up = explore_to_stairs(self, player, waited["state"], direction="up")
        self.assertTrue(underfoot(at_up)["stairs_up"])
        ascended = self.act(player, {"type": "move", "direction": "up"})
        self.assertIsNone(ascended["error"])
        self.assertEqual(underfoot(ascended["state"])["key"], underfoot(first_stair)["key"])

    def test_unvisited_generation_and_generated_mutations_survive_source_deletion(self):
        from process_harness import underfoot
        package = self.compact("pinned-future", 'kind = "open"\nmax_depth = 1000000')
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        server = self.server(scenario=package, wizard=True)
        player, _ = self.client()
        self.flush_save()
        player.stop()
        server.stop()
        shutil.rmtree(package)
        server = self.server(seed=None, wizard=True)
        player, _ = self.client()
        wizard = self.wizard()
        # Depth101 is far outside initial horizons. Its first slot's north
        # exterior row is guaranteed stone before these ordinary mutations.
        for x in [0, 1]:
            for z in [0, 1]:
                self.wizard_command(wizard, f"wall 401 {x} 0 {z} open")
        self.wizard_command(wizard, "teleport 1 401 0 0 0")
        before = self.request(player, {"type": "snapshot"})
        moved = self.act(player, {"type": "move", "direction": "east"})
        self.assertIsNone(moved["error"])
        self.assertNotEqual(underfoot(before["state"])["key"], underfoot(moved["state"])["key"])
        changed_cell = underfoot(moved["state"])["key"]
        self.flush_save()
        self.wizard_command(wizard, "teleport 1 1 13 3 0")
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        self.flush_save()
        player.stop()
        wizard.stop()
        server.stop()
        self.server(seed=None, wizard=True)
        player, _ = self.client()
        wizard = self.wizard()
        self.wizard_command(wizard, "teleport 1 401 0 0 0")
        again = self.act(player, {"type": "move", "direction": "east"})
        self.assertIsNone(again["error"])
        self.assertEqual(underfoot(again["state"])["key"], changed_cell)
