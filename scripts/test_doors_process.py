"""Door actions through the actual text, headless and native ASCII clients."""
import json
import os
from pathlib import Path
import subprocess
import unittest

import test_text_process as support
import test_headless_process as headless
import test_ascii_process as ascii_support
import test_adventure_process as adventure_support


class DoorProcesses(unittest.TestCase):
    launch = support.TextProcesses.launch
    setUp = headless.HeadlessProcesses.setUp
    server = headless.HeadlessProcesses.server
    client = headless.HeadlessProcesses.client
    frame = headless.HeadlessProcesses.frame
    request = headless.HeadlessProcesses.request
    command = headless.HeadlessProcesses.command
    act = headless.HeadlessProcesses.act
    adventure = adventure_support.AdventureProcesses.adventure
    say = adventure_support.AdventureProcesses.say
    send = adventure_support.AdventureProcesses.send
    ascii_frame = ascii_support.AsciiProcesses.frame
    key = ascii_support.AsciiProcesses.key
    # The reused ASCII key helper calls self.frame; our headless frame also accepts JSON frames.
    setUpClass = classmethod(ascii_support.AsciiProcesses.setUpClass.__func__)

    @staticmethod
    def door(frame):
        return next(c["door"] for c in frame["state"]["observation"]["visible_cells"] if c.get("door"))

    def setup_wizard(self, section="setup"):
        wizard = self.launch("tor-client-text", ["--connect", self.address], token=headless.WIZARD_TOKEN)
        wizard.until(lambda line: line == "Ready.")
        fixture = json.loads((Path(__file__).parent / "scenarios/doors.json").read_text())
        for command in fixture[section]:
            self.assertNotIn("Server error", wizard.command(command))
        wizard.command("release")
        return wizard

    def test_normal_text_intention_native_ascii_actions_spectators_and_restart(self):
        server = self.server()
        player, welcome = self.adventure()
        self.assertIn("open wooden door", welcome)
        self.assertIn("iron handle", self.say(player, "examine door"))
        self.assertEqual(self.say(player, "close it"), "You walk over to the wooden door and close it.\n> ")
        observer, closed = self.client(support.SPECTATOR_TOKEN)
        self.assertFalse(self.door(closed)["open"])
        self.assertEqual(closed["state"]["observation"]["tick"], 400)
        before = self.save.read_bytes()
        denied = self.act(observer, {"type":"set_door", "door":self.door(closed)["id"], "open":True})
        self.assertIsNotNone(denied["error"])
        self.assertEqual(self.save.read_bytes(), before)
        self.say(player, "release")
        capture = Path(os.environ.get("TOR_DOOR_CAPTURE", str(self.save.parent / "doors.ppm")))
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames", "--capture", capture])
        initial = self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"])
        self.assertTrue(initial["has_control"])
        doorway = next(c for c in initial["state"]["observation"]["visible_cells"] if c.get("door"))
        self.assertFalse(doorway["place_hint"])
        self.assertFalse(any(i["item"]["name"] == "stone tablet" for i in initial["state"]["observation"]["ground_items"]))
        key = self.native_keys(window)
        key("o", True)
        prompt = self.ascii_frame(window, lambda f: f.get("door_direction") is True)
        self.assertEqual(prompt["state"], initial["state"])
        key("o", False)
        key("Up", True)
        missing = self.ascii_frame(window, lambda f: f["status"] == "There is no door in that direction.")
        key("Up", False)
        self.assertEqual(missing["state"], initial["state"])
        self.assertEqual(self.save.read_bytes(), before)
        key("o", True)
        self.ascii_frame(window, lambda f: f.get("door_direction") is True)
        key("o", False)
        key("Right", True)
        opened = self.ascii_frame(window, lambda f: f["state"]["revision"] == closed["state"]["revision"] + 1 and not f["busy"])
        key("Right", False)
        self.assertTrue(self.door(opened)["open"])
        key("c", True)
        self.ascii_frame(window, lambda f: f.get("door_direction") is False)
        key("c", False)
        key("Right", True)
        closed_again = self.ascii_frame(window, lambda f: f["state"]["revision"] == opened["state"]["revision"] + 1 and not f["busy"])
        key("Right", False)
        self.assertFalse(self.door(closed_again)["open"])
        watched = self.request(observer, {"type":"snapshot"})
        self.assertEqual(watched["state"], closed_again["state"])
        self.assertEqual(watched["history"][-1]["content"]["event"]["type"], "door_changed")
        key("Escape", True)
        self.assertEqual(window.child.wait(timeout=15), 0)
        self.assertTrue(capture.read_bytes().startswith(b"P6\n1200 800\n255\n"))
        player.stop(); observer.stop(); server.stop()
        self.server()
        _, resumed = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"], closed_again["state"])
        self.assertFalse(resumed["state"]["wizard_game"])

    def test_wizard_occlusion_explicit_open_travel_memory_and_rewind(self):
        server = self.server(wizard=True)
        wizard = self.setup_wizard()
        observer, initial = self.client(support.SPECTATOR_TOKEN)
        player, welcome = self.adventure()
        self.assertIn("closed wooden door", welcome)
        self.assertNotIn("stone tablet", welcome)
        self.assertFalse(self.door(initial)["open"])
        self.assertTrue(all(key in {c["key"] for c in initial["state"]["observation"]["visible_cells"]} for key in self.door(initial)["approaches"]))
        self.assertEqual(self.say(player, "open door"), "You walk over to the wooden door and open it.\n> ")
        opened = self.request(observer, {"type":"snapshot"})
        self.assertEqual(opened["state"]["observation"]["tick"], 600)
        self.assertIn("stone tablet", self.say(player, "look"))
        tablet_cell = next(c for c in opened["state"]["observation"]["visible_cells"] if c["position"] == {"x":3,"y":0,"z":0})
        self.assertEqual(self.say(player, "close door"), "You close the wooden door.\n> ")
        hidden = self.request(observer, {"type":"snapshot"})
        self.assertNotIn("stone tablet", json.dumps(hidden["state"]))
        self.assertIn("stone tablet", json.dumps(hidden["memory"]))
        self.say(player, "release")
        controller, current = self.client()
        blocked = self.request(controller, {"type":"command", "branch":current["branch"], "command":{"type":"travel", "expected_revision":current["state"]["revision"], "destination":tablet_cell["key"]}})
        self.assertIsNotNone(blocked["error"])
        self.assertEqual(blocked["state"], current["state"])
        controller.stop()
        for client in (player, observer, wizard): client.stop()
        server.stop()
        self.server(wizard=True)
        wizard = self.launch("tor-client-text", ["--connect", self.address], token=headless.WIZARD_TOKEN)
        wizard.until(lambda line: line == "Ready.")
        observer, resumed = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"], hidden["state"])
        self.assertNotIn("Server error", wizard.command("wizard rewind initial"))
        rewound = self.request(observer, {"type":"snapshot"})
        self.assertNotEqual(rewound["branch"], resumed["branch"])
        self.assertTrue(self.door(rewound)["open"])
        self.assertFalse(any(c["key"] == tablet_cell["key"] for c in rewound["memory"]))

    def test_cancel_and_rewind_discard_pending_door_action(self):
        self.server(wizard=True)
        wizard = self.setup_wizard()
        player, _ = self.adventure()
        observer, initial = self.client(support.SPECTATOR_TOKEN)
        self.send(player, "open door")
        self.send(player, "stop")
        output = player.until(lambda line: line == "> ")
        self.assertNotIn("and open it", output)
        stopped = self.request(observer, {"type":"snapshot"})
        self.assertFalse(self.door(stopped)["open"])
        self.assertFalse(any(h["content"].get("action", {}).get("type") == "set_door" for h in stopped["history"]))
        self.send(player, "open door")
        self.frame(observer, lambda f: (f.get("travel") or {}).get("phase") == "active")
        # Sync the wizard before a revision-checked rewind while travel progresses.
        for _ in range(10):
            wizard.command("sync")
            result = wizard.command("wizard rewind initial")
            if "StaleRevision" not in result: break
        self.assertNotIn("Server error", result)
        restored = self.request(observer, {"type":"snapshot"})
        self.assertNotEqual(restored["branch"], initial["branch"])
        self.assertEqual(restored["state"]["observation"]["tick"], 0)
        # A barrier query waits until the text client has handled its fresh snapshot.
        self.assertNotIn("and open it", self.say(player, "look"))
        final = self.request(observer, {"type":"snapshot"})
        self.assertEqual(final["state"]["observation"]["tick"], 0)


    def test_rotated_aperture_door_remains_an_ordinary_visible_object(self):
        self.server(wizard=True)
        self.setup_wizard("rotated")
        player, welcome = self.adventure()
        self.assertIn("closed wooden door to the east", welcome)
        self.assertNotIn("stone tablet", welcome)
        self.assertEqual(self.say(player, "open door"), "You open the wooden door.\n> ")
        self.assertIn("stone tablet", self.say(player, "look"))
        self.say(player, "step east")
        self.say(player, "step east")
        self.assertEqual(self.say(player, "close door"), "You close the wooden door.\n> ")
        observer, view = self.client(support.SPECTATOR_TOKEN)
        self.assertFalse(self.door(view)["open"])
        door_cell = next(c for c in view["state"]["observation"]["visible_cells"] if c.get("door"))
        self.assertEqual(door_cell["position"], {"x":-1,"y":0,"z":0})
        for forbidden in ("Private", "region", "portal", "quarter_turns"):
            self.assertNotIn(forbidden, json.dumps(view))

    def test_arrival_revealing_an_actor_does_not_open_the_door(self):
        self.server(wizard=True)
        # The south approach is walled off: the first step must reveal the actor
        # to the east, independent of randomly salted opaque-key tie breaking.
        self.setup_wizard("arrival_hazard")
        player, _ = self.adventure()
        output = self.say(player, "open door")
        self.assertIn("figure comes into view", output)
        self.assertNotIn("and open it", output)
        _, state = self.client(support.SPECTATOR_TOKEN)
        self.assertFalse(self.door(state)["open"])
        self.assertEqual(state["travel"]["phase"], "arrived")
        self.assertEqual(state["travel"]["completed_steps"], 1)
        self.assertFalse(any(h["content"].get("action", {}).get("type") == "set_door" for h in state["history"]))

    def native_keys(self, client):
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes
            user32 = ctypes.WinDLL("user32", use_last_error=True)
            callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
            user32.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
            user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
            user32.IsWindowVisible.argtypes = [wintypes.HWND]
            user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
            handles = []
            @callback_type
            def find(hwnd, _):
                pid = wintypes.DWORD(); user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
                if pid.value == client.child.pid and user32.IsWindowVisible(hwnd): handles.append(hwnd)
                return True
            user32.EnumWindows(find, 0)
            self.assertEqual(len(handles), 1)
            def key(name, down):
                vk = {"o":0x4F,"c":0x43,"Right":0x27,"Up":0x26,"Escape":0x1B}[name]
                scan = user32.MapVirtualKeyW(vk, 0)
                self.assertTrue(user32.PostMessageW(handles[0], 0x100 if down else 0x101, vk, 1 | (scan << 16) | (0x01000000 if name in ("Up", "Right") else 0) | (0 if down else 0xC0000000)))
            return key
        windows = subprocess.check_output(["xdotool", "search", "--onlyvisible", "--name", r"^Thresholds of Ruin \| ASCII$"], text=True, timeout=10).split()
        self.assertEqual(len(windows), 1)
        def key(name, down):
            subprocess.run(["xdotool", "keydown" if down else "keyup", "--window", windows[0], name], check=True, timeout=10)
        return key


if __name__ == "__main__":
    unittest.main()

