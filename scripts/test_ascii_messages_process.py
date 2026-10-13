"""The ASCII client's message area and single map through the actual server and
native window: combat reads as messages that stay until the next command, the
map merges heights into one view, and the NetHack-style screens open on demand."""
import json
import time
import unittest

from process_harness import ProcessTestCase, ascii_input_completion


class AsciiMessageProcesses(ProcessTestCase):
    graphical = True

    def window(self):
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"])
        frame = self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"])
        return window, frame

    def test_fight_messages_persist_and_the_single_map_names_creatures(self):
        self.server(scenario="first-dungeon")
        window, initial = self.window()
        tiles = initial["map_tiles"]
        # One @, and the player's head (disclosed one cell up) is no creature.
        self.assertEqual([t["glyph"] for t in tiles].count("@"), 1)
        self.assertEqual(len({(t["position"]["x"], t["position"]["y"]) for t in tiles}), len(tiles))
        scout = next(t for t in tiles if t["kind"] == "creature")
        self.assertEqual(scout["glyph"], "s")
        health = initial["state"]["observation"]["combat"]
        self.assertEqual(len(health["own_stats"]["hit_dice"]), 4)
        self.assertEqual(initial["status_lines"][1].split()[0],
                         f"HP:{health['hp']}({health['max_hp']})")

        # Follow the disclosed scout, including its ordinary low-Health retreat.
        # A headless seed-42 probe takes 12 admitted moves/attacks with this build.
        # Wait through intention completion and recovery before the next input.
        # Keep twice that count plus eight inputs for bounded stale-revision
        # retries, rather than assuming the first hit kills. This is not a
        # native count; the postmerge Windows trace exposed early-start races.
        # Every turn's messages are only what happened since that command.
        directions = {(0, -1): "up", (1, -1): "north_east", (1, 0): "right",
                      (1, 1): "south_east", (0, 1): "down", (-1, 1): "south_west",
                      (-1, 0): "left", (-1, -1): "north_west"}
        killed = None
        seen = []
        trace = []
        current = initial
        deadline = time.monotonic() + 30
        for _ in range(32):
            view = current["state"]["observation"]
            target = next((a for a in view["visible_actors"]
                           if a["name"] == "ruin scout" and a["position"]["z"] == 0), None)
            self.assertIsNotNone(target, f"Scout disappeared without death narration: {trace!r}")
            self.assertFalse(view["combat"]["dead"], trace)
            remaining = deadline - time.monotonic()
            self.assertGreater(remaining, 0, f"Fight input deadline: {trace!r}")
            x, y = target["position"]["x"], target["position"]["y"]
            key = directions[((x > 0) - (x < 0), (y > 0) - (y < 0))]
            # Use the same native automation event and completion predicate as
            # key(), but cap this wait by the remaining whole-fight deadline.
            window.write(json.dumps({"type": "key", "key": key}))
            frame = self.ascii_frame(window, ascii_input_completion(key, wait_for_ready=True),
                                     seconds=remaining)
            current = frame
            turn = frame["messages"]["turn"]
            self.assertNotIn("You can act again", turn)
            self.assertNotIn("You move east", turn)
            self.assertNotIn("HP ", turn)
            seen.append(turn)
            observation = frame["state"]["observation"]
            trace.append({"key": key, "input_done": frame.get("input_done"),
                          "frame": frame["frame"], "tick": observation["tick"],
                          "ready": observation["ready"], "busy": frame["busy"],
                          "status": frame.get("status"),
                          "intentions": [i["phase"] for i in (frame.get("intentions") or [])],
                          "scout": [a["position"] for a in observation["visible_actors"]
                                    if a["name"] == "ruin scout" and a["position"]["z"] == 0],
                          "turn": turn})
            if "The ruin scout died." in turn:
                killed = frame
                break
        self.assertIsNotNone(killed, {"messages": seen, "input_trace": trace})
        self.assertIn("You used Attack on the ruin scout.", killed["messages"]["turn"])
        self.assertEqual(killed["messages"]["shown"][0][:24], killed["messages"]["turn"][:24])
        # Earlier turns went to the log.
        self.assertTrue(any("ruin scout" in turn for turn in killed["messages"]["log"]))

        # Look costs no time and names what's there.
        before = killed["state"]
        self.key(window, "look")
        looked = self.key(window, "look")
        self.assertEqual(looked["state"], before)
        self.assertEqual(looked["messages"]["turn"].split("  ")[-1], "That's you.")

        # The inventory and message log are screens, not panels.
        shown = self.key(window, "inventory")
        self.assertTrue(shown["screen"]["inventory"])
        closed = self.key(window, "inventory")
        self.assertFalse(closed["screen"]["inventory"])
        logged = self.key(window, "message_log")
        self.assertEqual(logged["screen"]["message_log"], 0)
        self.assertEqual(self.key(window, "escape")["screen"]["message_log"], None)
        self.assertEqual(self.key(window, "escape")["state"], before)
        self.assertEqual(window.child.wait(timeout=10), 0)


if __name__ == "__main__":
    unittest.main()
