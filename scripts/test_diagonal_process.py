"""Diagonal gameplay through actual native ASCII, text and headless processes."""
from test_text_process import flush_save, inspect_save
import json
import os
from pathlib import Path
import unittest

import test_text_process as support
import test_headless_process as headless
import test_ascii_process as ascii_support
import test_adventure_process as adventure_support
import test_doors_process as doors


class DiagonalProcesses(unittest.TestCase):
    setUpClass = classmethod(ascii_support.AsciiProcesses.setUpClass.__func__)
    setUp = headless.HeadlessProcesses.setUp
    launch = support.TextProcesses.launch
    server = headless.HeadlessProcesses.server
    client = headless.HeadlessProcesses.client
    frame = headless.HeadlessProcesses.frame
    command = headless.HeadlessProcesses.command
    request = headless.HeadlessProcesses.request
    act = headless.HeadlessProcesses.act
    adventure = adventure_support.AdventureProcesses.adventure
    say = adventure_support.AdventureProcesses.say
    native_keys = doors.DoorProcesses.native_keys
    key = ascii_support.AsciiProcesses.key

    def setup_wizard(self, section):
        wizard = self.launch("tor-client-text", ["--connect", self.address], token=headless.WIZARD_TOKEN)
        wizard.until(lambda line: line == "Ready.")
        fixture = json.loads((Path(__file__).parent / "scenarios/diagonal.json").read_text())
        for command in fixture[section]:
            self.assertNotIn("Server error", wizard.command(command))
        wizard.command("release")
        return wizard

    def test_normal_native_keys_notes_spectator_and_resume(self):
        server = self.server()
        observer, _ = self.client(support.SPECTATOR_TOKEN)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames", "--capture", os.environ.get("TOR_DIAGONAL_CAPTURE", str(self.save.parent / "diagonal.ppm"))])
        current = self.frame(window, lambda f: f.get("state") and not f["busy"])
        key = self.native_keys(window)
        for name, direction in [("y","north_west"),("n","south_east"),("u","north_east"),("b","south_west")]:
            revision = current["state"]["revision"]
            tick = current["state"]["observation"]["tick"]
            key(name, True)
            current = self.frame(window, lambda f: f.get("state",{}).get("revision",0) > revision and not f["busy"])
            key(name, False)
            self.assertEqual(current["state"]["observation"]["tick"], tick + 142)
            self.assertEqual(current["history"][-1]["content"]["event"]["direction"], direction)
            self.assertIsNone(current["note"])
        key("F4", True)
        note = self.frame(window, lambda f: f.get("note") == "")
        key("F4", False)
        self.assertEqual(note["state"], current["state"])
        key("Escape", True)
        self.frame(window, lambda f: f.get("note") is None)
        key("Escape", False)
        watched = self.request(observer, {"type":"snapshot"})
        self.assertEqual(watched["state"], current["state"])
        flush_save(self)
        before = self.save.read_bytes()
        denied = self.act(observer, {"type":"move","direction":"north_east"})
        self.assertIsNotNone(denied["error"])
        self.assertEqual(self.save.read_bytes(),before)
        window.stop(); observer.stop(); server.stop()
        self.server()
        _, resumed = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"],current["state"])
        self.assertEqual(inspect_save(self.save)["ruleset"],"diagonal-v11")

    def test_diagonal_doors_corner_travel_and_rewind(self):
        self.server(wizard=True)
        wizard = self.setup_wizard("doors")
        observer, initial = self.client(support.SPECTATOR_TOKEN)
        player, _ = self.adventure()
        self.assertEqual(self.say(player,"open door"),"You open the wooden door.\n> ")
        opened = self.request(observer,{"type":"snapshot"})
        self.assertEqual(opened["state"]["observation"]["tick"],100)
        self.assertIn("You walk northeast.",self.say(player,"ne"))
        arrived = self.request(observer,{"type":"snapshot"})
        self.assertEqual(arrived["state"]["observation"]["tick"],384)
        self.say(player,"step sw")
        self.say(player,"step southwest")
        self.assertEqual(self.say(player,"close door"),"You close the wooden door.\n> ")
        player.stop()
        window = self.launch("tor-client-ascii",["--connect",self.address,"--report-frames"])
        self.frame(window,lambda f:f.get("state") and not f["busy"])
        key = self.native_keys(window)
        key("o",True)
        self.frame(window,lambda f:f.get("door_direction") is True)
        key("o",False)
        key("u",True)
        reopened = self.frame(window,lambda f:f.get("state",{}).get("observation",{}).get("tick")==868 and not f["busy"])
        key("u",False)
        self.assertTrue(doors.DoorProcesses.door(reopened)["open"])
        window.stop()
        wizard.command("sync")
        self.assertNotIn("Server error",wizard.command("wizard wall 3 3 2 0 closed"))
        wizard.command("control")
        before = self.request(observer,{"type":"snapshot"})
        self.assertIn("Server error",wizard.command("ne"))
        self.assertEqual(self.request(observer,{"type":"snapshot"})["state"],before["state"])
        self.assertNotIn("Server error",wizard.command("wizard rewind initial"))
        rewound = self.request(observer,{"type":"snapshot"})
        self.assertNotEqual(rewound["branch"],initial["branch"])
        self.assertEqual(rewound["state"]["observation"]["tick"],0)

    def test_rotated_crossing_keeps_observer_axes(self):
        self.server(wizard=True)
        self.setup_wizard("rotated")
        player, _ = self.client()
        moved = self.act(player,{"type":"move","direction":"north_east"})
        self.assertIsNone(moved["error"])
        self.assertEqual(moved["state"]["observation"]["tick"],142)
        # Observer east is local south after the rotated crossing.
        moved = self.act(player,{"type":"move","direction":"east"})
        self.assertIsNone(moved["error"])
        tablet = next(i for i in moved["state"]["observation"]["ground_items"] if i["item"]["name"]=="stone tablet")
        self.assertTrue(tablet["reachable"])
        self.assertEqual(moved["state"]["observation"]["tick"],242)

    def test_native_stair_bindings_do_not_also_wait(self):
        self.server(wizard=True)
        self.setup_wizard("stairs")
        window = self.launch("tor-client-ascii",["--connect",self.address,"--report-frames"])
        self.frame(window,lambda f:f.get("state") and not f["busy"])
        observer, _ = self.client(support.SPECTATOR_TOKEN)
        key = self.native_keys(window)
        for name, direction, tick in [("comma","up",100),("period","down",200)]:
            key("Shift_L",True)
            key(name,True)
            current = self.frame(window,lambda f:f.get("state",{}).get("observation",{}).get("tick")==tick and not f["busy"])
            key(name,False)
            key("Shift_L",False)
            self.assertEqual(current["history"][-1]["content"]["event"]["direction"],direction)
        watched = self.request(observer,{"type":"snapshot"})
        self.assertEqual(watched["state"]["observation"]["tick"],200)
        self.assertFalse(any(h["content"].get("event",{}).get("type")=="waited" for h in watched["history"]))


if __name__ == "__main__":
    unittest.main()
