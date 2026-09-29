"""Three-dimensional sight through the actual server and all three frontends.

The `sight-3d-setup` package's character is two cells tall and sees from its
head. It looks over a waist-high wall, sees a creature hovering at head height,
and can't see past a closed door that fills its two-cell doorway until it opens
the door. See docs/sight-3d.md.
"""
import unittest
import test_text_process as support
import test_headless_process as headless
import test_ascii_process as ascii_support
import test_adventure_process as adventure_support
from test_text_process import inspect_save


class SightProcesses(unittest.TestCase):
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

    @staticmethod
    def cell(observation, x, y, z):
        return next((c for c in observation["visible_cells"]
                     if c["position"] == {"x": x, "y": y, "z": z}), None)

    @staticmethod
    def items(observation):
        return {i["item"]["name"] for i in observation["ground_items"]}

    def test_eye_height_hovering_creature_and_tall_door_across_clients_and_resume(self):
        server = self.server(scenario="sight-3d-setup")
        observer, initial = self.client(support.SPECTATOR_TOKEN)
        view = initial["state"]["observation"]
        # Over the waist-high wall, but not past the closed door.
        self.assertTrue(self.cell(view, -2, 0, 0)["wall"])
        self.assertEqual(self.items(view), {"copper token"})
        # The hovering creature, at head height.
        self.assertIn({"x": -3, "y": -1, "z": 1}, [a["position"] for a in view["visible_actors"]])
        # Both cells of the door show the same closed door.
        doors = [self.cell(view, 1, 0, z)["door"] for z in (0, 1)]
        self.assertEqual({d["id"] for d in doors}, {1})
        self.assertFalse(any(d["open"] for d in doors))
        # Floor and ceiling are seen solid cells.
        for z in (-1, 2):
            self.assertEqual((self.cell(view, 0, 0, z)["wall"], self.cell(view, 0, 0, z)["material"]),
                             (True, "stone"))
        text, welcome = self.adventure(support.SPECTATOR_TOKEN)
        self.assertIn("copper token", welcome)
        self.assertNotIn("stone tablet", welcome)
        self.assertIn("stone", self.say(text, "examine ceiling"))
        self.assertIn("stone", self.say(text, "examine floor"))
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"])
        native = self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"])
        self.assertEqual(native["state"], initial["state"])
        self.key(window, "open_door")
        opened = self.key(window, "right")
        seen = opened["state"]["observation"]
        self.assertTrue(self.cell(seen, 1, 0, 1)["door"]["open"])
        self.assertEqual(self.items(seen), {"copper token", "stone tablet"})
        watched = self.request(observer, {"type": "snapshot"})
        self.assertEqual(watched["state"], opened["state"])
        self.key(window, "escape")
        self.assertEqual(window.child.wait(timeout=10), 0)
        text.stop(); observer.stop(); server.stop()
        self.assertEqual(inspect_save(self.save)["ruleset"], "dungeon-v17")
        self.server(scenario="sight-3d-setup")
        _, resumed = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"], opened["state"])


if __name__ == "__main__":
    unittest.main()
