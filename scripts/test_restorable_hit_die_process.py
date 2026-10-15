"""Latest actual HD and owned choices survive ordinary source-free restoration."""
import shutil
import unittest

from process_harness import ProcessTestCase
from restorable_hit_die_package import DRAIN, prepare
from restorable_loss_package import WOUND, ZERO_WORD_SEED
from status_condition_package import actor_creature, checkpoint, combat, observation
from restorable_loss_harness import LossHelpers, self_ability


class HitDieHelpers(LossHelpers):
    def drain(self, player):
        frame = self.snapshot(player)
        self.assertIsNone(self.act(player, self_ability(frame, DRAIN))["error"])
        changed = self.finish(player)
        self.assertEqual(combat(changed)["own_stats"]["restorable_losses"]["hit_dice"], 1)
        return changed


class RestorableHitDieProcesses(HitDieHelpers, ProcessTestCase):
    def test_ordinary_hd_restore_recovers_exact_ordinal_after_source_deleted_restart(self):
        package = prepare(self)
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=ZERO_WORD_SEED)
        player, initial = self.client()
        # A save flushes history; the first ordinary action creates a checkpoint.
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        original = actor_creature(checkpoint(self, player))["build"]
        self.assertIsNone(self.act(player, self_ability(initial, WOUND))["error"])
        self.finish(player)
        changed = self.drain(player)
        self.assertEqual(combat(changed)["max_hp"] - combat(changed)["hp"], 2)
        saved = checkpoint(self, player)
        self.assertEqual(actor_creature(saved)["earned_progress"]["highest_applied_tier"], 2)
        self.assertEqual(actor_creature(saved)["earned_progress"]["spent_xp"], 0)
        player.stop(); server.stop()
        for path in package.iterdir():
            if path.name != ".compiled":
                shutil.rmtree(path) if path.is_dir() else path.unlink()
        self.server("--checkpoint-interval", 1, seed=None)
        player, restored = self.client()
        self.assertEqual(restored["state"], changed["state"])
        self.assertEqual(checkpoint(self, player), saved)
        self.assertIsNone(self.act(player, {"type": "drink", "item": self.restoration_item(restored)})["error"])
        final = self.finish(player)
        completed = checkpoint(self, player)
        self.assertEqual(actor_creature(completed)["build"], original)
        self.assertEqual(actor_creature(completed)["injury"], 2)
        self.assertEqual(completed["rules_rng"], saved["rules_rng"])
        self.assertEqual(combat(final)["max_hp"], combat(initial)["max_hp"])
        self.assertEqual(combat(final)["own_stats"]["restorable_losses"]["hit_dice"], 0)

    def test_text_discloses_applied_hd_and_missing_source_without_consuming_restoration(self):
        self.server(scenario=prepare(self, choose_again=True), seed=ZERO_WORD_SEED)
        player, _ = self.client()
        changed = self.drain(player)
        item = self.restoration_item(changed)
        before = self.snapshot(player)
        rejected = self.act(player, {"type": "drink", "item": item})
        self.assertIsNotNone(rejected["error"])
        self.assertEqual(rejected["state"], before["state"])
        self.assertEqual(next(row["quantity"] for row in observation(rejected)["inventory"]
                              if row["id"] == item), "2")
        player.stop()
        text, _ = self.text_client()
        self.assertIn("Restorable losses: Hit Dice 1", text.command("stats"))
        overview = text.command("advancement")
        self.assertIn("highest applied HD 2", overview)
        self.assertIn("Select a growth/restoration source", overview)
        self.assertIn("Future growth: racial", text.command("advancement source racial"))
        self.assertNotIn("Server error", text.command("advancement confirm"))
        self.assertIn("ItemStarted", text.command("drink fixture restoration"))
        self.assertNotIn("Restorable losses:", text.command("stats"))


class RestorableHitDieNativeProcesses(HitDieHelpers, ProcessTestCase):
    graphical = True

    def test_native_stats_and_restoration_preserve_latest_hd_ownership(self):
        self.server(scenario=prepare(self), seed=ZERO_WORD_SEED)
        player, _ = self.client()
        self.drain(player)
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
            return (frame["state"] is not None
                    and combat(frame)["own_stats"]["restorable_losses"]["hit_dice"] == 0)
        final = consumed if restored(consumed) else self.ascii_frame(window, restored)
        self.assertTrue(final["window_open"])


if __name__ == "__main__":
    unittest.main()
