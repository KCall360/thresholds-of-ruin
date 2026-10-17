"""Exact linked-Held selection through real Text, native ASCII and durable saves.

Service contracts own hidden-holder disclosure and stale/retried requests. These
witnesses own actual client input and source-free recovery of paid Escape work.
"""
import shutil
import unittest

from linked_hold_package import LINK, NAMES, prepare
from process_harness import ProcessTestCase, ROOT, non_graphical
from status_condition_package import checkpoint, combat, observation


def holds(frame):
    return observation(frame)["linked_holds"]


def settled(frame):
    return (observation(frame)["ready"]
            and combat(frame)["preparation_remaining"] is None
            and int(combat(frame)["recovery_remaining"]) == 0
            and not frame["intentions"])


class LinkedHoldProcesses(ProcessTestCase):
    graphical = True

    def snapshot(self, client):
        frame = self.request(client, {"type": "snapshot"})
        self.assertIsNone(frame["error"])
        return frame

    def advance(self, subject, controllers, predicate):
        """Only ordinary ready external peers may advance this finite fixture."""
        for _ in range(40):
            current = self.snapshot(subject)
            if predicate(current):
                return current
            for client in controllers.values():
                frame = self.snapshot(client)
                if frame["readiness"]["admission"] and observation(frame)["ready"]:
                    self.assertIsNone(self.act(client, {"type": "wait"})["error"])
        self.fail("Linked-hold fixture did not reach its bounded decision: "
                  + repr({"observation": observation(current),
                          "intentions": current["intentions"]}))

    def establish(self):
        package = prepare(self)
        server = self.server("--checkpoint-interval", 1, scenario=package)
        peers = {actor: self.client(actor=actor)[0] for actor in (1, 2, 3)}
        for actor in (2, 3):
            frame = self.snapshot(peers[actor])
            target = next(row["id"] for row in observation(frame)["visible_actors"]
                          if row["name"] == NAMES[0])
            # Admission queues both contacts before the subject yields its turn.
            # act() would wait for execution while that external turn is held.
            admitted = self.command(peers[actor], {"type": "act", "action": {
                "type": "use_ability", "id": LINK,
                "invocation": {"kind": "actor_target", "target": target},
                "supplies": {"kind": "carried"}}})
            self.assertIsNone(admitted["error"])
        frame = self.advance(peers[1], peers, lambda value: len(holds(value)) == 2 and settled(value))
        self.assertEqual(len({row["target"] for row in holds(frame)}), 2)
        self.assertTrue(all(row["target"].startswith("h_") for row in holds(frame)))
        self.assertFalse(frame["state"]["wizard_game"], "ordinary contact must establish both holds")
        return package, server, peers, frame

    @non_graphical
    def test_text_explicit_selectors_release_one_hold_at_a_time_before_movement(self):
        _, _, peers, initial = self.establish()
        subject = peers.pop(1)
        first, _ = holds(initial)
        start = checkpoint(self, subject)["actors"]["1"]["location"]
        blocked = self.act(subject, {"type": "move", "direction": "west"})
        self.assertIsNotNone(blocked["error"])
        self.assertEqual(self.snapshot(subject)["state"], initial["state"])
        self.assertIsNone(self.request(subject, {"type": "release_control"})["error"])
        text, _ = self.text_client()
        choices = text.command("escape")
        self.assertIn("Choose a hold", choices)
        for number, offer in enumerate(holds(initial), 1):
            self.assertIn(f"{number}. {NAMES[number]} (#{offer['target']})", choices)
        self.assertEqual(self.snapshot(subject)["state"], initial["state"],
                         "an ambiguous Text command must not select either hold")

        # One process covers both canonical selector forms without extra fixtures.
        for selector, expected in (("2", [first]), ("#" + first["target"], [])):
            with self.subTest(selector=selector):
                self.assertIn("You begin trying to escape a hold.",
                              text.command("escape hold " + selector))
                after = self.advance(subject, peers,
                    lambda value: holds(value) == expected and settled(value))
                self.assertEqual(observation(after)["position"], observation(initial)["position"])
                self.assertEqual(checkpoint(self, subject)["actors"]["1"]["location"], start)
                presented = text.command("stats")
                self.assertEqual("Holds (escape hold" in presented, bool(expected))
                if expected:
                    self.assertIn(f"1. {NAMES[1]} (#{first['target']})", presented)
                self.assertNotIn(f"2. {NAMES[2]}", presented)
                if expected:
                    self.assertIn("Server error", text.command("west"))
                    self.assertEqual(self.snapshot(subject)["state"], after["state"],
                                     "escaping the selected second hold must leave the first blocking movement")
        text.command("west")
        moved = self.advance(subject, peers, settled)
        # Observation positions are body-relative and stay at the origin. The
        # durable physical location witnesses the actual successful translation.
        destination = {**start, "position": {**start["position"],
                       "x": start["position"]["x"] - 1}}
        self.assertEqual(checkpoint(self, subject)["actors"]["1"]["location"],
                         destination)
        self.assertEqual(holds(moved), [])

    def test_native_f11_explicit_offer_selection_escapes_the_second_hold(self):
        _, _, peers, initial = self.establish()
        subject = peers.pop(1)
        first, second = holds(initial)
        self.assertIsNone(self.request(subject, {"type": "release_control"})["error"])
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        shown = self.ascii_frame(window, lambda frame: frame["has_control"] and not frame["busy"])
        self.assertIn("Held:2 (F11)", "\n".join(shown["status_lines"]))
        native = self.native_keys(window)
        if shown["messages"]["more"]:
            shown = self.native_press(window, native, "Escape", "escape")
            self.assertFalse(shown["messages"]["more"])
        choices = self.native_press(window, native, "F11", "escape_hold")
        self.assertEqual(choices["hold_choices"], [first, second])
        self.assertEqual(len(choices["hold_rows"]), 2)
        self.assertEqual(self.snapshot(subject)["state"], initial["state"],
                         "opening F11 must not submit an implicit first choice")
        self.native_press(window, native, "Down", "down")
        selected = self.native_press(window, native, "Return", "enter")
        self.assertEqual(selected["hold_choices"], [])
        self.advance(subject, peers, lambda value: holds(value) == [first] and settled(value))
        shown = self.ascii_frame(window, lambda frame: holds(frame) == [first] and settled(frame))
        self.assertIn("Held:1 (F11)", "\n".join(shown["status_lines"]))
        self.assertTrue(shown["window_open"])

    @non_graphical
    def test_pending_escape_reopens_without_sources_with_the_exact_paid_plan(self):
        package, server, peers, initial = self.establish()
        subject = peers[1]
        first, second = holds(initial)
        original = checkpoint(self, subject)
        links = original["actors"]["1"]["holds"]["incoming"]
        self.assertEqual(len(links), 2)
        chosen = links[1]["link"]
        started = self.act(subject, {"type": "escape", "hold": second["target"]})
        self.assertIsNone(started["error"])
        self.assertTrue(combat(started)["preparation_active"])
        self.assertIsNone(self.request(subject, {"type": "release_control"})["error"])
        paused = self.snapshot(subject)
        if not any(row["phase"] == "paused" for row in paused["intentions"]):
            paused = self.frame(subject, lambda frame: any(
                row["phase"] == "paused" for row in frame["intentions"]))
        saved = checkpoint(self, subject)
        pending = saved["actors"]["1"]["pending"]
        self.assertFalse(pending["active"])
        self.assertGreater(pending["remaining"], 0)
        self.assertLessEqual(pending["remaining"], 100)
        self.assertEqual(pending["plan"]["kind"], "escape")
        self.assertEqual(pending["plan"]["hold"], chosen["id"])
        self.assertEqual(pending["plan"]["holder"], chosen["holder"])
        self.assertEqual(pending["plan"]["policy"], chosen["policy"])
        self.assertEqual(pending["plan"]["preparation"], 100)
        self.assertEqual(pending["plan"]["recovery"], 100)
        self.assertEqual(saved["rules_rng"], original["rules_rng"],
                         "paid preparation must not roll the opposed check before resolution")
        self.assertEqual(saved["actors"]["1"]["holds"], original["actors"]["1"]["holds"])
        self.assertEqual(holds(paused), [first, second])
        # Explicit save is the asserted durable boundary; shutdown adds no save.
        for client in peers.values():
            client.stop()
        server.stop(save=False)
        shutil.rmtree(package)
        self.assertFalse(package.exists())
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        peers = {actor: self.client(actor=actor)[0] for actor in (1, 2, 3)}
        subject = peers.pop(1)
        restored = self.snapshot(subject)
        self.assertEqual(restored["state"], paused["state"])
        self.assertEqual(restored["intentions"], paused["intentions"])
        self.assertEqual(checkpoint(self, subject), saved,
                         "source-free reopen must preserve the complete saved plan, clocks, links and RNG")
        self.assertIsNone(self.command(subject, {"type": "resume_intention"})["error"])
        finished = self.advance(subject, peers, lambda value: holds(value) == [first] and settled(value))
        self.assertEqual(observation(finished)["position"], observation(initial)["position"])
        completed = checkpoint(self, subject)
        self.assertIsNone(completed["actors"]["1"]["pending"])
        incoming = completed["actors"]["1"]["holds"]["incoming"]
        self.assertEqual([row["link"] for row in incoming if row["kind"] == "linked"], [links[0]["link"]])
        self.assertEqual([row["holder"] for row in incoming if row["kind"] == "protection"], [chosen["holder"]])


if __name__ == "__main__":
    unittest.main()
