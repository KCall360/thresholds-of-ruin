"""The text client's scripted line interface through the real server and client.

Other process tests share this client through `process_harness`; this module
keeps the text client's own behavior: spectating, notes, history, control
transfer, authentication and piped input.
"""
import os
import subprocess
import unittest

from process_harness import ProcessTestCase, TOKEN, SPECTATOR_TOKEN


class TextProcesses(ProcessTestCase):
    def test_stats_inspection_does_not_submit_an_action_or_advance_time(self):
        client, _ = self.text_client()
        before = client.command("look")
        self.assertIn("tick 0.", before)
        for _ in range(2):
            self.assertIn("No creature stats are available.", client.command("stats"))
            self.assertIn("tick 0.", client.command("look"))

    def test_unavailable_ability_commands_do_not_submit_or_advance(self):
        client, _ = self.text_client()
        for command in ["power strike nobody", "magic bolt nobody", "fear nobody"]:
            self.assertIn("No matching actor is visible.", client.command(command))
            self.assertIn("tick 0.", client.command("look"))

    def setUp(self):
        super().setUp()
        # Spectator credentials are optional; the default server has none.
        self.game = self.server(spectator=False)

    def test_spectator_watches_actions_and_history_but_cannot_mutate_even_after_restart(self):
        self.game.stop()
        self.game = self.server()
        player, _ = self.text_client()
        spectator = self.launch("tor-client-text", ["--script", "--connect", self.address], token=SPECTATOR_TOKEN)
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
        self.flush_save()
        before = self.save.read_bytes()
        for command in ["control", "release", "save", "wait", "east", "note No writes",
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
        self.game.stop()
        self.game = self.server()
        resumed = self.launch("tor-client-text", ["--script", "--connect", self.address], token=SPECTATOR_TOKEN)
        welcome = resumed.until(lambda line: line == "Ready.")
        self.assertIn("read-only", welcome)
        self.assertIn("tick 250.", welcome)
        self.assertIn("Shared progress", welcome)
        self.assertNotIn("Private player plan", welcome)
        self.assertIn("read-only", resumed.command("control"))
        self.assertEqual(self.save.read_bytes(), before)

    def test_spectator_credentials_are_optional_distinct_and_validated_before_save_creation(self):
        disabled = self.launch("tor-client-text", ["--script", "--connect", self.address], token=SPECTATOR_TOKEN)
        self.assertNotEqual(disabled.child.wait(timeout=15), 0)
        for index, token in enumerate([TOKEN, "", "short", "x" * 1025, "contains-control\ncharacters"]):
            path = self.save.parent / f"invalid-{index}.json"
            invalid = self.launch("tor-server", ["--listen", "127.0.0.1:0", "--save", path],
                                  extra_env={"TOR_SPECTATOR_TOKEN": token})
            self.assertNotEqual(invalid.child.wait(timeout=15), 0)
            self.assertFalse(path.exists())

    def test_play_notes_and_real_restart(self):
        client, welcome = self.text_client()
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
        self.game.stop()
        self.game = self.server(spectator=False)
        resumed, welcome = self.text_client()
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
        controller, _ = self.text_client()
        observer, _ = self.text_client(observe=True)
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
        self.game.stop()
        self.assertNotEqual(observer.child.wait(timeout=15), 0)
        self.assertNotEqual(controller.child.wait(timeout=15), 0)

    def test_history_pagination_and_entry_anchors(self):
        client, _ = self.text_client()
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
