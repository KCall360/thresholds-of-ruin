"""Explore generated floors through actual clients using disclosed cells only."""
from collections import deque
import unittest

from process_harness import ProcessTestCase, SPECTATOR_TOKEN


DIRECTIONS = [(-1, 0, "west"), (1, 0, "east"),
              (0, -1, "north"), (0, 1, "south")]


def underfoot(state):
    return next(cell for cell in state["observation"]["visible_cells"]
                if cell["position"] == {"x": 0, "y": 0, "z": 0})


def explore_to_stairs(test, player, state):
    """Choose routes from observations, without reading generated terrain."""
    at = (0, 0)
    visited = set()
    floor = set()
    stairs = set()
    for _ in range(2500):
        visited.add(at)
        for cell in state["observation"]["visible_cells"]:
            position = cell["position"]
            if position["z"] != 0:
                continue
            here = (at[0] + position["x"], at[1] + position["y"])
            if not cell["wall"]:
                floor.add(here)
                if cell["stairs_down"]:
                    stairs.add(here)
        if underfoot(state)["stairs_down"]:
            return state
        queue = deque([(at, [])])
        seen = {at}
        route = None
        while queue:
            here, path = queue.popleft()
            if here != at and (here in stairs if stairs else here not in visited):
                route = path
                break
            for dx, dy, direction in DIRECTIONS:
                neighbor = (here[0] + dx, here[1] + dy)
                if neighbor in floor and neighbor not in seen:
                    seen.add(neighbor)
                    queue.append((neighbor, path + [(neighbor, direction)]))
        test.assertIsNotNone(route, "No disclosed route to unexplored terrain or stairs")
        target, direction = route[0]
        before = underfoot(state)["key"]
        response = test.act(player, {"type": "move", "direction": direction})
        test.assertIsNone(response["error"])
        state = response["state"]
        test.assertNotEqual(underfoot(state)["key"], before, "Disclosed floor was not traversable")
        at = target
    test.fail("Exploration exceeded its bounded action count")


class RogueExplorationProcess(ProcessTestCase):
    def test_authored_entry_and_group_stair_round_trip(self):
        from build_rogue_exploration import build
        root = self.directory / "mixed-entry"
        build(root)
        manifest = root / "scenario.toml"
        text = manifest.read_text(encoding="utf-8").replace('anchor = "1/start"', 'anchor = "235/entry"')
        text += '\n[stair_pairs.entrance]\nupper = "235/entry"\nlower = "1/start"\n'
        manifest.write_text(text, encoding="utf-8", newline="\n")
        (root / "regions" / "235.toml").write_text("id=235\nname='Entry'\nsize=[3,3,2]\nchamber=true\n[anchors]\nentry=[1,1,0]\n", encoding="utf-8", newline="\n")
        self.server("--allow-unvalidated", scenario=root, separate_stderr=True)
        player, welcome = self.client()
        origin = underfoot(welcome["state"])["key"]
        down = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNone(down["error"])
        self.assertTrue(underfoot(down["state"])["stairs_up"])
        up = self.act(player, {"type": "move", "direction": "up"})
        self.assertIsNone(up["error"])
        self.assertEqual(underfoot(up["state"])["key"], origin)

    def test_headless_exploration_stair_round_trip_resume_and_spectator(self):
        server = self.server(scenario="rogue-exploration")
        player, welcome = self.client()
        departure = explore_to_stairs(self, player, welcome["state"])
        origin = underfoot(departure)["key"]
        arrived = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNone(arrived["error"])
        self.assertTrue(underfoot(arrived["state"])["stairs_up"])
        observer, watched = self.client(token=SPECTATOR_TOKEN, observe=True)
        self.assertEqual(watched["state"], arrived["state"])
        self.flush_save()
        player.stop()
        observer.stop()
        server.stop()
        self.server()
        player, resumed = self.client()
        self.assertEqual(resumed["state"], arrived["state"])
        returned = self.act(player, {"type": "move", "direction": "up"})
        self.assertIsNone(returned["error"])
        self.assertEqual(underfoot(returned["state"])["key"], origin)

    def test_adventure_uses_generated_floor_stairs(self):
        self.server(scenario="rogue-exploration")
        player, welcome = self.client()
        explore_to_stairs(self, player, welcome["state"])
        player.stop()
        adventure, welcome = self.adventure()
        self.assertIn("stairs down", welcome)
        down = self.say(adventure, "down")
        self.assertTrue(down.startswith("You go down.\n"), down)
        self.assertIn("stairs up", down)
        self.assertTrue(self.say(adventure, "up").startswith("You go up.\n"))


class RogueExplorationAsciiProcess(ProcessTestCase):
    graphical = True

    def test_native_generated_floor_stair_round_trip(self):
        self.server(scenario="rogue-exploration")
        player, welcome = self.client()
        departure = explore_to_stairs(self, player, welcome["state"])
        player.stop()
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        initial = self.ascii_frame(window, lambda frame: frame.get("state") and not frame["busy"])
        origin = underfoot(initial["state"])["key"]
        tick = int(departure["observation"]["tick"])
        key = self.native_keys(window)
        for name, delta in [("period", 100), ("comma", 200)]:
            key("Shift_L", True)
            key(name, True)
            frame = self.ascii_frame(window, lambda frame: frame.get("state")
                and frame["state"]["observation"]["tick"] == str(tick + delta) and not frame["busy"])
            key(name, False)
            key("Shift_L", False)
            if delta == 100:
                self.assertTrue(underfoot(frame["state"])["stairs_up"])
            else:
                self.assertEqual(underfoot(frame["state"])["key"], origin)


if __name__ == "__main__":
    unittest.main()
