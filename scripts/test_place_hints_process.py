"""Unnamed place hints through real text, headless and native ASCII processes."""
from test_text_process import flush_save
import json
from pathlib import Path
import unittest

import test_ascii_process as ascii_support
import test_headless_process as headless_support
import test_text_process as support
import test_adventure_process as adventure_support


class PlaceHintProcesses(unittest.TestCase):
    launch = support.TextProcesses.launch
    setUp = headless_support.HeadlessProcesses.setUp
    server = headless_support.HeadlessProcesses.server
    client = headless_support.HeadlessProcesses.client
    frame = headless_support.HeadlessProcesses.frame
    command = headless_support.HeadlessProcesses.command
    request = headless_support.HeadlessProcesses.request
    ascii_frame = ascii_support.AsciiProcesses.frame
    key = ascii_support.AsciiProcesses.key
    adventure = adventure_support.AdventureProcesses.adventure
    say = adventure_support.AdventureProcesses.say

    @classmethod
    def setUpClass(cls):
        ascii_support.AsciiProcesses.setUpClass.__func__(cls)

    def test_durable_names_in_real_clients_save_reconnect_and_rewind(self):
        server = self.server(wizard=True)
        player, _ = self.adventure()
        observer, initial = self.client(support.SPECTATOR_TOKEN)
        places = initial["state"]["observation"]["places"]
        self.assertEqual(len(places), 2)
        self.assertTrue(all(not p["name"].startswith("Place ") for p in places))
        self.assertIn(places[0]["name"], self.say(player, "places"))
        self.assertIn("Hearth of Echoes", self.say(player, "name 1 Hearth of Echoes"))
        renamed = self.request(observer, {"type": "snapshot"})
        self.assertEqual(renamed["state"]["observation"]["tick"], initial["state"]["observation"]["tick"])
        self.assertEqual(renamed["state"]["observation"]["places"][0]["name"], "Hearth of Echoes")
        denied = self.request(observer, {"type": "command", "branch": renamed["branch"], "command": {
            "type": "rename_place", "expected_revision": renamed["state"]["revision"], "key": places[0]["key"], "name": "Forbidden"}})
        self.assertIsNotNone(denied["error"])
        self.say(player, "release")
        player.stop()
        native = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"])
        self.ascii_frame(native, lambda f: f["state"] is not None and not f["busy"])
        self.key(native, "control")
        shown = self.key(native, "places")
        self.assertTrue(shown["places_open"])
        self.assertEqual(shown["state"]["observation"]["places"][0]["name"], "Hearth of Echoes")
        self.key(native, "enter")
        native.child.stdin.write(json.dumps({"type": "text", "text": "Lantern Dream"}) + "\n")
        native.child.stdin.flush()
        changed = self.key(native, "enter")
        self.assertEqual(changed["state"]["observation"]["places"][0]["name"], "Lantern Dream")
        self.key(native, "escape")
        self.key(native, "release")
        wizard = self.launch("tor-client-text", ["--connect", self.address], token=headless_support.WIZARD_TOKEN)
        wizard.until(lambda line: line == "Ready.")
        fixture = json.loads((Path(__file__).parent / "scenarios/place-hints.json").read_text())
        for operation in fixture["setup"]:
            self.assertNotIn("Server error", wizard.command(operation))
        wizard.command(fixture["visit"])
        discovered = self.request(observer, {"type": "snapshot"})
        self.assertEqual(len(discovered["state"]["observation"]["places"]), 3)
        wizard.command(fixture["leave"])
        wizard.command(fixture["remove"])
        away = self.request(observer, {"type": "snapshot"})
        self.assertEqual(away["state"]["observation"]["places"], discovered["state"]["observation"]["places"])
        player, _ = self.adventure()
        self.assertIn("remembered", self.say(player, "places"))
        self.assertNotIn("Hidden authoring name", self.say(player, "places"))
        self.assertNotIn("Server error", wizard.command("save"))
        for process in [native, wizard, player, observer]:
            process.stop()
        server.stop()
        server = self.server(wizard=True)
        observer, resumed = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"]["observation"]["places"], away["state"]["observation"]["places"])
        player, _ = self.adventure()
        self.assertIn("Lantern Dream", self.say(player, "places"))
        wizard = self.launch("tor-client-text", ["--connect", self.address], token=headless_support.WIZARD_TOKEN)
        wizard.until(lambda line: line == "Ready.")
        wizard.command("wizard rewind initial")
        rewound = self.request(observer, {"type": "snapshot"})
        self.assertEqual(rewound["state"]["observation"]["places"], places)

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
