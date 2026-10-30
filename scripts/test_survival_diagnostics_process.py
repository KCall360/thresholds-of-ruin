"""Opt-in actual-publication telemetry, ordinary actions and source-free restart."""
import json
import shutil
import sqlite3
import time
import unittest

from checkpoint_payload import decode_checkpoint_payload
from nutrition_package import ROLLED_MEAL, ZERO_WORD_SEED, prepare, prepare_regeneration
from survival_diagnostics import read_log
from process_harness import ProcessTestCase, inspect_save


class SurvivalDiagnosticsProcesses(ProcessTestCase):
    def read_rows(self, path, predicate):
        deadline = time.monotonic() + 5
        while True:
            raw = path.read_text(encoding="utf-8") if path.exists() else ""
            complete = raw[:raw.rfind("\n") + 1]
            rows = [json.loads(line) for line in complete.splitlines()]
            if predicate(rows) or time.monotonic() >= deadline:
                return rows
            time.sleep(0.01)

    def barrier(self, player, path):
        before = len(self.read_rows(path, lambda _: True))
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        rows = self.read_rows(path, lambda rows: any(row.get("event") == "survival.barrier" for row in rows[before:]))
        self.assertTrue(any(row.get("event") == "survival.barrier" for row in rows[before:]))
        for row in rows:
            if "survival_version" in row:
                self.assertEqual(row["diagnostic_dropped"], 0)
                self.assertFalse(row["diagnostic_incomplete"])
        return rows

    def saved_checkpoint(self, player):
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as database:
            sequence, payload = database.execute("SELECT sequence, payload FROM checkpoint").fetchone()
        checkpoint = json.loads(decode_checkpoint_payload(payload))
        self.assertEqual(checkpoint["sequence"], sequence)
        return checkpoint

    def independent_anchor(self):
        with sqlite3.connect(self.save) as database:
            sequence, payload = database.execute("SELECT sequence, payload FROM checkpoint").fetchone()
            latest = database.execute("SELECT sequence, frame FROM journal WHERE sequence>0 "
                "UNION ALL SELECT sequence, frame FROM history WHERE sequence>0 "
                "ORDER BY sequence DESC LIMIT 1").fetchone()
        checkpoint = json.loads(decode_checkpoint_payload(payload))
        self.assertEqual(sequence, checkpoint["record_count"])
        self.assertEqual(latest[0] if latest else 0, sequence)
        entry = json.loads(latest[1][24:])["record"]["entry"] if latest else None
        return {"lineage": inspect_save(self.save)["branch"],
                "branch": checkpoint["current_branch"], "history_sequence": sequence,
                "entry_id": entry["id"] if entry else None, "tick": checkpoint["game"]["tick"]}

    def test_capped_rolled_food_observations_preserve_rng_state_and_public_results(self):
        package = prepare(self, initial=8, maximum=12, unit_ticks=100000,
                          meal_amount=ROLLED_MEAL)
        outcomes = []
        for mode in ("disabled", "enabled", "unwritable"):
            with self.subTest(mode=mode):
                self.save = self.directory / (mode + ".db")
                log = self.directory / (mode + ".jsonl")
                env = {} if mode == "disabled" else {"TOR_SURVIVAL_DIAGNOSTICS_LOG":
                    str(log if mode == "enabled" else self.directory)}
                server = self.server("--checkpoint-interval", 1, scenario=package,
                                     seed=ZERO_WORD_SEED, extra_env=env,
                                     separate_stderr=mode == "unwritable")
                player, initial = self.client()
                meal = next(item for item in initial["state"]["observation"]["inventory"]
                            if item["name"] == "fixture meal")
                result = self.act(player, {"type": "eat", "item": meal["id"]})
                self.assertIsNone(result["error"])
                nutrition = result["state"]["observation"]["combat"]["own_stats"]["nutrition"]
                self.assertEqual(nutrition["reserve"], 12)
                checkpoint = self.saved_checkpoint(player)
                self.assertEqual(checkpoint["game"]["rules_rng"], 0,
                                 "the known first food operand is rolled exactly once")
                outcomes.append((checkpoint["game"], checkpoint["shared"]))
                if mode == "enabled":
                    self.barrier(player, log)
                    measured = read_log(log, expected_final_anchor=self.independent_anchor())
                    flow = measured["actors"]["1"]
                    self.assertEqual(flow["nourishment_offered"], 1100)
                    self.assertEqual(flow["nourishment_gain"], 4)
                    self.assertEqual(flow["nourishment_wasted"], 1096)
                    self.assertEqual(flow["nourishment_debt_cleared"], 0)
                    self.assertEqual(flow["food_consumed"], 1)
                    self.assertEqual(flow["items_consumed"], 1)
                    self.assertEqual(flow["consumed_items"], {"10": 1})
                    self.assertEqual(measured["history_sequence"], checkpoint["record_count"])
                else:
                    self.assertFalse(log.exists())
                player.stop(); server.stop(save=False)
        self.assertEqual(len(outcomes), 3)
        self.assertEqual(outcomes[0], outcomes[1])
        self.assertEqual(outcomes[0], outcomes[2])

    def test_food_clears_existing_debt_without_inventing_negative_time_refunds(self):
        package = prepare(self, initial=0, maximum=12, unit_ticks=100,
                          meal_amount=ROLLED_MEAL)
        log = self.directory / "debt.jsonl"
        self.server("--checkpoint-interval", 1, scenario=package, seed=ZERO_WORD_SEED,
                    extra_env={"TOR_SURVIVAL_DIAGNOSTICS_LOG": str(log)})
        player, initial = self.client()
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        meal = next(item for item in initial["state"]["observation"]["inventory"]
                    if item["name"] == "fixture meal")
        self.assertIsNone(self.act(player, {"type": "eat", "item": meal["id"]})["error"])
        self.barrier(player, log)
        flow = read_log(log, expected_final_anchor=self.independent_anchor())["actors"]["1"]
        self.assertEqual(flow["nutrition_spent"], 2)
        self.assertEqual(flow["nourishment_debt_cleared"], 2)
        self.assertEqual(flow["nourishment_gain"], 12)
        self.assertEqual(flow["nourishment_offered"], 1100)
        self.assertEqual(flow["nourishment_wasted"], 1088)

    def test_committed_gross_flows_and_source_deleted_restart_do_not_replay_totals(self):
        package = prepare_regeneration(self)
        fallback = prepare(self, name="unrelated-restart-fallback", initial=8)
        log = self.directory / "private-survival.jsonl"
        env = {"TOR_SURVIVAL_DIAGNOSTICS_LOG": str(log)}
        server = self.server("--checkpoint-interval", 1, scenario=package, extra_env=env)
        player, initial = self.client()
        rows = self.read_rows(log, lambda rows: len(rows) >= 1)
        self.assertTrue(rows, "explicit opt-in must emit a begin anchor")
        self.assertEqual(rows[0]["event"], "survival.begin")
        self.assertEqual(rows[0]["history_sequence"], 0)
        self.assertEqual(rows[0]["selected_actor"], 1)
        for pool in ("stamina", "focus", "mana"):
            paid = self.act(player, {"type": "use_ability", "id": "fixture_spend_" + pool,
                                    "invocation": {"aim": {"kind": "actor_target", "target": initial["state"]["observation"]["self_target"]}, "weapon": {"kind": "default"}}})
            self.assertIsNone(paid["error"])
            self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        rows = self.barrier(player, log)
        commits = [row for row in rows if row.get("event") == "survival.commit"]
        self.assertEqual([row["history_sequence"] for row in commits], list(range(1, len(commits) + 1)))
        flows = [row["flows"]["actors"].get("1", {}) for row in commits]
        for pool in ("stamina", "focus", "mana"):
            self.assertEqual(sum(flow.get("paid_start", {}).get(pool, 0) for flow in flows), 1)
            self.assertEqual(sum(flow.get("periodic_resources", {}).get(pool, 0) for flow in flows), 1)
            self.assertEqual(sum(flow.get("direct_resources", {}).get(pool, 0) for flow in flows), 0)
        self.assertEqual(sum(flow.get("nutrition_spent", 0) for flow in flows), 1)
        self.assertEqual(sum(flow.get("nutrition_regeneration_demand", 0) for flow in flows), 1)
        for _ in range(3):
            self.request(player, {"type": "snapshot"})
        failed = self.act(player, {"type": "use_ability", "id": "fixture_missing",
                                  "invocation": {"aim": {"kind": "actor_target", "target": initial["state"]["observation"]["self_target"]}, "weapon": {"kind": "default"}}})
        self.assertIsNotNone(failed["error"])
        after_queries = self.barrier(player, log)
        self.assertEqual([row for row in after_queries if row.get("event") == "survival.commit"], commits)
        expected = self.request(player, {"type": "snapshot"})["state"]
        with sqlite3.connect(self.save) as database:
            self.assertEqual(database.execute("SELECT region FROM region_sources ORDER BY region").fetchall(),
                             [(1,), (2,)])
        player.stop(); server.stop(save=False)
        boundary_rows = self.read_rows(log, lambda _: True)
        shutil.rmtree(package)
        self.assertFalse(package.exists())
        server = self.server("--checkpoint-interval", 1, seed=None, scenario=fallback, extra_env=env)
        player, reopened = self.client()
        self.assertDisclosedStateEqual(reopened["state"], expected)
        restarted = self.barrier(player, log)
        new_rows = restarted[len(boundary_rows):]
        self.assertEqual(new_rows[0]["event"], "survival.begin")
        self.assertEqual(new_rows[0]["lineage"], rows[0]["lineage"])
        self.assertEqual(new_rows[0]["history_sequence"], commits[-1]["history_sequence"])
        self.assertFalse(any(row.get("event") == "survival.commit" for row in new_rows))
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        final = self.barrier(player, log)
        measured = read_log(log, expected_final_anchor=self.independent_anchor())
        self.assertEqual(measured["segments"], 2)
        all_commits = [row for row in final if row.get("event") == "survival.commit"]
        self.assertEqual(len({(row["lineage"], row["entry_id"]) for row in all_commits}), len(all_commits))
        self.assertEqual(sum(row["flows"]["actors"].get("1", {}).get("nutrition_spent", 0) for row in all_commits), 1)


if __name__ == "__main__":
    unittest.main()
