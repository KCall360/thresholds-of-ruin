"""Saved intention admission and execution through the real server and clients."""
import json
import shutil
import sqlite3
import subprocess

from process_harness import ProcessTestCase, ROOT, SPECTATOR_TOKEN, WIZARD_TOKEN


def lifecycle(frame, identity, phase):
    message = frame.get("message") or {}
    body = (message.get("update") or {}).get("body") or {}
    status = body.get("status") or {}
    return (message.get("type") == "update" and body.get("type") == "intention"
            and status.get("intention") == identity and status.get("phase") == phase)


class IntentionProcesses(ProcessTestCase):
    def test_decimal_readiness_revisions_cross_digit_boundaries_in_real_play(self):
        self.server(scenario="two-room")
        player, initial = self.client()
        previous = int(initial["readiness"]["revision"])
        for _ in range(12):
            completed = self.act(player, {"type": "wait"})
            self.assertIsNone(completed["error"])
            encoded = completed["readiness"]["revision"]
            self.assertIsInstance(encoded, str)
            self.assertEqual(encoded, str(int(encoded)))
            self.assertGreater(int(encoded), previous)
            previous = int(encoded)
        self.assertGreaterEqual(previous, 10)
        noted = self.request(player, {
            "type": "command", "context": completed["input_context"],
            "branch": completed["branch"], "command": {
                "type": "annotate", "anchor": {
                    "type": "state", "revision": completed["state"]["revision"]},
                "text": "Exact decimal context after queued execution"}})
        self.assertIsNone(noted["error"])
        self.assertEqual(noted["state"], completed["state"])

    def test_completed_action_returns_current_input_context_for_the_next_command(self):
        self.server(scenario="two-room")
        player, _ = self.client()
        completed = self.act(player, {"type": "wait"})
        self.assertIsNone(completed["error"])
        noted = self.request(player, {
            "type": "command", "context": completed["input_context"], "branch": completed["branch"],
            "command": {"type": "annotate", "anchor": {"type": "state", "revision": completed["state"]["revision"]},
                        "text": "Current input after execution"}})
        self.assertIsNone(noted["error"])
        self.assertEqual(noted["state"], completed["state"])

    def test_stale_command_context_rejects_without_admission_and_restart_keeps_the_valid_effect(self):
        server = self.server(scenario="two-room")
        player, initial = self.client()
        released = self.request(player, {"type": "release_control"})
        reacquired = self.request(player, {"type": "acquire_control"})
        reset = self.request(player, {"type": "snapshot"})
        self.assertEqual(reset["state"], initial["state"])
        self.assertEqual(reset["branch"], initial["branch"])
        observer, watched = self.client(SPECTATOR_TOKEN)
        for source in [initial, released, reacquired, watched]:
            denied = self.request(player, {
                "type": "command", "context": source["input_context"], "branch": reset["branch"],
                "command": {"type": "act", "expected_revision": reset["state"]["revision"],
                            "action": {"type": "wait"}}})
            self.assertTrue(denied["error"].startswith("StaleContext:"), denied["error"])
            self.assertEqual(denied["state"], initial["state"])
            self.assertEqual(denied["history"], initial["history"])
            self.assertEqual(denied["intentions"], [])
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        boundary = self.request(player, {"type": "snapshot"})
        self.assertEqual(int(boundary["state"]["revision"]), int(initial["state"]["revision"]) + 1)
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop(); observer.stop(); server.stop()
        self.server(scenario="two-room")
        _, restored = self.client()
        self.assertEqual(restored["state"], boundary["state"])
        self.assertEqual(restored["history"], boundary["history"])

    def test_readiness_is_authoritative_across_control_changes_and_simulation_execution(self):
        self.server(scenario="two-room")
        player, initial = self.client()
        observer, watched = self.client(SPECTATOR_TOKEN)
        self.assertTrue(initial["readiness"]["admission"])
        spectator_readiness = {"revision": "0", "admission": False, "resume": [], "cancel": []}
        self.assertEqual(watched["readiness"], spectator_readiness)
        revision = initial["readiness"]["revision"]
        released = self.request(player, {"type": "release_control"})
        self.assertEqual(released["readiness"], {
            "revision": str(int(revision) + 1), "admission": False, "resume": [], "cancel": []})
        reacquired = self.request(player, {"type": "acquire_control"})
        self.assertTrue(reacquired["readiness"]["admission"])
        self.assertEqual(int(reacquired["readiness"]["revision"]), int(revision) + 2)
        self.assertEqual(reacquired["state"], initial["state"])
        self.assertEqual(reacquired["branch"], initial["branch"])
        start = len(player.transcript)
        finished = self.act(player, {"type": "move", "direction": "east"})
        self.assertIsNone(finished["error"])
        self.assertTrue(finished["readiness"]["admission"])
        self.assertEqual(int(finished["readiness"]["revision"]), int(revision) + 4)
        permissions = []
        for line in player.transcript[start:]:
            if line.startswith("{"):
                message = json.loads(line).get("message") or {}
                body = (message.get("update") or {}).get("body") or {}
                if body.get("type") == "readiness":
                    permissions.append(body["readiness"])
        self.assertEqual(len(permissions), 2)
        self.assertFalse(permissions[0]["admission"])
        self.assertEqual(len(permissions[0]["cancel"]), 1)
        self.assertEqual(permissions[1]["cancel"], [])
        reset = self.request(player, {"type": "snapshot"})
        self.assertEqual(reset["readiness"], finished["readiness"])
        watched = self.request(observer, {"type": "snapshot"})
        self.assertEqual(watched["readiness"], spectator_readiness)

    def test_stream_context_and_exact_delta_bases_survive_reset_and_reconnect(self):
        server = self.server(scenario="two-room")
        player, _ = self.client()
        observer, _ = self.client(SPECTATOR_TOKEN)

        def snapshot(client):
            client.write(json.dumps({"type": "request", "request": {"type": "snapshot"}}))
            response = self.frame(client, lambda frame:
                (frame.get("message") or {}).get("type") == "snapshot")
            self.frame(client, lambda frame: frame["type"] == "ready")
            return response["message"]["snapshot"]

        def deltas(client, start, base):
            updates = []
            for line in client.transcript[start:]:
                if not line.startswith("{"):
                    continue
                message = json.loads(line).get("message") or {}
                update = message.get("update") or {}
                if (update.get("body") or {}).get("type") == "observation_delta":
                    updates.append(update)
            self.assertTrue(updates, "real process must receive an observation delta")
            expected = {"cursor": base["cursor"], "revision": base["state"]["revision"]}
            for update in updates:
                self.assertEqual(update["context"], base["context"])
                self.assertEqual(update["body"]["base"], expected)
                expected = {"cursor": update["cursor"],
                            "revision": update["body"]["state"]["revision"]}

        first = snapshot(player)
        watched = snapshot(observer)
        self.assertNotEqual(first["context"]["stream"], watched["context"]["stream"])
        for _ in range(2):
            player_start, observer_start = len(player.transcript), len(observer.transcript)
            result = self.act(player, {"type": "wait"})
            self.assertIsNone(result["error"])
            self.frame(observer, lambda frame:
                frame["state"]["revision"] == result["state"]["revision"]
                and not frame["intentions"])
            deltas(player, player_start, first)
            deltas(observer, observer_start, watched)
            reset = snapshot(player)
            self.assertEqual(reset["context"]["stream"], first["context"]["stream"])
            self.assertEqual(int(reset["context"]["epoch"]), int(first["context"]["epoch"]) + 1)
            self.assertEqual(reset["state"], result["state"])
            first = reset
            watched = snapshot(observer)
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            frames = db.execute("SELECT frame FROM journal UNION ALL SELECT frame FROM history").fetchall()
        for stream in [first["context"]["stream"], watched["context"]["stream"]]:
            self.assertFalse(any(stream.encode() in frame for (frame,) in frames))
        player.stop()
        observer.stop()
        server.stop()
        self.server(scenario="two-room")
        resumed, _ = self.client()
        recovered = snapshot(resumed)
        self.assertNotEqual(recovered["context"]["stream"], first["context"]["stream"])
        self.assertEqual(recovered["state"], first["state"])
        self.assertIsNone(self.act(resumed, {"type": "wait"})["error"])

    def test_autonomous_journal_uses_private_admission_and_backend_execution_without_rpc(self):
        server = self.server(scenario="dungeon-loop")
        player, _ = self.client()
        watcher, initial = self.client(SPECTATOR_TOKEN, actor=2)
        self.assertEqual(initial["intentions"], [])
        self.act(player, {"type": "wait"})
        observed = self.frame(watcher, lambda frame: any(
            entry.get("author", {}).get("component") == "scheduler"
            and entry.get("content", {}).get("type") == "action"
            for entry in (frame.get("history") or [])))
        self.assertEqual(observed["intentions"], [])
        self.assertFalse(observed["has_control"])
        boundary = self.request(watcher, {"type": "snapshot"})
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            rows = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 "
                "UNION ALL SELECT sequence,frame FROM history ORDER BY sequence").fetchall()
        records = [json.loads(frame[24:])["record"] for _, frame in rows]
        admissions = [record for record in records
            if record["entry"]["content"]["type"] == "autonomous_intention_admitted"]
        self.assertGreater(len(admissions), 0)
        for admission in admissions:
            entry = admission["entry"]
            self.assertIsNone(admission["receipt"])
            self.assertEqual(entry["actor"], 2)
            self.assertEqual(entry["audience"], "private")
            self.assertEqual(entry["author"], {"type": "backend", "component": "scheduler"})
            executions = [record for record in records
                if record["entry"]["content"].get("admission") == entry["id"]]
            self.assertEqual(len(executions), 1)
            self.assertIsNone(executions[0]["receipt"])
            self.assertEqual(executions[0]["entry"]["content"]["type"], "intention_started")
            self.assertEqual(executions[0]["entry"]["content"]["intention"],
                             entry["content"]["intention"])
            self.assertNotIn(entry["id"], [visible["id"] for visible in boundary["history"]])
        server.stop(); player.stop(); watcher.stop()
        self.server(scenario="dungeon-loop")
        _, recovered = self.client(SPECTATOR_TOKEN, actor=2)
        self.assertEqual(recovered["state"], boundary["state"])
        self.assertEqual(recovered["history"], boundary["history"])
        self.assertEqual(recovered["intentions"], [])

    def test_running_attack_restart_release_resume_and_cancel_keep_original_identity(self):
        package = self.directory / "paused-attack"
        shutil.copytree(ROOT / "scenarios/tests/dungeon-loop", package)
        region = package / "regions/1.toml"
        source = region.read_text()
        declaration = 'controller = "ai", ai = "tactical"'
        self.assertEqual(source.count(declaration), 1)
        region.write_text(source.replace(declaration, 'controller = "external"')
                          .replace("max_hp = 5", "max_hp = 100"))
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server(scenario=package)
        player, _ = self.client()
        accepted = self.command(player, {"type": "act", "action": {"type": "attack", "target": "2"}})
        self.assertIsNone(accepted["error"])
        identity = accepted["intentions"][0]["intention"]
        started = self.frame(player, lambda frame: lifecycle(frame, identity, "started"))
        combat = started["state"]["observation"]["combat"]
        self.assertTrue(combat["preparation_active"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        # Stop the server while control remains connected: startup must recover
        # saved active progress without relying on disconnect to pause it first.
        server.stop(); player.stop()
        server = self.server(scenario=package)
        player, restored = self.client()
        self.assertEqual(restored["intentions"][0]["intention"], identity)
        self.assertEqual(restored["intentions"][0]["phase"], "paused")
        self.assertFalse(restored["state"]["observation"]["combat"]["preparation_active"])
        self.assertEqual(restored["state"]["observation"]["combat"]["preparation_remaining"],
                         combat["preparation_remaining"])
        watcher, _ = self.client(SPECTATOR_TOKEN)
        resumed = self.command(player, {"type": "resume_intention"})
        self.assertIsNone(resumed["error"])
        self.assertEqual(resumed["intentions"][0]["intention"], identity)
        self.assertEqual(resumed["intentions"][0]["phase"], "queued")
        self.assertFalse(resumed["state"]["observation"]["combat"]["preparation_active"])
        continued = self.frame(player, lambda frame: lifecycle(frame, identity, "started"))
        self.assertTrue(continued["state"]["observation"]["combat"]["preparation_active"])
        self.assertEqual(continued["state"]["observation"]["combat"]["preparation_remaining"],
                         combat["preparation_remaining"])
        self.request(player, {"type": "release_control"})
        suspended = self.frame(watcher, lambda frame: lifecycle(frame, identity, "paused"))
        self.assertFalse(suspended["state"]["observation"]["combat"]["preparation_active"])
        acquired = self.request(player, {"type": "acquire_control"})
        self.assertEqual(acquired["intentions"][0]["phase"], "paused")
        cancelled = self.command(player, {"type": "cancel_intention"})
        self.assertIsNone(cancelled["error"])
        self.assertEqual(cancelled["intentions"], [])
        self.assertIsNone(cancelled["state"]["observation"]["combat"]["preparation_remaining"])
        seen = self.frame(watcher, lambda frame: lifecycle(frame, identity, "cancelled"))
        self.assertEqual(seen["intentions"], [])
        self.assertEqual(seen["state"], cancelled["state"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        server.stop(); player.stop(); watcher.stop()
        self.server(scenario=package)
        _, recovered = self.client()
        self.assertEqual(recovered["intentions"], [])
        self.assertEqual(recovered["state"], cancelled["state"])

    def test_control_loss_and_restart_require_explicit_original_intention_resume_or_cancel(self):
        server = self.server(wizard=True)
        wizard = self.wizard()
        self.wizard_command(wizard, "actor 75 1 2 1 0")
        first, _ = self.client()
        second, _ = self.client(WIZARD_TOKEN, actor=2)
        accepted = self.command(second, {"type": "act", "action": {"type": "wait"}})
        identity = accepted["intentions"][0]["intention"]
        # Audit admissions are deliberately excluded from selectable history.
        # A disclosed wizard boundary can retain the queued work for rewind.
        self.request(wizard, {"type": "snapshot"})
        marker = self.wizard_command(wizard, "teleport 1 1 1 1 0")
        rewind_target = marker["history"][-1]["id"]
        self.request(second, {"type": "release_control"})
        suspended = self.frame(second, lambda frame: lifecycle(frame, identity, "suspended"))
        self.assertEqual(suspended["state"], accepted["state"])
        reacquired = self.request(second, {"type": "acquire_control"})
        self.assertEqual(reacquired["intentions"][0]["phase"], "suspended")
        self.assertIsNone(self.request(first, {"type": "save"})["error"])
        first.stop(); second.stop(); wizard.stop(); server.stop()
        server = self.server(wizard=True)
        first, _ = self.client()
        second, restored = self.client(WIZARD_TOKEN, actor=2)
        self.assertEqual(restored["intentions"][0]["intention"], identity)
        self.assertEqual(restored["intentions"][0]["phase"], "suspended")
        self.assertEqual(restored["state"], accepted["state"])
        def change(client, frame, operation, target=identity):
            return self.request(client, {"type": "command", "context": frame["input_context"], "branch": frame["branch"],
                "command": {"type": operation, "expected_revision": frame["state"]["revision"],
                            "intention": target}})
        observer, seen = self.client(WIZARD_TOKEN, observe=True, actor=2)
        denied = change(observer, seen, "resume_intention")
        self.assertTrue(denied["error"].startswith("NotController:"))
        resumed = self.command(second, {"type": "resume_intention"})
        self.assertIsNone(resumed["error"])
        self.assertEqual(resumed["intentions"][0]["intention"], identity)
        self.assertEqual(resumed["intentions"][0]["phase"], "queued")
        self.act(first, {"type": "wait"})
        resolved = self.frame(second, lambda frame: lifecycle(frame, identity, "resolved"))
        self.assertEqual(resolved["intentions"], [])
        self.act(second, {"type": "wait"})  # Actor 1 is now due; actor 2 can queue future work.
        queued = self.command(second, {"type": "act", "action": {"type": "wait"}})
        next_identity = queued["intentions"][0]["intention"]
        self.assertNotEqual(next_identity, identity)
        cancelled = self.command(second, {"type": "cancel_intention"})
        self.assertIsNone(cancelled["error"])
        self.assertEqual(cancelled["intentions"], [])
        self.assertEqual(cancelled["state"], queued["state"])
        self.assertTrue(any(entry["content"]["type"] == "action" for entry in cancelled["history"]))
        wizard = self.wizard()
        self.request(wizard, {"type": "snapshot"})
        self.wizard_command(wizard, f"rewind {rewind_target}")
        rewound = self.request(second, {"type": "snapshot"})
        self.assertNotEqual(rewound["branch"], cancelled["branch"])
        self.assertEqual(rewound["intentions"][0]["intention"], identity)
        self.assertEqual(rewound["intentions"][0]["phase"], "suspended")
        self.assertEqual(rewound["state"], accepted["state"])
        self.assertIsNone(change(second, rewound, "cancel_intention")["error"])
        self.assertIsNone(self.request(first, {"type": "save"})["error"])
        first.stop(); second.stop(); observer.stop(); wizard.stop(); server.stop()
        self.server(wizard=True)
        _, recovered = self.client(WIZARD_TOKEN, actor=2)
        self.assertEqual(recovered["branch"], rewound["branch"])
        self.assertEqual(recovered["intentions"], [])
        self.assertEqual(recovered["state"], rewound["state"])

    def test_queued_move_fails_without_reinterpreting_direction_after_teleport(self):
        self.server(wizard=True)
        wizard = self.wizard()
        self.wizard_command(wizard, "actor 75 1 2 1 0")
        first, _ = self.client()
        second, _ = self.client(WIZARD_TOKEN, actor=2)
        accepted = self.command(second, {"type": "act", "action": {
            "type": "move", "direction": "east"}})
        self.assertIsNone(accepted["error"])
        identity = accepted["intentions"][0]["intention"]
        self.wizard_command(wizard, "teleport 2 1 3 1 0")
        self.act(first, {"type": "wait"})
        failed = self.frame(second, lambda frame: lifecycle(frame, identity, "failed"))
        self.assertEqual(failed["intentions"], [])
        self.assertFalse(any(entry["content"]["type"] == "action"
                             for entry in failed["history"]))
        self.assertEqual(failed["state"]["observation"]["tick"], "0")

    def test_acceptance_precedes_effect_and_spectator_receives_ordered_resolution(self):
        self.server(scenario="travel")
        player, initial = self.client()
        watcher, _ = self.client(SPECTATOR_TOKEN)
        player.write(json.dumps({"type": "act", "action": {"type": "move", "direction": "east"}}))
        accepted = self.frame(player, lambda frame: (frame.get("message") or {}).get("type") == "ack")
        receipt = accepted["message"]["receipt"]
        self.assertEqual(receipt["type"], "admitted")
        self.assertEqual(receipt["phase"], "queued")
        self.assertIsInstance(receipt["intention"], str)
        self.assertEqual(accepted["state"], initial["state"])
        self.assertEqual(accepted["intentions"][0]["intention"], receipt["intention"])
        resolved = self.frame(player, lambda frame: lifecycle(frame, receipt["intention"], "resolved"))
        self.assertGreater(int(resolved["state"]["revision"]), int(initial["state"]["revision"]))
        self.assertGreater(int(resolved["state"]["observation"]["tick"]), int(initial["state"]["observation"]["tick"]))
        self.assertEqual(resolved["intentions"], [])
        queued_watch = self.frame(watcher, lambda frame: lifecycle(frame, receipt["intention"], "queued"))
        self.assertEqual(queued_watch["state"], initial["state"])
        resolved_watch = self.frame(watcher, lambda frame: lifecycle(frame, receipt["intention"], "resolved"))
        self.assertEqual(resolved_watch["state"], resolved["state"])
        self.assertEqual(resolved_watch["intentions"], [])


class IntentionNativeProcesses(ProcessTestCase):
    graphical = True

    def test_completed_action_clears_queue_and_refreshes_presented_status(self):
        self.server(scenario="travel")
        window, initial = self.window()
        completed = self.key(window, "wait")
        self.assertEqual(completed["intentions"], [])
        self.assertIsNone(completed["action_hint"])
        self.assertEqual(completed["status"], "Action: Resolved.")
        self.assertGreater(int(completed["state"]["observation"]["tick"]),
                           int(initial["state"]["observation"]["tick"]))

    def test_native_resume_and_cancel_keys_preserve_original_queue_identity(self):
        self.server(wizard=True)
        wizard = self.wizard()
        self.wizard_command(wizard, "actor 75 1 2 1 0")
        self.client()  # Keep actor 1 due, so actor 2's work remains queued.
        second, _ = self.client(WIZARD_TOKEN, actor=2)
        accepted = self.command(second, {"type": "act", "action": {"type": "wait"}})
        identity = accepted["intentions"][0]["intention"]
        self.request(second, {"type": "release_control"})
        self.frame(second, lambda frame: lifecycle(frame, identity, "suspended"))
        second.stop()
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--actor", "2",
            "--report-frames"], token=WIZARD_TOKEN)
        initial = self.ascii_frame(window, lambda frame: frame["state"] is not None
            and frame["has_control"] and not frame["busy"])
        self.assertEqual(initial["intentions"][0]["intention"], identity)
        self.assertEqual(initial["intentions"][0]["phase"], "suspended")
        self.assertIn("F8 resume", initial["action_hint"])
        key = self.native_keys(window)
        key("F8", True)
        try:
            resumed = self.ascii_frame(window, lambda frame: not frame["busy"]
                and any(status["intention"] == identity and status["phase"] == "queued"
                        for status in (frame.get("intentions") or [])))
        finally:
            key("F8", False)
        self.assertEqual(resumed["state"], initial["state"])
        self.assertIn("F9 cancel", resumed["action_hint"])
        key("F9", True)
        try:
            cancelled = self.ascii_frame(window, lambda frame: not frame["busy"]
                and frame.get("intentions") == [])
        finally:
            key("F9", False)
        self.assertEqual(cancelled["state"], initial["state"])
        self.assertEqual(cancelled["history"], initial["history"])
        self.assertIsNone(cancelled["action_hint"])
