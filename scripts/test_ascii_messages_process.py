"""The ASCII client's message area and single map through the actual server and
native window: combat reads as messages that stay until the next command, the
map merges heights into one view, and the NetHack-style screens open on demand."""
import unittest

from process_harness import ProcessTestCase


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
        self.assertEqual(initial["status_lines"][1].split()[0], "HP:50(50)")

        # Walk east into the scout until it dies. Every turn's messages are
        # only what happened since that command, and stay on screen.
        killed = None
        seen = []
        for _ in range(20):
            frame = self.key(window, "right")
            turn = frame["messages"]["turn"]
            self.assertNotIn("You can act again", turn)
            self.assertNotIn("You move east", turn)
            self.assertNotIn("HP ", turn)
            seen.append(turn)
            if "You kill the ruin scout!" in turn:
                killed = frame
                break
        self.assertIsNotNone(killed, seen)
        self.assertTrue(any("struck the ruin scout" in turn for turn in seen), seen)
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
