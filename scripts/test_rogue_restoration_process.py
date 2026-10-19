"""One bounded production R/restoration composition with the unchanged Warrior.

The production R is autonomous. The unchanged Warrior fights it with disclosed
ordinary melee actions, then picks up and drinks one reachable production potion.
Only geometry and ground stock are fixtures; ordinary XP and growth remain active.
This is one starter-kit recovery witness, not generated-floor placement, an earned
route or balance. Population receipts own finite generation.

Unchanged contract owners, deliberately not multiplied here:
* abilities/restorable_loss_tests.rs: restoration_at_zero_deficit_is_a_successful_
  unchanged_effect; checked_loss_immunity_skips_its_draw_without_suppressing_the_
  primary; ordinary_consumable_restores_recorded_deficits_and_spends_exactly_one_item.
* simulation/tests/it/interactions.rs: drinking_consumes_at_completion_and_
  identifies_only_observable_effects (ineffective use and last-unit knowledge).
* test_restorable_loss_process.RestorableLossProcesses: pending restoration's
  frozen effects/hold and Text stats/drink; RestorableLossAsciiProcesses: native
  stats and Q drinking. Those unchanged clients retain their own real witnesses.
"""
import json
import shutil
import subprocess
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT, settle_rogue_action
from process_work_budget import ProcessBudgetExceeded, ProcessWorkBudget
from restorable_loss_harness import losses
from rogue_roster_package import PRODUCTION, encounter
from rogue_test_support import cardinal_melee_candidate, disclosed_actor_candidates
from scalar_wire import u64
from status_condition_package import actor_creature, checkpoint, combat, observation


BITE = "rogue.venomous_bite"
POTION = "rogue.restore_strength"
POTION_NAME = "potion of restore strength"
DISK_LIMIT = 64 * 1024 * 1024


def restoration_package(test):
    """Copy production modules and the actual unspent Warrior preset verbatim."""
    manifest, region = encounter("rattlesnake", character=1, autonomous=True)
    manifest["id"] = "rogue-rattlesnake-restoration"
    region.update(size=[4, 1, 2], anchors={"start": [1, 0, 0]})
    region["actors"][0].update(at=[2, 0, 0])
    # This restoration mechanism needs contact and underfoot pickup, not a
    # chase. Seal R's far end without changing production AI; the player's
    # west cell remains available for the original source-free revisit.
    region["walls"] = [[3, 0, z] for z in range(2)]
    # Explicit reachable stock, not evidence of a generated population attempt.
    region["items"] = [{"id": 1000, "at": [1, 0, 0], "archetype": POTION, "quantity": 1}]
    package = test.directory / "restoration"
    (package / "regions").mkdir(parents=True)
    (package / "catalog").mkdir()
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    log = test.directory / "restoration-validation.log"
    test.work_budget.output.paths += (log,)
    test.check_disk()
    with log.open("w", encoding="utf-8") as output:
        result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                                stdout=output, stderr=subprocess.STDOUT, timeout=30)
    test.check_disk()
    test.assertEqual(result.returncode, 0, log.read_text(encoding="utf-8")[-2000:])
    return package


def injury(frame):
    return u64(combat(frame)["max_hp"]) - u64(combat(frame)["hp"])


def potions(frame):
    return [item for item in observation(frame)["inventory"] if item["class"] == "potion"]


class RogueRestorationProcesses(ProcessTestCase):
    # First full witness from the bounded initial 0-7 qualification (0-4: no loss).
    # The permanent regression executes one seed, never a search or retry.
    seed = 5

    def setUp(self):
        super().setUp()
        self.work_budget = ProcessWorkBudget(requests=320, actor_ticks=8000,
                                             transcript_bytes=8 * 1024 * 1024)
        self.disk_peak = 0
        self.sample = {"seed": self.seed, "phase": "setup"}

    def check_disk(self):
        """Sample disk only at compiler, encounter and replay boundaries."""
        size = 0
        for path in self.directory.rglob("*"):
            try:
                size += path.stat().st_size if path.is_file() else 0
            except FileNotFoundError:  # SQLite may finish/remove a transient WAL.
                continue
        self.disk_peak = max(self.disk_peak, size)
        if size > DISK_LIMIT:
            raise ProcessBudgetExceeded("disk_bytes", size, DISK_LIMIT)
        self.work_budget.check()

    def tearDown(self):
        report = {**self.work_budget.report(), "sample": self.sample,
                  "sampled_disk_peak_bytes": self.disk_peak, "disk_limit_bytes": DISK_LIMIT}
        (self.directory / "work-budget.json").write_text(json.dumps(report, indent=2) + "\n",
                                                        encoding="utf-8")
        print("R/restoration budget: " + json.dumps(report, sort_keys=True), flush=True)
        if not self._failed():
            self.check_disk()
        super().tearDown()

    def launch(self, *args, **kwargs):
        self.work_budget.check()
        process = super().launch(*args, **kwargs)
        # Include server output as well as client and compiler output.
        self.work_budget.output.paths += (process.transcript.path,)
        self.work_budget.check()
        return process

    def client(self, *args, **kwargs):
        kwargs["omit_memory"] = True  # No assertion uses accumulated map memory.
        client, frame = super().client(*args, **kwargs)
        self.work_budget.track(client, frame)
        self.work_budget.check()
        return client, frame

    def command(self, client, value):
        self.work_budget.before()
        result = super().command(client, value)
        self.work_budget.after(client, result)
        return result

    def frame(self, client, predicate, seconds=15):
        return self.work_budget.frame_wait(super().frame, client, predicate, seconds=seconds)

    def execute(self, player, action, *, survive=True):
        return settle_rogue_action(self, player, self.act(player, action), survive=survive)

    def stop_case(self, server, player):
        # Explicit checkpoint requests own the save; cleanup must add no request.
        server.stop(save=False); player.stop(save=False)
        self.check_disk()

    def assert_starter(self, player, initial):
        self.assertFalse(initial["state"]["wizard_game"])
        self.assertEqual(u64(combat(initial)["own_stats"]["hit_dice"]["total"]), 1)
        self.assertEqual({row["name"]: int(row["quantity"]) for row in observation(initial)["inventory"]},
                         {"mace": 1, "ring mail": 1, "clean bandage": 3, "food ration": 1})
        names = {row["id"]: row["name"] for row in observation(initial)["inventory"]}
        self.assertEqual({names[row["item"]]: row["slot"]["id"]
                          for row in observation(initial)["interactions"]["inventory"]
                          if row["equipped_slot"] is not None},
                         {"mace": "weapon", "ring mail": "body_armor"})
        overview = self.command(player, {"type": "advancement", "command": "overview"})
        self.assertEqual(overview["advancement_page"]["entries"][0]["value"]["points"],
                         {"earned": "1", "spent": "0", "unspent": "1"})

    def test_unchanged_warrior_recovers_rattlesnake_loss_with_one_ordinary_potion_and_replays(self):
        package = restoration_package(self)
        server = self.server("--checkpoint-interval", 1, scenario=package, character=1, seed=self.seed)
        player, initial = self.client(actor=1)
        self.assert_starter(player, initial)
        ground = [row for row in observation(initial)["ground_items"] if row["item"]["class"] == "potion"]
        self.assertEqual(len(ground), 1)
        self.assertTrue(ground[0]["reachable"])
        self.assertEqual(ground[0]["position"], {"x": 0, "y": 0, "z": 0})
        self.assertEqual(ground[0]["item"]["quantity"], "1")
        self.assertFalse(ground[0]["item"]["identified"])
        self.assertNotIn(POTION_NAME, json.dumps(initial))
        foe_id = next(row["id"] for row in observation(initial)["visible_actors"]
                      if row["name"] == "rattlesnake")
        current, attacks = initial, 0
        self.sample["phase"] = "fight"
        start = len(player.transcript)
        for _ in range(24):
            target = next((row for row in disclosed_actor_candidates(observation(current)["visible_actors"])
                           if row["name"] == "rattlesnake"), None)
            if target is None or combat(current)["terminal"]:
                break
            self.assertTrue(cardinal_melee_candidate(target))
            current = self.execute(player, {"type": "use_ability", "id": "basic_melee",
                "invocation": {"aim": {"kind": "actor_target", "target": target["id"]}, "weapon": {"kind": "default"}}}, survive=False)
            attacks += 1
        events = [event for line in player.transcript[start:] if line.startswith("{")
                  for frame in [json.loads(line)] if frame.get("state") is not None
                  and int(frame["state"]["revision"]) > int(initial["state"]["revision"])
                  for event in combat(frame)["events"]]
        venom = any(event["type"] == "ability" and event["ability"]["id"] == BITE
                    and event["caster"] == foe_id for event in events)
        died = any(event["type"] == "died" and event["actor"] == foe_id for event in events)
        deficit = losses(current)["attributes"]["strength"]
        recovery = combat(current)["own_stats"]["health_recovery"]
        remaining = int(recovery["period_ticks"]) - int(recovery["elapsed_ticks"])
        self.sample.update(attacks=attacks, deficit=deficit, dead=combat(current)["dead"],
                           injury=injury(current), recovery_ticks_remaining=remaining,
                           observed_venom=venom, observed_r_death=died)
        self.check_disk()
        self.assertFalse(combat(current)["dead"], "Fixed starter-kit sample died; do not buff or extend bounds")
        killed = checkpoint(self, player)
        self.assertTrue(actor_creature(killed, 3)["dead"], "R survived the bounded ordinary fight")
        self.assertTrue(venom, "Autonomous production R must disclose a resolved Venomous Bite")
        self.assertTrue(died, "The Warrior must observe R die, not merely disappear")
        self.assertTrue(any(event["type"] == "ability" and event["ability"]["id"] == "basic_melee"
            and event["caster"] == observation(initial)["self_target"] and event["target"] == foe_id
            and event["outcome"] == "applied" for event in events))
        self.assertGreater(deficit, 0, "Fixed sample needs actual retained Strength loss")
        self.assertGreater(injury(current), 0, "Fixed sample needs a real surviving injury")
        self.assertGreater(remaining, 200, "Pickup/drink must precede passive Health recovery")
        self.assertGreater(actor_creature(killed)["earned_progress"]["earned_xp"], 0)
        self.sample["phase"] = "restoration"

        # Actions use only current disclosed handles. Private saved IDs below
        # are audit assertions after the choices, never a gameplay oracle.
        ground = next(row for row in observation(current)["ground_items"]
                      if row["item"]["class"] == "potion")
        self.assertTrue(ground["reachable"], "Pickup must use the server's ordinary reach affordance")
        before_take = int(observation(current)["tick"])
        carried = self.execute(player, {"type": "take", "item": ground["item"]["id"], "quantity": None})
        self.assertGreater(int(observation(carried)["tick"]), before_take)
        self.assertEqual(len(potions(carried)), 1)
        item = potions(carried)[0]
        self.assertEqual(item["quantity"], "1")
        self.assertFalse(item["identified"])
        affordance = next(row for row in observation(carried)["interactions"]["inventory"]
                          if row["item"] == item["id"])
        self.assertTrue(affordance["drinkable"], "Physical drinkability does not require identity knowledge")
        self.assertNotIn(POTION_NAME, json.dumps(carried))
        self.assertEqual(losses(carried)["attributes"]["strength"], deficit)
        saved_before = checkpoint(self, player)
        self.assertEqual(saved_before["items"]["1000"]["location"], {"Carried": 1})
        self.assertEqual(saved_before["items"]["1000"]["quantity"], 1)
        self.assertNotIn(POTION, saved_before["actors"]["1"]["knowledge"])

        # The unchanged item timing is one actual 100-tick Warrior turn. Keep
        # this dose strictly before passive recovery, so injury equality is a
        # causal restoration assertion rather than a claim that Health is fixed.
        before = self.command(player, {"type": "advancement", "command": "overview"})
        growth = before["advancement_page"]["entries"][0]["value"]["growth"]
        self.assertLess(u64(growth["held_xp"]), u64(growth["next_threshold"]),
                        "Restoration may legitimately retry funded blocked growth")
        stats = combat(before)["own_stats"]
        self.assertTrue(all(row["balance"] == row["maximum"] for row in stats["resources"]),
                        "Full pools separate potion effects from ordinary passive recovery")
        recovery = stats["health_recovery"]
        self.assertLess(int(recovery["elapsed_ticks"]) + 100, int(recovery["period_ticks"]))
        restored = self.execute(player, {"type": "drink", "item": item["id"]})
        elapsed = int(observation(restored)["tick"]) - int(observation(before)["tick"])
        self.assertEqual(elapsed, 100)
        self.assertLess(int(recovery["elapsed_ticks"]) + elapsed, int(recovery["period_ticks"]))
        self.assertIn({"type": "drink", "item": item["id"]},
                      observation(restored)["interactions"]["completed"])
        self.assertEqual(losses(restored), losses(initial))
        attributes = dict(combat(before)["own_stats"]["attributes"])
        attributes["strength"] += deficit
        self.assertEqual(combat(restored)["own_stats"]["attributes"], attributes)
        self.assertEqual({row["resource"]["id"]: row["balance"]
                          for row in combat(restored)["own_stats"]["resources"]},
                         {row["resource"]["id"]: row["balance"]
                          for row in combat(before)["own_stats"]["resources"]},
                         "Restoration does not refill resource balances")
        self.assertEqual(u64(combat(restored)["max_hp"]), u64(combat(before)["max_hp"]) + deficit)
        self.assertEqual(injury(restored), injury(before))
        self.assertGreater(injury(restored), 0)
        self.assertFalse(combat(restored)["dead"])
        self.assertEqual(potions(restored), [])
        self.assertFalse(any(row["item"]["class"] == "potion"
                             for row in observation(restored)["ground_items"]))
        saved_after = checkpoint(self, player)
        self.assertNotIn("1000", saved_after["items"], "The sole ordinary unit was consumed")
        self.assertEqual(actor_creature(saved_after)["build"], actor_creature(saved_before)["build"])
        self.assertEqual(actor_creature(saved_after)["earned_progress"],
                         actor_creature(saved_before)["earned_progress"], "Keep legitimate pre-dose XP/growth")
        self.assertEqual(actor_creature(saved_after)["injury"], actor_creature(saved_before)["injury"])
        self.assertEqual(saved_after["rules_rng"], saved_before["rules_rng"], "Fixed restoration draws no dice")
        self.assertIn(POTION, saved_after["actors"]["1"]["knowledge"],
                      "Observable positive restoration identifies even the consumed final unit")
        self.assertNotIn(POTION, saved_after["actors"]["3"]["knowledge"],
                         "Another actor does not inherit the Warrior's observed-use knowledge")
        self.stop_case(server, player)

        shutil.rmtree(package)
        self.assertFalse(package.exists())
        server = self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        self.sample["phase"] = "replay"
        player, reopened = self.client(actor=1)
        self.assertEqual(reopened["state"], restored["state"])
        self.assertEqual(checkpoint(self, player), saved_after)
        # Only already disclosed neighboring cells choose this local round trip;
        # generated floor revisit/activation behavior has independent owners.
        for direction, x in (("west", -1), ("east", 1)):
            current = self.request(player, {"type": "snapshot"})
            self.assertTrue(any(cell["position"] == {"x": x, "y": 0, "z": 0} and not cell["wall"]
                                for cell in observation(current)["visible_cells"]))
            current = self.execute(player, {"type": "move", "direction": direction})
        for _ in range(2):
            self.assertIsNone(self.command(player, {"type": "inspect"})["error"])
        self.assertEqual(potions(current), [])
        self.assertFalse(any(row["item"]["class"] == "potion"
                             for row in observation(current)["ground_items"]))
        revisited = checkpoint(self, player)
        self.assertNotIn("1000", revisited["items"])
        self.assertIn(POTION, revisited["actors"]["1"]["knowledge"])
        self.stop_case(server, player)
        self.sample["phase"] = "passed"


if __name__ == "__main__":
    unittest.main()
