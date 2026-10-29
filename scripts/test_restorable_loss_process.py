"""Ordinary restorable losses through real ability, item, save and client paths."""
from scalar_wire import u64
import shutil
import unittest
from unittest.mock import Mock

from process_harness import ProcessTestCase, SPECTATOR_TOKEN
from restorable_loss_package import DRAIN_HD, WOUND, ZERO_WORD_SEED, prepare
from restorable_loss_harness import LossHelpers, losses, self_ability
from status_condition_package import actor_creature, checkpoint, combat, observation


class RestorableLossProcesses(LossHelpers, ProcessTestCase):
    def test_ordinary_loss_and_restoration_preserve_injury_after_source_free_restart(self):
        package = prepare(self)
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=ZERO_WORD_SEED)
        player, initial = self.client()
        changed = self.apply_losses(player)
        saved = checkpoint(self, player)
        self.assertEqual(saved["rules_rng"], 0, "the single frozen 1d3 operand draws exactly once")
        self.assertEqual(actor_creature(saved)["restorable_losses"], {
            key: value for key, value in losses(changed).items() if key != "hit_dice"})
        self.assertEqual(u64(losses(changed)["hit_dice"]), 0)
        spectator, shown = self.client(SPECTATOR_TOKEN)
        self.assertEqual(losses(shown), losses(changed))
        self.assertFalse(shown["has_control"])
        for _ in range(3):
            self.assertDisclosedStateEqual(self.snapshot(player)["state"], changed["state"])
            self.assertIsNone(self.command(player, {"type": "inspect"})["error"])
        self.assertEqual(checkpoint(self, player), saved)
        player.stop(); spectator.stop(); server.stop()
        for path in package.iterdir():
            if path.name != ".compiled":
                shutil.rmtree(path) if path.is_dir() else path.unlink()
        self.server("--checkpoint-interval", 1, seed=None)
        player, restored = self.client()
        self.assertDisclosedStateEqual(restored["state"], changed["state"])
        self.assertEqual(checkpoint(self, player), saved)
        item = self.restoration_item(restored)
        self.assertIsNone(self.act(player, {"type": "drink", "item": item})["error"])
        final = self.finish(player)
        self.assertEqual(losses(final)["maximum_health"], 0)
        self.assertTrue(all(value == 0 for value in losses(final)["attributes"].values()))
        self.assertEqual(u64(combat(final)["max_hp"]), u64(combat(initial)["max_hp"]))
        self.assertEqual(u64(combat(final)["max_hp"]) - u64(combat(final)["hp"]), 2)
        self.assertEqual(next(row for row in observation(final)["inventory"] if row["id"] == item)["quantity"], "1")
        completed = checkpoint(self, player)
        self.assertEqual(completed["rules_rng"], saved["rules_rng"])
        self.assertEqual(actor_creature(completed)["injury"], 2)

    def test_pending_restoration_consumable_keeps_frozen_effects_and_one_hold_across_restart(self):
        package = prepare(self, peer=True, consume_turns=3)
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=ZERO_WORD_SEED)
        player, _ = self.client()
        peer, _ = self.client(actor=2)
        changed = self.apply_losses(player, peer)
        item = self.restoration_item(changed)
        started = self.act(player, {"type": "drink", "item": item})
        self.assertTrue(combat(started)["preparation_active"])
        self.assertIsNone(self.request(player, {"type": "release_control"})["error"])
        paused = self.snapshot(player)
        if not any(row["phase"] == "paused" for row in paused["intentions"]):
            paused = self.frame(player, lambda frame: any(row["phase"] == "paused" for row in frame["intentions"]))
        saved = checkpoint(self, player)
        pending = saved["actors"]["1"]["pending"]
        self.assertEqual(pending["plan"]["effects"], [
            {"kind": "restore_attribute", "attribute": "strength", "amount": {"kind": "fixed", "amount": 1}},
            {"kind": "restore_maximum_health", "amount": {"kind": "fixed", "amount": 100}},
        ])
        self.assertEqual(saved["items"]["10"]["quantity"], 2)
        self.assertEqual(len(saved["items"]["10"]["holds"]), 1)
        player.stop(); peer.stop(); server.stop()
        self.server("--checkpoint-interval", 1, seed=None)
        player, restored = self.client()
        peer, _ = self.client(actor=2)
        self.assertEqual(checkpoint(self, player), saved)
        self.assertEqual(restored["intentions"][0]["intention"], paused["intentions"][0]["intention"])
        self.assertIsNone(self.command(player, {"type": "resume_intention"})["error"])
        final = self.finish(player, peer)
        self.assertEqual(losses(final)["maximum_health"], 0)
        self.assertEqual(losses(final)["attributes"]["strength"], 0)
        completed = checkpoint(self, player)
        self.assertEqual(completed["items"]["10"]["quantity"], 1)
        self.assertEqual(completed["items"]["10"]["holds"], [])
        self.assertEqual(actor_creature(completed)["injury"], 2)
        self.assertEqual(completed["rules_rng"], saved["rules_rng"])

    def test_text_shows_restorable_capacity_and_ordinary_drinking_removes_the_summary(self):
        package = prepare(self)
        self.server(scenario=package, seed=ZERO_WORD_SEED)
        player, _ = self.client()
        self.apply_losses(player)
        player.stop()
        text, _ = self.text_client()
        self.assertIn("Restorable losses: Strength 1, maximum Health 1", text.command("stats"))
        self.assertIn("ItemStarted", text.command("drink fixture restoration"))
        self.assertNotIn("Restorable losses:", text.command("stats"))


class RestorableHitDieProcesses(LossHelpers, ProcessTestCase):
    def test_latest_ordinal_and_owned_purchase_restore_after_source_free_restart(self):
        package = prepare(self, hit_dice=True)
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=ZERO_WORD_SEED)
        player, initial = self.client()
        self.assertIsNone(self.act(player, self_ability(initial, WOUND))["error"])
        wounded = self.finish(player)
        original = actor_creature(checkpoint(self, player))
        self.assertIsNone(self.act(player, self_ability(wounded, DRAIN_HD))["error"])
        drained = self.finish(player)
        self.assertEqual(u64(combat(drained)["own_stats"]["hit_dice"]["total"]), 2)
        self.assertEqual(u64(losses(drained)["hit_dice"]), 1)
        saved = checkpoint(self, player)
        self.assertEqual(actor_creature(saved)["earned_progress"]["highest_applied_tier"], 3)
        self.assertEqual(actor_creature(saved)["injury"], 2)
        spectator, shown = self.client(SPECTATOR_TOKEN)
        self.assertEqual(losses(shown), losses(drained))
        spectator.stop(); player.stop(); server.stop()
        for path in package.iterdir():
            if path.name != ".compiled":
                shutil.rmtree(path) if path.is_dir() else path.unlink()
        self.server("--checkpoint-interval", 1, seed=None)
        player, reopened = self.client()
        self.assertEqual(checkpoint(self, player), saved)
        self.assertDisclosedStateEqual(reopened["state"], drained["state"])
        item = self.restoration_item(reopened)
        self.assertIsNone(self.act(player, {"type": "drink", "item": item})["error"])
        final = self.finish(player)
        restored = actor_creature(checkpoint(self, player))
        self.assertEqual(restored["build"], original["build"], "exact source, purchases and seeded Health ordinals")
        self.assertEqual(restored["earned_progress"], original["earned_progress"])
        self.assertEqual(restored["injury"], 2)
        self.assertEqual(u64(losses(final)["hit_dice"]), 0)
        self.assertEqual(u64(combat(final)["max_hp"]), u64(combat(initial)["max_hp"]))
        self.assertEqual(u64(combat(final)["max_hp"]) - u64(combat(final)["hp"]), 2)
        self.assertEqual(next(row for row in observation(final)["inventory"] if row["id"] == item)["quantity"], "1")

    def test_text_discloses_restorable_hit_die_and_restoration_removes_the_summary(self):
        package = prepare(self, hit_dice=True)
        self.server(scenario=package, seed=ZERO_WORD_SEED)
        player, initial = self.client()
        self.assertIsNone(self.act(player, self_ability(initial, DRAIN_HD))["error"])
        self.finish(player)
        player.stop()
        text, _ = self.text_client()
        self.assertIn("Restorable losses: Hit Dice 1", text.command("stats"))
        self.assertIn("ItemStarted", text.command("drink fixture restoration"))
        self.assertNotIn("Restorable losses:", text.command("stats"))


class RestorableLossAsciiProcesses(LossHelpers, ProcessTestCase):
    graphical = True

    def test_native_stats_and_drink_present_restorable_hit_die(self):
        package = prepare(self, hit_dice=True)
        self.server(scenario=package, seed=ZERO_WORD_SEED)
        player, initial = self.client()
        self.assertIsNone(self.act(player, self_ability(initial, DRAIN_HD))["error"])
        self.finish(player)
        player.stop()
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        self.ascii_frame(window, lambda frame: frame["has_control"] and not frame["busy"])
        native = self.native_keys(window)
        native("Shift_L", True)
        try:
            shown = self.native_press(window, native, "2", "stats")
        finally:
            native("Shift_L", False)
        self.assertIn("Restorable losses: Hit Dice 1", "\n".join(shown["stats_rows"]))
        self.native_press(window, native, "Escape", "escape")
        self.native_press(window, native, "q", "drink")
        consumed = self.native_press(window, native, "Return", "enter")
        def restored(frame):
            return frame["state"] is not None and u64(losses(frame)["hit_dice"]) == 0
        final = consumed if restored(consumed) else self.ascii_frame(window, restored)
        self.assertEqual(u64(combat(final)["own_stats"]["hit_dice"]["total"]), 3)
        self.assertTrue(final["window_open"])

    def test_native_stats_and_drink_present_the_same_restorable_deficit(self):
        package = prepare(self)
        self.server(scenario=package, seed=ZERO_WORD_SEED)
        player, _ = self.client()
        self.apply_losses(player)
        player.stop()
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        self.ascii_frame(window, lambda frame: frame["has_control"] and not frame["busy"])
        native = self.native_keys(window)
        native("Shift_L", True)
        try:
            shown = self.native_press(window, native, "2", "stats")
        finally:
            native("Shift_L", False)
        self.assertIn("Restorable losses: Strength 1, maximum Health 1", "\n".join(shown["stats_rows"]))
        self.native_press(window, native, "Escape", "escape")
        self.native_press(window, native, "q", "drink")
        consumed = self.native_press(window, native, "Return", "enter")
        def restored(frame):
            return frame["state"] is not None and losses(frame)["attributes"]["strength"] == 0
        final = consumed if restored(consumed) else self.ascii_frame(window, restored)
        self.assertEqual(losses(final)["maximum_health"], 0)
        self.assertEqual(u64(combat(final)["max_hp"]) - u64(combat(final)["hp"]), 2)
        self.assertTrue(final["window_open"])


class LossFinishPeerTests(unittest.TestCase):
    @staticmethod
    def value(*, active=False, done=False, recovery=False, identity="original", error=None):
        return {"error": error, "state": {"observation": {"ready": done,
                "combat": {"preparation_remaining": None if done or recovery else "100",
                           "preparation_active": active, "terminal": False,
                           "recovery_remaining": "100" if recovery else "0"}}},
                "intentions": [] if done or recovery else [{"intention": identity,
                    "phase": "started" if active else "queued"}],
                "readiness": {"admission": True, "revision": "4" if active else "3"}}

    def harness(self, *, active=False, done=False, recovery=False, started=None, peer_error=None):
        owner = RestorableLossProcesses("test_pending_restoration_consumable_keeps_frozen_effects_and_one_hold_across_restart")
        state = {"active": active, "fresh": False, "done": done}
        calls = []
        def snapshot(client):
            calls.append(client + " snapshot")
            if client == "peer":
                self.assertTrue(state["active"] or recovery, "peer refreshed before original work started")
                state["fresh"] = True
                return {"error": peer_error, "state": {"observation": {"ready": True}},
                        "readiness": {"admission": True, "revision": "73"}}
            return self.value(active=state["active"], done=state["done"], recovery=recovery and not state["done"])
        def request(client, action):
            self.assertEqual(action, {"type": "snapshot"})
            return snapshot(client)
        def frame(client, predicate, seconds=15):
            self.assertEqual((client, seconds), ("player", 15))
            for current in [self.value(), started or self.value(active=True)]:
                if predicate(current):
                    state["active"] = True
                    calls.append("started")
                    return current
            raise RuntimeError("original pending work never started")
        def act(client, action):
            self.assertEqual((client, action), ("peer", {"type": "wait"}))
            self.assertTrue(state["active"] or recovery, "peer Wait raced owner start")
            self.assertTrue(state["fresh"], "peer Wait reused stale context")
            state["done"] = True
            calls.append("peer Wait")
            return {"error": None}
        owner.snapshot = Mock(side_effect=snapshot)
        owner.request = Mock(side_effect=request)
        owner.frame = Mock(side_effect=frame)
        owner.act = Mock(side_effect=act)
        return owner, calls

    def test_already_complete_positive_needs_no_peer_turn(self):
        owner, calls = self.harness(done=True)
        owner.finish("player", "peer")
        self.assertEqual(calls, ["player snapshot"])
        owner.act.assert_not_called()
        owner.frame.assert_not_called()

    def test_queued_resume_waits_original_start_then_fresh_peer_and_one_turn(self):
        owner, calls = self.harness()
        owner.finish("player", "peer")
        self.assertEqual(calls, ["player snapshot", "started", "peer snapshot", "peer Wait", "player snapshot"])
        owner.frame.assert_called_once()
        owner.act.assert_called_once_with("peer", {"type": "wait"})

    def test_replacement_or_rejected_start_refuses_peer_turn(self):
        for started in [self.value(active=True, identity="replacement"), self.value(active=True, error="rejected")]:
            with self.subTest(started=started):
                owner, _ = self.harness(started=started)
                with self.assertRaises(AssertionError):
                    owner.finish("player", "peer")
                owner.frame.assert_called_once()
                owner.act.assert_not_called()

    def test_peer_snapshot_error_refuses_turn(self):
        owner, calls = self.harness(active=True, peer_error="snapshot rejected")
        with self.assertRaises(AssertionError):
            owner.finish("player", "peer")
        self.assertEqual(calls, ["player snapshot", "peer snapshot"])
        owner.act.assert_not_called()

    def test_recovery_only_progress_does_not_demand_active_preparation(self):
        owner, calls = self.harness(recovery=True)
        owner.finish("player", "peer")
        self.assertEqual(calls, ["player snapshot", "peer snapshot", "peer Wait", "player snapshot"])
        owner.frame.assert_not_called()
        owner.act.assert_called_once()


if __name__ == "__main__":
    unittest.main()
