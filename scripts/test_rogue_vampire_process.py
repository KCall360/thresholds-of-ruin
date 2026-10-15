"""Vampire mechanism acceptance through the real server and clients.

Actual Warrior/Mage kits face an authored controlled opponent for DR1/RG1.
A separate source-authored HD16 arena build tests a generated autonomous V's
death/drop identity. It is not a starter or earned-route survival result.
No newborn NPC controls, wizard grants, runtime sheet edits or AI tuning are used.
"""
from scalar_wire import u64
import json
import shutil
import sqlite3
import unittest

from checkpoint_payload import decode_checkpoint_payload
from process_harness import ProcessTestCase, SPECTATOR_TOKEN, settle_rogue_action
from process_work_budget import ProcessWorkBudget
from rogue_earned_route import first_purchase
from rogue_roster_package import unrelated_fallback
from rogue_test_support import disclosed_actor_candidates
from rogue_vampire_package import DRAIN, generated_carrier_package, vampire_package
from status_condition_package import actor_creature, checkpoint, combat, observation


class RogueVampireProcesses(ProcessTestCase):
    def setUp(self):
        super().setUp()
        # All ordinary requests, both participants' observed active work and
        # their complete stdout are bounded independently of nested loop limits.
        self.work_budget = ProcessWorkBudget(requests=1200, actor_ticks=20000,
                                             transcript_bytes=32 * 1024 * 1024)
        self.resumed_intentions = []

    def tearDown(self):
        report = self.work_budget.report()
        (self.directory / "work-budget.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("Process budget " + self.id() + ": " + json.dumps(report, sort_keys=True), flush=True)
        if not self._failed():
            self.work_budget.check()
        super().tearDown()

    def client(self, *args, **kwargs):
        self.work_budget.check()
        kwargs["omit_memory"] = True  # This suite never asserts accumulated map memory.
        client, frame = super().client(*args, **kwargs)
        self.work_budget.track(client, frame)
        return client, frame

    def server(self, *args, **kwargs):
        self.work_budget.check()
        return super().server(*args, **kwargs)

    def command(self, client, value):
        self.work_budget.before()
        result = super().command(client, value)
        self.work_budget.after(client, result)
        return result

    def frame(self, client, predicate, seconds=15):
        return self.work_budget.frame_wait(super().frame, client, predicate, seconds=seconds)

    def snapshot(self, client):
        return self.request(client, {"type": "snapshot"})

    def flush_save(self, *, actor=None):
        self.work_budget.before()
        super().flush_save(actor=actor)
        self.work_budget.check()

    def saved_intentions(self):
        """Resolve only the current checkpoint's owned work, not past entries."""
        with sqlite3.connect(self.save) as database:
            payload = database.execute("SELECT payload FROM checkpoint").fetchone()[0]
        document = json.loads(decode_checkpoint_payload(payload))
        refs = document["game"]["intentions"]
        return {"next_id": refs["next_id"], "entries": [
            document["shared"]["intentions"]["entries"][index] for index in refs["entries"]]}

    def finish(self, subject, peer, current=None):
        """Advance the explicitly authored peer using ordinary waits only."""
        def progress(frame):
            return ((frame.get("state") or {}).get("revision"), frame.get("readiness"),
                    frame.get("intentions"), frame.get("error"))

        current = self.snapshot(subject) if current is None else current
        for _ in range(40):
            self.assertIsNone(current["error"])
            if combat(current)["dead"] or combat(current)["terminal"]:
                return current
            if (observation(current)["ready"] and not current["intentions"]
                    and combat(current)["preparation_remaining"] is None
                    and int(combat(current)["recovery_remaining"]) == 0):
                return current
            resumable = [row for row in current["intentions"] if row["phase"] in ("paused", "suspended")
                         and row["intention"] in current["readiness"]["resume"]]
            if resumable:
                self.assertEqual(len(resumable), 1)
                resumed = self.command(subject, {"type": "resume_intention"})
                self.assertIsNone(resumed["error"])
                self.resumed_intentions.append(resumable[0]["intention"])
                if progress(resumed) != progress(current):
                    current = resumed
                    continue
            other = self.snapshot(peer)
            if not other["intentions"] and not combat(other)["dead"]:
                self.assertIsNone(self.command(peer, {"type": "act", "action": {"type": "wait"}})["error"])
            # The peer may have queued work behind the subject's next input
            # boundary, so waiting for the peer to finish could deadlock. Wait
            # for subject lifecycle/permission progress instead of polling.
            prior_tick = observation(current)["tick"]

            def advanced(frame):
                if frame.get("error"):
                    return True
                if frame.get("state") is None:
                    return False
                state = combat(frame)
                settled = (observation(frame)["ready"] and not frame["intentions"]
                           and state["preparation_remaining"] is None
                           and int(state["recovery_remaining"]) == 0)
                resume = any(row["intention"] in frame["readiness"]["resume"]
                             for row in frame["intentions"])
                return (observation(frame)["tick"] != prior_tick or settled or resume
                        or state["dead"] or state["terminal"])

            current = self.frame(subject, advanced)
        self.fail(f"Controlled ordinary action did not settle: {current['intentions']}")

    def execute(self, subject, peer, action, *, current=None):
        # Reuse a caller's consumed frame, never an inferred private state.
        self.finish(subject, peer, current)
        if action["type"] == "use_ability":
            action = {**action, "supplies": {"kind": "carried"}}
        accepted = self.command(subject, {"type": "act", "action": action})
        self.assertIsNone(accepted["error"])
        return self.finish(subject, peer, accepted)

    def buy_first(self, player, peer, character):
        identity = "rattling_blow" if character == 1 else "spark_lance_cold"
        before = self.snapshot(player)
        opened = self.command(player, {"type": "advancement", "command": "overview"})
        self.assertEqual(opened["advancement_page"]["entries"][0]["value"]["points"],
                         {"earned": "1", "spent": "0", "unspent": "1"})
        preview = self.command(player, {"type": "advancement", "command": first_purchase(character)})
        self.assertIsNone(preview["error"])
        self.assertEqual(preview["state"], before["state"], "A purchase preview cannot grant progression")
        self.assertIsNone(self.command(player, {"type": "advancement", "command": "confirm"})["error"])
        current = self.finish(player, peer)
        opened = self.command(player, {"type": "advancement", "command": "overview"})
        self.assertEqual(opened["advancement_page"]["entries"][0]["value"]["points"],
                         {"earned": "1", "spent": "1", "unspent": "0"})
        self.assertIn(identity, [row["id"] for row in combat(current)["own_stats"]["abilities"]])
        return identity

    def attack(self, player, peer, identity, name, *, basic_fallback=False, record_cost=False):
        frame = self.finish(player, peer)
        known = next(row for row in combat(frame)["own_stats"]["abilities"] if row["id"] == identity)
        for _ in range(40):
            cost = known["cost"]
            pools = {row["resource"]["id"]: row["available"]
                     for row in combat(frame)["own_stats"]["resources"]}
            if not cost or pools[cost["resource"]["id"]] >= cost["start"] + cost["resolution"]:
                break
            if basic_fallback:
                identity = "basic_melee"
                break
            frame = self.execute(player, peer, {"type": "wait"}, current=frame)
        else:
            self.fail("The actual starting kit could not recover the disclosed technique cost")
        target = next(row for row in disclosed_actor_candidates(observation(frame)["visible_actors"])
                      if row["name"] == name)
        start = len(player.transcript)
        result = self.execute(player, peer, {"type": "use_ability", "id": identity,
                              "invocation": {"kind": "actor_target", "target": target["id"]}}, current=frame)
        if record_cost:
            self.assert_paid_work(player, start, frame, result, known)
        return result

    def assert_paid_work(self, player, start, frame, result, known):
        cost = known["cost"]
        identity = known["id"]
        self.assertIsNotNone(cost, "The ordinary first technique has a disclosed resource cost")
        resource = cost["resource"]["id"]
        before = next(row for row in combat(frame)["own_stats"]["resources"]
                      if row["resource"]["id"] == resource)
        balances = []
        for line in player.transcript[start:]:
            if not line.startswith("{"):
                continue
            sample = json.loads(line)
            if sample.get("state") is not None:
                pool = next(row for row in combat(sample)["own_stats"]["resources"]
                            if row["resource"]["id"] == resource)
                balances.append(pool["balance"])
        witness = {"ability": identity, "resource": resource, "cost": cost,
                   "before_balance": before["balance"], "minimum_balance": min(balances),
                   "elapsed_ticks": int(observation(result)["tick"]) - int(observation(frame)["tick"])}
        print("Paid first-technique witness: " + json.dumps(witness, sort_keys=True), flush=True)
        self.assertGreater(witness["elapsed_ticks"], 0, "An attack must perform ordinary paid work")
        self.assertLessEqual(witness["minimum_balance"], before["balance"] - cost["start"],
                             "A held reservation alone is not evidence of a paid resource debit")

    def test_actual_warrior_and_mage_start_with_exact_vampire_owned_signature(self):
        for character in (1, 2):
            with self.subTest(character=character):
                package = vampire_package(self, character=character)
                self.save = self.directory / f"kit-{character}.db"
                server = self.server("--checkpoint-interval", 1, scenario=package,
                                     character=character, seed=42)
                player, initial = self.client(actor=character, omit_memory=True)
                vampire, foe = self.client(actor=3, omit_memory=True)
                self.assertFalse(initial["state"]["wizard_game"])
                self.assertEqual(u64(combat(initial)["own_stats"]["hit_dice"]["total"]), 1)
                self.assertEqual(u64(combat(foe)["own_stats"]["hit_dice"]["total"]), 8)
                self.assertIn(DRAIN, [row["id"] for row in combat(foe)["own_stats"]["abilities"]])
                self.assertEqual(combat(foe)["own_stats"]["nutrition"]["kind"], "exempt")
                self.assertEqual(combat(foe)["own_stats"]["health_recovery"]["period_ticks"], "460")
                self.assertIn("vampire", [row["name"] for row in observation(initial)["visible_actors"]])
                self.assertEqual({row["name"]: int(row["quantity"]) for row in observation(initial)["inventory"]},
                                 {"mace": 1, "ring mail": 1, "clean bandage": 3, "food ration": 1})
                names = {row["id"]: row["name"] for row in observation(initial)["inventory"]}
                self.assertEqual({names[row["item"]]: row["slot"]["id"]
                                  for row in observation(initial)["interactions"]["inventory"]
                                  if row["equipped_slot"] is not None},
                                 {"mace": "weapon", "ring mail": "body_armor"})
                self.buy_first(player, vampire, character)
                vampire.stop(); player.stop(); server.stop()

    def test_signature_miss_and_drain_keep_exact_deficit_and_injury_after_source_free_restart(self):
        package = vampire_package(self, character=1)
        outcomes = []
        witness = None
        # Declared mechanism samples, not a survival estimate or replacement
        # for the separate fixed ordinary earned-route cohort.
        for seed in range(16):
            self.save = self.directory / f"drain-{seed}.db"
            server = self.server("--checkpoint-interval", 1, scenario=package, seed=seed)
            player, initial = self.client(actor=1, omit_memory=True)
            vampire, _ = self.client(actor=3, omit_memory=True)
            self.buy_first(player, vampire, 1)
            start = len(vampire.transcript)
            prior = self.snapshot(vampire)
            self.attack(vampire, player, DRAIN, "Warrior")
            drained = self.snapshot(player)
            saved = checkpoint(self, player)
            subject = actor_creature(saved)
            deficit = combat(drained)["own_stats"]["restorable_losses"]["maximum_health"]
            events = [event for line in vampire.transcript[start:] if line.startswith("{")
                      for frame in [json.loads(line)] if frame.get("state") is not None
                      and int(frame["state"]["revision"]) > int(prior["state"]["revision"])
                      for event in combat(frame)["events"] if event["type"] == "ability"
                      and event["ability"]["id"] == DRAIN]
            self.assertTrue(events, "A real resolved DR1 event must be disclosed")
            self.assertEqual({event["strike"] for event in events}, {0}, "Natural damage occurs once")
            self.assertIn(deficit, range(4))
            self.assertEqual(subject["restorable_losses"]["maximum_health"], deficit)
            self.assertEqual(u64(combat(drained)["max_hp"]), u64(combat(initial)["max_hp"]) - deficit)
            outcome = {"seed": seed, "primary": events[-1]["outcome"], "deficit": deficit,
                       "dead": combat(drained)["dead"], "injury": subject["injury"]}
            outcomes.append(outcome)
            if outcome["primary"] == "miss":
                self.assertEqual(deficit, 0)
                self.assertEqual(subject["injury"], 0)
            if not outcome["dead"]:
                self.assertEqual(u64(combat(drained)["max_hp"]) - u64(combat(drained)["hp"]), subject["injury"])
            miss = any(row["primary"] == "miss" for row in outcomes)
            failed_rider = any(row["primary"] == "applied" and row["deficit"] == 0
                               and row["injury"] > 0 and not row["dead"] for row in outcomes)
            if deficit and not outcome["dead"] and miss and failed_rider:
                witness = (server, player, vampire, drained, saved)
                break
            vampire.stop(); player.stop(); server.stop()
        print("Vampire drain mechanism samples: " + json.dumps(outcomes, sort_keys=True), flush=True)
        self.assertIsNotNone(witness,
            f"Declared samples need primary miss, failed Spiritual rider and surviving drain: {outcomes}")
        server, player, vampire, before, saved = witness
        self.assertGreater(actor_creature(saved)["injury"], 0)
        spectator, shown = self.client(SPECTATOR_TOKEN, actor=1, omit_memory=True)
        self.assertEqual(combat(shown)["own_stats"]["restorable_losses"],
                         combat(before)["own_stats"]["restorable_losses"])
        spectator.stop()
        vampire.stop(); player.stop(); server.stop()
        shutil.rmtree(package)
        self.server("--checkpoint-interval", 1, scenario=unrelated_fallback(self), seed=None)
        player, reopened = self.client(actor=1, omit_memory=True)
        self.assertEqual(reopened["state"], before["state"])
        self.assertEqual(checkpoint(self, player), saved)

    def test_separate_unchanged_emu_carrier_drops_one_finite_ration_for_ordinary_take_and_eat(self):
        # Initial carried inventory isolates the ordinary identity pipeline.
        # It is explicitly distinct from generated birth-stock receipt tests.
        # A full-Health V survived the bounded real-kit probe; do not tune its
        # sheet or cost rules to rescue it. This unchanged one-HD emu proves
        # only the generic finite-item identity path, not a Vampire death.
        package = vampire_package(self, character=2, carried_ration=True, opponent="emu")
        self.server("--checkpoint-interval", 1, scenario=package, character=2, seed=42)
        player, initial = self.client(actor=2, omit_memory=True)
        carrier, carrier_initial = self.client(actor=3, omit_memory=True)
        self.assertEqual(u64(combat(carrier_initial)["own_stats"]["hit_dice"]["total"]), 1)
        identity = self.buy_first(player, carrier, 2)
        own_ration = next(row["id"] for row in observation(initial)["inventory"]
                          if row["name"] == "food ration")
        ate_starter = self.execute(player, carrier, {"type": "eat", "item": own_ration})
        self.assertFalse(any(row["name"] == "food ration" for row in observation(ate_starter)["inventory"]))
        before = checkpoint(self, player)
        self.assertEqual(before["items"]["1000"]["location"], {"Carried": 3})
        self.assertEqual(before["items"]["1000"]["quantity"], 1)
        # Ordinary finite attacks, with no waiting for mana or enemy death.
        for _ in range(16):
            current = self.attack(player, carrier, identity, "emu", basic_fallback=True)
            if combat(self.snapshot(carrier))["dead"]:
                break
        else:
            self.fail("The separate unchanged emu survived the bounded actual-kit sequence")
        self.assertFalse(combat(current)["dead"])
        killed = checkpoint(self, player)
        growth = actor_creature(killed, 2)["earned_progress"]
        self.assertIn("Ground", killed["items"]["1000"]["location"])
        self.assertEqual(killed["items"]["1000"]["quantity"], 1)
        self.assertEqual(killed["items"]["1000"]["spec"], before["items"]["1000"]["spec"])
        self.assertGreater(growth["earned_xp"], 0, "Death awards ordinary kill XP")
        self.assertEqual(actor_creature(killed, 2)["build"]["hd"][:1],
                         actor_creature(before, 2)["build"]["hd"], "The actual starter row remains owned")
        self.assertEqual(len([row for row in observation(current)["ground_items"]
                             if row["item"]["name"] == "food ration"]), 1)
        # The target's known adjacent ground cell, never its private save ID,
        # selects the ordinary step and item handle.
        stock = next(row for row in observation(current)["ground_items"] if row["item"]["name"] == "food ration")
        self.assertEqual(stock["position"], {"x": 1, "y": 0, "z": 0})
        current = self.act(player, {"type": "move", "direction": "east"})
        self.assertIsNone(current["error"])
        stock = next(row for row in observation(current)["ground_items"] if row["item"]["name"] == "food ration")
        current = self.act(player, {"type": "take", "item": stock["item"]["id"], "quantity": None})
        self.assertIsNone(current["error"])
        carried = [row for row in observation(current)["inventory"] if row["name"] == "food ration"]
        self.assertEqual(sum(int(row["quantity"]) for row in carried), 1)
        taken = checkpoint(self, player)
        self.assertEqual(taken["items"]["1000"]["location"], {"Carried": 2})
        self.assertEqual(taken["items"]["1000"]["spec"], before["items"]["1000"]["spec"])
        consumed = self.act(player, {"type": "eat", "item": carried[0]["id"]})
        self.assertIsNone(consumed["error"])
        if consumed["intentions"]:
            consumed = self.frame(player, lambda frame: not frame["intentions"])
        self.assertEqual(sum(int(row["quantity"]) for row in observation(consumed)["inventory"]
                             if row["name"] == "food ration"), 0)
        final = checkpoint(self, player)
        self.assertNotIn("1000", final["items"], "The same isolated dropped stack was consumed")
        self.assertEqual(actor_creature(final, 2)["earned_progress"], growth,
                         "Death/drop/take/consume cannot award the same kill twice")
        self.assertEqual({row["name"]: int(row["quantity"]) for row in observation(initial)["inventory"]},
                         {"mace": 1, "ring mail": 1, "clean bandage": 3, "food ration": 1})

    def test_generated_vampire_stock_survives_ordinary_death_and_source_free_restart(self):
        # Explicit HD16 arena recipe: not an earned or actual-starter build.
        # The newborn remains autonomous under the unchanged rogue_hostile AI.
        package = generated_carrier_package(self)
        samples = []
        witness = None
        for seed in range(8):
            self.save = self.directory / f"generated-carrier-{seed}.db"
            server = self.server("--checkpoint-interval", 1, scenario=package, seed=seed)
            player, current = self.client(actor=1)
            self.assertFalse(current["state"]["wizard_game"])
            self.assertEqual(u64(combat(current)["own_stats"]["hit_dice"]["total"]), 16)
            receipts = self.private_receipts(actor=1)
            attempts = receipts[0]["decisions"][0]["result"]["attempts"]
            self.assertEqual(len(attempts), 1)
            self.assertEqual(attempts[0]["kind"], "placed")
            stock = attempts[0]["entity"]["carried_loot"]
            samples.append({"seed": seed, "stock": stock["kind"]})
            if stock["kind"] == "carried":
                witness = (server, player, current, receipts, attempts[0])
                break
            player.stop(); server.stop()
        print("Generated V death fixture birth samples: " + json.dumps(samples), flush=True)
        self.assertIsNotNone(witness, "Declared seeds need an ordinary 1/5 birth-stock witness")
        server, player, current, receipts, born = witness
        stock = born["entity"]["carried_loot"]
        actor_id, item_id = born["entity"]["id"], str(stock["item"])
        # This ordinary wait makes a durable checkpoint; private IDs are used
        # solely for identity assertions, never to choose an action or target.
        current = settle_rogue_action(self, player, self.act(player, {"type": "wait"}))
        before = checkpoint(self, player)
        self.assertEqual(actor_creature(before, actor_id)["build"]["species"], "rogue.vampire")
        self.assertEqual(len(actor_creature(before, actor_id)["build"]["hd"]), 9)
        self.assertEqual(before["items"][item_id]["location"], {"Carried": actor_id})
        self.assertEqual(before["items"][item_id]["quantity"], 1)
        paid_actions = 0
        paid_witness = False
        for step in range(24):
            foes = [row for row in disclosed_actor_candidates(observation(current)["visible_actors"])
                    if row["name"] == "vampire"]
            if not foes:
                break
            target = foes[0]
            position = target["position"]
            paid = False
            if max(abs(position["x"]), abs(position["y"])) > 1:
                direction = (("east" if position["x"] > 0 else "west") if position["x"] else
                             ("south" if position["y"] > 0 else "north"))
                action = {"type": "move", "direction": direction}
            else:
                technique = next(row for row in combat(current)["own_stats"]["abilities"]
                                 if row["id"] == "rattling_blow")
                stamina = next(row for row in combat(current)["own_stats"]["resources"]
                               if row["resource"]["id"] == "stamina")
                cost = technique["cost"]
                paid = stamina["available"] >= cost["start"] + cost["resolution"]
                paid_actions += int(paid)
                action = {"type": "use_ability", "id": "rattling_blow" if paid else "basic_melee",
                          "invocation": {"kind": "actor_target", "target": target["id"]}}
            previous_tick = int(observation(current)["tick"])
            previous, transcript_start = current, len(player.transcript)
            current = settle_rogue_action(self, player, self.act(player, action))
            self.assertIsNone(current["error"])
            self.assertGreater(int(observation(current)["tick"]), previous_tick,
                               "Each mechanism action must perform ordinary active work")
            if paid and not paid_witness:
                self.assert_paid_work(player, transcript_start, previous, current, technique)
                paid_witness = True
        else:
            self.fail("Generated Vampire survived the bounded authored-HD16 mechanism")
        self.assertGreater(paid_actions, 0)
        self.assertTrue(paid_witness)
        killed = checkpoint(self, player)
        self.assertTrue(actor_creature(killed, actor_id)["dead"])
        self.assertGreater(actor_creature(killed, 1)["earned_progress"]["earned_xp"], 0)
        self.assertIn("Ground", killed["items"][item_id]["location"])
        self.assertEqual(killed["items"][item_id]["quantity"], stock["quantity"])
        self.assertEqual(killed["items"][item_id]["spec"], before["items"][item_id]["spec"])
        self.assertEqual(self.private_receipts(actor=1), receipts, "Death cannot reroll birth stock")
        before_restart = self.snapshot(player)
        print("Generated V ordinary death witness: " + json.dumps({
            "recipe": "warrior_rank_heavy/16", "actions": step, "paid_techniques": paid_actions,
            "born_actor": actor_id, "item": item_id, "content": stock["content"],
            "quantity": stock["quantity"]}, sort_keys=True), flush=True)
        player.stop(); server.stop()
        shutil.rmtree(package)
        self.server("--checkpoint-interval", 1, scenario=unrelated_fallback(self), seed=None)
        player, restored = self.client(actor=1)
        self.assertEqual(restored["state"], before_restart["state"])
        self.assertEqual(checkpoint(self, player), killed)
        self.assertEqual(self.private_receipts(actor=1), receipts)

    def first_technique_recovery(self, character):
        # The exact first +1 boundary is sufficient here; shared recovery tests
        # cover full Health/reset. A retained full-heal probe exceeded its fixed
        # output cap, so do not extend this test by raising the cap or tuning V.
        package = vampire_package(self, character=character)
        self.save = self.directory / f"recovery-{character}.db"
        server = self.server("--checkpoint-interval", 1, scenario=package,
                             character=character, seed=42)
        player, initial = self.client(actor=character, omit_memory=True)
        vampire, _ = self.client(actor=3, omit_memory=True)
        identity = self.buy_first(player, vampire, character)
        if character == 2:
            self.execute(player, vampire, {"type": "move", "direction": "west"})
        for attempt in range(24):
            self.attack(player, vampire, identity, "vampire", record_cost=attempt == 0)
            injured = self.snapshot(vampire)
            if u64(combat(injured)["hp"]) < u64(combat(injured)["max_hp"]):
                break
        else:
            self.fail(f"Actual character {character}'s first technique did not cause injury")
        self.assertFalse(combat(injured)["dead"])
        recovery = combat(injured)["own_stats"]["health_recovery"]
        self.assertEqual(recovery["period_ticks"], "460")
        self.assertFalse(recovery["stopped"])
        # Queries preserve partial work. These actors stay in the same
        # active chamber; no global-time/frozen-time claim is inferred.
        saved = checkpoint(self, player)
        before = self.snapshot(vampire)
        spectator, seen = self.client(SPECTATOR_TOKEN, actor=3, omit_memory=True)
        self.assertEqual(combat(seen), combat(before))
        self.assertFalse(seen["has_control"])
        spectator.stop()
        self.assertEqual(checkpoint(self, player), saved)
        paid_work = self.saved_intentions()
        # Stop authority first. Disconnecting a controller while authority is
        # live can legitimately suspend its queued wait and change this prefix.
        server.stop(); vampire.stop(); player.stop()
        shutil.rmtree(package)
        fallback = self.directory / "unrelated-fallback"
        if not fallback.exists():
            fallback = unrelated_fallback(self)
        server = self.server("--checkpoint-interval", 1, scenario=fallback,
                             seed=None, save_actor=character)
        player, _ = self.client(actor=character, omit_memory=True)
        vampire, reopened = self.client(actor=3, omit_memory=True)
        self.assertEqual(reopened["state"], before["state"])
        self.assertEqual(checkpoint(self, player), saved)
        # Restored human queues suspend until their controller explicitly
        # resumes the same work. Only that lifecycle state may differ.
        restored_work = self.saved_intentions()
        expected_work = {"next_id": paid_work["next_id"], "entries": []}
        for entry in paid_work["entries"]:
            self.assertEqual(entry["actor"], 3)
            self.assertEqual(entry["work"], {"kind": "action", "action": {"type": "wait"}})
            self.assertIn(entry["state"], ("queued", "suspended"))
            expected_work["entries"].append({**entry, "state": "suspended"})
        self.assertEqual(restored_work, expected_work,
                         "Restore must preserve owned intention IDs, operands and queued prefixes")
        self.assertEqual(reopened["intentions"], [dict(row, phase="suspended")
                                                  for row in before["intentions"]])
        print("Restored peer work witness: " + json.dumps({
            "character": character, "public_intentions": reopened["intentions"],
            "before": paid_work, "restored": restored_work}, sort_keys=True), flush=True)
        resume_start = len(self.resumed_intentions)
        start = int(observation(reopened)["tick"])
        progress = int(combat(reopened)["own_stats"]["health_recovery"]["elapsed_ticks"])
        hp, maximum = u64(combat(reopened)["hp"]), u64(combat(reopened)["max_hp"])
        recovered = reopened
        for _ in range(6):
            recovered = self.execute(vampire, player, {"type": "wait"}, current=recovered)
            elapsed = int(observation(recovered)["tick"]) - start
            self.assertEqual(u64(combat(recovered)["hp"]), min(maximum, hp + (progress + elapsed) // 460))
            if u64(combat(recovered)["hp"]) > hp:
                break
        self.assertEqual(u64(combat(recovered)["hp"]), hp + 1)
        expected_progress = 0 if hp + 1 == maximum else (progress + elapsed) % 460
        self.assertEqual(int(combat(recovered)["own_stats"]["health_recovery"]["elapsed_ticks"]),
                         expected_progress)
        print("First natural regeneration witness: " + json.dumps({
            "character": character, "period": 460, "start_hp": hp, "final_hp": hp + 1,
            "start_progress": progress, "elapsed_ticks": elapsed,
            "final_progress": expected_progress}, sort_keys=True), flush=True)
        self.assertEqual(u64(combat(self.snapshot(player))["max_hp"]), u64(combat(initial)["max_hp"]))
        self.assertTrue({row["intention"] for row in reopened["intentions"]}.issubset(
            set(self.resumed_intentions[resume_start:])), "Continue the same restored wait, never replace it")
        vampire.stop(); player.stop(); server.stop()

    def test_warrior_first_technique_and_first_regeneration_boundary_survive_source_free_restart(self):
        self.first_technique_recovery(1)

    def test_mage_first_technique_and_first_regeneration_boundary_survive_source_free_restart(self):
        self.first_technique_recovery(2)


if __name__ == "__main__":
    unittest.main()
