"""Actual headless client disclosure, memory, authorization and rewind acceptance."""
import json
import unittest

from process_harness import ProcessTestCase, SPECTATOR_TOKEN, load_fixture


class HeadlessProcesses(ProcessTestCase):
    def test_body_indexes_rebuild_after_portal_physics_save_and_restart(self):
        server = self.server(scenario="physics-portal", seed=None)
        player, _ = self.client()
        crossed = self.act(player, {"type": "wait"})
        self.assertIsNone(crossed["error"])
        observation = crossed["state"]["observation"]
        self.assertTrue(observation["motion"]["displaced"])
        self.assertGreater(observation["motion"]["velocity"][0], 4096)
        self.assertTrue(any(actor["id"] == 1 and actor["position"]["z"] == 1
                            for actor in observation["visible_actors"]))
        self.flush_save()
        player.stop()
        server.stop()
        self.server(scenario="physics-portal", seed=None)
        resumed, state = self.client()
        self.assertEqual(state["state"], crossed["state"])
        continued = self.act(resumed, {"type": "wait"})
        self.assertIsNone(continued["error"])
        self.assertTrue(any(actor["id"] == 1 and actor["position"]["z"] == 1
                            for actor in continued["state"]["observation"]["visible_actors"]))
        self.assertNotIn('"region"', json.dumps(continued["state"]["observation"]))

    def test_private_history_pagination_and_anchors_survive_restart(self):
        server = self.server()
        player, initial = self.client()
        # The initial snapshot holds at most 100 entries; leave an older page.
        for n in range(104):
            result = self.request(player, {
                "type": "command", "branch": initial["branch"],
                "command": {"type": "annotate", "anchor": {"type": "state", "revision": 0},
                            "text": f"Note {n}", "source": "user", "category": "note",
                            "audience": "actor" if n % 9 == 0 else "private"}})
            self.assertIsNone(result["error"])
        self.flush_save()
        player.stop()
        server.stop()
        self.server()
        player, resumed = self.client()
        self.assertEqual(len(resumed["history"]), 100)
        player.write(json.dumps({"type": "request", "request": {
            "type": "history", "limit": 10, "before": resumed["history"][0]["id"]}}))
        response = self.frame(player, lambda frame: frame["type"] == "response"
                              and frame["message"]["type"] == "history")
        older = self.frame(player, lambda frame: frame["type"] == "ready")
        self.assertIsNone(older["error"])
        self.assertEqual([entry["content"]["text"] for entry in response["message"]["page"]["entries"]],
                         [f"Note {n}" for n in range(4)])
        self.assertIsNone(response["message"]["page"]["older_before"])
        observer, public = self.client(SPECTATOR_TOKEN)
        self.assertEqual([entry["content"]["text"] for entry in public["history"]],
                         [f"Note {n}" for n in range(0, 104, 9)])
        denied = self.request(observer, {"type": "history", "limit": 1,
                                        "before": resumed["history"][0]["id"]})
        self.assertIn("InvalidAnchor", denied["error"])
        self.assertEqual(denied["history"], public["history"])

    def test_item_location_indexes_survive_inventory_save_and_drop(self):
        server = self.server()
        player, initial = self.client()
        tablet = initial["state"]["observation"]["ground_items"][0]["item"]
        taken = self.act(player, {"type": "take", "item": tablet["id"]})
        self.assertIsNone(taken["error"])
        self.assertIn(tablet, taken["state"]["observation"]["inventory"])
        self.assertFalse(any(item["item"]["id"] == tablet["id"]
                             for item in taken["state"]["observation"]["ground_items"]))
        self.flush_save()
        player.stop()
        server.stop()
        self.server()
        resumed, state = self.client()
        self.assertEqual(state["state"], taken["state"])
        dropped = self.act(resumed, {"type": "drop", "item": tablet["id"]})
        self.assertIsNone(dropped["error"])
        self.assertNotIn(tablet, dropped["state"]["observation"]["inventory"])
        self.assertTrue(any(item["item"] == tablet
                            for item in dropped["state"]["observation"]["ground_items"]))

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
