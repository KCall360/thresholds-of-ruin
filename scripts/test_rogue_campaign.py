"""Run summaries, campaign aggregation and one bounded real-process descent."""
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

import rogue_campaign as campaign
from rogue_earned_route import Decision


def witness(total, tick):
    return {"hit_dice": {"total": str(total)}, "tick": str(tick)}


def fake_controller():
    return SimpleNamespace(
        first_entries={1: {"tick": "0", "commands": 0}, 2: {"tick": "5000", "commands": 40}},
        growth_history=[witness(2, 0), witness(3, 4000)],
        floor_commands={1: 40, 2: 10}, commands=50, rest_episodes=2)


def sample(tick, phase="explore", hunger="nourished", hit_dice=3, depth=1, foes=()):
    return {"tick": str(tick), "phase": phase, "depth": depth, "foes": list(foes),
            "hp": 10, "max_hp": 20,
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


def view(*, hp=5, hunger=None, rations=0, foe_at=None):
    actors = [] if foe_at is None else [{"actor": "9", "hostile": True}]
    visible = [] if foe_at is None else [
        {"id": "9", "name": "orc", "position": dict(zip("xyz", foe_at))}]
    inventory = [{"id": "r", "quantity": str(rations), "known_supply": {"id": "ration"}}] if rations else []
    return {"observation": {
        "combat": {"hp": str(hp), "max_hp": "20", "actors": actors,
                   "own_stats": {"nutrition": {"hunger": hunger},
                                 "resources": [{"balance": 1, "maximum": 4}]}},
        "visible_actors": visible, "inventory": inventory}}


class CampaignHabits(unittest.TestCase):
    def setUp(self):
        self.controller = campaign.CampaignController(1)
        self.seen = []

    def base(self, results):
        def choose(controller, state):
            full = state["observation"]["combat"]["hp"] == state["observation"]["combat"]["max_hp"]
            self.seen.append(full)
            return results[full]
        return mock.patch.object(campaign.Controller, "choose", choose)

    def test_steady_shows_full_health_and_resources_without_touching_the_state(self):
        state = view(hp=3)
        shown = campaign.steady(state)
        self.assertEqual(shown["observation"]["combat"]["hp"], "20")
        self.assertEqual(shown["observation"]["combat"]["own_stats"]["resources"][0]["balance"], 4)
        self.assertEqual(state["observation"]["combat"]["hp"], "3")

    def test_retreat_on_foot_beside_a_foe_becomes_a_fight(self):
        fight = Decision({"type": "use_ability", "id": "basic_melee"}, "combat")
        step = Decision({"type": "move", "direction": "west"}, "retreat")
        with self.base({False: step, True: fight}):
            self.assertEqual(self.controller.choose(view(foe_at=(1, 0, 0))), fight)
            # A distant foe still allows the step away.
            self.assertEqual(self.controller.choose(view(foe_at=(4, 0, 0))), step)
        stairs = Decision({"type": "move", "direction": "up"}, "retreat")
        with self.base({False: stairs, True: fight}):
            self.assertEqual(self.controller.choose(view(foe_at=(1, 0, 0))), stairs)

    def test_starving_without_food_stops_resting(self):
        rest = Decision({"type": "wait"}, "rest")
        onward = Decision({"type": "move", "direction": "east"}, "supply_approach")
        with self.base({False: rest, True: onward}):
            self.assertEqual(self.controller.choose(view(hunger="weak")), onward)
            self.assertEqual(self.seen, [True])
            # Food in the pack, or mere hunger, keeps the ordinary rest.
            self.assertEqual(self.controller.choose(view(hunger="weak", rations=1)), rest)
            self.assertEqual(self.controller.choose(view(hunger="hungry")), rest)

    def test_starving_explorer_takes_known_down_stairs(self):
        explore = Decision({"type": "move", "direction": "east"}, "explore")
        floor = mock.Mock()
        floor.graph.stairs_down = {"stairs"}
        floor.route.return_value = [("stairs", "north")]
        self.controller.floors[1] = floor
        here = {"key": "here", "stairs_down": False}
        with self.base({False: explore, True: explore}),                 mock.patch.object(campaign, "_here", return_value=here),                 mock.patch.object(campaign.CampaignController, "_step_decision",
                                  lambda self, view, direction, phase: Decision(
                                      {"type": "move", "direction": direction}, phase)):
            self.assertEqual(self.controller.choose(view(hunger="empty")),
                             Decision({"type": "move", "direction": "north"}, "descend"))
            here["stairs_down"] = True
            self.assertEqual(self.controller.choose(view(hunger="empty")),
                             Decision({"type": "move", "direction": "down"}, "descend"))
            self.assertEqual(self.controller.choose(view(hunger=None)), explore)


class RunSummaries(unittest.TestCase):
    def test_summary_reports_levels_floors_hunger_and_unavailable_fields(self):
        samples = [sample(0, hit_dice=2), sample(4500, "eat", "hungry"),
                   sample(7000, "combat", depth=2, foes=["orc", "bat"])]
        run = campaign.summarize(fake_controller(), samples, "death", seed=3, wall_seconds=1.0)
        self.assertEqual(run["summary"], campaign.SUMMARY)
        self.assertEqual((run["deepest_depth"], run["death_depth"]), (2, 2))
        # The body's racial HD is not a Rogue level: total 2 is level 1.
        self.assertEqual(run["level_on_arrival"], {"1": 1, "2": 2})
        self.assertEqual(run["ticks_per_floor"], {"1": 5000, "2": 2000})
        self.assertAlmostEqual(run["hungry_share"], 1 / 3)
        self.assertEqual(run["meals_eaten"], 1)
        self.assertEqual(run["phases_per_floor"], {"1": {"explore": 1, "eat": 1}, "2": {"combat": 1}})
        self.assertEqual(run["before_death"], {"foes": {"orc": 1, "bat": 1},
                                               "phases": {"explore": 1, "eat": 1, "combat": 1},
                                               "hunger": {"nourished": 2, "hungry": 1}})
        self.assertEqual(run["hunger_at_death"], "nourished")
        self.assertEqual(run["final_level"], 2)
        self.assertIn("wanderer_arrivals", run["unavailable"])

    def test_levels_on_arrival_compare_ticks_as_numbers(self):
        # "88300" sorts after "500000" as text; a later level must not count.
        controller = fake_controller()
        controller.first_entries = {1: {"tick": "0", "commands": 0},
                                    2: {"tick": "88300", "commands": 40}}
        controller.growth_history = [witness(2, 0), witness(3, 50000), witness(9, 500000)]
        run = campaign.summarize(controller, [sample(0), sample(600000, depth=2)],
                                 "censored_action_budget", seed=1, wall_seconds=1.0)
        self.assertEqual(run["level_on_arrival"], {"1": 1, "2": 2})
        # The raw evidence is kept so levels can be recomputed.
        self.assertEqual(run["first_entry_ticks"], {"1": "0", "2": "88300"})
        self.assertEqual(run["growth"], [["0", 2], ["50000", 3], ["500000", 9]])

    def test_the_named_cause_of_death_is_recorded(self):
        me, other = "a_01", "a_02"
        view = {"self_target": me, "combat": {"events": [
            {"type": "died", "actor": other, "cause": "injury"},
            {"type": "died", "actor": me, "cause": "starvation"}]}}
        self.assertEqual(campaign.own_death_cause(view), "starvation")
        view["combat"]["events"] = view["combat"]["events"][:1]
        self.assertIsNone(campaign.own_death_cause(view))
        run = campaign.summarize(fake_controller(), [sample(0), sample(6000)], "death",
                                 seed=1, wall_seconds=1.0, cause="starvation")
        self.assertEqual(run["cause"], "starvation")
        censored = campaign.summarize(fake_controller(), [sample(0), sample(6000)],
                                      "censored_action_budget", seed=1, wall_seconds=1.0)
        self.assertIsNone(censored["cause"])

    def test_a_failed_closing_save_keeps_the_finished_run(self):
        run = campaign.summarize(fake_controller(), [sample(0), sample(6000)],
                                 "death", seed=2, wall_seconds=1.0)

        def failed_save():
            raise AssertionError("Explicit save failed: " + "x" * 5000 + " behind schedule")
        campaign.keep_item_generation(run, failed_save)
        self.assertEqual(run["outcome"], "death")
        self.assertIsNone(run["item_generation"])
        self.assertIn("item_generation", run["unavailable"])
        self.assertTrue(run["item_generation_error"].endswith("behind schedule"))

    def test_harness_errors_keep_the_final_exception(self):
        text = "Traceback\n" + "y" * 5000 + "\nAssertionError: the cause"
        clipped = campaign.clipped(text)
        self.assertTrue(clipped.startswith("Traceback"))
        self.assertTrue(clipped.endswith("AssertionError: the cause"))
        self.assertLess(len(clipped), 3100)

    def test_censored_runs_have_no_death_depth(self):
        run = campaign.summarize(fake_controller(), [sample(0), sample(6000)],
                                 "censored_action_budget", seed=1, wall_seconds=1.0)
        self.assertFalse(run["died"])
        self.assertIsNone(run["death_depth"])
        self.assertIsNone(run["before_death"])
        self.assertIsNone(run["hunger_at_death"])

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
