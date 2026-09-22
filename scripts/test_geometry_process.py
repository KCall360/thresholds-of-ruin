"""Portal geometry acceptance through server, text, headless and native ASCII."""
import json
from pathlib import Path
import unittest

import test_ascii_process as ascii_support
import test_headless_process as headless_support
import test_text_process as support


class GeometryProcesses(unittest.TestCase):
    launch = support.TextProcesses.launch
    frame = headless_support.HeadlessProcesses.frame
    command = headless_support.HeadlessProcesses.command
    request = headless_support.HeadlessProcesses.request
    client = headless_support.HeadlessProcesses.client
    server = headless_support.HeadlessProcesses.server
    ascii_frame = ascii_support.AsciiProcesses.frame

    @classmethod
    def setUpClass(cls):
        ascii_support.AsciiProcesses.setUpClass.__func__(cls)

    def setUp(self):
        directory = support.ProcessTestDirectory()
        self.addCleanup(directory.cleanup)
        self.save = Path(directory.name) / "geometry.json"

    def test_wide_join_is_one_continuous_scene_in_every_frontend(self):
        self.server(wizard=True)
        wizard = self.launch("tor-client-text", ["--connect", self.address], token=headless_support.WIZARD_TOKEN)
        wizard.until(lambda line: line == "Ready.")
        fixture = json.loads((Path(__file__).parent / "scenarios/wide-join.json").read_text())
        for command in fixture["setup"]:
            self.assertNotIn("Server error", wizard.command(command))
        observer, seen = self.client(support.SPECTATOR_TOKEN)
        ascii_client = self.launch("tor-client-ascii", ["--connect", self.address, "--automation", "--capture", self.save.parent / "wide.ppm"], token=support.SPECTATOR_TOKEN)
        frame = self.ascii_frame(ascii_client, lambda f: f["state"] is not None and not f["busy"])
        self.assertEqual(frame["state"], seen["state"])
        o = seen["state"]["observation"]
        self.assertEqual({(c["position"]["x"], c["position"]["y"]) for c in o["visible_cells"] if c["place_hint"]}, {(0, 0), (5, 0)})
        self.assertEqual(o["ground_items"][0]["position"], fixture["item_offset"])
        cells = {(c["position"]["x"], c["position"]["y"]) for c in o["visible_cells"]}
        for x in range(-2, 7):
            for y in range(-1, 2):
                self.assertIn((x, y), cells)
        for hidden in ("region", "portal", "quarter_turns", "West space", "East space"):
            self.assertNotIn(hidden, json.dumps(seen))
        self.assertIn("offset (5, -1, 0)", wizard.command("look"))
        for command in fixture["walk"]:
            self.assertNotIn("Server error", wizard.command(command))
        arrived = self.request(observer, {"type": "snapshot"})
        self.assertEqual(arrived["state"]["observation"]["inventory"][0]["name"], "stone tablet")
        self.assertEqual(arrived["state"]["observation"]["tick"], 650)
        ascii_support.AsciiProcesses.key(self, ascii_client, "escape")
        ascii_client.child.wait(timeout=10)
        self.assertTrue((self.save.parent / "wide.ppm").exists())

    def test_rotated_sight_occlusion_memory_stairs_rewind_and_resume(self):
        server = self.server(wizard=True)
        wizard = self.launch("tor-client-text", ["--connect", self.address], token=headless_support.WIZARD_TOKEN)
        wizard.until(lambda line: line == "Ready.")
        observer, initial = self.client(support.SPECTATOR_TOKEN)
        ascii_client = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"], token=support.SPECTATOR_TOKEN)
        self.ascii_frame(ascii_client, lambda frame: frame["state"] is not None and not frame["busy"])
        fixture = json.loads((Path(__file__).parent / "scenarios/portal-geometry.json").read_text())
        for command in fixture["setup"]:
            self.assertNotIn("Server error", wizard.command(command))
        visible = self.request(observer, {"type": "snapshot"})
        remote = {"x": 0, "y": -3, "z": 0}
        o = visible["state"]["observation"]
        self.assertTrue(any(item["position"] == remote for item in o["ground_items"]))
        self.assertTrue(all("region" not in item["position"] for item in o["ground_items"]))
        self.assertNotIn("known_places", o)
        self.assertIn("offset (0, -3, 0)", wizard.command("look"))
        self.assertIn("Server error", wizard.command("take tablet"))  # Visible is not reachable.
        ascii_seen = self.ascii_frame(ascii_client, lambda frame: frame["state"]["revision"] == visible["state"]["revision"])
        self.assertEqual(ascii_seen["state"], visible["state"])
        self.assertNotIn("Server error", wizard.command("wizard wall 1 1 0 0 closed"))
        occluded = self.request(observer, {"type": "snapshot"})
        self.assertFalse(any(item["position"] == remote for item in occluded["state"]["observation"]["ground_items"]))
        old_memory = next(cell for cell in occluded["memory"] if cell["position"] == remote)
        self.assertEqual(len(old_memory["ground_items"]), 1)
        self.assertIn("Server error", wizard.command("wizard teleport 1 1 1 0 0"))
        wizard.command("wizard item token 3 1 2 1")
        hidden = self.request(observer, {"type": "snapshot"})
        self.assertEqual(next(cell for cell in hidden["memory"] if cell["position"] == remote), old_memory)
        wizard.command("wizard wall 1 1 0 0 open")
        refreshed = self.request(observer, {"type": "snapshot"})
        self.assertEqual(len(next(cell for cell in refreshed["memory"] if cell["position"] == remote)["ground_items"]), 2)
        for command in fixture["walk"]:
            self.assertNotIn("Server error", wizard.command(command))
        arrived = self.request(observer, {"type": "snapshot"})
        self.assertEqual(arrived["state"]["observation"]["position"], fixture["destination"])
        self.assertEqual(arrived["state"]["observation"]["tick"], fixture["tick"])
        ascii_seen = self.ascii_frame(ascii_client, lambda frame: frame["state"]["revision"] == arrived["state"]["revision"])
        self.assertEqual(ascii_seen["state"], arrived["state"])
        for client in (wizard, observer, ascii_client):
            client.stop()
        server.stop()
        server = self.server(wizard=True)
        wizard = self.launch("tor-client-text", ["--connect", self.address], token=headless_support.WIZARD_TOKEN)
        wizard.until(lambda line: line == "Ready.")
        observer, resumed = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"], arrived["state"])
        self.assertNotIn("Server error", wizard.command("wizard rewind initial"))
        rewound = self.request(observer, {"type": "snapshot"})
        self.assertNotEqual(rewound["branch"], initial["branch"])
        self.assertFalse(any(cell["key"] == old_memory["key"] for cell in rewound["memory"]))
        self.assertIn("Server error", wizard.command("wizard teleport 1 3 1 2 1"))


if __name__ == "__main__":
    unittest.main()
