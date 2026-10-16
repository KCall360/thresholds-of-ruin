"""Generated C/Y composition, not an actual-kit, earned-route or balance study.

A labeled authored HD16 player provokes each unchanged passive recipe, witnesses
its natural sequence, then kills it ordinarily. Fixed birth seeds avoid process
searches; exact chance boundaries and pool permutations have lower-level owners.
"""
import json
import shutil
import unittest

from process_harness import ProcessTestCase, settle_rogue_action
from process_work_budget import ProcessWorkBudget
from rogue_roster_package import encounter_sources, unrelated_fallback
from rogue_test_support import disclosed_actor_candidates
from rogue_vampire_package import generated_carrier_package
from scalar_wire import u64
from status_condition_package import actor_creature, checkpoint, combat, observation


class RogueCarrierProcesses(ProcessTestCase):
    def setUp(self):
        super().setUp()
        self.work_budget = ProcessWorkBudget(requests=300, actor_ticks=12000,
                                             transcript_bytes=16 * 1024 * 1024)

    def tearDown(self):
        report = self.work_budget.report()
        (self.directory / "work-budget.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("Process budget " + self.id() + ": " + json.dumps(report, sort_keys=True), flush=True)
        if not self._failed():
            self.work_budget.check()
        super().tearDown()

    def client(self, *args, **kwargs):
        self.work_budget.check()
        kwargs["omit_memory"] = True  # This witness never asserts accumulated map memory.
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
        self.work_budget.before()
        super().flush_save(actor=actor)
        self.work_budget.check()

    def execute(self, player, action):
        current = settle_rogue_action(self, player, self.act(player, action))
        self.assertIsNone(current["error"])
        self.assertFalse(combat(current)["dead"], "Authored mechanism player must survive")
        return current

    def target(self, current, species):
        return next((row for row in disclosed_actor_candidates(observation(current)["visible_actors"])
                     if row["name"] == species), None)

    def assert_retaliation(self, player, start, target, strikes):
        for raw in player.transcript[start:]:
            frame = json.loads(raw)
            view = (frame.get("state") or {}).get("observation")
            if view is None:
                continue
            events = [row for row in view["combat"]["events"] if row["type"] == "ability"
                      and row["caster"] == target and row["ability"]["id"] == "basic_melee"]
            if [row["strike"] for row in events] == list(range(strikes)):
                return True
        return False

    def generated_carrier(self, species, seed, depth, strikes, stock_content):
        package = generated_carrier_package(self, species)
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=seed)
        player, current = self.client()
        self.assertFalse(current["state"]["wizard_game"])
        self.assertEqual(u64(combat(current)["own_stats"]["hit_dice"]["total"]), 16)
        receipts = self.private_receipts()
        self.assertEqual(len(receipts), 1)
        self.assertEqual(receipts[0]["scope"], f"dungeon/{depth}")
        attempts = receipts[0]["decisions"][0]["result"]["attempts"]
        self.assertEqual(len(attempts), 1)
        born = attempts[0]
        self.assertEqual((born["kind"], born["content"]), ("placed", "rogue." + species))
        actor_id = born["entity"]["id"]
        stock = born["entity"]["carried_loot"]
        self.assertEqual((stock["kind"], stock["content"], stock["quantity"]),
                         ("carried", stock_content, 1))
        item_id = str(stock["item"])
        target = self.target(current, species)
        self.assertIsNotNone(target, "Tiny lit mechanism must disclose its ordinary newborn")
        target_id = target["id"]
        health = combat(current)["hp"]
        # Ordinary unequip gives a low-damage provocation without altering either
        # creature sheet. Re-equip only after observing the full natural sequence.
        weapon = next(row["id"] for row in observation(current)["inventory"] if row["name"] == "mace")
        current = self.execute(player, {"type": "unequip", "item": weapon})
        for _ in range(8):
            target = self.target(current, species)
            self.assertIsNotNone(target)
            position = target["position"]
            if max(abs(position["x"]), abs(position["y"])) <= 1:
                break
            direction = (("east" if position["x"] > 0 else "west") if position["x"] else
                         ("south" if position["y"] > 0 else "north"))
            current = self.execute(player, {"type": "move", "direction": direction})
        else:
            self.fail("Carrier was not reached within eight ordinary steps")
        current = self.execute(player, {"type": "wait"})
        before = checkpoint(self, player)
        self.assertEqual(combat(current)["hp"], health, "Passive newborn must not attack on sight or approach")
        self.assertIsNone(before["actors"][str(actor_id)]["behavior"]["engagement"])
        creature = actor_creature(before, actor_id)
        recipe = encounter_sources()["actors"]["rogue." + species]["creature"]
        self.assertEqual(creature["build"]["species"], "rogue." + species)
        self.assertEqual(len(creature["build"]["hd"]), 4)
        sources = [{"kind": "racial"} if row["source"] == "racial" else
                   {"kind": "class", "id": row["source"]} for row in recipe["hit_dice"]]
        self.assertEqual([row["source"] for row in creature["build"]["hd"]], sources)
        self.assertEqual(creature["build"]["choices"], [
            {"purchases": row["purchases"], "attribute": row.get("attribute")} for row in recipe["hit_dice"]])
        self.assertEqual(before["items"][item_id]["location"], {"Carried": actor_id})
        self.assertEqual(before["items"][item_id]["quantity"], 1)
        start = len(player.transcript)
        current = self.execute(player, {"type": "use_ability", "id": "basic_melee",
            "invocation": {"kind": "actor_target", "target": target_id}})
        for _ in range(8):
            if self.assert_retaliation(player, start, target_id, strikes):
                break
            current = self.execute(player, {"type": "wait"})
        else:
            self.fail(f"Provoked {species} did not resolve its {strikes}-strike sequence")
        current = self.execute(player, {"type": "equip", "item": weapon, "slot": 0})
        for _ in range(24):
            target = self.target(current, species)
            if target is None:
                break
            current = self.execute(player, {"type": "use_ability", "id": "basic_melee",
                "invocation": {"kind": "actor_target", "target": target["id"]}})
        else:
            self.fail("Carrier survived 24 ordinary finishing attacks")
        killed = checkpoint(self, player)
        dead = actor_creature(killed, actor_id)
        self.assertTrue(dead["dead"])
        self.assertEqual(dead["build"], creature["build"], "Combat preserves owned rows, purchases and Health seeds")
        self.assertGreater(actor_creature(killed, 1)["earned_progress"]["earned_xp"], 0)
        self.assertIn("Ground", killed["items"][item_id]["location"])
        self.assertEqual(killed["items"][item_id]["quantity"], 1)
        self.assertEqual(killed["items"][item_id]["spec"], before["items"][item_id]["spec"])
        self.assertEqual(self.private_receipts(), receipts, "Ordinary death cannot reroll birth stock")
        before_restart = self.request(player, {"type": "snapshot"})
        player.stop(); server.stop()
        shutil.rmtree(package)
        server = self.server("--checkpoint-interval", 1, scenario=unrelated_fallback(self), seed=None)
        player, restored = self.client()
        self.assertEqual(restored["state"], before_restart["state"])
        self.assertEqual(restored["history"], before_restart["history"])
        self.assertEqual(checkpoint(self, player), killed)
        self.assertEqual(self.private_receipts(), receipts)
        player.stop(); server.stop()

    def test_generated_centaur_retaliates_and_retains_birth_stock_through_death_and_restart(self):
        self.generated_carrier("centaur", 2, 7, 3, "rogue.ration")

    def test_generated_yeti_retaliates_and_retains_birth_stock_through_death_and_restart(self):
        self.generated_carrier("yeti", 0, 11, 2, "rogue.clean_bandage")


if __name__ == "__main__":
    unittest.main()
