"""Native ASCII remembered map, using only received client observations."""
import json
import os
import shutil
import unittest

from process_harness import ProcessTestCase, TOKEN, SPECTATOR_TOKEN


class AsciiMemoryProcesses(ProcessTestCase):
    graphical = True

    def window(self, token=TOKEN):
        self.capture = self.save.parent / "memory.ppm"
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--automation", "--capture", self.capture], token=token)
        return window, self.frame(window, lambda f: f.get("state") and not f["busy"])

    def tile(self, frame, x, y=0, z=0):
        return next(t for t in frame["map_tiles"] if t["position"] == {"x":x,"y":y,"z":z})

    def test_normal_occlusion_movement_pickup_refresh_and_fresh_resume(self):
        server = self.server()
        observer, _ = self.client(SPECTATOR_TOKEN)
        window, _ = self.window()
        for _ in range(3): self.key(window,"right")
        self.key(window,"close_door")
        closed = self.key(window,"right")
        remembered = self.tile(closed,4)
        self.assertEqual(remembered["glyph"],"!")
        self.assertTrue(remembered["remembered"])
        self.assertEqual(remembered["color"],0x626262)
        # Verify the actual presented pixels, not only the diagnostic model.
        pixels = self.capture.read_bytes().split(b"\n",3)[3]
        cx,cy = remembered["center"]
        colors = [pixels[(y*1200+x)*3:(y*1200+x)*3+3]
                  for y in range(cy-4,cy+4) for x in range(cx-4,cx+4)]
        self.assertIn(bytes([98,98,98]),colors)
        if os.environ.get("TOR_MEMORY_CAPTURE"):
            shutil.copyfile(self.capture, os.environ["TOR_MEMORY_CAPTURE"])
        self.assertNotIn("stone tablet",json.dumps(closed["state"]))
        self.assertEqual(self.request(observer,{"type":"snapshot"})["state"],closed["state"])
        shifted = self.key(window,"left")
        self.assertTrue(self.tile(shifted,5)["remembered"])
        self.assertEqual(self.tile(shifted,5)["glyph"],"!")
        self.key(window,"right")
        self.key(window,"open_door")
        opened = self.key(window,"right")
        self.assertFalse(self.tile(opened,4)["remembered"])
        for _ in range(4): self.key(window,"right")
        self.key(window,"pickup")
        for _ in range(4): self.key(window,"left")
        self.key(window,"close_door")
        refreshed = self.key(window,"right")
        self.assertEqual(self.tile(refreshed,4)["glyph"],".")
        self.assertTrue(self.tile(refreshed,4)["remembered"])
        self.key(window,"escape")
        window.child.wait(timeout=10)
        observer.stop(); server.stop()
        self.server()
        _, resumed = self.window()
        self.assertEqual(resumed["state"],refreshed["state"])
        self.assertFalse(any(t["remembered"] for t in resumed["map_tiles"]))

    def test_rotated_crossing_keeps_item_aligned_and_rewind_clears_chart(self):
        self.server(wizard=True, scenario="ascii-memory-rotated")
        wizard = self.wizard()
        window, initial = self.window()
        self.assertEqual(self.tile(initial,-1)["glyph"],"!")
        self.key(window,"right")
        self.key(window,"right")
        self.key(window,"close_door")
        hidden = self.key(window,"left")
        self.assertEqual(self.tile(hidden,-3)["glyph"],"!")
        self.assertTrue(self.tile(hidden,-3)["remembered"])
        self.wizard_command(wizard, "rewind initial")
        rewound = self.frame(window,lambda f:f.get("branch") != hidden["branch"])
        self.assertFalse(any(t["remembered"] for t in rewound["map_tiles"]))

    def test_spectator_hides_unseen_actor_but_retains_item_and_ignores_hidden_changes(self):
        self.server(wizard=True, scenario="ascii-memory-actors")
        wizard = self.wizard()
        window, initial = self.window(SPECTATOR_TOKEN)
        self.assertEqual(self.tile(initial,4)["glyph"],"&")
        self.wizard_command(wizard, "wall 3 2 0 0 closed")
        hidden = self.frame(window,lambda f:f.get("state",{}).get("revision",0)>initial["state"]["revision"])
        self.assertEqual(self.tile(hidden,4)["glyph"],"!")
        self.assertTrue(self.tile(hidden,4)["remembered"])
        self.assertFalse(any(t["glyph"] == "&" for t in hidden["map_tiles"]))
        self.wizard_command(wizard, "item token 3 3 0 0")
        stale = self.frame(window,lambda f:f.get("state",{}).get("revision",0)>hidden["state"]["revision"])
        self.assertEqual(self.tile(stale,3)["glyph"],".")
        self.assertTrue(self.tile(stale,3)["remembered"])
        # Snapshot refresh cannot disclose the hidden new token.
        self.wizard_command(wizard, "wall 3 2 0 0 open")
        seen = self.frame(window,lambda f:f.get("state",{}).get("revision",0)>=hidden["state"]["revision"]+2)
        self.assertEqual(self.tile(seen,3)["glyph"],"!")
        self.assertFalse(self.tile(seen,3)["remembered"])
        self.assertEqual(self.tile(seen,4)["glyph"],"&")


if __name__ == "__main__":
    unittest.main()
