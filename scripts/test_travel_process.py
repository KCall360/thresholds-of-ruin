"""Backend travel through actual server, text observer, headless and native ASCII."""
import json
import os
import subprocess
import shutil
import sqlite3
from pathlib import Path
import time
import unittest

from process_harness import ProcessTestCase, SPECTATOR_TOKEN


class TravelJournalProcesses(ProcessTestCase):
    def terminal(self, client):
        return self.frame(client, lambda frame: frame.get("travel") and frame["travel"]["phase"] != "active")

    def test_backend_steps_have_private_admission_linked_execution_and_durable_history(self):
        server = self.server(scenario="travel")
        player, initial = self.client()
        watcher, _ = self.client(SPECTATOR_TOKEN)
        destination = next(cell["key"] for cell in initial["state"]["observation"]["visible_cells"]
                           if cell["position"] == {"x": 5, "y": 0, "z": 0})
        accepted = self.request(player, {"type": "command", "context": initial["input_context"], "branch": initial["branch"],
            "command": {"type": "travel", "expected_revision": initial["state"]["revision"],
                        "destination": destination}})
        self.assertIsNone(accepted["error"])
        self.assertEqual(accepted["state"]["observation"]["tick"], '0')
        self.assertEqual(accepted["travel"]["completed_steps"], "0")
        self.assertEqual(accepted["intentions"], [])
        arrived = self.terminal(player)
        self.assertEqual(arrived["travel"]["phase"], "arrived")
        self.assertEqual(arrived["travel"]["completed_steps"], "5")
        self.assertEqual(arrived["state"]["observation"]["tick"], '500')
        observed = self.terminal(watcher)
        self.assertEqual(observed["state"], arrived["state"])
        self.assertEqual(observed["travel"], arrived["travel"])
        boundary = self.request(player, {"type": "snapshot"})
        actions = [entry for entry in boundary["history"] if entry["content"]["type"] == "action"]
        self.assertEqual(len(actions), 5)
        self.assertTrue(all(entry["author"] == {"type": "backend", "component": "scheduler"}
                            for entry in actions))
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            rows = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 "
                "UNION ALL SELECT sequence,frame FROM history ORDER BY sequence").fetchall()
        records = [json.loads(frame[24:])["record"] for _, frame in rows]
        admissions = [record for record in records
                      if record["entry"]["content"]["type"] == "travel_intention_admitted"]
        self.assertEqual(len(admissions), 5)
        self.assertEqual([record["entry"]["content"]["step"] for record in admissions], list(range(1, 6)))
        visible_ids = {entry["id"] for entry in boundary["history"]}
        for admission in admissions:
            entry = admission["entry"]
            self.assertIsNone(admission["receipt"])
            self.assertEqual(entry["actor"], 1)
            self.assertEqual(entry["audience"], "private")
            self.assertEqual(entry["author"], {"type": "backend", "component": "scheduler"})
            self.assertEqual(entry["content"]["journey"], arrived["travel"]["id"])
            self.assertNotIn(entry["id"], visible_ids)
            executions = [record for record in records
                          if record["entry"]["content"].get("admission") == entry["id"]]
            self.assertEqual(len(executions), 1)
            execution = executions[0]
            self.assertIsNone(execution["receipt"])
            self.assertEqual(execution["entry"]["content"]["type"], "intention_started")
            self.assertEqual(execution["entry"]["content"]["intention"], entry["content"]["intention"])
            self.assertEqual(execution["entry"]["content"]["action"], entry["content"]["action"])
            self.assertIn(execution["entry"]["id"], visible_ids)
        self.assertNotIn('"region"', json.dumps(boundary))
        server.stop(); player.stop(); watcher.stop()
        self.server(scenario="travel")
        _, recovered = self.client(SPECTATOR_TOKEN)
        self.assertEqual(recovered["state"], boundary["state"])
        self.assertEqual(recovered["history"], boundary["history"])
        self.assertEqual(recovered["intentions"], [])
        self.assertIsNone(recovered["travel"])


class TravelProcesses(ProcessTestCase):
    graphical = True

    def terminal(self, client):
        return self.frame(client, lambda f: f.get("travel") and f["travel"]["phase"] != "active")

    def test_ascii_selection_click_skip_and_resume(self):
        server = self.server(wizard=True, scenario="travel")
        spectator, _ = self.client(SPECTATOR_TOKEN)
        # A slow pace keeps the third journey on screen long enough to skip.
        ascii_client = self.launch("tor-client-ascii", ["--connect", self.address, "--automation", "--pace", "500"])
        initial = self.ascii_frame(ascii_client, lambda f: f["state"] is not None and not f["busy"])
        self.assertTrue(initial["has_control"])
        self.key(ascii_client, "travel")
        for _ in range(5):
            selected = self.key(ascii_client, "right")
        self.assertEqual(selected["state"]["observation"]["tick"], '0')
        self.assertEqual(selected["travel_cursor"], {"x": 5, "y": 0, "z": 0})
        self.key(ascii_client, "enter")
        arrived = self.ascii_frame(ascii_client, lambda f: (f.get("travel") or {}).get("phase") == "arrived")
        self.assertEqual(arrived["travel"]["completed_steps"], "5")
        self.assertEqual(arrived["state"]["observation"]["tick"], '500')
        watched = self.terminal(spectator)
        self.assertEqual(watched["state"], arrived["state"])
        self.assertEqual(watched["travel"], arrived["travel"])
        # Eight cells centered in the map: click the first cell (world corridor x=0).
        ascii_client.child.stdin.write(json.dumps({"type": "click", "x": 214, "y": 208}) + "\n")
        ascii_client.child.stdin.flush()
        returned = self.ascii_frame(ascii_client, lambda f: f.get("travel") and f["travel"]["id"] != arrived["travel"]["id"] and f["travel"]["phase"] == "arrived")
        self.assertEqual(returned["state"]["observation"]["tick"], '1000')
        self.key(ascii_client, "travel")
        for _ in range(7): self.key(ascii_client, "right")
        self.key(ascii_client, "enter")
        # Only the server ends a journey: Escape shows the rest of it at once.
        skipped = self.key(ascii_client, "escape")
        self.assertEqual(skipped["status"], "Skipping ahead.")
        finished = self.ascii_frame(ascii_client, lambda f: f.get("travel") and f["travel"]["id"] != returned["travel"]["id"] and f["travel"]["phase"] != "active")
        self.assertEqual(finished["travel"]["phase"], "arrived")
        self.assertEqual(finished["travel"]["completed_steps"], "7")
        synced = self.request(spectator, {"type": "snapshot"})
        self.assertEqual(synced["travel"], finished["travel"])
        self.assertEqual(synced["state"]["observation"]["tick"], '1700')
        self.assertIn("travel", [entry["content"]["type"] for entry in synced["history"]])
        self.assertNotIn("region", json.dumps(synced))
        self.flush_save()
        before = self.save.read_bytes()
        destination = synced["state"]["observation"]["visible_cells"][0]["key"]
        denied = self.request(spectator, {"type":"command", "context": synced["input_context"], "branch":synced["branch"], "command":{"type":"travel", "expected_revision":synced["state"]["revision"], "destination":destination}})
        self.assertIsNotNone(denied["error"])
        self.assertEqual(self.save.read_bytes(), before)
        for client in (ascii_client, spectator): client.stop()
        server.stop()
        self.server(wizard=True)
        observer, resumed = self.client(SPECTATOR_TOKEN)
        self.assertEqual(resumed["state"], synced["state"])
        self.assertIsNone(resumed["travel"])
        wizard = self.wizard()
        self.wizard_command(wizard, "rewind initial")
        rewound = self.request(observer, {"type":"snapshot"})
        self.assertEqual(rewound["state"]["observation"]["tick"], '0')
        self.assertIsNone(rewound["travel"])
        self.assertNotEqual(rewound["branch"], resumed["branch"])

    def test_normal_game_backend_command_and_wizard_discovery(self):
        self.server()
        player, initial = self.client()
        destination = next(c["key"] for c in initial["state"]["observation"]["visible_cells"] if c["position"] == {"x":-1,"y":0,"z":0})
        accepted = self.request(player, {"type":"command", "context": initial["input_context"], "branch":initial["branch"], "command":{"type":"travel", "expected_revision":initial["state"]["revision"], "destination":destination}})
        self.assertIsNone(accepted["error"])
        arrived = self.terminal(player)
        self.assertEqual(arrived["travel"]["phase"], "arrived")
        self.assertEqual(arrived["state"]["observation"]["tick"], '100')
        self.assertFalse(arrived["state"]["wizard_game"])

    def test_native_underscore_and_mouse_click(self):
        self.server(scenario="travel")
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
            user32.SetWindowPos.argtypes = [wintypes.HWND, wintypes.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, wintypes.UINT]
            user32.WindowFromPoint.argtypes = [wintypes.POINT]
            user32.WindowFromPoint.restype = wintypes.HWND
            user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
            def describe(hwnd):
                name = ctypes.create_unicode_buffer(256)
                user32.GetClassNameW(hwnd, name, 256)
                pid = wintypes.DWORD()
                user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
                return f"window {hwnd} (class {name.value!r}, process {pid.value})"
            # Hosted desktops can be smaller than the default game window.
            # Keep the test window and click target on screen and unobscured.
            self.assertTrue(user32.SetWindowPos(handles[0], wintypes.HWND(-1), 0, 0,
                min(1000, user32.GetSystemMetrics(0)), min(700, user32.GetSystemMetrics(1)), 0x0040))
            def mouse(down):
                rect = wintypes.RECT()
                user32.GetClientRect(handles[0], ctypes.byref(rect))
                scale = min(rect.right / 1200, rect.bottom / 800)
                point = wintypes.POINT(int((rect.right - 1200 * scale) / 2 + 214 * scale), int((rect.bottom - 800 * scale) / 2 + 208 * scale))
                user32.ClientToScreen(handles[0], ctypes.byref(point))
                self.assertTrue(user32.SetCursorPos(point.x, point.y))
                actual = wintypes.POINT()
                self.assertTrue(user32.GetCursorPos(ctypes.byref(actual)))
                self.assertEqual((actual.x, actual.y), (point.x, point.y))
                # The window manager can take a moment to apply the resize and
                # z-order, so re-assert them briefly before requiring the hit.
                for _ in range(20):
                    user32.SetForegroundWindow(handles[0])
                    hit = user32.WindowFromPoint(actual)
                    if hit == handles[0]:
                        break
                    user32.SetWindowPos(handles[0], wintypes.HWND(-1), 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0040)
                    time.sleep(0.05)
                self.assertEqual(hit, handles[0], f"Native click must hit the game window, not {describe(hit)}")
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
        self.assertEqual(selected["state"]["observation"]["tick"], '0')
        shutil.copyfile(capture, capture.with_name(capture.stem + "-selection.ppm"))
        key("Right", True)
        self.ascii_frame(client, lambda f: f.get("travel_cursor") == {"x":1,"y":0,"z":0})
        key("Right", False)
        key("Return", True)
        arrived = self.ascii_frame(client, lambda f: f.get("travel") and f["travel"]["phase"] == "arrived")
        key("Return", False)
        self.assertEqual(arrived["state"]["observation"]["tick"],'100')
        mouse(True)
        returned = self.ascii_frame(client, lambda f: f.get("travel") and f["travel"]["id"] != arrived["travel"]["id"] and f["travel"]["phase"] == "arrived")
        mouse(False)
        self.assertEqual(returned["state"]["observation"]["tick"],'200')
        self.assertTrue(capture.read_bytes().startswith(b"P6\n1200 800\n255\n"))

    def travel_scenario(self, package):
        self.server(scenario=package)
        player, initial = self.client()
        ascii_client = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"], token=SPECTATOR_TOKEN)
        self.ascii_frame(ascii_client, lambda f: f["state"] is not None and not f["busy"])
        destination = next(c["key"] for c in initial["state"]["observation"]["visible_cells"] if c["position"] == {"x":7,"y":0,"z":0})
        self.request(player, {"type":"command", "context": initial["input_context"], "branch":initial["branch"], "command":{"type":"travel", "expected_revision":initial["state"]["revision"], "destination":destination}})
        stopped = self.terminal(player)
        shown = self.ascii_frame(ascii_client, lambda f: f.get("travel") and f["travel"]["phase"] != "active")
        self.assertEqual(shown["travel"], stopped["travel"])
        self.assertEqual(shown["state"], stopped["state"])
        self.assertFalse(shown["has_control"])
        return player, initial, stopped

    def test_harmless_discoveries_do_not_interrupt_travel(self):
        _, initial, arrived = self.travel_scenario("travel-harmless-discovery")
        self.assertFalse(initial["state"]["observation"]["ground_items"])
        self.assertFalse(any(c["place_hint"] for c in initial["state"]["observation"]["visible_cells"]))
        self.assertEqual(arrived["travel"]["phase"], "arrived")
        self.assertEqual(arrived["travel"]["completed_steps"], "7")
        self.assertEqual(arrived["state"]["observation"]["tick"], '700')
        self.assertTrue(arrived["state"]["observation"]["ground_items"])
        self.assertTrue(any(c["place_hint"] for c in arrived["state"]["observation"]["visible_cells"]))
        old_keys = {c["key"] for c in initial["state"]["observation"]["visible_cells"]}
        self.assertTrue(any(c["key"] not in old_keys for c in arrived["state"]["observation"]["visible_cells"]))

    def test_new_other_actor_interrupts_travel_as_potential_hazard(self):
        player, initial, stopped = self.travel_scenario("travel-hazard")
        self.assertFalse(initial["state"]["observation"]["visible_actors"])
        self.assertEqual(stopped["travel"]["phase"], "hazard")
        self.assertEqual(stopped["travel"]["completed_steps"], "1")
        self.assertTrue(stopped["state"]["observation"]["visible_actors"])
        synced = self.request(player, {"type":"snapshot"})
        self.assertEqual(synced["state"], stopped["state"])
        self.assertEqual(synced["travel"], stopped["travel"])


if __name__ == "__main__":
    unittest.main()
