"""Paired stairs through ordinary authored and procedurally generated floors."""
import json
import unittest

from process_harness import ProcessTestCase, SPECTATOR_TOKEN


def underfoot(observation):
    return next(c for c in observation["visible_cells"]
                if c["position"] == {"x": 0, "y": 0, "z": 0})


class PairedStairsProcess(ProcessTestCase):
    def test_same_region_shared_cell_blocked_body_and_changed_terrain_resume(self):
        server = self.server(scenario="paired-stairs", wizard=True)
        player, _ = self.client()
        wizard = self.wizard()
        self.wizard_command(wizard, "teleport 1 3 1 1 0")
        self.wizard_command(wizard, "wall 3 4 3 0 closed")
        self.wizard_command(wizard, json.dumps({"type": "set_body", "actor": 1,
            "cells": [[0, 0, 0], [1, 0, 0]], "eye": [0, 0, 0], "mass": 80}))
        initial = self.request(player, {"type": "snapshot"})["state"]["observation"]
        here = underfoot(initial)
        landing = next(c["key"] for c in initial["visible_cells"]
                       if c["position"] == {"x": 2, "y": 2, "z": 0})
        self.assertTrue(here["stairs_up"] and here["stairs_down"])
        blocked = self.act(player, {"type": "move", "direction": "down"})["state"]["observation"]
        self.assertEqual(underfoot(blocked)["key"], here["key"])
        self.assertEqual(blocked["tick"], initial["tick"])

        self.wizard_command(wizard, json.dumps({"type": "set_body", "actor": 1,
            "cells": [[0, 0, 0]], "eye": [0, 0, 0], "mass": 80}))
        arrived = self.act(player, {"type": "move", "direction": "down"})
        self.assertEqual(underfoot(arrived["state"]["observation"])["key"], landing)
        self.assertEqual(arrived["state"]["observation"]["tick"], "100")
        wall = {"x": 1, "y": 0, "z": 0}
        self.assertTrue(next(c for c in arrived["state"]["observation"]["visible_cells"]
                             if c["position"] == wall)["wall"])
        self.flush_save()
        player.stop()
        wizard.stop()
        server.stop()
        self.server(wizard=True)
        player, resumed = self.client()
        self.assertEqual(resumed["state"], arrived["state"])
        blocked = self.act(player, {"type": "move", "direction": "east"})["state"]["observation"]
        self.assertEqual(underfoot(blocked)["key"], landing)
        self.assertEqual(blocked["tick"], "100")
        returned = self.act(player, {"type": "move", "direction": "up"})
        self.assertEqual(underfoot(returned["state"]["observation"])["key"], here["key"])
        again = self.act(player, {"type": "move", "direction": "down"})
        self.assertTrue(next(c for c in again["state"]["observation"]["visible_cells"]
                             if c["position"] == wall)["wall"])

    def test_generated_round_trip_resume_and_spectator_disclosure(self):
        server = self.server(scenario="paired-stairs")
        player, initial = self.client()
        observer, _ = self.client(SPECTATOR_TOKEN)
        start = underfoot(initial["state"]["observation"])["key"]
        landing = next(c["key"] for c in initial["state"]["observation"]["visible_cells"]
                       if c["stairs_up"])
        arrived = self.act(player, {"type": "move", "direction": "down"})
        self.assertIsNone(arrived["error"])
        observation = arrived["state"]["observation"]
        self.assertEqual(underfoot(observation)["key"], landing)
        self.assertNotEqual(landing, start)
        self.assertEqual(observation["tick"], "100")
        self.assertTrue(any(c["stairs_up"] for c in observation["visible_cells"]))
        self.assertTrue(all("region" not in c["position"] for c in observation["visible_cells"]))
        watched = self.request(observer, {"type": "snapshot"})
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
        self.assertEqual(underfoot(returned["state"]["observation"])["key"], start)
        again = self.act(player, {"type": "move", "direction": "down"})
        self.assertEqual(underfoot(again["state"]["observation"])["key"], landing)

    def test_adventure_traverses_generated_pair(self):
        self.server(scenario="paired-stairs")
        player, welcome = self.adventure()
        self.assertIn("Stairs lead down.", welcome)
        down = self.say(player, "down")
        self.assertTrue(down.startswith("You go down.\n"), down)
        self.assertIn("Stairs lead up.", down)
        self.assertTrue(self.say(player, "up").startswith("You go up.\n"))


class PairedStairsAsciiProcess(ProcessTestCase):
    graphical = True

    def test_native_generated_pair_round_trip(self):
        self.server(scenario="paired-stairs")
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        initial = self.ascii_frame(window, lambda f: f.get("state") and not f["busy"])
        start = underfoot(initial["state"]["observation"])["key"]
        landing = next(c["key"] for c in initial["state"]["observation"]["visible_cells"]
                       if c["stairs_up"])
        key = self.native_keys(window)
        for name, tick in [("period", 100), ("comma", 200)]:
            key("Shift_L", True)
            key(name, True)
            current = self.ascii_frame(window, lambda f: f.get("state")
                and f["state"]["observation"]["tick"] == str(tick) and not f["busy"])
            key(name, False)
            key("Shift_L", False)
            self.assertEqual(underfoot(current["state"]["observation"])["key"],
                             landing if tick == 100 else start)


if __name__ == "__main__":
    unittest.main()
