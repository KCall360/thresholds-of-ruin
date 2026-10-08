"""Finite material enclosure through actual server and all three frontends."""
import json
import os
from pathlib import Path
import unittest

from process_harness import ProcessTestCase, SPECTATOR_TOKEN, inspect_save, load_fixture


class MaterialProcesses(ProcessTestCase):
    graphical = True

    def here(self, frame):
        return self.column(frame, 0)

    def column(self, frame, z):
        """The seen cell z cells above the player's feet, or None if unseen."""
        return next((c for c in frame["state"]["observation"]["visible_cells"]
                     if c["position"] == {"x": 0, "y": 0, "z": z}), None)

    def test_normal_enclosure_surfaces_movement_and_resume(self):
        server = self.server()
        player, welcome = self.adventure()
        self.assertIn("chamber of stone", welcome)
        self.assertIn("stone", self.say(player, "examine ceiling"))
        observer, initial = self.client(SPECTATOR_TOKEN)
        # Floors and ceilings are seen solid cells: stone underfoot, open
        # headroom, and a stone ceiling two cells up.
        self.assertEqual((self.column(initial, -1)["wall"], self.column(initial, -1)["material"]), (True, "stone"))
        self.assertFalse(self.column(initial, 1)["wall"])
        self.assertEqual((self.column(initial, 2)["wall"], self.column(initial, 2)["material"]), (True, "stone"))
        for direction in ("up", "down"):
            self.say(player, "step " + direction)
            self.assertEqual(self.request(observer, {"type": "snapshot"})["state"], initial["state"])
        self.assertIn("walk east", self.say(player, "east"))
        expected = self.request(observer, {"type": "snapshot"})["state"]
        player.stop(); observer.stop(); server.stop()
        self.assertEqual(inspect_save(self.save)["ruleset"], "interactions-v25")
        self.server()
        _, resumed = self.client(SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"], expected)

    def test_doorway_corners_from_west_on_door_and_east_in_native_ascii(self):
        server = self.server()
        observer, _ = self.client(SPECTATOR_TOKEN)
        capture = self.save.parent / "door-rim.ppm"
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--automation", "--capture", capture])
        self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"])
        for _ in range(3):
            native = self.key(window, "right")
        for name, floor_x, wall_x in [("west", 2, 1), ("on-door", 1, 0), ("east", 0, -1)]:
            cells = {(c["position"]["x"], c["position"]["y"]): c
                     for c in native["state"]["observation"]["visible_cells"]
                     if c["position"]["z"] == 0}
            for y in (-1, 1):
                self.assertIn((floor_x, y), cells, name)
                self.assertFalse(cells[(floor_x, y)]["wall"], name)
                self.assertIn((wall_x, y), cells, name)
                self.assertTrue(cells[(wall_x, y)]["wall"], name)
            watched = self.request(observer, {"type": "snapshot"})
            self.assertEqual(watched["state"], native["state"])
            if os.environ.get("TOR_RIM_CAPTURE_DIR"):
                import shutil
                shutil.copyfile(capture, Path(os.environ["TOR_RIM_CAPTURE_DIR"]) / ("rim-" + name + ".ppm"))
            if name != "east":
                native = self.key(window, "right")
        self.key(window, "close_door")
        closed = self.key(window, "left")
        self.assertNotIn("copper token", json.dumps(closed["state"]["observation"]["ground_items"]))
        self.key(window, "open_door")
        opened = self.key(window, "left")
        self.assertIn("copper token", json.dumps(opened["state"]["observation"]["ground_items"]))
        self.key(window, "escape")
        self.assertEqual(window.child.wait(timeout=10), 0)
        observer.stop(); server.stop()
        self.server()
        _, resumed = self.client(SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"], opened["state"])

    def test_wizard_chamber_surface_refresh_native_view_and_rewind(self):
        server = self.server(wizard=True, scenario="material-volumes")
        wizard = self.wizard()
        fixture = load_fixture("material-volumes.json")
        observer, initial = self.client(SPECTATOR_TOKEN)
        text, welcome = self.adventure(SPECTATOR_TOKEN)
        self.assertIn("chamber of stone", welcome)
        self.assertIn("stone", self.say(text, "examine ceiling"))
        capture = Path(os.environ.get("TOR_MATERIAL_CAPTURE", str(self.save.parent / "material.ppm")))
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--automation", "--capture", capture], token=SPECTATOR_TOKEN)
        native = self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"])
        self.assertEqual(native["state"], initial["state"])
        self.wizard_command(wizard, "teleport 1 1 1 1 0")
        away = self.request(observer, {"type": "snapshot"})
        key = self.here(initial)["key"]
        # The ceiling is its own seen cell, remembered like any other.
        ceiling = self.column(initial, 2)["key"]
        remembered = lambda view: next(c for c in view["memory"] if c["key"] == ceiling)
        self.assertTrue(remembered(away)["wall"])
        self.wizard_command(wizard, fixture["remove_ceiling"])
        stale = self.request(observer, {"type": "snapshot"})
        self.assertTrue(remembered(stale)["wall"])
        self.wizard_command(wizard, "teleport 1 3 1 1 0")
        refreshed = self.request(observer, {"type": "snapshot"})
        # The hole is open, and nothing past the chamber's storage is shown.
        self.assertFalse(self.column(refreshed, 2)["wall"])
        self.assertIsNone(self.column(refreshed, 3))
        self.assertFalse(remembered(refreshed)["wall"])
        native = self.ascii_frame(window, lambda f: f["state"] == refreshed["state"])
        self.assertEqual(native["state"], refreshed["state"])
        self.assertIn("stone", self.say(text, "examine ceiling")) # Other ceiling cells remain visible.
        self.wizard_command(wizard, "rewind initial")
        rewound = self.request(observer, {"type": "snapshot"})
        self.assertTrue(self.column(rewound, 2)["wall"])
        self.assertIn(key, [c["key"] for c in rewound["memory"]])
        self.ascii_frame(window, lambda f: f["state"] == rewound["state"])
        self.key(window, "escape")
        self.assertEqual(window.child.wait(timeout=10), 0)
        self.assertTrue(capture.read_bytes().startswith(b"P6"))
        wizard.stop(); observer.stop(); text.stop(); server.stop()
        self.server(wizard=True)
        _, resumed = self.client(SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"], rewound["state"])


if __name__ == "__main__":
    unittest.main()
