"""Launch a local game: server, player client and an optional spectator.

By default this opens the graphical launcher (`launcher_window.py`): choose the
player client, a spectator, and either a new game (scenario, seed, save name)
or a saved game from the list or a file browser. Options prefill the window.
With `--console`, numbered menus in the terminal ask instead, and any choice
given as an option skips its menu. A new game gets its own directory under the
saves directory. Each launch uses fresh random player and spectator
credentials, so nothing secret is stored with the save.

    python scripts/launch_game.py
    python scripts/launch_game.py --console --client ascii --spectator text \\
        --scenario rogue-exploration --seed 42 --new my-game
    python scripts/launch_game.py --console --client text --save my-game

When the player client exits, the launcher asks the server for an explicit
save, then stops the server and any spectator it started. On Windows its child
processes also share a job object, so closing the launcher's window ends them.
"""
import argparse
from dataclasses import dataclass, field
import datetime
import json
import os
from pathlib import Path
import random
import re
import secrets
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
WINDOWS = os.name == "nt"
SUFFIX = ".exe" if WINDOWS else ""
# Console programs whose output goes to logs get no console window, even when
# the launcher runs without one (pythonw) and Windows would otherwise open one.
NO_CONSOLE = subprocess.CREATE_NO_WINDOW if WINDOWS else 0
CLIENTS = ("text", "ascii")
SPECTATORS = ("none", *CLIENTS)
SAVE_FILES = ("game.db", "game.json")
METADATA = "launch.json"
NAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9 _.-]{0,63}")
MAX_SEED = 2**64 - 1
READY_SECONDS = 60


class LaunchError(Exception):
    """A choice or launch step the user can correct; printed without a traceback."""


@dataclass
class Save:
    """A save file, its run directory and any launch metadata stored beside it."""
    path: Path
    metadata: dict = field(default_factory=dict)

    @property
    def directory(self):
        return self.path.parent

    @property
    def name(self):
        return self.directory.name

    def describe(self):
        details = [self.metadata[key] if key != "seed" else f"seed {self.metadata[key]}"
                   for key in ("scenario", "seed") if key in self.metadata]
        played = datetime.datetime.fromtimestamp(self.path.stat().st_mtime).strftime("%Y-%m-%d %H:%M")
        return f"{self.name} ({', '.join([*details, 'last saved ' + played])})"


@dataclass
class Choices:
    client: str
    spectator: str
    save: Path
    new: bool
    scenario: Path
    seed: int | None


# Discovery.

def scenarios(directory):
    """Scenario packages (directories with a manifest), by name."""
    if not directory.is_dir():
        return {}
    return {path.name: path for path in sorted(directory.iterdir())
            if (path / "scenario.toml").is_file()}


def read_metadata(directory):
    try:
        # utf-8-sig: records edited by hand on Windows may start with a BOM.
        value = json.loads((directory / METADATA).read_text(encoding="utf-8-sig"))
    except (OSError, ValueError):
        return {}
    return value if isinstance(value, dict) else {}


def save_in(directory):
    """The save file in a run directory, if it has one."""
    return next((directory / name for name in SAVE_FILES if (directory / name).is_file()), None)


def saves(directory):
    """Existing saves under the saves directory, most recently saved first."""
    if not directory.is_dir():
        return []
    found = [Save(path, read_metadata(path.parent))
             for child in directory.iterdir() if child.is_dir() and (path := save_in(child))]
    return sorted(found, key=lambda save: save.path.stat().st_mtime, reverse=True)


def resolve_save(value, saves_dir):
    """An existing save named by run name, run directory or save file."""
    for candidate in (Path(value), saves_dir / value):
        if candidate.is_file():
            return Save(candidate, read_metadata(candidate.parent))
        if candidate.is_dir() and (path := save_in(candidate)):
            return Save(path, read_metadata(candidate))
    raise LaunchError(f"No save found at {value!r} (looked for {' or '.join(SAVE_FILES)})")


# Validation.

def parse_seed(text):
    try:
        seed = int(str(text).strip())
    except ValueError:
        raise LaunchError(f"Seed must be a whole number from 0 to {MAX_SEED}") from None
    if not 0 <= seed <= MAX_SEED:
        raise LaunchError(f"Seed must be a whole number from 0 to {MAX_SEED}")
    return seed


def default_name(now=None):
    return "game-" + (now or datetime.datetime.now()).strftime("%Y%m%d-%H%M%S")


def new_save(name, saves_dir):
    """The save path for a new game; the name must be fresh and path-free."""
    name = name.strip()
    if not NAME.fullmatch(name) or name.endswith((".", " ")):
        raise LaunchError("Save names use 1-64 letters, digits, spaces, '.', '_' or '-', "
                          "start with a letter or digit, and do not end with '.' or a space")
    directory = saves_dir / name
    if directory.exists():
        raise LaunchError(f"A save named {name!r} already exists; resume it or choose another name")
    return directory / SAVE_FILES[0]


def scenario_path(value, available):
    if value in available:
        return available[value]
    path = Path(value)
    if (path / "scenario.toml").is_file():
        return path.resolve()
    raise LaunchError(f"Unknown scenario {value!r}; choose one of: {', '.join(available) or 'none found'}")


def resume_scenario(save, available):
    """The package a resumed save was started from, if the launcher recorded it."""
    recorded = save.metadata.get("scenario_path")
    if recorded and (Path(recorded) / "scenario.toml").is_file():
        return Path(recorded)
    name = save.metadata.get("scenario")
    return available.get(name) if name else None


# Menus.

class Prompter:
    """Line prompts that never read ahead of the line they return.

    A piped stdin is shared with the text client after the menus, so it is
    read one byte at a time instead of through Python's buffered reader.
    """

    def __init__(self, read_line=None, write=None):
        self.read_line = read_line or self._stdin_line
        self.write = write or (lambda text: (sys.stdout.write(text), sys.stdout.flush()))

    @staticmethod
    def _stdin_line():
        if sys.stdin.isatty():
            return input()
        data = bytearray()
        while (byte := os.read(0, 1)) and byte != b"\n":
            data += byte
        if not byte and not data:
            raise EOFError
        return data.decode("utf-8").rstrip("\r")

    def ask(self, question, default=None):
        suffix = f" [{default}]" if default is not None else ""
        self.write(f"{question}{suffix}: ")
        answer = self.read_line().strip()
        return answer if answer else default

    def menu(self, title, options, default=0):
        """Choose one of `options` (labels) by number; returns its index."""
        self.write(title + "\n")
        for index, label in enumerate(options, 1):
            self.write(f"  {index}) {label}\n")
        while True:
            answer = self.ask("Choose", default + 1)
            try:
                index = int(answer) - 1
            except (TypeError, ValueError):
                index = -1
            if 0 <= index < len(options):
                return index
            self.write(f"Enter a number from 1 to {len(options)}.\n")

    def retry(self, question, convert, default=None):
        while True:
            try:
                return convert(self.ask(question, default))
            except LaunchError as error:
                self.write(f"{error}\n")


def choose(args, prompter, *, scenarios_dir, saves_dir):
    """Fill every launch choice from options, prompting for the rest."""
    available = scenarios(scenarios_dir)
    labels = {"text": "Text adventure (this window)", "ascii": "ASCII map window"}
    client = args.client or CLIENTS[prompter.menu("Player client:", [labels[c] for c in CLIENTS], 1)]
    spectator_labels = {"none": "No spectator", "text": "Text spectator (separate window)",
                        "ascii": "ASCII spectator window"}
    spectator = args.spectator or SPECTATORS[
        prompter.menu("Spectator:", [spectator_labels[s] for s in SPECTATORS], 0)]

    if args.save:
        existing = resolve_save(args.save, saves_dir)
    elif args.new is not None:
        existing = None
    else:
        found = saves(saves_dir)
        index = prompter.menu("Game:", ["New game", *(s.describe() for s in found),
                                        "Resume a save from another path"], 0)
        if index == 0:
            existing = None
        elif index <= len(found):
            existing = found[index - 1]
        else:
            existing = prompter.retry("Save path", lambda value: resolve_save(value or "", saves_dir))

    if existing:
        scenario = (scenario_path(args.scenario, available) if args.scenario
                    else resume_scenario(existing, available))
        if scenario is None:
            names = list(available)
            if not names:
                raise LaunchError(f"No scenarios found in {scenarios_dir}")
            scenario = available[names[prompter.menu(
                "This save has no launch record. Which scenario was it started from?", names, 0)]]
        return Choices(client, spectator, existing.path, False, scenario, None)

    if args.scenario:
        scenario = scenario_path(args.scenario, available)
    else:
        names = list(available)
        if not names:
            raise LaunchError(f"No scenarios found in {scenarios_dir}")
        preferred = names.index("rogue-exploration") if "rogue-exploration" in names else 0
        scenario = available[names[prompter.menu("Scenario:", names, preferred)]]
    if args.seed is not None:
        seed = parse_seed(args.seed)
    else:
        seed = prompter.retry("Seed", parse_seed, random.randrange(2**32))
    if args.new is not None:
        save = new_save(args.new or default_name(), saves_dir)
    else:
        save = prompter.retry("Save name", lambda name: new_save(name or "", saves_dir), default_name())
    return Choices(client, spectator, save, True, scenario, seed)


# Processes.

class ProcessGroup:
    """One game's child processes, ended together when the game ends.

    On Windows they share a job object, so they also end if the launcher
    itself is killed or its console window is closed.
    """

    def __init__(self):
        self.processes = []
        self.job = None
        if WINDOWS:
            self.job = _kill_on_close_job()

    def add(self, process):
        self.processes.append(process)
        if self.job:
            import ctypes
            if not ctypes.windll.kernel32.AssignProcessToJobObject(self.job, int(process._handle)):
                raise ctypes.WinError(ctypes.get_last_error())

    def close(self):
        """End anything still running, including grandchildren in the job."""
        for process in self.processes:
            stop(process)
        if self.job:
            import ctypes
            ctypes.windll.kernel32.CloseHandle(self.job)
            self.job = None


def _kill_on_close_job():
    import ctypes
    from ctypes import wintypes

    class Limits(ctypes.Structure):
        _fields_ = [("PerProcessUserTimeLimit", ctypes.c_int64), ("PerJobUserTimeLimit", ctypes.c_int64),
                    ("LimitFlags", wintypes.DWORD), ("MinimumWorkingSetSize", ctypes.c_size_t),
                    ("MaximumWorkingSetSize", ctypes.c_size_t), ("ActiveProcessLimit", wintypes.DWORD),
                    ("Affinity", ctypes.c_size_t), ("PriorityClass", wintypes.DWORD),
                    ("SchedulingClass", wintypes.DWORD)]

    class Counters(ctypes.Structure):
        _fields_ = [(name, ctypes.c_uint64) for name in (
            "ReadOperationCount", "WriteOperationCount", "OtherOperationCount",
            "ReadTransferCount", "WriteTransferCount", "OtherTransferCount")]

    class ExtendedLimits(ctypes.Structure):
        _fields_ = [("BasicLimitInformation", Limits), ("IoInfo", Counters),
                    ("ProcessMemoryLimit", ctypes.c_size_t), ("JobMemoryLimit", ctypes.c_size_t),
                    ("PeakProcessMemoryUsed", ctypes.c_size_t), ("PeakJobMemoryUsed", ctypes.c_size_t)]

    kernel32 = ctypes.windll.kernel32
    kernel32.CreateJobObjectW.restype = wintypes.HANDLE
    kernel32.SetInformationJobObject.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD]
    kernel32.AssignProcessToJobObject.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    job = kernel32.CreateJobObjectW(None, None)
    if not job:
        raise ctypes.WinError(ctypes.get_last_error())
    limits = ExtendedLimits()
    limits.BasicLimitInformation.LimitFlags = 0x2000  # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
    if not kernel32.SetInformationJobObject(job, 9, ctypes.byref(limits), ctypes.sizeof(limits)):
        raise ctypes.WinError(ctypes.get_last_error())
    return job


class Session:
    """Lets another thread (the launcher window) end a running game."""

    def __init__(self):
        self.player = None
        self.ended = False

    def end(self):
        """Close the player client; the game then saves and stops as usual."""
        self.ended = True
        stop(self.player)


def console_python():
    """A console Python for client windows, even when the launcher runs under pythonw."""
    executable = Path(sys.executable)
    if executable.stem.lower() == "pythonw":
        console = executable.with_name("python" + executable.suffix)
        if console.is_file():
            return str(console)
    return str(executable)


def console_window(command, transcript, title, environment):
    """A new console window running `command` through this script's transcript forwarder."""
    return subprocess.Popen(
        [console_python(), str(Path(__file__).resolve()), "--tee", str(transcript), "--title", title, "--", *command],
        env=environment, creationflags=subprocess.CREATE_NEW_CONSOLE)

def tee(command, transcript, env=None):
    """Run a console client, copying its output to the console and a transcript.

    Output is forwarded per character so the text client's prompt, which has no
    trailing newline, appears immediately. Input stays attached to the console.
    """
    child = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                             env=env, text=True, encoding="utf-8", errors="replace", bufsize=1)
    if hasattr(sys.stdout, "reconfigure") and not sys.stdout.isatty():
        sys.stdout.reconfigure(encoding="utf-8")
    try:
        with open(transcript, "w", encoding="utf-8") as log:
            while character := child.stdout.read(1):
                sys.stdout.write(character)
                sys.stdout.flush()
                log.write(character)
                if character == "\n":
                    log.flush()
        return child.wait()
    finally:
        if child.poll() is None:
            child.kill()
            child.wait()
        child.stdout.close()


def binary(bin_dir, name):
    path = bin_dir / (name + SUFFIX)
    if not path.is_file():
        raise LaunchError(f"Missing executable: {path}. Build it with: cargo build --release")
    return path


def client_command(bin_dir, client, address, report_frames=False):
    command = [str(binary(bin_dir, "tor-client-" + client)), "--connect", address, "--actor", "1"]
    # Diagnostic ASCII frame reports go to the run's log, keeping the native keyboard.
    return command + ["--report-frames"] if client == "ascii" and report_frames else command


def credentials(spectator):
    player = secrets.token_hex(16)
    return player, (secrets.token_hex(16) if spectator != "none" else None)


def server_environment(player, spectator):
    environment = {k: v for k, v in os.environ.items()
                   if k not in ("TOR_SERVER_TOKEN", "TOR_SPECTATOR_TOKEN", "TOR_WIZARD_TOKEN")}
    environment["TOR_SERVER_TOKEN"] = player
    if spectator:
        environment["TOR_SPECTATOR_TOKEN"] = spectator
    return environment


def client_environment(token):
    environment = {k: v for k, v in os.environ.items()
                   if k not in ("TOR_SPECTATOR_TOKEN", "TOR_WIZARD_TOKEN")}
    environment["TOR_SERVER_TOKEN"] = token
    return environment


def server_command(bin_dir, choices):
    command = [str(binary(bin_dir, "tor-server")), "--listen", "127.0.0.1:0",
               "--scenario", str(choices.scenario), "--save", str(choices.save)]
    return command + ["--seed", str(choices.seed)] if choices.seed is not None else command


def wait_ready(server, stdout_path, stderr_path, seconds=READY_SECONDS):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if server.poll() is not None:
            detail = stderr_path.read_text(encoding="utf-8", errors="replace").strip()
            raise LaunchError(f"The server stopped during startup: {detail or 'exit ' + str(server.returncode)}")
        try:
            line = stdout_path.read_text(encoding="utf-8").partition("\n")[0]
        except OSError:
            line = ""
        if line.startswith("{") and "address" in line:
            return json.loads(line)["address"]
        time.sleep(0.1)
    raise LaunchError(f"The server did not become ready within {seconds} seconds")


def explicit_save(bin_dir, address, token):
    """Ask the server to write everything acknowledged so far; True on success."""
    requests = (json.dumps({"type": "request", "request": {"type": "save"}}) + "\n"
                + json.dumps({"type": "quit"}) + "\n")
    try:
        result = subprocess.run([str(binary(bin_dir, "tor-client-headless")), "--connect", address, "--observe"],
                                input=requests, text=True, capture_output=True, timeout=60,
                                env=client_environment(token), creationflags=NO_CONSOLE)
    except (LaunchError, OSError, subprocess.TimeoutExpired):
        return False
    replies = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
    return result.returncode == 0 and not any(reply.get("error") for reply in replies)


def logged_process(command, logs, role, environment):
    """A client with its own native window, its output in the run's logs."""
    with open(logs / f"{role}.stdout.log", "w") as out, open(logs / f"{role}.stderr.log", "w") as err:
        return subprocess.Popen(command, env=environment, stdout=out, stderr=err, stdin=subprocess.DEVNULL,
                                creationflags=NO_CONSOLE)


def spectator_process(choices, bin_dir, address, token, logs, group, report_frames=False):
    environment = client_environment(token)
    if choices.spectator == "ascii":
        process = logged_process(client_command(bin_dir, "ascii", address, report_frames), logs, "spectator",
                                 environment)
    else:
        process = console_window(client_command(bin_dir, "text", address), logs / "spectator.transcript.log",
                                 "Thresholds of Ruin - Text spectator", environment)
    group.add(process)
    return process


def stop(process):
    if process and process.poll() is None:
        process.kill()
        process.wait(timeout=10)


def check_platform(choices):
    if choices.spectator == "text" and not WINDOWS:
        raise LaunchError("A text spectator needs a separate console window, which this launcher "
                          "only opens on Windows; choose an ASCII spectator instead")


def launch(choices, bin_dir, *, write=lambda text: print(text, flush=True), report_frames=False,
           session=None, text_window=False):
    """Run one game session; returns the player client's exit code.

    With `text_window` (the graphical launcher on Windows), a text player gets
    its own console window instead of this process's console.
    """
    check_platform(choices)
    session = session or Session()
    for name in ("tor-server", "tor-client-headless", "tor-client-" + choices.client,
                 *(["tor-client-" + choices.spectator] if choices.spectator != "none" else [])):
        binary(bin_dir, name)
    run_dir = choices.save.parent
    run_dir.mkdir(parents=True, exist_ok=True)
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    logs = run_dir / "logs" / stamp
    suffix = 1
    while logs.exists():
        suffix += 1
        logs = run_dir / "logs" / f"{stamp}-{suffix}"
    logs.mkdir(parents=True)
    if choices.new:
        (run_dir / METADATA).write_text(json.dumps({
            "scenario": choices.scenario.name, "scenario_path": str(choices.scenario),
            "seed": choices.seed, "created": datetime.datetime.now().isoformat(timespec="seconds"),
        }, indent=2) + "\n", encoding="utf-8")

    group = ProcessGroup()
    player_token, spectator_token = credentials(choices.spectator)
    server = spectator = player = None
    address = None
    code = 1
    try:
        with open(logs / "server.stdout.log", "w") as out, open(logs / "server.stderr.log", "w") as err:
            server = subprocess.Popen(server_command(bin_dir, choices), env=server_environment(player_token, spectator_token),
                                      stdout=out, stderr=err, stdin=subprocess.DEVNULL,
                                      creationflags=NO_CONSOLE)
        group.add(server)
        address = wait_ready(server, logs / "server.stdout.log", logs / "server.stderr.log")
        if choices.spectator != "none":
            spectator = spectator_process(choices, bin_dir, address, spectator_token, logs, group, report_frames)
        status = {"launcher": os.getpid(), "server": server.pid, "spectator": spectator and spectator.pid,
                  "address": address, "save": str(choices.save), "client": choices.client,
                  "spectator_client": choices.spectator}
        player_command = client_command(bin_dir, choices.client, address, report_frames)
        if choices.client == "text" and not text_window:
            (logs / "status.json").write_text(json.dumps(status, indent=2) + "\n", encoding="utf-8")
            code = tee(player_command, logs / "player.transcript.log", client_environment(player_token))
        else:
            if choices.client == "text":
                player = console_window(player_command, logs / "player.transcript.log",
                                        "Thresholds of Ruin - Text player", client_environment(player_token))
            else:
                player = logged_process(player_command, logs, "player", client_environment(player_token))
            group.add(player)
            session.player = player
            if session.ended:
                stop(player)
            status["player"] = player.pid
            (logs / "status.json").write_text(json.dumps(status, indent=2) + "\n", encoding="utf-8")
            window = "text" if choices.client == "text" else "ASCII"
            write(f"Playing in the {window} window. Close it to save and quit.")
            code = player.wait()
    except KeyboardInterrupt:
        code = 130
    finally:
        stop(player)
        stop(spectator)
        if server and server.poll() is None:
            if address and not explicit_save(bin_dir, address, player_token):
                write("Warning: the final save request failed; recent turns may be lost.")
            stop(server)
        group.close()
    write(f"Game saved in {run_dir}")
    return code


def run_tee(argv):
    """`--tee LOG [--title TITLE] -- COMMAND...`: the text spectator window's forwarder."""
    separator = argv.index("--")
    options = dict(zip(argv[:separator:2], argv[1:separator:2]))
    if (title := options.get("--title")) and WINDOWS:
        import ctypes
        ctypes.windll.kernel32.SetConsoleTitleW(title)
    code = tee(argv[separator + 1:], options["--tee"])
    if sys.stdin.isatty():
        input("The game has ended. Press Enter to close this window.")
    return code


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv[:1] == ["--tee"]:
        return run_tee(argv)
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--client", choices=CLIENTS, help="player client")
    parser.add_argument("--spectator", choices=SPECTATORS, help="spectator client, or none")
    parser.add_argument("--scenario", help="scenario name or package directory")
    parser.add_argument("--seed", help="seed for a new game (saves keep their own)")
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--save", help="resume this save (name, directory or file)")
    group.add_argument("--new", nargs="?", const="", metavar="NAME", help="start a new game, optionally named")
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/release")
    parser.add_argument("--scenarios-dir", type=Path, default=ROOT / "scenarios")
    parser.add_argument("--saves-dir", type=Path, default=ROOT / "saves/games")
    parser.add_argument("--report-frames", action="store_true",
                        help="diagnostic: log ASCII frame reports in the run's log directory")
    parser.add_argument("--console", action="store_true", help="use terminal menus instead of the window")
    parser.add_argument("--pause-on-exit", action="store_true",
                        help="with --console: wait for Enter before closing (for desktop shortcuts)")
    parser.add_argument("--automation", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    if args.save and args.seed is not None:
        parser.error("--seed applies to new games; a save keeps its original seed")
    if args.console and args.automation:
        parser.error("--automation drives the graphical launcher; omit --console")
    if not args.console:
        import launcher_window
        return launcher_window.run(args)
    prompter = Prompter()
    try:
        return play(args, prompter)
    finally:
        if args.pause_on_exit and sys.stdin.isatty():
            input("Press Enter to close this window.")


def play(args, prompter):
    try:
        choices = choose(args, prompter, scenarios_dir=args.scenarios_dir, saves_dir=args.saves_dir.resolve())
        verb = "Starting" if choices.new else "Resuming"
        seed = f", seed {choices.seed}" if choices.seed is not None else ""
        spectator = f" with a {choices.spectator} spectator" if choices.spectator != "none" else ""
        print(f"{verb} {choices.save.parent.name}: {choices.scenario.name}{seed}, "
              f"{choices.client} client{spectator}.", flush=True)
        return launch(choices, args.bin_dir, report_frames=args.report_frames)
    except LaunchError as error:
        print(f"Error: {error}", file=sys.stderr, flush=True)
        return 2
    except EOFError:
        print("Error: input ended before every launch choice was made", file=sys.stderr, flush=True)
        return 2


if __name__ == "__main__":
    sys.exit(main())
