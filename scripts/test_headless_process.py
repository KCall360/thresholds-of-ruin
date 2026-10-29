"""Actual headless client disclosure, memory, authorization and rewind acceptance."""
import json
import sqlite3
import unittest

from process_harness import ProcessTestCase, SPECTATOR_TOKEN, load_fixture


class HeadlessProcesses(ProcessTestCase):
    def hydrated(self, client, frame):
        """Wait for the explicit bounded-map completion without changing gameplay."""
        if not frame["map_hydration"]["complete"]:
            frame = self.frame(client, lambda value: value["map_hydration"]["complete"])
        self.assertFalse(frame["map_hydration"]["failed"])
        self.assertFalse(frame["map_hydration"]["limited"])
        return frame

    def test_omitting_diagnostic_memory_preserves_disclosure_and_reduces_serialized_output(self):
        self.server()
        player, initial = self.client(omit_memory=True)
        full, first = self.client(SPECTATOR_TOKEN, observe=True)
        projected, _ = self.client(SPECTATOR_TOKEN, observe=True, omit_memory=True)
        self.assertNotIn("memory", initial)
        self.assertTrue(first["memory"])
        self.assertTrue(initial["has_control"])
        item = initial["state"]["observation"]["ground_items"][0]["item"]
        taken = self.act(player, {"type": "take", "item": item["id"]})
        self.assertIsNone(taken["error"])
        self.assertIn(item, taken["state"]["observation"]["inventory"])
        for _ in range(5):
            moved = self.act(player, {"type": "move", "direction": "east"})
            self.assertIsNone(moved["error"])
        # Matching same-branch snapshots synchronize both observers while
        # preserving the default client's accumulated historical memory.
        for client in (full, projected):
            synced = self.request(client, {"type": "snapshot"})
            self.assertIsNone(synced["error"])
            self.assertEqual(synced["state"], moved["state"])
            self.hydrated(client, synced)
        full_frame = self.command(full, {"type": "inspect"})
        full_line = full.transcript[-1].encode("utf-8")
        projected_frame = self.command(projected, {"type": "inspect"})
        projected_line = projected.transcript[-1].encode("utf-8")
        self.assertEqual(json.loads(full_line), full_frame)
        self.assertEqual(json.loads(projected_line), projected_frame)
        self.assertLess(len(projected_line), len(full_line))
        self.assertGreater(len(full_frame["memory"]), len(first["memory"]))
        self.assertGreater(len(full_frame["memory"]),
                           len(full_frame["state"]["observation"]["visible_cells"]))
        self.assertEqual(full_frame.keys() - projected_frame.keys(), {"memory"})
        retained = {key: value for key, value in full_frame.items() if key != "memory"}
        # Each attachment has its own opaque stream ID. Every other retained
        # field, including epoch, readiness, observation and palette, must match.
        self.assertNotEqual(retained["input_context"]["stream"]["stream"],
                            projected_frame["input_context"]["stream"]["stream"])
        retained["input_context"]["stream"]["stream"] = projected_frame["input_context"]["stream"]["stream"]
        self.assertEqual(retained, projected_frame)
        denied = self.act(projected, {"type": "wait"})
        self.assertIn("read-only", denied["error"])
        self.assertEqual(denied["state"], moved["state"])
        invalid = self.command(player, {"type": "not_an_input"})
        self.assertIn("Invalid input", invalid["error"])
        for client in (player, projected):
            frames = [json.loads(line) for line in client.transcript if line.startswith("{")]
            self.assertTrue({"ready", "response", "update"}.issubset({row["type"] for row in frames}))
            self.assertTrue(all("memory" not in row for row in frames))

    def test_compact_background_retains_raw_pages_and_complete_gameplay_and_errors(self):
        self.server()
        player, initial = self.client(omit_memory=True, compact_background=True)
        self.assertIn("state", initial)
        self.assertNotIn("state_omitted", initial)
        self.hydrated(player, initial)
        before = self.command(player, {"type": "inspect"})
        moved = self.act(player, {"type": "move", "direction": "east"})
        self.assertIsNone(moved["error"])
        self.assertGreater(int(moved["state"]["observation"]["tick"]),
                           int(before["state"]["observation"]["tick"]))
        self.assertIn("history", moved)
        self.hydrated(player, moved)
        invalid = self.command(player, {"type": "not_an_input"})
        self.assertIn("Invalid input", invalid["error"])
        self.assertEqual(invalid["state"], moved["state"])
        rejected = self.act(player, {"type": "take", "item": "i_" + "00" * 32})
        self.assertIsNotNone(rejected["error"])
        self.assertIn("state", rejected)
        frames = [json.loads(line) for line in player.transcript if line.startswith("{")]
        compact = [row for row in frames if row.get("state_omitted")]
        pages = [row for row in compact if row["message"]["type"] == "map_page"]
        self.assertTrue(pages, "Actual background hydration must exercise the projection")
        self.assertTrue(any(row["message"]["page"]["cells"] for row in pages))
        for row in compact:
            self.assertEqual(set(row), {"type", "message", "error", "map_hydration", "state_omitted"})
            self.assertIn(row["message"]["type"], {"map_page", "palette", "waiting"})
            self.assertNotIn("state", row)
        errors = [row for row in frames if (row.get("message") or {}).get("type") == "error"]
        self.assertTrue(errors, "A real rejected request must remain visible")
        for row in frames:
            if row["type"] == "ready" or (row.get("message") or {}).get("type") in {"error", "update", "snapshot"}:
                self.assertIn("state", row)
                self.assertIn("history", row)
                self.assertNotIn("state_omitted", row)

    def test_diagnostic_actor_and_revision_keep_numeric_schema_independent_of_wire(self):
        self.server()
        player = self.launch("tor-client-headless", ["--connect", self.address],
                             extra_env={"TOR_TIMING_DIAGNOSTICS": "1"})
        initial = self.frame(player, lambda f: f.get("type") == "ready")
        self.assertTrue(initial["has_control"])
        player.write(json.dumps({"type": "request", "request": {"type": "snapshot"}}))
        snapshot = self.frame(player, lambda f: f.get("type") == "ready")
        self.assertIsNone(snapshot["error"])
        self.assertIs(type(snapshot["state"]["revision"]), str)
        self.assertIs(type(snapshot["state"]["observation"]["actor"]), str)
        diagnostics = [json.loads(line) for line in player.transcript if line.startswith("{")]
        requests = [row for row in diagnostics if row.get("event") == "client_request"]
        self.assertEqual(len(requests), 2)  # Startup AcquireControl, then explicit Snapshot.
        for row in requests:
            self.assertEqual(row["timing_version"], 1)
            self.assertIs(type(row["actor"]), int)
            self.assertIs(type(row["revision"]), int)
            self.assertEqual(row["actor"], int(snapshot["state"]["observation"]["actor"]))
            self.assertEqual(row["revision"], int(snapshot["state"]["revision"]))

        self.assertEqual([row["request_kind"] for row in requests], ["non_action", "non_action"])
        self.assertNotEqual(requests[0]["request_id"], requests[1]["request_id"])
        self.assertEqual(initial["state"]["revision"], snapshot["state"]["revision"])

        # Diagnostic JSON shares stdout but is not a gameplay frame. Submit
        # exactly one ordinary action and ignore only those diagnostic rows.
        player.write(json.dumps({"type": "act", "action": {"type": "wait"}}))
        acted = self.frame(player, lambda f: f.get("type") == "ready")
        self.assertIsNone(acted["error"])
        previous_revision = int(snapshot["state"]["revision"])
        if int(acted["state"]["revision"]) <= previous_revision:
            acted = self.frame(player, lambda f: f.get("state") is not None
                               and int(f["state"]["revision"]) > previous_revision)
        self.assertIsNone(acted["error"])
        self.assertGreater(int(acted["state"]["revision"]), previous_revision)
        after = [json.loads(line) for line in player.transcript if line.startswith("{")]
        action_requests = [row for row in after if row.get("event") == "client_request"]
        self.assertEqual(action_requests[:2], requests)
        self.assertEqual(len(action_requests), 3)
        action = action_requests[2]
        self.assertEqual(action["request_kind"], "act")
        self.assertEqual(action["actor"], requests[0]["actor"])
        self.assertEqual(action["revision"], requests[0]["revision"])
        self.assertNotIn(action["request_id"], [row["request_id"] for row in requests])
        for row in after:
            if row.get("event") in ("client_request_sent", "client_ack"):
                self.assertNotIn("request_kind", row)

    def test_checkpointed_portal_bodies_survive_cold_restore_rewind_and_replay(self):
        server = self.server('--checkpoint-interval', 1, wizard=True, scenario='physics-portal', seed=None)
        player, initial = self.client()
        crossed = self.act(player, {'type': 'wait'})
        self.assertIsNone(crossed['error'])
        later = self.act(player, {'type': 'wait'})
        self.assertIsNone(later['error'])
        self.flush_save()
        with sqlite3.connect(self.save) as db:
            self.assertGreaterEqual(db.execute('SELECT sequence FROM checkpoint').fetchone()[0], 2)
        player.stop()
        server.stop()
        server = self.server('--checkpoint-interval', 1, wizard=True, scenario='physics-portal', seed=None)
        player, restored = self.client()
        self.assertEqual(restored['state'], later['state'])
        self.assertEqual(restored['history'], later['history'])
        wizard = self.wizard()
        self.wizard_command(wizard, 'rewind initial')
        rewound = self.request(player, {'type': 'snapshot'})
        self.assertNotEqual(rewound['branch'], initial['branch'])
        self.assertEqual(rewound['state']['observation'], initial['state']['observation'])
        repeated = self.act(player, {'type': 'wait'})
        self.assertIsNone(repeated['error'])
        self.assertEqual(repeated['state']['observation'], crossed['state']['observation'])
        self.flush_save()
        wizard.stop()
        player.stop()
        server.stop()
        self.server('--checkpoint-interval', 1, wizard=True, scenario='physics-portal', seed=None)
        _, restored_again = self.client()
        self.assertEqual(restored_again['state'], repeated['state'])
        self.assertEqual(restored_again['history'], repeated['history'])

    def test_stale_request_keeps_committed_state_and_history_after_restart(self):
        server = self.server()
        player, initial = self.client()
        accepted = self.act(player, {"type": "wait"})
        self.assertIsNone(accepted["error"])
        rejected = self.request(player, {
            "type": "command", "context": initial["input_context"], "branch": initial["branch"],
            "command": {"type": "act", "action": {"type": "wait"}}})
        self.assertTrue(rejected["error"].startswith("StaleContext:"))
        self.assertEqual(rejected["state"], accepted["state"])
        self.assertEqual(rejected["history"], accepted["history"])
        self.flush_save()
        player.stop()
        server.stop()
        self.server()
        resumed, state = self.client()
        self.assertEqual(state["state"], accepted["state"])
        self.assertEqual(state["history"], accepted["history"])
        self.assertIsNone(self.act(resumed, {"type": "wait"})["error"])

    def test_ai_navigation_continues_after_saved_decision_boundary(self):
        server = self.server(scenario="first-dungeon", seed=None)
        player, _ = self.client()
        def next_turn(client, frame):
            if frame["state"]["observation"]["ready"]:
                return frame
            return self.frame(client, lambda update: update.get("state") is not None
                              and update["state"]["observation"]["ready"])
        for _ in range(3):
            result = self.act(player, {"type": "wait"})
            self.assertIsNone(result["error"])
            next_turn(player, result)
        observer, ai_state = self.client(token=SPECTATOR_TOKEN, observe=True, actor=3)
        ai_entries = [entry for entry in ai_state["history"]
                      if entry["author"] == {"type": "backend", "component": "scheduler"}
                      and entry["actor"] == "3" and entry["content"]["type"] == "action"]
        self.assertTrue(ai_entries)
        self.assertEqual(len({entry["id"] for entry in ai_entries}), len(ai_entries))
        observer.stop()
        boundary = self.request(player, {"type": "snapshot"})
        self.assertTrue(boundary["state"]["observation"]["ready"])
        self.flush_save()
        player.stop()
        server.stop()
        server = self.server(scenario="first-dungeon", seed=None)
        resumed, state = self.client()
        self.assertEqual(state["state"], boundary["state"])
        restored_observer, restored_ai = self.client(token=SPECTATOR_TOKEN, observe=True, actor=3)
        self.assertEqual(restored_ai["state"], ai_state["state"])
        self.assertEqual(restored_ai["history"], ai_state["history"])
        continued = self.act(resumed, {"type": "wait"})
        self.assertIsNone(continued["error"])
        continued = next_turn(resumed, continued)
        self.assertGreater(int(continued["state"]["observation"]["tick"]),
                           int(boundary["state"]["observation"]["tick"]))
        # A second cold decode must preserve the new boundary and its history.
        boundary = self.request(resumed, {"type": "snapshot"})
        self.flush_save()
        restored_observer.stop()
        resumed.stop()
        server.stop()
        self.server(scenario="first-dungeon", seed=None)
        _, second_restore = self.client()
        self.assertEqual(second_restore["state"], boundary["state"])
        self.assertEqual(second_restore["history"], boundary["history"])

    def test_body_indexes_rebuild_after_portal_physics_save_and_restart(self):
        server = self.server(scenario="physics-portal", seed=None)
        player, _ = self.client()
        crossed = self.act(player, {"type": "wait"})
        self.assertIsNone(crossed["error"])
        observation = crossed["state"]["observation"]
        self.assertTrue(observation["motion"]["displaced"])
        self.assertGreater(int(observation["motion"]["velocity"][0]), 4096)
        self.assertTrue(any(actor["id"] == observation["self_target"] and actor["position"]["z"] == 1
                            for actor in observation["visible_actors"]))
        self.flush_save()
        player.stop()
        server.stop()
        server = self.server(scenario="physics-portal", seed=None)
        resumed, state = self.client()
        self.assertEqual(state["state"], crossed["state"])
        continued = self.act(resumed, {"type": "wait"})
        self.assertIsNone(continued["error"])
        self.assertTrue(any(actor["id"] == continued["state"]["observation"]["self_target"] and actor["position"]["z"] == 1
                            for actor in continued["state"]["observation"]["visible_actors"]))
        self.assertNotIn('"region"', json.dumps(continued["state"]["observation"]))
        # Definitions and derived portal bodies remain stable across another
        # private candidate and a second cold restore.
        self.flush_save()
        resumed.stop()
        server.stop()
        self.server(scenario="physics-portal", seed=None)
        _, restored = self.client()
        self.assertEqual(restored["state"], continued["state"])

    def test_private_history_pagination_and_anchors_survive_restart(self):
        server = self.server()
        player, initial = self.client()
        # The initial snapshot holds at most 100 entries; leave an older page.
        for n in range(104):
            result = self.request(player, {
                "type": "command", "context": initial["input_context"], "branch": initial["branch"],
                "command": {"type": "annotate", "anchor": {"type": "state", "revision": '0'},
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
        self.assertGreater(int(second["state"]["revision"]), int(initial["state"]["revision"]))

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
        synced = self.hydrated(player, synced)
        synced_cells = {cell["key"]: cell for cell in synced["memory"]}
        for cell in moved["memory"]:
            self.assertEqual(synced_cells[cell["key"]], cell)
        history = self.request(spectator, {"type": "history", "limit": 50, "before": None})
        self.assertIsNone(history["error"])
        player.stop()
        spectator.stop()
        server.stop()
        self.server()
        resumed, state = self.client()
        self.assertEqual(state["state"], moved["state"])
        state = self.hydrated(resumed, state)
        resumed_cells = {cell["key"]: cell for cell in state["memory"]}
        self.assertEqual(resumed_cells.keys(), synced_cells.keys())
        for key, cell in synced_cells.items():
            # Hydration supplies appearance at the current placement, without
            # inventing the original client's observation timestamp.
            appearance_fields = cell.keys() - {"position", "last_seen_tick", "last_seen_revision"}
            self.assertEqual({field: cell[field] for field in appearance_fields},
                             {field: resumed_cells[key][field] for field in appearance_fields})
        self.assertGreater(len(state["memory"]), len(state["state"]["observation"]["visible_cells"]))
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
        for frame in (before, after, refreshed, rewound):
            for cell in frame["memory"]:
                for entry in cell["ground_items"]:
                    self.assertEqual(set(entry), {"item", "position"})
                    self.assertEqual(set(entry["item"]), {
                        "class", "quantity", "appearance", "identified", "description", "name", "asset",
                        "presentation"})
                    self.assertIsNone(entry["item"]["presentation"])
                    for value in (entry, entry["item"]):
                        for field in ("id", "reachable", "supply_reachable", "known_supply"):
                            self.assertNotIn(field, value)

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
        failed = self.act(player, {"type": "take", "item": "i_" + "00" * 32})
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
        self.assertEqual(waited["state"]["observation"]["tick"], '100')
        rejected = self.launch("tor-client-headless", ["--connect", self.address], token="wrong-test-credential")
        self.assertNotEqual(rejected.child.wait(timeout=10), 0)
        output = rejected.until(lambda line: json.loads(line)["type"] == "fatal")
        self.assertNotIn("wrong-test-credential", output)
        observer.child.stdin.write('{"type":"quit"}\n')
        observer.child.stdin.flush()
        self.assertEqual(observer.child.wait(timeout=10), 0)


if __name__ == "__main__":
    unittest.main()
