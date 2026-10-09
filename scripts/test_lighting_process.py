"""Ambient lighting and remembered destinations through actual client processes."""

import json
import unittest
from process_harness import ProcessTestCase, SPECTATOR_TOKEN


class LightingProcesses(ProcessTestCase):
    graphical = True

    def test_disclosure_local_awareness_memory_and_resume(self):
        server = self.server(scenario="lighting")
        player, initial = self.client()
        spectator, watched = self.client(SPECTATOR_TOKEN)
        view = initial["state"]["observation"]
        self.assertEqual(view, watched["state"]["observation"])
        self.assertEqual(
            {i["item"]["name"] for i in view["ground_items"]}, {"lit tablet"}
        )
        self.assertNotIn("hidden token", json.dumps(initial))
        positions = {
            tuple(c["position"][axis] for axis in ("x", "y", "z"))
            for c in view["visible_cells"]
        }
        self.assertIn((12, 0, 0), positions)
        self.assertNotIn((6, 0, 0), positions)
        for z in (-1, 0, 1, 2):
            for y in (-1, 0, 1):
                for x in (-1, 0, 1):
                    self.assertIn((x, y, z), positions)
        text, welcome = self.adventure(SPECTATOR_TOKEN)
        self.assertIn("lit tablet", welcome)
        self.assertNotIn("hidden token", welcome)
        window, frame = self.window(token=SPECTATOR_TOKEN, observe=True)
        self.assertEqual(frame["state"], initial["state"])
        for _ in range(5):
            moved = self.act(player, {"type": "move", "direction": "east"})
            self.assertIsNone(moved.get("error"))
        self.assertIn(
            "hidden token", json.dumps(moved["state"]["observation"]["ground_items"])
        )
        self.flush_save()
        expected = self.request(player, {"type": "snapshot"})["state"]
        for child in (player, spectator, text, window):
            child.stop()
        server.stop()
        self.server(scenario="lighting")
        _, resumed = self.client()
        self.assertEqual(resumed["state"], expected)

    def test_ascii_travels_to_remembered_dark_cells_by_keyboard_and_click(self):
        self.server(scenario="lighting")
        native = self.launch(
            "tor-client-ascii", ["--connect", self.address, "--report-frames"]
        )
        self.ascii_frame(native, lambda f: f.get("state") is not None and not f["busy"])
        key = self.native_keys(native)
        key("Right", True)
        self.ascii_frame(
            native,
            lambda f: int(f["state"]["observation"]["tick"]) == 100 and not f["busy"],
        )
        key("Right", False)
        native.stop()
        window, initial = self.window()
        for _ in range(4):
            moved = self.key(window, "right")
        destination = next(
            tile
            for tile in moved["map_tiles"]
            if tile["position"] == {"x": -5, "y": 0, "z": 0}
        )
        self.assertTrue(destination["remembered"])
        self.key(window, "travel")
        for _ in range(5):
            self.key(window, "left")
        self.key(window, "enter")
        arrived = self.ascii_frame(
            window, lambda f: (f.get("travel") or {}).get("phase") == "arrived"
        )
        self.assertEqual(arrived["travel"]["completed_steps"], "5")
        for _ in range(5):
            moved = self.key(window, "right")
        destination = next(
            tile
            for tile in moved["map_tiles"]
            if tile["position"] == {"x": -5, "y": 0, "z": 0}
        )
        self.assertTrue(destination["remembered"])
        window.write(
            json.dumps(
                {
                    "type": "click",
                    "x": destination["center"][0],
                    "y": destination["center"][1],
                }
            )
        )
        again = self.ascii_frame(
            window,
            lambda f: (
                (f.get("travel") or {}).get("phase") == "arrived"
                and f["travel"]["id"] != arrived["travel"]["id"]
            ),
        )
        self.assertEqual(again["travel"]["completed_steps"], "5")


if __name__ == "__main__":
    unittest.main()
