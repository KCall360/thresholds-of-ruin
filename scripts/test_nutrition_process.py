"""Shared nutrition and Eat through real ordinary server/Text/ASCII processes."""
import copy
import json
import shutil
import sqlite3
import unittest

from checkpoint_payload import decode_checkpoint_payload
from nutrition_package import ROLLED_MEAL, prepare, prepare_regeneration, prepare_rolled
from process_harness import ProcessTestCase, SPECTATOR_TOKEN, resource


# Source-algebra anchor: the next SplitMix state and word are both zero, so
# 1d400+1099 is exactly 1100. No test computes its expected result with the kernel.
FIRST_MEAL_SEED = 0x61C8864680B583EB


def observation(frame):
    return frame["state"]["observation"]


def nutrition(frame):
    return observation(frame)["combat"]["own_stats"]["nutrition"]


def authoritative_game(checkpoint):
    """Expand current pooled values, preserving every authoritative game field."""
    game = copy.deepcopy(checkpoint["game"])
    shared = checkpoint["shared"]
    for field, pool in (("actors", "actors"), ("items", "items"),
                        ("lifecycle", "lifecycles"), ("floor_activation", "floor_activations")):
        if field in game:
            game[field] = copy.deepcopy(shared[pool][game[field]])
    world = shared["worlds"]["instances"][game["world"]]
    game["world"] = copy.deepcopy(shared["worlds"]["geometry"][world["geometry"]])
    game["world"]["doors"] = copy.deepcopy(world["doors"])
    navigation = shared["navigation"]
    game["navigation"] = {
        actor: {field: [copy.deepcopy(navigation[field][region]) for region in references]
                for field, references in navigation["instances"][index].items()}
        for actor, index in game["navigation"].items()}
    game["intentions"]["entries"] = [
        copy.deepcopy(shared["intentions"]["entries"][index])
        for index in game["intentions"]["entries"]]
    return game


class NutritionProcesses(ProcessTestCase):
    def test_periodic_regeneration_fraction_survives_save_reconnect_without_duplicate_charge(self):
        package = prepare_regeneration(self)
        game = self.server("--checkpoint-interval", 1, scenario=package)
        player, frame = self.client()
        for index, pool in enumerate(("stamina", "focus", "mana"), 1):
            maximum = resource(frame, pool)["maximum"]
            paid = self.act(player, {"type": "use_ability", "id": "fixture_spend_" + pool,
                                    "invocation": {"kind": "actor_target",
                                                   "target": observation(frame)["self_target"]}})
            self.assertIsNone(paid["error"])
            self.assertEqual(resource(paid, pool)["balance"], maximum - 1)
            frame = self.act(player, {"type": "wait"})
            self.assertIsNone(frame["error"])
            self.assertEqual(resource(frame, pool)["balance"], maximum)
            self.assertEqual(nutrition(frame)["reserve"], 40 - index // 3)
            self.assertNotIn("regeneration_remainder", nutrition(frame))
            saved = self.checkpoint(player)
            retained = saved["actors"]["1"]["combat"]["creature"]["nutrition"]
            self.assertEqual(retained["regeneration_remainder"], index % 3)
            for _ in range(3):
                self.assertEqual(self.request(player, {"type": "snapshot"})["state"], frame["state"])
            failed = self.act(player, {"type": "use_ability", "id": "fixture_missing",
                                      "invocation": {"kind": "actor_target",
                                                     "target": observation(frame)["self_target"]}})
            self.assertIsNotNone(failed["error"])
            self.assertEqual(self.checkpoint(player), saved)
            if index == 1:
                player.stop(); game.stop()
                game = self.server("--checkpoint-interval", 1, seed=None)
                player, reopened = self.client()
                self.assertEqual(reopened["state"], frame["state"])
                self.assertEqual(self.checkpoint(player), saved)
                frame = reopened
        full = self.act(player, {"type": "wait"})
        self.assertEqual(nutrition(full)["reserve"], 39, "full pools cannot incur extra recovery cost")

    def checkpoint(self, player):
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as database:
            sequence, payload = database.execute("SELECT sequence, payload FROM checkpoint").fetchone()
            self.assertEqual(database.execute(
                "SELECT count(*) FROM journal WHERE sequence > ?", (sequence,)).fetchone()[0], 0)
        return authoritative_game(json.loads(decode_checkpoint_payload(payload)))

    def assert_no_meal_operand(self, value):
        if isinstance(value, dict):
            if "edible" in value:
                self.assertEqual(set(value), {"item", "slot", "equipped_slot", "known_equipment", "known_cursed", "known_protected",
                                             "drinkable", "edible", "readable", "zappable", "charges"})
                self.assertFalse(value["readable"])
                self.assertFalse(value["zappable"])
                self.assertIsNone(value["charges"])
                self.assertIsNone(value["known_cursed"])
                self.assertIsNone(value["known_protected"])
            for key, child in value.items():
                self.assertNotIn(key, ("dice", "rules_rng", "raw_sample", "on_success", "on_failure"))
                self.assert_no_meal_operand(child)
        elif isinstance(value, list):
            for child in value:
                self.assert_no_meal_operand(child)

    def finish_pending_meal(self, player, peer, before):
        self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
        return self.frame(player, lambda frame: frame.get("state") is not None
                          and nutrition(frame)["reserve"] > before
                          and not observation(frame)["combat"]["preparation_active"])

    def pause_meal(self, player):
        self.assertIsNone(self.request(player, {"type": "release_control"})["error"])
        def paused(frame):
            return any(row["phase"] == "paused" for row in frame["intentions"])
        current = self.request(player, {"type": "snapshot"})
        return current if paused(current) else self.frame(player, paused)

    def test_rolled_meals_sample_below_cap_and_delete_only_the_last_exact_source_unit(self):
        package = prepare_rolled(self, spare_meal=True)
        game = self.server("--checkpoint-interval", 1, scenario=package, seed=FIRST_MEAL_SEED)
        player, initial = self.client()
        meals = [item for item in observation(initial)["inventory"] if item["name"] == "fixture meal"]
        meal = next(item for item in meals if item["quantity"] == "2")
        spare = next(item for item in meals if item["quantity"] == "3")
        first = self.act(player, {"type": "eat", "item": meal["id"]})
        self.assertIsNone(first["error"])
        self.assertEqual(nutrition(first)["reserve"], 1108)
        self.assertEqual(next(item for item in observation(first)["inventory"]
                              if item["id"] == meal["id"])["quantity"], "1")
        saved = self.checkpoint(player)
        self.assertEqual(saved["rules_rng"], 0)
        self.assertEqual(saved["items"]["10"]["quantity"], 1)
        self.assertEqual(saved["items"]["10"]["holds"], [])
        self.assertEqual(saved["items"]["12"]["quantity"], 3)
        for _ in range(4):
            self.assertEqual(self.request(player, {"type": "snapshot"})["state"], first["state"])
            self.assertIsNone(self.request(player, {"type": "history", "limit": 50, "before": None})["error"])
            self.assertIsNone(self.command(player, {"type": "inspect"})["error"])
        self.assertEqual(self.checkpoint(player), saved, "queries must not draw, digest, or change holds")
        final = self.act(player, {"type": "eat", "item": meal["id"]})
        self.assertIsNone(final["error"])
        self.assertGreaterEqual(nutrition(final)["reserve"] - nutrition(first)["reserve"], 1100)
        self.assertLessEqual(nutrition(final)["reserve"] - nutrition(first)["reserve"], 1499)
        self.assertLess(nutrition(final)["reserve"], nutrition(final)["maximum"])
        self.assertFalse(any(item["id"] == meal["id"] for item in observation(final)["inventory"]))
        self.assertEqual(next(item for item in observation(final)["inventory"]
                              if item["id"] == spare["id"])["quantity"], "3")
        completed = self.checkpoint(player)
        self.assertNotIn("10", completed["items"], "the physical source disappears before frozen effects finish")
        self.assertEqual(completed["items"]["12"]["quantity"], 3)
        self.assertEqual(completed["items"]["12"]["holds"], [])
        self.assertNotEqual(completed["rules_rng"], saved["rules_rng"])
        player.stop(); game.stop()
        self.server("--checkpoint-interval", 1, seed=None)
        player, restored = self.client()
        self.assertEqual(restored["state"], final["state"])
        self.assertEqual(self.checkpoint(player), completed, "restart must not repeat the deleted item's meal")

    def test_source_deleted_pending_rolled_meal_matches_uninterrupted_checkpoint_continuation(self):
        package = prepare_rolled(self, peer=True, spare_meal=True, consume_turns=3)
        game = self.server("--checkpoint-interval", 1, scenario=package, seed=FIRST_MEAL_SEED)
        player, initial = self.client()
        peer, _ = self.client(actor=2)
        meal = next(item for item in observation(initial)["inventory"]
                    if item["name"] == "fixture meal" and item["quantity"] == "2")
        started = self.act(player, {"type": "eat", "item": meal["id"]})
        self.assertTrue(observation(started)["combat"]["preparation_active"])
        self.assertEqual(nutrition(started)["reserve"], 8)
        paused = self.pause_meal(player)
        self.assertEqual(paused["intentions"][0]["phase"], "paused")
        frozen = self.checkpoint(player)
        pending = frozen["actors"]["1"]["pending"]
        self.assertEqual(frozen["rules_rng"], FIRST_MEAL_SEED)
        self.assertEqual(pending["plan"]["effects"], [{"kind": "nourish", "amount": ROLLED_MEAL}])
        self.assertEqual(pending["item_payment"], {"kind": "held", "refs": [{"cost_ordinal": 0, "item": 10}]})
        self.assertEqual(frozen["items"]["10"]["quantity"], 2)
        self.assertEqual(len(frozen["items"]["10"]["holds"]), 1)
        self.assertEqual(frozen["items"]["10"]["holds"][0]["quantity"], 1)
        self.assertEqual(frozen["items"]["12"]["holds"], [])
        replay_save = self.directory / "rolled-replay.db"
        with sqlite3.connect(self.save) as source, sqlite3.connect(replay_save) as destination:
            source.backup(destination)

        # The control server stays alive. Both branches resume the same paused
        # owned plan; only the second branch restarts and receives extra queries.
        self.assertIsNone(self.request(player, {"type": "acquire_control"})["error"])
        self.assertIsNone(self.command(player, {"type": "resume_intention"})["error"])
        first = self.finish_pending_meal(player, peer, 8)
        self.assertEqual(nutrition(first)["reserve"], 1108)
        baseline = self.act(player, {"type": "eat", "item": meal["id"]})
        self.assertIsNone(baseline["error"])
        baseline_game = self.checkpoint(player)
        self.assertNotIn("10", baseline_game["items"])
        self.assertLess(nutrition(baseline)["reserve"], 4000)
        player.stop(); peer.stop(); game.stop()

        # Remove all original authoring inputs, retaining only immutable pinned
        # compiler artifacts. Neither branch removes the held runtime item.
        self.assertTrue((package / ".compiled").is_dir())
        for path in package.iterdir():
            if path.name != ".compiled":
                if path.is_dir():
                    shutil.rmtree(path)
                else:
                    path.unlink()
        self.assertFalse((package / "scenario.toml").exists())
        self.assertFalse((package / "regions").exists())
        self.save = replay_save
        game = self.server("--checkpoint-interval", 1, seed=None)
        player, restored = self.client()
        peer, _ = self.client(actor=2)
        self.assertEqual(restored["intentions"][0]["intention"], paused["intentions"][0]["intention"])
        self.assertEqual(self.checkpoint(player), frozen)
        for _ in range(6):
            self.assertIsNone(self.request(player, {"type": "snapshot"})["error"])
            self.assertIsNone(self.request(player, {"type": "history", "limit": 50, "before": None})["error"])
            self.assertIsNone(self.command(player, {"type": "inspect"})["error"])
        self.assertEqual(self.checkpoint(player), frozen)
        self.assertIsNone(self.command(player, {"type": "resume_intention"})["error"])
        first = self.finish_pending_meal(player, peer, 8)
        self.assertEqual(nutrition(first)["reserve"], 1108)
        replayed = self.act(player, {"type": "eat", "item": meal["id"]})
        self.assertIsNone(replayed["error"])
        self.assertEqual(replayed["state"], baseline["state"])
        self.assertEqual(self.checkpoint(player), baseline_game)
        player.stop(); peer.stop(); game.stop()
        self.server("--checkpoint-interval", 1, seed=None)
        player, reopened = self.client()
        self.assertEqual(reopened["state"], replayed["state"])
        self.assertEqual(self.checkpoint(player), baseline_game)

    def test_websocket_edible_affordance_never_discloses_rolled_operands_to_any_role(self):
        package = prepare_rolled(self, peer=True, concealed=True)
        self.server(scenario=package, seed=FIRST_MEAL_SEED)
        player, initial = self.client()
        observer, observed = self.client(observe=True)
        spectator, watched = self.client(SPECTATOR_TOKEN)
        peer, _ = self.client(actor=2)
        outsider, other_view = self.client(SPECTATOR_TOKEN, observe=True, actor=2)
        meal = next(item for item in observation(initial)["inventory"] if item["name"] == "wrapped fixture meal")
        self.assertFalse(meal["identified"])
        for frame in (initial, observed, watched):
            entry = next(row for row in observation(frame)["interactions"]["inventory"] if row["item"] == meal["id"])
            self.assertTrue(entry["edible"])
            self.assertEqual(set(entry), {"item", "slot", "equipped_slot", "known_equipment", "known_cursed", "known_protected",
                                          "drinkable", "edible", "readable", "zappable", "charges"})
            self.assertFalse(entry["readable"])
            self.assertFalse(entry["zappable"])
            self.assertIsNone(entry["charges"])
            self.assertIsNone(entry["known_cursed"])
            self.assertIsNone(entry["known_protected"])
            self.assert_no_meal_operand(frame)
        self.assertFalse(observed["has_control"])
        self.assertEqual(watched["role"], "spectator")
        self.assert_no_meal_operand(other_view)
        self.assert_no_meal_operand(self.command(player, {"type": "inspect"}))
        stone = next(item for item in observation(initial)["inventory"] if item["name"] == "stone food")
        self.assertIsNotNone(self.act(player, {"type": "eat", "item": stone["id"]})["error"])
        self.assertIsNotNone(self.act(spectator, {"type": "eat", "item": meal["id"]})["error"])
        started = self.act(player, {"type": "eat", "item": meal["id"]})
        self.assertTrue(observation(started)["combat"]["preparation_active"])
        paused = self.pause_meal(player)
        self.assertEqual(paused["intentions"][0]["phase"], "paused")
        for client in (observer, spectator, outsider):
            self.assert_no_meal_operand(self.request(client, {"type": "snapshot"}))
        self.assert_no_meal_operand(self.command(player, {"type": "inspect"}))
        self.assertIsNone(self.request(player, {"type": "acquire_control"})["error"])
        self.assertIsNone(self.command(player, {"type": "resume_intention"})["error"])
        final = self.finish_pending_meal(player, peer, 8)
        self.assertEqual(nutrition(final)["reserve"], 1108)
        for client in (player, observer, spectator):
            frame = self.request(client, {"type": "snapshot"})
            identified = next(item for item in observation(frame)["inventory"] if item["id"] == meal["id"])
            self.assertTrue(identified["identified"])
            self.assertEqual(identified["name"], "fixture meal")
            self.assert_no_meal_operand(frame)
            self.assertIsNone(self.request(client, {"type": "history", "limit": 50, "before": None})["error"])
        self.assert_no_meal_operand(self.command(player, {"type": "inspect"}))
        other = self.request(outsider, {"type": "snapshot"})
        self.assertEqual(nutrition(other)["reserve"], 8)
        self.assertNotIn('"nutrition"', json.dumps(observation(other)["combat"]["actors"]))
        # Every received wire message is retained by the real headless clients,
        # including queued/start/pause/resume/completion and rejected requests.
        for client in (player, observer, spectator, outsider):
            for line in client.transcript:
                if line.startswith("{"):
                    self.assert_no_meal_operand(json.loads(line))

    def test_eat_exact_unit_hunger_starvation_and_checkpoint_replay(self):
        package = prepare(self)
        game = self.server("--checkpoint-interval", 1, scenario=package, spectator=False)
        player, first = self.client()
        self.assertEqual(nutrition(first)["reserve"], 8)
        view = observation(first)
        meal = next(item for item in view["inventory"] if item["name"] == "fixture meal")
        stone = next(item for item in view["inventory"] if item["name"] == "stone food")
        denied = self.act(player, {"type": "eat", "item": stone["id"]})
        self.assertIsNotNone(denied["error"])
        self.assertEqual(denied["state"], first["state"])
        eaten = self.act(player, {"type": "eat", "item": meal["id"]})
        self.assertIsNone(eaten["error"])
        self.assertEqual(nutrition(eaten)["reserve"], 12)
        self.assertEqual(next(item for item in observation(eaten)["inventory"] if item["id"] == meal["id"])["quantity"], "1")
        self.assertIn({"type": "eat", "item": meal["id"]}, observation(eaten)["interactions"]["completed"])
        # Queries never earn time or consume reserve.
        self.assertEqual(nutrition(self.request(player, {"type": "snapshot"})), nutrition(eaten))
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop(); game.stop()
        self.server("--checkpoint-interval", 1, scenario=package, seed=None, spectator=False)
        player, restored = self.client()
        self.assertEqual(restored["state"], eaten["state"])
        states = set()
        final = restored
        for _ in range(16):
            final = self.act(player, {"type": "wait"})
            states.add(nutrition(final)["hunger"])
            if observation(final)["combat"]["dead"]:
                break
        self.assertTrue({"hungry", "weak", "empty"}.issubset(states))
        self.assertTrue(observation(final)["combat"]["dead"])
        self.assertEqual(nutrition(final)["reserve"], -3)
        self.assertEqual(final["intentions"], [])
        dropped = [row["item"] for row in observation(final)["ground_items"] if row["item"]["name"] == "fixture meal"]
        self.assertEqual(sum(int(item["quantity"]) for item in dropped), 1)

    def test_neutral_hunger_recovery_remains_running_through_source_free_restart(self):
        recovery = {field: {"kind": "normal"} for field in
                    ("hungry_recovery", "weak_recovery", "empty_recovery")}
        package = prepare(self, initial=2, recovery_policies=recovery)
        game = self.server("--checkpoint-interval", 1, scenario=package, spectator=False)
        player, initial = self.client()
        self.assertEqual(nutrition(initial)["hunger"], "weak")
        clock = observation(initial)["combat"]["own_stats"]["health_recovery"]
        self.assertFalse(clock["stopped"])
        empty = initial
        for _ in range(2):
            empty = self.act(player, {"type": "wait"})
            self.assertIsNone(empty["error"])
        self.assertEqual(nutrition(empty)["hunger"], "empty")
        self.assertFalse(observation(empty)["combat"]["own_stats"]["health_recovery"]["stopped"])
        self.assertFalse(observation(empty)["combat"]["dead"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop(); game.stop()
        # Restart uses saved, independently encoded rules and compiled inputs.
        for path in package.iterdir():
            if path.name != ".compiled":
                if path.is_dir():
                    shutil.rmtree(path)
                else:
                    path.unlink()
        self.server("--checkpoint-interval", 1, seed=None, spectator=False)
        player, restored = self.client()
        self.assertEqual(restored["state"], empty["state"])
        meal = next(item for item in observation(restored)["inventory"] if item["name"] == "fixture meal")
        eaten = self.act(player, {"type": "eat", "item": meal["id"]})
        self.assertIsNone(eaten["error"])
        self.assertEqual(nutrition(eaten)["reserve"], 6)
        self.assertFalse(observation(eaten)["combat"]["own_stats"]["health_recovery"]["stopped"])
        self.assertEqual(clock["period_ticks"], observation(eaten)["combat"]["own_stats"]["health_recovery"]["period_ticks"])

    def test_starvation_at_eat_completion_releases_the_held_food_unconsumed(self):
        package = prepare(self, initial=0, consume_turns=3)
        self.server("--checkpoint-interval", 1, scenario=package, spectator=False)
        player, initial = self.client()
        meal = next(item for item in observation(initial)["inventory"] if item["name"] == "fixture meal")
        final = self.act(player, {"type": "eat", "item": meal["id"]})
        self.assertIsNone(final["error"])
        self.assertTrue(observation(final)["combat"]["dead"])
        self.assertEqual(nutrition(final)["reserve"], -3)
        self.assertEqual(final["intentions"], [])
        dropped = [row["item"] for row in observation(final)["ground_items"] if row["item"]["name"] == "fixture meal"]
        self.assertEqual(sum(int(item["quantity"]) for item in dropped), 2)
        self.assertEqual(int(observation(final)["tick"]), 300)

    def test_text_eat_and_stats_show_personal_nutrition(self):
        package = prepare(self)
        self.server(scenario=package, spectator=False)
        text = self.launch("tor-client-text", ["--script", "--connect", self.address])
        text.until(lambda line: line == "Ready.")
        self.assertIn("Nutrition: 8/12", text.command("stats"))
        self.assertIn("ItemStarted", text.command("eat fixture meal"))
        self.assertIn("Nutrition: 12/12", text.command("stats"))

    def test_text_rolled_eat_reports_uncapped_gain_and_last_unit_depletion(self):
        package = prepare_rolled(self)
        self.server(scenario=package, seed=FIRST_MEAL_SEED)
        observer, _ = self.client(observe=True)
        text, _ = self.text_client()
        self.assertIn("ItemStarted", text.command("eat fixture meal"))
        self.assertIn("Nutrition: 1108/4000", text.command("stats"))
        self.assertIn("ItemStarted", text.command("eat fixture meal"))
        final = self.request(observer, {"type": "snapshot"})
        self.assertGreaterEqual(nutrition(final)["reserve"], 2208)
        self.assertLessEqual(nutrition(final)["reserve"], 2607)
        self.assertFalse(any(item["name"] == "fixture meal" for item in observation(final)["inventory"]))


class NutritionAsciiProcesses(ProcessTestCase):
    graphical = True

    def test_native_e_opens_an_exact_meal_picker_without_advancing_time(self):
        package = prepare(self)
        self.server(scenario=package, spectator=False)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        initial = self.ascii_frame(window, lambda frame: frame["has_control"] and not frame["busy"])
        native = self.native_keys(window)
        menu = self.native_press(window, native, "e", "eat")
        self.assertEqual(menu["state"], initial["state"])
        accepted = self.native_press(window, native, "Return", "enter")
        def finished(frame):
            return frame["state"]["observation"]["combat"]["own_stats"]["nutrition"]["reserve"] == 12
        final = accepted if finished(accepted) else self.ascii_frame(window, finished)
        self.assertTrue(final["window_open"])
        self.assertEqual(next(item for item in final["state"]["observation"]["inventory"] if item["name"] == "fixture meal")["quantity"], "1")

    def test_ascii_eat_picker_and_status(self):
        package = prepare(self)
        self.server(scenario=package, spectator=False)
        ascii_client = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"])
        self.ascii_frame(ascii_client, lambda f: f["state"] is not None and not f["busy"])
        self.key(ascii_client, "eat")
        eaten = self.key(ascii_client, "enter")
        self.assertEqual(eaten["state"]["observation"]["combat"]["own_stats"]["nutrition"]["reserve"], 12)
        self.assertTrue(eaten["window_open"])
        self.assertEqual(next(item for item in eaten["state"]["observation"]["inventory"] if item["name"] == "fixture meal")["quantity"], "1")

    def test_ascii_rolled_eat_picker_queries_preserve_the_first_uncapped_roll(self):
        package = prepare_rolled(self)
        self.server(scenario=package, seed=FIRST_MEAL_SEED)
        ascii_client = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"])
        initial = self.ascii_frame(ascii_client, lambda frame: frame["state"] is not None and not frame["busy"])
        for _ in range(3):
            picker = self.key(ascii_client, "eat")
            self.assertEqual(picker["state"], initial["state"])
            closed = self.key(ascii_client, "escape")
            self.assertEqual(closed["state"], initial["state"])
        self.key(ascii_client, "eat")
        first = self.key(ascii_client, "enter")
        self.assertEqual(nutrition(first)["reserve"], 1108)
        self.assertTrue(first["window_open"])
        self.key(ascii_client, "eat")
        final = self.key(ascii_client, "enter")
        self.assertGreaterEqual(nutrition(final)["reserve"], 2208)
        self.assertLessEqual(nutrition(final)["reserve"], 2607)
        self.assertFalse(any(item["name"] == "fixture meal" for item in observation(final)["inventory"]))


if __name__ == "__main__":
    unittest.main()
