"""Actual headless client disclosure, memory, authorization and rewind acceptance."""
import json
import unittest

from process_harness import ProcessTestCase, SPECTATOR_TOKEN, load_fixture


class HeadlessProcesses(ProcessTestCase):
    def test_many_watchers_keep_independent_bases_when_a_late_watcher_joins(self):
        self.server()
        player, initial = self.client()
        watchers = [self.client(SPECTATOR_TOKEN)[0] for _ in range(3)]
        first = self.act(player, {"type": "move", "direction": "east"})
        self.assertIsNone(first["error"])
        for watcher in watchers:
            seen = self.frame(watcher, lambda f: f["state"]["revision"] == first["state"]["revision"])
            self.assertEqual(seen["state"], first["state"])
        late, attached = self.client(SPECTATOR_TOKEN)
        self.assertEqual(attached["state"], first["state"])
        second = self.act(player, {"type": "move", "direction": "east"})
        self.assertIsNone(second["error"])
        sequences = []
        for watcher in [*watchers, late]:
            seen = self.frame(watcher, lambda f: f["state"]["revision"] == second["state"]["revision"])
            self.assertEqual(seen["state"], second["state"])
            sequences.append(seen["message"]["update"]["cursor"]["sequence"])
        self.assertEqual(sequences[:3], [sequences[0]] * 3)
        self.assertGreater(sequences[0], sequences[-1])
        self.assertGreater(second["state"]["revision"], initial["state"]["revision"])

    def test_normal_play_spectator_stream_denials_and_resume(self):
        server = self.server()
        player, initial = self.client()
        spectator, observing = self.client(SPECTATOR_TOKEN)
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
        self.flush_save()
        before = self.save.read_bytes()
        denied = self.act(spectator, {"type": "wait"})
        self.assertIn("read-only", denied["error"])
        denied = self.request(spectator, {"type": "acquire_control"})
        self.assertIn("read-only", denied["error"])
        self.assertEqual(self.save.read_bytes(), before)
        for _ in range(5):
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
        wizard = self.wizard()
        observer, initial = self.client(SPECTATOR_TOKEN)
        self.assertTrue(initial["state"]["wizard_game"])
        fixture = load_fixture("perception-memory.json")
        for command in fixture["visit_and_leave"]:
            self.wizard_command(wizard, command)
        before = self.request(observer, {"type": "snapshot"})
        gallery = next(view for view in before["memory"] if any(i["item"]["name"] == "stone tablet" for i in view["ground_items"]))
        self.assertEqual(len(gallery["ground_items"]), 1)
        self.wizard_command(wizard, fixture["hidden_change"])
        after = self.request(observer, {"type": "snapshot"})
        # Wizard receipts advance their author's actor revision, even when the
        # command changes a hidden room. The disclosed scene stays unchanged.
        self.assertEqual(next(view for view in after["memory"] if any(i["item"]["name"] == "stone tablet" for i in view["ground_items"])), gallery)
        self.assertEqual(after["state"]["observation"], before["state"]["observation"])
        self.assertFalse(any(entry["content"]["type"] == "wizard" for entry in after["history"]))
        self.wizard_command(wizard, fixture["revisit"])
        refreshed = self.request(observer, {"type": "snapshot"})
        gallery = next(view for view in refreshed["memory"] if any(i["item"]["name"] == "stone tablet" for i in view["ground_items"]))
        self.assertEqual(len(gallery["ground_items"]), 2)
        self.wizard_command(wizard, "rewind initial")
        rewound = self.request(observer, {"type": "snapshot"})
        self.assertNotEqual(rewound["branch"], initial["branch"])
        self.assertEqual(len(rewound["memory"]), len(rewound["state"]["observation"]["visible_cells"]))
        self.assertIn("stone tablet", json.dumps(rewound))

    def test_invalid_input_failed_actions_control_transfer_and_authentication(self):
        self.server()
        player, initial = self.client()
        observer, attached = self.client()
        self.assertFalse(attached["has_control"])
        self.flush_save()
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
