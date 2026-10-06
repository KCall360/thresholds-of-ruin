"""Shared harness for the actual-process acceptance tests (`test_*_process.py`).

Every process test launches the real `tor-server` and real clients. This module
holds what they share: building and locating the binaries, launching and reading
processes, credentials, save barriers, and one helper set per client.

`ProcessTestCase` is the base class. Its helpers are grouped by client:

- **server**: `server()` starts `tor-server` on a free port and sets `address`.
- **headless** (the automation client): `client()`, `frame()`, `command()`,
  `request()`, `act()`, and `wizard()` / `wizard_command()` for privileged setup.
- **text**: `text_client()` for the scripted line interface.
- **adventure**: `adventure()`, `say()` and `send()` for the interactive prompt.
- **ASCII**: `window()`, `ascii_frame()`, `key()` and `native_keys()`.

Set `graphical = True` on a class that opens native windows; on Linux it then
requires a display (CI uses `xvfb-run`), and a missing display is a failure.
Use the headless client for setup and driving unless the behavior under test is
another client's input or presentation (see docs/testing.md).
"""
import json
import os
from pathlib import Path
import queue
import shutil
import sqlite3
import subprocess
import threading
import time
import unittest
import uuid
from contextlib import closing

ROOT = Path(__file__).resolve().parents[1]
SCENARIOS = ROOT / "scenarios"
TOKEN = "text-process-test-token-not-a-secret"
SPECTATOR_TOKEN = "spectator-process-test-token-not-a-secret"
WIZARD_TOKEN = "headless-wizard-test-token-not-a-secret"
SUFFIX = ".exe" if os.name == "nt" else ""

_binaries = None


def ascii_input_completion(key, *, wait_for_simulation=True):
    """Track request acknowledgement across later presented simulation frames."""
    acknowledged = False

    def complete(frame):
        nonlocal acknowledged
        acknowledged |= frame.get("input_done") == key
        return (acknowledged and not frame["busy"]
                and (not wait_for_simulation or not any(
                    status["phase"] == "queued" for status in (frame.get("intentions") or []))))

    return complete


def binaries():
    """Build every binary once per test run and return their directory."""
    global _binaries
    if _binaries is None:
        profile = os.environ.get("TOR_TEST_PROFILE", "debug")
        if profile not in ("debug", "release"):
            raise ValueError("TOR_TEST_PROFILE must be debug or release")
        command = ["cargo", "build", "--workspace", "--bins", "--locked"]
        if profile == "release":
            command.append("--release")
        subprocess.run(command, cwd=ROOT, check=True, timeout=600)
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"], cwd=ROOT,
        ))
        _binaries = Path(metadata["target_directory"]) / profile
    return _binaries


def package_path(name):
    """A scenario package by name: shipped packages first, then test packages."""
    path = Path(name)
    if path.is_absolute():
        return path
    shipped = SCENARIOS / name
    return shipped if (shipped / "scenario.toml").exists() else SCENARIOS / "tests" / name


def load_fixture(name):
    """A versioned process-test fixture from `scripts/fixtures/`."""
    return json.loads((Path(__file__).parent / "fixtures" / name).read_text(encoding="utf-8"))


def door(frame):
    """The only visible door in a frame's observation."""
    return next(c["door"] for c in frame["state"]["observation"]["visible_cells"] if c.get("door"))


class ProcessTestDirectory:
    """Temporary directory whose permissions are inherited by child processes."""

    def __init__(self):
        root = ROOT / "target" / "process-tests"
        root.mkdir(parents=True, exist_ok=True)
        self.name = str(root / uuid.uuid4().hex)
        Path(self.name).mkdir()

    def cleanup(self):
        shutil.rmtree(self.name, ignore_errors=True)


class Process:
    """A real child process whose output lines are read on a background thread."""

    def __init__(self, executable, args, token=TOKEN, extra_env=None, *, separate_stderr=False):
        environment = {k: v for k, v in os.environ.items() if k not in ("TOR_SPECTATOR_TOKEN", "TOR_WIZARD_TOKEN")}
        environment.update(extra_env or {})
        self.executable = Path(executable)
        if self.executable.stem == "tor-server" and "--scenario" not in args and "--regions" not in args:
            args = [*args, "--scenario", SCENARIOS / "two-room"]
        self.token = token
        # A server started for another character is saved through that actor.
        args = list(args)
        self.character = str(args[args.index("--character") + 1]) if "--character" in args else None
        self.child = subprocess.Popen(
            [str(executable), *map(str, args)], cwd=ROOT,
            env={**environment, "TOR_SERVER_TOKEN": token},
            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE if separate_stderr else subprocess.STDOUT,
            text=True, encoding="utf-8", bufsize=1,
        )
        self.lines = queue.Queue()
        self.transcript = []
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def _read(self):
        try:
            for line in self.child.stdout:
                self.lines.put(line.rstrip("\n"))
        finally:
            self.lines.put(None)

    def until(self, predicate, seconds=15):
        deadline = time.monotonic() + seconds
        output = []
        while True:
            try:
                line = self.lines.get(timeout=max(0, deadline - time.monotonic()))
            except queue.Empty:
                raise AssertionError(f"Process output deadline: {self.transcript}") from None
            if line is None:
                raise AssertionError(f"Unexpected process exit: {self.transcript}")
            self.transcript.append(line)
            output.append(line)
            if predicate(line):
                return "\n".join(output)

    def write(self, line):
        self.child.stdin.write(line + "\n")
        self.child.stdin.flush()

    def command(self, command):
        """A text-client command, answered once the client prints `Ready.`."""
        self.write(command)
        return self.until(lambda line: line == "Ready.")

    def stop(self):
        try:
            if self.child.poll() is None:
                try:
                    # Restart tests need a durable boundary. Crash-loss tests
                    # deliberately kill the child directly instead.
                    if self.executable.stem == "tor-server":
                        ready = next((json.loads(line) for line in self.transcript if line.startswith('{') and '"address"' in line), None)
                        if ready:
                            save_at(ready["address"], self.token, self.character)
                finally:
                    self.child.kill()
        finally:
            self.child.wait(timeout=10)
            self.reader.join(timeout=10)
            for stream in (self.child.stdin, self.child.stdout):
                stream.close()
            if self.child.stderr is not None:
                self.child.stderr.close()


class AdventureProcess(Process):
    """The text client's interactive prompt, read character by character."""

    def _read(self):
        # The interactive prompt is flushed without a newline. Read characters
        # so acceptance tests verify its actual bytes instead of requiring the
        # application to put the cursor on another line for the test harness.
        pending = ""
        try:
            while character := self.child.stdout.read(1):
                pending += character
                if pending == "> " or character == "\n":
                    self.lines.put(pending.removesuffix("\n"))
                    pending = ""
            if pending:
                self.lines.put(pending)
        finally:
            self.lines.put(None)


def save_at(address, token=TOKEN, actor=None):
    """Explicit save barrier through a real headless client; no ordinary-action timing."""
    result = subprocess.run(
        [str(binaries() / ("tor-client-headless" + SUFFIX)), "--connect", address, "--observe",
         *(["--actor", actor] if actor else [])],
        input=json.dumps({"type": "request", "request": {"type": "save"}}) + "\n" + json.dumps({"type": "quit"}) + "\n",
        text=True, capture_output=True, timeout=35, cwd=ROOT, env={**os.environ, "TOR_SERVER_TOKEN": token})
    if result.returncode or any(json.loads(line).get("error") for line in result.stdout.splitlines() if line.startswith("{")):
        raise AssertionError(f"Explicit save failed: {result.stdout} {result.stderr}")


def inspect_save(path):
    """Inspect immutable base metadata only; this does not replay journal rows."""
    with closing(sqlite3.connect(path)) as conn:
        frame = conn.execute("SELECT frame FROM journal WHERE sequence=0").fetchone()[0]
    return json.loads(frame[24:])["archive"]


class ProcessTestCase(unittest.TestCase):
    """Base class for process tests; see the module documentation."""

    graphical = False

    @classmethod
    def setUpClass(cls):
        if cls.graphical and os.name != "nt" and not os.environ.get("DISPLAY"):
            raise RuntimeError("Graphical process tests require DISPLAY; run under xvfb-run on Linux")
        cls.bin = binaries()
        cls.suffix = SUFFIX

    def setUp(self):
        directory = ProcessTestDirectory()
        self.addCleanup(directory.cleanup)
        self.directory = Path(directory.name)
        self.save = self.directory / "game.db"

    def launch(self, name, args, cls=Process, **kwargs):
        process = cls(self.bin / (name + self.suffix), args, **kwargs)
        self.addCleanup(process.stop)
        return process

    def flush_save(self):
        save_at(self.address)

    # Server.

    def server(self, *args, wizard=False, scenario=None, character=None, spectator=True, seed=42,
               extra_env=None, separate_stderr=False):
        """Start `tor-server` on a free port and set `self.address`.

        `scenario` is a package name (see `package_path()`) or a path; extra
        command-line arguments are passed through.
        """
        env = dict(extra_env or {})
        if spectator:
            env["TOR_SPECTATOR_TOKEN"] = SPECTATOR_TOKEN
        if wizard:
            env["TOR_WIZARD_TOKEN"] = WIZARD_TOKEN
        server = self.launch("tor-server", [
            "--listen", "127.0.0.1:0", "--save", self.save,
            *(["--seed", str(seed)] if seed is not None else []),
            *(["--wizard"] if wizard else []),
            *(["--scenario", package_path(scenario)] if scenario else []),
            *(["--character", str(character)] if character else []),
            *args], extra_env=env, separate_stderr=separate_stderr)
        self.address = json.loads(server.until(lambda line: line.startswith("{")))["address"]
        return server

    # Headless client.

    def frame(self, client, predicate, seconds=15):
        """The next headless JSON frame that satisfies `predicate`."""
        found = []

        def match(line):
            if not line.startswith("{"):
                return False
            value = json.loads(line)
            if predicate(value):
                found.append(value)
                return True
            return False
        client.until(match, seconds)
        return found[-1]

    def client(self, token=TOKEN, observe=False, actor=None):
        client = self.launch("tor-client-headless", [
            "--connect", self.address, *(["--observe"] if observe else []),
            *(["--actor", str(actor)] if actor else [])], token=token)
        return client, self.frame(client, lambda frame: frame["type"] == "ready")

    def command(self, client, value):
        client.write(json.dumps(value))
        return self.frame(client, lambda frame: frame["type"] == "ready")

    def request(self, client, request):
        return self.command(client, {"type": "request", "request": request})

    def act(self, client, action):
        accepted = self.command(client, {"type": "act", "action": action})
        if accepted.get("error"):
            return accepted
        pending = next((status for status in accepted.get("intentions", [])
                        if status["phase"] == "queued"), None)
        if pending is None:
            return accepted
        # Request acknowledgement is admission. Assert action effects only after
        # the matching ordered lifecycle update from simulation execution.
        executed = self.frame(client, lambda frame: (
            (frame.get("message") or {}).get("type") == "update"
            and frame["message"]["update"]["body"]["type"] == "intention"
            and frame["message"]["update"]["body"]["status"]["intention"] == pending["intention"]
            and frame["message"]["update"]["body"]["status"]["phase"] in
                ("started", "resolved", "failed", "cancelled", "suspended")))
        phase = executed["message"]["update"]["body"]["status"]["phase"]
        if phase == "started":
            return executed
        # Completed/suspended work changes the available controls. Return after
        # that ordered permission update, so callers can capture a fresh context.
        revision = int(executed["readiness"]["revision"])
        return self.frame(client, lambda frame: int(frame["readiness"]["revision"]) > revision)

    def play(self, client, steps):
        """Play fixture steps as ordinary actions: `{"move": direction}` or `{"take": item name}`."""
        frame = None
        for step in steps:
            if "move" in step:
                frame = self.act(client, {"type": "move", "direction": step["move"]})
            else:
                state = frame or self.request(client, {"type": "snapshot"})
                item = next(i["item"]["id"] for i in state["state"]["observation"]["ground_items"] if i["item"]["name"] == step["take"])
                frame = self.act(client, {"type": "take", "item": item})
            self.assertIsNone(frame["error"], step)
        return frame

    def wizard(self):
        """A headless wizard for privileged setup; it never takes control of the actor."""
        wizard, ready = self.client(WIZARD_TOKEN, observe=True)
        self.assertIsNone(ready["error"])
        return wizard

    def wizard_command(self, wizard, command, succeeds=True):
        """Run one developer command (the text client's form, with or without `wizard `)."""
        frame = self.command(wizard, {"type": "wizard", "command": command.removeprefix("wizard ")})
        if succeeds is not None:
            (self.assertIsNone if succeeds else self.assertIsNotNone)(frame["error"], command)
        return frame

    # Text client.

    def text_client(self, token=TOKEN, observe=False):
        client = self.launch("tor-client-text", ["--script", "--connect", self.address, *(["--observe"] if observe else [])], token=token)
        welcome = client.until(lambda line: line == "Ready.")
        self.assertNotIn(TOKEN, welcome)
        return client, welcome

    # Adventure interface.

    def adventure(self, token=TOKEN):
        process = self.launch("tor-client-text", ["--connect", self.address], cls=AdventureProcess, token=token)
        return process, process.until(lambda line: line == "> ")

    def say(self, process, text):
        process.write(text)
        return process.until(lambda line: line == "> ")

    def send(self, process, text):
        process.write(text)

    # Native ASCII client.

    def window(self, *args, token=TOKEN, observe=False):
        """A native ASCII window under automation, once it presents a settled state."""
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--automation",
                                                  *(["--observe"] if observe else []), *args], token=token)
        frame = self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"])
        self.assertTrue(frame["window_open"])
        return window, frame

    def ascii_frame(self, process, predicate, seconds=20):
        """The next presented ASCII frame that satisfies `predicate`."""
        return self.frame(process, lambda f: f.get("type") == "frame" and predicate(f), seconds)

    def key(self, process, key, *, wait_for_simulation=True):
        """Inject an input event through the window's automation channel."""
        process.write(json.dumps({"type": "key", "key": key}))
        return self.ascii_frame(process, ascii_input_completion(
            key, wait_for_simulation=wait_for_simulation))

    def native_keys(self, client):
        """Send genuine OS key events to the client's only visible window."""
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
                pid = wintypes.DWORD()
                user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
                if pid.value == client.child.pid and user32.IsWindowVisible(hwnd):
                    handles.append(hwnd)
                return True
            user32.EnumWindows(find, 0)
            self.assertEqual(len(handles), 1)

            def key(name, down):
                vk = {"o": 0x4F, "c": 0x43, "Right": 0x27, "Up": 0x26, "Escape": 0x1B, "y": 0x59, "u": 0x55, "b": 0x42,
                      "n": 0x4E, "F4": 0x73, "F8": 0x77, "F9": 0x78, "Shift_L": 0x10, "comma": 0xBC, "period": 0xBE, "g": 0x47}[name]
                scan = user32.MapVirtualKeyW(vk, 0)
                self.assertTrue(user32.PostMessageW(handles[0], 0x100 if down else 0x101, vk,
                    1 | (scan << 16) | (0x01000000 if name in ("Up", "Right") else 0) | (0 if down else 0xC0000000)))
            return key
        windows = subprocess.check_output(["xdotool", "search", "--onlyvisible", "--name", r"^Thresholds of Ruin \| ASCII$"],
                                          text=True, timeout=10).split()
        self.assertEqual(len(windows), 1)

        def key(name, down):
            subprocess.run(["xdotool", "keydown" if down else "keyup", "--window", windows[0], name], check=True, timeout=10)
        return key
