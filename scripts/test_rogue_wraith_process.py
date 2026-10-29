"""Autonomous paid-row loss and source-free reopen, not full W recovery survival.

W/emu recipes, production rules, Warrior kit and autonomous AI stay unchanged.
Eight ordinary emu kills precede one survived drain; stop before another attack.
advancement_session_tests owns the controlled ordinary-kill regrowth composition.
Generic hit_die_effect_tests and abilities/restorable_loss_tests own fatality,
restoration ceilings, injury preservation, rollback and replayed paid work;
test_restorable_hit_die_process owns unchanged Text/native loss disclosure.
"""
import json
import shutil
import unittest

from process_harness import ProcessTestCase, ROOT, settle_rogue_action, underfoot
from process_work_budget import ProcessBudgetExceeded, ProcessWorkBudget
from rogue_test_support import (cardinal_melee_candidate, disclosed_actor_candidates,
                                disclosed_contact_step, disclosed_frames, ordered_combat_events)
from rogue_wraith_package import disclosed_corridor_step, drain_interval, recovery_package
from scalar_wire import u64
from status_condition_package import actor_creature, checkpoint, combat, observation

DRAIN = "rogue.energy_drain"
DISK_LIMIT = 128 * 1024 * 1024
EMPTY = {"purchase_highwater": 0, "purchases": [], "attribute": None}
RATTLING = {"pool": "advancement", "selection": {"kind": "talent", "talent": "rattling_blow"}}
LORE = {"pool": "advancement", "selection": {"kind": "skill_rank", "skill": "lore"}}


def owned_rows(build, kind):
    return [{"hit_die": row, "choices": build["choices"][index]}
            for index, row in enumerate(build["hd"]) if row["owner"]["kind"] == kind]


def paid_choice(owner, purchase):
    return {"purchase_highwater": 1, "purchases": [dict(purchase, id={"owner": owner, "serial": 1})],
            "attribute": None}


def survived_personal_drain(frame):
    # Public deficit, not a fixed total-HD stop or a private saved build.
    return u64(stats(frame)["restorable_losses"]["hit_dice"]) == 1


def stats(frame):
    return combat(frame)["own_stats"]


def quantity(frame, name):
    return sum(int(row["quantity"]) for row in observation(frame)["inventory"] if row["name"] == name)


def lore(frame):
    return next(row["rank"] for row in stats(frame)["skills"] if row["skill"]["id"] == "lore")


def events_since(player, start):
    return list(ordered_combat_events(disclosed_frames(player, start)))


class RogueWraithProcesses(ProcessTestCase):
    # Retained seed0 reached the survived drain. This narrower source-free
    # prefix still requires its own qualification; never search seeds in CI.
    # Rebind source/binary and constructor seeds after integration; the same
    # numeric seed does not promise byte-identical outcomes for changed inputs.
    seed = 0

    def setUp(self):
        super().setUp()
        self.work_budget = ProcessWorkBudget(requests=800, actor_ticks=40000,
                                             transcript_bytes=96 * 1024 * 1024)
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
        print("W/drain-prefix budget: " + json.dumps(report, sort_keys=True), flush=True)
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

    def overview(self, player):
        current = self.command(player, {"type": "advancement", "command": "overview"})
        self.assertIsNone(current["error"])
        return current, current["advancement_page"]["entries"][0]["value"]

    def assert_progress(self, saved, earned, spent, hd):
        creature = actor_creature(saved)
        progress = creature["earned_progress"]
        self.assertEqual({key: progress[key] for key in (
            "authored_baseline", "highest_applied_tier", "highest_earned_tier",
            "earned_xp", "spent_xp", "held_xp")}, {
            "authored_baseline": 2, "highest_applied_tier": 2, "highest_earned_tier": 2,
            "earned_xp": earned, "spent_xp": spent, "held_xp": earned - spent})
        self.assertEqual(len(creature["build"]["hd"]), hd)
        self.assertEqual(len(owned_rows(creature["build"], "body")), 1)
        self.assertEqual(len(owned_rows(creature["build"], "personal")), hd - 1)
        self.assertFalse(creature["dead"])

    def audit(self, player, current, events=()):
        # Private facts only assert the already-selected public action's result.
        # They never choose movement, targeting, recovery or the public-HD stop.
        saved = checkpoint(self, player)
        dead = {actor for actor in range(3, 13) if actor_creature(saved, actor)["dead"]}
        new = dead - self.dead
        self.assertEqual(len(new), len([event for event in events if event["type"] == "died"]),
                         "Every new death needs its public transition")
        prior = actor_creature(self.saved)["earned_progress"] if self.saved else None
        earned = 20 * len(dead - {11}) + 35 * int(11 in dead)
        spent = 200 if 12 in dead else 0
        if prior:
            self.assertEqual(earned - prior["earned_xp"],
                             sum(35 if actor == 11 else 20 for actor in new))
        self.assert_progress(saved, earned, spent, u64(stats(current)["hit_dice"]["total"]))
        if self.paid is not None:
            build = actor_creature(saved)["build"]
            self.assertIsNone(build["health_root_seed"])
            self.assertEqual(build["body_build"]["health_root"], self.paid["body_build"]["health_root"])
            self.assertEqual(owned_rows(build, "body"), owned_rows(self.paid, "body"))
            self.assertEqual(owned_rows(build, "personal")[0], owned_rows(self.paid, "personal")[0])
            self.assertEqual(build["hd"][0], self.paid["hd"][0])
            self.assertEqual(build["choices"][0], self.paid["choices"][0])
            if len(owned_rows(build, "personal")) == 1 and self.drained is None:
                self.assertEqual(build["hd"], self.paid["hd"][:2])
                self.assertEqual(build["body_build"]["drained"], [owned_rows(self.paid, "personal")[1]])
                self.assertEqual(build["drained_hd"], [])
                self.assertEqual(build["choices"], self.paid["choices"][:2])
                self.assertEqual(lore(current), 0)
                self.assertEqual(u64(stats(current)["restorable_losses"]["hit_dice"]), 1)
                prior_creature = actor_creature(self.saved)
                boundary = drain_interval(events, wraith=self.wraith, victim=observation(current)["self_target"],
                    prior_earned=prior["earned_xp"], earned=earned,
                    prior_hd=len(owned_rows(prior_creature["build"], "personal")),
                    hd=len(owned_rows(build, "personal")))
                self.drained = saved
                self.sample["drain_boundary"] = boundary
                (self.directory / "survived-drain.json").write_text(json.dumps({
                    "boundary": boundary, "prior_build": prior_creature["build"], "prior_xp": prior,
                    "build": build, "earned_progress": actor_creature(saved)["earned_progress"],
                    "ordered_events": events}, indent=2))
        self.dead, self.saved = dead, saved
        self.sample.update(earned=earned, spent=spent, actual_hd=len(actor_creature(saved)["build"]["hd"]),
                           kills=len(dead), drained=self.drained is not None)
        return saved

    def action(self, player, current, action):
        start = len(player.transcript)
        current = self.execute(player, action, survive=False)
        self.assertFalse(combat(current)["dead"], "Bounded unchanged-kit attempt died; do not repair it")
        self.assertFalse(combat(current)["terminal"])
        events = events_since(player, start)
        self.audit(player, current, events)
        return current, events

    def purchase(self, player, current, ordinal, selection, expected, unspent):
        before = checkpoint(self, player)
        ration = quantity(current, "food ration")
        self.overview(player)
        preview = self.command(player, {"type": "advancement", "command":
            f"purchase {ordinal} advancement {selection}"})
        self.assertIsNone(preview["error"])
        self.assertEqual(checkpoint(self, player), before, "Preview cannot spend or mutate the ledger")
        confirmed = self.command(player, {"type": "advancement", "command": "confirm"})
        self.assertIsNone(confirmed["error"])
        if any(row["phase"] == "queued" for row in confirmed["intentions"]):
            confirmed = self.frame(player, lambda frame: not any(
                row["phase"] == "queued" for row in frame["intentions"]))
        current = settle_rogue_action(self, player, confirmed)
        current, page = self.overview(player)
        self.assertEqual(page["points"], {"earned": "2", "spent": str(2 - unspent), "unspent": str(unspent)})
        after = checkpoint(self, player)
        self.assertEqual(actor_creature(after)["build"]["choices"][ordinal - 1],
                         paid_choice(actor_creature(after)["build"]["hd"][ordinal - 1]["owner"], expected))
        self.assertEqual({key: value for key, value in actor_creature(after)["earned_progress"].items()
                          if key != "peak_threat"},
                         {key: value for key, value in actor_creature(before)["earned_progress"].items()
                          if key != "peak_threat"}, "A paid choice may raise threat, never XP")
        self.assertEqual(quantity(current, "food ration"), ration)
        self.saved = after
        return current

    def step(self, player, current, destination=None):
        origin = underfoot(current["state"])["key"]
        self.visited.add(origin)
        cell, direction = disclosed_corridor_step(current, self.visited, destination)
        if cell.get("door") and not cell["door"]["open"]:
            # Opening is paid ordinary work. Reconsider the fresh observation
            # before moving, since the newly disclosed creature can roam.
            self.assertTrue(cell["door"]["reachable"])
            return self.action(player, current, {"type": "set_door",
                "door": cell["door"]["id"], "open": True})
        current, events = self.action(player, current, {"type": "move", "direction": direction})
        self.assertEqual(underfoot(current["state"])["key"], cell["key"])
        self.visited.add(cell["key"])
        self.last_edge = (origin, cell["key"])
        return current, events

    def targets(self, current):
        seen = observation(current)
        return disclosed_actor_candidates(row for row in seen["visible_actors"]
                                          if row["id"] != seen["self_target"])

    def approach(self, player, current, name):
        while True:
            targets = self.targets(current)
            if targets:
                self.assertEqual(len(targets), 1, "The fixed bays must not create a simultaneous swarm")
                self.assertEqual(targets[0]["name"], name)
                if cardinal_melee_candidate(targets[0]):
                    return current, targets[0]["id"]
            current, _ = self.step(player, current)

    def recover(self, player, current):
        # Do not walk toward an unseen actor for recovery. The cleared cell is
        # the ordinary resting place; all time, food and finite care still count.
        self.sample["phase"] = "ordinary_care"
        while u64(combat(current)["hp"]) < u64(combat(current)["max_hp"]):
            self.assertFalse(self.targets(current), "Recovery bay has an observed opponent")
            care = next(row for row in stats(current)["abilities"] if row["id"] == "bind_wounds")
            injury = u64(combat(current)["max_hp"]) - u64(combat(current)["hp"])
            if int(stats(current)["nutrition"]["reserve"]) < 300:
                ration = next((row for row in observation(current)["inventory"] if row["name"] == "food ration"), None)
                self.assertIsNotNone(ration, "The unchanged finite ration supply is exhausted")
                action = {"type": "eat", "item": ration["id"]}
            elif injury >= 2 and quantity(current, "clean bandage") and int(care["cooldown_remaining"]) == 0:
                action = {"type": "use_ability", "id": "bind_wounds", "invocation": {"aim": {
                    "kind": "actor_target", "target": observation(current)["self_target"]}, "weapon": {"kind": "default"}}}
            else:
                action = {"type": "wait"}
            current, _ = self.action(player, current, action)
        self.check_disk()
        return current

    def fight(self, player, current, target, name):
        self.sample["phase"] = "fight_" + name
        while True:
            if name == "wraith" and survived_personal_drain(current):
                # Reapproach is paid too: a drain during that Move must stop
                # before another hostile player input, just like an attack.
                self.check_disk()
                return current
            seen = [row for row in self.targets(current) if row["id"] == target]
            self.assertEqual(len(seen), 1, "A lost target is not evidence of death")
            if not cardinal_melee_candidate(seen[0]):
                cell, _ = disclosed_contact_step(observation(current), seen[0])
                current, _ = self.step(player, current, cell["key"])
                continue
            technique = next(row for row in stats(current)["abilities"] if row["id"] == "rattling_blow")
            stamina = next(row for row in stats(current)["resources"] if row["resource"]["id"] == "stamina")
            cost = technique["cost"]
            paid = stamina["available"] >= cost["start"] + cost["resolution"]
            current, events = self.action(player, current, {"type": "use_ability",
                "id": "rattling_blow" if paid else "basic_melee",
                "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
            if name == "wraith" and survived_personal_drain(current):
                # action() already required a live actor and audited the real
                # contact/row loss. Choose this stop only from disclosed own HD.
                self.check_disk()
                return current
            if any(event["type"] == "died" and event["actor"] == target for event in events):
                self.check_disk()
                return current

    def test_autonomous_wraith_paid_latest_row_loss_survives_source_free_restart(self):
        self.dead, self.saved, self.paid, self.drained, self.wraith = set(), None, None, None, None
        self.visited, self.last_edge = set(), None
        package = recovery_package(self)
        server = self.server("--checkpoint-interval", 1, scenario=package, character=1, seed=self.seed)
        player, initial = self.client(actor=1)
        self.assertFalse(initial["state"]["wizard_game"])
        self.assertEqual({row["name"]: int(row["quantity"]) for row in observation(initial)["inventory"]},
                         {"mace": 1, "ring mail": 1, "clean bandage": 3, "food ration": 1})
        self.assertEqual(u64(stats(initial)["hit_dice"]["total"]), 3)
        current, _ = self.action(player, initial, {"type": "wait"})
        original = actor_creature(self.saved)["build"]
        self.assertEqual(original["choices"], [EMPTY, EMPTY, EMPTY])
        self.assertEqual(original["hd"][0]["owner"], {"kind": "body", "form": "rogue.body.human", "ordinal": 0})
        self.assertEqual([row["hit_die"]["owner"] for row in owned_rows(original, "personal")],
                         [{"kind": "personal", "id": 1}, {"kind": "personal", "id": 2}])
        self.assertEqual([row["health_policy"] for row in original["hd"]], ["maximized", "seeded", "seeded"])
        self.assertEqual(original["body_build"]["birth_personal_count"], 2)
        self.assertEqual(original["body_build"]["next_personal_id"], 3)
        self.assertEqual(original["body_build"]["drained"], [])
        self.assertEqual(original["restoration"], {"kind": "choose_again"})
        self.assertEqual([row["source"] for row in original["hd"]],
                         [{"kind": "racial"}] + [{"kind": "class", "id": "warrior"}] * 2)
        self.assertIsNone(original["health_root_seed"])
        self.assertIs(type(original["body_build"]["health_root"]), int)
        self.assertTrue(all(type(row["health_seed"]) is int for row in original["hd"]))
        wraith = actor_creature(self.saved, 11)
        self.assertEqual(len(wraith["build"]["hd"]), 5)
        self.assertEqual(wraith["build"]["choices"], [paid_choice(
            {"kind": "body", "form": "rogue.body.wraith", "ordinal": 0},
            {"pool": "advancement", "selection": {"kind": "talent", "talent": DRAIN}})] + [EMPTY] * 4)
        self.assertEqual(owned_rows(wraith["build"], "personal"), [])
        self.assertEqual([row["owner"] for row in wraith["build"]["hd"]], [
            {"kind": "body", "form": "rogue.body.wraith", "ordinal": index} for index in range(5)])
        self.assertEqual(wraith["earned_progress"]["peak_threat"], 4)
        self.assertTrue(all(actor_creature(self.saved, actor)["earned_progress"]["peak_threat"] == 1
                            for actor in (*range(3, 11), 12)))
        self.assertFalse(any(row["location"] == {"Carried": 11} for row in self.saved["items"].values()))
        self.assert_progress(self.saved, 0, 0, 3)
        current = self.purchase(player, current, 2, "talent rattling_blow", RATTLING, 1)
        current = self.purchase(player, current, 3, "skill lore", LORE, 0)
        self.paid = actor_creature(self.saved)["build"]
        self.assertEqual(self.paid["hd"], original["hd"])
        self.assertEqual(lore(current), 1)
        for ordinal in range(8):
            self.sample["phase"] = f"approach_emu_{ordinal + 1}"
            current, target = self.approach(player, current, "emu")
            current = self.fight(player, current, target, "emu")
            self.assert_progress(self.saved, (ordinal + 1) * 20, 0, 3)
            if ordinal == 0:
                before_retry = self.saved
                refused = self.act(player, {"type": "use_ability", "id": "basic_melee",
                    "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
                self.assertIsNotNone(refused["error"], "An observed dead target cannot be killed twice")
                self.assertEqual(checkpoint(self, player), before_retry)
                current = self.request(player, {"type": "snapshot"})
                self.audit(player, current)
            current = self.recover(player, current)
        self.sample["phase"] = "passive_wraith"
        current, self.wraith = self.approach(player, current, "wraith")
        start = len(player.transcript)
        passive = self.saved
        for _ in range(2):
            current, _ = self.action(player, current, {"type": "wait"})
        previous, here = self.last_edge
        self.assertEqual(here, underfoot(current["state"])["key"])
        current, _ = self.step(player, current, previous)
        return_cell, _ = disclosed_corridor_step(current, self.visited, here)
        occupants = [row for row in observation(current)["visible_actors"]
                     if row["position"] == return_cell["position"]]
        if occupants:
            # A passive roaming W may step into the vacated cell. Its current
            # public contact replaces a colliding return Move, never an attack.
            self.assertTrue(all(row["id"] == self.wraith for row in occupants))
            self.assertTrue(cardinal_melee_candidate(occupants[0]))
        else:
            current, _ = self.step(player, current, here)
        self.assertEqual(actor_creature(self.saved)["build"], actor_creature(passive)["build"])
        self.assertFalse(any(event["type"] == "ability" and event["caster"] == self.wraith
            for event in events_since(player, start)), "Passive W cannot attack before provocation")
        current = self.fight(player, current, self.wraith, "wraith")
        self.assertIsNotNone(self.drained, "W died without a survived successful drain; not an accepted witness")
        self.assert_progress(self.saved, 160, 0, 2)
        self.assertEqual(self.dead, set(range(3, 11)))
        self.assertFalse(actor_creature(self.saved, 11)["dead"])
        self.assertEqual(self.sample["drain_boundary"], "separate_160_checkpoint")
        self.assertEqual(u64(stats(current)["restorable_losses"]["hit_dice"]), 1)
        self.assertEqual(self.saved["items"], {key: value for key, value in self.saved["items"].items()
            if value["location"] != {"Carried": 11}}, "W has no carried birth stock")
        before = self.saved
        self.stop_case(server, player)
        shutil.rmtree(package)
        self.assertFalse(package.exists())
        self.sample["phase"] = "source_free_restart"
        server = self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        player, reopened = self.client(actor=1)
        self.assertDisclosedStateEqual(reopened["state"], current["state"])
        self.assertEqual(checkpoint(self, player), before)
        self.assert_progress(before, 160, 0, 2)
        self.assertEqual(lore(reopened), 0)
        self.assertEqual(u64(stats(reopened)["restorable_losses"]["hit_dice"]), 1)
        # No further gameplay input: the separate controlled Service case owns
        # W death,195 source-free replay,215/200 regrowth and ordinary repurchase.
        self.sample["phase"] = "passed"
        self.stop_case(server, player)


class RogueWraithTargetSelection(unittest.TestCase):
    def test_drain_during_contact_reapproach_stops_before_a_player_attack(self):
        from unittest.mock import patch
        driver = RogueWraithProcesses()
        driver.sample = {}
        initial = {"state": {"observation": {"self_target": "self",
            "visible_actors": [{"id": "foe", "position": {"x": 2, "y": 0, "z": 0}}],
            "visible_cells": [{"key": "next", "position": {"x": 1, "y": 0, "z": 0}, "wall": False}],
            "combat": {"own_stats": {"hit_dice": {"total": "2"}, "restorable_losses": {"hit_dice": "0"}}}}}}
        drained = {"state": {"observation": {"combat": {"own_stats": {"hit_dice": {"total": "1"}, "restorable_losses": {"hit_dice": "1"}}}}}}
        with patch.object(driver, "step", return_value=(drained, [])) as step, \
                patch.object(driver, "action") as action, patch.object(driver, "check_disk"):
            self.assertIs(driver.fight(None, initial, "foe", "wraith"), drained)
            step.assert_called_once_with(None, initial, "next")
            action.assert_not_called()

    def test_closed_disclosed_gate_opens_without_reusing_its_old_move(self):
        from unittest.mock import patch
        driver = RogueWraithProcesses()
        driver.visited, driver.last_edge = set(), None
        here = {"key": "here", "position": {"x": 0, "y": 0, "z": 0}, "wall": False}
        gate = {"key": "gate", "position": {"x": 1, "y": 0, "z": 0}, "wall": False,
                "door": {"id": "observed-door", "open": False, "reachable": True}}
        initial = {"state": {"observation": {"visible_cells": [here, gate]}}}
        refreshed = {"newly_disclosed_creature_may_have_moved": True}
        with patch.object(driver, "action", return_value=(refreshed, [])) as action:
            self.assertEqual(driver.step(None, initial), (refreshed, []))
            action.assert_called_once_with(None, initial, {
                "type": "set_door", "door": "observed-door", "open": True})
        self.assertEqual(driver.visited, {"here"})
        self.assertIsNone(driver.last_edge)

    def test_fight_stops_at_public_survived_wraith_loss_before_another_attack(self):
        from unittest.mock import patch
        driver = RogueWraithProcesses()
        driver.sample = {}
        def frame(hd):
            return {"state": {"observation": {"self_target": "self", "visible_actors": [
                {"id": "foe", "name": "foe", "position": {"x": 1, "y": 0, "z": 0}}],
                "combat": {"dead": False, "own_stats": {"hit_dice": {"total": str(hd)},
                    "restorable_losses": {"hit_dice": str(2 - hd)},
                    "abilities": [{"id": "rattling_blow", "cost": {"start": 1, "resolution": 1}}],
                    "resources": [{"resource": {"id": "stamina"}, "available": 2}]}}}}}
        initial, drained = frame(2), frame(1)
        death = {"type": "died", "actor": "foe"}
        contact = {"type": "ability", "caster": "foe", "target": "self", "ability": {
            "id": DRAIN, "name": "Energy Drain"}, "outcome": "applied", "strike": 0}
        # The old callable's positive control keeps fighting an emu until death.
        with patch.object(driver, "action", side_effect=[(drained, []), (drained, [death])]) as action, \
                patch.object(driver, "check_disk"):
            self.assertIs(driver.fight(None, initial, "foe", "emu"), drained)
            self.assertEqual(action.call_count, 2)
        # A real public HD loss is the new W stop, before the next hostile input.
        with patch.object(driver, "action", side_effect=[(drained, [contact]), (drained, [death])]) as action, \
                patch.object(driver, "check_disk"):
            self.assertIs(driver.fight(None, initial, "foe", "wraith"), drained)
            self.assertEqual(action.call_count, 1)
        # Contact without actual HD loss cannot satisfy the stop boundary.
        with patch.object(driver, "action", side_effect=[(initial, [contact]), (drained, [contact])]) as action, \
                patch.object(driver, "check_disk"):
            self.assertIs(driver.fight(None, initial, "foe", "wraith"), drained)
            self.assertEqual(action.call_count, 2)

    def test_recovery_work_and_output_limits_are_explicit_and_finite(self):
        from unittest.mock import patch
        driver = RogueWraithProcesses()
        with patch.object(ProcessTestCase, "setUp"):
            driver.setUp()
        self.assertEqual(driver.work_budget.limits, {
            "requests": 800, "actor_ticks": 40000, "transcript_bytes": 96 * 1024 * 1024})
        self.assertEqual(DISK_LIMIT, 128 * 1024 * 1024)

    def test_disclosed_diagonal_foe_uses_public_forward_edge_before_contact(self):
        from unittest.mock import patch
        driver = RogueWraithProcesses()
        driver.visited, driver.last_edge = {"prior"}, None
        def cell(key, x, y):
            return {"key": key, "position": {"x": x, "y": y, "z": 0}, "wall": False}
        def frame(cells, x, y):
            return {"state": {"observation": {"self_target": "self", "visible_cells": cells,
                "visible_actors": [{"id": "foe", "name": "emu", "position": {"x": x, "y": y, "z": 0}}]}}}
        diagonal = frame([cell("prior", 0, -1), cell("here", 0, 0), cell("next", 0, 1)], 1, 1)
        contact = frame([cell("here", 0, -1), cell("next", 0, 0)], 1, 0)
        # Positive control executes the old callable without movement at contact.
        with patch.object(driver, "action") as action:
            self.assertEqual(driver.approach(None, contact, "emu"), (contact, "foe"))
            action.assert_not_called()
        def ordinary_move(player, current, action):
            self.assertIs(current, diagonal)
            self.assertEqual(action, {"type": "move", "direction": "south"})
            return contact, []
        with patch.object(driver, "action", side_effect=ordinary_move) as action:
            self.assertEqual(driver.approach(None, diagonal, "emu"), (contact, "foe"))
            action.assert_called_once()
        self.assertEqual(driver.last_edge, ("here", "next"))
        self.assertEqual(driver.visited, {"prior", "here", "next"})

    def test_wraith_targets_exclude_disclosed_controlled_body_and_keep_opponent(self):
        driver = RogueWraithProcesses()
        own = {"id": "self", "name": "Warrior", "position": {"x": 0, "y": 0, "z": 1}}
        foe = {"id": "foe", "name": "emu", "position": {"x": 1, "y": 0, "z": 0}}
        def frame(rows):
            return {"state": {"observation": {"self_target": "self", "visible_actors": rows}}}
        # Execute the existing callable's positive control before regression cases.
        self.assertEqual(driver.targets(frame([foe])), [foe])
        with self.subTest(case="controlled upper-body sighting is not an opponent"):
            self.assertEqual(driver.targets(frame([own])), [])
        with self.subTest(case="self exclusion precedes public body deduplication"):
            self.assertEqual(driver.targets(frame([
                own, dict(own, position={"x": 0, "y": 0, "z": 0}),
                dict(foe, position={"x": 1, "y": 0, "z": 1}), foe])), [foe])


if __name__ == "__main__":
    unittest.main()
