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
- **ASCII**: `window()`, `ascii_frame()`, `key()`, `native_keys()` and `native_press()`.

Set `graphical = True` on a class that opens native windows. Mark an audited
headless/Text-only method in a mixed class with `@non_graphical`. Every other
method requires a display on Linux (CI uses `xvfb-run`); absence is a failure.
Native process launches also check the display regardless of test classification.
Use the headless client for setup and driving unless the behavior under test is
another client's input or presentation (see docs/development/testing.md).
"""
import gzip
import hashlib
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

from scalar_wire import u64

ROOT = Path(__file__).resolve().parents[1]
SCENARIOS = ROOT / "scenarios"
TOKEN = "text-process-test-token-not-a-secret"
SPECTATOR_TOKEN = "spectator-process-test-token-not-a-secret"
WIZARD_TOKEN = "headless-wizard-test-token-not-a-secret"
SUFFIX = ".exe" if os.name == "nt" else ""

_binaries = None


DIRECTIONS = [(-1, 0, "west"), (1, 0, "east"),
              (0, -1, "north"), (0, 1, "south")]


def non_graphical(method):
    """Declare an audited method in a graphical class needs no native window."""
    method._tor_non_graphical = True
    return method


def is_graphical_test(test):
    """Native classes default to requiring a display for every unmarked method."""
    method = getattr(test, test._testMethodName, None)
    return (getattr(type(test), "graphical", False) is True
            and getattr(method, "_tor_non_graphical", False) is not True)


def _require_native_display(environment):
    if os.name != "nt" and not environment.get("DISPLAY"):
        raise RuntimeError("Graphical process tests require DISPLAY; run under xvfb-run on Linux")


def read_capture(path):
    """Read one published framebuffer without interfering with replacement."""
    if os.name != "nt":
        with open(path, "rb") as source:
            return source.read()

    import ctypes
    from ctypes import wintypes
    import msvcrt

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                                    ctypes.c_void_p, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
    kernel32.CreateFileW.restype = wintypes.HANDLE
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel32.CloseHandle.restype = wintypes.BOOL

    def open_shared(filename, _flags):
        # Reading owns the old complete frame while the producer atomically
        # publishes a new one. Ordinary Python opens omit FILE_SHARE_DELETE.
        handle = kernel32.CreateFileW(
            os.fsdecode(filename), 0x80000000, 0x00000001 | 0x00000002 | 0x00000004,
            None, 3, 0x00000080, None)
        if handle == ctypes.c_void_p(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            # Success transfers ownership to the descriptor, then to open().
            return msvcrt.open_osfhandle(handle, os.O_RDONLY | os.O_BINARY)
        except BaseException as error:
            if not kernel32.CloseHandle(handle):
                error.add_note(f"Capture handle cleanup failed: {ctypes.WinError(ctypes.get_last_error())}")
            raise

    with open(path, "rb", opener=open_shared) as source:
        return source.read()


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


def settle_rogue_action(test, player, current, *, survive=True):
    """Settle one ordinary controlled Rogue action, resuming its paid work.

    This is walkthrough policy, not the general act() contract: positive Health
    loss may pause the existing preparation, and only disclosed permission may
    resume it. Death is returned only when the caller explicitly permits it.
    """
    identity = None
    resumes = 0
    last = current

    def statuses(frame):
        return [row for row in frame["intentions"]
                if row["phase"] in ("queued", "started", "paused", "suspended")]

    def diagnostic():
        view = (last.get("state") or {}).get("observation") or {}
        combat = view.get("combat") or {}
        return repr({"intention": identity, "resumes": resumes, "error": last.get("error"),
                     "ready": view.get("ready"), "terminal": combat.get("terminal"),
                     "preparation_remaining": combat.get("preparation_remaining"),
                     "preparation_active": combat.get("preparation_active"),
                     "readiness": last.get("readiness"), "intentions": last.get("intentions")})

    def remember_intention(frame):
        nonlocal identity
        pending = statuses(frame)
        test.assertLessEqual(len(pending), 1,
                             "Ambiguous ordinary Rogue intention: " + diagnostic())
        if pending:
            if identity is None:
                identity = pending[0]["intention"]
            test.assertEqual(pending[0]["intention"], identity,
                             "Ordinary Rogue intention changed: " + diagnostic())

    def stopped(frame):
        nonlocal last
        last = frame
        if frame.get("error") is not None:
            return True
        view = (frame.get("state") or {}).get("observation")
        if view is None:
            return False
        combat = view["combat"]
        if combat["terminal"]:
            return True
        if combat["preparation_remaining"] is not None:
            pending = statuses(frame)
            return (not combat["preparation_active"] and len(pending) == 1
                    and pending[0]["phase"] == "paused"
                    and pending[0]["intention"] in frame["readiness"]["resume"])
        # Resolution can precede the final permission delta. Old cancel/resume
        # targets still carry the previous input context, even with admission.
        # Wait for that boundary before the walkthrough sends another query.
        return (view["ready"] and frame["readiness"]["admission"]
                and not frame["readiness"]["resume"]
                and not frame["readiness"]["cancel"]
                and not any(row["phase"] in ("queued", "started")
                            for row in frame["intentions"]))

    while True:
        last = current
        test.assertIsNone(current["error"], diagnostic())
        remember_intention(current)
        if not stopped(current):
            try:
                current = test.frame(player, stopped)
            except AssertionError as error:
                test.fail(f"Ordinary Rogue action did not settle: {diagnostic()}; {error}")
        last = current
        test.assertIsNone(current["error"], diagnostic())
        test.assertFalse(current["state"]["wizard_game"], diagnostic())
        combat = current["state"]["observation"]["combat"]
        if combat["terminal"]:
            test.assertFalse(survive, "Ordinary Rogue walkthrough reached terminal state: " + diagnostic())
            return current
        remember_intention(current)
        if combat["preparation_remaining"] is None:
            return current
        test.assertLess(resumes, 16,
                        "Ordinary preparation did not finish within 16 explicit resumes: " + diagnostic())
        # The headless client targets its disclosed intention. There is exactly
        # one paused identity here, and its matching resume permission is known.
        resumes += 1
        current = test.command(player, {"type": "resume_intention"})


def ascii_stats_viewport_text(frame):
    """Join wrapped text only within the 24 actually presented inspection rows."""
    start = frame["screen"]["stats_scroll"]
    return " ".join(frame["stats_rows"][start:start + 24])


def headless_fight_completion(frame):
    """Wait for completed ordinary combat work before the next fixture action."""
    view = frame["state"]["observation"]
    readiness = frame["readiness"]
    # Observation and lifecycle arrive before their ordered permission update.
    # Even admission=True is stale while cancel still names resolved work.
    # A paused paid intention is not completed work to replace with a new attack.
    return view["combat"]["dead"] or (view["ready"]
        and not view["combat"]["preparation_active"]
        and not frame["intentions"] and readiness["admission"]
        and not readiness["resume"] and not readiness["cancel"])


def ascii_input_completion(key, *, wait_for_simulation=True, wait_for_ready=False,
                           wait_for_admission=False):
    """Track acknowledgement, optionally through ordinary actor readiness.

    Most UI tests intentionally stop once a queued intention starts. Sequential
    gameplay input must also wait for that intention and recovery to settle.
    A paused intention or death remains observable instead of timing out here.
    Sequential noncombat fixtures can additionally require the client's
    authoritative admission permission, which may arrive after the effect.
    """
    acknowledged = False

    def complete(frame):
        nonlocal acknowledged
        acknowledged |= frame.get("input_done") == key
        phases = {status["phase"] for status in (frame.get("intentions") or [])}
        if not acknowledged or frame["busy"]:
            return False
        if wait_for_admission and frame.get("can_admit_intention") is not True:
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
    if _binaries is None and os.environ.get("TOR_TEST_BINARIES"):
        # A parallel parent built them for this run (see parallel_tests.py).
        _binaries = Path(os.environ["TOR_TEST_BINARIES"])
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


def creature_package(test, name, *, hostile=False,
                     abilities=("fixture_stamina_strike", "fixture_mana_bolt", "fixture_focus_fear"),
                     ability_aliases=None, extra_abilities=(), target_x=2):
    """Construct a generic transport fixture with explicit grants and geometry."""
    package = test.directory / name
    shutil.copytree(ROOT / "scenarios/two-room", package)
    from arena_matrix import toml_source
    from process_rule_fixtures import add_transport_abilities, ability_alias
    from ruleset_sources import load_rules
    rules_path = package / "rules.toml"
    rules = load_rules(rules_path, test.bin / ("tor-scenario" + test.suffix))
    # Universal care has its own finite-supply fixture. These probes are test
    # definitions and direct grants, never production acquisition shortcuts.
    rules["constants"]["default_abilities"] = ["basic_melee"]
    templates = [template for template, _ in (ability_aliases or {}).values()]
    add_transport_abilities(rules, (*abilities, *extra_abilities, *templates))
    for identity, (template, display_name) in (ability_aliases or {}).items():
        ability_alias(rules, identity, template, display_name)
    rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
    manifest = package / "scenario.toml"
    source = manifest.read_text(encoding="utf-8")
    build = ('creature = { species = "human", name = "trainee", faction = "neutral", '
             'binding = "intellect", starting_skills = ["heavy_weaponry", "spellcasting"], '
             'restoration = "choose_again", nutrition = { kind = "exempt" }, hit_dice = [{ source = "warrior" }, { source = "mage", '
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
    source += ('\nai_profiles = { practice = { care = true, equipment = true, loot = true, flee_percent = 0 } }\n'
               '[creatures.species.human]\nbehavior = { disposition = "mean", idle = "explore", memory_ticks = 1000 }\nkind = "humanoid"\n'
               'attributes = { strength = 2, speed = 1, intellect = 2, '
               'willpower = 2, awareness = 1, presence = 1 }\n'
               'melee = { skill = "heavy_weaponry", bonus = 0, wind_up = 60, recovery = 40, strikes = [{ offset = 0, check = { actor = "user", skill = { kind = "selected_weapon" }, against = { kind = "defense", defense = "physical" } }, on_success = [{ kind = "damage", damage = { kind = "components", components = [{ category = "impact", amount = { kind = "rolled", dice = { count = 1, sides = 6, bonus = 0 } } }] } }], on_failure = [], descriptors = [], riders = [] }] }\n'
               'modifiers = [' + modifiers + ']\n')
    manifest.write_text(source, encoding="utf-8", newline="\n")
    region = package / "regions/1.toml"
    source = region.read_text(encoding="utf-8")
    source = "\n".join(line for line in source.splitlines() if not line.startswith("items =")) + "\n"
    faction = "foe" if hostile else "neutral"
    source += ('\n[[actors]]\nid = 2\nat = [' + str(target_x) + ', 1, 0]\ncontroller = "ai"\nai = "practice"\n'
               'creature = { species = "human", name = "practice target", '
               'faction = "' + faction + '", binding = "intellect", '
               'starting_skills = ["athletics", "lore"], restoration = "authored", nutrition = { kind = "exempt" }, '
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
    def _verified_compress(source, compressed):
        """Verify complete raw bytes before a caller may remove its source."""
        def digest(stream):
            checksum, length = hashlib.sha256(), 0
            while chunk := stream.read(1024 * 1024):
                checksum.update(chunk)
                length += len(chunk)
            return checksum.digest(), length

        # Processes must already be stopped. Hash before compression so changed
        # or truncated input cannot be accepted as the original evidence.
        with source.open("rb") as original:
            expected = digest(original)
            original.seek(0)
            with gzip.open(compressed, "xb") as output:
                shutil.copyfileobj(original, output, length=1024 * 1024)
        with gzip.open(compressed, "rb") as restored:
            if digest(restored) != expected:
                raise OSError(f"Compression round-trip verification failed: {source}")

    @staticmethod
    def compress_logs(directory):
        # Preserve complete text without truncating saves or mutating SQLite.
        for path in directory.rglob("*"):
            if path.suffix in (".log", ".jsonl") and path.stat().st_size > 1024 * 1024:
                compressed = path.with_name(path.name + ".gz")
                if compressed.exists():
                    raise FileExistsError(compressed)
                ProcessTestDirectory._verified_compress(path, compressed)
                path.unlink()

    @staticmethod
    def copy_artifact(source, destination):
        """Stream large text logs into staging without making a raw copy first."""
        source, destination = Path(source), Path(destination)
        if source.suffix not in (".log", ".jsonl") or source.stat().st_size <= 1024 * 1024:
            return shutil.copy2(source, destination)
        compressed = destination.with_name(destination.name + ".gz")
        # Check the source sibling too: copytree may not have visited it yet.
        # Both files are evidence, so never overwrite an existing archive.
        sibling = source.with_name(source.name + ".gz")
        if sibling.exists():
            raise FileExistsError(sibling)
        if compressed.exists():
            raise FileExistsError(compressed)
        ProcessTestDirectory._verified_compress(source, compressed)
        shutil.copystat(source, compressed)
        return str(compressed)

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
                # Copy bytes and sidecars, compressing logs directly from their
                # source so staging never requires another raw trace's space.
                # Do not close/checkpoint/save the database or rely on renaming
                # its still-open source directory. Publish only a complete copy.
                shutil.copytree(source, staging, copy_function=self.copy_artifact)
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
    def __init__(self, path, *, source=None):
        self.path = path
        self.source = source
        if source is None:
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
        if self.source is not None:
            with closing(self.source.iter_lines()) as lines:
                yield from islice(lines, self.count)
            return
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

    def __init__(self, executable, args, token=TOKEN, extra_env=None, *, separate_stderr=False,
                 diagnostics=None, save_actor=None):
        environment = {k: v for k, v in os.environ.items() if k not in ("TOR_SPECTATOR_TOKEN", "TOR_WIZARD_TOKEN")}
        environment.update(extra_env or {})
        # Harness-only, explicitly per-launch: never alter the child's wire or
        # stdout projection and never inherit a global opt-in accidentally.
        capture_key = "TOR_TEST_SEGMENTED_STDOUT_BYTES"
        capture_option = (extra_env or {}).get(capture_key)
        environment.pop(capture_key, None)
        segment_bytes = None
        if capture_option is not None:
            if (not isinstance(capture_option, str) or not capture_option.isascii()
                    or not capture_option.isdecimal() or str(int(capture_option)) != capture_option
                    or not 1 <= int(capture_option) <= 64 * 1024 * 1024):
                raise ValueError("Segmented stdout size must be a canonical integer in 1..67108864")
            segment_bytes = int(capture_option)
        self._segmented_stdout = segment_bytes is not None
        self.executable = Path(executable)
        if self.executable.stem == "tor-client-ascii":
            _require_native_display(environment)
        if self.executable.stem == "tor-server" and "--scenario" not in args and "--regions" not in args:
            args = [*args, "--scenario", SCENARIOS / "two-room"]
        self.token = token
        # Cleanup can name a known saved actor without overriding the character
        # restored by the server. Fresh starts default to their explicit preset.
        args = list(args)
        self.character = str(args[args.index("--character") + 1]) if "--character" in args else None
        self.save_actor = str(save_actor) if save_actor is not None else self.character
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
        self.lines = None
        if segment_bytes is not None:
            from segmented_process_capture import SegmentedOutputQueue
            # Prepare capture before launching a child: a storage failure cannot
            # leave an unowned child running. Popen closes non-inherited FDs.
            self.lines = SegmentedOutputQueue(self.stdout_path, segment_bytes)
            self.metadata["stdout_capture"] = {"kind": "lossless_segments", "segment_bytes": segment_bytes}
        try:
            self.child = subprocess.Popen(
                self.metadata["command"], cwd=ROOT,
                env={**environment, "TOR_SERVER_TOKEN": token},
                stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                stderr=subprocess.PIPE if separate_stderr else subprocess.STDOUT,
                text=True, encoding="utf-8", bufsize=1,
            )
        except BaseException:
            if self.lines is not None:
                self.lines.close()
            raise
        self.metadata["pid"] = self.child.pid
        self._write_status("launched")
        if self.lines is None:
            self.lines = OutputQueue(self.stdout_path)
        self.transcript = Transcript(self.stdout_path, source=self.lines if segment_bytes is not None else None)
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
        except BaseException as error:
            if getattr(self, "_segmented_stdout", False):
                self.lines.fail(error)
            raise
        finally:
            self.lines.put(None)

    def until(self, predicate, seconds=15, *, include_transcript=True):
        """Wait within one deadline; optionally return only the matched line.

        JSON-frame callers already retain their parsed result and need no second
        read of consumed capture chunks. This changes only the return value;
        FIFO consumption, the diagnostic tail and lossless retention are shared.
        """
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
                if not include_transcript:
                    return line
                if getattr(self, "_segmented_stdout", False):
                    try:
                        result = self.lines.read_between(start, self.lines.offset, deadline=deadline)
                    except queue.Empty:
                        raise self._failure("Process output deadline") from None
                    if time.monotonic() >= deadline:
                        raise self._failure("Process output deadline")
                    return result
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
                            save_at(ready["address"], self.token, self.save_actor)
                finally:
                    self.child.kill()
        finally:
            self.child.wait(timeout=10)
            self.reader.join(timeout=10)
            if getattr(self, "_segmented_stdout", False) and self.reader.is_alive():
                self.lines.fail(RuntimeError("Stdout reader did not finish before capture cleanup"))
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
        except BaseException as error:
            if getattr(self, "_segmented_stdout", False):
                self.lines.fail(error)
            raise
        finally:
            self.lines.put(None)


def save_at(address, token=TOKEN, actor=None):
    """Explicit save barrier through a real headless client; no ordinary-action timing."""
    result = subprocess.run(
        [str(binaries() / ("tor-client-headless" + SUFFIX)), "--connect", address, "--observe",
         *(["--actor", str(actor)] if actor is not None else [])],
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
        cls.bin = binaries()
        cls.suffix = SUFFIX

    def setUp(self):
        if is_graphical_test(self):
            _require_native_display(os.environ)
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

    def flush_save(self, *, actor=None):
        save_at(self.address, actor=actor)

    def private_receipts(self, *, actor=None):
        """Audit all durable entry receipts after the selected actor's save barrier.

        This is chronological evidence, not a deduplicated current-world view.
        Never use these private records to navigate an ordinary playthrough.
        """
        self.flush_save(actor=actor)
        base = inspect_save(self.save)
        self.assertIn("initial_activations", base)
        receipts = list(base["initial_activations"])
        with closing(sqlite3.connect(self.save)) as db:
            # Compaction moves covered frames unchanged into history. Read the
            # complete chronological audit, including retained branches; neither
            # replay a current-branch view nor deduplicate real repeat receipts.
            rows = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 "
                              "UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 "
                              "ORDER BY sequence").fetchall()
        for _, frame in rows:
            payload = json.loads(frame[24:])
            if "record" in payload:
                record = payload["record"]
                self.assertIn("activations", record)
                receipts.extend(record["activations"])
        return receipts

    # Server.

    def server(self, *args, wizard=False, scenario=None, character=None, spectator=True, seed=42,
               extra_env=None, separate_stderr=False, writer_barrier=False, save_actor=None):
        """Start `tor-server` on a free port and set `self.address`.

        `scenario` is a package name (see `package_path()`) or a path; extra
        command-line arguments are passed through. `save_actor` selects only
        cleanup's save client, never the server's initial or restored character.
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
            *args], extra_env=env, separate_stderr=separate_stderr, save_actor=save_actor)
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
        # The parsed frame is the result; replaying the consumed transcript here
        # can wait on archival even after the exact response arrived in time.
        client.until(match, seconds, include_transcript=False)
        return found[-1]

    def client(self, token=TOKEN, observe=False, actor=None, *, omit_memory=False, compact_background=False):
        """Opt-in projections keep ready and gameplay frames fully state-bearing."""
        client = self.launch("tor-client-headless", [
            "--connect", self.address, *(["--observe"] if observe else []),
            *(["--actor", str(actor)] if actor else []),
            *(["--omit-memory"] if omit_memory else []),
            *(["--compact-background"] if compact_background else [])], token=token)
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
                ("started", "paused", "resolved", "failed", "cancelled", "suspended")))
        phase = executed["message"]["update"]["body"]["status"]["phase"]
        if phase == "started":
            return executed
        # Work may start and be interrupted in one publication, reporting paused
        # without a separate started phase. Paused/completed/suspended work
        # changes the controls; wait for that ordered permission update. Pause
        # is not completion: the caller decides whether to resume the paid work.
        revision = int(executed["readiness"]["revision"])
        return self.frame(client, lambda frame: not frame.get("state_omitted", False)
                          and int(frame["readiness"]["revision"]) > revision)

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

    def key(self, process, key, *, wait_for_simulation=True, wait_for_admission=False):
        """Inject an input event through the window's automation channel."""
        process.write(json.dumps({"type": "key", "key": key}))
        return self.ascii_frame(process, ascii_input_completion(
            key, wait_for_simulation=wait_for_simulation,
            wait_for_admission=wait_for_admission))

    def native_press(self, process, native, name, reported, *, seconds=20,
                     wait_for_simulation=True, wait_for_ready=False, wait_for_admission=False):
        """Present a genuine OS press, then acknowledge its native release.

        All waits share one deadline. Return the latest real presented frame,
        including progress seen during release; callers that stop at started
        must not miss a terminal/readiness frame consumed by that second wait.
        An explicit admission barrier runs after release, within that same
        deadline. Permission is not completion: callers needing both also set
        wait_for_ready=True. Defaults retain started/queue-control witnesses.
        """
        deadline = time.monotonic() + seconds
        sequence = None
        complete = ascii_input_completion(reported, wait_for_simulation=wait_for_simulation,
                                          wait_for_ready=wait_for_ready)

        def acknowledged(frame):
            nonlocal sequence
            if frame.get("input_done") == reported:
                sequence = frame.get("native_input_sequence")
            return complete(frame)

        try:
            native(name, True)
            completed = self.ascii_frame(process, acknowledged,
                                         seconds=max(0, deadline - time.monotonic()))
            self.assertTrue(type(sequence) is int and sequence > 0,
                            "missing fresh native input sequence in presented acknowledgement")
        except BaseException as error:
            try:
                native(name, False)
            except BaseException as cleanup:
                error.add_note(f"Native key release also failed: {cleanup}")
            raise
        native(name, False)

        def released(record):
            nonlocal completed
            if record.get("type") == "frame":
                completed = record
            return record.get("type") == "native_release" and record.get("sequence") == sequence

        self.frame(process, released, seconds=max(0, deadline - time.monotonic()))
        if wait_for_admission:
            def permitted(frame):
                return complete(frame) and frame.get("can_admit_intention") is True

            if not permitted(completed):
                completed = self.ascii_frame(process, permitted,
                                             seconds=max(0, deadline - time.monotonic()))
        return completed

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
                vk = {"i": 0x49, "semicolon": 0xBA, "o": 0x4F, "c": 0x43, "Right": 0x27, "Up": 0x26, "Escape": 0x1B, "y": 0x59, "u": 0x55, "b": 0x42,
                      "n": 0x4E, "F4": 0x73, "F6": 0x75, "F7": 0x76, "F8": 0x77, "F9": 0x78, "F10": 0x79, "F11": 0x7A, "F12": 0x7B, "Shift_L": 0x10, "comma": 0xBC, "period": 0xBE, "g": 0x47,
                      "w": 0x57, "t": 0x54, "q": 0x51, "e": 0x45, "z": 0x5A, "x": 0x58, "v": 0x56, "2": 0x32, "Return": 0x0D, "Down": 0x28,
                      "PageUp": 0x21, "PageDown": 0x22, "Tab": 0x09, "space": 0x20,
                      "bracketleft": 0xDB, "bracketright": 0xDD}[name]
                scan = user32.MapVirtualKeyW(vk, 0)
                self.assertTrue(user32.PostMessageW(handle, 0x100 if down else 0x101, vk,
                    1 | (scan << 16) | (0x01000000 if name in ("Up", "Down", "Right", "PageUp", "PageDown") else 0)
                    | (0 if down else 0xC0000000)))
            return key
        def key(name, down):
            keysym = {"PageUp": "Prior", "PageDown": "Next"}.get(name, name)
            subprocess.run(["xdotool", "keydown" if down else "keyup", "--window", handle, keysym], check=True, timeout=10)
        return key


def content_ids(values):
    """Machine identities from a current disclosed content list."""
    return [value["id"] for value in values]


def hit_die_counts(summary):
    """Typed source counts, preserving a class named racial without expanding HD rows."""
    total = u64(summary["total"])
    counts = {(row["source_kind"], row["source"]["id"]): u64(row["count"])
              for row in summary["sources"]}
    if (len(counts) != len(summary["sources"]) or any(count == 0 for count in counts.values())
            or sum(counts.values()) != total):
        raise ValueError("Inconsistent hit-die source counts")
    return counts
