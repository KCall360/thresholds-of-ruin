"""The 3D maintenance gate owns its native checks and cannot add core gates."""
import importlib.util
import contextlib
import io
import json
import os
from pathlib import Path
import tempfile
import subprocess
from types import SimpleNamespace
import unittest
from unittest import mock


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


class IndependentVerification(unittest.TestCase):
    def test_failed_native_child_retains_evidence_inside_its_verification_logs(self):
        spec = importlib.util.spec_from_file_location("client_3d_verify_artifacts", HERE / "verify.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        child = ("import os,pathlib; "
                 "p=pathlib.Path(os.environ['TOR_TEST_ARTIFACTS']); "
                 "p.mkdir(parents=True,exist_ok=True); "
                 "(p/'witness.txt').write_text('retained child diagnostic'); "
                 "raise SystemExit(1)")
        with tempfile.TemporaryDirectory(dir=ROOT / ".local") as temporary:
            directory = Path(temporary)
            outside = directory / "unrelated-artifacts"
            with mock.patch.object(module, "ROOT", directory), \
                    mock.patch.object(module, "TARGET", directory / "target"), \
                    mock.patch.object(module, "steps", return_value=[
                        ("process-tests", [module.sys.executable, "-c", child])]), \
                    mock.patch.object(module, "source_identity", return_value={"inputs_sha256": "fixed"}), \
                    mock.patch.object(module.subprocess, "check_output", return_value="fixture toolchain"), \
                    mock.patch.dict(os.environ, {"TOR_TEST_ARTIFACTS": str(outside)}), \
                    contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(module.main([]), 1, "retaining evidence must not mask failure")
            receipts = list((directory / ".local/verify-3d").glob("*/receipt.json"))
            self.assertEqual(len(receipts), 1)
            evidence = receipts[0].parent / "process-failures/witness.txt"
            self.assertTrue(evidence.is_file(), "the uploaded log directory lost the failing child evidence")
            self.assertEqual(evidence.read_text(), "retained child diagnostic")
            self.assertFalse(outside.exists(), "a verifier must own its artifact destination")
            receipt = json.loads(receipts[0].read_text())
            self.assertEqual(receipt["environment"]["TOR_TEST_ARTIFACTS"], str(evidence.parent))
            self.assertFalse(receipt["passed"])

    def test_receipt_records_renderer_and_build_configuration_without_credentials(self):
        spec = importlib.util.spec_from_file_location("client_3d_verify_configuration", HERE / "verify.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory(dir=ROOT / ".local") as temporary:
            directory = Path(temporary)
            with mock.patch.object(module, "ROOT", directory), mock.patch.object(module, "TARGET", directory / "target"), \
                    mock.patch.object(module, "steps", return_value=[]), \
                    mock.patch.object(module, "source_identity", return_value={"inputs_sha256": "fixed"}), \
                    mock.patch.object(module.subprocess, "check_output", return_value="fixture toolchain"), \
                    mock.patch.dict(os.environ, {"WGPU_BACKEND": "dx12", "RUSTFLAGS": "-Ctarget-cpu=generic",
                                                 "TOR_SERVER_TOKEN": "must-not-record"}, clear=True), \
                    contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(module.main([]), 0)
            receipts = list((directory / ".local/verify-3d").glob("*/receipt.json"))
            self.assertEqual(len(receipts), 1)
            receipt = json.loads(receipts[0].read_text(encoding="utf-8"))
            self.assertEqual(receipt["environment"]["WGPU_BACKEND"], "dx12")
            self.assertEqual(receipt["environment"]["RUSTFLAGS"], "-Ctarget-cpu=generic")
            self.assertEqual(receipt["environment"]["CARGO_BUILD_JOBS"], "2")
            self.assertEqual(receipt["platform"], module.sys.platform)
            self.assertNotIn("must-not-record", json.dumps(receipt))
            self.assertNotIn("TOR_SERVER_TOKEN", receipt["environment"])

    def test_failed_source_inventory_releases_its_target_owner(self):
        spec = importlib.util.spec_from_file_location("client_3d_verify_failure", HERE / "verify.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory(dir=ROOT / ".local") as temporary:
            directory = Path(temporary)
            with mock.patch.object(module, "ROOT", directory), mock.patch.object(module, "TARGET", directory / "target"), \
                    mock.patch.object(module, "source_identity", side_effect=RuntimeError("inventory failed")):
                with self.assertRaisesRegex(RuntimeError, "inventory failed"):
                    module.main([])
            self.assertFalse((directory / "target/.verify-3d-owner").exists(), "a failed preflight must release the target owner")

    def test_workflow_requires_an_explicit_client_path_change(self):
        path = ROOT / ".github/workflows/client-3d.yml"
        self.assertTrue(path.is_file(), "3D checks need a separate, path-scoped workflow")
        text = path.read_text(encoding="utf-8")
        self.assertEqual(text.count('      - "crates/client-3d/**"'), 2)
        self.assertNotIn('      - "crates/protocol/**"', text)
        self.assertNotIn('      - "crates/client-common/**"', text)
        self.assertIn("os: [windows-latest, ubuntu-latest]", text)
        self.assertIn("profile: [debug, release]", text)

    def test_profile_plan_preserves_real_process_and_renderer_checks(self):
        path = HERE / "verify.py"
        self.assertTrue(path.is_file(), "the independent workspace needs its own verifier")
        spec = importlib.util.spec_from_file_location("client_3d_verify", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        for profile in ("debug", "release"):
            steps = module.steps(profile)
            commands = {name: command for name, command in steps}
            for name in ("fmt", "clippy", "architecture", "rust-tests", "core-binaries", "client-binary", "process-tests"):
                self.assertIn(name, commands)
            self.assertIn("--manifest-path", commands["rust-tests"])
            self.assertIn("--workspace", commands["rust-tests"])
            self.assertIn("--locked", commands["rust-tests"])
            self.assertEqual("--release" in commands["rust-tests"], profile == "release")
            self.assertIn("--release" if profile == "release" else "--locked", commands["client-binary"])
            self.assertIn("test_*process.py", commands["process-tests"])


class FakeX11Desktop:
    """Model X11 coordinates and the observed xdotool relative-sync behavior."""
    def __init__(self, *, reported_position=None, reported_window=71):
        self.origin = (200, 100)
        self.size = (800, 600)
        self.pointer = (600, 400)
        self.reported_position = reported_position
        self.reported_window = reported_window
        self.buttons = []

    def move(self, command):
        end = command.index('getmouselocation') if 'getmouselocation' in command else len(command)
        x, y = map(int, command[end - 2:end])
        target = (x + self.origin[0], y + self.origin[1]) if '--window' in command else (x, y)
        # Upstream compares old screen coordinates with the unconverted request.
        # A relative no-op can therefore wait for a movement that never happens.
        if '--sync' in command and self.pointer == target and self.pointer != (x, y):
            raise subprocess.TimeoutExpired(command, 10)
        self.pointer = target

    def run(self, command, **_options):
        action = command[1]
        if action == 'mousemove':
            self.move(command)
        elif action in ('mousedown', 'mouseup'):
            self.buttons.append((action, self.pointer))
        elif action != 'windowfocus':
            raise AssertionError(f'unexpected native action: {action}')
        return subprocess.CompletedProcess(command, 0)

    def output(self, command, **_options):
        if command[1] == 'getwindowgeometry':
            return 'WINDOW=71\nX=200\nY=100\nWIDTH=800\nHEIGHT=600\nSCREEN=0\n'
        if command[1] == 'mousemove':
            self.move(command)
        elif command[1] != 'getmouselocation':
            raise AssertionError(f'unexpected native query: {command[1]}')
        x, y = self.reported_position or self.pointer
        return f'X={x}\nY={y}\nSCREEN=0\nWINDOW={self.reported_window}\n'


class NativeMouseContract(unittest.TestCase):
    @contextlib.contextmanager
    def linux_mouse(self, desktop):
        spec = importlib.util.spec_from_file_location('client_3d_mouse_contract', HERE / 'test_client_process.py')
        native = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(native)
        caller = unittest.TestCase()
        caller._native_window = lambda _client: '71'
        with mock.patch.object(native, 'os', SimpleNamespace(name='posix')), \
                mock.patch.object(native.subprocess, 'run', side_effect=desktop.run), \
                mock.patch.object(native.subprocess, 'check_output', side_effect=desktop.output):
            try:
                yield native.Native3DProcesses.native_mouse(caller, object())
            finally:
                caller.doCleanups()

    def test_repeated_pointer_position_still_delivers_down_and_up_to_owned_window(self):
        desktop = FakeX11Desktop()
        with self.linux_mouse(desktop) as mouse:
            mouse((0.5, 0.5))
            mouse((0.5, 0.5), True)
            mouse((0.5, 0.5), False)
            self.assertEqual(desktop.buttons, [('mousedown', (600, 400)), ('mouseup', (600, 400))])

    def test_pointer_readback_must_confirm_position_and_window_before_press(self):
        for position, window in (((601, 400), 71), (None, 72)):
            with self.subTest(position=position, window=window):
                desktop = FakeX11Desktop(reported_position=position, reported_window=window)
                desktop.pointer = (0, 0)
                with self.linux_mouse(desktop) as mouse:
                    with self.assertRaises(AssertionError):
                        mouse((0.5, 0.5), True)
                    self.assertEqual(desktop.buttons, [])


class ScreenshotPixels(unittest.TestCase):
    def test_native_png_filters_preserve_actual_pixels(self):
        import struct
        import zlib
        from render_pixels import decode_png, dark_fraction

        def chunk(kind, payload):
            return (struct.pack(">I", len(payload)) + kind + payload
                    + struct.pack(">I", zlib.crc32(kind + payload)))
        expected = [bytes([10, 20, 30, 40, 50, 60]), bytes([20, 35, 50, 65, 80, 95])]
        filtered = [
            ([10, 20, 30, 40, 50, 60], [20, 35, 50, 65, 80, 95]),
            ([10, 20, 30, 30, 30, 30], [20, 35, 50, 45, 45, 45]),
            ([10, 20, 30, 40, 50, 60], [10, 15, 20, 25, 30, 35]),
            ([10, 20, 30, 35, 40, 45], [15, 25, 35, 35, 38, 40]),
            ([10, 20, 30, 30, 30, 30], [10, 15, 20, 25, 30, 35]),
        ]
        for kind, rows in enumerate(filtered):
            raw = b"".join(bytes([kind]) + bytes(row) for row in rows)
            data = (b"\x89PNG\r\n\x1a\n"
                    + chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 2, 8, 2, 0, 0, 0))
                    + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))
            width, height, channels, decoded = decode_png(data)
            self.assertEqual((width, height, channels), (2, 2, 3))
            self.assertEqual(decoded, expected, kind)
            self.assertEqual(dark_fraction(data, (0, 0, 1, 1)), 0.75)
        raw = bytes([0, 10, 20, 30, 255, 90, 100, 110, 0])
        data = (b"\x89PNG\r\n\x1a\n"
                + chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 1, 8, 6, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))
        self.assertEqual(dark_fraction(data, (0, 0, 1, 1)), 0.5, "alpha is not brightness")

if __name__ == "__main__":
    unittest.main()
