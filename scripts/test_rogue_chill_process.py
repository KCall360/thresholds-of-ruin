"""New reconstruction: exact CH1 and actual Mage kit through real applications.

Controlled ice uses unchanged checked content. A bounded seed witness proves the
lifecycle; it is not an autonomous fight, food study or accepted win rate.
"""
from scalar_wire import u64
import shutil
import unittest
from unittest.mock import Mock
from process_harness import AdventureProcess, ProcessTestCase, SPECTATOR_TOKEN, underfoot
from rogue_chill_package import CHILL, FROZEN, chill_package
from rogue_roster_package import unrelated_fallback
from status_condition_package import checkpoint


def view(frame):
    return frame["state"]["observation"]


def phase(frame):
    return next((row["phase"] for row in view(frame)["combat"]["own_stats"]["conditions"]
                 if row["condition"]["id"] == FROZEN), None)


class RogueChillProcesses(ProcessTestCase):
    def finish_resumed_care(self, player, ice, identity):
        def active_or_complete(frame):
            self.assertIsNone(frame["error"])
            self.assertNotEqual((frame.get("message") or {}).get("type"), "error")
            self.assertFalse(view(frame)["combat"]["terminal"])
            if not frame["intentions"]:
                self.assertFalse(view(frame)["combat"]["preparation_active"])
                return True
            self.assertEqual(len(frame["intentions"]), 1)
            pending, = frame["intentions"]
            self.assertEqual(pending["intention"], identity)
            self.assertIn(pending["phase"], ("queued", "started"))
            return pending["phase"] == "started" and view(frame)["combat"]["preparation_active"]

        resumed = self.command(player, {"type": "resume_intention"})
        if not active_or_complete(resumed):
            self.frame(player, active_or_complete)
        for _ in range(12):
            complete = self.request(player, {"type": "snapshot"})
            if not complete["intentions"]:
                break
            refreshed = self.request(ice, {"type": "snapshot"})
            self.assertIsNone(refreshed["error"])
            self.assertTrue(refreshed["readiness"]["admission"])
            self.assertIsNone(self.act(ice, {"type": "wait"})["error"])
        else:
            self.fail("Explicitly resumed care failed to complete")
        return complete

    def test_actual_checked_chill_preserves_care_holds_through_restart_and_explicit_thaw_resume(self):
        package = chill_package(self)
        witness = None
        outcomes = []
        # Keep the actual defense-16 Mage kit. Seed 0 is a miss control; 749
        # supplies the deterministic double-check witness without altered stats.
        for seed in (0, 749):
            self.save = self.directory / f"chill-{seed}.db"
            server = self.server("--checkpoint-interval", 1, scenario=package, character=2, seed=seed)
            player, initial = self.client(actor=2)
            ice, ice_initial = self.client(actor=3)
            self.assertFalse(initial["state"]["wizard_game"])
            started = self.act(player, {"type": "use_ability", "id": "bind_wounds",
                "invocation": {"aim": {"kind": "actor_target", "target": view(initial)["self_target"]}, "weapon": {"kind": "default"}},
                "supplies": {"kind": "carried"}})
            self.assertIsNone(started["error"])
            original = checkpoint(self, player)
            target = next(row["id"] for row in view(ice_initial)["visible_actors"] if row["name"] == "Mage")
            cast = self.act(ice, {"type": "use_ability", "id": CHILL,
                "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
            self.assertIsNone(cast["error"])
            for _ in range(12):
                current = self.request(player, {"type": "snapshot"})
                if phase(current) == "active" or not current["intentions"]:
                    break
                self.assertIsNone(self.act(ice, {"type": "wait"})["error"])
            frozen = phase(current) == "active"
            outcomes.append({"seed": seed, "frozen_care": frozen})
            if frozen:
                witness = (server, player, ice, initial, current, original)
                break
            ice.stop(); player.stop(); server.stop()
        self.assertIsNotNone(witness, f"No actual-kit freeze witness in bounded seeds: {outcomes}")
        self.assertEqual(outcomes, [{"seed": 0, "frozen_care": False},
                                    {"seed": 749, "frozen_care": True}])
        server, player, ice, initial, paused, original = witness
        saved = checkpoint(self, player)
        self.assertTrue(any(row["phase"] == "paused" for row in paused["intentions"]))
        self.assertEqual(u64(view(paused)["combat"]["hp"]), u64(view(initial)["combat"]["hp"]))
        pending = saved["actors"]["2"]["pending"]
        for field in ("plan", "hold", "item_payment", "progress"):
            self.assertEqual(pending[field], original["actors"]["2"]["pending"][field])
        self.assertEqual(saved["items"], original["items"])
        here = underfoot(paused["state"])["key"]
        denied = self.command(player, {"type": "cancel_intention", "step": "west"})
        self.assertIsNotNone(denied["error"])
        self.assertEqual(underfoot(denied["state"])["key"], here)
        self.assertEqual(denied["intentions"], paused["intentions"])
        text = self.launch("tor-client-text", ["--connect", self.address, "--actor", "2"],
                           cls=AdventureProcess, token=SPECTATOR_TOKEN)
        welcome = text.until(lambda line: line == "> ")
        self.assertIn("ice monster", welcome)
        self.assertIn("Frozen", self.say(text, "stats"))
        text.stop()
        self.flush_save(actor=2)
        ice.stop(); player.stop(); server.stop()
        shutil.rmtree(package)
        server = self.server("--checkpoint-interval", 1, scenario=unrelated_fallback(self),
                             seed=None, save_actor=2)
        player, restored = self.client(actor=2)
        ice, _ = self.client(actor=3)
        self.assertEqual(checkpoint(self, player), saved)
        self.assertFalse(view(restored)["combat"]["preparation_active"])
        for _ in range(12):
            thawed = self.request(player, {"type": "snapshot"})
            if phase(thawed) == "protection":
                break
            self.assertIsNone(self.act(ice, {"type": "wait"})["error"])
        else:
            self.fail("CH1 failed to reach its shared protection phase")
        self.assertFalse(view(thawed)["combat"]["preparation_active"], "Thaw must not auto-resume human care")
        self.assertEqual(checkpoint(self, player)["actors"]["2"]["pending"], pending)
        self.assertLess(view(thawed)["combat"]["own_stats"]["nutrition"]["reserve"],
                        view(initial)["combat"]["own_stats"]["nutrition"]["reserve"])
        complete = self.finish_resumed_care(player, ice, thawed["intentions"][0]["intention"])
        bandage = next(row for row in view(complete)["inventory"] if row["name"] == "clean bandage")
        self.assertEqual(int(bandage["quantity"]), 2)
        ice.stop(); player.stop(); server.stop()


class ChillResumePeerTests(unittest.TestCase):
    @staticmethod
    def value(*, active=False, complete=False, identity="care-original", error=None):
        return {"error": error, "state": {"observation": {"ready": complete,
                "combat": {"preparation_active": active, "terminal": False}}},
                "intentions": [] if complete else [{"intention": identity,
                    "phase": "started" if active else "queued"}],
                "readiness": {"admission": True, "revision": "4" if active else "3"}}

    def harness(self, *, active=False, resumed=None, peer_error=None):
        owner = RogueChillProcesses(
            "test_actual_checked_chill_preserves_care_holds_through_restart_and_explicit_thaw_resume")
        state = {"active": active, "fresh": active, "done": False}
        calls = []
        owner.command = Mock(return_value=self.value(active=active))
        def frame(client, predicate, seconds=15):
            self.assertEqual((client, seconds), ("player", 15))
            for current in [self.value(), resumed or self.value(active=True)]:
                if predicate(current):
                    state["active"] = True
                    calls.append("started")
                    return current
            raise RuntimeError("Resumed care never reached its original active lifecycle")
        def request(client, request):
            self.assertEqual(request, {"type": "snapshot"})
            calls.append(client + " snapshot")
            if client == "ice":
                self.assertTrue(state["active"], "peer refresh raced resumed care")
                state["fresh"] = True
                return {"error": peer_error, "state": {"observation": {"ready": True}},
                        "readiness": {"admission": True, "revision": "73"}}
            return self.value(active=state["active"], complete=state["done"])
        def act(client, action):
            self.assertEqual((client, action), ("ice", {"type": "wait"}))
            self.assertTrue(state["active"], "peer Wait raced resumed care start")
            self.assertTrue(state["fresh"], "peer Wait reused a stale observation")
            state["done"] = True
            calls.append("ice Wait")
            return {"error": None}
        owner.frame = Mock(side_effect=frame)
        owner.request = Mock(side_effect=request)
        owner.act = Mock(side_effect=act)
        return owner, calls

    def test_already_active_positive_keeps_single_resume_and_peer_turn(self):
        owner, _ = self.harness(active=True)
        complete = owner.finish_resumed_care("player", "ice", "care-original")
        self.assertFalse(complete["intentions"])
        owner.command.assert_called_once_with("player", {"type": "resume_intention"})
        owner.act.assert_called_once_with("ice", {"type": "wait"})
        owner.frame.assert_not_called()

    def test_delayed_start_then_fresh_peer_context_precedes_exactly_one_turn(self):
        owner, calls = self.harness()
        owner.finish_resumed_care("player", "ice", "care-original")
        self.assertEqual(calls, ["started", "player snapshot", "ice snapshot", "ice Wait", "player snapshot"])
        owner.command.assert_called_once()
        owner.act.assert_called_once()
        owner.frame.assert_called_once()

    def test_changed_identity_and_resume_error_refuse_peer_turn(self):
        for resumed in [self.value(active=True, identity="replacement"), self.value(active=True, error="rejected")]:
            with self.subTest(resumed=resumed):
                owner, _ = self.harness(resumed=resumed)
                with self.assertRaises(AssertionError):
                    owner.finish_resumed_care("player", "ice", "care-original")
                owner.frame.assert_called_once()
                owner.act.assert_not_called()

    def test_peer_snapshot_rejection_cannot_submit_wait(self):
        owner, calls = self.harness(active=True, peer_error="snapshot rejected")
        with self.assertRaises(AssertionError):
            owner.finish_resumed_care("player", "ice", "care-original")
        self.assertIn("ice snapshot", calls)
        owner.act.assert_not_called()


if __name__ == "__main__":
    unittest.main()
