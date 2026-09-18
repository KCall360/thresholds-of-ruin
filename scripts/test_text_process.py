"""Launch the real server and text client; no simulated input/presentation adapters."""
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
TOKEN = "text-process-test-token-not-a-secret"
SPECTATOR_TOKEN = "spectator-process-test-token-not-a-secret"


class Process:
    def __init__(self, executable, args, token=TOKEN, extra_env=None):
        environment = {k: v for k, v in os.environ.items() if k not in ("TOR_SPECTATOR_TOKEN", "TOR_WIZARD_TOKEN")}
        environment.update(extra_env or {})
        self.child = subprocess.Popen(
            [str(executable), *map(str, args)], cwd=ROOT,
            env={**environment, "TOR_SERVER_TOKEN": token},
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
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

    def command(self, command):
        self.child.stdin.write(command + "\n")
        self.child.stdin.flush()
        return self.until(lambda line: line == "Ready.")

    def stop(self):
        if self.child.poll() is None:
            self.child.kill()
        self.child.wait(timeout=10)
        self.reader.join(timeout=10)
        for stream in (self.child.stdin, self.child.stdout):
            stream.close()


class TextProcesses(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        profile = os.environ.get("TOR_TEST_PROFILE", "debug")
        if profile not in ("debug", "release"):
            raise ValueError("TOR_TEST_PROFILE must be debug or release")
        command = ["cargo", "build", "--workspace", "--bins", "--locked"]
        if profile == "release":
            command.append("--release")
        subprocess.run(command, cwd=ROOT, check=True, timeout=300)
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"], cwd=ROOT,
        ))
        cls.bin = Path(metadata["target_directory"]) / profile
        cls.suffix = ".exe" if os.name == "nt" else ""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.save = Path(self.directory.name) / "game.json"
        self.server, self.address = self.start_server()

    def launch(self, name, args, **kwargs):
        process = Process(self.bin / (name + self.suffix), (["--script", *args] if name == "tor-client-text" else args), **kwargs)
        self.addCleanup(process.stop)
        return process

    def start_server(self, spectator=False):
        server = self.launch("tor-server", ["--listen", "127.0.0.1:0", "--seed", "42", "--save", self.save],
                             extra_env={"TOR_SPECTATOR_TOKEN": SPECTATOR_TOKEN} if spectator else {})
        ready = json.loads(server.until(lambda line: line.startswith("{")))
        return server, ready["address"]

    def client(self, observe=False):
        client = self.launch("tor-client-text", ["--connect", self.address, *(["--observe"] if observe else [])])
        welcome = client.until(lambda line: line == "Ready.")
        self.assertNotIn(TOKEN, welcome)
        return client, welcome

    def test_spectator_watches_actions_and_history_but_cannot_mutate_even_after_restart(self):
        self.server.stop()
        self.server, self.address = self.start_server(spectator=True)
        player, _ = self.client()
        spectator = self.launch("tor-client-text", ["--connect", self.address], token=SPECTATOR_TOKEN)
        welcome = spectator.until(lambda line: line == "Ready.")
        self.assertIn("Spectator access is read-only", welcome)
        self.assertIn("stone tablet", welcome)
        self.assertNotIn(SPECTATOR_TOKEN, welcome)
        for revision, (action, event) in enumerate([("take token", "Taken"), ("wait", "Waited"), ("east", "Moved")], 1):
            player.command(action)
            seen = spectator.until(lambda line: f"tick {50 + (revision - 1) * 100}." in line)
            self.assertIn(event, seen)  # The result is presented as well as the new state.
        player.command("note Private player plan")
        player.command("annotate user actor note here Shared progress")
        shared = spectator.until(lambda line: "Shared progress" in line)
        self.assertNotIn("Private player plan", shared)
        before = self.save.read_bytes()
        for command in ["control", "release", "wait", "east", "note No writes",
                        "annotate frontend actor note here No shared writes"]:
            self.assertIn("read-only", spectator.command(command))
        self.assertEqual(self.save.read_bytes(), before)
        self.assertIn("tick 250.", spectator.command("sync"))
        history = spectator.command("history")
        self.assertIn("Shared progress", history)
        self.assertNotIn("Private player plan", history)
        player.command("release")
        self.assertIn("read-only", spectator.command("control"))
        self.assertIn("Control: yours", player.command("control"))
        player.stop()
        spectator.stop()
        self.server.stop()
        self.server, self.address = self.start_server(spectator=True)
        resumed = self.launch("tor-client-text", ["--connect", self.address], token=SPECTATOR_TOKEN)
        welcome = resumed.until(lambda line: line == "Ready.")
        self.assertIn("read-only", welcome)
        self.assertIn("tick 250.", welcome)
        self.assertIn("Shared progress", welcome)
        self.assertNotIn("Private player plan", welcome)
        self.assertIn("read-only", resumed.command("control"))
        self.assertEqual(self.save.read_bytes(), before)

    def test_spectator_credentials_are_optional_distinct_and_validated_before_save_creation(self):
        disabled = self.launch("tor-client-text", ["--connect", self.address], token=SPECTATOR_TOKEN)
        self.assertNotEqual(disabled.child.wait(timeout=15), 0)
        for index, token in enumerate([TOKEN, "", "short", "x" * 1025, "contains-control\ncharacters"]):
            path = self.save.parent / f"invalid-{index}.json"
            invalid = self.launch("tor-server", ["--listen", "127.0.0.1:0", "--save", path],
                                  extra_env={"TOR_SPECTATOR_TOKEN": token})
            self.assertNotEqual(invalid.child.wait(timeout=15), 0)
            self.assertFalse(path.exists())

    def test_play_notes_and_real_restart(self):
        client, welcome = self.client()
        self.assertIn("Your surroundings", welcome)
        self.assertIn("stone tablet", welcome)
        self.assertIn("Server error", client.command("take stone tablet"))
        self.assertIn("tick 0.", client.command("look"))
        self.assertIn("Taken", client.command("take the token"))
        self.assertIn("token", client.command("inventory"))
        self.assertIn("Private User", client.command("note Return here later."))
        note = client.command("annotate frontend actor explanation here The token is safe.")
        self.assertIn("Actor Frontend", note)
        self.assertIn('component: "text"', note)
        self.assertIn("tick 50.", client.command("look"))
        self.assertIn("InvalidAnchor", client.command("annotate user private note state:99 Future"))
        self.assertIn("tick 50.", client.command("look"))
        for _ in range(4):
            movement = client.command("east")
        self.assertIn("Your surroundings", movement)
        self.assertIn("stone tablet", movement)
        self.assertIn("tick 450.", movement)
        client.child.stdin.write("quit\n")
        client.child.stdin.flush()
        self.assertEqual(client.child.wait(timeout=10), 0)
        self.server.stop()
        self.server, self.address = self.start_server()
        resumed, welcome = self.client()
        self.assertIn("Your surroundings", welcome)
        self.assertIn("tick 450.", welcome)
        self.assertIn("Inventory: ", welcome)
        self.assertIn("token", welcome)
        self.assertIn("Return here later.", welcome)
        self.assertIn("The token is safe.", resumed.command("history"))
        # EOF must terminate promptly even though stdin is handled on a thread.
        resumed.child.stdin.close()
        self.assertEqual(resumed.child.wait(timeout=10), 0)

    def test_idle_streaming_control_transfer_and_disconnect(self):
        controller, _ = self.client()
        observer, _ = self.client(observe=True)
        self.assertIn("observing", observer.command("wait"))
        self.assertIn("ControlTaken", observer.command("control"))
        controller.command("note live note")
        self.assertIn("live note", observer.until(lambda line: "live note" in line))
        # Same authenticated user sees its private notes in both frontends.
        controller.command("wait")
        observer.until(lambda line: "tick 100." in line)
        controller.command("release")
        self.assertIn("Control: yours", observer.command("control"))
        self.assertIn("tick 200.", observer.command("wait"))
        self.server.stop()
        self.assertNotEqual(observer.child.wait(timeout=15), 0)
        self.assertNotEqual(controller.child.wait(timeout=15), 0)

    def test_history_pagination_and_entry_anchors(self):
        client, _ = self.client()
        note = client.command("bookmark first marker")
        entry = next(line.split("]", 1)[0][1:] for line in note.splitlines() if line.startswith("["))
        self.assertIn("InvalidAnchor", client.command(f"annotate user actor note entry:{entry} shared link"))
        self.assertIn("Entry", client.command(f"annotate user private note entry:{entry} linked marker"))
        for index in range(50):
            client.command(f"note marker {index}")
        page = client.command("history")
        cursor = next(line.removeprefix("Older entries: history ") for line in page.splitlines() if line.startswith("Older entries:"))
        older = client.command(f"history {cursor}")
        self.assertIn("first marker", older)
        self.assertIn("linked marker", older)
        self.assertIn("tick 0.", client.command("look"))

    def test_bad_authentication_and_cli_fail_without_disclosing_token(self):
        for args, token in [(["--connect", self.address], "incorrect-test-token"), (["--actor"], TOKEN), (["--connect", "192.0.2.1:4000"], TOKEN)]:
            result = subprocess.run([str(self.bin / ("tor-client-text" + self.suffix)), *args], env={**os.environ, "TOR_SERVER_TOKEN": token}, text=True, capture_output=True, timeout=15)
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn(token, result.stdout + result.stderr)
            self.assertNotIn("Entry chamber", result.stdout)

    def test_piped_commands_wait_for_authoritative_updates(self):
        result = subprocess.run(
            [str(self.bin / ("tor-client-text" + self.suffix)), "--script", "--connect", self.address],
            env={**os.environ, "TOR_SERVER_TOKEN": TOKEN},
            input="east\ntake token\nwest\ntake token\nnote piped note\nlook\ninventory\n",
            text=True, encoding="utf-8", capture_output=True, timeout=15,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("InvalidAction", result.stdout)  # Visible, but out of reach.
        self.assertIn("tick 250.", result.stdout)
        self.assertIn("Taken", result.stdout)
        self.assertIn("piped note", result.stdout)
        self.assertIn("Goodbye.", result.stdout)


if __name__ == "__main__":
    unittest.main()
