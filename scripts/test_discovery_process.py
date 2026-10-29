"""Real Text/native/headless discovery and journal/checkpoint source-free recovery."""
import copy
import json
import shutil
import unittest

from discovery_package import doors, generated_package, observation, package, traps
from process_harness import (
    ProcessTestCase, ROOT, SPECTATOR_TOKEN, non_graphical, settle_rogue_action, underfoot,
)
from rogue_test_support import DisclosedFloorGraph


class DiscoveryProcesses(ProcessTestCase):
    graphical = True

    def snapshot(self, client):
        frame = self.request(client, {"type": "snapshot"})
        self.assertIsNone(frame["error"])
        return frame

    def capture_discovery_snapshot(self, client, name):
        start = len(client.transcript)
        current = self.snapshot(client)
        messages = [json.loads(line).get("message") for line in client.transcript[start:]
                    if line.startswith("{")]
        snapshots = [message for message in messages if message and message["type"] == "snapshot"]
        self.assertEqual(len(snapshots), 1)
        self.assertEqual(snapshots[0]["snapshot"]["state"], current["state"])
        self.discovery_samples[name] = copy.deepcopy(snapshots[0])
        return current

    def reopen(self, checkpoint_interval):
        self.discovery_samples = {}
        source = package(self)
        server = self.server("--checkpoint-interval", checkpoint_interval, scenario=source, seed=42)
        text, welcome = self.adventure()
        self.assertIn("trap", welcome)
        spectator, _ = self.client(SPECTATOR_TOKEN)
        presence = self.capture_discovery_snapshot(spectator, "presence")
        self.assertIsNone(next(iter(traps(presence).values()))["family"])
        self.assertIn("rust trap", self.say(text, "examine trap"))
        initial = self.capture_discovery_snapshot(spectator, "family")
        first = next(iter(traps(initial).values()))
        self.assertEqual(first["family"], "rust")
        self.assertEqual(observation(initial)["tick"], "0", "Explicit examination is not a paid search")
        self.assertIn("not disarmed", self.say(text, "examine trap"))
        for _ in range(3):
            self.assertDisclosedStateEqual(self.snapshot(spectator)["state"], initial["state"])
        self.say(text, "step east")
        self.say(text, "open secret entrance")
        opened = self.capture_discovery_snapshot(spectator, "opened")
        self.assertTrue(next(iter(doors(opened).values()))["open"])
        self.assertEqual(len(traps(opened)), 2)
        # Saved discovery is actor knowledge; spectators of that actor see the
        # same known family, not the other trap's still-unknown mechanism.
        self.assertEqual(sum(trap["family"] is None for trap in traps(opened).values()), 1)
        self.assertIsNone(self.request(spectator, {"type": "snapshot"})["error"])
        self.say(text, "save")
        text.stop(save=False)
        spectator.stop(save=False)
        server.stop(save=False)
        shutil.rmtree(source)
        self.server("--checkpoint-interval", checkpoint_interval, scenario=ROOT / "scenarios/two-room", seed=None)
        restored_client, _ = self.client(SPECTATOR_TOKEN)
        restored = self.capture_discovery_snapshot(restored_client, "restored")
        self.assertDisclosedStateEqual(restored["state"], opened["state"])
        self.assertEqual(traps(restored), traps(opened))
        self.assertFalse(restored["state"]["wizard_game"])

    @non_graphical
    def test_text_examination_and_secret_open_survive_source_free_checkpoint(self):
        self.reopen(1)

    @non_graphical
    def test_text_examination_and_secret_open_survive_source_free_journal(self):
        self.reopen(100000)

    def generated_action(self, player, action):
        self.generated_actions += 1
        self.assertLessEqual(self.generated_actions, 512, "Generated witness action budget")
        return settle_rogue_action(self, player, self.act(player, action))

    def explore_generated_floor(self, player, current, expected=None):
        """Visit only disclosed keys; open/examine only disclosed reachable facts."""
        graph = DisclosedFloorGraph()
        visited, entrances, plates = set(), {}, {}
        stairs = {"up": set(), "down": set()}
        for _ in range(128):
            state = current["state"]
            graph.observe(state)
            self.assertLessEqual(len(graph.edges), 32, "Two bounded room interiors and corridor")
            here = underfoot(state)["key"]
            visited.add(here)
            cells = sorted((cell for cell in observation(current)["visible_cells"]
                            if cell["position"]["z"] == 0),
                           key=lambda cell: (cell["position"]["x"], cell["position"]["y"]))
            for cell in cells:
                for direction in stairs:
                    if cell["stairs_" + direction]:
                        stairs[direction].add(cell["key"])
                if door := cell.get("door"):
                    self.assertEqual(door["name"], "secret entrance")
                    if expected is None and door["id"] not in entrances:
                        self.assertFalse(door["open"], "Generated closures must start physically closed")
                    entrances[door["id"]] = {"key": cell["key"], "open": door["open"]}
                    if expected is not None:
                        self.assertEqual(entrances[door["id"]], expected[2][door["id"]])
                if trap := cell.get("trap"):
                    plates[trap["id"]] = {"key": cell["key"], "family": trap["family"]}
                    if expected is not None:
                        self.assertEqual(plates[trap["id"]], expected[3][trap["id"]])
            closed = next((cell["door"] for cell in cells if cell.get("door")
                           and cell["door"]["reachable"] and not cell["door"]["open"]), None)
            if closed:
                current = self.generated_action(player, {
                    "type": "set_door", "door": closed["id"], "open": True})
                continue
            unknown = next((cell["trap"] for cell in cells if cell.get("trap")
                            and cell["trap"]["reachable"] and cell["trap"]["family"] is None), None)
            if unknown:
                current = self.generated_action(player, {"type": "examine_trap", "trap": unknown["id"]})
                continue
            route = graph.route_to_any(here, set(graph.edges) - visited)
            if route is None:
                self.assertEqual(len(entrances), 2, "Both generated corridor closures were exercised")
                self.assertEqual(len(plates), 2, "Both generated trap identities were exercised")
                self.assertTrue(all(row["open"] for row in entrances.values()))
                self.assertTrue(all(row["family"] in ("rust", "sleeping_gas") for row in plates.values()))
                self.assertTrue({row["key"] for row in entrances.values()} <= visited,
                                "Opening is insufficient: traverse both real doorway cells")
                self.assertTrue({row["key"] for row in plates.values()} <= visited)
                return current, (graph, stairs, entrances, plates)
            target, direction = route[0]
            current = self.generated_action(player, {"type": "move", "direction": direction})
            self.assertEqual(underfoot(current["state"])["key"], target)
        self.fail("Generated floor exploration exceeded its bounded action count")

    def generated_stair(self, player, current, floor, direction):
        graph, stairs, _, _ = floor
        self.assertTrue(stairs[direction], "Required generated stair was physically discovered")
        route = graph.route_to_any(underfoot(current["state"])["key"], stairs[direction])
        self.assertIsNotNone(route, "All-open disclosed floor must connect its required stairs")
        for target, move in route:
            current = self.generated_action(player, {"type": "move", "direction": move})
            self.assertEqual(underfoot(current["state"])["key"], target)
        self.assertTrue(underfoot(current["state"])["stairs_" + direction])
        return current

    @non_graphical
    def test_generated_features_open_traverse_and_survive_source_free_reopen(self):
        self.generated_actions = 0
        source = generated_package(self)
        self.assertFalse(list((source / "regions").glob("*.toml")), "Feature placement must be generated")
        server = self.server("--checkpoint-interval", 1, scenario=source, seed=42)
        player, current = self.client()
        current, first = self.explore_generated_floor(player, current)
        current = self.generated_stair(player, current, first, "down")
        first_exit = underfoot(current["state"])["key"]
        before = self.snapshot(player)
        self.flush_save()
        player.stop(save=False)
        server.stop(save=False)
        shutil.rmtree(source)
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        player, restored = self.client()
        self.assertDisclosedStateEqual(restored["state"], before["state"])
        self.assertEqual(restored["history"], before["history"])
        self.assertFalse(restored["state"]["wizard_game"])

        # These ordinary entries happen only after the package was deleted.
        # In particular, the third floor was not visited before the restart.
        current = self.generated_action(player, {"type": "move", "direction": "down"})
        self.assertTrue(underfoot(current["state"])["stairs_up"])
        current, second = self.explore_generated_floor(player, current)
        current = self.generated_stair(player, current, second, "down")
        second_exit = underfoot(current["state"])["key"]
        current = self.generated_action(player, {"type": "move", "direction": "down"})
        self.assertTrue(underfoot(current["state"])["stairs_up"])
        current, third = self.explore_generated_floor(player, current)
        for feature_index in (2, 3):
            identities = [set(floor[feature_index]) for floor in (first, second, third)]
            self.assertEqual(len(set.union(*identities)), 6, "Different floors cannot alias feature IDs")
        current = self.generated_stair(player, current, third, "up")
        current = self.generated_action(player, {"type": "move", "direction": "up"})
        self.assertEqual(underfoot(current["state"])["key"], second_exit)
        current = self.generated_stair(player, current, second, "up")
        current = self.generated_action(player, {"type": "move", "direction": "up"})
        self.assertEqual(underfoot(current["state"])["key"], first_exit)
        _, returned = self.explore_generated_floor(player, current, expected=first)
        self.assertEqual(returned[2:], first[2:], "Generated openings, trap families and IDs survive return")

    def test_native_selected_look_examines_only_discovered_adjacent_trap(self):
        self.server(scenario=package(self), seed=42)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        initial = self.ascii_frame(window, lambda frame: frame["state"] is not None
                                   and not frame["busy"] and frame["can_admit_intention"])
        self.assertTrue(any(tile["kind"] == "trap" and tile["glyph"] == "^" for tile in initial["map_tiles"]))
        self.assertIsNone(next(iter(traps(initial).values()))["family"])
        native = self.native_keys(window)
        self.native_press(window, native, "semicolon", "look")
        self.native_press(window, native, "Right", "right")
        examined = self.native_press(window, native, "Return", "enter", wait_for_ready=True)
        self.assertEqual(next(iter(traps(examined).values()))["family"], "rust")
        self.assertEqual(observation(examined)["tick"], observation(initial)["tick"])
        self.assertIsNone(examined["screen"]["look_cursor"])
        self.assertIn("rust trap", examined["messages"]["turn"])


if __name__ == "__main__":
    unittest.main()
