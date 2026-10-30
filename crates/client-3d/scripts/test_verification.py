"""The 3D maintenance gate owns its native checks and cannot add core gates."""
import importlib.util
import contextlib
import io
import json
import os
import re
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
        self.assertIn("  workflow_dispatch:", text)
        self.assertNotIn("paths-ignore:", text)
        self.assertIn("os: [windows-latest, ubuntu-latest]", text)
        self.assertIn("profile: [debug, release]", text)
        for event in ("pull_request", "push"):
            section = re.split(r"\n  \S", text.split(f"  {event}:\n", 1)[1], maxsplit=1)[0]
            # Read the checked-in patterns, preserving order. Limit the matcher
            # to exact paths and directory/**, the only grammar used here.
            # GitHub applies the last matching rule per file, then runs when
            # ANY changed file is included (not when every file is included).
            patterns = re.findall(r'^      - "([^"\n]+)"$', section, re.MULTILINE)
            self.assertTrue(patterns, event)
            def selected(paths, rules=patterns):
                for path in paths:
                    included = False
                    for rule in rules:
                        negative = rule.startswith("!")
                        pattern = rule[1:] if negative else rule
                        directory = pattern.endswith("/**")
                        literal = pattern[:-3] if directory else pattern
                        self.assertNotRegex(literal, r"[!*?\[\]]", "extend the matcher for new glob syntax")
                        matches = path.startswith(literal + "/") if directory else path == literal
                        if matches:
                            included = not negative
                    if included:
                        return True
                return False

            scenario = "crates/client-3d/scenarios/menagerie/"
            excluded = [scenario + name for name in (
                "scenario.toml", "index.json", "validation.json",
                "catalog/actors.toml", "regions/1.toml")]
            excluded += ["crates/client-3d/scenarios/future/nested/source.toml",
                         "crates/protocol/src/lib.rs", "crates/client-common/src/lib.rs",
                         "scenarios/rogue/scenario.toml", "docs/status.md"]
            included = ["crates/client-3d/" + name for name in (
                "src/main.rs", "model/src/lib.rs", "model/tests/it/input.rs",
                "assets/models/body.glb", "scripts/test_client_process.py",
                "Cargo.toml", "Cargo.lock", "scenarios-other/client.asset")]
            included += [".github/workflows/client-3d.yml"]
            for changed in excluded:
                with self.subTest(event=event, excluded=changed):
                    self.assertFalse(selected([changed]))
            self.assertFalse(selected(excluded), "scenario/core-only batches must not start 3D")
            for changed in included:
                with self.subTest(event=event, included=changed):
                    self.assertTrue(selected([changed]))
                    self.assertTrue(selected([*excluded, changed]), "a scenario exclusion cannot veto a client edit")
                    self.assertTrue(selected([changed, *excluded]), "file ordering cannot change the trigger")
            # Guard the regression oracle too: negation is ordered, not a
            # global blacklist, and later positives can re-include a file.
            positive = "crates/client-3d/**"
            negative = "!crates/client-3d/scenarios/**"
            self.assertFalse(selected([scenario + "scenario.toml"], [positive, negative]))
            self.assertTrue(selected([scenario + "scenario.toml"], [negative, positive]))
            self.assertTrue(selected([scenario + "scenario.toml"],
                                     [positive, negative, scenario + "scenario.toml"]))

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
    def __init__(self, *, reported_position=None, reported_window=71,
                 origin=(200, 100), size=(800, 600), root_size=None):
        self.origin = origin
        self.size = size
        self.root_size = root_size
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
        if self.root_size:
            target = tuple(max(0, min(value, limit - 1)) for value, limit in zip(target, self.root_size))
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
            return (f'WINDOW=71\nX={self.origin[0]}\nY={self.origin[1]}\n'
                    f'WIDTH={self.size[0]}\nHEIGHT={self.size[1]}\nSCREEN=0\n')
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


    def test_hud_background_target_remains_reachable_on_the_default_xvfb_desktop(self):
        spec = importlib.util.spec_from_file_location('client_3d_hud_fixture', HERE / 'test_client_process.py')
        native = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(native)
        actor = native.Native3DProcesses('test_framed_hud_dock_opens_inventory_without_a_world_action')
        actions = ['Inventory', 'Equipment', 'Character', 'Map', 'Places', 'Journal', 'Wait']
        buttons = [{'action': action, 'screen': [0.2 + i * 0.08, 0.9], 'size': [80, 54]}
                   for i, action in enumerate(actions)]
        initial = {'has_control': True, 'busy': False, 'revision': '7', 'world_feet': [2, 3, 0],
                   'hud': {'readiness': 'READY', 'buttons': buttons}, 'screen': None,
                   'panel_open': False, 'mouse': []}
        closed = dict(initial)
        frames = iter([initial, dict(initial, panel_open=True, panel_actions=['Drop']),
                       dict(initial, mouse=['Left']), closed, dict(initial, screen='History')])
        def captured(_window, predicate):
            frame = next(frames)
            self.assertTrue(predicate(frame), 'positive frame fixture must match the requested state')
            return frame
        def tap(_window, _native, _key, predicate):
            self.assertTrue(predicate(closed))
            return closed
        actor.suffix = ""  # The isolated fixture bypasses process-class setup.
        actor.address = "127.0.0.1:1"
        actor.directory = Path("fixture")
        actor.server = mock.Mock()
        actor.fallback_package = mock.Mock(return_value=Path('fixture'))
        actor.launch = mock.Mock(return_value=SimpleNamespace(child=mock.Mock()))
        actor.launch.return_value.child.poll.return_value = None
        actor.launch.return_value.child.wait.return_value = 0
        actor.captured = captured
        actor.tap = tap
        actor.native_keys = mock.Mock(return_value=mock.Mock())
        desktop = FakeX11Desktop(origin=(0, 0), size=(1440, 900), root_size=(1280, 1024))
        with self.linux_mouse(desktop) as mouse:
            actor.native_mouse = lambda _window: mouse
            actor.test_framed_hud_dock_opens_inventory_without_a_world_action()
        background = [point for action, point in desktop.buttons if action == 'mousedown' and point[1] > 870]
        self.assertEqual(len(background), 1, 'retain the independent empty-background click witness')
        self.assertLess(background[0][0], 1280)
        self.assertGreater(background[0][1], max(button['screen'][1] * 900 + button['size'][1] / 2
                                               for button in buttons))

    def test_map_screen_wait_ignores_transitional_frames_before_the_requested_screen(self):
        spec = importlib.util.spec_from_file_location('client_3d_screen_fixture', HERE / 'test_client_process.py')
        native = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(native)
        actor = native.Native3DProcesses('test_map_journal_shares_the_disclosed_map_and_preserves_note_typing')
        actor.suffix = ""  # The isolated fixture bypasses process-class setup.
        actor.address = "127.0.0.1:1"
        actor.directory = Path("fixture")
        actor.server = mock.Mock()
        actor.launch = mock.Mock()
        actor.captured = mock.Mock(return_value={'has_control': True, 'busy': False})
        actor.native_keys = mock.Mock()
        class ObservedPredicate(Exception):
            pass
        def tap(_window, _native, key, predicate):
            self.assertEqual(key, 'm')
            self.assertFalse(predicate({'screen': None, 'map_journal': {'visible': False}}))
            self.assertFalse(predicate({'screen': 'History', 'map_journal': {'visible': True}}))
            self.assertTrue(predicate({'screen': 'Places { renaming: None }', 'map_journal': {'visible': True}}))
            raise ObservedPredicate()
        actor.tap = tap
        with self.assertRaises(ObservedPredicate):
            actor.test_map_journal_shares_the_disclosed_map_and_preserves_note_typing()


class LocalGraphicalWaiverContracts(unittest.TestCase):
    def module(self):
        spec = importlib.util.spec_from_file_location("client_3d_verify_waiver", HERE / "verify.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_pre_window_appearance_validation_is_required_non_graphical(self):
        import test_client_process
        from process_harness import is_graphical_test
        case = test_client_process.Native3DProcesses("test_live_appearance_override_is_rejected_before_connecting")
        self.assertFalse(is_graphical_test(case))

    def test_local_only_rejects_ci_before_any_verification(self):
        module = self.module()
        with mock.patch.dict(os.environ, {"CI": "1"}), mock.patch.object(module, "steps") as steps:
            with self.assertRaises(SystemExit):
                module.main(["--waive-local-graphical", "unused.json"])
            steps.assert_not_called()

    def test_exact_discovery_rejects_unknown_duplicate_and_non_graphical_ids(self):
        module = self.module()
        class Native(unittest.TestCase):
            graphical = True
            def test_window(self): pass
        class Pure(unittest.TestCase):
            def test_pure(self): pass
        native, pure = Native("test_window"), Pure("test_pure")
        for names in ([native.id(), native.id()], [pure.id()], ["test_missing.Class.test_window"]):
            with self.subTest(names=names), mock.patch.object(module.graphical_waiver, "read_manifest",
                    return_value={"reason": "explicit local authorization", "tests": names}), \
                    mock.patch.object(module.unittest.defaultTestLoader, "discover",
                    return_value=unittest.TestSuite([native, pure])):
                with self.assertRaises(ValueError):
                    module.waiver_plan("fixture.json")

    def test_zero_exit_requires_exact_accounting_and_stable_source_and_never_claims_clean(self):
        module = self.module()
        name = "test_fixture.Native.test_window"
        manifest = {"reason": "explicit local authorization", "tests": [name]}
        rows = [{"kind": "local_graphical_waiver", **manifest},
                {"kind": "test", "test_id": name, "outcome": "waived"},
                {"kind": "suite", "waived": 1, "waived_tests": [name]}]
        for mode in ("valid", "missing", "mismatch", "source-change"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory(dir=ROOT / ".local") as temporary:
                directory = Path(temporary)
                def run(command, **kwargs):
                    self.assertIn("--waive-local-graphical", command)
                    self.assertIn("test_runner.py", command[1])
                    snapshot = Path(command[command.index("--waive-local-graphical") + 1])
                    self.assertNotEqual(snapshot, Path("fixture.json"))
                    self.assertEqual(json.loads(snapshot.read_text()), manifest)
                    if mode != "missing":
                        actual = rows if mode != "mismatch" else rows[:-1]
                        Path(kwargs["env"]["TOR_TEST_TIMINGS"]).write_text(
                            "\n".join(json.dumps(row) for row in actual) + "\n")
                    return SimpleNamespace(returncode=0)
                with mock.patch.object(module, "ROOT", directory), \
                        mock.patch.object(module, "TARGET", directory / "target"), \
                        mock.patch.object(module, "waiver_plan", return_value=manifest), \
                        mock.patch.object(module, "steps", return_value=[("process-tests", ["python", "-m", "unittest", "discover"])]), \
                        mock.patch.object(module, "source_identity", side_effect=[{"hash": "a"}, {"hash": "b" if mode == "source-change" else "a"}]), \
                        mock.patch.object(module.subprocess, "check_output", return_value="fixture toolchain"), \
                        mock.patch.object(module.subprocess, "run", side_effect=run), \
                        mock.patch.dict(os.environ, {}, clear=True), contextlib.redirect_stdout(io.StringIO()):
                    code = module.main(["--waive-local-graphical", "fixture.json"])
                receipt = json.loads(next((directory / ".local/verify-3d").glob("*/receipt.json")).read_text())
                self.assertEqual(code, 0 if mode == "valid" else 1)
                self.assertEqual(receipt["schema"], "tor-client-3d-verification-v3")
                self.assertEqual(receipt["gate_passed"], mode == "valid")
                self.assertFalse(receipt["clean_passed"])
                self.assertFalse(receipt["passed"])
    def test_a_timed_out_process_suite_still_reports_what_failed(self):
        module = self.module()
        name = "test_fixture.Native.test_window"
        rows = [{"kind": "diagnostic", "test_id": name, "exception_type": "TimeoutExpired",
                 "traceback": "Traceback\nsubprocess.TimeoutExpired: window took too long"},
                {"kind": "test", "test_id": name, "outcome": "error", "seconds": 181.5}]
        with tempfile.TemporaryDirectory(dir=ROOT / ".local") as temporary:
            directory = Path(temporary)
            def run(command, **kwargs):
                self.assertIn("test_runner.py", command[1])
                self.assertEqual(command[-1], "-v")
                self.assertNotIn("--waive-local-graphical", command)
                self.assertEqual(kwargs["timeout"], module.PROCESS_TIMEOUT)
                Path(kwargs["env"]["TOR_TEST_TIMINGS"]).write_text("\n".join(json.dumps(row) for row in rows) + "\n")
                raise module.subprocess.TimeoutExpired(command, kwargs["timeout"])
            printed = io.StringIO()
            with mock.patch.object(module, "ROOT", directory), \
                    mock.patch.object(module, "TARGET", directory / "target"), \
                    mock.patch.object(module, "steps", return_value=[("process-tests", ["python", "-m", "unittest", "discover"])]), \
                    mock.patch.object(module, "source_identity", return_value={"hash": "a"}), \
                    mock.patch.object(module.subprocess, "check_output", return_value="fixture toolchain"), \
                    mock.patch.object(module.subprocess, "run", side_effect=run), \
                    mock.patch.dict(os.environ, {}, clear=True), contextlib.redirect_stdout(printed):
                self.assertEqual(module.main([]), 1)
            receipt = json.loads(next((directory / ".local/verify-3d").glob("*/receipt.json")).read_text())
            self.assertEqual(receipt["steps"][0]["exit_code"], 124)
            output = printed.getvalue()
            self.assertIn("181.5s error " + name, output)
            self.assertIn("TimeoutExpired in " + name, output)
            self.assertIn("window took too long", output)


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
