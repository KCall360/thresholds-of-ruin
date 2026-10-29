"""Real server + Bevy window + OS input, maintained outside the core gate."""
import hashlib
import json
import math
import os
import re
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import sys
import threading
import time
import tomllib
from types import SimpleNamespace
import unittest

CLIENT = Path(__file__).resolve().parents[1]
ROOT = CLIENT.parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
# Shared harness imports require the repository path established above.
from process_harness import ProcessTestCase, SPECTATOR_TOKEN, creature_package, door, non_graphical  # noqa: E402
from arena_matrix import toml_source  # noqa: E402
from stream_relay import StreamRelay  # noqa: E402
import model_generation  # noqa: E402


class AbilityTrace:
    """Bounded read-only evidence from the real server-to-client stream."""
    def __init__(self):
        self.lock = threading.Lock()
        self.events = []

    def observe(self, message):
        if message.get("type") != "update":
            return
        body = message["update"]["body"]
        if body["type"] not in ("observation", "observation_delta"):
            return
        state = body["state"]
        combat = (state["observation"] if body["type"] == "observation" else state).get("combat")
        if not isinstance(combat, dict):
            return
        with self.lock:
            for event in combat.get("events", []):
                if event.get("type") == "ability":
                    self.events.append(event)
            if len(self.events) > 64:
                raise AssertionError("Native attack trace exceeded its bounded evidence budget")

    def attacks(self, caster, target):
        with self.lock:
            return [event for event in self.events if event["ability"]["id"] == "basic_melee"
                    and event["caster"] == caster and event["target"] == target]


class Native3DProcesses(ProcessTestCase):
    graphical = True

    def assert_live_portrait(self, portrait, initial):
        own_actor = next(actor for actor in portrait["actors"] if actor["is_self"])
        self.assertEqual(portrait["portrait"]["target"], own_actor["target"])
        self.assertEqual(len(portrait["actors"]), len(initial["actors"]), "the portrait reuses the live rig rather than adding an actor")
        self.assertLessEqual(portrait["portrait"]["texture_size"][0], 768)
        self.assertLessEqual(portrait["portrait"]["texture_size"][1], 1024)
        from render_pixels import decode_png
        width, height, channels, rows = decode_png(Path(portrait["png"]).read_bytes())
        left, top, right, bottom = portrait["portrait"]["bounds"]
        self.assertGreater(right - left, 0.1)
        self.assertGreater(bottom - top, 0.2)
        colored = 0
        colored_top, colored_bottom = height, 0
        for y in range(int((top + 0.02) * height), int((bottom - 0.02) * height), 2):
            for x in range(int((left + 0.02) * width), int((right - 0.02) * width), 2):
                rgb = rows[y][x * channels:x * channels + 3]
                if max(rgb) > 55 and max(rgb) - min(rgb) > 10:
                    colored += 1
                    colored_top = min(colored_top, y)
                    colored_bottom = max(colored_bottom, y)
        self.assertGreater(colored, 1000, "the actual offscreen portrait must contain the rendered character, not an empty target")
        with self.subTest(portrait="character_scale"):
            self.assertGreater((colored_bottom - colored_top) / ((bottom - top) * height), 0.52,
                               "the full character must use a substantial part of the portrait's height")
        with self.subTest(portrait="heading_space"):
            self.assertGreater(top * height - left * width, 20,
                               "the equally inset portrait pane must reserve a visible heading line above its image")

    def test_equipment_screen_browses_locally_and_removes_then_equips_the_disclosed_item(self):
        self.server(scenario=self.fallback_package())
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--connect", self.address, "--capture", self.directory / "equipment", "--capture-every", "0.4",
        ], executable=executable)
        initial = self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        native = self.native_keys(window)
        opened = self.tap(window, native, "F6", lambda frame: (frame.get("inventory_browser") or {}).get("page") == "Equipment")
        self.assertTrue(opened["equipment_slots"], "the fixture must disclose actual equipment slots")
        portrait = self.captured(window, lambda frame: (frame.get("portrait") or {}).get("active") and not frame["portrait"]["pending"] and frame["portrait"]["mesh_parts"] > 0)
        self.assert_live_portrait(portrait, initial)
        turn = portrait["portrait"]["turn"]
        rotated = self.tap(window, native, "bracketright", lambda frame: frame["portrait"]["turn"] > turn + 0.5)
        self.assertEqual(rotated["revision"], initial["revision"], "turning the portrait is local")
        self.assertEqual(rotated["world_feet"], initial["world_feet"])
        self.tap(window, native, "bracketleft", lambda frame: abs(frame["portrait"]["turn"] - turn) < 0.01)
        selected = self.tap(window, native, "Right", lambda frame: frame["inventory_browser"]["selected"] is not None)
        target = selected["inventory_browser"]["selected"]
        item = next(item for item in selected["inventory"] if item["id"] == target)
        self.assertIsNotNone(item["equipped"], "positive fixture: first gear item starts equipped")
        slot = next(slot for slot in selected["equipment_slots"] if slot["occupant"] == target)
        self.assertEqual(selected["inventory_browser"]["comparison_slot"], slot["index"])
        slot_controls = [control for control in selected["inventory_browser"]["controls"] if control["control"].startswith("Socket(")]
        self.assertEqual(len(slot_controls), len(selected["equipment_slots"]), "all declared socket controls must be laid out")
        _, height = struct.unpack(">II", Path(selected["png"]).read_bytes()[16:24])
        slot_bottom = max(control["screen"][1] + control["size"][1] / (2 * height) for control in slot_controls)
        self.assertLess(slot_bottom, 0.57, "the slot grid must leave the opening viewport's lower area for item comparisons")
        viewport = selected["panel_viewport"]
        remove_buttons = [button for button in selected["panel_buttons"] if button["target"] == target and button["operation"] == "Unequip"]
        self.assertEqual(len(remove_buttons), 1, "the selected equipped item must have one real Remove button")
        left, top, right, bottom = remove_buttons[0]["bounds"]
        self.assertLess(left, right)
        self.assertLess(top, bottom)
        self.assertGreaterEqual(left, viewport[0] - 0.002)
        self.assertGreaterEqual(top, viewport[1] - 0.002)
        self.assertLessEqual(right, viewport[2] + 0.002)
        self.assertLessEqual(bottom, viewport[3] + 0.002, "Remove must be wholly visible without first scrolling the comparison")
        self.assertEqual(selected["revision"], initial["revision"], "browsing cannot spend a turn")
        self.assertEqual(selected["world_feet"], initial["world_feet"])

        def action_key(frame, label):
            index = next(index for index, (action, identity) in enumerate(zip(frame["panel_actions"], frame["panel_item_targets"]))
                         if identity == target and action.endswith(": " + label))
            self.assertLess(index, 10, "the fixture must keep this action within the numbered hotkeys")
            return str((index + 1) % 10)

        removed = self.tap(window, native, action_key(selected, "Remove"),
                           lambda frame: not frame["busy"] and frame["can_act"] and any(item["id"] == target and item["equipped"] is None for item in frame["inventory"]))
        self.assertGreater(removed["revision"], selected["revision"])
        self.assertIsNone(removed["equipment_slots"][slot["index"]]["occupant"])
        restored = self.tap(window, native, action_key(removed, "Equip"),
                            lambda frame: not frame["busy"] and frame["can_act"] and any(item["id"] == target and item["equipped"] is not None for item in frame["inventory"]))
        self.assertGreater(restored["revision"], removed["revision"])
        self.assertEqual(restored["equipment_slots"][slot["index"]]["occupant"], target)
        inventory = self.tap(window, native, "Tab", lambda frame: frame["inventory_browser"]["page"] == "Inventory")
        self.captured(window, lambda frame: not frame["portrait"]["active"])
        self.assertEqual(inventory["revision"], restored["revision"])
        self.assertTrue(inventory["has_control"], "Tab inside the panel must not transfer actor control")
        self.tap(window, native, "Escape", lambda frame: not frame["panel_open"])
        native("Escape", True)
        self.assertEqual(window.child.wait(timeout=30), 0)

    def test_inventory_reframes_the_player_without_moving_the_actor(self):
        self.server(scenario=self.fallback_package())
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--connect", self.address, "--capture", self.directory / "inventory-framing", "--capture-every", "0.4",
        ], executable=executable)
        initial = self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        original = next(actor for actor in initial["actors"] if actor["is_self"])
        self.assertAlmostEqual(original["screen"][0], 0.5, delta=0.05)
        native = self.native_keys(window)
        opened = self.tap(window, native, "i", lambda frame: frame.get("inventory_browser") is not None)
        actor = next(actor for actor in opened["actors"] if actor["is_self"])
        self.assertLess(actor["screen"][0], 0.4, "the player must remain visible in the uncovered world beside inventory")
        self.assertEqual(actor["world"], original["world"])
        self.assertEqual(opened["revision"], initial["revision"])
        self.tap(window, native, "Escape", lambda frame: not frame["panel_open"])
        restored = self.captured(window, lambda frame: any(actor["is_self"] and 0.45 < actor["screen"][0] < 0.55 for actor in frame["actors"]))
        self.assertEqual(restored["world_feet"], initial["world_feet"])
        native("Escape", True)
        self.assertEqual(window.child.wait(timeout=30), 0)

    def test_inventory_grid_search_and_arrow_selection_do_not_send_world_actions(self):
        self.server(scenario=self.fallback_package())
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--connect", self.address, "--capture", self.directory / "inventory-grid", "--capture-every", "0.4",
        ], executable=executable)
        initial = self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        native = self.native_keys(window)
        opened = self.tap(window, native, "i", lambda frame: frame.get("inventory_browser") is not None)
        self.assertTrue(opened["inventory_browser"]["shown"])
        selected = self.tap(window, native, "Right", lambda frame: frame["inventory_browser"]["selected"] is not None)
        self.assertEqual(selected["inventory_browser"]["selected"], opened["inventory_browser"]["shown"][0])
        native("Control_L", True)
        try:
            self.tap(window, native, "f", lambda frame: frame["inventory_browser"]["search_focused"])
        finally:
            native("Control_L", False)
        self.captured(window, lambda frame: "ControlLeft" not in frame["keys"])
        query = ""
        for letter in "wqm1":
            query += letter
            searched = self.tap(window, native, letter, lambda frame, query=query: frame["inventory_browser"]["query"] == query)
        self.assertEqual(searched["inventory_browser"]["shown"], [])
        self.assertEqual(searched["minimap_open"], initial["minimap_open"], "typing M must not toggle the map")
        self.assertEqual(searched["world_feet"], initial["world_feet"])
        self.assertEqual(searched["revision"], initial["revision"], "search and selection must not submit an item or movement action")
        self.tap(window, native, "Escape", lambda frame: frame["panel_open"] and not frame["inventory_browser"]["search_focused"])
        self.tap(window, native, "Escape", lambda frame: not frame["panel_open"])
        native("Escape", True)
        self.assertEqual(window.child.wait(timeout=30), 0)

    def test_inventory_grid_mouse_filter_selection_and_drop_use_the_disclosed_item(self):
        self.server(scenario=self.fallback_package())
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--connect", self.address, "--capture", self.directory / "inventory-mouse", "--capture-every", "0.4",
        ], executable=executable)
        self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        native = self.native_keys(window)
        self.tap(window, native, "g", lambda frame: any(item["name"] == "unfamiliar relic" for item in frame["inventory"]))
        opened = self.tap(window, native, "i", lambda frame: frame.get("inventory_browser") is not None)
        mouse = self.native_mouse(window)

        def click_control(frame, predicate, done):
            control = next(control for control in frame["inventory_browser"]["controls"] if predicate(control))
            try:
                mouse(control["screen"], True)
                self.captured(window, done)
            finally:
                mouse(control["screen"], False)
            return self.captured(window, lambda frame: "Left" not in frame["mouse"] and done(frame))

        gear = click_control(opened, lambda control: control["control"] == "Filter(Gear)",
                             lambda frame: frame["inventory_browser"]["filter"] == "Gear")
        all_items = click_control(gear, lambda control: control["control"] == "Filter(All)",
                                  lambda frame: frame["inventory_browser"]["filter"] == "All")
        index = next(index for index, label in enumerate(all_items["panel_actions"][:10]) if label.endswith(": Drop"))
        item = next(item for item in all_items["inventory"] if item["id"] == all_items["panel_item_targets"][index])
        selected = click_control(all_items, lambda control: control["target"] == item["id"],
                                 lambda frame: frame["inventory_browser"]["selected"] == item["id"])
        self.assertEqual(selected["revision"], opened["revision"])
        self.assertEqual(selected["world_feet"], opened["world_feet"])
        self.tap(window, native, str((index + 1) % 10), lambda frame: all(
            current["id"] != item["id"] or int(current["quantity"]) < int(item["quantity"]) for current in frame["inventory"]))
        self.tap(window, native, "Escape", lambda frame: not frame["panel_open"])
        native("Escape", True)
        self.assertEqual(window.child.wait(timeout=30), 0)

    def test_framed_hud_dock_opens_inventory_without_a_world_action(self):
        self.server(scenario=self.fallback_package())
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--connect", self.address, "--capture", self.directory / "hud", "--capture-every", "0.4",
        ], executable=executable)
        initial = self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        self.assertTrue("hud" in initial, "the live frame must present the framed action dock")
        self.assertEqual(initial["hud"]["readiness"], "READY")
        self.assertEqual({button["action"] for button in initial["hud"]["buttons"]},
                         {"Inventory", "Equipment", "Character", "Map", "Places", "Journal", "Wait"})
        for button in initial["hud"]["buttons"]:
            self.assertTrue(all(0 < value < 1 for value in button["screen"]), button)
            self.assertTrue(all(value > 20 for value in button["size"]), button)
        pack = next(button for button in initial["hud"]["buttons"] if button["action"] == "Inventory")
        mouse = self.native_mouse(window)
        try:
            mouse(pack["screen"], True)
            opened = self.captured(window, lambda frame: frame["panel_open"])
        finally:
            mouse(pack["screen"], False)
        self.assertEqual(opened["revision"], initial["revision"], "the dock must not click through into the world")
        self.assertEqual(opened["world_feet"], initial["world_feet"])
        self.assertTrue(opened["panel_actions"])
        native = self.native_keys(window)
        closed = self.tap(window, native, "Escape", lambda frame: not frame["panel_open"])
        try:
            mouse([0.98, 0.98], True)
            blank = self.captured(window, lambda frame: "Left" in frame["mouse"])
        finally:
            mouse([0.98, 0.98], False)
        self.assertEqual(blank["revision"], closed["revision"], "empty dock areas must also block world clicks")
        self.captured(window, lambda frame: "Left" not in frame["mouse"])
        journal = next(button for button in initial["hud"]["buttons"] if button["action"] == "Journal")
        try:
            mouse(journal["screen"], True)
            history = self.captured(window, lambda frame: frame["screen"] == "History")
        finally:
            mouse(journal["screen"], False)
        self.assertEqual(history["revision"], closed["revision"])
        self.tap(window, native, "Escape", lambda frame: frame["screen"] is None)
        self.assertIsNone(window.child.poll())
        native("Escape", True)
        self.assertEqual(window.child.wait(timeout=30), 0)

    @model_generation.local
    def test_art_lineup_presents_generated_bodies_and_accepts_native_close(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        captures = self.directory / "art-lineup"
        captures.mkdir(parents=True)
        window = self.launch("tor-client-3d", [
            "--lineups", "--capture", captures, "--capture-warmup", "2",
        ], executable=executable)
        deadline = time.monotonic() + 90
        image = captures / "00-line-Warrior.png"
        def complete_png():
            if not image.exists() or image.stat().st_size < 12:
                return False
            with image.open("rb") as stream:
                stream.seek(-12, 2)
                return stream.read() == b"\x00\x00\x00\x00IEND\xaeB`\x82"
        while not complete_png() and time.monotonic() < deadline:
            self.assertIsNone(window.child.poll(), "client exited before presenting its generated lineup")
            time.sleep(0.05)
        self.assertTrue(complete_png(), "complete GPU screenshot of the five generated bodies is required")
        data = image.read_bytes()
        self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
        self.assertGreater(len(data), 4096)
        width, height = struct.unpack(">II", data[16:24])
        self.assertGreaterEqual(width, 640)
        self.assertGreaterEqual(height, 480)
        handle = self._native_window(window, expected_title="Thresholds of Ruin \u2014 bestiary")
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes
            user32 = ctypes.WinDLL("user32", use_last_error=True)
            user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
            self.assertTrue(user32.PostMessageW(handle, 0x0010, 0, 0))  # WM_CLOSE
        else:
            # Send the normal close request directly: CI's Xvfb has no window
            # manager, and xdotool windowclose forcibly destroys the surface.
            import ctypes
            import ctypes.util
            class Data(ctypes.Union):
                _fields_ = [("longs", ctypes.c_long * 5)]
            class ClientMessage(ctypes.Structure):
                _fields_ = [("type", ctypes.c_int), ("serial", ctypes.c_ulong),
                            ("send_event", ctypes.c_int), ("display", ctypes.c_void_p),
                            ("window", ctypes.c_ulong), ("message_type", ctypes.c_ulong),
                            ("format", ctypes.c_int), ("data", Data)]
            class Event(ctypes.Union):
                _fields_ = [("client", ClientMessage), ("padding", ctypes.c_long * 24)]
            library = ctypes.util.find_library("X11")
            self.assertIsNotNone(library, "native close requires X11")
            x11 = ctypes.CDLL(library)
            x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
            x11.XOpenDisplay.restype = ctypes.c_void_p
            x11.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
            x11.XInternAtom.restype = ctypes.c_ulong
            x11.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int,
                                      ctypes.c_long, ctypes.POINTER(Event)]
            x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
            display = x11.XOpenDisplay(None)
            self.assertTrue(display, "native close requires an X11 display")
            try:
                event = Event()
                event.client.type = 33  # ClientMessage
                event.client.display = display
                event.client.window = int(handle, 0)
                event.client.message_type = x11.XInternAtom(display, b"WM_PROTOCOLS", 0)
                event.client.format = 32
                event.client.data.longs[0] = x11.XInternAtom(display, b"WM_DELETE_WINDOW", 0)
                self.assertTrue(x11.XSendEvent(display, int(handle, 0), 0, 0, ctypes.byref(event)))
            finally:
                x11.XCloseDisplay(display)
        self.assertEqual(window.child.wait(timeout=30), 0)
        errors = [line for line in window.stdout_path.read_text(encoding="utf-8").splitlines()
                  if "ERROR" in line and any(term in line.lower() for term in ("shader", "pipeline", "wgpu"))]
        self.assertFalse(errors, "\n".join(errors)[-2500:])

    @model_generation.local
    def test_rogue_family_lineups_render_their_own_bodies_and_finish(self):
        """Shared humanoid contours, fitted adult eyes and Rogue recipes reach
        real GPU frames, including a glowing vampire, feathered and membrane-winged
        beasts, a compact ground bird, two hinged biting snakes, a reared grasping aberration, a squat amphibian, an armored maw, a broad-leaved rooted trap
        and a spectral body, each in a line-up."""
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        captures = self.directory / "rogue-lineup"
        captures.mkdir(parents=True)
        # Line-ups keep the bestiary's order whatever order the names are given in.
        families = ["vampire", "emu", "snake", "nymph", "medusa", "phantom", "wraith", "bat", "kestrel", "griffin", "dragon", "jabberwock", "aquator", "xeroc", "venus flytrap", "rattlesnake"]
        window = self.launch("tor-client-3d", [
            "--lineups", "--family", ",".join(reversed(families)), "--capture", captures, "--capture-warmup", "2",
        ], executable=executable)
        self.assertEqual(window.child.wait(timeout=240), 0, "line-ups must finish once every family is captured")
        for index, name in enumerate(families):
            for view in ("line", "close-a", "close-b"):
                image = captures / f"{index:02d}-{view}-{name.replace(' ', '_')}.png"
                self.assertTrue(image.exists(), f"missing {image.name}")
                data = image.read_bytes()
                self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
                self.assertEqual(data[-12:], b"\x00\x00\x00\x00IEND\xaeB`\x82", image.name)
                self.assertGreater(len(data), 4096, image.name)
        # Only the named families are lined up.
        self.assertEqual(len(list(captures.glob("*.png"))), len(families) * 3)

    @model_generation.local
    def test_women_lineups_study_each_individuals_own_bust(self):
        """A figure study lines up only women of each people, each with her
        own resolved bust, and renders them to real GPU frames."""
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        captures = self.directory / "women-lineup"
        captures.mkdir(parents=True)
        # Fixed witnesses cover the authored coat openings and chest details;
        # exact recipe-transfer/restoration assertions belong to garment_tests.
        families = ["Warrior", "Mage", "hobgoblin", "zombie", "vampire",
                    "orc", "troll", "leprechaun", "nymph", "medusa", "phantom", "wraith"]
        window = self.launch("tor-client-3d", [
            "--women", "--family", ",".join(families), "--capture", captures, "--capture-warmup", "2",
        ], executable=executable, extra_env={"RUST_LOG": "info"})
        self.assertEqual(window.child.wait(timeout=240), 0, "the study must finish once every family is captured")
        for index, name in enumerate(families):
            for view in ("line", "close-a", "close-b"):
                image = captures / f"{index:02d}-{view}-{name}.png"
                self.assertTrue(image.exists(), f"missing {image.name}")
                self.assertGreater(len(image.read_bytes()), 4096, image.name)
        log = window.stdout_path.read_text(encoding="utf-8")
        for name in families:
            busts = re.findall(rf"Lineup individual family={name} index=\d bust=(\w+):([\d.]+)", log)
            self.assertEqual(len(busts), 5, f"{name}: every individual is a woman with a bust\n{log[-2000:]}")
            self.assertGreater(len(set(busts)), 1, f"{name}: individuals differ: {busts}")
        self.assertNotIn("panicked", log)

    @model_generation.local
    def test_equipment_conflicts_restore_compositions_in_one_native_run(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        source = CLIENT / "review-scripts/equipment-conflicts.json"
        script = json.loads(source.read_text(encoding="utf-8"))
        output = self.directory / "equipment-conflicts"
        window = self.launch("tor-client-3d", ["--scene-script", source, "--capture", output],
                             executable=executable, extra_env={"RUST_LOG": "info,wgpu_hal=warn"})
        self.assertEqual(window.child.wait(timeout=240), 0, window.stdout_path.read_text(encoding="utf-8")[-3000:])
        manifest = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
        self.assertEqual(manifest["status"], "complete")
        shots = manifest["captures"]
        self.assertEqual([(s["name"], s["tick"]) for s in shots],
                         [(s["name"], s["tick"]) for s in script["captures"]])
        by_tick = {}
        for shot in shots:
            if shot["tick"] in by_tick:
                self.assertEqual(shot["actors"], by_tick[shot["tick"]], "camera changes must preserve model state")
            by_tick[shot["tick"]] = shot["actors"]
            image = (output / (shot["name"] + ".png")).read_bytes()
            self.assertEqual(image[:8], b"\x89PNG\r\n\x1a\n")
            self.assertEqual(struct.unpack(">II", image[16:24]), (640, 480))
            self.assertGreater(len(image), 4096)
        snapshots = {tick: {a["id"]: a for a in actors} for tick, actors in by_tick.items()}
        self.assertEqual({a["model"] for a in script["actors"]}, {"Human", "Nymph", "Mage", "RedMage"})
        for requested in script["actors"]:
            initial, bare, restored = (snapshots[t][requested["id"]] for t in (0, 10, 20))
            self.assertEqual(initial["composition"]["equipment"], requested["equipment"])
            self.assertEqual(bare["composition"]["equipment"], [])
            self.assertEqual(restored["composition"], initial["composition"], "unequip/re-equip preserves body and outfit")
            self.assertNotEqual(bare["resolved_key"], initial["resolved_key"])
            self.assertEqual(restored["resolved_key"], initial["resolved_key"])
            self.assertEqual(restored["root_children"], initial["root_children"], "replacement must not accumulate rigs")
            self.assertGreater(initial["root_children"], 0)
            self.assertNotEqual(restored["joints"], initial["joints"], "the equipped model must actually animate")

    @model_generation.local
    def test_redressed_head_matches_a_cold_render_in_both_outfit_orders(self):
        from render_pixels import decode_png
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        images = {}
        states = {}
        for initial, final in [("commoner", "commoner"), ("princess", "princess"),
                               ("commoner", "princess"), ("princess", "commoner")]:
            name = f"{initial}-to-{final}"
            script = {
                "version": 1, "hz": 50, "viewport": [320, 320],
                "actors": [{"id": "wearer", "model": "Human", "seed": 17,
                            "body": {"woman": True, "hair": 2}, "outfit": initial,
                            "equipment": [], "position": [0, 0, 0], "yaw": 0}],
                "cameras": [{"id": "neck", "position": [0, 1.078, 1.5],
                             "target": [0, 1.078, 0], "fov_degrees": 10}],
                "events": [{"tick": 0, "actor": "wearer",
                            "operation": {"type": "teleport", "position": [0, 0, 0], "yaw": 0}},
                           {"tick": 0, "actor": "wearer",
                            "operation": {"type": "dress", "outfit": final}}],
                "captures": [{"tick": 0, "name": "head", "camera": "neck"}],
            }
            source = self.directory / f"{name}.json"
            source.write_text(json.dumps(script), encoding="utf-8")
            output = self.directory / name
            window = self.launch("tor-client-3d", ["--scene-script", source, "--capture", output],
                                 executable=executable)
            self.assertEqual(window.child.wait(timeout=240), 0,
                             window.stdout_path.read_text(encoding="utf-8")[-3000:])
            manifest = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(manifest["status"], "complete")
            actor = manifest["captures"][0]["actors"][0]
            states[initial, final] = actor
            self.assertEqual(actor["composition"]["outfit"], final)
            self.assertEqual(manifest["captures"][0]["clock_ns"], "0")
            width, height, channels, rows = decode_png((output / "head.png").read_bytes())
            self.assertEqual((width, height), (320, 320))
            # The close-up isolates the neck. Quantized darkness tolerates small
            # shading differences while preserving the actual choker silhouette.
            images[initial, final] = [max(rows[y][x * channels:x * channels + 3]) < 80
                                      for y in range(int(height * 0.46), int(height * 0.54))
                                      for x in range(int(width * 0.25), int(width * 0.75))]
        plain, choker = images["commoner", "commoner"], images["princess", "princess"]
        self.assertGreater(sum(a != b for a, b in zip(plain, choker)), 200,
                           "positive witness: the camera must see the neck ornament")
        for initial, final in [("commoner", "princess"), ("princess", "commoner")]:
            self.assertEqual(states[initial, final]["joints"], states[final, final]["joints"],
                             "compare the same explicit pose, not construction versus refreshed idle")
            expected = images[final, final]
            changed = sum(a != b for a, b in zip(images[initial, final], expected))
            self.assertLess(changed, 50, f"{initial} -> {final}: cached head must match cold render")

    @model_generation.local
    def test_yeti_surface_coat_presents_around_the_body_during_motion(self):
        from render_pixels import decode_png
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        source = CLIENT / "review-scripts/yeti-coat.json"
        output = self.directory / "yeti-coat"
        window = self.launch("tor-client-3d", ["--scene-script", source, "--capture", output],
                             executable=executable)
        self.assertEqual(window.child.wait(timeout=240), 0,
                         window.stdout_path.read_text(encoding="utf-8")[-3000:])
        report = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
        self.assertEqual(report["status"], "complete")
        expected = {f"{view}-{pose}" for view in ("front", "side", "back")
                    for pose in ("rest", "walk", "heavy")}
        self.assertEqual({shot["name"] for shot in report["captures"]}, expected)
        snapshots, images = {}, {}
        for shot in report["captures"]:
            tick = shot["tick"]
            self.assertIn(tick, (0, 25, 50))
            self.assertEqual(shot["clock_ns"], str(tick * 20_000_000))
            self.assertEqual(len(shot["actors"]), 1)
            actor = shot["actors"][0]
            self.assertEqual(actor["id"], "yeti")
            self.assertEqual(actor["composition"]["model"], "Yeti")
            self.assertGreater(actor["root_children"], 0)
            self.assertTrue(all(math.isfinite(v) for frame in actor["joints"].values() for v in frame))
            if tick in snapshots:
                self.assertEqual(actor, snapshots[tick], "camera changes hold the same pose")
            snapshots[tick] = actor
            data = (output / (shot["name"] + ".png")).read_bytes()
            width, height, channels, rows = decode_png(data)
            self.assertEqual((width, height), (640, 640))
            # White fur need not be saturated. Compare with both background
            # edges on this row; either edge can cross the finite floor horizon.
            # Keep the control strip above the cast shadow at the screen bottom.
            def foreground(x, y):
                pixel = rows[y][x * channels:x * channels + 3]
                return min(max(abs(a - b) for a, b in
                               zip(pixel, rows[y][edge * channels:edge * channels + 3]))
                           for edge in (8, 632)) > 20

            presented = sum(foreground(x, y) for y in range(80, 520) for x in range(80, 560))
            self.assertGreater(presented, 10_000, "each view must present a substantial body")
            self.assertFalse(any(foreground(x, y) for y in range(80, 520) for x in range(608, 640)),
                             "the same oracle must reject the empty backdrop and floor")
            images[shot["name"]] = hashlib.sha256(data).digest()
        self.assertNotEqual(snapshots[0]["joints"]["HandR"][:12], snapshots[25]["joints"]["HandR"][:12])
        self.assertNotEqual(snapshots[25]["joints"]["Torso"][:12], snapshots[50]["joints"]["Torso"][:12])
        for view in ("front", "side", "back"):
            self.assertNotEqual(images[view + "-rest"], images[view + "-walk"])
            self.assertNotEqual(images[view + "-walk"], images[view + "-heavy"])

    @model_generation.local
    def test_centaur_rider_builds_present_and_keep_the_grip_during_motion(self):
        from render_pixels import decode_png
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        source = CLIENT / "review-scripts/centaur-builds.json"
        script = json.loads(source.read_text(encoding="utf-8"))
        output = self.directory / "centaur-builds"
        window = self.launch("tor-client-3d", ["--scene-script", source, "--capture", output],
                             executable=executable)
        self.assertEqual(window.child.wait(timeout=240), 0,
                         window.stdout_path.read_text(encoding="utf-8")[-3000:])
        report = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
        self.assertEqual(report["status"], "complete")
        self.assertEqual(len(report["captures"]), 12)
        expected_ids = {actor["id"] for actor in script["actors"]}
        self.assertEqual(len(expected_ids), 6)
        snapshots = {}
        images = {}
        for shot in report["captures"]:
            tick = shot["tick"]
            self.assertIn(tick, (0, 25))
            self.assertEqual(shot["clock_ns"], str(tick * 20_000_000))
            actors = {actor["id"]: actor for actor in shot["actors"]}
            self.assertEqual(set(actors), expected_ids)
            if tick in snapshots:
                self.assertEqual(actors, snapshots[tick], "camera changes must hold the same pose")
            snapshots[tick] = actors
            data = (output / (shot["name"] + ".png")).read_bytes()
            width, height, channels, rows = decode_png(data)
            self.assertEqual((width, height), (640, 640))
            colored = sum(max(rows[y][x * channels:x * channels + 3])
                          - min(rows[y][x * channels:x * channels + 3]) > 25
                          for y in range(160, 560) for x in range(160, 480))
            self.assertGreater(colored, 500, "the GPU capture must contain the colored body, not just floor")
            images[shot["name"]] = hashlib.sha256(data).digest()

        def hand_in_torso(actor):
            torso, hand = actor["joints"]["Torso"], actor["joints"]["HandR"]
            self.assertTrue(all(math.isfinite(v) for v in torso + hand))
            delta = [hand[index] - torso[index] for index in range(12, 15)]
            # Bone frames carry uniform creature scale. Project onto their
            # orthogonal axes to compare anatomy independently of world scale.
            return [sum(d * v for d, v in zip(delta, torso[start:start + 3]))
                    / sum(v * v for v in torso[start:start + 3]) for start in (0, 4, 8)]

        for requested in script["actors"]:
            identity = requested["id"]
            rest, walking = snapshots[0][identity], snapshots[25][identity]
            self.assertEqual(rest["composition"]["model"], "Centaur")
            self.assertEqual(rest["composition"]["body"], walking["composition"]["body"])
            self.assertGreater(rest["root_children"], 0)
            self.assertEqual(rest["root_children"], walking["root_children"])
            self.assertAlmostEqual(walking["position"][2], 24 * 0.65 / 50, places=5)
            self.assertNotEqual(rest["joints"]["HandR"][:12], walking["joints"]["HandR"][:12])
            self.assertNotEqual(images[identity + "-rest"], images[identity + "-walk"])
            for actor in (rest, walking):
                local = hand_in_torso(actor)
                self.assertTrue(0.30 < math.hypot(local[1], local[2]) < 0.65,
                                "the spear grip must stay beyond the elbow during the held pose")
        for sex in ("man", "woman"):
            for tick in (0, 25):
                hands = [hand_in_torso(snapshots[tick][f"{build}-{sex}"])
                         for build in ("slender", "average", "sturdy")]
                widths = [abs(hand[0]) for hand in hands]
                self.assertTrue(widths[1] > widths[0] * 1.04 and widths[2] > widths[1] * 1.04,
                                f"{sex}/{tick}: rendered shoulder spacing must follow build: {widths}")
                for hand in hands:
                    self.assertAlmostEqual(hand[1], hands[1][1], places=4,
                                           msg="fullness must not stretch the arm's reach")
                    self.assertAlmostEqual(hand[2], hands[1][2], places=4,
                                           msg="fullness must not move the weapon out of its grip")

    @model_generation.local
    def test_scripted_review_replays_composition_motion_and_capture_ticks(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        script = {
            "version": 1, "hz": 50, "viewport": [640, 480],
            "actors": [{"id": "woman", "model": "Human", "seed": 7,
                        "body": {"woman": True}, "outfit": "royal-princess",
                        "equipment": [], "position": [0, 0, 0], "yaw": 0}],
            "cameras": [{"id": "front", "position": [0, 1.2, 3], "target": [0, 0.5, 0], "fov_degrees": 40},
                        {"id": "side", "position": [3, 1.2, 0], "target": [0, 0.5, 0], "fov_degrees": 40}],
            "events": [
                {"tick": 1, "actor": "woman", "operation": {"type": "action", "name": "jump"}},
                {"tick": 1, "actor": "woman", "operation": {"type": "velocity", "value": [1, 0, 0]}},
                {"tick": 5, "actor": "woman", "operation": {"type": "dress", "outfit": "leotard"}},
                {"tick": 5, "actor": "woman", "operation": {"type": "equipment", "items": ["breastplate"]}},
            ],
            "captures": [{"tick": 0, "name": "initial-front", "camera": "front"},
                         {"tick": 0, "name": "initial-side", "camera": "side"},
                         {"tick": 10, "name": "moving", "camera": "front"}],
        }
        source = self.directory / "scene.json"
        source.write_text(json.dumps(script), encoding="utf-8")
        manifests = []
        for index in range(2):
            output = self.directory / f"scene-{index}"
            window = self.launch("tor-client-3d", ["--scene-script", source, "--capture", output],
                                 executable=executable, extra_env={"RUST_LOG": "info,wgpu_hal=warn"})
            self.assertEqual(window.child.wait(timeout=240), 0, window.stdout_path.read_text(encoding="utf-8")[-3000:])
            manifest = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(manifest["status"], "complete")
            shots = manifest["captures"]
            self.assertEqual([s["tick"] for s in shots], [0, 0, 10])
            self.assertEqual([s["clock_ns"] for s in shots], ["0", "0", "200000000"])
            self.assertEqual(shots[0]["actors"], shots[1]["actors"], "camera changes cannot age the pose")
            actor = shots[2]["actors"][0]
            # Velocity begins at tick 1; capture tick 10 follows nine intervals.
            self.assertAlmostEqual(actor["position"][0], 9 / 50, places=5)
            self.assertEqual(actor["composition"]["outfit"], "leotard")
            self.assertEqual(actor["composition"]["equipment"], ["breastplate"])
            self.assertEqual(actor["action"]["name"], "Jump")
            self.assertEqual(actor["root_children"], shots[0]["actors"][0]["root_children"], "same-tick changes must not leave duplicate rigs")
            self.assertNotEqual(actor["joints"], shots[0]["actors"][0]["joints"], "joint pose must actually animate")
            self.assertGreater(actor["action"]["phase"], 0)
            for shot in shots:
                image = (output / (shot["name"] + ".png")).read_bytes()
                self.assertEqual(image[:8], b"\x89PNG\r\n\x1a\n")
                self.assertEqual(struct.unpack(">II", image[16:24]), (640, 480))
                self.assertGreater(len(image), 4096)
                self.assertTrue(shot["renderer"]["name"])
            manifests.append(manifest)
        self.assertEqual([s["actors"] for s in manifests[0]["captures"]],
                         [s["actors"] for s in manifests[1]["captures"]], "resolved poses replay independently of renderer delay")

    @model_generation.local
    def test_scripted_motion_events_apply_at_their_timestamp(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        expected = [(0, 0.0), (10, 0.0), (11, 0.02), (19, 0.18),
                    (20, 1.0), (21, 1.04), (30, 1.4), (31, 1.4)]
        script = {
            "version": 1, "hz": 50, "viewport": [640, 480],
            "actors": [{"id": "actor", "model": "Human", "seed": 7,
                        "body": {"woman": False}, "outfit": "ranger", "equipment": [],
                        "position": [0, 0, 0], "yaw": 0}],
            "cameras": [{"id": "front", "position": [0, 1, 3],
                         "target": [0, 0.5, 0], "fov_degrees": 45}],
            "events": [
                {"tick": 10, "actor": "actor", "operation": {"type": "velocity", "value": [1, 0, 0]}},
                {"tick": 20, "actor": "actor", "operation": {"type": "teleport", "position": [1, 0, 0], "yaw": 90}},
                {"tick": 20, "actor": "actor", "operation": {"type": "velocity", "value": [3, 0, 0]}},
                {"tick": 20, "actor": "actor", "operation": {"type": "velocity", "value": [2, 0, 0]}},
                {"tick": 30, "actor": "actor", "operation": {"type": "velocity", "value": [0, 0, 0]}},
            ],
            "captures": [{"tick": tick, "name": f"boundary-{tick}", "camera": "front"}
                         for tick, _ in expected],
        }
        source = self.directory / "event-boundaries.json"
        source.write_text(json.dumps(script), encoding="utf-8")
        output = self.directory / "event-boundaries"
        window = self.launch("tor-client-3d", ["--scene-script", source, "--capture", output],
                             executable=executable)
        self.assertEqual(window.child.wait(timeout=240), 0,
                         window.stdout_path.read_text(encoding="utf-8")[-3000:])
        report = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
        self.assertEqual(report["status"], "complete")
        self.assertEqual([shot["tick"] for shot in report["captures"]], [tick for tick, _ in expected])
        for shot, (tick, x) in zip(report["captures"], expected):
            with self.subTest(tick=tick):
                position = shot["actors"][0]["position"]
                self.assertAlmostEqual(position[0], x, places=5,
                                       msg="events affect the following interval, never the preceding one")
                self.assertEqual(position[1:], [0.0, 0.0])

    @model_generation.local
    def test_scripted_redress_releasing_a_cloak_still_captures(self):
        # Redressing frees the old outfit's wearer-owned meshes (here the
        # paladin's cloak); readiness must wait only for what is drawn now.
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        script = {
            "version": 1, "hz": 50, "viewport": [480, 640],
            "actors": [{"id": "actor", "model": "Human", "seed": 7,
                        "body": {"woman": True}, "outfit": "paladin", "equipment": [],
                        "position": [0, 0, 0], "yaw": 0}],
            "cameras": [{"id": "front", "position": [0, 1, 3], "target": [0, 0.5, 0], "fov_degrees": 45}],
            "events": [{"tick": 2, "actor": "actor", "operation": {"type": "dress", "outfit": "barbarian"}}],
            "captures": [{"tick": 1, "name": "paladin", "camera": "front"},
                         {"tick": 3, "name": "barbarian", "camera": "front"}],
        }
        source = self.directory / "redress.json"
        source.write_text(json.dumps(script), encoding="utf-8")
        output = self.directory / "redress"
        window = self.launch("tor-client-3d", ["--scene-script", source, "--capture", output],
                             executable=executable)
        self.assertEqual(window.child.wait(timeout=240), 0,
                         window.stdout_path.read_text(encoding="utf-8")[-3000:])
        report = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
        self.assertEqual(report["status"], "complete")
        self.assertEqual([shot["name"] for shot in report["captures"]], ["paladin", "barbarian"])
        self.assertTrue((output / "barbarian.png").is_file())

    @model_generation.local
    def test_startup_splash_presents_before_world_start_and_enters_with_eggs(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--server-bin", self.bin / ("tor-server" + self.suffix),
            "--scenario", CLIENT / "scenarios/crowd-hundred", "--seed", "7", "--distance", "34",
            "--save-root", self.directory / "startup-saves", "--capture", self.directory / "startup-captures",
            "--capture-every", "0.4", "--quit-after", "120",
        ], executable=executable)
        shown = self.frame(window, lambda frame: frame.get("type") == "startup_splash" and frame.get("event") == "presented", seconds=45)
        self.own_private_backend_cleanup(shown["pid"])
        self.assertNotEqual(shown["pid"], window.child.pid)
        self.assertEqual(shown["completed"], 0)
        self.assertTrue(shown["software_presented"], "the first frame must not depend on the game's GPU initialization")
        initial_png = Path(shown["png"]).read_bytes()
        self.assertEqual(initial_png[:8], b"\x89PNG\r\n\x1a\n")
        self.assertGreater(len(initial_png), 4096)
        started = self.frame(window, lambda frame: frame.get("type") == "local_server", seconds=90)
        self.own_private_backend_cleanup(started["pid"])
        steps = [shown]

        def entering(frame):
            if frame.get("type") == "startup_splash" and frame.get("event") == "presented":
                steps.append(frame)
                return frame["completed"] == frame["total"] == 4
            return False

        ready = self.frame(window, entering, seconds=120)
        self.assertEqual([step["completed"] for step in steps], sorted(step["completed"] for step in steps))
        self.assertTrue(any(0 < step["completed"] < 4 for step in steps), "present an intermediate progress milestone")
        self.assertNotEqual(initial_png, Path(ready["png"]).read_bytes(), "the progress display must actually change its pixels")
        frame = self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        self.assertGreater(frame["pending_models"], 0, "entering the dungeon must not wait for all hundred models")
        self.assertEqual(frame["failed_models"], 0)
        native = self.native_keys(window)
        self.tap(window, native, "i", lambda frame: frame["panel_open"])
        self.tap(window, native, "Escape", lambda frame: not frame["panel_open"])
        native("Escape", True)
        try:
            self.assertEqual(window.child.wait(timeout=40), 0)
        finally:
            if window.child.poll() is None:
                native("Escape", False)
        reports = [json.loads(line) for line in window.stdout_path.read_text(encoding="utf-8").splitlines() if line.startswith("{")]
        self.assertTrue(any(report.get("type") == "startup_splash" and report.get("event") == "closed"
                            and report["pid"] == shown["pid"] for report in reports))

    def test_closing_the_startup_splash_cancels_the_client_and_owned_world(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--server-bin", self.bin / ("tor-server" + self.suffix), "--scenario", ROOT / "scenarios/two-room",
            "--save-root", self.directory / "cancelled-startup", "--quit-after", "120",
        ], executable=executable)
        shown = self.frame(window, lambda frame: frame.get("type") == "startup_splash" and frame.get("event") == "presented", seconds=45)
        self.own_private_backend_cleanup(shown["pid"])
        helper = SimpleNamespace(child=SimpleNamespace(pid=shown["pid"]))
        native = self.native_keys(helper, expected_title="Thresholds of Ruin — Starting")
        native("Escape", True)
        self.assertEqual(window.child.wait(timeout=90), 0)
        reports = [json.loads(line) for line in window.stdout_path.read_text(encoding="utf-8").splitlines() if line.startswith("{")]
        self.assertTrue(any(report.get("event") == "cancelled" and report.get("pid") == shown["pid"] for report in reports))
        for report in reports:
            if report.get("type") != "local_server":
                continue
            host, port = report["address"].rsplit(":", 1)
            try:
                with socket.create_connection((host, int(port)), timeout=2):
                    self.own_private_backend_cleanup(report["pid"])
                    self.fail("cancelling startup left its private server running")
            except OSError:
                pass

    @model_generation.local
    def test_live_models_show_eggs_accept_input_and_replace_them_without_a_new_view(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--server-bin", self.bin / ("tor-server" + self.suffix),
            "--scenario", CLIENT / "scenarios/crowd", "--seed", "7", "--distance", "16",
            "--save-root", self.directory / "async-model-saves", "--capture", self.directory / "async-models",
            "--capture-every", "0.4", "--quit-after", "180",
        ], executable=executable)
        started = self.frame(window, lambda frame: frame.get("type") == "local_server", seconds=90)
        self.own_private_backend_cleanup(started["pid"])
        loading = self.captured(window, lambda frame: frame["has_control"] and frame["pending_models"] > 0)
        eggs = {actor["target"] for actor in loading["actors"]
                if not actor["model_ready"] and actor["presented_plan"] == "Marker"}
        self.assertTrue(eggs, "actual GPU capture must contain disclosed actors presented as eggs")
        self.assertEqual(loading["failed_models"], 0)
        native = self.native_keys(window)
        before = next(actor["world"] for actor in loading["actors"] if actor["is_self"])
        moved = self.tap(window, native, "a", lambda frame: frame["world_feet"] != loading["world_feet"] and any(
            actor["is_self"] and actor["world"] != before for actor in frame["actors"]))
        self.assertGreater(moved["pending_models"], 0, "both server movement and the presented player must advance while models load")
        opened = self.tap(window, native, "i", lambda frame: frame["panel_open"])
        self.assertGreater(opened["pending_models"], 0, "inventory input must be processed while models are still loading")
        closed = self.tap(window, native, "Escape", lambda frame: not frame["panel_open"])
        ready = self.captured(window, lambda frame: frame["pending_models"] == 0)
        self.assertEqual(ready["failed_models"], 0)
        self.assertEqual(ready["revision"], closed["revision"], "completion must not need another server view after movement")
        replaced = [actor for actor in ready["actors"] if actor["target"] in eggs]
        self.assertEqual(len(replaced), len(eggs))
        self.assertTrue(all(actor["model_ready"] and actor["presented_plan"] != "Marker" for actor in replaced))
        self.assertNotEqual(Path(ready["png"]).read_bytes(), Path(loading["png"]).read_bytes())
        native("Escape", True)
        try:
            self.assertEqual(window.child.wait(timeout=40), 0)
        finally:
            if window.child.poll() is None:
                native("Escape", False)

    @model_generation.local
    def test_closing_while_models_load_stops_only_the_owned_private_backend(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--server-bin", self.bin / ("tor-server" + self.suffix),
            "--scenario", CLIENT / "scenarios/crowd-hundred", "--seed", "7", "--distance", "34",
            "--save-root", self.directory / "unfinished-model-saves", "--capture", self.directory / "unfinished-models",
            "--capture-every", "0.4", "--quit-after", "120",
        ], executable=executable)
        started = self.frame(window, lambda frame: frame.get("type") == "local_server", seconds=90)
        self.own_private_backend_cleanup(started["pid"])
        self.captured(window, lambda frame: frame["has_control"] and frame["pending_models"] > 0)
        native = self.native_keys(window)
        native("Escape", True)
        try:
            self.assertEqual(window.child.wait(timeout=40), 0)
        finally:
            if window.child.poll() is None:
                native("Escape", False)
        host, port = started["address"].rsplit(":", 1)
        with self.assertRaises(OSError, msg="quitting with pending models must stop its private server"):
            with socket.create_connection((host, int(port)), timeout=2):
                pass
        self.assertTrue(Path(started["save"]).is_file())

    @model_generation.local
    def test_frame_stats_report_the_crowd_benchmark_and_exit(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--server-bin", self.bin / ("tor-server" + self.suffix), "--scenario", CLIENT / "scenarios/crowd",
            "--save-root", self.directory / "crowd-saves", "--seed", "7", "--distance", "16", "--frame-stats", "2",
        ], executable=executable)
        report = self.frame(window, lambda frame: frame.get("type") == "3d_frame_stats", seconds=240)
        self.assertEqual(window.child.wait(timeout=40), 0)
        self.assertEqual(report["creatures"], 25, "the character and the crowd's 24 creatures")
        self.assertEqual(report["posed_rigs"], 25)
        self.assertEqual(report["frustum_creatures"], {
            "min": 25, "max": 25, "sampled_frames": report["frames"], "unavailable_frames": 0,
        })
        self.assertGreater(report["frames"], 0)
        self.assertLessEqual(report["p50_ms"], report["p95_ms"])
        self.assertLessEqual(report["p95_ms"], report["p99_ms"])
        self.assertLessEqual(report["p99_ms"], report["max_ms"])
        self.assertAlmostEqual(report["fps"], 1000.0 / report["mean_ms"], places=3)
        self.assertEqual(set(report["motion_ms_per_frame"]), {"pose", "robes", "drapes", "hair"})

    @model_generation.local
    def test_frame_stats_measure_one_hundred_visible_creatures(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--server-bin", self.bin / ("tor-server" + self.suffix),
            "--scenario", CLIENT / "scenarios/crowd-hundred",
            "--save-root", self.directory / "hundred-crowd-saves", "--seed", "7",
            "--distance", "34", "--frame-stats", "2",
        ], executable=executable)
        started = self.frame(window, lambda frame: frame.get("type") == "local_server", seconds=90)
        self.own_private_backend_cleanup(started["pid"])
        report = self.frame(window, lambda frame: frame.get("type") == "3d_frame_stats", seconds=480)
        self.assertEqual(window.child.wait(timeout=40), 0)
        self.assertEqual(report["creatures"], 101)
        self.assertEqual(report["posed_rigs"], 101)
        self.assertGreater(report["frames"], 0)
        self.assertEqual(report["frustum_creatures"], {
            "min": 101, "max": 101, "sampled_frames": report["frames"], "unavailable_frames": 0,
        })

    @model_generation.local
    def test_scripted_action_boundaries_do_not_consume_a_step(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        script = {
            "version": 1, "hz": 50, "viewport": [640, 480],
            "actors": [{"id": "actor", "model": "Human", "seed": 7,
                        "body": {"woman": False}, "outfit": "ranger", "equipment": [],
                        "position": [0, 0, 0], "yaw": 0}],
            "cameras": [{"id": "front", "position": [0, 1, 3], "target": [0, 0.5, 0], "fov_degrees": 45},
                        {"id": "side", "position": [3, 1, 0], "target": [0, 0.5, 0], "fov_degrees": 45}],
            "events": [{"tick": tick, "actor": "actor", "operation": {"type": "action", "name": name}}
                       for tick, name in [(10, "jump"), (12, "cancel"), (13, "jump")]],
            "captures": [{"tick": tick, "name": name, "camera": camera}
                         for tick, name, camera in [(10, "start-front", "front"), (10, "start-side", "side"),
                                                    (11, "advanced", "front"), (12, "cancelled", "front"),
                                                    (13, "restarted", "front")]],
        }
        source = self.directory / "action-boundaries.json"
        source.write_text(json.dumps(script), encoding="utf-8")
        output = self.directory / "action-boundaries"
        window = self.launch("tor-client-3d", ["--scene-script", source, "--capture", output], executable=executable)
        self.assertEqual(window.child.wait(timeout=240), 0,
                         window.stdout_path.read_text(encoding="utf-8")[-3000:])
        report = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
        self.assertEqual(report["status"], "complete")
        shots = report["captures"]
        self.assertEqual([s["tick"] for s in shots], [10, 10, 11, 12, 13])
        self.assertEqual(shots[0]["actors"], shots[1]["actors"], "capture waits cannot re-pose or age actors")
        for index, phase in [(0, 0.0), (2, 0.02 / 1.15), (4, 0.0)]:
            with self.subTest(tick=shots[index]["tick"]):
                self.assertEqual(shots[index]["actors"][0]["action"]["name"], "Jump")
                self.assertAlmostEqual(shots[index]["actors"][0]["action"]["phase"], phase, places=6)
        self.assertIsNone(shots[3]["actors"][0]["action"])

    @model_generation.local
    def test_scripted_jump_hands_do_not_snap_at_flight_boundaries(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        ticks = [16, 17, 18, 19, 39, 40, 41, 42]
        script = {
            "version": 1, "hz": 50, "viewport": [640, 480],
            "actors": [{"id": "jumper", "model": "Human", "seed": 7,
                        "body": {"woman": False}, "outfit": "knight",
                        "equipment": [], "position": [0, 0, 0], "yaw": 0}],
            "cameras": [{"id": "front", "position": [0, 1.2, 3],
                         "target": [0, 0.5, 0], "fov_degrees": 40}],
            "events": [{"tick": 1, "actor": "jumper",
                        "operation": {"type": "action", "name": "jump"}}],
            "captures": [{"tick": tick, "name": f"jump-{tick}", "camera": "front"}
                         for tick in ticks],
        }
        source = self.directory / "jump-boundaries.json"
        source.write_text(json.dumps(script), encoding="utf-8")
        output = self.directory / "jump-boundaries"
        window = self.launch("tor-client-3d", ["--scene-script", source, "--capture", output],
                             executable=executable, extra_env={"RUST_LOG": "info,wgpu_hal=warn"})
        self.assertEqual(window.child.wait(timeout=240), 0,
                         window.stdout_path.read_text(encoding="utf-8")[-3000:])
        manifest = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
        self.assertEqual(manifest["status"], "complete")
        shots = manifest["captures"]
        self.assertEqual([s["tick"] for s in shots], ticks)
        for first, second in zip(shots, shots[1:]):
            if second["tick"] != first["tick"] + 1:
                continue
            for hand in ["HandL", "HandR"]:
                a = first["actors"][0]["joints"][hand][12:15]
                b = second["actors"][0]["joints"][hand][12:15]
                distance = sum((x - y) ** 2 for x, y in zip(a, b)) ** 0.5
                self.assertLess(distance, 0.1,
                                f"{hand} snaps {distance} at tick {second['tick']}")
        for shot in shots:
            image = (output / (shot["name"] + ".png")).read_bytes()
            self.assertEqual(image[:8], b"\x89PNG\r\n\x1a\n")
            self.assertGreater(len(image), 4096)

    @model_generation.local
    def test_live_playlist_loops_in_one_window_with_timing_and_no_capture_holds(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        scene = {
            "version": 1, "hz": 60, "viewport": [640, 480],
            "actors": [{"id": "actor", "model": "Human", "seed": 7,
                        "body": {"woman": False}, "outfit": "ranger", "equipment": [],
                        "position": [0, 0, 0], "yaw": 0}],
            "cameras": [{"id": "front", "position": [0, 1, 3], "target": [0, 0.5, 0], "fov_degrees": 45},
                        {"id": "side", "position": [2, 1, 2], "target": [0, 0.5, 0], "fov_degrees": 45}],
            "events": [{"tick": 1, "actor": "actor", "operation": {"type": "velocity", "value": [0.2, 0, 0]}},
                       {"tick": 12, "actor": "actor", "operation": {"type": "dress", "outfit": "knight"}},
                       {"tick": 24, "actor": "actor", "operation": {"type": "velocity", "value": [0, 0, 0]}}],
            # Live chapters must not need a dummy screenshot to bound events.
        }
        (self.directory / "scene.json").write_text(json.dumps(scene), encoding="utf-8")
        chapter = {"script": "scene.json", "shots": [
            {"camera": "front", "hold_seconds": 0.3, "transition_seconds": 0.2},
            {"camera": "side", "hold_seconds": 0.3, "transition_seconds": 0.2}]}
        source = self.directory / "live.json"
        source.write_text(json.dumps({"version": 1, "scenes": [chapter, chapter]}), encoding="utf-8")
        report = self.directory / "live-report.json"
        proof = self.directory / "live.png"
        window = self.launch("tor-client-3d", ["--live-playlist", source, "--cycles", "2",
                             "--diagnostics", report, "--proof-frame", proof], executable=executable)
        self.assertEqual(window.child.wait(timeout=240), 0, window.stdout_path.read_text(encoding="utf-8")[-3000:])
        data = json.loads(report.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "complete")
        self.assertEqual(data["completed_cycles"], 2)
        self.assertEqual(data["windows_seen"], 1)
        self.assertEqual(data["model_builds"], 4, "preload each composition once, never rebuild on loop")
        self.assertEqual([(e["cycle"], e["scene"]) for e in data["scene_entries"]], [(0, 0), (0, 1), (1, 0), (1, 1)])
        self.assertTrue(all(e["positions"] == [[0.0, 0.0, 0.0]] for e in data["scene_entries"]))
        self.assertGreater(data["frame_intervals"]["count"], 10)
        self.assertGreater(data["render_intervals"]["count"], 10)
        self.assertGreater(data["animation_steps"], 10)
        self.assertGreater(data["simulated_seconds"], 3.9)
        self.assertEqual(data["capture_holds"], 0)
        self.assertEqual(data["proof_frames"], 1)
        self.assertEqual(data["actors"][0]["outfit"], "knight")
        self.assertAlmostEqual(data["actors"][0]["position"][0], 0.2 * (0.4 - 1 / 60), places=5)
        self.assertEqual(proof.read_bytes()[:8], b"\x89PNG\r\n\x1a\n")

    def test_live_playlist_rejects_invalid_input_without_creating_diagnostics(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        fixture = CLIENT / "review-scripts/live-showcase.json"
        original = json.loads(fixture.read_text(encoding="utf-8"))
        for chapter in original["scenes"]:
            chapter["script"] = str(fixture.parent / chapter["script"])
        for case in ("version", "camera", "duration", "event_at_chapter_end", "empty", "unknown"):
            data = json.loads(json.dumps(original))
            if case == "version": data["version"] = 999
            elif case == "camera": data["scenes"][0]["shots"][0]["camera"] = "missing"
            elif case == "duration": data["scenes"][0]["shots"][0]["transition_seconds"] = 0
            elif case == "event_at_chapter_end":
                chapter = data["scenes"][0]
                scene = json.loads(Path(chapter["script"]).read_text(encoding="utf-8"))
                scene.pop("captures", None)
                duration = sum(shot["hold_seconds"] + shot["transition_seconds"] for shot in chapter["shots"])
                scene["events"] = [{"tick": round(duration * scene["hz"]),
                                    "actor": scene["actors"][0]["id"],
                                    "operation": {"type": "action", "name": "gesture"}}]
                scene_path = self.directory / "event-at-end-scene.json"
                scene_path.write_text(json.dumps(scene), encoding="utf-8")
                chapter["script"] = str(scene_path)
            elif case == "empty": data["scenes"] = []
            else: data["typo"] = True
            source = self.directory / (case + ".json")
            source.write_text(json.dumps(data), encoding="utf-8")
            report = self.directory / (case + "-report.json")
            process = self.launch("tor-client-3d", ["--live-playlist", source, "--diagnostics", report], executable=executable)
            self.assertNotEqual(process.child.wait(timeout=30), 0, case)
            self.assertFalse(report.exists(), case)
            if case == "event_at_chapter_end":
                self.assertIn("ends before its events", process.stdout_path.read_text(encoding="utf-8"))

    def test_scripted_review_rejects_case_colliding_capture_names_before_output(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        script = {
            "version": 1, "hz": 50, "viewport": [320, 320],
            "actors": [{"id": "actor", "model": "Human", "seed": 7,
                        "body": {"woman": True}, "outfit": "commoner",
                        "equipment": [], "position": [0, 0, 0], "yaw": 0}],
            "cameras": [{"id": "front", "position": [0, 1.2, 3],
                         "target": [0, 0.5, 0], "fov_degrees": 40}],
            "events": [],
            "captures": [{"tick": 0, "name": name, "camera": "front"}
                         for name in ["front", "FRONT"]],
        }
        source = self.directory / "case-collision.json"
        source.write_text(json.dumps(script), encoding="utf-8")
        output = self.directory / "collision-output"
        process = self.launch("tor-client-3d", ["--scene-script", source, "--capture", output], executable=executable)
        code = process.child.wait(timeout=60)
        self.assertFalse(output.exists(), "capture filename collision must fail before output creation")
        self.assertNotEqual(code, 0)

    def test_scripted_review_rejects_invalid_input_before_creating_output(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        source = self.directory / "invalid.json"
        source.write_text('{"version": 999, "unexpected": true}', encoding="utf-8")
        output = self.directory / "invalid-output"
        process = self.launch("tor-client-3d", ["--scene-script", source, "--capture", output], executable=executable)
        self.assertNotEqual(process.child.wait(timeout=30), 0)
        self.assertFalse(output.exists())

    def test_scripted_review_rejects_unsupported_compositions_before_output(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        for model, outfit, equipment, reason in [
            ("Human", "royal-princess", [], "outfit requires a female body"),
            ("Bat", "native", ["boots"], "cannot wear the requested composition"),
        ]:
            with self.subTest(model=model):
                script = {"version": 1, "hz": 50, "viewport": [640, 480],
                          "actors": [{"id": "subject", "model": model, "seed": 7,
                                      "body": {"woman": False}, "outfit": outfit,
                                      "equipment": equipment, "position": [0, 0, 0], "yaw": 0}],
                          "cameras": [{"id": "front", "position": [0, 1.2, 3],
                                       "target": [0, 0.5, 0], "fov_degrees": 40}],
                          "events": [], "captures": [{"tick": 0, "name": "shot", "camera": "front"}]}
                source = self.directory / (model + ".json")
                source.write_text(json.dumps(script), encoding="utf-8")
                output = self.directory / (model + "-output")
                process = self.launch("tor-client-3d", ["--scene-script", source, "--capture", output], executable=executable)
                self.assertNotEqual(process.child.wait(timeout=30), 0)
                self.assertFalse(output.exists())
                self.assertIn("subject: " + reason, process.stdout_path.read_text(encoding="utf-8"))

    @model_generation.local
    def test_princess_gown_presents_on_female_bodies_in_motion(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        captures = self.directory / "princess"
        captures.mkdir()
        families = ["Warrior", "orc", "nymph"]
        window = self.launch("tor-client-3d", [
            "--women", "--family", ",".join(families), "--outfit", "royal-princess",
            "--jump", "--capture-every", "0.5", "--capture", captures, "--capture-warmup", "2",
        ], executable=executable, extra_env={"RUST_LOG": "info"})
        self.assertEqual(window.child.wait(timeout=420), 0)
        for index, family in enumerate(families):
            motion = sorted(captures.glob(f"{index:02d}-motion-*-{family}.png"))
            self.assertGreaterEqual(len(motion), 4, family)
            self.assertGreater(len({hashlib.sha256(p.read_bytes()).digest() for p in motion}), 1)
            for view in ("line", "close-a", "close-b"):
                image = captures / f"{index:02d}-{view}-{family}.png"
                self.assertTrue(image.exists(), image.name)
                self.assertGreater(image.stat().st_size, 4096)
        log = window.stdout_path.read_text(encoding="utf-8")
        self.assertNotIn("panicked", log)
        for family in families:
            self.assertEqual(len(re.findall(rf"Lineup individual family={family} index=\d bust=", log)), 5)

    @model_generation.local
    def test_lineup_readback_holds_and_resumes_the_presentation_clock(self):
        self._check_review_clock()

    @model_generation.local
    def test_fixed_step_lineup_captures_on_the_selected_clock(self):
        self._check_review_clock(50)

    def _check_review_clock(self, hz=None):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        captures = self.directory / "capture-clock"
        captures.mkdir()
        window = self.launch("tor-client-3d", [
            "--women", "--family", "Warrior", "--outfit", "leotard",
            "--jump", "--capture-every", "0.5", "--capture", captures,
            "--capture-warmup", "2",
        ] + ([] if hz is None else ["--review-step-hz", str(hz)]), executable=executable, extra_env={"RUST_LOG": "info"})
        self.assertEqual(window.child.wait(timeout=180), 0)
        log = window.stdout_path.read_text(encoding="utf-8")
        events = re.findall(r"Review capture clock (held|resumed) at (\d+) ns", log)
        self.assertGreaterEqual(len(events), 4, log[-2000:])
        if hz is not None:
            for _, timestamp in events:
                self.assertEqual(int(timestamp) % (1_000_000_000 // hz), 0)
        pending = None
        completed = 0
        for kind, tick in events:
            if kind == "held":
                self.assertIsNone(pending)
                pending = tick
            else:
                self.assertEqual(tick, pending, "presentation time changed during GPU readback")
                pending = None
                completed += 1
        self.assertGreaterEqual(completed, 2)
        for view in ("line", "close-a", "close-b"):
            image = captures / f"00-{view}-Warrior.png"
            self.assertEqual(image.read_bytes()[:8], b"\x89PNG\r\n\x1a\n")
            self.assertGreater(image.stat().st_size, 4096)
        self.assertNotIn("panicked", log)

    @model_generation.local
    def test_sylvan_duelist_panels_present_on_women_in_motion(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        captures = self.directory / "sylvan-duelist"
        captures.mkdir()
        families = ["Warrior", "Mage", "orc"]
        window = self.launch("tor-client-3d", [
            "--women", "--family", ",".join(families), "--outfit", "sylvan-duelist",
            "--jump", "--capture-every", "0.5", "--capture", captures, "--capture-warmup", "2",
        ], executable=executable, extra_env={"RUST_LOG": "info"})
        self.assertEqual(window.child.wait(timeout=420), 0)
        for index, family in enumerate(families):
            motion = sorted(captures.glob(f"{index:02d}-motion-*-{family}.png"))
            self.assertGreaterEqual(len(motion), 4, family)
            self.assertGreater(len({hashlib.sha256(p.read_bytes()).digest() for p in motion}), 1)
            for view in ("line", "close-a", "close-b"):
                image = captures / f"{index:02d}-{view}-{family}.png"
                self.assertEqual(image.read_bytes()[:8], b"\x89PNG\r\n\x1a\n")
                self.assertGreater(image.stat().st_size, 4096)
        self.assertNotIn("panicked", window.stdout_path.read_text(encoding="utf-8"))

    @model_generation.local
    def test_skinned_lineup_has_no_depth_prepass_speckles(self):
        from render_pixels import dark_fraction
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        captures = self.directory / "skin-depth"
        captures.mkdir()
        window = self.launch("tor-client-3d", [
            "--lineups", "--family", "Warrior", "--outfit", "ranger",
            "--capture", captures, "--capture-warmup", "2",
        ], executable=executable)
        self.assertEqual(window.child.wait(timeout=180), 0)
        # Undecorated skin inside the first Ranger's collar. FXC on the
        # review host left rejected fragments; DXC retains continuous skin.
        image = (captures / "00-close-a-Warrior.png").read_bytes()
        fraction = dark_fraction(image, (0.475, 0.54, 0.512, 0.59))
        self.assertLess(fraction, 0.002, f"dark holes in exposed skin: {fraction:.3%}")

    @model_generation.local
    def test_character_study_outfits_present_paired_bodies(self):
        """Every study recipe reaches real GPU frames on the mixed-sex lineup."""
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        studies = ["elven-ranger", "rogue", "paladin", "barbarian", "valkyrie", "cleric",
                   "druid", "bard", "vampire-noble", "necromancer", "desert"]
        for outfit in studies:
            with self.subTest(outfit=outfit):
                captures = self.directory / ("study-" + outfit)
                captures.mkdir()
                animated = outfit in ("barbarian", "cleric", "druid", "bard", "necromancer")
                arguments = [
                    "--lineups", "--family", "Warrior", "--outfit", outfit,
                    "--capture", captures, "--capture-warmup", "2",
                ]
                if animated:
                    arguments += ["--jump", "--capture-every", "0.5"]
                window = self.launch("tor-client-3d", arguments, executable=executable)
                self.assertEqual(window.child.wait(timeout=180), 0, outfit)
                motion = sorted(captures.glob("00-motion-*-Warrior.png"))
                if animated:
                    self.assertGreaterEqual(len(motion), 4, outfit)
                    self.assertGreater(len({hashlib.sha256(image.read_bytes()).digest() for image in motion}), 1,
                                       f"{outfit}: timed motion must present changing frames")
                images = [captures / f"00-{view}-Warrior.png" for view in ("line", "close-a", "close-b")]
                for image in images + motion:
                    data = image.read_bytes()
                    self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
                    self.assertEqual(data[-12:], b"\x00\x00\x00\x00IEND\xaeB`\x82")
                    self.assertGreater(len(data), 4096, f"{outfit}: {image.name}")
                self.assertNotIn("panicked", window.stdout_path.read_text(encoding="utf-8"))

    @model_generation.local
    def test_wardrobe_dresses_lineups_and_redresses_them_live(self):
        """Humanoids of different kinds are dressed in one outfit from the
        wardrobe, then redressed while they move, reaching real GPU frames."""
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        captures = self.directory / "wardrobe-lineup"
        captures.mkdir(parents=True)
        families = ["Warrior", "blue sentinel", "zombie", "vampire", "orc", "nymph", "medusa", "phantom", "wraith"]
        window = self.launch("tor-client-3d", [
            "--lineups", "--family", ",".join(families), "--outfit", "ranger",
            "--wardrobe-cycle", "1.5", "--capture", captures, "--capture-warmup", "2",
        ], executable=executable)
        self.assertEqual(window.child.wait(timeout=240), 0, "line-ups must finish while being redressed")
        for index, name in enumerate(families):
            for view in ("line", "close-a", "close-b"):
                label = name.replace(" ", "_")
                image = captures / f"{index:02d}-{view}-{label}.png"
                self.assertTrue(image.exists(), f"missing {image.name}")
                data = image.read_bytes()
                self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
                self.assertGreater(len(data), 4096, image.name)
        self.assertEqual(len(list(captures.glob("*.png"))), len(families) * 3)
        log = window.stdout_path.read_text(encoding="utf-8")
        redressed = [line for line in log.splitlines() if line.startswith("Wardrobe redressed")]
        self.assertTrue(redressed, "the wardrobe must redress the line-ups while they run")
        # Nine four-second family studies advance outfits on presented time,
        # not on wall time spent regenerating models or waiting for readback.
        outfits = [line.rsplit(" as ", 1)[-1] for line in redressed]
        self.assertEqual(outfits[:6], ["Knight", "Courtier", "Leotard", "Sarong", "Tavern", "Queen"])
        self.assertLessEqual(len(outfits), 26, "paused capture time must not repeatedly rebuild outfits")
        self.assertTrue(any(" 0 humanoids" not in line for line in redressed), log[-2000:])
        self.assertNotIn("panicked", log)

    @model_generation.local
    def test_both_mages_redress_live_without_losing_their_lineups(self):
        """Both caster palettes accept shared outfits on the mixed-sex lineup."""
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        captures = self.directory / "mage-wardrobe"
        captures.mkdir(parents=True)
        families = ["Mage", "red arcanist"]
        window = self.launch("tor-client-3d", [
            "--lineups", "--family", ",".join(families), "--outfit", "ranger",
            "--wardrobe-cycle", "1.5", "--capture", captures, "--capture-warmup", "2",
        ], executable=executable)
        self.assertEqual(window.child.wait(timeout=240), 0)
        for index, name in enumerate(families):
            for view in ("line", "close-a", "close-b"):
                image = captures / f"{index:02d}-{view}-{name.replace(' ', '_')}.png"
                data = image.read_bytes()
                self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
                self.assertGreater(len(data), 4096, image.name)
        self.assertEqual(len(list(captures.glob("*.png"))), 6)
        log = window.stdout_path.read_text(encoding="utf-8")
        redressed = [line for line in log.splitlines() if line.startswith("Wardrobe redressed")]
        self.assertTrue(redressed, log[-2000:])
        self.assertTrue(all(" 5 humanoids" in line for line in redressed), redressed)
        self.assertEqual([line.rsplit(" as ", 1)[-1] for line in redressed][:2], ["Knight", "Courtier"])
        self.assertNotIn("panicked", log)

    @model_generation.local
    def test_equipment_overlays_present_and_survive_live_wardrobe_changes(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        for outfit, cycle in [("royal-princess", False), ("native", True)]:
            with self.subTest(outfit=outfit):
                captures = self.directory / ("equipment-" + outfit)
                captures.mkdir(parents=True)
                args = ["--lineups", "--family", "Warrior,Mage,wraith", "--outfit", outfit,
                        "--wear", "helm", "--wear", "boots", "--wear", "cloak", "--wear", "breastplate",
                        "--capture", captures, "--capture-warmup", "2"]
                if cycle:
                    args += ["--wardrobe-cycle", "1.5"]
                window = self.launch("tor-client-3d", args, executable=executable)
                self.assertEqual(window.child.wait(timeout=240), 0)
                for index, family in enumerate(["Warrior", "Mage", "wraith"]):
                    for view in ("line", "close-a", "close-b"):
                        image = captures / f"{index:02d}-{view}-{family}.png"
                        data = image.read_bytes()
                        self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
                        self.assertGreater(len(data), 4096, image.name)
                log = window.stdout_path.read_text(encoding="utf-8")
                equipped = [line for line in log.splitlines() if line.startswith("Equipped wardrobe")]
                self.assertGreaterEqual(len(equipped), 10)
                for line in equipped:
                    for item in ["SteelHelm", "LeatherBoots", "TravelCloak", "SteelBreastplate"]:
                        self.assertIn(item, line)
                if cycle:
                    self.assertTrue(any("wardrobe Commoner:" in line for line in equipped))
                    self.assertTrue(any("wardrobe Ranger:" in line for line in equipped))
                    self.assertNotIn("Wardrobe redressed 0 humanoids", log)
                self.assertNotIn("panicked", log)

    @model_generation.local
    def test_held_sword_presents_in_native_bestiary(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        captures = self.directory / "held-sword"
        captures.mkdir()
        # Standard bestiary warriors carry swords; five-person --lineups
        # intentionally omit them and cannot witness a held-item attachment.
        window = self.launch("tor-client-3d", [
            "--bestiary", "--outfit", "ranger",
            "--capture", captures,
            "--capture-every", "1", "--capture-warmup", "2", "--quit-after", "6",
        ], executable=executable)
        self.assertEqual(window.child.wait(timeout=180), 0)
        images = sorted(captures.glob("*.png"))
        self.assertGreaterEqual(len(images), 3)
        for image in images:
            data = image.read_bytes()
            self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
            self.assertGreater(len(data), 4096)
        self.assertGreater(len({hashlib.sha256(p.read_bytes()).digest() for p in images}), 1)
        log = window.stdout_path.read_text(encoding="utf-8")
        self.assertNotIn("panicked", log)

    def test_equipment_preview_rejects_live_and_unknown_selections(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        cases = [
            (["--connect", "127.0.0.1:1", "--wear", "helm"], "live equipment is server-owned"),
            (["--connect", "127.0.0.1:1", "--lineups", "--wear", "boots"], "live equipment is server-owned"),
            (["--lineups", "--wear", "unknown"], "unknown equipment"),
        ]
        for index, (args, expected) in enumerate(cases):
            with self.subTest(args=args):
                log = self.directory / f"equipment-rejected-{index}.log"
                with log.open("w", encoding="utf-8") as output:
                    result = subprocess.run([executable, *args], stdout=output, stderr=subprocess.STDOUT, timeout=10)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(expected, log.read_text(encoding="utf-8"))

    def own_private_backend_cleanup(self, pid):
        """Retain an OS process handle so failed tests cannot leak a backend
        or accidentally terminate a later process that reuses its PID.
        """
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes
            kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
            kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
            kernel32.OpenProcess.restype = wintypes.HANDLE
            kernel32.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
            kernel32.TerminateProcess.argtypes = [wintypes.HANDLE, wintypes.UINT]
            kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
            handle = kernel32.OpenProcess(0x100001, False, pid)
            self.assertTrue(handle, f"cannot retain private server process: {ctypes.get_last_error()}")
            def cleanup():
                try:
                    if kernel32.WaitForSingleObject(handle, 0) == 258:
                        if not kernel32.TerminateProcess(handle, 1):
                            self.assertEqual(kernel32.WaitForSingleObject(handle, 0), 0)
                        self.assertEqual(kernel32.WaitForSingleObject(handle, 10_000), 0)
                finally:
                    kernel32.CloseHandle(handle)
            self.addCleanup(cleanup)
        else:
            import signal
            handle = os.pidfd_open(pid)
            def cleanup():
                import select
                try:
                    if not select.select([handle], [], [], 0)[0]:
                        try:
                            signal.pidfd_send_signal(handle, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                        self.assertTrue(select.select([handle], [], [], 10)[0])
                finally:
                    os.close(handle)
            self.addCleanup(cleanup)

    def tap(self, window, native, name, done):
        """Type one key once, then wait for the frame showing its effect.

        X delivers a whole keystroke, so nothing is held across slow frames
        for it to auto-repeat. A posted Windows key-down must still be down
        when the client samples input, so it is held only until the client
        reports it pressed."""
        if getattr(native, "stroke", None):
            native.stroke(name)
            return self.captured(window, lambda frame: not frame["busy"] and done(frame))
        code = ("Key" + name.upper() if name.isalpha() and len(name) == 1 else
                "Digit" + name if name.isdigit() else {"Return": "Enter", "space": "Space", "Up": "ArrowUp",
                                                       "Down": "ArrowDown", "Left": "ArrowLeft", "Right": "ArrowRight",
                                                       "bracketleft": "BracketLeft", "bracketright": "BracketRight"}.get(name, name))
        native(name, True)
        try:
            self.captured(window, lambda frame: code in frame["keys"])
        finally:
            native(name, False)
        return self.captured(window, lambda frame: code not in frame["keys"] and not frame["busy"] and done(frame))

    def native_keys(self, client, *, expected_title="Thresholds of Ruin — 3D prototype"):
        handle = self._native_window(client, expected_title=expected_title)
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes
            user32 = ctypes.WinDLL("user32", use_last_error=True)
            user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
            user32.MapVirtualKeyW.argtypes = [wintypes.UINT, wintypes.UINT]
            def key(name, down):
                vk = ord(name.upper()) if len(name) == 1 and name.isascii() and name.isalnum() else {
                    "Escape": 0x1B, "space": 0x20, "Return": 0x0D, "Tab": 0x09, "Control_L": 0x11,
                    "Up": 0x26, "Down": 0x28, "Left": 0x25, "Right": 0x27,
                    "PageUp": 0x21, "PageDown": 0x22, "Home": 0x24, "End": 0x23, "F2": 0x71, "F4": 0x73, "F5": 0x74, "F6": 0x75, "F7": 0x76, "F10": 0x79,
                    "bracketleft": 0xDB, "bracketright": 0xDD,
                }[name]
                scan = user32.MapVirtualKeyW(vk, 0)
                extended = name in ("Up", "Down", "Left", "Right", "PageUp", "PageDown", "Home", "End")
                self.assertTrue(user32.PostMessageW(handle, 0x100 if down else 0x101, vk,
                    1 | (scan << 16) | (0x01000000 if extended else 0) | (0 if down else 0xC0000000)))
            return key
        def key(name, down):
            # Winit consumes XInput events. Use XTEST on the focused game
            # window instead of synthetic core events sent with --window.
            subprocess.run(["xdotool", "windowfocus", "--sync", handle], check=True, timeout=10)
            subprocess.run(["xdotool", "keydown" if down else "keyup", name], check=True, timeout=10)
        def stroke(name):
            subprocess.run(["xdotool", "windowfocus", "--sync", handle], check=True, timeout=10)
            subprocess.run(["xdotool", "key", name], check=True, timeout=10)
        key.stroke = stroke
        return key

    def test_private_launches_keep_prior_saves_and_stop_their_own_server(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        saves = self.directory / "private-saves"
        prior = None
        for session in range(2):
            window = self.launch("tor-client-3d", [
                "--server-bin", self.bin / ("tor-server" + self.suffix), "--scenario", ROOT / "scenarios/two-room",
                "--save-root", saves, "--seed", "42", "--capture", self.directory / f"private-{session}",
                "--capture-every", "0.4", "--quit-after", "120",
            ], executable=executable)
            started = self.frame(window, lambda frame: frame.get("type") == "local_server", seconds=90)
            self.assertNotEqual(started["pid"], window.child.pid)
            self.own_private_backend_cleanup(started["pid"])
            self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
            native = self.native_keys(window)
            try:
                native("Escape", True)
                self.assertEqual(window.child.wait(timeout=40), 0)
            finally:
                if window.child.poll() is None:
                    native("Escape", False)
            host, port = started["address"].rsplit(":", 1)
            with self.assertRaises(OSError, msg="the private backend must stop with its owning window"):
                with socket.create_connection((host, int(port)), timeout=2):
                    pass
            save = Path(started["save"])
            self.assertTrue(save.is_file())
            if prior:
                path, contents = prior
                self.assertNotEqual(save, path)
                self.assertEqual(path.read_bytes(), contents, "a later launch changed a prior save")
            prior = (save, save.read_bytes())

    @non_graphical
    def test_missing_connection_credential_is_rejected_before_opening_the_splash(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        environment = dict(os.environ)
        environment.pop("TOR_SERVER_TOKEN", None)
        captures = self.directory / "invalid-connection-captures"
        log = self.directory / "invalid-connection.log"
        with log.open("w", encoding="utf-8") as output:
            result = subprocess.run([executable, "--connect", "127.0.0.1:1", "--capture", captures],
                                    env=environment, stdout=output, stderr=subprocess.STDOUT, timeout=30)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(captures.exists(), "invalid credentials must be rejected before splash presentation or capture side effects")
        self.assertIn("TOR_SERVER_TOKEN", log.read_text(encoding="utf-8"))

    @non_graphical
    def test_live_appearance_override_is_rejected_before_connecting(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        log = self.directory / "appearance-option.log"
        with log.open("w", encoding="utf-8") as output:
            result = subprocess.run([executable, "--connect", "127.0.0.1:1", "--appearance", "woman"],
                                    stdout=output, stderr=subprocess.STDOUT, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("live appearance is server-owned", log.read_text(encoding="utf-8"))

    def test_dark_awareness_has_no_decorative_lights_and_a_dimmer_presented_scene(self):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        luminance = {}
        for lit in (True, False):
            mode = "lit" if lit else "dark"
            package = self.directory / f"lighting-{mode}"
            shutil.copytree(CLIENT / "scenarios/menagerie", package)
            region = package / "regions/1.toml"
            region.write_text(f'lit = {str(lit).lower()}\n' + region.read_text(encoding="utf-8"),
                              encoding="utf-8", newline="\n")
            log = self.directory / f"lighting-{mode}-validation.log"
            with log.open("w", encoding="utf-8") as output:
                result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                        stdout=output, stderr=subprocess.STDOUT, timeout=90)
            self.assertEqual(result.returncode, 0, log.read_text(encoding="utf-8")[-2500:])
            self.save = self.directory / f"lighting-{mode}.db"
            server = self.server(scenario=package)
            window = self.launch("tor-client-3d", [
                "--connect", self.address, "--capture", self.directory / f"captures-{mode}",
                "--capture-every", "0.4", "--quit-after", "120",
            ], executable=executable)
            frame = self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
            self.assertGreater(frame["visible_cells"], 0, "dark body awareness still discloses geometry")
            if lit:
                self.assertGreater(frame["lit_cells"], 0)
                self.assertGreater(frame["active_point_lights"], 0, "lit positive control must retain artistic lights")
            else:
                self.assertEqual(frame["lit_cells"], 0)
                self.assertEqual(frame["active_point_lights"], 0, "darkness must suppress every decorative point light")
            self.assertIsNotNone(frame["scene_luminance"], "brightness evidence must come from actual GPU pixels")
            luminance[lit] = frame["scene_luminance"]
            native = self.native_keys(window)
            try:
                native("Escape", True)
                self.assertEqual(window.child.wait(timeout=40), 0)
            finally:
                if window.child.poll() is None:
                    native("Escape", False)
            server.stop(save=False)
        self.assertGreater(luminance[True], luminance[False] * 1.25, luminance)

    def _native_window(self, client, *, expected_title="Thresholds of Ruin \u2014 3D prototype"):
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes
            user32 = ctypes.WinDLL("user32", use_last_error=True)
            callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
            user32.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
            user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
            user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
            user32.IsWindowVisible.argtypes = [wintypes.HWND]
            handles = []
            @callback_type
            def find(hwnd, _):
                pid = wintypes.DWORD()
                user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
                if pid.value == client.child.pid and user32.IsWindowVisible(hwnd):
                    title = ctypes.create_unicode_buffer(256)
                    user32.GetWindowTextW(hwnd, title, len(title))
                    # Winit may also own a visible IME helper; input belongs to
                    # the unique game window, identified by both PID and title.
                    if title.value == expected_title:
                        handles.append(hwnd)
                return True
            user32.EnumWindows(find, 0)
            self.assertEqual(len(handles), 1, "native input needs the client's unique game window")
            return handles[0]
        windows = subprocess.check_output([
            "xdotool", "search", "--onlyvisible", "--pid", str(client.child.pid),
        ], text=True, timeout=10).split()
        self.assertEqual(len(windows), 1, "native input needs the client's own window")
        return windows[0]

    def fallback_package(self):
        package = self.directory / "fallback-models"
        shutil.copytree(CLIENT / "scenarios/menagerie", package)
        for relative in ("scenario.toml", "catalog/actors.toml"):
            path = package / relative
            text = path.read_text(encoding="utf-8")
            text = text.replace('"creature.bird.emu"', '"creature.unmodelled"')
            text = text.replace('"creature.equine.quagga"', '"creature.equine.authored"')
            path.write_text(text, encoding="utf-8", newline="\n")
        path = package / "catalog/items.toml"
        with path.open("a", encoding="utf-8", newline="\n") as output:
            output.write('\n[items."rogue.unknown_object"]\nname = "unfamiliar relic"\nclass = "misc"\n')
        path = package / "regions/1.toml"
        path.write_text(path.read_text(encoding="utf-8").replace(
            "items = [", 'items = [\n  { id = 104, at = [2,4,0], archetype = "rogue.unknown_object" },',
        ), encoding="utf-8", newline="\n")
        log = self.directory / "fallback-validation.log"
        with log.open("w", encoding="utf-8") as output:
            result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                    cwd=ROOT, stdout=output, stderr=subprocess.STDOUT, timeout=90)
        self.assertEqual(result.returncode, 0, log.read_text(encoding="utf-8")[-2500:])
        return package

    def captured(self, client, predicate=lambda frame: True):
        frame = self.frame(client, lambda frame: frame.get("type") == "3d_frame" and predicate(frame), seconds=90)
        self.assertTrue(frame["gpu_readback"])
        self.assertEqual(frame["captures_in_flight"], 1, "GPU capture requests must apply readback backpressure")
        self.assertFalse(frame["fatal"])
        self.assertIsNotNone(frame["renderer"], "GPU evidence must identify its actual renderer")
        if os.name == "nt" and "WGPU_BACKEND" not in os.environ:
            self.assertEqual(frame["renderer"]["backend"], "Dx12", "Windows must use the tested default backend")
        image = Path(frame["png"]).read_bytes()
        self.assertEqual(image[:8], b"\x89PNG\r\n\x1a\n")
        width, height = struct.unpack(">II", image[16:24])
        self.assertGreaterEqual(width, 640)
        self.assertGreaterEqual(height, 480)
        self.assertGreater(len(image), 4096, "capture must contain a rendered scene")
        render_errors = [line for line in client.stdout_path.read_text(encoding="utf-8").splitlines()
                         if "ERROR" in line and any(word in line.lower() for word in ("shader", "pipeline", "wgpu"))]
        self.assertFalse(render_errors, "\n".join(render_errors)[-2500:])
        return frame

    def test_unknown_models_render_and_the_item_remains_usable_with_native_input(self):
        package = self.fallback_package()
        server = self.server(scenario=package)
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--connect", self.address, "--capture", self.directory / "captures", "--capture-every", "0.4",
            "--quit-after", "600",
        ], executable=executable)
        initial = self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        self.assertGreater(initial["visible_cells"], 0)
        self.assertTrue(any(actor["name"] == "emu" and actor["model"] == "Placeholder" for actor in initial["actors"]))
        equine = next(actor for actor in initial["actors"] if actor["name"] == "quagga")
        self.assertEqual(equine["model"], "Equine")
        self.assertRegex(equine["appearance_seed"], r"^p_[0-9a-f]{64}$")
        self.assertIn(":Equine:", equine["appearance_key"])
        self.assertTrue(any(item["name"] == "unfamiliar relic" and item["model"] == "Placeholder"
                            for item in initial["ground_items"]))
        native = self.native_keys(window)
        try:
            native("g", True)
            picked = self.captured(window, lambda frame: "KeyG" in frame["keys"] and any(
                item["name"] == "unfamiliar relic" for item in frame["inventory"]))
        finally:
            native("g", False)
        self.assertTrue(any(item["name"] == "unfamiliar relic" and item["model"] == "Placeholder"
                            for item in picked["inventory"]))
        self.captured(window, lambda frame: "KeyG" not in frame["keys"])
        _, observed = self.client(token=SPECTATOR_TOKEN, observe=True)
        source = next(actor for actor in observed["state"]["observation"]["visible_actors"] if actor["name"] == "quagga")
        self.assertEqual(equine["appearance_seed"], source["appearance_seed"], "the rendered rig must use backend cosmetic identity")
        self.assertTrue(any(item["name"] == "unfamiliar relic" for item in observed["state"]["observation"]["inventory"]))
        try:
            native("Escape", True)
            self.assertEqual(window.child.wait(timeout=30), 0)
        finally:
            if window.child.poll() is None:
                native("Escape", False)
        server.stop(save=False)
        self.server(scenario=package)
        _, restored = self.client(token=SPECTATOR_TOKEN, observe=True)
        source = next(actor for actor in restored["state"]["observation"]["visible_actors"] if actor["name"] == "quagga")
        self.assertEqual(equine["appearance_seed"], source["appearance_seed"])
        self.assertTrue(any(item["name"] == "unfamiliar relic" for item in restored["state"]["observation"]["inventory"]),
                        "the native client's clean exit must persist its action without a helper save")

    def test_native_keys_read_identify_and_zap_magic_items(self):
        """The pack's number keys reach reading, identification and a
        directional zap, all answered by a real server."""
        self.server(scenario="magic-items")
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--connect", self.address, "--capture", self.directory / "captures", "--capture-every", "0.4",
            "--quit-after", "600",
        ], executable=executable)
        frame = self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        native = self.native_keys(window)

        def tap(name, done):
            return self.tap(window, native, name, done)

        def action(frame, label):
            position = next(index for index, text in enumerate(frame["panel_actions"]) if text.endswith(label))
            self.assertLess(position, 10, "the action must have a number key")
            return str((position + 1) % 10)

        for _ in range(len(frame["ground_items"])):
            held = len(frame["inventory"])
            frame = tap("g", lambda frame, held=held: len(frame["inventory"]) > held)
        self.assertFalse(frame["ground_items"], "every supply is picked up with G")
        frame = tap("i", lambda frame: frame["panel_open"] and any(text.endswith(": Read") for text in frame["panel_actions"]))
        frame = tap(action(frame, ": Read"), lambda frame: any(
            text.endswith(": Identify") for text in frame["panel_actions"]))
        self.assertTrue(frame["panel_open"], "the identification choice opens the pack")
        frame = tap(action(frame, ": Identify"), lambda frame: any(
            "embers" in item["name"] for item in frame["inventory"]))
        frame = tap(action(frame, "embers: Zap"), lambda frame: frame["aiming"] is not None)
        self.assertFalse(frame["panel_open"], "aiming hands the direction to the map")
        tap("d", lambda frame: frame["aiming"] is None)
        _, observed = self.client(token=SPECTATOR_TOKEN, observe=True)
        view = observed["state"]["observation"]
        wands = [row for row in view["inventory"] if row["class"] == "wand"]
        self.assertTrue(all(row["identified"] for row in wands))
        _, owner = self.client(observe=True)
        charges = sorted(row["charges"] for row in owner["state"]["observation"]["interactions"]["inventory"]
                         if row["charges"] is not None)
        self.assertEqual(charges, [2], "the zap spent one charge of the identified wand east")
        native("Escape", True)
        try:
            self.assertEqual(window.child.wait(timeout=30), 0)
        finally:
            if window.child.poll() is None:
                native("Escape", False)

    def test_map_destination_selection_is_local_and_enter_travels_to_the_exact_cell(self):
        self.server()
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", ["--connect", self.address, "--capture", self.directory / "map-route", "--capture-every", "0.4", "--quit-after", "600"], executable=executable)
        initial = self.captured(window, lambda frame: frame["has_control"] and frame["can_act"] and not frame["busy"])
        native = self.native_keys(window)
        def route(frame):
            return (frame.get("map_journal") or {}).get("route") or {}
        opened = self.tap(window, native, "m", lambda frame: str(frame["screen"]).startswith("Places") and route(frame).get("options"))
        self.assertIsNone(route(opened)["selected"])
        feet = route(opened)["feet"]
        options = route(opened)["options"]
        adjacent = {row["key"] for row in options if row["label"] == "Known floor" and max(abs(row["position"][axis] - feet[axis]) for axis in (0, 1)) == 1}
        self.assertTrue(adjacent, "positive fixture must contain an adjacent disclosed floor")
        frame = opened
        for _ in range(len(options)):
            previous = route(frame)["selected"]
            frame = self.tap(window, native, "Down", lambda current, previous=previous: route(current)["selected"] != previous)
            if route(frame)["selected"] in adjacent:
                break
        target = route(frame)["selected"]
        self.assertIn(target, adjacent)
        self.assertTrue(route(frame)["can_travel"])
        self.assertEqual(frame["revision"], initial["revision"], "selection must not advance the world")
        self.assertEqual(frame["world_feet"], initial["world_feet"])
        row = next(row for row in options if row["key"] == target)
        from render_pixels import decode_png
        width, height, channels, rows = decode_png(Path(frame["png"]).read_bytes())
        left, top, right, bottom = frame["map_journal"]["picture_bounds"]
        x = round((left + row["point"][0] * (right - left)) * width)
        y = round((top + row["point"][1] * (bottom - top)) * height)
        gold = 0
        for py in range(max(0, y - 12), min(height, y + 13)):
            for px in range(max(0, x - 12), min(width, x + 13)):
                red, green, blue = rows[py][px * channels:px * channels + 3]
                gold += red > 235 and green > 180 and 80 < blue < 160
        self.assertGreater(gold, 10, "the selected destination must have a real visible marker")
        arrived = self.tap(window, native, "Return", lambda current: current["screen"] is None and current["foot_cell"] == target and current["can_act"])
        self.assertGreater(arrived["revision"], initial["revision"])
        self.assertFalse(arrived["map_journal"]["visible"])
        self.assertNotEqual(arrived["world_feet"], initial["world_feet"])
        native("Escape", True)
        try:
            self.assertEqual(window.child.wait(timeout=30), 0)
        finally:
            if window.child.poll() is None:
                native("Escape", False)

    def test_map_journal_shares_the_disclosed_map_and_preserves_note_typing(self):
        self.server()
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", ["--connect", self.address, "--capture", self.directory / "map-journal", "--capture-every", "0.4", "--quit-after", "600"], executable=executable)
        initial = self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        native = self.native_keys(window)
        opened = self.tap(window, native, "m", lambda frame: frame["screen"].startswith("Places") and (frame.get("map_journal") or {}).get("visible"))
        left, top, right, bottom = opened["map_journal"]["picture_bounds"]
        self.assertGreater(right - left, 0.25)
        self.assertGreater(bottom - top, 0.3)
        from render_pixels import decode_png
        width, height, channels, rows = decode_png(Path(opened["png"]).read_bytes())
        colored = 0
        for y in range(int(top * height), int(bottom * height), 2):
            for x in range(int(left * width), int(right * width), 2):
                rgb = rows[y][x * channels:x * channels + 3]
                if max(rgb) > 50 and max(rgb) - min(rgb) > 15:
                    colored += 1
        self.assertGreater(colored, 100, "the enlarged map must contain actual disclosed terrain")
        history = self.tap(window, native, "F2", lambda frame: frame["screen"] == "History")
        self.assertTrue(history["map_journal"]["visible"])
        note = self.tap(window, native, "F4", lambda frame: frame["screen"].startswith("Note"))
        self.assertTrue(note["map_journal"]["visible"])
        typed = self.tap(window, native, "m", lambda frame: frame["typed"] == "m")
        self.assertTrue(typed["screen"].startswith("Note"), "M belongs to the note draft")
        self.assertEqual(typed["revision"], initial["revision"])
        self.assertEqual(typed["world_feet"], initial["world_feet"])
        self.tap(window, native, "Escape", lambda frame: frame["screen"] is None and not frame["map_journal"]["visible"])
        self.tap(window, native, "m", lambda frame: frame["screen"].startswith("Places"))
        self.tap(window, native, "m", lambda frame: frame["screen"] is None)
        native("Escape", True)
        try:
            self.assertEqual(window.child.wait(timeout=30), 0)
        finally:
            if window.child.poll() is None:
                native("Escape", False)

    def test_character_screen_shares_the_live_portrait_and_keeps_command_keys(self):
        self.server(scenario=self.fallback_package())
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", ["--connect", self.address, "--capture", self.directory / "character", "--capture-every", "0.4", "--quit-after", "600"], executable=executable)
        initial = self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        native = self.native_keys(window)
        self.tap(window, native, "F10", lambda frame: frame["screen"] == "Advancement" and not frame["busy"])
        portrait = self.captured(window, lambda frame: (frame.get("portrait") or {}).get("active") and not frame["portrait"]["pending"] and frame["portrait"]["mesh_parts"] > 0)
        self.assert_live_portrait(portrait, initial)
        pane = portrait["advancement"]["pane_bounds"]
        self.assertAlmostEqual(pane[2] - pane[0], 0.54, delta=0.01)
        self.assertLessEqual(portrait["portrait"]["bounds"][2], pane[0], "the live portrait and character controls must not overlap")
        turn = portrait["portrait"]["turn"]
        rotated = self.tap(window, native, "Home", lambda frame: frame["portrait"]["turn"] < turn - 0.1)
        self.assertEqual(rotated["revision"], portrait["revision"])
        self.assertEqual(rotated["world_feet"], portrait["world_feet"])
        self.tap(window, native, "End", lambda frame: abs(frame["portrait"]["turn"] - turn) < 0.01)
        typed = self.tap(window, native, "bracketleft", lambda frame: frame["typed"] == "[")
        self.assertAlmostEqual(typed["portrait"]["turn"], turn)
        held = self.tap(window, native, "Home", lambda frame: frame["typed"] == "[")
        self.assertEqual(held["typed"], "[")
        self.assertAlmostEqual(held["portrait"]["turn"], turn, msg="a command draft keeps navigation keys")
        self.assertEqual(held["revision"], initial["revision"])
        self.assertEqual(held["world_feet"], initial["world_feet"])
        self.tap(window, native, "Escape", lambda frame: frame["screen"] is None and not frame["portrait"]["active"])
        native("Escape", True)
        try:
            self.assertEqual(window.child.wait(timeout=30), 0)
        finally:
            if window.child.poll() is None:
                native("Escape", False)

    def test_native_advancement_screen_reads_the_owners_pages(self):
        """F10 opens the owner's advancement pages from the real server, and a
        typed command fetches another; Esc closes it without quitting."""
        self.server(scenario="advancement-owner")
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--connect", self.address, "--capture", self.directory / "captures", "--capture-every", "0.4",
            "--quit-after", "600",
        ], executable=executable)
        self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        native = self.native_keys(window)

        def tap(name, done):
            return self.tap(window, native, name, done)

        def lines(frame):
            return chr(10).join((frame["advancement"] or {}).get("lines", []))

        feet = self.captured(window)["world_feet"]
        frame = tap("F10", lambda frame: "Overview" in lines(frame))
        self.assertNotIn("Open overview to fetch", lines(frame))
        frame = tap("Tab", lambda frame: "Point balances" in lines(frame))
        self.assertEqual(frame["world_feet"], feet, "character navigation is free browsing")
        frame = tap("Tab", lambda frame: "SkillRank catalog" in lines(frame))
        self.assertEqual(frame["world_feet"], feet)
        for letter in "ledger":
            frame = tap(letter, lambda frame, letter=letter: frame["advancement"]["typed"].endswith(letter))
        self.assertEqual(frame["advancement"]["typed"], "ledger")
        frame = tap("Return", lambda frame: "Purchase ledger" in lines(frame) and frame["advancement"]["typed"] == "")
        self.assertEqual(frame["world_feet"], feet, "typing a, d, e and the like never moves the character")
        frame = tap("Escape", lambda frame: frame["advancement"] is None)
        self.assertIsNone(window.child.poll(), "Esc closes the screen, not the game")
        native("Escape", True)
        try:
            self.assertEqual(window.child.wait(timeout=30), 0)
        finally:
            if window.child.poll() is None:
                native("Escape", False)

    def test_graphical_advancement_selects_a_pool_previews_and_explicitly_confirms(self):
        """Ordinary keyboard selection previews a disclosed purchase; F7 alone commits it."""
        self.server(scenario="advancement-owner")
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--connect", self.address, "--capture", self.directory / "captures", "--capture-every", "0.4",
            "--quit-after", "600",
        ], executable=executable)
        self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        native = self.native_keys(window)

        def tap(name, done):
            return self.tap(window, native, name, done)

        def lines(frame):
            return chr(10).join((frame.get("advancement") or {}).get("lines", []))

        def choices(frame):
            return (frame.get("advancement") or {}).get("choices") or {}

        def type_line(line, done):
            prefix = ""
            for letter in line:
                prefix += letter
                tap(letter, lambda frame, prefix=prefix: frame["typed"] == prefix)
            return tap("Return", done)

        def select(frame, predicate):
            rows = choices(frame).get("rows", [])
            index = next((index for index, row in enumerate(rows) if predicate(row)), None)
            self.assertIsNotNone(index, f"expected disclosed choice among {rows}")
            for _ in range(len(rows) + 1):
                previous = choices(frame).get("selected")
                if previous == index:
                    return frame
                frame = tap("Down", lambda current, previous=previous: choices(current).get("selected") != previous)
            self.fail("keyboard selection must reach the disclosed row")

        frame = tap("F10", lambda frame: "Overview" in lines(frame))
        feet, revision = frame["world_feet"], int(frame["revision"])
        frame = tap("Tab", lambda frame: "Point balances" in lines(frame))
        self.assertTrue(choices(frame).get("rows"), "Point balances must present graphical funding choices")
        frame = select(frame, lambda row: row.get("pool") == "practice" and row.get("owner") == 0 and row.get("kind") == "SkillRank")
        frame = tap("Return", lambda frame: choices(frame).get("funding") == {"pool": "practice", "owner": 0} and "SkillRank catalog" in lines(frame))
        frame = type_line("supplies", lambda frame: "Ration:" in lines(frame) and frame["typed"] == "")
        self.assertTrue(choices(frame).get("supplies"), "typed supply chooser must replace the retained catalog display")
        self.assertFalse(choices(frame).get("rows"), "supply pages must not contain hidden catalog shortcuts")
        self.assertFalse(frame["panel_open"], "typing supplies stays in the character screen")
        frame = type_line("skills", lambda frame: "SkillRank catalog" in lines(frame) and frame["typed"] == "" and choices(frame).get("rows"))
        self.assertEqual(choices(frame).get("funding"), {"pool": "practice", "owner": 0})
        frame = select(frame, lambda row: row.get("selection") == {"kind": "skill_rank", "skill": "perception"})
        frame = tap("Return", lambda frame: choices(frame).get("can_confirm") and (choices(frame).get("proposal") or {}).get("kind") == "purchase")
        self.assertTrue(choices(frame).get("structured_preview"), "the default preview must show result cards")
        confirmation = choices(frame).get("confirm_button")
        self.assertIsNotNone(confirmation, "the preview must present a confirmation control")
        self.assertIn("Purchase HD 1 / pool practice / skill perception", confirmation["label"])
        bounds, clip = confirmation["bounds"], confirmation["clip"]
        self.assertGreater(bounds[2] - bounds[0], 100)
        self.assertGreater(bounds[3] - bounds[1], 20)
        for axis in (0, 1):
            self.assertGreaterEqual(bounds[axis] + 1, clip[axis], "confirmation starts inside the presented clip")
            self.assertLessEqual(bounds[axis + 2], clip[axis + 2] + 1, "confirmation stays fully visible outside the scrolling body")
        self.assertEqual(int(frame["revision"]), revision, "preview is free and leaves the build unchanged")
        self.assertEqual(frame["world_feet"], feet)
        frame = type_line("ledger", lambda frame: "Purchase ledger" in lines(frame) and frame["typed"] == "")
        self.assertNotIn("skill perception", lines(frame), "preview must not appear in the committed ledger")
        self.assertTrue(choices(frame).get("can_confirm"), "browsing keeps the exact retained preview")
        self.assertFalse(choices(frame).get("structured_preview"), "browsing must keep the requested ledger visible")
        self.assertIn("Purchase HD 1 / pool practice / skill perception", choices(frame)["confirm_button"]["label"], "confirmation must keep naming its exact proposal while browsing")
        frame = tap("F7", lambda frame: not frame["busy"] and int(frame["revision"]) > revision)
        self.assertEqual(frame["world_feet"], feet)
        self.assertFalse(choices(frame).get("can_confirm"), "changed state invalidates the confirmation")
        type_line("overview", lambda frame: "Overview" in lines(frame) and frame["typed"] == "")
        frame = type_line("ledger", lambda frame: "Purchase ledger" in lines(frame) and frame["typed"] == "")
        self.assertEqual(lines(frame).count("skill perception"), 1, "the exact purchase commits once")
        tap("Escape", lambda frame: frame["advancement"] is None)
        native("Escape", True)
        try:
            self.assertEqual(window.child.wait(timeout=30), 0)
        finally:
            if window.child.poll() is None:
                native("Escape", False)

    def test_native_screens_rename_a_place_save_a_note_and_read_both_in_history(self):
        """F5 renames a known place and F4 saves a note through the real
        server; F2 then shows both as the server recorded them, and C opens
        the charts. No screen's typing moves the character."""
        self.server()
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--connect", self.address, "--capture", self.directory / "captures", "--capture-every", "0.4",
            "--quit-after", "600",
        ], executable=executable)
        self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        native = self.native_keys(window)

        def tap(name, done):
            return self.tap(window, native, name, done)

        def text(frame):
            return chr(10).join(frame["screen_text"] or [])

        def type_line(line):
            for letter in line:
                frame = tap(letter, lambda frame, letter=letter: (frame["typed"] or "").endswith(letter))
            return frame

        feet = self.captured(window)["world_feet"]
        frame = tap("F5", lambda frame: (frame["screen"] or "").startswith("Places"))
        self.assertIn("1 ", text(frame), "the starting place is known")
        frame = tap("1", lambda frame: "New name for" in text(frame))
        type_line("den")
        frame = tap("Return", lambda frame: "den (your name for it)" in text(frame))
        frame = tap("Escape", lambda frame: frame["screen"] is None)

        frame = tap("F4", lambda frame: (frame["screen"] or "").startswith("Note"))
        self.assertIn("Audience: only you", text(frame))
        type_line("ok")
        frame = tap("Tab", lambda frame: "Audience: everyone" in text(frame))
        frame = tap("Return", lambda frame: frame["screen"] is None)

        frame = tap("F2", lambda frame: frame["screen"] == "History" and "Note: ok" in text(frame))
        self.assertIn("Place named den.", text(frame))
        frame = tap("F2", lambda frame: frame["screen"] is None)

        frame = tap("c", lambda frame: frame["screen"] == "Charts" and "Waiting for the charts." not in text(frame))
        frame = tap("c", lambda frame: frame["screen"] is None)
        self.assertEqual(frame["world_feet"], feet, "typing on the screens never moves the character")
        native("Escape", True)
        try:
            self.assertEqual(window.child.wait(timeout=30), 0)
        finally:
            if window.child.poll() is None:
                native("Escape", False)

    def test_fresh_window_hydrates_occluded_terrain_after_save_reload(self):
        server = self.server()
        player, _ = self.client()
        seen = self.play(player, [{"move": "east"}] * 3)
        observed = seen["state"]["observation"]
        tablet = next(i for i in observed["ground_items"] if i["item"]["name"] == "stone tablet")
        cell = next(c for c in observed["visible_cells"] if c["position"] == tablet["position"])
        closed = self.act(player, {"type": "set_door", "door": door(seen)["id"], "open": False})
        self.assertIsNone(closed["error"])
        hidden = self.act(player, {"type": "move", "direction": "west"})
        self.assertIsNone(hidden["error"])
        self.assertNotIn(cell["key"], {c["key"] for c in hidden["state"]["observation"]["visible_cells"]})
        self.flush_save()
        player.stop()
        server.stop(save=False)
        self.server()
        map_seen = threading.Event()
        def notice_map(message):
            if message.get("type") == "map_page":
                map_seen.set()
        # Observe the actual hydration reply without holding it behind GPU
        # startup: the shared protocol correctly expires map queries after
        # ten seconds, independently of how fast a native frame renders.
        relay = StreamRelay(self.address, on_server_message=notice_map)
        self.addCleanup(relay.close)
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--connect", relay.address, "--observe", "--capture", self.directory / "map-captures", "--capture-every", "0.4",
            "--quit-after", "120",
        ], executable=executable)
        frame = self.captured(window, lambda f: any(c["key"] == cell["key"] for c in f["remembered_terrain"]))
        self.assertTrue(map_seen.wait(5), "the server must send an actual hydration reply")
        restored = next(c for c in frame["remembered_terrain"] if c["key"] == cell["key"])
        self.assertFalse(restored["lit"], "remembered terrain must not claim current illumination")
        self.assertNotIn(cell["key"], frame["visible_keys"], "map hydration must not manufacture current sight")
        self.assertFalse(any(i["name"] == "stone tablet" for i in frame["ground_items"]),
                         "remembered items must not become live pickable entities")
        native = self.native_keys(window)
        try:
            native("Escape", True)
            self.assertEqual(window.child.wait(timeout=30), 0)
        finally:
            if window.child.poll() is None:
                native("Escape", False)

    def native_mouse(self, client):
        handle = self._native_window(client)
        if os.name == "nt":
            import ctypes
            import time
            from ctypes import wintypes
            user32 = ctypes.WinDLL("user32", use_last_error=True)
            user32.GetClientRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
            user32.ClientToScreen.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.POINT)]
            user32.SetForegroundWindow.argtypes = [wintypes.HWND]
            user32.SetWindowPos.argtypes = [wintypes.HWND, wintypes.HWND, ctypes.c_int, ctypes.c_int,
                                          ctypes.c_int, ctypes.c_int, wintypes.UINT]
            user32.WindowFromPoint.argtypes = [wintypes.POINT]
            user32.WindowFromPoint.restype = wintypes.HWND
            original = wintypes.POINT()
            self.assertTrue(user32.GetCursorPos(ctypes.byref(original)))
            self.addCleanup(lambda: user32.SetCursorPos(original.x, original.y))
            self.addCleanup(lambda: user32.mouse_event(0x0004, 0, 0, 0, 0))
            self.assertTrue(user32.SetWindowPos(handle, wintypes.HWND(-1), 0, 0, 0, 0, 0x0041))

            def mouse(point, down=None):
                self.assertTrue(all(0 < component < 1 for component in point))
                rect = wintypes.RECT()
                self.assertTrue(user32.GetClientRect(handle, ctypes.byref(rect)))
                screen = wintypes.POINT(round(point[0] * rect.right), round(point[1] * rect.bottom))
                self.assertTrue(user32.ClientToScreen(handle, ctypes.byref(screen)))
                self.assertTrue(user32.SetCursorPos(screen.x, screen.y),
                                f"SetCursorPos denied: {ctypes.get_last_error()}")
                actual = wintypes.POINT()
                self.assertTrue(user32.GetCursorPos(ctypes.byref(actual)))
                self.assertEqual((actual.x, actual.y), (screen.x, screen.y))
                for _ in range(20):
                    user32.SetForegroundWindow(handle)
                    if user32.WindowFromPoint(actual) == handle:
                        break
                    time.sleep(0.05)
                self.assertEqual(user32.WindowFromPoint(actual), handle, "mouse must hit this client's own window")
                if down is not None:
                    user32.mouse_event(0x0002 if down else 0x0004, 0, 0, 0, 0)
        else:
            self.addCleanup(lambda: subprocess.run(["xdotool", "mouseup", "1"], check=True, timeout=10))

            def mouse(point, down=None):
                self.assertTrue(all(0 < component < 1 for component in point))
                geometry = subprocess.check_output(["xdotool", "getwindowgeometry", "--shell", handle],
                                                   text=True, timeout=10)
                dimensions = dict(line.split("=", 1) for line in geometry.splitlines())
                screen = (int(dimensions["X"]) + round(point[0] * int(dimensions["WIDTH"])),
                          int(dimensions["Y"]) + round(point[1] * int(dimensions["HEIGHT"])))
                subprocess.run(["xdotool", "windowfocus", "--sync", handle], check=True, timeout=10)
                # Relative --sync compares against screen coordinates and can
                # wait forever on a stationary press/release. An absolute warp
                # followed by a same-connection query verifies actual input.
                readback = subprocess.check_output([
                    "xdotool", "mousemove", str(screen[0]), str(screen[1]), "getmouselocation", "--shell",
                ], text=True, timeout=10)
                actual = dict(line.split("=", 1) for line in readback.splitlines())
                self.assertEqual((int(actual["X"]), int(actual["Y"])), screen)
                self.assertEqual(int(actual["WINDOW"]), int(handle), "mouse must hit this client's own window")
                if down is not None:
                    subprocess.run(["xdotool", "mousedown" if down else "mouseup", "1"], check=True, timeout=10)

        return mouse

    def test_mouse_picks_the_presented_item_and_inventory_keys_do_not_exit_early(self):
        self.server(scenario=self.fallback_package())
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        # This route verifies Escape-driven exit. Per-step capture/input/exit
        # waits bound failure; an independent auto-close timer can interrupt
        # the route (or falsely satisfy its final exit assertion).
        window = self.launch("tor-client-3d", [
            "--connect", self.address, "--capture", self.directory / "mouse-captures", "--capture-every", "0.4",
        ], executable=executable)
        initial = self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        item = next(item for item in initial["ground_items"] if item["name"] == "unfamiliar relic")
        mouse = self.native_mouse(window)
        mouse(item["screen"])
        hovered = self.captured(window, lambda frame: frame["hover"]["title"] == "Unfamiliar relic")
        self.assertEqual(hovered["hover"]["cell"], item["position"])
        try:
            mouse(item["screen"], True)
            picked = self.captured(window, lambda frame: "Left" in frame["mouse"] and any(
                item["name"] == "unfamiliar relic" for item in frame["inventory"]))
        finally:
            mouse(item["screen"], False)
        self.assertFalse(any(item["name"] == "unfamiliar relic" for item in picked["ground_items"]))
        native = self.native_keys(window)
        try:
            native("i", True)
            self.captured(window, lambda frame: frame["panel_open"])
        finally:
            native("i", False)
        self.captured(window, lambda frame: "KeyI" not in frame["keys"])
        try:
            native("Escape", True)
            self.captured(window, lambda frame: not frame["panel_open"] and "Escape" in frame["keys"])
        finally:
            native("Escape", False)
        self.assertIsNone(window.child.poll(), "first Escape closes the pack")
        self.captured(window, lambda frame: "Escape" not in frame["keys"])
        try:
            native("Escape", True)
            self.assertEqual(window.child.wait(timeout=30), 0)
        finally:
            if window.child.poll() is None:
                native("Escape", False)

    def validate_package(self, package):
        log = self.directory / (package.name + "-validation.log")
        with log.open("w", encoding="utf-8") as output:
            result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                    stdout=output, stderr=subprocess.STDOUT, timeout=90)
        self.assertEqual(result.returncode, 0, log.read_text(encoding="utf-8")[-2500:])

    def launch_connected(self, captures, *, address=None):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        # Native routes below finish with escape_window and retain their
        # bounded per-step waits; their lifetime belongs to the test driver.
        return self.launch("tor-client-3d", [
            "--connect", address or self.address, "--capture", self.directory / captures,
            "--capture-every", "0.1",
        ], executable=executable)

    def escape_window(self, window, native):
        try:
            native("Escape", True)
            self.assertEqual(window.child.wait(timeout=30), 0)
        finally:
            if window.child.poll() is None:
                native("Escape", False)

    def test_native_movement_doors_and_fetch_preserve_disclosed_cells(self):
        package = self.directory / "native-traversal"
        shutil.copytree(ROOT / "scenarios/two-room", package)
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        region["anchors"]["start"] = [4, 1, 0]
        region["items"][0]["at"] = [4, 2, 0]
        region["lit"] = True
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        self.validate_package(package)
        self.server(scenario=package)
        window = self.launch_connected("traversal-captures")
        initial = self.captured(window, lambda frame: frame["has_control"] and not frame["busy"] and frame["can_act"])
        self.assertTrue(initial["foot_cell"])
        initial_body = next(actor["world"] for actor in initial["actors"] if actor["is_self"])
        door_id = next(door["id"] for door in initial["doors"] if door["open"] and door["reachable"])
        previous_revision = int(initial["revision"])
        native = self.native_keys(window)
        for open_state in (False, True):
            try:
                native("x", True)
                changed = self.captured(window, lambda frame: "KeyX" in frame["keys"] and any(
                    door["id"] == door_id and door["open"] == open_state for door in frame["doors"]))
            finally:
                native("x", False)
            self.assertGreater(int(changed["revision"]), previous_revision)
            previous_revision = int(changed["revision"])
            self.captured(window, lambda frame: "KeyX" not in frame["keys"] and
                          not frame["busy"] and frame["can_act"])
        def displaced(frame):
            return frame["foot_cell"] != initial["foot_cell"] and any(
                actor["is_self"] and max(abs(a-b) for a,b in zip(actor["world"], initial_body)) > 0.01
                for actor in frame["actors"])
        try:
            native("a", True)
            # Held movement repeats. On a slow renderer every captured frame
            # can already contain the next request; idle belongs after release.
            self.captured(window, lambda frame: "KeyA" in frame["keys"] and displaced(frame))
        finally:
            native("a", False)
        moved = self.captured(window, lambda frame: "KeyA" not in frame["keys"] and
                              not frame["busy"] and frame["can_act"] and displaced(frame))
        self.assertNotEqual(moved["world_feet"], initial["world_feet"], "presented scene must move too")
        previous = None
        for _ in range(12):
            settled = self.captured(window, lambda frame: "KeyA" not in frame["keys"] and not frame["busy"])
            item = next(item for item in settled["ground_items"] if item["cell_key"] != settled["foot_cell"]
                        and "token" in item["name"])
            if previous is not None and max(abs(a-b) for a,b in zip(previous, item["screen"])) < 0.0005:
                break
            previous = item["screen"]
        else:
            self.fail("camera did not settle for a native item pick")
        destination = item["cell_key"]
        self.assertTrue(destination)
        mouse = self.native_mouse(window)
        mouse(item["screen"])
        self.captured(window, lambda frame: frame["hover"]["title"] == item["name"].capitalize())
        try:
            mouse(item["screen"], True)
            fetched = self.captured(window, lambda frame: any(held["name"] == item["name"] for held in frame["inventory"]))
        finally:
            mouse(item["screen"], False)
        self.assertEqual(fetched["foot_cell"], destination, "fetch must reach the actual disclosed item cell")
        self.assertNotEqual(fetched["foot_cell"], moved["foot_cell"])
        self.escape_window(window, native)

    def test_native_click_attacks_the_selected_actor_and_plays_its_motion(self):
        package = creature_package(self, "native-attack", hostile=True, abilities=())
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        manifest["characters"][0]["turn_ticks"] = 100
        manifest["characters"][0]["asset"] = "creature.humanoid.human"
        manifest["factions"] = {"neutral": ["foe"], "foe": ["neutral"]}
        manifest["creatures"]["species"]["human"]["behavior"] = {
            "disposition": "passive", "idle": "wait", "memory_ticks": 1000}
        manifest["ai_profiles"]["practice"] = {"care": False, "equipment": False, "loot": False, "flee_percent": 0}
        for theme in manifest["themes"]:
            manifest.setdefault("assets", {}).setdefault(theme, []).append("creature.humanoid.human")
        manifest.setdefault("modules", []).append("native-actors.toml")
        path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        (package / "native-actors.toml").write_text(
            'namespace = "native"\n[actors."native.practice"]\nasset = "creature.humanoid.human"\n',
            encoding="utf-8", newline="\n")
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        region["actors"][0]["archetype"] = "native.practice"
        region["lit"] = True
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        self.validate_package(package)
        self.server(scenario=package)
        trace = AbilityTrace()
        relay = StreamRelay(self.address, on_server_message=trace.observe)
        self.addCleanup(relay.close)
        window = self.launch_connected("attack-captures", address=relay.address)
        initial = self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        caster = next(actor for actor in initial["actors"] if actor["is_self"])["target"]
        target = next(actor for actor in initial["actors"] if actor["name"] == "practice target")
        self.assertTrue(target["hostile"])
        self.assertIsNotNone(target["screen"])
        native = self.native_keys(window)
        mouse = self.native_mouse(window)
        mouse(target["screen"])
        self.captured(window, lambda frame: frame["hover"]["title"] == "Practice target")
        def motion(frame):
            return any(actor["is_self"] and actor["motion"] == "Strike" for actor in frame["actors"])
        try:
            mouse(target["screen"], True)
            clicked = self.captured(window, lambda frame: "Left" in frame["mouse"] and not frame["busy"])
        finally:
            mouse(target["screen"], False)
        saw_motion = motion(clicked)
        for _ in range(8):
            if trace.attacks(caster, target["target"]):
                break
            try:
                native("space", True)
                advanced = self.captured(window, lambda frame: "Space" in frame["keys"] and not frame["busy"])
                saw_motion |= motion(advanced)
            finally:
                native("space", False)
            released = self.captured(window, lambda frame: "Space" not in frame["keys"])
            saw_motion |= motion(released)
        self.assertTrue(trace.attacks(caster, target["target"]), "the real server must execute the selected-target basic attack")
        if not saw_motion:
            self.captured(window, motion)
        self.escape_window(window, native)

    def test_transport_failure_is_reported_and_quit_returns_failure(self):
        server = self.server(scenario=CLIENT / "scenarios/menagerie")
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        window = self.launch("tor-client-3d", [
            "--connect", self.address, "--capture", self.directory / "failure-captures", "--capture-every", "0.4",
            "--quit-after", "120",
        ], executable=executable)
        self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        server.stop(save=False)
        failed = self.frame(window, lambda frame: frame.get("type") == "3d_frame" and frame["fatal"], seconds=30)
        self.assertTrue(failed["gpu_readback"], "the connection error must reach the presented UI")
        native = self.native_keys(window)
        try:
            native("Escape", True)
            self.assertEqual(window.child.wait(timeout=40), 1, "save failure must return the application error code, not a driver crash")
        finally:
            if window.child.poll() is None:
                native("Escape", False)


if __name__ == "__main__":
    unittest.main()
