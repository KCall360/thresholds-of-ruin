"""Saved intention admission and execution through the real server and clients."""
import json

from process_harness import ProcessTestCase, SPECTATOR_TOKEN, WIZARD_TOKEN


def lifecycle(frame, identity, phase):
    message = frame.get("message") or {}
    body = (message.get("update") or {}).get("body") or {}
    status = body.get("status") or {}
    return (message.get("type") == "update" and body.get("type") == "intention"
            and status.get("intention") == identity and status.get("phase") == phase)


class IntentionProcesses(ProcessTestCase):
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
            return self.request(client, {"type": "command", "branch": frame["branch"],
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
        self.assertEqual(failed["state"]["observation"]["tick"], 0)

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
        self.assertGreater(resolved["state"]["revision"], initial["state"]["revision"])
        self.assertGreater(resolved["state"]["observation"]["tick"], initial["state"]["observation"]["tick"])
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
        self.assertGreater(completed["state"]["observation"]["tick"],
                           initial["state"]["observation"]["tick"])

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
