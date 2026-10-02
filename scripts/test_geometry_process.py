"""Portal geometry acceptance through server, text, headless and native ASCII."""
import json
import unittest

from process_harness import ProcessTestCase, SPECTATOR_TOKEN, WIZARD_TOKEN, load_fixture


class GeometryProcesses(ProcessTestCase):
    graphical = True

    def test_wide_join_is_one_continuous_scene_in_every_frontend(self):
        self.server(scenario="wide-join")
        player, _ = self.client()
        fixture = load_fixture("wide-join.json")
        observer, seen = self.client(SPECTATOR_TOKEN)
        ascii_client = self.launch("tor-client-ascii", ["--connect", self.address, "--automation", "--capture", self.save.parent / "wide.ppm"], token=SPECTATOR_TOKEN)
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
        self.play(player, fixture["walk"])
        arrived = self.request(observer, {"type": "snapshot"})
        self.assertEqual(arrived["state"]["observation"]["inventory"][0]["name"], "stone tablet")
        self.assertEqual(arrived["state"]["observation"]["tick"], 650)
        self.key(ascii_client, "escape")
        ascii_client.child.wait(timeout=10)
        self.assertTrue((self.save.parent / "wide.ppm").exists())

    def test_rotated_sight_occlusion_memory_stairs_rewind_and_resume(self):
        server = self.server(wizard=True, scenario="portal-geometry")
        # The wizard also plays the character, so it keeps control.
        wizard, _ = self.client(WIZARD_TOKEN)
        observer, initial = self.client(SPECTATOR_TOKEN)
        ascii_client = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"], token=SPECTATOR_TOKEN)
        initial_ascii = self.ascii_frame(ascii_client, lambda frame: frame["state"] is not None and not frame["busy"])
        fixture = load_fixture("portal-geometry.json")
        visible = self.request(observer, {"type": "snapshot"})
        remote = {"x": 0, "y": -3, "z": 0}
        o = visible["state"]["observation"]
        self.assertTrue(any(item["position"] == remote for item in o["ground_items"]))
        self.assertTrue(all("region" not in item["position"] for item in o["ground_items"]))
        self.assertNotIn("known_places", o)
        tablet = next(i["item"]["id"] for i in o["ground_items"] if i["position"] == remote)
        self.assertIsNotNone(self.act(wizard, {"type": "take", "item": tablet})["error"])  # Visible is not reachable.
        ascii_seen = initial_ascii
        self.assertEqual(ascii_seen["state"], visible["state"])
        self.wizard_command(wizard, "wall 1 1 0 0 closed")
        occluded = self.request(observer, {"type": "snapshot"})
        self.assertFalse(any(item["position"] == remote for item in occluded["state"]["observation"]["ground_items"]))
        old_memory = next(cell for cell in occluded["memory"] if cell["position"] == remote)
        self.assertEqual(len(old_memory["ground_items"]), 1)
        self.wizard_command(wizard, "teleport 1 1 1 0 0", succeeds=False)
        self.wizard_command(wizard, "item token 3 1 2 1")
        hidden = self.request(observer, {"type": "snapshot"})
        self.assertEqual(next(cell for cell in hidden["memory"] if cell["position"] == remote), old_memory)
        self.wizard_command(wizard, "wall 1 1 0 0 open")
        refreshed = self.request(observer, {"type": "snapshot"})
        self.assertEqual(len(next(cell for cell in refreshed["memory"] if cell["position"] == remote)["ground_items"]), 2)
        self.play(wizard, fixture["walk"])
        arrived = self.request(observer, {"type": "snapshot"})
        self.assertEqual(arrived["state"]["observation"]["position"], fixture["destination"])
        self.assertEqual(arrived["state"]["observation"]["tick"], fixture["tick"])
        ascii_seen = self.ascii_frame(ascii_client, lambda frame: frame["state"]["revision"] == arrived["state"]["revision"])
        self.assertEqual(ascii_seen["state"], arrived["state"])
        for client in (wizard, observer, ascii_client):
            client.stop()
        server.stop()
        server = self.server(wizard=True)
        wizard = self.wizard()
        observer, resumed = self.client(SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"], arrived["state"])
        self.wizard_command(wizard, "rewind initial")
        rewound = self.request(observer, {"type": "snapshot"})
        self.assertNotEqual(rewound["branch"], initial["branch"])
        self.assertEqual(rewound["state"]["observation"], initial["state"]["observation"])
        self.wizard_command(wizard, "teleport 1 3 1 2 1")


if __name__ == "__main__":
    unittest.main()
