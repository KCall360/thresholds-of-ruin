"""Backend travel through actual server, text observer, headless and native ASCII."""
import json
import os
import subprocess
import shutil
from pathlib import Path
import unittest

import test_ascii_process as ascii_support
import test_headless_process as headless_support
import test_text_process as support


class TravelProcesses(unittest.TestCase):
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

    def key(self, client, key):
        client.child.stdin.write(json.dumps({"type": "key", "key": key}) + "\n")
        client.child.stdin.flush()
        return self.ascii_frame(client, lambda f: f.get("input_done") == key and not f["busy"])

    def wizard(self):
        wizard = self.launch("tor-client-text", ["--connect", self.address], token=headless_support.WIZARD_TOKEN)
        wizard.until(lambda line: line == "Ready.")
        return wizard

    def terminal(self, client):
        return self.frame(client, lambda f: f.get("travel") and f["travel"]["phase"] != "active")

    def test_ascii_selection_click_cancellation_and_resume(self):
        server = self.server(wizard=True)
        wizard = self.wizard()
        fixture = json.loads((Path(__file__).parent / "scenarios/travel.json").read_text())
        for command in fixture["setup"]:
            self.assertNotIn("Server error", wizard.command(command))
        wizard.command("release")
        spectator, _ = self.client(support.SPECTATOR_TOKEN)
        ascii_client = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"])
        initial = self.ascii_frame(ascii_client, lambda f: f["state"] is not None and not f["busy"])
        self.assertTrue(initial["has_control"])
        self.key(ascii_client, "travel")
        for _ in range(5):
            selected = self.key(ascii_client, "right")
        self.assertEqual(selected["state"]["observation"]["tick"], 0)
        self.assertEqual(selected["travel_cursor"], {"x": 5, "y": 0, "z": 0})
        self.key(ascii_client, "enter")
        arrived = self.ascii_frame(ascii_client, lambda f: (f.get("travel") or {}).get("phase") == "arrived")
        self.assertEqual(arrived["travel"]["completed_steps"], 5)
        self.assertEqual(arrived["state"]["observation"]["tick"], 500)
        watched = self.terminal(spectator)
        self.assertEqual(watched["state"], arrived["state"])
        self.assertEqual(watched["travel"], arrived["travel"])
        # Eight cells centered in the map: click the first cell (world corridor x=0).
        ascii_client.child.stdin.write(json.dumps({"type": "click", "x": 214, "y": 208}) + "\n")
        ascii_client.child.stdin.flush()
        returned = self.ascii_frame(ascii_client, lambda f: f.get("travel") and f["travel"]["id"] != arrived["travel"]["id"] and f["travel"]["phase"] == "arrived")
        self.assertEqual(returned["state"]["observation"]["tick"], 1000)
        self.key(ascii_client, "travel")
        for _ in range(7): self.key(ascii_client, "right")
        self.key(ascii_client, "enter")
        self.key(ascii_client, "escape")
        # The cancellation acknowledgement frame already contains final travel status.
        synced = self.request(spectator, {"type": "snapshot"})
        self.assertEqual(synced["travel"]["phase"], "cancelled")
        self.assertLess(synced["travel"]["completed_steps"], 7)
        self.assertIn("Travel requested", wizard.command("history"))
        self.assertIn("Your surroundings", wizard.command("look"))
        self.assertNotIn("region", json.dumps(synced))
        before = self.save.read_bytes()
        denied = self.request(spectator, {"type":"cancel_travel", "branch":synced["branch"], "travel_id":synced["travel"]["id"]})
        self.assertIsNotNone(denied["error"])
        self.assertEqual(self.save.read_bytes(), before)
        for client in (ascii_client, wizard, spectator): client.stop()
        server.stop()
        self.server(wizard=True)
        observer, resumed = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"], synced["state"])
        self.assertIsNone(resumed["travel"])
        wizard = self.wizard()
        wizard.command("wizard rewind initial")
        rewound = self.request(observer, {"type":"snapshot"})
        self.assertEqual(rewound["state"]["observation"]["tick"], 0)
        self.assertIsNone(rewound["travel"])
        self.assertNotEqual(rewound["branch"], resumed["branch"])

    def test_normal_game_backend_command_and_wizard_discovery(self):
        self.server()
        player, initial = self.client()
        destination = next(c["key"] for c in initial["state"]["observation"]["visible_cells"] if c["position"] == {"x":-1,"y":0,"z":0})
        accepted = self.request(player, {"type":"command", "branch":initial["branch"], "command":{"type":"travel", "expected_revision":initial["state"]["revision"], "destination":destination}})
        self.assertIsNone(accepted["error"])
        arrived = self.terminal(player)
        self.assertEqual(arrived["travel"]["phase"], "arrived")
        self.assertEqual(arrived["state"]["observation"]["tick"], 100)
        self.assertFalse(arrived["state"]["wizard_game"])

    def test_native_underscore_and_mouse_click(self):
        self.server(wizard=True)
        wizard = self.wizard()
        fixture = json.loads((Path(__file__).parent / "scenarios/travel.json").read_text())
        for command in fixture["setup"]: wizard.command(command)
        wizard.command("release")
        capture = Path(os.environ.get("TOR_TRAVEL_CAPTURE", str(self.save.parent / "travel.ppm")))
        client = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames", "--capture", capture])
        self.ascii_frame(client, lambda f: f["state"] is not None and not f["busy"])
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
            def find_window(hwnd, _):
                pid = wintypes.DWORD()
                user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
                if pid.value == client.child.pid and user32.IsWindowVisible(hwnd): handles.append(hwnd)
                return True
            user32.EnumWindows(find_window, 0)
            self.assertEqual(len(handles), 1)
            def underscore():
                self.assertTrue(user32.PostMessageW(handles[0], 0x102, ord('_'), 1))
            def key(key, down):
                vk = {"Right":0x27, "Return":0x0D}[key]
                scan = user32.MapVirtualKeyW(vk, 0)
                self.assertTrue(user32.PostMessageW(handles[0], 0x100 if down else 0x101, vk, 1 | (scan << 16) | (0x01000000 if key == "Right" else 0) | (0 if down else 0xC0000000)))
            user32.ClientToScreen.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.POINT)]
            user32.GetClientRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
            original = wintypes.POINT()
            user32.GetCursorPos(ctypes.byref(original))
            self.addCleanup(lambda: user32.SetCursorPos(original.x, original.y))
            self.addCleanup(lambda: user32.mouse_event(0x0004, 0, 0, 0, 0))
            user32.SetForegroundWindow.argtypes = [wintypes.HWND]
            def mouse(down):
                rect = wintypes.RECT()
                user32.GetClientRect(handles[0], ctypes.byref(rect))
                scale = min(rect.right / 1200, rect.bottom / 800)
                point = wintypes.POINT(int((rect.right - 1200 * scale) / 2 + 214 * scale), int((rect.bottom - 800 * scale) / 2 + 208 * scale))
                user32.ClientToScreen(handles[0], ctypes.byref(point))
                self.assertTrue(user32.SetCursorPos(point.x, point.y))
                user32.SetForegroundWindow(handles[0])
                # Use real button state: synthetic WM_LBUTTONDOWN can be undone
                # by a queued native WM_MOUSEMOVE reporting no held button.
                user32.mouse_event(0x0002 if down else 0x0004, 0, 0, 0, 0)
        else:
            windows = subprocess.check_output(["xdotool", "search", "--onlyvisible", "--name", r"^Thresholds of Ruin \| ASCII$"], text=True, timeout=10).split()
            self.assertEqual(len(windows), 1)
            def underscore():
                subprocess.run(["xdotool", "key", "--window", windows[0], "underscore"], check=True, timeout=10)
            def key(key, down):
                subprocess.run(["xdotool", "keydown" if down else "keyup", "--window", windows[0], key], check=True, timeout=10)
            def mouse(down):
                subprocess.run(["xdotool", "mousemove", "--window", windows[0], "214", "208", "mousedown" if down else "mouseup", "1"], check=True, timeout=10)
        underscore()
        selected = self.ascii_frame(client, lambda f: f.get("travel_cursor") is not None)
        self.assertEqual(selected["state"]["observation"]["tick"], 0)
        shutil.copyfile(capture, capture.with_name(capture.stem + "-selection.ppm"))
        key("Right", True)
        self.ascii_frame(client, lambda f: f.get("travel_cursor") == {"x":1,"y":0,"z":0})
        key("Right", False)
        key("Return", True)
        arrived = self.ascii_frame(client, lambda f: f.get("travel") and f["travel"]["phase"] == "arrived")
        key("Return", False)
        self.assertEqual(arrived["state"]["observation"]["tick"],100)
        mouse(True)
        returned = self.ascii_frame(client, lambda f: f.get("travel") and f["travel"]["id"] != arrived["travel"]["id"] and f["travel"]["phase"] == "arrived")
        mouse(False)
        self.assertEqual(returned["state"]["observation"]["tick"],200)
        self.assertTrue(capture.read_bytes().startswith(b"P6\n1200 800\n255\n"))

    def travel_scenario(self, setup_name):
        self.server(wizard=True)
        wizard = self.wizard()
        fixture = json.loads((Path(__file__).parent / "scenarios/travel.json").read_text())
        for command in fixture[setup_name]:
            self.assertNotIn("Server error", wizard.command(command))
        wizard.command("release")
        player, initial = self.client()
        ascii_client = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"], token=support.SPECTATOR_TOKEN)
        self.ascii_frame(ascii_client, lambda f: f["state"] is not None and not f["busy"])
        destination = next(c["key"] for c in initial["state"]["observation"]["visible_cells"] if c["position"] == {"x":7,"y":0,"z":0})
        self.request(player, {"type":"command", "branch":initial["branch"], "command":{"type":"travel", "expected_revision":initial["state"]["revision"], "destination":destination}})
        stopped = self.terminal(player)
        shown = self.ascii_frame(ascii_client, lambda f: f.get("travel") and f["travel"]["phase"] != "active")
        self.assertEqual(shown["travel"], stopped["travel"])
        self.assertEqual(shown["state"], stopped["state"])
        self.assertFalse(shown["has_control"])
        return player, initial, stopped

    def test_harmless_discoveries_do_not_interrupt_travel(self):
        _, initial, arrived = self.travel_scenario("harmless_discovery_setup")
        self.assertFalse(initial["state"]["observation"]["ground_items"])
        self.assertFalse(any(c["place_hint"] for c in initial["state"]["observation"]["visible_cells"]))
        self.assertEqual(arrived["travel"]["phase"], "arrived")
        self.assertEqual(arrived["travel"]["completed_steps"], 7)
        self.assertEqual(arrived["state"]["observation"]["tick"], 700)
        self.assertTrue(arrived["state"]["observation"]["ground_items"])
        self.assertTrue(any(c["place_hint"] for c in arrived["state"]["observation"]["visible_cells"]))
        old_keys = {c["key"] for c in initial["state"]["observation"]["visible_cells"]}
        self.assertTrue(any(c["key"] not in old_keys for c in arrived["state"]["observation"]["visible_cells"]))

    def test_new_other_actor_interrupts_travel_as_potential_hazard(self):
        player, initial, stopped = self.travel_scenario("hazard_setup")
        self.assertFalse(initial["state"]["observation"]["visible_actors"])
        self.assertEqual(stopped["travel"]["phase"], "hazard")
        self.assertEqual(stopped["travel"]["completed_steps"], 1)
        self.assertTrue(stopped["state"]["observation"]["visible_actors"])
        synced = self.request(player, {"type":"snapshot"})
        self.assertEqual(synced["state"], stopped["state"])
        self.assertEqual(synced["travel"], stopped["travel"])


if __name__ == "__main__":
    unittest.main()
