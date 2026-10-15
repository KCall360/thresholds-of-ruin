"""Real selected-character entry and source-free receipts for unselected candidates.

Later-floor starts are bounded receipt diagnostics with the actual HD-one kit.
They do not stand in for earned descent, dangerous encounters or escape evidence.
"""
import shutil
import sqlite3
import unittest

from process_harness import ProcessTestCase, inspect_save
from rogue_activation_candidate import conditional_bands, create_candidate


class RogueActivationCandidateProcesses(ProcessTestCase):
    def candidate_restart(self, selected, depth, checkpoint_interval):
        output = self.directory / f"candidate-{selected}-{depth}-{checkpoint_interval}"
        inventory = create_candidate(output, self.bin / ("tor-scenario" + self.suffix), start_depth=depth)
        self.assertEqual(inventory["production_activation"], "not_selected")
        self.save = self.directory / f"candidate-{selected}-{depth}-{checkpoint_interval}.db"
        server = self.server("--checkpoint-interval", checkpoint_interval, scenario=output / "package",
                             character=selected, seed=42)
        player, initial = self.client(actor=selected)
        view = initial["state"]["observation"]
        self.assertFalse(initial["state"]["wizard_game"])
        self.assertEqual({row["name"]: int(row["quantity"]) for row in view["inventory"]},
                         {"mace": 1, "ring mail": 1, "clean bandage": 3, "food ration": 1})
        receipts = self.private_receipts(actor=selected)
        initial_save = inspect_save(self.save)
        self.assertEqual(initial_save["scenario"]["package"]["selected"], selected)
        self.assertEqual(len(receipts), 1)
        receipt = receipts[0]
        self.assertEqual(receipt["scope"], f"dungeon/{depth}")
        self.assertEqual(receipt["entrant"], selected)
        self.assertEqual(len(receipt["decisions"]), 3)
        foe_result = receipt["decisions"][0]["result"]
        self.assertEqual(foe_result["kind"], "evaluated")
        self.assertEqual(foe_result["requested"], 4)
        self.assertEqual(len(foe_result["attempts"]), 4)
        eligible = {choice["archetype"] for band in conditional_bands()
                    if band["depth"]["minimum"] <= depth <= band["depth"]["maximum"]
                    for choice in band["choices"]}
        placed = [row for row in foe_result["attempts"] if row["kind"] == "placed"]
        self.assertTrue(placed, "The diagnostic must actually activate ordinary creature content")
        for row in placed:
            self.assertIn(row["content"], eligible)
            self.assertEqual(row["entity"]["kind"], "actor")
        # Repeated observations and save barriers are not fresh entry or wake.
        for _ in range(3):
            observed = self.request(player, {"type": "snapshot"})
            self.assertEqual(observed["state"], initial["state"])
        self.assertEqual(self.private_receipts(actor=selected), receipts)
        # One ordinary action creates a real journal/checkpoint tail. Births are
        # outside initial sight, so this is a receipt test, not an all-hit duel.
        waited = self.act(player, {"type": "wait"})
        self.assertIsNone(waited["error"])
        self.assertEqual(self.private_receipts(actor=selected), receipts)
        before = self.request(player, {"type": "snapshot"})
        player.stop()
        server.stop()
        with sqlite3.connect(self.save) as database:
            checkpoint = database.execute("SELECT sequence FROM checkpoint").fetchone()
            self.assertEqual(checkpoint is not None, checkpoint_interval == 1)
        shutil.rmtree(output)
        server = self.server("--checkpoint-interval", checkpoint_interval, "--regions", 2,
                             seed=None, save_actor=selected)
        player, restored = self.client(actor=selected)
        self.assertEqual(restored["state"], before["state"])
        self.assertEqual(restored["history"], before["history"])
        self.assertEqual(self.private_receipts(actor=selected), receipts)
        self.assertEqual(inspect_save(self.save)["scenario"]["package"]["selected"], selected)
        player.stop()
        server.stop()

    def test_warrior_candidate_entry_keeps_actual_kit_and_journal_receipt_after_source_deletion(self):
        self.candidate_restart(1, 1, 0)

    def test_mage_candidate_entry_keeps_actual_kit_and_checkpoint_receipt_after_source_deletion(self):
        self.candidate_restart(2, 1, 1)

    def test_ice_quagga_candidate_band_survives_source_free_checkpoint_restart(self):
        self.candidate_restart(1, 11, 1)

    def test_quagga_unicorn_candidate_boundary_survives_source_free_journal_restart(self):
        self.candidate_restart(2, 17, 0)

    def test_depth_twenty_six_candidate_retains_unicorn_receipts_without_granting_earned_hd(self):
        self.candidate_restart(1, 26, 1)


if __name__ == "__main__":
    unittest.main()
