"""Text automation boundaries and truthful combat prose over real transport."""
from scalar_wire import u64
import copy
import json
import shutil
import sqlite3
import subprocess
import threading
import tomllib
import unittest
from contextlib import closing

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT, creature_package, resource
from stream_relay import StreamRelay


def observation(frame):
    return frame["state"]["observation"]


class AttackBoundary:
    """Hold one real envelope completing the first admitted attack's turn.

    All stream envelopes retain their original order and identities. Only the
    rewind case holds an envelope; ordinary effects use the native scheduler.
    """
    def __init__(self, *, hold=False):
        self.hold = hold
        self.held = threading.Event()
        self.release = threading.Event()
        self.replies = []
        self.map_rejections = []
        self.waiting = []
        self.events = []
        self.health = []
        self.snapshots = 0
        self.admitted = None
        self.execution = None
        self.permissions = False
        self.ready = False
        self.tick = None
        self.boundary_tick = None

    def __call__(self, message):
        kind = message["type"]
        if kind == "snapshot":
            self.snapshots += 1
        if kind in ("ack", "error") and message.get("request_id") is not None:
            # An expired read-only hydration query is not another paid action.
            # Keep its complete error as separate evidence; every other error
            # remains in the gameplay reply count and still fails the witness.
            if (kind == "error" and message["request_id"].startswith("map-")
                    and message.get("code") == "stale_context"):
                self.map_rejections.append(message)
            else:
                self.replies.append(message)
            if len(self.replies) + len(self.map_rejections) > 64:
                raise AssertionError("Unbounded Text request loop")
        if kind == "waiting":
            self.waiting.append(message["on"])
            if len(self.waiting) > 64:
                raise AssertionError("Unbounded Text waiting-message loop")
        if kind == "ack" and message["receipt"]["type"] == "admitted" and self.admitted is None:
            self.admitted = message["receipt"]
        if kind != "update":
            return
        update = message["update"]
        body = update["body"]
        if body["type"] in ("observation", "observation_delta"):
            view = body["state"]
            if body["type"] == "observation":
                view = view["observation"]
            self.ready, self.tick = view["ready"], int(view["tick"])
            self.events.extend((view.get("combat") or {}).get("events", []))
            if view.get("combat"):
                self.health.append((u64(view["combat"]["hp"]), u64(view["combat"]["max_hp"])))
                if len(self.health) > 256:
                    raise AssertionError("Unbounded Text observation loop")
            if len(self.events) > 256:
                raise AssertionError("Unbounded Text combat event loop")
        admitted = self.admitted
        if admitted is None or any(update[key] != admitted[key] for key in ("actor", "branch")):
            return
        if body["type"] == "intention":
            status = body["status"]
            if all(status[key] == admitted[key] for key in ("actor", "branch", "intention", "entry_id")):
                self.execution = ((update["context"], int(update["cursor"]["sequence"]))
                                  if status["phase"] == "resolved" else None)
                self.permissions = False
        elif body["type"] == "readiness" and self.execution is not None:
            context, sequence = self.execution
            if update["context"] == context and int(update["cursor"]["sequence"]) > sequence:
                self.permissions = body["readiness"]["admission"]
        if self.hold and self.execution and self.permissions and self.ready and not self.held.is_set():
            self.boundary_tick = self.tick
            self.held.set()
            if not self.release.wait(15):
                raise AssertionError("Test did not release the completed attack boundary")


class AttackBoundaryTests(unittest.TestCase):
    def test_background_map_stale_context_is_separate_from_gameplay_replies(self):
        boundary = AttackBoundary()
        stale = {"type": "error", "request_id": "map-background", "code": "stale_context"}
        ack = {"type": "ack", "request_id": "move", "receipt": {"type": "immediate"}}
        boundary(stale)
        boundary(ack)
        self.assertEqual(boundary.replies, [ack])
        self.assertEqual(boundary.map_rejections, [stale])

    def test_other_map_errors_and_gameplay_stale_context_still_count(self):
        boundary = AttackBoundary()
        replies = [
            {"type": "error", "request_id": "map-background", "code": "invalid_request"},
            {"type": "error", "request_id": "move", "code": "stale_context"},
        ]
        for message in replies:
            boundary(message)
        self.assertEqual(boundary.replies, replies)


class TextCombatBoundaryProcesses(ProcessTestCase):
    def validate(self, package):
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return package

    def attach_text(self, boundary):
        upstream = self.address
        relay = StreamRelay(upstream, on_server_message=boundary)
        self.addCleanup(relay.close)
        self.addCleanup(boundary.release.set)
        self.address = relay.address
        try:
            text, welcome = self.adventure()
        finally:
            self.address = upstream
        boundary.replies.clear()
        boundary.map_rejections.clear()
        boundary.waiting.clear()
        boundary.events.clear()
        boundary.health.clear()
        boundary.snapshots = 0
        return text, welcome

    def admissions(self, kind="intention_admitted"):
        self.flush_save()
        with closing(sqlite3.connect(self.save.resolve().as_uri() + "?mode=ro", uri=True)) as database:
            rows = database.execute(
                "SELECT sequence,frame FROM journal WHERE sequence>0 "
                "UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 ORDER BY sequence"
            ).fetchall()
        records = []
        for sequence, frame in rows:
            # The current persisted contract has exactly two non-base journal
            # kinds: ordinary records (1) and the permanent wizard marker (2).
            # Never silently skip an unexpected or malformed record envelope.
            self.assertGreaterEqual(len(frame), 24)
            self.assertEqual(frame[:4], b"TORJ")
            self.assertEqual(int.from_bytes(frame[4:6], "little"), 6)
            self.assertEqual(int.from_bytes(frame[8:16], "little"), sequence)
            self.assertEqual(int.from_bytes(frame[16:20], "little"), len(frame) - 24)
            frame_kind = int.from_bytes(frame[6:8], "little")
            self.assertIn(frame_kind, (1, 2), "Unknown persisted journal frame kind")
            payload = json.loads(frame[24:])
            self.assertEqual(payload["generation"], 0)
            if frame_kind == 2:
                self.assertEqual(set(payload), {"save_id", "generation", "wizard_game"})
                self.assertIs(payload["wizard_game"], True)
            else:
                self.assertEqual(set(payload), {"save_id", "generation", "record"})
                records.append(payload["record"])
        return [record["entry"] for record in records
                if record["entry"]["actor"] == 1
                and record["entry"]["content"]["type"] == kind]

    def assert_one_request(self, boundary):
        self.assertEqual(len(boundary.replies), 1, boundary.replies)
        self.assertEqual(boundary.replies[0]["type"], "ack")
        self.assertEqual(boundary.replies[0]["receipt"]["type"], "admitted")

    def combat_package(self, name, *, named=False, rider=False, target_x=2):
        identity = "fixture_named_attack"
        package = creature_package(
            self, name, abilities=(identity,) if named else (),
            ability_aliases={identity: ("fixture_mana_bolt", "Attack")} if named else None,
            target_x=target_x)
        path = package / "rules.toml"
        rules = tomllib.loads(path.read_text(encoding="utf-8"))
        for pool in rules["resources"]:
            pool["recovery_period"] = 100000
        rules["health_recovery"] = {"base_period": 100000, "minimum_period": 100000,
                                    "reduction_per_hd": 0}
        basic = next(row for row in rules["abilities"] if row["id"] == "basic_melee")
        basic.update(timing={"source": {"kind": "fixed", "prepare": 20, "recover": 80}},
                     cost={"resource": "stamina", "start": 1, "resolution": 1},
                     strikes={"kind": "natural_melee", "additional_riders": []})
        basic.pop("cooldown", None)
        damage = {"kind": "damage", "damage": {"kind": "components", "components": [
            {"category": "vital", "amount": {"kind": "fixed", "amount": 3}}]}}
        if named:
            ability = next(row for row in rules["abilities"] if row["id"] == identity)
            ability.update(timing={"source": {"kind": "fixed", "prepare": 20, "recover": 80}},
                           strikes={"kind": "authored", "strikes": [{"offset": 0,
                               "on_success": [copy.deepcopy(damage)], "on_failure": [],
                               "descriptors": [], "riders": []}]})
        path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        species = manifest["creatures"]["species"]["human"]
        species["attributes"] = dict.fromkeys(species["attributes"], 0)
        species["modifiers"].append({"scope": "always", "op": {"kind": "set", "stat": {
            "kind": "resource_max", "resource": "stamina"}, "value": 20}})
        species["behavior"] = {"disposition": "passive", "idle": "wait", "memory_ticks": 1000}
        harmless = {"offset": 0, "on_success": [{"kind": "heal", "amount": {
            "kind": "fixed", "amount": 0}}], "on_failure": [], "descriptors": [], "riders": []}
        species["melee"]["strikes"] = [copy.deepcopy(harmless)]
        target = copy.deepcopy(species)
        target["modifiers"] = [row for row in target["modifiers"] if row["op"]["kind"] != "grant_ability"]
        manifest["creatures"]["species"]["practice_subject"] = target
        if rider:
            species["melee"]["strikes"][0]["riders"] = [
                {"trigger": "success", "effects": [copy.deepcopy(damage)], "descriptors": []}]
        manifest["ai_profiles"]["practice"] = {
            "care": False, "equipment": False, "loot": False, "flee_percent": 0}
        if named or rider:
            manifest["archetypes"]["fixture_blade"] = {
                "class": "weapon", "name": "unmistakable blade", "equipment": {
                    "slot": "weapon", "modifiers": [], "attack": {
                        "skill": "heavy_weaponry", "bonus": 0, "wind_up": 60, "recovery": 40,
                        "damage": {"primary": {"category": "keen"}, "components": [
                            {"category": "keen", "amount": {"type": "fixed", "value": 7}}]}}}}
        path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        region["actors"][0]["creature"]["species"] = "practice_subject"
        region["actors"][0]["turn_ticks"] = 1000
        region["items"] = []
        if named or rider:
            region["items"].append({"id": 21, "at": [1, 1, 0], "carried_by": 1,
                                    "equipped_slot": 0, "archetype": "fixture_blade"})
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        return self.validate(package)

    def test_vanished_fight_target_preserves_waiting_others_and_stops_the_chain(self):
        package = self.directory / "fight-waiting"
        shutil.copytree(ROOT / "scenarios/tests/dungeon-loop", package,
                        ignore=shutil.ignore_patterns(".compiled"))
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        region["actors"].append({"id": 3, "at": [5, 1, 0], "turn_ticks": 30,
            "controller": "external", "creature": {"species": "delver", "name": "partner",
                "faction": "delvers", "binding": "intellect", "hit_dice": [{"source": "racial"}],
                "starting_skills": ["deception", "persuasion"], "restoration": "authored",
                "nutrition": {"kind": "exempt"}}})
        region["items"].append({"id": 101, "at": [1, 1, 0], "name": "carried token", "carried_by": 1})
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        self.server(scenario=self.validate(package))
        peer, _ = self.client(actor=3, omit_memory=True)
        observer, initial = self.client(observe=True, omit_memory=True)
        boundary = AttackBoundary()
        text, _ = self.attach_text(boundary)
        # Queue the peer's genuine action, without waiting for execution while
        # the owner is still the actor to act at tick zero.
        queued = self.command(peer, {"type": "act", "action": {"type": "wait"}})
        self.assertIsNone(queued["error"])
        told = self.say(text, "fight ruin guard, then drop carried token")
        self.assertIn("falls dead", told)
        self.assertIn("others", boundary.waiting)
        self.assert_one_request(boundary)
        final = self.request(observer, {"type": "snapshot"})
        self.assertEqual(observation(final)["tick"], "30")
        self.assertFalse(observation(final)["ready"])
        self.assertEqual(observation(final)["inventory"], observation(initial)["inventory"])
        admissions = self.admissions()
        self.assertEqual(len(admissions), 1)
        self.assertEqual(admissions[0]["content"]["action"]["id"], "basic_melee")

    def recovery_package(self, change, *, follow=False):
        """Ordinary peer work while the owner's first paid action recovers.

        A ready owner is the next scheduled actor, so peers cannot create a
        natural post-settlement transition without another owner command.
        Scripted stream regressions isolate that exact catch-up boundary; these
        cases exercise the same caution policy over genuine ordinary updates.
        """
        package = self.combat_package("ordinary-" + change, target_x=4 if follow else 2)
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        peer = copy.deepcopy(manifest["creatures"]["species"]["practice_subject"])
        if change != "newcomer":
            peer["modifiers"].append({"scope": "always", "op": {
                "kind": "grant_ability", "ability": "fixture_boundary_wound"}})
            rules_path = package / "rules.toml"
            rules = tomllib.loads(rules_path.read_text(encoding="utf-8"))
            wound = copy.deepcopy(next(row for row in rules["abilities"] if row["id"] == "basic_melee"))
            wound.update(id="fixture_boundary_wound", name="Boundary Wound", basic_attack=False,
                timing={"source": {"kind": "fixed", "prepare": 25, "recover": 975}},
                reach={"kind": "sight", "range": 6},
                strikes={"kind": "authored", "strikes": [{"offset": 0,
                    "on_success": [{"kind": "damage", "damage": {
                        "kind": "components", "components": [{"category": "vital",
                            "amount": {"kind": "fixed", "amount": 5 if follow else 200}}]}}],
                    "on_failure": [], "descriptors": [], "riders": []}]})
            rules["abilities"].append(wound)
            rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        manifest["creatures"]["species"]["boundary_partner"] = peer
        path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        creature = copy.deepcopy(region["actors"][0]["creature"])
        creature.update(species="boundary_partner", name="boundary partner")
        region["actors"].append({"id": 3, "at": [4, 1, 0] if change == "newcomer" else [1, 0, 0],
                                 "turn_ticks": 1000, "controller": "external", "creature": creature})
        if change == "newcomer":
            # A two-high closed doorway hides the peer, which can open it by
            # an ordinary action. Neither setup mutation nor spawn is involved.
            region["walls"].extend([[3, y, z] for y in (0, 2) for z in (0, 1)])
            region["doors"].append({"id": 2, "at": [3, 1, 0], "open": False, "height": 2})
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        return self.validate(package)

    def queue_peer(self, change):
        peer, initial = self.client(actor=3, omit_memory=True)
        if change == "newcomer":
            door = next(cell["door"] for cell in observation(initial)["visible_cells"]
                        if cell.get("door") and not cell["door"]["open"])
            action = {"type": "set_door", "door": door["id"], "open": True}
        else:
            target = next(actor["id"] for actor in observation(initial)["visible_actors"]
                          if actor["name"] == "trainee")
            action = {"type": "use_ability", "id": "fixture_boundary_wound",
                      "invocation": {"kind": "actor_target", "target": target},
                      "supplies": {"kind": "carried"}}
        # Admission queues genuine work behind the owner at tick zero. Do not
        # wait for execution before the Text client submits its first action.
        queued = self.command(peer, {"type": "act", "action": action})
        self.assertIsNone(queued["error"])
        return peer, initial

    def recovery_caution_case(self, change):
        self.server(scenario=self.recovery_package(change))
        peer, peer_before = self.queue_peer(change)
        observer, before = self.client(observe=True, omit_memory=True)
        visible = {actor["name"] for actor in observation(before)["visible_actors"]}
        if change == "newcomer":
            self.assertNotIn("boundary partner", visible)
        else:
            self.assertIn("boundary partner", visible)
        boundary = AttackBoundary()
        text, _ = self.attach_text(boundary)
        told = self.say(text, "fight practice target, then wait")
        self.assert_one_request(boundary)
        self.assertEqual(boundary.snapshots, 0, "Ordinary caution must not be a stream reset")
        self.assertNotIn("others", boundary.waiting)
        self.assertIn("break off" if change == "newcomer" else "Badly hurt", told)
        final = self.request(observer, {"type": "snapshot"})
        self.assertEqual(observation(final)["tick"], "100")
        self.assertTrue(observation(final)["ready"])
        self.assertEqual(resource(final, "stamina")["balance"], resource(before, "stamina")["balance"] - 2)
        self.assertEqual(resource(final, "stamina")["reserved"], 0)
        if change == "newcomer":
            self.assertIn("boundary partner", {actor["name"] for actor in observation(final)["visible_actors"]})
        else:
            hp, maximum = u64(observation(final)["combat"]["hp"]), u64(observation(final)["combat"]["max_hp"])
            self.assertEqual(hp, u64(observation(before)["combat"]["hp"]) - 200)
            self.assertGreater(hp, 0)
            self.assertLessEqual(hp * 4, maximum)
            self.assertIn((hp, maximum), boundary.health)
            peer_after = self.request(peer, {"type": "snapshot"})
            self.assertEqual(resource(peer_after, "stamina")["balance"],
                             resource(peer_before, "stamina")["balance"] - 2)
        admissions = self.admissions()
        self.assertEqual(len(admissions), 1)
        self.assertEqual(admissions[0]["content"]["action"]["id"], "basic_melee")

    def test_recovery_newcomer_stops_before_a_second_paid_attack(self):
        self.recovery_caution_case("newcomer")

    def test_recovery_quarter_health_stops_before_a_second_paid_attack(self):
        self.recovery_caution_case("quarter_health")

    def test_catch_up_real_rewind_snapshot_stops_before_a_second_paid_attack(self):
        self.server(scenario=self.combat_package("fight-catch-up-rewind"), wizard=True)
        wizard = self.wizard()
        observer, before = self.client(observe=True, omit_memory=True)
        boundary = AttackBoundary(hold=True)
        text, _ = self.attach_text(boundary)
        self.send(text, "fight practice target, then wait")
        try:
            self.assertTrue(boundary.held.wait(15), "First attack never reached its complete input boundary")
            self.assertEqual(boundary.boundary_tick, 100)
            self.wizard_command(wizard, "rewind initial")
        finally:
            boundary.release.set()
        text.until(lambda line: line == "> ")
        self.assert_one_request(boundary)
        self.assertEqual(boundary.snapshots, 1)
        final = self.request(observer, {"type": "snapshot"})
        self.assertEqual(observation(final)["tick"], "0")
        self.assertEqual(resource(final, "stamina")["balance"], resource(before, "stamina")["balance"])
        self.assertEqual(resource(final, "stamina")["reserved"], 0)
        # The durable audit retains the paid attack on the abandoned future.
        self.assertEqual(len(self.admissions()), 1)

    def test_follow_first_leg_wound_stops_before_chained_wait(self):
        self.server(scenario=self.recovery_package("follow-wound", follow=True))
        peer, peer_before = self.queue_peer("wound")
        observer, before = self.client(observe=True, omit_memory=True)
        boundary = AttackBoundary()
        text, _ = self.attach_text(boundary)
        told = self.say(text, "follow practice target, then wait")
        self.assertEqual(len(boundary.replies), 1, boundary.replies)
        self.assertEqual(boundary.replies[0]["type"], "ack")
        receipt = boundary.replies[0]["receipt"]
        self.assertEqual(receipt["type"], "immediate")
        self.assertEqual(boundary.snapshots, 0)
        self.assertNotIn("others", boundary.waiting)
        self.assertNotIn("Time passes", told)
        final = self.request(observer, {"type": "snapshot"})
        hp, maximum = u64(observation(final)["combat"]["hp"]), u64(observation(final)["combat"]["max_hp"])
        self.assertEqual(hp, u64(observation(before)["combat"]["hp"]) - 5)
        self.assertIn((hp, maximum), boundary.health)
        self.assertEqual(observation(final)["tick"], "100")
        self.assertEqual(final["travel"]["id"], receipt["entry_id"])
        self.assertEqual(final["travel"]["phase"], "hazard")
        self.assertEqual(final["travel"]["completed_steps"], "1")
        self.assertEqual(self.admissions(), [])
        travel = self.admissions("travel_intention_admitted")
        self.assertEqual(len(travel), 1)
        self.assertEqual(travel[0]["content"]["step"], 1)
        self.assertEqual(travel[0]["content"]["action"]["type"], "move")
        peer_after = self.request(peer, {"type": "snapshot"})
        self.assertEqual(resource(peer_after, "stamina")["balance"],
                         resource(peer_before, "stamina")["balance"] - 2)

    def test_nonbasic_ability_named_attack_does_not_invent_a_weapon_source(self):
        self.server(scenario=self.combat_package("named-nonbasic", named=True))
        observer, initial = self.client(observe=True, omit_memory=True)
        abilities = observation(initial)["combat"]["own_stats"]["abilities"]
        selected = next(row for row in abilities if row["id"] == "fixture_named_attack")
        self.assertEqual(selected["name"], "Attack")
        self.assertFalse(selected["basic_attack"])
        target, target_before = self.client(observe=True, actor=2, omit_memory=True)
        boundary = AttackBoundary()
        text, _ = self.attach_text(boundary)
        told = self.say(text, "fixture_named_attack practice target")
        self.assertIn("You prepare to use Attack.", told)
        self.assertNotIn("unmistakable blade", told)
        self.assert_one_request(boundary)
        self.assertTrue(any(event["type"] == "ability" and event["ability"]["id"] == "fixture_named_attack"
                            for event in boundary.events))
        final = self.request(observer, {"type": "snapshot"})
        self.assertEqual(observation(final)["tick"], "100")
        self.assertEqual(resource(final, "mana")["balance"], resource(initial, "mana")["balance"] - 2)
        target_after = self.request(target, {"type": "snapshot"})
        self.assertEqual(u64(observation(target_after)["combat"]["hp"]), u64(observation(target_before)["combat"]["hp"]) - 3)
        admissions = self.admissions()
        self.assertEqual(len(admissions), 1)
        self.assertEqual(admissions[0]["content"]["action"]["id"], "fixture_named_attack")

    def test_unaffected_primary_with_real_damaging_rider_is_narrated_neutrally(self):
        self.server(scenario=self.combat_package("neutral-rider", rider=True))
        target, initial = self.client(observe=True, actor=2, omit_memory=True)
        boundary = AttackBoundary()
        text, _ = self.attach_text(boundary)
        told = self.say(text, "attack practice target")
        self.assert_one_request(boundary)
        self.assertTrue(any(event["type"] == "ability" and event["ability"]["id"] == "basic_melee"
                            and event["outcome"] == "unaffected" for event in boundary.events), boundary.events)
        final = self.request(target, {"type": "snapshot"})
        self.assertEqual(u64(observation(final)["combat"]["hp"]), u64(observation(initial)["combat"]["hp"]) - 3)
        self.assertEqual(observation(final)["tick"], "100")
        self.assertIn("practice target", told)
        # This genuine basic attack explicitly inherits natural strikes, even
        # though a weapon is equipped. The basic flag cannot prove its source.
        self.assertNotIn("unmistakable blade", told)
        for claim in ("no harm", "no visible effect", "unharmed", "shrug it off", "to no effect"):
            self.assertNotIn(claim, told)
        self.assertEqual(len(self.admissions()), 1)
