"""Unnamed place hints through real text, headless and native ASCII processes."""
from test_text_process import flush_save
import json
from pathlib import Path
import unittest

import test_ascii_process as ascii_support
import test_headless_process as headless_support
import test_text_process as support


class PlaceHintProcesses(unittest.TestCase):
    launch = support.TextProcesses.launch
    setUp = headless_support.HeadlessProcesses.setUp
    server = headless_support.HeadlessProcesses.server
    client = headless_support.HeadlessProcesses.client
    frame = headless_support.HeadlessProcesses.frame
    command = headless_support.HeadlessProcesses.command
    request = headless_support.HeadlessProcesses.request
    ascii_frame = ascii_support.AsciiProcesses.frame

    @classmethod
    def setUpClass(cls):
        ascii_support.AsciiProcesses.setUpClass.__func__(cls)

    def test_perception_stale_memory_dynamic_removal_restart_and_rewind(self):
        server = self.server(wizard=True)
        wizard = self.launch("tor-client-text", ["--connect", self.address], token=headless_support.WIZARD_TOKEN)
        wizard.until(lambda line: line == "Ready.")
        observer, initial = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(sum(c["place_hint"] for c in initial["state"]["observation"]["visible_cells"]), 2)
        ascii_client = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"], token=support.SPECTATOR_TOKEN)
        self.ascii_frame(ascii_client, lambda f: f["state"] is not None and not f["busy"])
        fixture = json.loads((Path(__file__).parent / "scenarios/place-hints.json").read_text())
        for command in fixture["setup"]:
            self.assertNotIn("Server error", wizard.command(command))
        hidden = self.request(observer, {"type": "snapshot"})
        self.assertEqual(hidden["state"]["observation"], initial["state"]["observation"])
        self.assertEqual([(c["key"], c["place_hint"]) for c in hidden["memory"]],
                         [(c["key"], c["place_hint"]) for c in initial["memory"]])
        self.assertNotIn("Server error", wizard.command(fixture["visit"]))
        visited = self.request(observer, {"type": "snapshot"})
        marked = [c for c in visited["state"]["observation"]["visible_cells"] if c["place_hint"]]
        self.assertEqual(len(marked), 1)
        key = marked[0]["key"]
        self.assertEqual(marked[0]["position"], {"x": 1, "y": 0, "z": 0})
        shown = self.ascii_frame(ascii_client, lambda f: f["state"]["revision"] == visited["state"]["revision"])
        self.assertEqual(shown["state"], visited["state"])
        look = wizard.command("look")
        self.assertNotIn("Hidden authoring name", look)
        self.assertNotIn("place_hint", look)
        for forbidden in ("region", "portal", "Hidden authoring name"):
            self.assertNotIn(forbidden, json.dumps(visited))
        flush_save(self)
        before = self.save.read_bytes()
        denied = self.request(observer, {"type": "command", "branch": visited["branch"], "command": {
            "type": "wizard", "expected_revision": visited["state"]["revision"], "operation": "place 3 2 1 0 off"}})
        self.assertIsNotNone(denied["error"])
        self.assertEqual(self.save.read_bytes(), before)
        wizard.command(fixture["leave"])
        away = self.request(observer, {"type": "snapshot"})
        memory = next(c for c in away["memory"] if c["key"] == key)
        wizard.command(fixture["remove"])
        removed = self.request(observer, {"type": "snapshot"})
        self.assertEqual(next(c for c in removed["memory"] if c["key"] == key), memory)
        wizard.command(fixture["visit"])
        refreshed = self.request(observer, {"type": "snapshot"})
        self.assertFalse(next(c for c in refreshed["memory"] if c["key"] == key)["place_hint"])
        for client in (wizard, observer, ascii_client):
            client.stop()
        server.stop()
        self.server(wizard=True)
        wizard = self.launch("tor-client-text", ["--connect", self.address], token=headless_support.WIZARD_TOKEN)
        wizard.until(lambda line: line == "Ready.")
        observer, resumed = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"], refreshed["state"])
        wizard.command("wizard rewind initial")
        rewound = self.request(observer, {"type": "snapshot"})
        self.assertNotEqual(rewound["branch"], resumed["branch"])
        self.assertFalse(any(c["key"] == key for c in rewound["memory"]))
        self.assertEqual(sum(c["place_hint"] for c in rewound["state"]["observation"]["visible_cells"]), 2)


if __name__ == "__main__":
    unittest.main()
