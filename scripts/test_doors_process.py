"""Door actions through the actual text, headless and native ASCII clients."""
import json
import os
import threading
from pathlib import Path
import unittest

from process_harness import ProcessTestCase, SPECTATOR_TOKEN, door, non_graphical
from stream_relay import StreamRelay


class DoorProcesses(ProcessTestCase):
    graphical = True

    def test_normal_text_intention_native_ascii_actions_spectators_and_restart(self):
        server = self.server()
        player, welcome = self.adventure()
        self.assertIn("open wooden door", welcome)
        self.assertIn("iron handle", self.say(player, "examine door"))
        self.assertEqual(self.say(player, "close it"), "You walk over to the wooden door and close it.\n> ")
        observer, closed = self.client(SPECTATOR_TOKEN)
        self.assertFalse(door(closed)["open"])
        self.assertEqual(closed["state"]["observation"]["tick"], '400')
        self.flush_save()
        before = self.save.read_bytes()
        denied = self.act(observer, {"type":"set_door", "door":door(closed)["id"], "open":True})
        self.assertIsNotNone(denied["error"])
        self.assertEqual(self.save.read_bytes(), before)
        self.say(player, "release")
        capture = Path(os.environ.get("TOR_DOOR_CAPTURE", str(self.save.parent / "doors.ppm")))
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames", "--capture", capture])
        initial = self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"])
        self.assertTrue(initial["has_control"])
        doorway = next(c for c in initial["state"]["observation"]["visible_cells"] if c.get("door"))
        self.assertFalse(doorway["place_hint"])
        self.assertFalse(any(i["item"]["name"] == "stone tablet" for i in initial["state"]["observation"]["ground_items"]))
        key = self.native_keys(window)
        key("o", True)
        prompt = self.ascii_frame(window, lambda f: f.get("door_direction") is True)
        self.assertEqual(prompt["state"], initial["state"])
        key("o", False)
        key("Up", True)
        missing = self.ascii_frame(window, lambda f: f["status"] == "There is no door in that direction.")
        key("Up", False)
        self.assertEqual(missing["state"], initial["state"])
        self.assertEqual(self.save.read_bytes(), before)
        key("o", True)
        self.ascii_frame(window, lambda f: f.get("door_direction") is True)
        key("o", False)
        key("Right", True)
        opened = self.ascii_frame(window, lambda f: door(f)["open"] and not f["busy"])
        key("Right", False)
        self.assertTrue(door(opened)["open"])
        key("c", True)
        self.ascii_frame(window, lambda f: f.get("door_direction") is False)
        key("c", False)
        key("Right", True)
        closed_again = self.ascii_frame(window, lambda f: not door(f)["open"] and not f["busy"])
        key("Right", False)
        self.assertFalse(door(closed_again)["open"])
        watched = self.request(observer, {"type":"snapshot"})
        self.assertDisclosedStateEqual(watched["state"], closed_again["state"])
        self.assertEqual(watched["history"][-1]["content"]["event"]["type"], "door_changed")
        # Native S saves and closes; successful exit destroys the key target.
        key("Shift_L", True)
        key("s", True)
        self.assertEqual(window.child.wait(timeout=15), 0)
        self.assertTrue(capture.read_bytes().startswith(b"P6\n1200 800\n255\n"))
        player.stop(); observer.stop(); server.stop()
        self.server()
        _, resumed = self.client(SPECTATOR_TOKEN)
        self.assertDisclosedStateEqual(resumed["state"], closed_again["state"])
        self.assertFalse(resumed["state"]["wizard_game"])

    @non_graphical
    def test_wizard_occlusion_explicit_open_travel_memory_and_rewind(self):
        server = self.server(wizard=True, scenario="doors")
        wizard = self.wizard()
        observer, initial = self.client(SPECTATOR_TOKEN)
        player, welcome = self.adventure()
        self.assertIn("closed wooden door", welcome)
        self.assertNotIn("stone tablet", welcome)
        self.assertFalse(door(initial)["open"])
        self.assertTrue(all(key in {c["key"] for c in initial["state"]["observation"]["visible_cells"]} for key in door(initial)["approaches"]))
        self.assertEqual(self.say(player, "open door"), "You walk over to the wooden door and open it.\n> ")
        opened = self.request(observer, {"type":"snapshot"})
        self.assertEqual(opened["state"]["observation"]["tick"], '600')
        self.assertIn("stone tablet", self.say(player, "look"))
        tablet_cell = next(c for c in opened["state"]["observation"]["visible_cells"] if c["position"] == {"x":3,"y":0,"z":0})
        self.assertEqual(self.say(player, "close door"), "You close the wooden door.\n> ")
        hidden = self.request(observer, {"type":"snapshot"})
        self.assertNotIn("stone tablet", json.dumps(hidden["state"]))
        self.assertIn("stone tablet", json.dumps(hidden["memory"]))
        self.say(player, "release")
        controller, current = self.client()
        blocked = self.request(controller, {"type":"command", "context": current["input_context"], "branch":current["branch"], "command":{"type":"travel", "expected_revision":current["state"]["revision"], "destination":tablet_cell["key"]}})
        self.assertIsNotNone(blocked["error"])
        self.assertEqual(blocked["state"], current["state"])
        controller.stop()
        for client in (player, observer, wizard): client.stop()
        server.stop()
        self.server(wizard=True)
        wizard = self.wizard()
        observer, resumed = self.client(SPECTATOR_TOKEN)
        self.assertDisclosedStateEqual(resumed["state"], hidden["state"])
        self.wizard_command(wizard, "rewind initial")
        rewound = self.request(observer, {"type":"snapshot"})
        self.assertNotEqual(rewound["branch"], resumed["branch"])
        self.assertFalse(door(rewound)["open"])
        self.assertFalse(any(c["key"] == tablet_cell["key"] for c in rewound["memory"]))

    @non_graphical
    def test_stop_and_rewind_discard_pending_door_action(self):
        self.server(wizard=True, scenario="doors")
        wizard = self.wizard()
        hold_arrival = threading.Event()
        arrival_held = threading.Event()

        def hold_pending_door(message):
            body = message.get("update", {}).get("body", {})
            if (hold_arrival.is_set() and body.get("type") == "travel"
                    and body["status"]["phase"] == "arrived"):
                hold_arrival.clear()
                relay.gate.clear()
                arrival_held.set()
                # Hold this exact arrival before delivery, not merely the next
                # frame. The real server remains free to finish and rewind.
                relay.gate.wait()

        relay = StreamRelay(self.address, on_server_message=hold_pending_door)
        self.addCleanup(relay.close)
        upstream = self.address
        try:
            self.address = relay.address
            player, _ = self.adventure()
        finally:
            self.address = upstream
        observer, initial = self.client(SPECTATOR_TOKEN)
        # A turn runs to its end before the next command is read.
        self.send(player, "open door")
        self.send(player, "stop")
        output = player.until(lambda line: line == "There's nothing to stop.")
        self.assertIn("You walk over to the wooden door and open it.", output)
        player.until(lambda line: line == "> ")
        opened = self.request(observer, {"type":"snapshot"})
        self.assertTrue(door(opened)["open"])
        self.wizard_command(wizard, "rewind initial")
        self.say(player, "look")
        # Pace controls presentation, not authority. Hold the real arrival so
        # Text still owns its pending door goal while the approach completes.
        hold_arrival.set()
        try:
            self.send(player, "open door")
            self.assertTrue(arrival_held.wait(15), "Text's approach arrival was not held")
            arrived = self.request(observer, {"type":"snapshot"})
            self.assertEqual(arrived["travel"]["phase"], "arrived")
            self.assertFalse(door(arrived)["open"])
            self.assertGreater(int(arrived["travel"]["completed_steps"]), 0)
            self.assertFalse(any(h["content"].get("action", {}).get("type") == "set_door"
                                 for h in arrived["history"]))
            # No authority is moving now: refresh and rewind cannot race a
            # travel step. The ordered reset must discard Text's pending goal.
            self.wizard_command(wizard, "rewind initial")
            restored = self.request(observer, {"type":"snapshot"})
            self.assertNotEqual(restored["branch"], arrived["branch"])
            self.assertNotEqual(restored["branch"], initial["branch"])
            self.assertEqual(restored["state"]["observation"]["tick"], '0')
            self.assertFalse(door(restored)["open"])
        finally:
            relay.gate.set()
        # Consume the pending turn's prompt before issuing a real barrier query.
        self.assertNotIn("and open it", player.until(lambda line: line == "> "))
        # A barrier query waits until Text has handled the ordered reset.
        self.assertNotIn("and open it", self.say(player, "look"))
        final = self.request(observer, {"type":"snapshot"})
        self.assertEqual(final["state"]["observation"]["tick"], '0')
        self.assertFalse(door(final)["open"])
        self.assertFalse(any(h["content"].get("action", {}).get("type") == "set_door"
                             for h in final["history"]))
        self.assertEqual(relay.errors, [])

    @non_graphical
    def test_rotated_aperture_door_remains_an_ordinary_visible_object(self):
        self.server(scenario="doors-rotated")
        player, welcome = self.adventure()
        self.assertIn("A closed wooden door leads east.", welcome)
        self.assertNotIn("stone tablet", welcome)
        self.assertEqual(self.say(player, "open door"), "You open the wooden door.\n> ")
        self.assertIn("stone tablet", self.say(player, "look"))
        self.say(player, "step east")
        self.say(player, "step east")
        self.assertEqual(self.say(player, "close door"), "You close the wooden door.\n> ")
        observer, view = self.client(SPECTATOR_TOKEN)
        self.assertFalse(door(view)["open"])
        door_cell = next(c for c in view["state"]["observation"]["visible_cells"] if c.get("door"))
        self.assertEqual(door_cell["position"], {"x":-1,"y":0,"z":0})
        for forbidden in ("Private", "region", "portal", "quarter_turns"):
            self.assertNotIn(forbidden, json.dumps(view))

    @non_graphical
    def test_arrival_revealing_an_actor_does_not_open_the_door(self):
        self.server(scenario="doors-arrival-hazard")
        # The door is beyond diagonal reach. The south approach is walled off;
        # the first eastward step reveals the actor before manipulation.
        player, _ = self.adventure()
        output = self.say(player, "open door, then wait")
        self.assertIn("figure comes into view", output)
        self.assertNotIn("and open it", output)
        _, state = self.client(SPECTATOR_TOKEN)
        self.assertFalse(door(state)["open"])
        self.assertEqual(state["travel"]["phase"], "arrived")
        self.assertEqual(state["travel"]["completed_steps"], "1")
        self.assertFalse(any(h["content"].get("action", {}).get("type") == "set_door" for h in state["history"]))
        self.assertFalse(any(h["content"].get("action", {}).get("type") == "wait" for h in state["history"]))

if __name__ == "__main__":
    unittest.main()

