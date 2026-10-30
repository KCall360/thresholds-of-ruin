"""Run summaries, campaign aggregation and one bounded real-process descent."""
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest

import rogue_campaign as campaign


def witness(total, tick):
    return {"hit_dice": {"total": str(total)}, "tick": str(tick)}


def fake_controller():
    return SimpleNamespace(
        first_entries={1: {"tick": "0", "commands": 0}, 2: {"tick": "5000", "commands": 40}},
        growth_history=[witness(2, 0), witness(3, 4000)],
        floor_commands={1: 40, 2: 10}, commands=50, rest_episodes=2)


def sample(tick, phase="explore", hunger="nourished", hit_dice=3, depth=1):
    return {"tick": str(tick), "phase": phase, "depth": depth, "hp": 10, "max_hp": 20,
            "hit_dice": str(hit_dice), "earned_xp": "150", "hunger": hunger}


class RollingSpool(unittest.TestCase):
    def test_spool_restarts_only_when_fully_read_and_past_its_limit(self):
        with tempfile.TemporaryDirectory(dir=campaign.ROOT / "target") as directory:
            path = Path(directory) / "stdout.log"
            spool = campaign.RollingOutputQueue(path)
            spool.limit = 10
            try:
                for line in ("first line", "second line"):
                    spool.put(line)
                self.assertEqual(spool.get(1), "first line")
                # An unread line keeps the spool intact.
                self.assertGreater(path.stat().st_size, 10)
                self.assertEqual(spool.get(1), "second line")
                self.assertEqual(path.stat().st_size, 0)
                spool.put("third")
                self.assertEqual(spool.get(1), "third")
                # Under the limit, a fully read spool is kept.
                self.assertEqual(path.read_text(encoding="utf-8"), "third\n")
            finally:
                spool.close()


class RunSummaries(unittest.TestCase):
    def test_summary_reports_levels_floors_hunger_and_unavailable_fields(self):
        samples = [sample(0, hit_dice=2), sample(4500, "eat", "hungry"), sample(7000, depth=2)]
        run = campaign.summarize(fake_controller(), samples, "death", seed=3, wall_seconds=1.0)
        self.assertEqual(run["summary"], campaign.SUMMARY)
        self.assertEqual((run["deepest_depth"], run["death_depth"]), (2, 2))
        # The body's racial HD is not a Rogue level: total 2 is level 1.
        self.assertEqual(run["level_on_arrival"], {"1": 1, "2": 2})
        self.assertEqual(run["ticks_per_floor"], {"1": 5000, "2": 2000})
        self.assertAlmostEqual(run["hungry_share"], 1 / 3)
        self.assertEqual(run["meals_eaten"], 1)
        self.assertEqual(run["phases_per_floor"], {"1": {"explore": 1, "eat": 1}, "2": {"explore": 1}})
        self.assertEqual(run["final_level"], 2)
        self.assertIn("wanderer_arrivals", run["unavailable"])

    def test_censored_runs_have_no_death_depth(self):
        run = campaign.summarize(fake_controller(), [sample(0), sample(6000)],
                                 "censored_action_budget", seed=1, wall_seconds=1.0)
        self.assertFalse(run["died"])
        self.assertIsNone(run["death_depth"])

    def test_aggregate_excludes_harness_errors_from_shape(self):
        runs = [campaign.summarize(fake_controller(), [sample(0), sample(6000)], outcome,
                                   seed=seed, wall_seconds=1.0)
                for seed, outcome in enumerate(["death", "censored_action_budget"])]
        runs.append({"summary": campaign.SUMMARY, "seed": 9, "outcome": "harness_error"})
        report = campaign.aggregate(runs)
        self.assertEqual((report["runs"], report["completed"]), (3, 2))
        self.assertEqual(report["death_depths"], {"2": 1})
        self.assertEqual(report["mean_level_on_arrival"], {"1": 1.0, "2": 2.0})
        self.assertEqual(report["outcomes"]["harness_error"], 1)


class CampaignProcess(unittest.TestCase):
    def test_bounded_real_descent_produces_a_summary(self):
        run = campaign.run_seed(0, floor_limit=40, run_limit=40)
        self.assertNotEqual(run["outcome"], "harness_error", run.get("error"))
        self.assertEqual(run["summary"], campaign.SUMMARY)
        self.assertEqual(run["commands"], 40)
        self.assertEqual(run["outcome"], "censored_action_budget")
        self.assertEqual(run["level_on_arrival"]["1"], 1)



class ItemGeneration(unittest.TestCase):
    def receipt(self, scope, decisions):
        return {"scope": scope, "decisions": decisions}

    def placed(self, content):
        return {"kind": "placed", "content": content,
                "entity": {"kind": "item", "id": 1, "quantity": 1}}

    def test_counts_items_food_and_drought_per_floor(self):
        receipts = [
            self.receipt("dungeon/1", [
                {"id": "rogue.items", "result": {"attempts": [self.placed("rogue.healing")]}},
                {"id": "rogue.items_other", "result": {"attempts": [
                    self.placed("rogue.ration"), {"kind": "skipped", "content": "rogue.fruit"}]}},
                {"id": "rogue.food_drought", "result": {"attempts": []}},
                {"id": "rogue.eight_species_foes", "result": {"attempts": [self.placed("rogue.emu")]}},
            ]),
            self.receipt("dungeon/2", [
                {"id": "rogue.food_drought", "result": {"attempts": [self.placed("rogue.ration")]}},
                {"id": "rogue.treasure_room", "result": {"selection": {"items": [
                    self.placed("rogue.dart")]}}},
            ]),
        ]
        generation = campaign.item_generation(receipts)
        self.assertEqual(generation["floors"], 2)
        self.assertEqual(generation["items_per_floor"], 2)
        self.assertEqual(generation["food_per_floor"], 1)
        self.assertEqual(generation["drought_floors"], 1)

    def test_deep_start_uses_the_model_level(self):
        self.assertEqual(campaign.deep_start_level(1), 1)
        self.assertGreater(campaign.deep_start_level(10), campaign.deep_start_level(3))


if __name__ == "__main__":
    unittest.main()
