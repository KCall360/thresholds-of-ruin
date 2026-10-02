"""Shadowcasting disclosure through real text, headless and graphical clients."""
import json
import os
from pathlib import Path
import unittest

from process_harness import ProcessTestCase, SPECTATOR_TOKEN, inspect_save


class ShadowcastingProcesses(ProcessTestCase):
    graphical = True

    def test_door_corner_disclosure_agrees_across_clients_and_resume(self):
        server = self.server(wizard=True, scenario="shadowcasting")
        wizard = self.wizard()
        observer, initial = self.client(SPECTATOR_TOKEN)
        view = initial["state"]["observation"]
        self.assertEqual([i["item"]["name"] for i in view["ground_items"]],["copper token"])
        self.assertTrue(any(c["position"] == {"x":1,"y":-1,"z":0} for c in view["visible_cells"]))
        self.assertFalse(any(c["position"] == {"x":2,"y":0,"z":0} for c in view["visible_cells"]))
        text, welcome = self.adventure(SPECTATOR_TOKEN)
        self.assertIn("copper token",welcome)
        self.assertNotIn("stone tablet",welcome)
        capture=Path(os.environ.get("TOR_SHADOW_CAPTURE",str(self.save.parent / "shadow.ppm")))
        window=self.launch("tor-client-ascii",["--connect",self.address,"--automation","--capture",capture])
        native=self.ascii_frame(window,lambda f:f["state"] is not None and not f["busy"])
        self.assertEqual(native["state"],initial["state"])
        self.key(window,"open_door")
        opened=self.key(window,"right")
        self.assertEqual(len(opened["state"]["observation"]["ground_items"]),2)
        self.key(window,"close_door")
        closed=self.key(window,"right")
        self.assertEqual(closed["state"]["observation"]["ground_items"],view["ground_items"])
        self.assertEqual(closed["state"]["observation"]["tick"],view["tick"]+200)
        watched=self.request(observer,{"type":"snapshot"})
        self.assertEqual(watched["state"],closed["state"])
        # Hidden objects remain only in explicitly stale client memory.
        self.assertNotIn("stone tablet",json.dumps(watched["state"]))
        self.assertIn("stone tablet",json.dumps(watched["memory"]))
        self.key(window,"escape")
        self.assertEqual(window.child.wait(timeout=10),0)
        self.assertTrue(capture.read_bytes().startswith(b"P6\n1200 800\n255\n"))
        text.stop(); observer.stop(); wizard.stop(); server.stop()
        self.assertEqual(inspect_save(self.save)["ruleset"],"dungeon-v17")
        self.server(wizard=True)
        _, resumed=self.client(SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"],closed["state"])

if __name__ == "__main__":
    unittest.main()
