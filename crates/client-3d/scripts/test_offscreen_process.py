"""Real image-target rendering without a native display or OS input.

Requires a built 3D client and a working wgpu adapter (software is sufficient).
Unlike native acceptance tests, this class must not request a display. Failure
artifacts use the shared process harness; this owner never builds binaries.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest

CLIENT = Path(__file__).resolve().parents[1]
ROOT = CLIENT.parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from process_harness import ProcessTestCase  # noqa: E402
from render_pixels import decode_png  # noqa: E402


def scene():
    """One actual showcase actor, held at the same tick for both captures."""
    return {
        "version": 1, "hz": 20, "viewport": [641, 479],
        "actors": [{"id": "duelist", "model": "RedMage", "seed": 7,
                    "body": {"woman": True, "hair": 2, "build": 1, "skin": 2},
                    "outfit": "sylvan-duelist", "equipment": [],
                    "position": [0, -0.5, 0], "yaw": 0}],
        "cameras": [{"id": "front", "position": [0, 1, 3.3],
                     "target": [0, 0.22, 0], "fov_degrees": 48}],
        "events": [],
        "captures": [{"tick": 0, "name": name, "camera": "front"}
                     for name in ("first", "repeat")],
    }


class Offscreen3DProcesses(ProcessTestCase):
    @classmethod
    def setUpClass(cls):
        # Do not invoke ProcessTestCase's unrelated core-workspace build.
        profile = os.environ.get("TOR_TEST_PROFILE", "debug")
        if profile not in ("debug", "release"):
            raise ValueError("TOR_TEST_PROFILE must be debug or release")
        suffix = ".exe" if sys.platform == "win32" else ""
        cls.executable = Path(os.environ.get(
            "TOR_3D_TEST_BINARY", CLIENT / "target" / profile / ("tor-client-3d" + suffix)))
        if not cls.executable.is_file():
            raise FileNotFoundError(f"Build the 3D client or set TOR_3D_TEST_BINARY: {cls.executable}")

    def source(self, value=None, name="scene"):
        path = self.directory / f"{name}.json"
        path.write_text(json.dumps(scene() if value is None else value), encoding="utf-8")
        return path

    def invoke(self, label, arguments, timeout=30):
        environment = {key: value for key, value in os.environ.items()
                       if key not in ("DISPLAY", "WAYLAND_DISPLAY", "TOR_SPECTATOR_TOKEN", "TOR_WIZARD_TOKEN")}
        command = [str(self.executable), *map(str, arguments)]
        metadata = {"command": command, "profile": os.environ.get("TOR_TEST_PROFILE", "debug"),
                    "cwd": str(ROOT), "timeout_seconds": timeout,
                    "display_environment": {key: environment.get(key)
                                            for key in ("DISPLAY", "WAYLAND_DISPLAY")}}
        receipt = self.directory / f"{label}.process.json"
        receipt.write_text(json.dumps(metadata, indent=2), encoding="utf-8")
        log_path = self.directory / f"{label}.log"
        # subprocess.run kills and waits on timeout before harness cleanup can
        # preserve the evidence directory. Adapter startup is bounded as well.
        with log_path.open("wb") as log:
            try:
                result = subprocess.run(command, cwd=ROOT, env=environment,
                                        stdout=log, stderr=subprocess.STDOUT,
                                        timeout=timeout, check=False)
                metadata["returncode"] = result.returncode
            except subprocess.TimeoutExpired:
                metadata["timed_out"] = True
                raise
            finally:
                receipt.write_text(json.dumps(metadata, indent=2), encoding="utf-8")
        return result.returncode, log_path.read_text(encoding="utf-8", errors="replace")

    def test_odd_sized_image_and_repeated_capture_without_display(self):
        source = self.source()
        output = self.directory / "rendered"
        code, log = self.invoke("render", ["--scene-script", source, "--offscreen", "--capture", output],
                                timeout=240)
        self.assertEqual(code, 0, log[-4000:])
        manifest = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
        self.assertEqual(manifest["status"], "complete")
        self.assertEqual(manifest["capture_mode"], "offscreen")
        shots = manifest["captures"]
        self.assertEqual([(shot["name"], shot["tick"], shot["clock_ns"]) for shot in shots],
                         [("first", 0, "0"), ("repeat", 0, "0")])
        camera = shots[0]["camera"]
        requested_camera = scene()["cameras"][0]
        self.assertEqual(camera["id"], requested_camera["id"])
        self.assertEqual(camera["fov_degrees"], requested_camera["fov_degrees"])
        for field in ("position", "target"):
            for actual, requested in zip(camera[field], requested_camera[field], strict=True):
                self.assertAlmostEqual(actual, requested, places=6)
        self.assertEqual(shots[0]["camera"], shots[1]["camera"])
        self.assertEqual(shots[0]["actors"], shots[1]["actors"], "readback must not advance the held scene")
        actor = shots[0]["actors"][0]
        requested_actor = scene()["actors"][0]
        for field, expected in requested_actor.items():
            if field == "body":
                for feature, value in expected.items():
                    self.assertEqual(actor["composition"]["body"][feature], value)
            else:
                self.assertEqual(actor["composition"][field], expected)
        self.assertGreater(actor["root_children"], 0)
        self.assertTrue(actor["joints"], "capture must contain the actual procedural rig")
        for shot in shots:
            self.assertTrue(shot["renderer"]["name"], "record the adapter that rendered the image")
            width, height, channels, rows = decode_png((output / f'{shot["name"]}.png').read_bytes())
            self.assertEqual((width, height), (641, 479), "odd-width readback must strip row alignment")
            pixels = [tuple(rows[y][x * channels:x * channels + 3])
                      for y in range(height // 5, height * 4 // 5, 2)
                      for x in range(width // 3, width * 2 // 3, 2)]
            self.assertGreater(len(set(pixels)), 64, "the character region must not be a cleared target")
            colored = sum(max(rgb) > 55 and max(rgb) - min(rgb) > 10 for rgb in pixels)
            self.assertGreater(colored, 100, "the real dressed actor must be visible")
        self.assertEqual(sorted(path.name for path in output.glob("*.png")), ["first.png", "repeat.png"])
        # Exact pixels are intentionally not compared across frames/adapters;
        # deterministic state and decoded nonblank images are separate contracts.

    def test_existing_output_is_refused_without_touching_sentinel(self):
        source = self.source()
        output = self.directory / "existing"
        output.mkdir()
        sentinel = output / "sentinel.bin"
        sentinel.write_bytes(b"preserve previous capture\x00\xff")
        code, log = self.invoke("existing", ["--scene-script", source, "--offscreen", "--capture", output])
        self.assertNotEqual(code, 0, log)
        self.assertEqual(sentinel.read_bytes(), b"preserve previous capture\x00\xff")
        self.assertEqual(list(output.iterdir()), [sentinel])

    def test_invalid_script_and_cli_do_not_create_output(self):
        source = self.source()
        invalid = scene()
        invalid["viewport"] = [0, 479]
        invalid_source = self.source(invalid, "invalid")
        for label, path, extra in [("invalid-script", invalid_source, []),
                                   ("duplicate-offscreen", source, ["--offscreen"]),
                                   ("unknown-option", source, ["--not-a-review-option"])]:
            with self.subTest(case=label):
                output = self.directory / label
                code, log = self.invoke(label, ["--scene-script", path, "--offscreen", "--capture", output, *extra])
                self.assertNotEqual(code, 0, log)
                self.assertFalse(output.exists(), "invalid input must fail before creating capture artifacts")
                self.assertNotIn("panicked at", log, "rejection must be a normal CLI error")


if __name__ == "__main__":
    unittest.main()
