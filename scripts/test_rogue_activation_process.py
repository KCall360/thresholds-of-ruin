"""Real selected-character entry and source-free receipts for unselected candidates.

Later-floor starts are bounded receipt diagnostics with the actual HD-one kit.
They do not stand in for earned descent, dangerous encounters or escape evidence.
"""
import json
import shutil
import sqlite3
import unittest

from checkpoint_payload import crc32c, decode_checkpoint_payload
from process_harness import ProcessTestCase, inspect_save
from process_work_budget import ProcessWorkBudget
from rogue_activation_candidate import conditional_bands, create_candidate
from rogue_roster_package import encounter_sources
from status_condition_package import actor_creature, checkpoint


class RogueActivationCandidateProcesses(ProcessTestCase):
    def setUp(self):
        super().setUp()
        self.work_budget = ProcessWorkBudget(requests=512, actor_ticks=3000,
                                             transcript_bytes=32 * 1024 * 1024)

    def tearDown(self):
        report = self.work_budget.report()
        (self.directory / "work-budget.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("Process budget " + self.id() + ": " + json.dumps(report, sort_keys=True), flush=True)
        if not self._failed():
            self.work_budget.check()
        super().tearDown()

    def client(self, *args, **kwargs):
        kwargs["omit_memory"] = True  # Assertions use current disclosure and saved authority.
        client, frame = super().client(*args, **kwargs)
        self.work_budget.track(client, frame)
        return client, frame

    def command(self, client, value):
        self.work_budget.before()
        result = super().command(client, value)
        self.work_budget.after(client, result)
        return result

    def frame(self, client, predicate, seconds=15):
        return self.work_budget.frame_wait(super().frame, client, predicate, seconds=seconds)

    def flush_save(self, *, actor=None):
        # private_receipts uses this separate synchronous save connection.
        self.work_budget.before()
        super().flush_save(actor=actor)
        self.work_budget.check()

    def complete_checkpoint(self, player):
        """Include referenced dormant records, never stale region history."""
        saved = checkpoint(self, player)
        with sqlite3.connect(self.save) as database:
            payload = database.execute("SELECT payload FROM checkpoint").fetchone()[0]
            document = json.loads(decode_checkpoint_payload(payload))
            game, shared = document["game"], document["shared"]
            lifecycle = shared["lifecycles"][game["lifecycle"]]
            for record in lifecycle["detached"].values():
                frame = database.execute("SELECT frame FROM regions WHERE record=?", (record,)).fetchone()[0]
                self.assertEqual(frame[:4], b"TORR")
                self.assertEqual(len(frame) - 24, int.from_bytes(frame[16:20], "little"))
                self.assertEqual(crc32c(frame[4:20] + frame[24:]), int.from_bytes(frame[20:24], "little"))
                detached = json.loads(frame[24:])
                for key in ("actors", "items"):
                    self.assertFalse(set(saved[key]) & set(detached[key]), "Owned identity cannot be duplicated")
                    saved[key].update(detached[key])
                saved["frozen_at"].update(detached["stamps"])
        return saved

    def candidate_restart(self, selected, depth, checkpoint_interval, *, seed=42):
        output = self.directory / f"candidate-{selected}-{depth}-{checkpoint_interval}-{seed}"
        inventory = create_candidate(output, self.bin / ("tor-scenario" + self.suffix), start_depth=depth)
        self.assertEqual(inventory["production_activation"], "not_selected")
        self.save = self.directory / f"candidate-{selected}-{depth}-{checkpoint_interval}-{seed}.db"
        server = self.server("--checkpoint-interval", checkpoint_interval, scenario=output / "package",
                             character=selected, seed=seed)
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
                    if band["depth"]["minimum"] <= depth
                    and (band["depth"]["kind"] == "open" or depth <= band["depth"]["maximum"])
                    for choice in band["choices"]}
        placed = [row for row in foe_result["attempts"] if row["kind"] == "placed"]
        self.assertTrue(placed, "The diagnostic must actually activate ordinary creature content")
        for row in placed:
            self.assertIn(row["content"], eligible)
            self.assertEqual(row["entity"]["kind"], "actor")
            stock = row["entity"]["carried_loot"]
            if row["content"] == "rogue.vampire":
                self.assertIn(stock["kind"], ("carried", "not_carried"))
                if stock["kind"] == "carried":
                    self.assertIn(stock["content"], ("rogue.ration", "rogue.clean_bandage"))
                    self.assertEqual(stock["quantity"], 1)
            else:
                self.assertIsNone(stock)
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
        saved = self.complete_checkpoint(player) if checkpoint_interval == 1 else None
        if saved is not None:
            source = encounter_sources()["actors"]
            for row in placed:
                recipe = source[row["content"]]["creature"]
                born = actor_creature(saved, row["entity"]["id"])
                expected = len(recipe["hit_dice"]) + max(0, depth - 26)
                self.assertEqual(len(born["build"]["hd"]), expected)
                self.assertEqual(born["build"]["species"], row["content"])
                self.assertEqual(len(born["build"]["restoration"]["entries"]), expected)
                stock = row["entity"]["carried_loot"]
                if stock is not None and stock["kind"] == "carried":
                    item = saved["items"][str(stock["item"])]
                    self.assertEqual(item["location"], {"Carried": row["entity"]["id"]})
                    self.assertEqual(item["quantity"], stock["quantity"])
            self.assertEqual(len(actor_creature(saved, selected)["build"]["hd"]), 1,
                             "Later-depth starts never grant the player progression")
        before = self.request(player, {"type": "snapshot"})
        player.stop()
        server.stop()
        with sqlite3.connect(self.save) as database:
            checkpoint_row = database.execute("SELECT sequence FROM checkpoint").fetchone()
            self.assertEqual(checkpoint_row is not None, checkpoint_interval == 1)
        shutil.rmtree(output)
        server = self.server("--checkpoint-interval", checkpoint_interval, "--regions", 2,
                             seed=None, save_actor=selected)
        player, restored = self.client(actor=selected)
        self.assertEqual(restored["state"], before["state"])
        self.assertEqual(restored["history"], before["history"])
        self.assertEqual(self.private_receipts(actor=selected), receipts)
        self.assertEqual(inspect_save(self.save)["scenario"]["package"]["selected"], selected)
        if saved is not None:
            self.assertEqual(self.complete_checkpoint(player), saved,
                             "Source-free replay retains complete owned rows, stock and Health state")
        player.stop()
        server.stop()
        return receipts

    def test_warrior_candidate_entry_keeps_actual_kit_and_journal_receipt_after_source_deletion(self):
        self.candidate_restart(1, 1, 0)

    def test_mage_candidate_entry_keeps_actual_kit_and_checkpoint_receipt_after_source_deletion(self):
        self.candidate_restart(2, 1, 1)

    def test_ice_quagga_candidate_band_survives_source_free_checkpoint_restart(self):
        self.candidate_restart(1, 11, 1)

    def test_quagga_unicorn_candidate_boundary_survives_source_free_journal_restart(self):
        self.candidate_restart(2, 17, 0)

    def test_depth_twenty_six_candidate_retains_unicorn_vampire_receipts_without_granting_earned_hd(self):
        self.candidate_restart(1, 26, 1)

    def test_depth_twenty_seven_open_tail_retains_nine_vampire_rows_after_source_deletion(self):
        self.candidate_restart(2, 27, 1)

    def test_depth_274_open_tail_retains_256_owned_vampire_rows_after_source_deletion(self):
        self.candidate_restart(1, 274, 1)

    def test_open_tail_birth_stock_success_and_miss_remain_pinned_after_source_deletion(self):
        seen = set()
        samples = []
        # A bounded deterministic mechanism witness, not a frequency estimate.
        for seed in range(8):
            receipts = self.candidate_restart(2, 27, 1, seed=seed)
            decisions = [row["entity"]["carried_loot"]["kind"]
                         for row in receipts[0]["decisions"][0]["result"]["attempts"]
                         if row["kind"] == "placed"]
            seen.update(decisions)
            samples.append((seed, decisions))
            if seen == {"carried", "not_carried"}:
                break
        print("Vampire generated birth-stock samples: " + json.dumps(samples), flush=True)
        self.assertEqual(seen, {"carried", "not_carried"}, samples)


if __name__ == "__main__":
    unittest.main()
