"""Actual headless client disclosure, memory, authorization and rewind acceptance."""
import json
from pathlib import Path
import tempfile
import unittest

import test_text_process as support

WIZARD_TOKEN = "headless-wizard-test-token-not-a-secret"


class HeadlessProcesses(unittest.TestCase):
    launch = support.TextProcesses.launch

    @classmethod
    def setUpClass(cls):
        support.TextProcesses.setUpClass.__func__(cls)

    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.save = Path(directory.name) / "game.json"

    def server(self, wizard=False):
        env = {"TOR_SPECTATOR_TOKEN": support.SPECTATOR_TOKEN}
        if wizard:
            env["TOR_WIZARD_TOKEN"] = WIZARD_TOKEN
        server = self.launch("tor-server", ["--listen", "127.0.0.1:0", "--seed", "42",
                              "--save", self.save, *(["--wizard"] if wizard else [])], extra_env=env)
        self.address = json.loads(server.until(lambda line: line.startswith("{")))["address"]
        return server

    def frame(self, client, predicate):
        output = client.until(lambda line: predicate(json.loads(line)))
        return json.loads(output.splitlines()[-1])

    def client(self, token=support.TOKEN):
        client = self.launch("tor-client-headless", ["--connect", self.address], token=token)
        return client, self.frame(client, lambda frame: frame["type"] == "ready")

    def command(self, client, value):
        client.child.stdin.write(json.dumps(value) + "\n")
        client.child.stdin.flush()
        return self.frame(client, lambda frame: frame["type"] == "ready")

    def request(self, client, request):
        return self.command(client, {"type": "request", "request": request})

    def act(self, client, action):
        return self.command(client, {"type": "act", "action": action})

    def test_normal_play_spectator_stream_denials_and_resume(self):
        server = self.server()
        player, initial = self.client()
        spectator, observing = self.client(support.SPECTATOR_TOKEN)
        self.assertTrue(initial["has_control"])
        self.assertFalse(observing["has_control"])
        self.assertEqual(observing["role"], "spectator")
        self.assertEqual(len(initial["memory"]), len(initial["state"]["observation"]["visible_cells"]))
        self.assertIn("stone tablet", json.dumps(initial))
        token = initial["state"]["observation"]["ground_items"][0]["item"]["id"]
        taken = self.act(player, {"type": "take", "item": token})
        seen = self.frame(spectator, lambda f: f["state"]["revision"] == taken["state"]["revision"])
        self.assertEqual(seen["state"], taken["state"])
        self.assertEqual(seen["message"]["update"]["body"]["event"]["content"]["event"]["type"], "taken")
        before = self.save.read_bytes()
        denied = self.act(spectator, {"type": "wait"})
        self.assertIn("read-only", denied["error"])
        denied = self.request(spectator, {"type": "acquire_control"})
        self.assertIn("read-only", denied["error"])
        self.assertEqual(self.save.read_bytes(), before)
        for _ in range(4):
            moved = self.act(player, {"type": "move", "direction": "east"})
        self.assertEqual(next(i["position"] for i in moved["state"]["observation"]["ground_items"] if i["item"]["name"] == "stone tablet"), {"x":2,"y":0,"z":0})
        self.assertGreater(len(moved["memory"]), len(initial["memory"]))
        synced = self.request(player, {"type": "snapshot"})
        self.assertEqual(synced["memory"], moved["memory"])
        history = self.request(spectator, {"type": "history", "limit": 50, "before": None})
        self.assertIsNone(history["error"])
        player.stop()
        spectator.stop()
        server.stop()
        self.server()
        resumed, state = self.client()
        self.assertEqual(state["state"], moved["state"])
        self.assertEqual(len(state["memory"]), len(state["state"]["observation"]["visible_cells"]))
        resumed.child.stdin.close()
        self.assertEqual(resumed.child.wait(timeout=10), 0)

    def test_wizard_hidden_change_stale_memory_revisit_and_rewind(self):
        self.server(wizard=True)
        wizard = self.launch("tor-client-text", ["--connect", self.address], token=WIZARD_TOKEN)
        wizard.until(lambda line: line == "Ready.")
        observer, initial = self.client(support.SPECTATOR_TOKEN)
        self.assertTrue(initial["state"]["wizard_game"])
        fixture = json.loads((Path(__file__).parent / "scenarios/perception-memory.json").read_text())
        for command in fixture["visit_and_leave"]:
            self.assertNotIn("Server error", wizard.command(command))
        before = self.request(observer, {"type": "snapshot"})
        gallery = next(view for view in before["memory"] if any(i["item"]["name"] == "stone tablet" for i in view["ground_items"]))
        self.assertEqual(len(gallery["ground_items"]), 1)
        wizard.command(fixture["hidden_change"])
        after = self.request(observer, {"type": "snapshot"})
        # Wizard receipts advance their author's actor revision, even when the
        # command changes a hidden room. The disclosed scene stays unchanged.
        self.assertEqual(next(view for view in after["memory"] if any(i["item"]["name"] == "stone tablet" for i in view["ground_items"])), gallery)
        self.assertEqual(after["state"]["observation"], before["state"]["observation"])
        self.assertFalse(any(entry["content"]["type"] == "wizard" for entry in after["history"]))
        wizard.command(fixture["revisit"])
        refreshed = self.request(observer, {"type": "snapshot"})
        gallery = next(view for view in refreshed["memory"] if any(i["item"]["name"] == "stone tablet" for i in view["ground_items"]))
        self.assertEqual(len(gallery["ground_items"]), 2)
        wizard.command("wizard rewind initial")
        rewound = self.request(observer, {"type": "snapshot"})
        self.assertNotEqual(rewound["branch"], initial["branch"])
        self.assertEqual(len(rewound["memory"]), len(rewound["state"]["observation"]["visible_cells"]))
        self.assertIn("stone tablet", json.dumps(rewound))

    def test_invalid_input_failed_actions_control_transfer_and_authentication(self):
        self.server()
        player, initial = self.client()
        observer, attached = self.client()
        self.assertFalse(attached["has_control"])
        before = self.save.read_bytes()
        player.child.stdin.write("not JSON\n")
        player.child.stdin.flush()
        invalid = self.frame(player, lambda f: f["type"] == "ready")
        self.assertIn("Invalid input", invalid["error"])
        invalid = self.command(player, {"type": "act", "action": {"type": "wait"}, "extra": True})
        self.assertIn("Invalid input", invalid["error"])
        failed = self.act(player, {"type": "take", "item": 999999})
        self.assertIsNotNone(failed["error"])
        self.assertEqual(failed["memory"], initial["memory"])
        self.assertEqual(failed["state"], initial["state"])
        denied = self.act(observer, {"type": "wait"})
        self.assertIn("control", denied["error"])
        self.assertEqual(self.save.read_bytes(), before)
        self.request(player, {"type": "release_control"})
        controlled = self.request(observer, {"type": "acquire_control"})
        self.assertTrue(controlled["has_control"])
        waited = self.act(observer, {"type": "wait"})
        self.assertEqual(waited["state"]["observation"]["tick"], 100)
        rejected = self.launch("tor-client-headless", ["--connect", self.address], token="wrong-test-credential")
        self.assertNotEqual(rejected.child.wait(timeout=10), 0)
        output = rejected.until(lambda line: json.loads(line)["type"] == "fatal")
        self.assertNotIn("wrong-test-credential", output)
        observer.child.stdin.write('{"type":"quit"}\n')
        observer.child.stdin.flush()
        self.assertEqual(observer.child.wait(timeout=10), 0)


if __name__ == "__main__":
    unittest.main()
