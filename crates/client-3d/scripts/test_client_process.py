"""Real server + Bevy window + OS input, maintained outside the core gate."""
import hashlib
import json
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
import unittest

CLIENT = Path(__file__).resolve().parents[1]
ROOT = CLIENT.parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from process_harness import ProcessTestCase, SPECTATOR_TOKEN, creature_package, door, non_graphical
from arena_matrix import toml_source
from stream_relay import StreamRelay


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
                {"tick": 5, "actor": "woman", "operation": {"type": "dress", "outfit": "sylvan-duelist"}},
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
            self.assertEqual(actor["composition"]["outfit"], "sylvan-duelist")
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

    def test_lineup_readback_holds_and_resumes_the_presentation_clock(self):
        self._check_review_clock()

    def test_fixed_step_lineup_captures_on_the_selected_clock(self):
        self._check_review_clock(50)

    def _check_review_clock(self, hz=None):
        executable = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug" / ("tor-client-3d" + self.suffix)))
        captures = self.directory / "capture-clock"
        captures.mkdir()
        window = self.launch("tor-client-3d", [
            "--women", "--family", "Warrior", "--outfit", "sylvan-duelist",
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

    def native_keys(self, client):
        handle = self._native_window(client)
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes
            user32 = ctypes.WinDLL("user32", use_last_error=True)
            user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
            user32.MapVirtualKeyW.argtypes = [wintypes.UINT, wintypes.UINT]
            def key(name, down):
                vk = ord(name.upper()) if len(name) == 1 and name.isascii() and name.isalnum() else {
                    "Escape": 0x1B, "space": 0x20, "Return": 0x0D, "Tab": 0x09,
                    "Up": 0x26, "Down": 0x28, "Left": 0x25, "Right": 0x27,
                    "PageUp": 0x21, "PageDown": 0x22,
                }[name]
                scan = user32.MapVirtualKeyW(vk, 0)
                extended = name in ("Up", "Down", "Left", "Right", "PageUp", "PageDown")
                self.assertTrue(user32.PostMessageW(handle, 0x100 if down else 0x101, vk,
                    1 | (scan << 16) | (0x01000000 if extended else 0) | (0 if down else 0xC0000000)))
            return key
        def key(name, down):
            # Winit consumes XInput events. Use XTEST on the focused game
            # window instead of synthetic core events sent with --window.
            subprocess.run(["xdotool", "windowfocus", "--sync", handle], check=True, timeout=10)
            subprocess.run(["xdotool", "keydown" if down else "keyup", name], check=True, timeout=10)
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
            "--quit-after", "120",
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
            "--quit-after", "240",
        ], executable=executable)
        frame = self.captured(window, lambda frame: frame["has_control"] and not frame["busy"])
        native = self.native_keys(window)

        def tap(name, done):
            native(name, True)
            try:
                return self.captured(window, lambda frame: not frame["busy"] and done(frame))
            finally:
                native(name, False)

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
