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
another client's input or presentation (see docs/development/testing.md).
"""
import gzip
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
from contextlib import closing, contextmanager
from collections import deque
from collections.abc import Sequence
from itertools import islice

ROOT = Path(__file__).resolve().parents[1]
SCENARIOS = ROOT / "scenarios"
TOKEN = "text-process-test-token-not-a-secret"
SPECTATOR_TOKEN = "spectator-process-test-token-not-a-secret"
WIZARD_TOKEN = "headless-wizard-test-token-not-a-secret"
SUFFIX = ".exe" if os.name == "nt" else ""

_binaries = None


DIRECTIONS = [(-1, 0, "west"), (1, 0, "east"),
              (0, -1, "north"), (0, 1, "south")]


def underfoot(state):
    return next(cell for cell in state["observation"]["visible_cells"]
                if cell["position"] == {"x": 0, "y": 0, "z": 0})


def disclosed_exploration_route(at, floor, stairs, visited):
    """Prefer reachable stairs; otherwise discover their undisclosed approach."""
    route = None
    # Seeing a stair across a wall does not disclose its connecting route.
    # Prefer a known route to it, then keep exploring reachable frontiers.
    for seek_stair in [True, False]:
        queue = deque([(at, [])])
        seen = {at}
        while queue:
            here, path = queue.popleft()
            if here != at and (here in stairs if seek_stair else here not in visited):
                route = path
                break
            for dx, dy, direction in DIRECTIONS:
                neighbor = (here[0] + dx, here[1] + dy)
                if neighbor in floor and neighbor not in seen:
                    seen.add(neighbor)
                    queue.append((neighbor, path + [(neighbor, direction)]))
        if route is not None:
            return route
    return None


def explore_to_stairs(test, player, state, direction="down"):
    """Choose routes from observations, without reading generated terrain."""
    stair_field = "stairs_" + direction
    at = (0, 0)
    visited = set()
    floor = set()
    stairs = set()
    for _ in range(2500):
        visited.add(at)
        for cell in state["observation"]["visible_cells"]:
            position = cell["position"]
            if position["z"] != 0:
                continue
            here = (at[0] + position["x"], at[1] + position["y"])
            if not cell["wall"]:
                floor.add(here)
                if cell[stair_field]:
                    stairs.add(here)
        if underfoot(state)[stair_field]:
            return state
        route = disclosed_exploration_route(at, floor, stairs, visited)
        test.assertIsNotNone(route, "No disclosed route to unexplored terrain or stairs")
        target, direction = route[0]
        before = underfoot(state)["key"]
        response = test.act(player, {"type": "move", "direction": direction})
        test.assertIsNone(response["error"])
        state = response["state"]
        test.assertNotEqual(underfoot(state)["key"], before, "Disclosed floor was not traversable")
        at = target
    test.fail("Exploration exceeded its bounded action count")


def ascii_input_completion(key, *, wait_for_simulation=True, wait_for_ready=False):
    """Track acknowledgement, optionally through ordinary actor readiness.

    Most UI tests intentionally stop once a queued intention starts. Sequential
    gameplay input must also wait for that intention and recovery to settle.
    A paused intention or death remains observable instead of timing out here.
    """
    acknowledged = False

    def complete(frame):
        nonlocal acknowledged
        acknowledged |= frame.get("input_done") == key
        phases = {status["phase"] for status in (frame.get("intentions") or [])}
        if not acknowledged or frame["busy"]:
            return False
        if wait_for_ready:
            observation = (frame.get("state") or {}).get("observation")
            if observation is None:
                return False
            combat = observation["combat"]
            return combat["dead"] or (observation["ready"]
                and not combat["preparation_active"]
                and not phases.intersection({"queued", "started"}))
        return not wait_for_simulation or "queued" not in phases

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
        started = time.monotonic()
        outcome, returncode = "error", None
        try:
            subprocess.run(command, cwd=ROOT, check=True, timeout=600)
            outcome, returncode = "passed", 0
        except subprocess.CalledProcessError as error:
            outcome, returncode = "failed", error.returncode
            raise
        except subprocess.TimeoutExpired:
            outcome = "timeout"
            raise
        finally:
            if path := os.environ.get("TOR_TEST_TIMINGS"):
                destination = Path(path)
                destination.parent.mkdir(parents=True, exist_ok=True)
                with destination.open("a", encoding="utf-8") as log:
                    log.write(json.dumps({"schema_version": 1, "kind": "build", "profile": profile,
                        "seconds": time.monotonic() - started, "outcome": outcome,
                        "returncode": returncode}) + "\n")
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"], cwd=ROOT,
        ))
        _binaries = Path(metadata["target_directory"]) / profile
    return _binaries


def creature_package(test, name, *, hostile=False, abilities=("power_strike", "magic_bolt", "fear"), ability_aliases=None):
    package = test.directory / name
    shutil.copytree(ROOT / "scenarios/two-room", package)
    from arena_matrix import toml_source
    from ruleset_sources import load_rules
    rules_path = package / "rules.toml"
    rules = load_rules(rules_path, test.bin / ("tor-scenario" + test.suffix))
    # This fixture tests explicit grants; universal care has its own finite-supply fixture.
    rules["constants"]["default_abilities"] = ["basic_melee"]
    rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
    if ability_aliases:
        import copy
        import tomllib
        from arena_matrix import toml_source
        rules_path = package / "rules.toml"
        rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
        for identity, (template, display_name) in ability_aliases.items():
            definition = copy.deepcopy(next(value for value in rules["abilities"] if value["id"] == template))
            definition.update(id=identity, name=display_name, basic_attack=False)
            rules["abilities"].append(definition)
        rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
    manifest = package / "scenario.toml"
    source = manifest.read_text(encoding="utf-8")
    build = ('creature = { species = "human", name = "trainee", faction = "neutral", '
             'binding = "intellect", starting_skills = ["heavy_weaponry", "spellcasting"], '
             'restoration = "choose_again", hit_dice = [{ source = "warrior" }, { source = "mage", '
             'purchases = [{ pool = "advancement", selection = { kind = "skill_rank", skill = "intimidation" } }] }] }, ')
    source = source.replace('characters = [{ ', 'characters = [{ ' + build)
    if hostile:
        source = source.replace('"turn_ticks" = 100', '"turn_ticks" = 10')
        source += '\nfactions = { neutral = [], foe = ["neutral"] }\n'
    modifiers = ", ".join([
        *('{ scope = "always", op = { kind = "grant_ability", ability = "' + ability + '" } }' for ability in abilities),
        '{ scope = "always", op = { kind = "flag", flag = "magical" } }',
        '{ scope = "always", op = { kind = "add", stat = { kind = "health" }, value = 200 } }',
    ])
    source += ('\nai_profiles = { practice = { memory_ticks = 100, flee_percent = 0 } }\n'
               '[creatures.species.human]\nkind = "humanoid"\n'
               'attributes = { strength = 2, speed = 1, intellect = 2, '
               'willpower = 2, awareness = 1, presence = 1 }\n'
               'melee = { skill = "heavy_weaponry", bonus = 0, wind_up = 60, recovery = 40, strikes = [{ offset = 0, check = { actor = "user", skill = { kind = "selected_weapon" }, against = { kind = "defense", defense = "physical" } }, on_success = [{ kind = "damage", damage = { kind = "components", components = [{ category = "impact", amount = { kind = "rolled", dice = { count = 1, sides = 6, bonus = 0 } } }] } }], on_failure = [], descriptors = [], riders = [] }] }\n'
               'modifiers = [' + modifiers + ']\n')
    manifest.write_text(source, encoding="utf-8", newline="\n")
    region = package / "regions/1.toml"
    source = region.read_text(encoding="utf-8")
    source = "\n".join(line for line in source.splitlines() if not line.startswith("items =")) + "\n"
    target_x = 4 if hostile and abilities == ("magic_bolt",) else 2
    faction = "foe" if hostile else "neutral"
    source += ('\n[[actors]]\nid = 2\nat = [' + str(target_x) + ', 1, 0]\ncontroller = "ai"\nai = "practice"\n'
               'creature = { species = "human", name = "practice target", '
               'faction = "' + faction + '", binding = "intellect", '
               'starting_skills = ["athletics", "lore"], restoration = "authored", '
               'hit_dice = [{ source = "racial" }] }\n')
    region.write_text(source, encoding="utf-8", newline="\n")
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=15)
    test.assertEqual(result.returncode, 0, result.stderr)
    return package


def resource(frame, name):
    stats = frame["state"]["observation"]["combat"]["own_stats"]
    return next(pool for pool in stats["resources"] if pool["resource"]["id"] == name)



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

    @staticmethod
    def compress_logs(directory):
        # Preserve complete text without truncating saves or mutating SQLite.
        for path in directory.rglob("*"):
            if path.suffix in (".log", ".jsonl") and path.stat().st_size > 1024 * 1024:
                compressed = path.with_name(path.name + ".gz")
                if compressed.exists():
                    raise FileExistsError(compressed)
                with path.open("rb") as original, gzip.open(compressed, "wb") as output:
                    shutil.copyfileobj(original, output)
                path.unlink()

    def cleanup(self, *, failed=False, test_id=None):
        if not failed:
            shutil.rmtree(self.name, ignore_errors=True)
            return
        source = Path(self.name)
        metadata = {"test_id": test_id, "original_directory": str(source),
                    "profile": os.environ.get("TOR_TEST_PROFILE", "debug")}
        (source / "test.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
        if root := os.environ.get("TOR_TEST_ARTIFACTS"):
            destination = Path(root) / source.name
            staging = destination.with_name(f".{source.name}.partial-{uuid.uuid4().hex}")
            try:
                destination.parent.mkdir(parents=True, exist_ok=True)
                if destination.exists():
                    raise FileExistsError(destination)
                # Python traceback locals can retain SQLite handles on Windows.
                # Copy bytes and sidecars first; do not close/checkpoint/save the
                # database or rely on renaming its still-open source directory.
                shutil.copytree(source, staging)
                self.compress_logs(staging)
                staging.rename(destination)
            except Exception:
                shutil.rmtree(staging, ignore_errors=True)
                print(f"Artifact copy failed; original evidence unchanged: {source}", flush=True)
                raise
            self.name = str(destination)
            try:
                shutil.rmtree(source)
            except OSError as error:
                # A complete published copy is usable even when a locked source
                # cannot be fully removed. Report both locations, not lost data.
                metadata["source_cleanup_error"] = str(error)
                (destination / "test.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
                print(f"Retained evidence: {destination}; source cleanup incomplete: {source}: {error}", flush=True)
                return
        else:
            self.compress_logs(source)
        print(f"Retained failing process test: {self.name}", flush=True)


class Transcript(Sequence):
    """Consumed output on disk; only a bounded diagnostic tail stays in memory.

    Sequence offsets remain stable for existing before/after message assertions.
    Full iteration and slicing are explicit reads, not permanently retained frames.
    """
    def __init__(self, path):
        self.path = path
        path.touch()
        self.count = 0
        self.tail = deque()
        self.tail_chars = 0

    def append(self, line):
        self.count += 1
        self.tail.append(line[-8192:])
        self.tail_chars += len(self.tail[-1])
        while self.tail_chars > 65536 or len(self.tail) > 32:
            self.tail_chars -= len(self.tail.popleft())

    def __len__(self):
        return self.count

    def __iter__(self):
        with self.path.open(encoding="utf-8") as log:
            for line in islice(log, self.count):
                yield line.removesuffix("\n")

    def __getitem__(self, index):
        if isinstance(index, slice):
            start, stop, step = index.indices(self.count)
            if step > 0:
                return list(islice(self, start, stop, step))
            selected = {n: line for n, line in enumerate(self) if n in range(start, stop, step)}
            return [selected[n] for n in range(start, stop, step)]
        if index < 0:
            index += self.count
        if not 0 <= index < self.count:
            raise IndexError(index)
        return next(line for n, line in enumerate(self) if n == index)

    def __add__(self, other):
        return list(self) + other


class OutputQueue:
    """An append-only disk spool avoids memory growth or reader backpressure."""
    def __init__(self, path):
        self.path = path
        path.touch()
        self.condition = threading.Condition()
        # Separate owned handles keep FIFO position stable without thousands of
        # file-open operations. Line buffering publishes each complete record.
        self.writer = path.open("a", encoding="utf-8", newline="\n", buffering=1)
        self.reader = path.open(encoding="utf-8")
        self.offset = 0
        self.count = 0
        self.eof = False

    def put(self, line):
        with self.condition:
            if line is None:
                self.eof = True
            else:
                self.writer.write(line + "\n")
                self.count += 1
            self.condition.notify()

    def get(self, timeout):
        with self.condition:
            if not self.condition.wait_for(lambda: self.count or self.eof, timeout):
                raise queue.Empty
            if not self.count:
                return None
            line = self.reader.readline().removesuffix("\n")
            self.offset = self.reader.tell()
            self.count -= 1
            return line

    def close(self):
        self.writer.close()
        self.reader.close()

    def __enter__(self):
        return self

    def __exit__(self, *error):
        self.close()

    def read_between(self, start, stop):
        with self.path.open(encoding="utf-8") as log:
            log.seek(start)
            rows = []
            while log.tell() < stop:
                rows.append(log.readline().removesuffix("\n"))
            return "\n".join(rows)

    @property
    def queue(self):
        # Compatibility for explicit pending-output diagnostics and wire samples.
        with self.condition, self.path.open(encoding="utf-8") as log:
            log.seek(self.offset)
            return [line.removesuffix("\n") for line in log] + ([None] if self.eof else [])


class Process:
    """A real child process whose output lines are read on a background thread."""

    def __init__(self, executable, args, token=TOKEN, extra_env=None, *, separate_stderr=False, diagnostics=None):
        environment = {k: v for k, v in os.environ.items() if k not in ("TOR_SPECTATOR_TOKEN", "TOR_WIZARD_TOKEN")}
        environment.update(extra_env or {})
        self.executable = Path(executable)
        if self.executable.stem == "tor-server" and "--scenario" not in args and "--regions" not in args:
            args = [*args, "--scenario", SCENARIOS / "two-room"]
        self.token = token
        # A server started for another character is saved through that actor.
        args = list(args)
        self.character = str(args[args.index("--character") + 1]) if "--character" in args else None
        self.diagnostics = Path(diagnostics) if diagnostics else ROOT / "target" / "process-tests" / ("standalone-" + uuid.uuid4().hex)
        self.diagnostics.mkdir(parents=True, exist_ok=True)
        self.stdout_path = self.diagnostics / "stdout.log"
        if self.executable.stem == "tor-server":
            environment.setdefault("TOR_TEST_STARTUP_LOG", str(self.diagnostics / "startup.jsonl"))
        self.startup_path = Path(environment["TOR_TEST_STARTUP_LOG"]) if self.executable.stem == "tor-server" else None
        self.failed = False
        self._stopped = False
        self.metadata = {"command": [str(executable), *map(str, args)],
                         "profile": os.environ.get("TOR_TEST_PROFILE", "debug"),
                         "started_at": time.time(), "cwd": str(ROOT),
                         "separate_stderr": separate_stderr}
        self.child = subprocess.Popen(
            self.metadata["command"], cwd=ROOT,
            env={**environment, "TOR_SERVER_TOKEN": token},
            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE if separate_stderr else subprocess.STDOUT,
            text=True, encoding="utf-8", bufsize=1,
        )
        self.metadata["pid"] = self.child.pid
        self._write_status("launched")
        self.lines = OutputQueue(self.stdout_path)
        self.transcript = Transcript(self.stdout_path)
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def _record_line(self, line):
        self.lines.put(line)

    def _write_status(self, stage):
        self.metadata.setdefault("events", []).append({"stage": stage, "returncode": self.child.poll(), "at": time.time()})
        self.metadata.update(stage=stage, returncode=self.child.poll(), observed_at=time.time())
        (self.diagnostics / "process.json").write_text(json.dumps(self.metadata, indent=2) + "\n", encoding="utf-8")

    def _failure(self, message):
        self.failed = True
        self._write_status(message)
        return AssertionError(f"{message}; pid={self.child.pid} exit={self.child.poll()}; "
                              f"logs={self.diagnostics}; tail={list(self.transcript.tail)!r}")

    def _read(self):
        try:
            for line in self.child.stdout:
                self._record_line(line.rstrip("\n"))
        finally:
            self.lines.put(None)

    def until(self, predicate, seconds=15):
        deadline = time.monotonic() + seconds
        start = self.lines.offset
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise self._failure("Process output deadline")
            try:
                line = self.lines.get(timeout=remaining)
            except queue.Empty:
                raise self._failure("Process output deadline") from None
            if line is None:
                raise self._failure("Unexpected process exit")
            self.transcript.append(line)
            if time.monotonic() >= deadline:
                raise self._failure("Process output deadline")
            try:
                matched = predicate(line)
            except Exception:
                self.failed = True
                self._write_status("predicate raised")
                raise
            if time.monotonic() >= deadline:
                raise self._failure("Process output deadline")
            if matched:
                return self.lines.read_between(start, self.lines.offset)

    def write(self, line):
        self.child.stdin.write(line + "\n")
        self.child.stdin.flush()

    def command(self, command):
        """A text-client command, answered once the client prints `Ready.`."""
        self.write(command)
        return self.until(lambda line: line == "Ready.")

    def stop(self, *, save=True):
        if self._stopped:
            return
        self._write_status("before cleanup")
        try:
            if self.child.poll() is None:
                try:
                    # Restart tests need a durable boundary. Crash-loss tests
                    # deliberately kill the child directly instead.
                    if save and not self.failed and self.executable.stem == "tor-server":
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
                # A few acceptance tests deliberately leave this pipe unread to
                # test diagnostic backpressure. Drain only after killing, never
                # neutralize that precondition with a background reader.
                with (self.diagnostics / "stderr.log").open("w", encoding="utf-8") as log:
                    shutil.copyfileobj(self.child.stderr, log)
                self.child.stderr.close()
            self.lines.close()
            self._stopped = True
            self._write_status("stopped")


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
                    self._record_line(pending.removesuffix("\n"))
                    pending = ""
            if pending:
                self._record_line(pending)
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
        self.addCleanup(lambda: directory.cleanup(failed=self._failed(), test_id=self.id()))
        self.processes = []
        self.directory = Path(directory.name)
        self.save = self.directory / "game.db"

    def _failed(self):
        result = self._outcome.result if self._outcome else None
        failures = [] if result is None else [*result.errors, *result.failures]
        return any(test is self or getattr(test, "test_case", None) is self for test, _ in failures)

    def launch(self, name, args, cls=Process, *, executable=None, **kwargs):
        diagnostics = self.directory / f"process-{len(self.processes):02d}-{name}"
        process = cls(executable or self.bin / (name + self.suffix), args, diagnostics=diagnostics, **kwargs)
        self.processes.append(process)
        self.addCleanup(lambda: process.stop(save=not self._failed()))
        return process

    @contextmanager
    def held_save_writer(self):
        """Arm one persistence batch; tests must observe entry before asserting.

        The opt-in server watchdog is 30 seconds. Our entry and operation waits
        each have 15-second liveness bounds, not performance pass/fail targets.
        """
        base = self.directory / "save-barrier"
        entered = Path(str(base) + ".entered")
        released = Path(str(base) + ".release")
        finished = Path(str(base) + ".finished")
        error_detail = Path(str(base) + ".error.json")
        finished.unlink(missing_ok=True)
        error_detail.unlink(missing_ok=True)
        entered.unlink(missing_ok=True)
        released.unlink(missing_ok=True)
        armed = Path(str(base) + ".armed")
        def publish(path):
            # Windows cannot rename/delete a file held by Python's CRT handle.
            # Publish only after close, so the worker never observes that window.
            staged = path.with_name(path.name + ".pending-" + uuid.uuid4().hex)
            try:
                fd = os.open(staged, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
                os.close(fd)
                staged.replace(path)
            finally:
                staged.unlink(missing_ok=True)
        publish(armed)
        class Barrier:
            def wait(inner):
                deadline = time.monotonic() + 15
                while not entered.exists():
                    if time.monotonic() >= deadline:
                        self.fail("Save worker did not enter the armed persistence barrier")
                    time.sleep(.01)
                self.assertFalse(armed.exists())
            def assert_held(inner):
                self.assertTrue(entered.exists())
                self.assertFalse(released.exists())
                self.assertFalse(finished.exists(), "writer left barrier before responsiveness was established")
            def assert_released(inner):
                detail = error_detail.read_text(encoding="utf-8") if error_detail.exists() else None
                self.assertEqual(finished.read_text(encoding="utf-8"), "released", detail)
        try:
            yield Barrier()
        finally:
            # Always release before cleanup's save barrier, even on assertions.
            publish(released)
            armed.unlink(missing_ok=True)

    def flush_save(self):
        save_at(self.address)

    # Server.

    def server(self, *args, wizard=False, scenario=None, character=None, spectator=True, seed=42,
               extra_env=None, separate_stderr=False, writer_barrier=False):
        """Start `tor-server` on a free port and set `self.address`.

        `scenario` is a package name (see `package_path()`) or a path; extra
        command-line arguments are passed through.
        """
        env = {"TOR_TEST_STARTUP_LOG": str(self.directory / f"startup-{len(self.processes):02d}.jsonl"),
               **(extra_env or {})}
        if writer_barrier:
            env["TOR_TEST_SAVE_BARRIER"] = str(self.directory / "save-barrier")
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
        # This helper authors current valid requests. Raw command/request helpers
        # remain unchanged for missing-field and malformed-wire tests.
        if action.get("type") == "use_ability" and "supplies" not in action:
            action = {**action, "supplies": {"kind": "carried"}}
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

    def _native_window(self, client):
        """Find the native game window; fail unless it is unique."""
        self.assertIsNone(client.child.poll(), "native client exited before window lookup")
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes
            user32 = ctypes.WinDLL("user32", use_last_error=True)
            callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
            user32.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
            user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
            user32.IsWindowVisible.argtypes = [wintypes.HWND]
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
            return handles[0]
        # minifb 0.28 does not publish _NET_WM_PID. Native process tests run
        # serially; exact title plus uniqueness preserves the existing X11 gate.
        windows = subprocess.check_output(["xdotool", "search", "--onlyvisible",
            "--name", r"^Thresholds of Ruin \| ASCII$"], text=True, timeout=10).split()
        self.assertEqual(len(windows), 1)
        return windows[0]

    def native_text(self, client, text):
        """Send text through native window events, bypassing the JSON input driver."""
        handle = self._native_window(client)
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes
            user32 = ctypes.WinDLL("user32", use_last_error=True)
            user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
            encoded = text.encode("utf-16-le")
            for offset in range(0, len(encoded), 2):
                unit = int.from_bytes(encoded[offset:offset + 2], "little")
                self.assertTrue(user32.PostMessageW(handle, 0x102, unit, 1))
        else:
            subprocess.run(["xdotool", "type", "--window", handle, "--clearmodifiers", "--", text], check=True, timeout=10)

    def native_keys(self, client):
        """Send genuine OS key events to the client's only visible window."""
        handle = self._native_window(client)
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes
            user32 = ctypes.WinDLL("user32", use_last_error=True)
            user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
            def key(name, down):
                vk = {"o": 0x4F, "c": 0x43, "Right": 0x27, "Up": 0x26, "Escape": 0x1B, "y": 0x59, "u": 0x55, "b": 0x42,
                      "n": 0x4E, "F4": 0x73, "F7": 0x76, "F8": 0x77, "F9": 0x78, "F10": 0x79, "Shift_L": 0x10, "comma": 0xBC, "period": 0xBE, "g": 0x47,
                      "w": 0x57, "t": 0x54, "q": 0x51, "z": 0x5A, "2": 0x32, "Return": 0x0D, "Down": 0x28,
                      "Tab": 0x09, "space": 0x20, "bracketleft": 0xDB, "bracketright": 0xDD}[name]
                scan = user32.MapVirtualKeyW(vk, 0)
                self.assertTrue(user32.PostMessageW(handle, 0x100 if down else 0x101, vk,
                    1 | (scan << 16) | (0x01000000 if name in ("Up", "Down", "Right") else 0) | (0 if down else 0xC0000000)))
            return key
        def key(name, down):
            subprocess.run(["xdotool", "keydown" if down else "keyup", "--window", handle, name], check=True, timeout=10)
        return key


def content_ids(values):
    """Machine identities from a current disclosed content list."""
    return [value["id"] for value in values]
