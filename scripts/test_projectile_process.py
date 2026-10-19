"""Real source pickers, exact previews and projectile payment/recovery.

Default test_*_process discovery registers all methods. The graphical class
places the native keyboard witness in CI's exclusive native lane; only the Text
methods is explicitly non-graphical. No production test API or save mutation.
"""
import json
import shutil
import sqlite3
import unittest
from unittest.mock import Mock, patch

from process_harness import ProcessTestCase, non_graphical, resource
from projectile_package import projectile_package
from rogue_roster_package import unrelated_fallback
from stream_relay import StreamRelay


def view(frame):
    return frame["state"]["observation"]


def item(frame, name):
    return next(row for row in view(frame)["inventory"] if row["name"] == name)


def preview_lines(preview):
    rows = ["Fresh action: start cost is paid when preparation starts; resolution cost is reserved."]
    if preview["cost"] is None:
        rows.append("No resource charge.")
    else:
        for resource in ("stamina", "focus", "mana"):
            cost = preview["cost"][resource]
            if cost is not None:
                rows.append(f"Cost {cost['resource']['name']}: start {cost['start']}, resolution {cost['resolution']}")
    rows.append(f"Time to next strike: {preview['preparation']} ticks; recovery: {preview['recovery']} ticks.")
    return rows


class ProjectileProcesses(ProcessTestCase):
    graphical = True

    def setup_encounter(self):
        self.package = projectile_package(self)
        self.game = self.server("--checkpoint-interval", 1, scenario=self.package, seed=42)
        self.probe, initial = self.client(observe=True)
        self.target, target = self.client(actor=3)
        self.assertFalse(initial["state"]["wizard_game"])
        self.target_health = int(view(target)["combat"]["hp"])
        return initial

    def snapshot(self):
        return self.request(self.probe, {"type": "snapshot"})

    def settle(self):
        # The external target takes ordinary waits to advance the player's
        # finite preparation/recovery. Never mistake admission for completion.
        # 16 * 100 ticks covers this fixture's acquisition and equipment work.
        for _ in range(16):
            current = self.snapshot()
            if view(current)["ready"] and not current["intentions"]:
                return current
            peer = self.request(self.target, {"type": "snapshot"})
            self.assertTrue(view(peer)["ready"], "Fixture target cannot advance pending player work")
            self.assertIsNone(self.act(self.target, {"type": "wait"})["error"])
        self.fail("Projectile fixture did not settle within 1600 ticks of ordinary target waits")

    def capture_client(self, native=False, adventure=False):
        self.replies = []
        relay = StreamRelay(self.address, on_server_message=self.replies.append)
        self.addCleanup(relay.close)
        upstream = self.address
        self.address = relay.address
        try:
            if native:
                # --automation consumes stdin instead of native keyboard input.
                window = self.launch("tor-client-ascii", [
                    "--connect", self.address, "--report-frames"])
                frame = self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"])
                self.assertTrue(frame["window_open"])
                return window, frame
            return self.adventure() if adventure else self.text_client()
        finally:
            self.address = upstream

    def reply(self, kind, since):
        replies = [row for row in self.replies[since:] if row["type"] == kind]
        self.assertEqual(len(replies), 1, (kind, replies))
        return replies[0]

    def admissions(self, ability):
        # Read-only chronological evidence after a save barrier, never setup.
        self.flush_save()
        with sqlite3.connect(self.save) as db:
            rows = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 "
                              "UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 "
                              "ORDER BY sequence").fetchall()
        records = [json.loads(frame[24:])["record"]["entry"]["content"] for _, frame in rows]
        return [row for row in records if row["type"] == "intention_admitted"
                and row["action"].get("id") == ability]

    def assert_preview(self, text, reply, ability, delivery):
        preview = reply["preview"]
        self.assertEqual(preview["ability"], ability)
        self.assertEqual(preview["phase"], "fresh")
        self.assertEqual(preview["supplies"], {"kind": "carried"})
        self.assertEqual(preview["invocation"]["aim"]["direction"], "east")
        self.assertEqual(preview["invocation"]["aim"]["delivery"], delivery)
        for line in preview_lines(preview):
            self.assertIn(line, text)
        return preview

    def learn_knife(self, text):
        self.assertIn("Starting skills", text.command("advancement"))
        preview = text.command("advancement purchase 1 advancement expertise rogue.knife")
        self.assertIn("expertise rogue.knife", preview)
        self.assertIn("Confirm", preview)
        self.assertNotIn("Server error", text.command("advancement confirm"))
        state = self.settle()
        self.assertIn({"expertise": {"id": "rogue.knife", "name": "Knife"},
                       "status": "active", "source_count": 1},
                      view(state)["combat"]["own_stats"]["expertises"])
        return state

    def text_throw_preview(self, text):
        start = len(self.replies)
        self.assertIn("dagger", text.command("throw"))
        source_reply = self.reply("projectile_choices", start)
        page = source_reply["page"]
        self.assertEqual(page["query"]["phase"], {"kind": "sources"})
        self.assertEqual(len(page["choices"]), 1)
        choice = page["choices"][0]
        self.assertEqual(choice["kind"], "throw")
        self.assertEqual(choice["source"]["socket"], 10)
        self.assertIn("Which direction?", text.command("1"))
        start = len(self.replies)
        output = text.command("east")
        preview = self.assert_preview(output, self.reply("ability_weapon_preview", start),
                                      "rogue.throw", choice)
        self.assertIn("Use this exact weapon action? (yes/no)", output)
        self.assertGreater(sum(cost["start"] + cost["resolution"]
                               for cost in preview["cost"].values() if cost), 0)
        return preview

    @non_graphical
    def test_text_expertise_throw_launch_payment_recovery_and_source_free_restart(self):
        initial = self.setup_encounter()
        text, _ = self.capture_client()
        start = len(self.replies)
        self.assertIn("No choices on this page.", text.command("throw"))
        self.assertEqual(self.reply("projectile_choices", start)["page"]["choices"], [])
        text.command("cancel")
        self.assertEqual(self.snapshot()["state"], initial["state"])
        learned = self.learn_knife(text)
        dagger = item(learned, "dagger")["id"]
        self.text_throw_preview(text)
        self.assertEqual(self.snapshot()["state"], learned["state"])
        text.command("no")
        self.assertEqual(self.snapshot()["state"], learned["state"])
        self.assertEqual(self.admissions("rogue.throw"), [])
        terms = self.text_throw_preview(text)
        before_focus = resource(learned, "focus")["balance"]
        self.assertNotIn("Server error", text.command("yes"))
        started = self.snapshot()
        focus = terms["cost"]["focus"]
        self.assertEqual(resource(started, "focus")["balance"], before_focus - focus["start"])
        self.assertEqual(resource(started, "focus")["reserved"], focus["resolution"])
        thrown = self.settle()
        # Both start and settlement occur before the production 300-tick Focus
        # recovery period, from the initial full pool (the purchase costs 0 ticks).
        self.assertLess(int(view(thrown)["tick"]), 300)
        self.assertEqual(resource(thrown, "focus")["reserved"], 0)
        self.assertEqual(resource(thrown, "focus")["balance"],
                         before_focus - focus["start"] - focus["resolution"])
        self.assertEqual(len(self.admissions("rogue.throw")), 1)
        self.assertFalse(any(row["id"] == dagger for row in view(thrown)["inventory"]))
        landed = next(row for row in view(thrown)["ground_items"] if row["item"]["id"] == dagger)
        self.assertEqual(landed["item"]["quantity"], "1")
        self.assertLess(int(view(self.request(self.target, {"type": "snapshot"}))["combat"]["hp"]), self.target_health)
        self.assertNotIn("Server error", text.command("equip shortbow"))
        equipped = self.settle()
        bow = item(equipped, "shortbow")["id"]
        arrow = item(equipped, "arrow")["id"]
        start = len(self.replies)
        self.assertIn("shortbow", text.command("launch"))
        launchers = self.reply("projectile_choices", start)["page"]
        self.assertEqual(len(launchers["choices"]), 1)
        launcher = launchers["choices"][0]["launcher"]
        self.assertEqual(launcher["item"], bow)
        start = len(self.replies)
        ammunition_text = text.command("1")
        ammunition = self.reply("projectile_choices", start)["page"]
        self.assertEqual(ammunition["query"]["phase"], {"kind": "ammunition", "launcher": launcher})
        self.assertEqual(ammunition["choices"], [{"kind": "ammunition", "item": arrow}])
        self.assertIn("compatible ammunition", ammunition_text)
        self.assertIn("arrow", ammunition_text)
        self.assertIn("Which direction?", text.command("1"))
        start = len(self.replies)
        self.assert_preview(text.command("east"), self.reply("ability_weapon_preview", start),
                            "rogue.launch", {"kind": "launch", "launcher": launcher, "ammunition": arrow})
        self.assertEqual(self.snapshot()["state"], equipped["state"])
        self.assertNotIn("Server error", text.command("yes"))
        launched = self.settle()
        final = launched
        self.assertEqual(item(final, "arrow")["quantity"], "5")
        self.assertEqual(item(final, "shortbow")["id"], bow)
        self.assertFalse(any(row["item"]["name"] == "arrow" for row in view(final)["ground_items"]))
        self.assertEqual(len(self.admissions("rogue.launch")), 1)
        self.assertEqual(len(self.admissions("rogue.throw")), 1)
        # Vacate the hit cell through ordinary play, then recover the same
        # durable unit. The target is adjacent; a pickup requires its exact cell.
        moved = self.command(self.target, {"type": "act", "action": {"type": "move", "direction": "north"}})
        self.assertIsNone(moved["error"])
        self.assertNotIn("Server error", text.command("wait"))
        self.settle()
        self.assertNotIn("Server error", text.command("east"))
        at_landing = self.settle()
        self.assertTrue(next(row for row in view(at_landing)["ground_items"]
                             if row["item"]["id"] == dagger)["reachable"])
        self.assertIn("Taken", text.command(f"take #{dagger}"))
        final = self.settle()
        self.assertEqual(item(final, "dagger")["quantity"], "1")
        self.flush_save()
        text.stop(); self.probe.stop(); self.target.stop(); self.game.stop()
        shutil.rmtree(self.package)
        self.server(scenario=unrelated_fallback(self), seed=None)
        resumed, restored = self.client()
        self.assertEqual(restored["state"], final["state"])
        self.assertEqual(restored["history"], final["history"])
        self.assertEqual(restored["intentions"], [])

    @non_graphical
    def test_adventure_projectile_menu_and_exact_preview_gate(self):
        self.setup_encounter()
        setup, _ = self.text_client()
        self.learn_knife(setup)
        setup.stop()
        text, _ = self.capture_client(adventure=True)
        before = self.snapshot()

        def choose():
            start = len(self.replies)
            self.assertIn("dagger", self.say(text, "throw"))
            source = self.reply("projectile_choices", start)["page"]["choices"][0]
            self.assertIn("Which direction?", self.say(text, "1"))
            start = len(self.replies)
            output = self.say(text, "east")
            self.assert_preview(output, self.reply("ability_weapon_preview", start), "rogue.throw", source)

        choose()
        self.say(text, "no")
        self.assertEqual(self.snapshot()["state"], before["state"])
        self.assertEqual(self.admissions("rogue.throw"), [])
        choose()
        self.say(text, "yes")
        final = self.settle()
        self.assertEqual(len(self.admissions("rogue.throw")), 1)
        self.assertFalse(any(row["name"] == "dagger" for row in view(final)["inventory"]))
        self.assertEqual(sum(int(row["item"]["quantity"]) for row in view(final)["ground_items"]
                             if row["item"]["name"] == "dagger"), 1)

    def test_native_projectile_source_direction_preview_cancel_and_confirm(self):
        self.setup_encounter()
        setup, _ = self.text_client()
        self.learn_knife(setup)
        setup.stop()
        window, initial = self.capture_client(native=True)
        native = self.native_keys(window)

        def press(name, key):
            return self.native_press(window, native, name, key)

        def choose_throw():
            menu = press("z", "abilities")
            index = [row["id"] for row in menu["ability_choices"]].index("rogue.throw")
            for _ in range(index):
                press("Down", "down")
            sources = press("Return", "enter")
            self.assertIn("dagger", "\n".join(sources["projectile_rows"]))
            press("Return", "enter")
            start = len(self.replies)
            preview = press("Right", "right")
            server = self.reply("ability_weapon_preview", start)
            self.assertEqual(preview["weapon_preview"]["preview"], server["preview"])
            for line in preview_lines(server["preview"]):
                self.assertIn(line, "\n".join(preview["weapon_preview_rows"]))
            return preview

        preview = choose_throw()
        self.assertEqual(preview["state"], initial["state"])
        cancelled = press("Escape", "escape")
        self.assertIsNone(cancelled["weapon_preview"])
        self.assertEqual(cancelled["state"], initial["state"])
        self.assertEqual(self.admissions("rogue.throw"), [])
        choose_throw()
        committed = press("Return", "enter")
        self.assertTrue(committed["window_open"])
        self.assertEqual(len(self.admissions("rogue.throw")), 1)
        final = self.settle()
        self.assertFalse(any(row["name"] == "dagger" for row in view(final)["inventory"]))
        self.assertEqual(sum(int(row["item"]["quantity"]) for row in view(final)["ground_items"]
                             if row["item"]["name"] == "dagger"), 1)

        # Prepare the launcher with the ordinary headless equipment action,
        # then exercise the separate native launcher → ammunition path.
        window.stop()
        owner, owned = self.client()
        self.assertIsNone(self.act(owner, {"type": "equip", "item": item(owned, "shortbow")["id"], "slot": 0})["error"])
        self.settle()
        owner.stop()
        window, initial = self.capture_client(native=True)
        native = self.native_keys(window)
        menu = press("z", "abilities")
        index = [row["id"] for row in menu["ability_choices"]].index("rogue.launch")
        for _ in range(index):
            press("Down", "down")
        sources = press("Return", "enter")
        self.assertIn("shortbow", "\n".join(sources["projectile_rows"]))
        start = len(self.replies)
        ammunition = press("Return", "enter")
        page = self.reply("projectile_choices", start)["page"]
        self.assertEqual(page["query"]["phase"]["kind"], "ammunition")
        self.assertEqual(page["choices"], [{"kind": "ammunition", "item": item(initial, "arrow")["id"]}])
        self.assertIn("compatible ammunition", "\n".join(ammunition["projectile_rows"]))
        press("Return", "enter")
        start = len(self.replies)
        preview = press("Right", "right")
        server = self.reply("ability_weapon_preview", start)
        self.assertEqual(preview["weapon_preview"]["preview"], server["preview"])
        for line in preview_lines(server["preview"]):
            self.assertIn(line, "\n".join(preview["weapon_preview_rows"]))
        self.assertEqual(preview["state"], initial["state"])
        press("Return", "enter")
        launched = self.settle()
        self.assertEqual(item(launched, "arrow")["quantity"], "5")
        self.assertEqual(item(launched, "shortbow")["id"], item(initial, "shortbow")["id"])
        self.assertFalse(any(row["item"]["name"] == "arrow" for row in view(launched)["ground_items"]))
        self.assertEqual(len(self.admissions("rogue.launch")), 1)


class ProjectileCaptureLaunch(unittest.TestCase):
    """Launch wiring checks do not start a server or a native window."""

    def test_native_capture_keeps_os_input_and_relayed_frame_reports(self):
        harness = ProjectileProcesses()
        harness.address = "127.0.0.1:41001"
        window = object()
        initial = {"state": {"observation": {}}, "busy": False, "window_open": True}
        relay = Mock(address="127.0.0.1:41002")

        def presented(client, predicate):
            self.assertIs(client, window)
            self.assertFalse(predicate({**initial, "state": None}))
            self.assertFalse(predicate({**initial, "busy": True}))
            self.assertTrue(predicate(initial))
            return initial

        with patch(f"{__name__}.StreamRelay", return_value=relay) as capture, \
                patch.object(harness, "launch", return_value=window) as launch, \
                patch.object(harness, "ascii_frame", side_effect=presented):
            self.addCleanup(harness.doCleanups)
            self.assertEqual(harness.capture_client(native=True), (window, initial))
        launch.assert_called_once_with("tor-client-ascii", [
            "--connect", relay.address, "--report-frames"])
        self.assertEqual(harness.address, "127.0.0.1:41001")
        self.assertEqual(capture.call_args.args, (harness.address,))
        message = {"type": "projectile_choices"}
        capture.call_args.kwargs["on_server_message"](message)
        self.assertEqual(harness.replies, [message])
        harness.doCleanups()
        relay.close.assert_called_once_with()


if __name__ == "__main__":
    unittest.main()
