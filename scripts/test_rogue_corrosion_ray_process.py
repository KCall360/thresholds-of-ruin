"""Bounded production A/D mechanism cases, not balance or long-survival evidence.

The creature recipes, owned purchases, rules and selected Warrior kit are copied
unchanged. Only the lit encounter geometry and external foe control differ.
Each case uses one fixed seed and ordinary client commands. Public item/creature
state is checked again after deleting the copied source and reopening its save.
Text/ASCII controls and stream repair remain covered by their existing owners.
"""
import copy
import json
import shutil
import subprocess
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase
from process_work_budget import ProcessWorkBudget
from rogue_roster_package import PRODUCTION, encounter_sources, unrelated_fallback
from scalar_wire import u64
from status_condition_package import combat, observation
from rogue_test_support import disclosed_frames, resolved_events, decision_boundary


def mechanism_package(test, species):
    source = encounter_sources()
    identity = "rogue." + species
    if identity not in source["actors"]:
        raise ValueError("The mechanism case requires an actual production recipe")
    manifest = {key: copy.deepcopy(source["manifest"][key]) for key in
                ("format", "version", "ruleset", "modules", "factions", "ai_profiles", "themes", "assets")}
    character = copy.deepcopy(source["manifest"]["characters"][0])
    character["anchor"] = "1/start"
    manifest.update(id="rogue-mechanism-" + species, default_character=1, characters=[character])
    region = {"id": 1, "name": "Authored Rogue mechanism encounter", "size": [9, 5, 2],
              "chamber": True, "gravity": [0, 0, -1], "anchors": {"start": [2, 2, 0]},
              "actors": [{"id": 3, "at": [3, 2, 0], "archetype": identity,
                          "controller": "external"}]}
    package = test.directory / ("mechanism-" + species)
    (package / "regions").mkdir(parents=True)
    (package / "catalog").mkdir()
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=30)
    (test.directory / (species + "-validation.log")).write_text(
        result.stdout + result.stderr, encoding="utf-8")
    test.assertEqual(result.returncode, 0, result.stderr)
    return package


def armor_item(frame):
    return next(item for item in observation(frame)["inventory"] if item["class"] == "armor")


def armor_reductions(frame):
    values = []
    item = armor_item(frame)["id"]
    interaction = next(row for row in observation(frame)["interactions"]["inventory"]
                       if row["item"] == item)
    for modifier in interaction["known_equipment"]["modifiers"]:
        if modifier["id"] == "reduction":
            values.extend(int(field["value"]["value"]) for field in modifier["fields"]
                          if field["id"] == "amount")
    return sorted(values)


class RogueCorrosionRayProcesses(ProcessTestCase):
    def setUp(self):
        super().setUp()
        self.work_budget = ProcessWorkBudget(requests=600, actor_ticks=20000,
                                             transcript_bytes=128 * 1024 * 1024)

    def tearDown(self):
        report = self.work_budget.report()
        (self.directory / "work-budget.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("Corrosion/ray process budget: " + json.dumps(report, sort_keys=True), flush=True)
        if not self._failed():
            self.work_budget.check()
        super().tearDown()

    def launch(self, *args, **kwargs):
        self.work_budget.check()
        process = super().launch(*args, **kwargs)
        self.work_budget.output.paths += (process.transcript.path,)
        self.work_budget.check()
        return process

    def client(self, *args, **kwargs):
        kwargs.update(omit_memory=True, compact_background=True)
        client, frame = super().client(*args, **kwargs)
        self.work_budget.track(client, frame)
        return client, frame

    def command(self, client, value):
        self.work_budget.before()
        frame = super().command(client, value)
        self.work_budget.after(client, frame)
        return frame

    def frame(self, client, predicate, seconds=15):
        return self.work_budget.frame_wait(super().frame, client, predicate, seconds=seconds)

    def snapshot(self, client):
        return self.request(client, {"type": "snapshot"})

    def decision_snapshots(self, clients, attempts=24):
        """Read one held external-input boundary without advancing gameplay."""
        for _ in range(attempts):
            frames = tuple(self.snapshot(client) for client in clients)
            for frame in frames:
                self.assertIsNone(frame["error"])
            if observation(frames[0])["tick"] != observation(frames[1])["tick"]:
                continue
            owners = [index for index, frame in enumerate(frames)
                      if observation(frame)["ready"] and frame["readiness"]["admission"]]
            if len(owners) != 1 and not any(combat(frame)["terminal"] for frame in frames):
                # An admission ACK can precede execution of already paid work.
                # Read again; never queue another action to make it settle.
                continue
            owner = owners[0] if len(owners) == 1 else 0
            # Peers sampled before this owner may predate same-tick execution.
            # Reread those peers so the owner's two samples bracket every peer.
            frames = list(frames)
            for index in range(owner):
                frames[index] = self.snapshot(clients[index])
                self.assertIsNone(frames[index]["error"])
            confirmed = self.snapshot(clients[owner])
            self.assertIsNone(confirmed["error"])
            # Snapshot issuance changes transport/display generations even at
            # rest. All disclosed payload and causal permissions must remain
            # exact for this actor across the intervening peer reads.
            if (decision_boundary(confirmed) == decision_boundary(frames[owner])
                    and all(observation(frame)["tick"] == observation(confirmed)["tick"]
                            for frame in frames)):
                return tuple(confirmed if index == owner else frame
                             for index, frame in enumerate(frames))
        self.fail(f"Mechanism snapshots did not align within {attempts} read attempts")

    def advance_until(self, clients, predicate, decisions=24, allow_dead=False):
        for _ in range(decisions):
            frames = self.decision_snapshots(clients)
            self.assertEqual(observation(frames[0])["tick"], observation(frames[1])["tick"])
            if not allow_dead:
                self.assertFalse(any(combat(frame)["dead"] for frame in frames))
            if predicate(frames):
                return frames
            ready = [index for index, frame in enumerate(frames) if observation(frame)["ready"]]
            self.assertEqual(len(ready), 1)
            self.assertTrue(frames[ready[0]]["readiness"]["admission"])
            self.assertIsNone(self.act(clients[ready[0]], {"type": "wait"})["error"])
        self.fail(f"Mechanism did not settle within {decisions} ordinary decisions")

    def reopen_without_source(self, package, server, clients):
        before = tuple(self.snapshot(client) for client in clients)
        self.flush_save()
        for client in clients:
            client.stop()
        server.stop()
        shutil.rmtree(package)
        server = self.server(scenario=unrelated_fallback(self), seed=None)
        reopened = [self.client(actor=actor) for actor in (1, 3)]
        for (_, after), prior in zip(reopened, before):
            self.assertEqual(after["state"], prior["state"])
            self.assertEqual(after["history"], prior["history"])
        for client, _ in reopened:
            client.stop()
        server.stop()

    def test_production_aquator_rusts_owned_armor_to_floor_without_changing_defense(self):
        package = mechanism_package(self, "aquator")
        server = self.server("--checkpoint-interval", 4096, scenario=package, seed=42)
        player, initial = self.client(actor=1)
        foe, initial_foe = self.client(actor=3)
        clients = (player, foe)
        self.assertFalse(initial["state"]["wizard_game"])
        self.assertEqual(u64(combat(initial_foe)["own_stats"]["hit_dice"]["total"]), 5)
        self.assertEqual(observation(initial_foe)["inventory"], [])
        item = armor_item(initial)["id"]
        gear = next(row for row in observation(initial)["interactions"]["inventory"]
                    if row["item"] == item)
        self.assertIsNone(gear["known_equipment"])
        self.assertIsNone(gear["known_cursed"])
        self.assertIsNone(gear["known_protected"])
        slot = gear["equipped_slot"]
        # Authored starting equipment is not an exact-instance numeric witness.
        # Ordinary removal and wearing observes this armor without identifying
        # its curse/protection state or changing the production kit.
        for action in ({"type": "unequip", "item": item},
                       {"type": "equip", "item": item, "slot": slot}):
            self.advance_until(clients, lambda frames: observation(frames[0])["ready"])
            self.assertIsNone(self.act(player, action)["error"])
            self.advance_until(clients, lambda frames: not frames[0]["intentions"])
        initial, _ = self.advance_until(clients, lambda frames: observation(frames[1])["ready"])
        self.assertEqual(armor_reductions(initial), [2, 2])
        gear = next(row for row in observation(initial)["interactions"]["inventory"]
                    if row["item"] == item)
        self.assertEqual(gear["equipped_slot"], slot)
        self.assertIsNone(gear["known_cursed"])
        self.assertIsNone(gear["known_protected"])
        hp = combat(initial)["hp"]
        physical = combat(initial)["own_stats"]["defenses"]["physical"]
        target = next(actor["id"] for actor in observation(initial_foe)["visible_actors"]
                      if actor["name"] == "Warrior")
        seen = {2}
        floor_contacts = 0
        for _ in range(32):
            before = self.snapshot(foe)
            start = len(foe.transcript)
            accepted = self.command(foe, {"type": "act", "action": {
                "type": "use_ability", "id": "rogue.corrode",
                "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}},
                "supplies": {"kind": "carried"}}})
            self.assertIsNone(accepted["error"])
            current, _ = self.advance_until(clients, lambda frames: not frames[1]["intentions"])
            self.assertEqual(armor_item(current)["id"], item)
            self.assertEqual(combat(current)["hp"], hp)
            self.assertEqual(combat(current)["own_stats"]["defenses"]["physical"], physical)
            reductions = armor_reductions(current)
            self.assertIn(reductions, ([2, 2], [1, 1], []))
            remaining = reductions[0] if reductions else 0
            seen.add(remaining)
            events = list(resolved_events(disclosed_frames(foe, start), before,
                                          "rogue.corrode", target))
            self.assertTrue(events, "The actual completed contact must be disclosed")
            if remaining == 0 and any(event["outcome"] != "miss" for event in events):
                # Include the success which reaches zero, then require another
                # successful zero-damage contact at the exhausted armor floor.
                floor_contacts += 1
                if floor_contacts == 2:
                    break
        else:
            self.fail("Fixed-seed production contacts did not reach and retain the armor floor")
        self.assertEqual(seen, {0, 1, 2})
        self.reopen_without_source(package, server, clients)


    def test_production_dragon_breath_returns_then_hits_and_reopens(self):
        package = mechanism_package(self, "dragon")
        server = self.server("--checkpoint-interval", 4096, scenario=package, seed=42)
        player, initial = self.client(actor=1)
        dragon, original = self.client(actor=3)
        clients = (player, dragon)
        stats = combat(original)["own_stats"]
        self.assertEqual(u64(stats["hit_dice"]["total"]), 10)
        self.assertEqual(stats["attributes"]["willpower"], 3)
        self.assertEqual(next(row["rank"] for row in stats["skills"]
                              if row["skill"]["id"] == "breath"), 2)
        self.assertFalse(initial["state"]["wizard_game"])
        ability_id = "rogue.breath_weapon"
        def known(frame):
            return next(row for row in combat(frame)["own_stats"]["abilities"]
                        if row["id"] == ability_id)
        self.assertEqual(known(original)["target_kind"], "directional_ray")
        self.advance_until(clients, lambda frames: observation(frames[1])["ready"])
        before = self.snapshot(dragon)
        start = len(dragon.transcript)
        self.assertIsNone(self.command(dragon, {"type":"act", "action":{
            "type":"use_ability", "id":ability_id,
            "invocation":{"aim": {"kind":"directional_ray", "direction":"north"}, "weapon": {"kind": "default"}},
            "supplies":{"kind":"carried"}}})["error"])
        current, returned = self.advance_until(clients, lambda frames: not frames[1]["intentions"])
        receipts = list(resolved_events(disclosed_frames(dragon, start), before,
                                       ability_id, observation(before)["self_target"]))
        self.assertTrue(receipts, "The wall-returning ray must encounter its source after leaving its body")
        # Public outcome alone does not distinguish a dodge from component immunity.
        # The exact failed-check/immunity/interception branch has a ray unit owner.
        self.assertTrue(any(row["outcome"] == "unaffected" for row in receipts),
                        "The disclosed returning encounter leaves the fire-immune source unharmed")
        self.assertEqual(combat(returned)["hp"], combat(original)["hp"])
        self.assertEqual(combat(current)["hp"], combat(initial)["hp"])
        self.assertGreater(u64(known(returned)["cooldown_remaining"]), 0)
        self.advance_until(clients, lambda frames: observation(frames[1])["ready"] and
                           u64(known(frames[1])["cooldown_remaining"]) == 0, decisions=64)
        before = self.snapshot(dragon)
        start = len(dragon.transcript)
        target = next(row["id"] for row in observation(before)["visible_actors"] if row["name"] == "Warrior")
        hp = u64(combat(self.snapshot(player))["hp"])
        self.assertIsNone(self.command(dragon, {"type":"act", "action":{
            "type":"use_ability", "id":ability_id,
            "invocation":{"aim": {"kind":"directional_ray", "direction":"west"}, "weapon": {"kind": "default"}},
            "supplies":{"kind":"carried"}}})["error"])
        hit, after = self.advance_until(clients, lambda frames: not frames[1]["intentions"], allow_dead=True)
        receipts = list(resolved_events(disclosed_frames(dragon, start), before, ability_id, target))
        self.assertTrue(any(row["outcome"] == "applied" for row in receipts),
                        "The fixed-seed production ray must reach a failed Agility dodge")
        self.assertGreater(hp - u64(combat(hit)["hp"]), 0)
        self.assertLessEqual(hp - u64(combat(hit)["hp"]), 36)
        self.assertEqual(combat(after)["hp"], combat(original)["hp"])
        self.reopen_without_source(package, server, clients)


class RogueMechanismSnapshotTests(unittest.TestCase):
    @staticmethod
    def view(tick, sequence, *, ready=False, admission=True, intentions=(), terminal=False):
        return {"cursor": {"sequence": str(sequence), "tick": str(tick)},
                "state": {"revision": str(sequence), "observation": {
                    "tick": str(tick), "ready": ready,
                    "combat": {"dead": terminal, "terminal": terminal}}},
                "readiness": {"revision": str(sequence), "admission": admission},
                "intentions": list(intentions), "error": None}

    def test_mixed_ticks_are_reread_before_predicate_without_spending_a_decision(self):
        from unittest.mock import Mock, patch
        driver = RogueCorrosionRayProcesses()
        clients = (object(), object())
        player = self.view(800, 32, ready=True)
        foe = self.view(800, 46)
        # Different attachment-local sequences at one stable tick are legal.
        reads = [self.view(768, 31), self.view(800, 46), player, foe, player]
        predicate = Mock(return_value=True)
        with patch.object(driver, "snapshot", side_effect=reads) as snapshot, \
                patch.object(driver, "act") as act:
            self.assertEqual(driver.advance_until(clients, predicate, decisions=1), (player, foe))
            predicate.assert_called_once_with((player, foe))
            self.assertEqual(snapshot.call_count, 5)
            act.assert_not_called()

    def test_same_actor_progress_invalidates_an_equal_tick_pair(self):
        from unittest.mock import Mock, patch
        driver = RogueCorrosionRayProcesses()
        clients = (object(), object())
        old_owner = self.view(800, 32, ready=True)
        player = self.view(800, 33)
        foe = self.view(800, 47, ready=True)
        reads = [old_owner, self.view(800, 46), player, player, foe, player, foe]
        predicate = Mock(return_value=True)
        with patch.object(driver, "snapshot", side_effect=reads), patch.object(driver, "act") as act:
            self.assertEqual(driver.advance_until(clients, predicate, decisions=1), (player, foe))
            predicate.assert_called_once_with((player, foe))
            act.assert_not_called()

    def test_paid_admission_settles_before_one_wait_at_the_current_owner(self):
        from unittest.mock import Mock, patch
        driver = RogueCorrosionRayProcesses()
        clients = (object(), object())
        queued = self.view(800, 32, ready=True, admission=False,
                           intentions=({"phase": "queued"},))
        player = self.view(800, 33, ready=True)
        foe = self.view(800, 46)
        after = (self.view(900, 35), self.view(900, 48, ready=True))
        reads = [queued, foe, player, foe, player, *after, after[0], after[1]]
        predicate = Mock(side_effect=[False, True])
        with patch.object(driver, "snapshot", side_effect=reads), \
                patch.object(driver, "act", return_value={"error": None}) as act:
            self.assertEqual(driver.advance_until(clients, predicate, decisions=2), after)
            self.assertEqual(predicate.call_count, 2)
            act.assert_called_once_with(clients[0], {"type": "wait"})

    def test_never_aligned_reads_fail_closed_without_predicate_or_gameplay(self):
        from unittest.mock import Mock, patch
        driver = RogueCorrosionRayProcesses()
        clients = (object(), object())
        reads = [self.view(768, 31), self.view(800, 46)] * 24
        predicate = Mock(return_value=True)
        with patch.object(driver, "snapshot", side_effect=reads) as snapshot, \
                patch.object(driver, "act") as act:
            with self.assertRaisesRegex(AssertionError, "did not align within 24 read attempts"):
                driver.advance_until(clients, predicate, decisions=1)
            self.assertEqual(snapshot.call_count, 48)
            predicate.assert_not_called()
            act.assert_not_called()

    def test_terminal_boundary_preserves_allow_dead_without_waiting(self):
        from unittest.mock import Mock, patch
        driver = RogueCorrosionRayProcesses()
        clients = (object(), object())
        frames = (self.view(800, 32, admission=False, terminal=True),
                  self.view(800, 46, admission=False, terminal=True))
        for allow_dead in (False, True):
            with self.subTest(allow_dead=allow_dead), \
                    patch.object(driver, "snapshot", side_effect=[*frames, frames[0]]), \
                    patch.object(driver, "act") as act:
                predicate = Mock(return_value=True)
                if allow_dead:
                    self.assertEqual(driver.advance_until(clients, predicate, decisions=1,
                                                          allow_dead=True), frames)
                    predicate.assert_called_once_with(frames)
                else:
                    with self.assertRaises(AssertionError):
                        driver.advance_until(clients, predicate, decisions=1)
                    predicate.assert_not_called()
                act.assert_not_called()


if __name__ == "__main__":
    unittest.main()
