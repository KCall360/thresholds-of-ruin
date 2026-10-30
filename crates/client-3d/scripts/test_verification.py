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
    def test_windows_native_suite_has_bounded_headroom_for_the_expanded_selection(self):
        spec = importlib.util.spec_from_file_location("client_3d_verify_budget", HERE / "verify.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        # Hosted debug completed 35 of 57 selected methods in 3000 seconds,
        # with 21 passes, 14 skips and no assertion failures. Leave room for
        # the remaining methods without changing any individual frame wait.
        self.assertGreaterEqual(module.process_timeout("win32"), 90 * 60)
        self.assertLessEqual(module.process_timeout("win32"), 100 * 60)
        self.assertEqual(module.process_timeout("linux"), 50 * 60)

    def test_windows_native_job_reserves_build_and_artifact_headroom(self):
        workflow = (ROOT / ".github/workflows/client-3d.yml").read_text(encoding="utf-8")
        self.assertIn("timeout-minutes: ${{ matrix.os == 'windows-latest' && 150 || 120 }}", workflow)
        # Forty minutes of measured compilation, ninety for native processes,
        # and fifteen for setup/artifact cleanup fit the bounded Windows job.
        minutes = int(re.search(r"timeout-minutes: .*&& (\d+) ", workflow)[1])
        spec = importlib.util.spec_from_file_location("client_3d_verify_job_budget", HERE / "verify.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        self.assertGreaterEqual(minutes * 60, module.process_timeout("win32") + (40 + 15) * 60)

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


class NativeDisplayContracts(unittest.TestCase):
    def module(self):
        spec = importlib.util.spec_from_file_location('client_3d_display_contract', HERE / 'native_display.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_windows_mode_abi_matches_the_public_utf16_structure(self):
        import ctypes
        mode = self.module().DisplayMode
        self.assertEqual(ctypes.sizeof(mode), 220)
        self.assertEqual((mode.dmPelsWidth.offset, mode.dmPelsHeight.offset), (172, 176))

    def test_ci_display_must_fit_the_client_before_physical_mouse_tests(self):
        display = mock.Mock()
        display.size.side_effect = [(1024, 768), (1920, 1080)]
        display.modes.return_value = [(1024, 768), (1280, 1024), (1920, 1080), (2560, 1440)]
        self.assertEqual(self.module().configure(display), (1920, 1080))
        display.change.assert_called_once_with((1920, 1080))

    def test_larger_existing_desktop_is_preserved(self):
        display = mock.Mock()
        display.size.return_value = (2560, 1440)
        self.assertEqual(self.module().configure(display), (2560, 1440))
        display.change.assert_not_called()
        display.modes.assert_not_called()

    def test_unsupported_or_unchanged_small_desktop_fails_setup(self):
        for modes, error in (([(1024, 768)], None), ([(1920, 1080)], None),
                             ([(1920, 1080)], RuntimeError('driver rejected mode'))):
            with self.subTest(modes=modes, error=error):
                display = mock.Mock()
                display.size.return_value = (1024, 768)
                display.modes.return_value = modes
                display.change.side_effect = error
                with self.assertRaises(RuntimeError):
                    self.module().configure(display)

    def test_workflow_configures_the_physical_desktop_before_native_verification(self):
        workflow = (ROOT / '.github/workflows/client-3d.yml').read_text(encoding='utf-8')
        setup = 'run: python crates/client-3d/scripts/native_display.py'
        self.assertIn(setup, workflow)
        self.assertLess(workflow.index(setup), workflow.index('run: python crates/client-3d/scripts/verify.py'))
        self.assertIn('--server-args="-screen 0 1920x1080x24"', workflow)

    def test_windows_driver_change_requires_success_and_uses_a_temporary_mode(self):
        for code in (0, -2):
            with self.subTest(driver_result=code):
                module = self.module()
                driver = SimpleNamespace(EnumDisplaySettingsW=mock.Mock(),
                                         ChangeDisplaySettingsExW=mock.Mock(return_value=code))
                with mock.patch.object(module.ctypes, 'WinDLL', return_value=driver, create=True):
                    display = module.WindowsDisplay()
                    if code:
                        with self.assertRaisesRegex(RuntimeError, 'failed: -2'):
                            display.change((1920, 1080))
                    else:
                        display.change((1920, 1080))
                device, pointer, window, flags, extra = driver.ChangeDisplaySettingsExW.call_args.args
                mode = pointer._obj
                self.assertEqual((mode.dmSize, mode.dmBitsPerPel, mode.dmPelsWidth, mode.dmPelsHeight),
                                 (220, 32, 1920, 1080))
                self.assertEqual(mode.dmFields, 0x1C0000)
                self.assertEqual((device, window, flags, extra), (None, None, 0, None))

    def test_entrypoint_rejects_local_desktop_changes_before_constructing_a_driver(self):
        module = self.module()
        for platform, environment in (('posix', {'GITHUB_ACTIONS': 'true'}), ('nt', {})):
            with self.subTest(platform=platform), \
                    mock.patch.object(module, 'os', SimpleNamespace(name=platform, environ=environment)), \
                    mock.patch.object(module, 'WindowsDisplay') as driver:
                with self.assertRaises(RuntimeError):
                    module.main()
                driver.assert_not_called()

    def test_advancement_fixture_remains_owned_until_its_explicit_exit(self):
        spec = importlib.util.spec_from_file_location('client_3d_advancement_lifetime', HERE / 'test_client_process.py')
        native = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(native)
        actor = native.Native3DProcesses('test_graphical_advancement_selects_a_pool_previews_and_explicitly_confirms')
        actor.suffix = ''
        actor.address = '127.0.0.1:1'
        actor.directory = Path('fixture')
        actor.server = mock.Mock()
        class ObservedLaunch(Exception):
            pass
        def launch(_name, args, **_options):
            self.assertNotIn('--quit-after', args, 'software rendering must not terminate a valid in-progress test')
            raise ObservedLaunch()
        actor.launch = launch
        with self.assertRaises(ObservedLaunch):
            actor.test_graphical_advancement_selects_a_pool_previews_and_explicitly_confirms()


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
                self.assertEqual(kwargs["timeout"], module.process_timeout(module.sys.platform))
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

class PortraitSilhouette(unittest.TestCase):
    """Known pixel geometry, independent of camera/mesh bounds or the mask."""

    BOUNDS = (0.1, 0.16, 0.9, 0.96)

    @staticmethod
    def picture(rectangles, *, channels=3, background=(6, 7, 8)):
        import struct
        import zlib

        width, height = 400, 500
        rows = [bytearray((*background, *([255] if channels == 4 else [])) * width)
                for _ in range(height)]
        for left, top, right, bottom, color in rectangles:
            pixel = bytes((*color, *([255] if channels == 4 else [])))
            for y in range(top, bottom):
                rows[y][left * channels:right * channels] = pixel * (right - left)

        def chunk(kind, payload):
            return (struct.pack(">I", len(payload)) + kind + payload
                    + struct.pack(">I", zlib.crc32(kind + payload)))

        return (b"\x89PNG\r\n\x1a\n"
                + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8,
                                             2 if channels == 3 else 6, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(b"".join(b"\0" + row for row in rows)))
                + chunk(b"IEND", b""))

    @staticmethod
    def figure(bottom=400, lower=(28, 22, 23)):
        # Head and torso are bright; both legs are visibly distinct from the
        # clear background but need not be bright or chromatic. Extent 120..399
        # is authored here, not derived from the production mask.
        return [(170, 120, 230, 170, (80, 100, 120)),
                (150, 170, 250, 280, (20, 90, 100)),
                (165, 280, 190, bottom, lower),
                (210, 280, 235, bottom, lower)]

    def native_result(self, data, bounds=None):
        from test_client_process import Native3DProcesses

        with tempfile.TemporaryDirectory(dir=ROOT / ".local") as temporary:
            path = Path(temporary) / "portrait.png"
            path.write_bytes(data)
            frame = {"png": str(path), "actors": [{"is_self": True, "target": "self"}],
                     "portrait": {"target": "self", "texture_size": [320, 400],
                                  "bounds": bounds or self.BOUNDS}}
            case = unittest.FunctionTestCase(lambda: Native3DProcesses.assert_live_portrait(case, frame, frame))
            result = unittest.TestResult()
            case.run(result)
            self.assertFalse(result.errors, result.errors)
            return result

    def test_native_scale_counts_dark_legs_not_just_colored_clothing(self):
        result = self.native_result(self.picture(self.figure()))
        self.assertFalse(result.failures, result.failures)

    def test_known_extent_is_independent_of_clothing_color_and_alpha(self):
        from render_pixels import portrait_pixels

        for lower in ((28, 22, 23), (28, 28, 28), (70, 80, 100)):
            for channels in (3, 4):
                with self.subTest(lower=lower, channels=channels):
                    colored, fraction = portrait_pixels(
                        self.picture(self.figure(lower=lower), channels=channels), self.BOUNDS)
                    self.assertGreater(colored, 1000)
                    # Every-other-row sampling sees 120..398 within a 400px pane.
                    self.assertAlmostEqual(fraction, 278 / 400)

    def test_empty_uniform_and_disconnected_noise_do_not_form_a_character(self):
        from render_pixels import portrait_pixels

        noise = [(x, y, x + 4, y + 4, (80, 100, 120))
                 for y in range(100, 450, 12) for x in range(60, 340, 12)]
        # Both borders touch the sampling perimeter; the narrow caption and
        # outer frame are entirely outside the reported image pane.
        border = [(48, 90, 352, 96, (80, 100, 120)),
                  (48, 464, 352, 470, (80, 100, 120)),
                  (48, 90, 54, 470, (80, 100, 120)),
                  (346, 90, 352, 470, (80, 100, 120))]
        caption = [(10, 20, 390, 60, (80, 100, 120)),
                   (10, 480, 390, 495, (80, 100, 120))]
        for name, data in (
                ("empty", self.picture([])),
                ("uniform", self.picture([], background=(80, 100, 120))),
                ("border", self.picture(border)),
                ("caption", self.picture(caption)),
                ("noise", self.picture(noise))):
            with self.subTest(name=name):
                colored, fraction = portrait_pixels(data, self.BOUNDS)
                self.assertEqual(fraction, 0)
                if name in ("uniform", "noise", "border"):
                    self.assertGreater(colored, 1000, "not merely an empty-image rejection")
                self.assertTrue(self.native_result(data).failures)

    def test_disconnected_marks_cannot_enlarge_an_undersized_body(self):
        from render_pixels import portrait_pixels

        body = [(150, 190, 250, 310, (20, 90, 100))]
        marks = [(180, 100, 220, 130, (80, 100, 120)),
                 (180, 410, 220, 440, (28, 22, 23))]
        colored, fraction = portrait_pixels(self.picture(body + marks), self.BOUNDS)
        self.assertGreater(colored, 1000)
        self.assertAlmostEqual(fraction, 118 / 400)
        self.assertTrue(self.native_result(self.picture(body + marks)).failures)

    def test_native_scale_keeps_the_strict_point_52_boundary(self):
        from render_pixels import portrait_pixels

        # These bounds yield exactly 400 in the native floating denominator.
        # Known sampled endpoints are 120..326/328/330, not inferred by the mask.
        bounds = (0.1, 0.125, 0.9, 0.925)
        for bottom, expected, accepted in ((327, 0.515, False), (329, 0.52, False),
                                           (331, 0.525, True)):
            with self.subTest(bottom=bottom):
                data = self.picture(self.figure(bottom=bottom))
                colored, fraction = portrait_pixels(data, bounds)
                self.assertGreater(colored, 1000)
                self.assertEqual(fraction, expected)
                self.assertEqual(not self.native_result(data, bounds).failures, accepted)

    def test_translated_pane_excludes_other_ui_and_keeps_the_same_figure(self):
        from render_pixels import portrait_pixels

        translated = [(left + 20, top - 10, right + 20, bottom - 10, color)
                      for left, top, right, bottom, color in self.figure()]
        translated += [(0, 0, 400, 50, (80, 100, 120))]
        colored, fraction = portrait_pixels(self.picture(translated), (0.15, 0.14, 0.95, 0.94))
        self.assertGreater(colored, 1000)
        self.assertAlmostEqual(fraction, 278 / 400)

    def test_invalid_or_empty_insets_are_rejected(self):
        from render_pixels import portrait_pixels

        data = self.picture([])
        for bounds in ((-0.1, 0, 1, 1), (0, 0, 1.1, 1), (0.5, 0, 0.5, 1),
                       (0, 0, 0.01, 0.01), (0, float("nan"), 1, 1)):
            with self.subTest(bounds=bounds), self.assertRaises(ValueError):
                portrait_pixels(data, bounds)

    def test_background_and_foreground_contrast_cutoffs_are_explicit(self):
        from render_pixels import portrait_pixels

        for background, expected in (((20, 20, 20), 278 / 400), ((21, 21, 21), 0)):
            with self.subTest(background=background):
                _, fraction = portrait_pixels(
                    self.picture(self.figure(lower=(40, 40, 40)), background=background), self.BOUNDS)
                self.assertAlmostEqual(fraction, expected)
        # The default background is (6,7,8): exact 12-channel contrast is not
        # foreground; 13 is. The colored upper body is unchanged in both cases.
        for lower, expected in (((18, 19, 20), 158 / 400), ((19, 20, 21), 278 / 400)):
            with self.subTest(lower=lower):
                _, fraction = portrait_pixels(self.picture(self.figure(lower=lower)), self.BOUNDS)
                self.assertAlmostEqual(fraction, expected)

    def test_component_seed_cutoff_and_multiple_bodies_cannot_union(self):
        from render_pixels import portrait_pixels

        gray = [(left, top, right, bottom, (28, 28, 28))
                for left, top, right, bottom, _ in self.figure()]
        # 50 columns × 20 rows on the two-pixel sampling grid, then one sample.
        patch = [(150, 170, 250, 210, (20, 90, 100))]
        extra = [(150, 210, 151, 211, (20, 90, 100))]
        for added, expected_count, expected_span in (([], 1000, 0), (extra, 1001, 278 / 400)):
            with self.subTest(seeds=expected_count):
                count, span = portrait_pixels(self.picture(gray + patch + added), self.BOUNDS)
                self.assertEqual(count, expected_count)
                self.assertAlmostEqual(span, expected_span)
        # Both components independently exceed 1000 seeds, but neither meets
        # scale. Counting their combined top-to-bottom range would falsely pass.
        separate = [(150, 120, 250, 230, (20, 90, 100)),
                    (150, 320, 250, 430, (20, 90, 100))]
        count, span = portrait_pixels(self.picture(separate), self.BOUNDS)
        self.assertGreater(count, 2000)
        self.assertAlmostEqual(span, 108 / 400)
        self.assertTrue(self.native_result(self.picture(separate)).failures)


if __name__ == "__main__":
    unittest.main()
