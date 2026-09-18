"""Finite material enclosure through actual server and all three frontends."""
import json
import os
from pathlib import Path
import unittest
import test_text_process as support
import test_headless_process as headless
import test_ascii_process as ascii_support
import test_adventure_process as adventure_support


class MaterialProcesses(unittest.TestCase):
    setUpClass = classmethod(ascii_support.AsciiProcesses.setUpClass.__func__)
    setUp = headless.HeadlessProcesses.setUp
    launch = support.TextProcesses.launch
    server = headless.HeadlessProcesses.server
    client = headless.HeadlessProcesses.client
    frame = headless.HeadlessProcesses.frame
    request = headless.HeadlessProcesses.request
    command = headless.HeadlessProcesses.command
    ascii_frame = ascii_support.AsciiProcesses.frame
    key = ascii_support.AsciiProcesses.key
    adventure = adventure_support.AdventureProcesses.adventure
    say = adventure_support.AdventureProcesses.say

    def here(self, frame):
        return next(c for c in frame["state"]["observation"]["visible_cells"]
                    if c["position"] == {"x": 0, "y": 0, "z": 0})

    def test_normal_enclosure_surfaces_movement_and_resume(self):
        server = self.server()
        player, welcome = self.adventure()
        self.assertIn("walls of stone", welcome)
        self.assertIn("stone", self.say(player, "examine ceiling"))
        observer, initial = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(self.here(initial)["floor"], {"material": "stone", "distance": 1})
        self.assertEqual(self.here(initial)["ceiling"], {"material": "stone", "distance": 2})
        for direction in ("up", "down"):
            self.say(player, "step " + direction)
            self.assertEqual(self.request(observer, {"type": "snapshot"})["state"], initial["state"])
        self.assertIn("walk east", self.say(player, "east"))
        expected = self.request(observer, {"type": "snapshot"})["state"]
        player.stop(); observer.stop(); server.stop()
        self.assertEqual(json.loads(self.save.read_text())["ruleset"], "material-rims-v10")
        self.server()
        _, resumed = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"], expected)

    def test_doorway_corners_from_west_on_door_and_east_in_native_ascii(self):
        server = self.server()
        observer, _ = self.client(support.SPECTATOR_TOKEN)
        capture = self.save.parent / "door-rim.ppm"
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--automation", "--capture", capture])
        self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"])
        for _ in range(3):
            native = self.key(window, "right")
        for name, floor_x, wall_x in [("west", 2, 1), ("on-door", 1, 0), ("east", 0, -1)]:
            cells = {(c["position"]["x"], c["position"]["y"]): c
                     for c in native["state"]["observation"]["visible_cells"]}
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
        _, resumed = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"], opened["state"])

    def test_wizard_chamber_surface_refresh_native_view_and_rewind(self):
        server = self.server(wizard=True)
        wizard = self.launch("tor-client-text", ["--connect", self.address], token=headless.WIZARD_TOKEN)
        wizard.until(lambda line: line == "Ready.")
        fixture = json.loads((Path(__file__).parent / "scenarios/material-volumes.json").read_text())
        for command in fixture["setup"]:
            self.assertNotIn("Server error", wizard.command(command))
        observer, initial = self.client(support.SPECTATOR_TOKEN)
        text, welcome = self.adventure(support.SPECTATOR_TOKEN)
        self.assertIn("stone floor", welcome)
        self.assertIn("stone", self.say(text, "examine ceiling"))
        capture = Path(os.environ.get("TOR_MATERIAL_CAPTURE", str(self.save.parent / "material.ppm")))
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--automation", "--capture", capture], token=support.SPECTATOR_TOKEN)
        native = self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"])
        self.assertEqual(native["state"], initial["state"])
        self.assertNotIn("Server error", wizard.command("wizard teleport 1 1 1 1 0"))
        away = self.request(observer, {"type": "snapshot"})
        key = self.here(initial)["key"]
        self.assertEqual(next(c for c in away["memory"] if c["key"] == key)["ceiling"]["distance"], 2)
        self.assertNotIn("Server error", wizard.command(fixture["remove_ceiling"]))
        stale = self.request(observer, {"type": "snapshot"})
        self.assertEqual(next(c for c in stale["memory"] if c["key"] == key)["ceiling"]["distance"], 2)
        self.assertNotIn("Server error", wizard.command("wizard teleport 1 3 1 1 0"))
        refreshed = self.request(observer, {"type": "snapshot"})
        self.assertIsNone(self.here(refreshed)["ceiling"])
        native = self.ascii_frame(window, lambda f: f["state"] == refreshed["state"])
        self.assertEqual(native["state"], refreshed["state"])
        self.assertIn("stone", self.say(text, "examine ceiling")) # Other ceiling cells remain visible.
        self.assertNotIn("Server error", wizard.command("wizard rewind initial"))
        rewound = self.request(observer, {"type": "snapshot"})
        self.assertEqual(self.here(rewound)["ceiling"]["distance"], 2)
        self.assertNotIn(key, [c["key"] for c in rewound["memory"]])
        self.ascii_frame(window, lambda f: f["state"] == rewound["state"])
        self.key(window, "escape")
        self.assertEqual(window.child.wait(timeout=10), 0)
        self.assertTrue(capture.read_bytes().startswith(b"P6"))
        wizard.stop(); observer.stop(); text.stop(); server.stop()
        self.server(wizard=True)
        _, resumed = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"], rewound["state"])


if __name__ == "__main__":
    unittest.main()
