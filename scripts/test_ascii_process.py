"""Real native-window tests. Linux requires DISPLAY (use xvfb-run).

Automation injects UI input events, not OS keystrokes. Frames are reported only
after the real native window successfully presents its pixel buffer.
"""
import json
import os
import subprocess
import unittest

import test_text_process as text_support
from test_text_process import TOKEN, SPECTATOR_TOKEN


class AsciiProcesses(unittest.TestCase):
    setUp = text_support.TextProcesses.setUp
    launch = text_support.TextProcesses.launch
    start_server = text_support.TextProcesses.start_server
    client = text_support.TextProcesses.client

    @classmethod
    def setUpClass(cls):
        if os.name != "nt" and not os.environ.get("DISPLAY"):
            raise RuntimeError("Graphical process tests require DISPLAY; run under xvfb-run on Linux")
        text_support.TextProcesses.setUpClass.__func__(cls)

    def ascii(self, observe=True):
        process = self.launch("tor-client-ascii", ["--connect", self.address, "--automation", *(["--observe"] if observe else [])])
        frame = self.frame(process, lambda f: f["state"] is not None and not f["busy"])
        self.assertGreater(frame["frame"], 0)
        self.assertTrue(frame["window_open"])
        self.assertNotIn(TOKEN, str(frame))
        return process, frame

    def frame(self, process, predicate):
        found = []
        def match(line):
            if not line.startswith('{'): return False
            value = json.loads(line)
            if value.get("type") == "frame" and predicate(value):
                found.append(value)
                return True
            return False
        process.until(match, seconds=20)
        return found[-1]

    def key(self, process, key):
        process.child.stdin.write(json.dumps({"type":"key", "key":key}) + "\n")
        process.child.stdin.flush()
        return self.frame(process, lambda f: f.get("input_done") == key and not f["busy"])

    def test_read_only_spectator_window_streams_actions_and_resumes(self):
        self.server.stop()
        self.server, self.address = self.start_server(spectator=True)
        player, _ = self.client()
        spectator = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"], token=SPECTATOR_TOKEN)
        initial = self.frame(spectator, lambda f: f["state"] is not None and not f["busy"])
        self.assertEqual(initial["role"], "spectator")
        self.assertFalse(initial["has_control"])
        self.assertIn("read-only", initial["status"])
        self.assertIn("stone tablet", str(initial))
        actions = ["take token", "wait", "east", "east", "east", "east", "east"]
        for revision, action in enumerate(actions, 1):
            player.command(action)
            watched = self.frame(spectator, lambda f: f["state"]["revision"] == revision)
            entries = [e for e in watched["history"] if e["content"]["type"] == "action"]
            self.assertEqual(len(entries), revision)
            self.assertIn("event", entries[-1]["content"])
            self.assertFalse(watched["has_control"])
        self.assertEqual(next(i["position"] for i in watched["state"]["observation"]["ground_items"] if i["item"]["name"] == "stone tablet"), {"x":2,"y":0,"z":0})
        player.command("note Secret")
        player.command("annotate user actor note here Public progress")
        shared = self.frame(spectator, lambda f: "Public progress" in str(f["history"]))
        self.assertNotIn("Secret", str(shared))
        before = self.save.read_bytes()
        for key in ["control", "release", "right", "pickup", "wait", "note"]:
            denied = self.key(spectator, key)
            self.assertIn("read-only", denied["status"])
            self.assertFalse(denied["has_control"])
            self.assertIsNone(denied["note"])
            self.assertEqual(denied["state"], watched["state"])
        history = self.key(spectator, "history")
        self.assertIn("Public progress", str(history))
        self.assertNotIn("Secret", str(history))
        self.key(spectator, "escape")
        self.key(spectator, "escape")
        self.assertEqual(spectator.child.wait(timeout=10), 0)
        self.assertEqual(self.save.read_bytes(), before)
        player.stop()
        self.server.stop()
        self.server, self.address = self.start_server(spectator=True)
        resumed = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"], token=SPECTATOR_TOKEN)
        restored = self.frame(resumed, lambda f: f["state"] is not None and not f["busy"])
        self.assertEqual(restored["role"], "spectator")
        self.assertEqual(restored["state"], watched["state"])
        self.assertEqual(restored["history"], shared["history"])
        self.assertIn("read-only", self.key(resumed, "control")["status"])

    def test_text_to_window_control_transfer_and_save_resume(self):
        text, _ = self.client()
        ascii_client, initial = self.ascii()
        self.assertIn("stone tablet", str(initial))
        self.assertFalse(initial["has_control"])
        text.command("take token")
        self.frame(ascii_client, lambda f: f["state"]["revision"] == 1)
        text.command("note Return through the entry.")
        self.frame(ascii_client, lambda f: "Return through the entry." in str(f["history"]))
        denied = self.key(ascii_client, "control")
        self.assertIn("ControlTaken", denied["status"])
        text.command("release")
        self.assertTrue(self.key(ascii_client, "control")["has_control"])
        for _ in range(5):
            moved = self.key(ascii_client, "right")
        self.assertEqual(next(i["position"] for i in moved["state"]["observation"]["ground_items"] if i["item"]["name"] == "stone tablet"), {"x":2,"y":0,"z":0})
        self.assertEqual(moved["state"]["observation"]["tick"], 550)
        self.assertIn("stone tablet", str(moved))
        self.assertIn("token", str(moved["state"]["observation"]["inventory"]))
        # Sync is a protocol barrier after all pushes to the observing text client.
        self.assertIn("Your surroundings", text.command("sync"))
        before = moved["state"]
        self.key(ascii_client, "release")
        self.assertIn("Control: yours", text.command("control"))
        self.key(ascii_client, "escape")
        self.assertEqual(ascii_client.child.wait(timeout=10), 0)
        text.stop()
        self.server.stop()
        self.server, self.address = self.start_server()
        resumed_text, welcome = self.client(observe=True)
        resumed_ascii, restored = self.ascii()
        self.assertEqual(restored["state"], before)
        self.assertEqual(restored["history"], moved["history"])
        self.assertEqual(restored["branch"], moved["branch"])
        self.assertIn("Your surroundings", welcome)
        self.assertIn("tick 550.", welcome)
        self.assertIn("token", resumed_text.command("inventory"))
        self.assertIn("Return through the entry.", resumed_text.command("history"))
        self.assertTrue(self.key(resumed_ascii, "control")["has_control"])

    def test_graphical_pickup_notes_invalid_move_and_disconnect(self):
        client, _ = self.ascii(observe=False)
        taken = self.key(client, "pickup")
        self.assertEqual(taken["state"]["revision"], 1)
        self.assertIn("token", str(taken["state"]["observation"]["inventory"]))
        self.key(client, "note")
        client.child.stdin.write(json.dumps({"type":"text", "text":"A graphical note"}) + "\n")
        client.child.stdin.flush()
        noted = self.key(client, "enter")
        self.assertIn("A graphical note", str(noted["history"]))
        self.assertEqual(noted["state"]["revision"], 1)
        invalid = self.key(client, "ascend")
        self.assertIn("InvalidAction", invalid["status"])
        self.assertEqual(invalid["state"], noted["state"])
        self.server.stop()
        self.frame(client, lambda f: not f["connected"])
        self.assertNotEqual(client.child.wait(timeout=15), 0)

    def test_bad_authentication_discloses_no_state(self):
        result = subprocess.run([str(self.bin / ("tor-client-ascii" + self.suffix)), "--connect", self.address, "--automation"], env={**os.environ, "TOR_SERVER_TOKEN":"wrong-test-token"}, capture_output=True, text=True, timeout=20)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("Entry chamber", result.stdout)
        self.assertNotIn("wrong-test-token", result.stdout + result.stderr)

    def test_native_keyboard_pickup_and_close(self):
        # This path uses real OS key messages, not the JSON automation driver.
        capture = self.save.parent / "native-frame.ppm"
        client = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames", "--capture", capture])
        self.frame(client, lambda f: f["has_control"] and not f["busy"])
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
                if pid.value == client.child.pid and user32.IsWindowVisible(hwnd):
                    handles.append(hwnd)
                return True
            user32.EnumWindows(find_window, 0)
            self.assertTrue(handles, "Actual native window must exist")
            def key_event(key, down):
                vk = {"g":0x47, "Escape":0x1B}[key]
                scan = user32.MapVirtualKeyW(vk, 0)
                lparam = 1 | (scan << 16) | (0 if down else 0xC0000000)
                self.assertTrue(user32.PostMessageW(handles[0], 0x100 if down else 0x101, vk, lparam))
        else:
            windows = subprocess.check_output(["xdotool", "search", "--onlyvisible", "--name", r"^Thresholds of Ruin \| ASCII$"], text=True, timeout=10).split()
            self.assertEqual(len(windows), 1)
            def key_event(key, down):
                subprocess.run(["xdotool", "keydown" if down else "keyup", "--window", windows[0], key], check=True, timeout=10)
        key_event("g", True)
        picked = self.frame(client, lambda f: f["state"]["revision"] == 1 and not f["busy"])
        key_event("g", False)
        self.assertIn("token", str(picked["state"]["observation"]["inventory"]))
        key_event("Escape", True)
        self.assertEqual(client.child.wait(timeout=15), 0)
        data = capture.read_bytes()
        self.assertTrue(data.startswith(b"P6\n1200 800\n255\n"))
        pixels = data.split(b"\n", 3)[3]
        self.assertEqual(len(pixels), 1200 * 800 * 3)
        self.assertGreater(len(set(pixels)), 8)


if __name__ == "__main__":
    unittest.main()
