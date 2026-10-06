"""Three-dimensional sight through the actual server and all three frontends.

The `sight-3d` package's character is two cells tall and sees from its
head. It looks over a waist-high wall, sees a creature hovering at head height,
and can't see past a closed door that fills its two-cell doorway until it opens
the door. The `sight-3d-giant` package compares a three-cell giant with a
two-cell humanoid at the same start. See docs/sight-3d.md.
"""
import unittest

from process_harness import ProcessTestCase, SPECTATOR_TOKEN, inspect_save


class SightProcesses(ProcessTestCase):
    graphical = True

    @staticmethod
    def cell(observation, x, y, z):
        return next((c for c in observation["visible_cells"]
                     if c["position"] == {"x": x, "y": y, "z": z}), None)

    @staticmethod
    def items(observation):
        return {i["item"]["name"] for i in observation["ground_items"]}

    def test_eye_height_hovering_creature_and_tall_door_across_clients_and_resume(self):
        server = self.server(scenario="sight-3d")
        observer, initial = self.client(SPECTATOR_TOKEN)
        view = initial["state"]["observation"]
        # Over the waist-high wall, but not past the closed door.
        self.assertTrue(self.cell(view, -2, 0, 0)["wall"])
        self.assertEqual(self.items(view), {"copper token"})
        # The hovering creature, at head height.
        self.assertIn({"x": -3, "y": -1, "z": 1}, [a["position"] for a in view["visible_actors"]])
        # Both cells of the door show the same closed door.
        doors = [self.cell(view, 1, 0, z)["door"] for z in (0, 1)]
        self.assertEqual({d["id"] for d in doors}, {"1"})
        self.assertFalse(any(d["open"] for d in doors))
        # Floor and ceiling are seen solid cells.
        for z in (-1, 2):
            self.assertEqual((self.cell(view, 0, 0, z)["wall"], self.cell(view, 0, 0, z)["material"]),
                             (True, "stone"))
        text, welcome = self.adventure(SPECTATOR_TOKEN)
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
        self.assertEqual(inspect_save(self.save)["ruleset"], "dungeon-v21")
        self.server(scenario="sight-3d")
        _, resumed = self.client(SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"], opened["state"])

    def test_three_cell_giant_sees_over_a_wall_a_humanoid_cannot(self):
        # The `sight-3d-giant` package, once per character: the same start,
        # hall and creature; only the selected character's body differs.
        creature = {"x": 5, "y": 0, "z": 1}
        for character, giant in ((1, True), (2, False)):
            self.save = self.save.with_name(f"game-{character}.json")
            server = self.server(scenario="sight-3d-giant", character=character)
            client = self.launch("tor-client-headless", ["--connect", self.address, "--actor", str(character)])
            state = self.frame(client, lambda f: f["type"] == "ready")["state"]
            view = state["observation"]
            # The top of the wall two cells high, and the stone ceiling.
            self.assertTrue(self.cell(view, 2, 0, 1)["wall"])
            self.assertTrue(self.cell(view, 0, 0, 3)["wall"])
            # Waist height beyond the wall: the giant sees down over it past
            # the cell in its lee; the humanoid sees nothing there.
            beyond = [c["position"]["x"] for c in view["visible_cells"]
                      if c["position"]["y"] == 0 and c["position"]["z"] <= 1 and c["position"]["x"] > 2]
            self.assertEqual(sorted(beyond), [4, 5, 6, 7] if giant else [])
            positions = [a["position"] for a in view["visible_actors"]]
            self.assertEqual(creature in positions, giant)
            if giant:
                window = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"],
                                     token=SPECTATOR_TOKEN)
                native = self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"])
                self.assertEqual(native["state"], state)
                self.key(window, "escape")
                self.assertEqual(window.child.wait(timeout=10), 0)
            client.stop(); server.stop()


if __name__ == "__main__":
    unittest.main()
