"""Real scheduler, paid Create, and durable wandering-clock acceptance.

The private checkpoint is an assertion oracle only: actions never navigate using
undisclosed cells. Shorter fixture cadence and passive neighbors isolate birth
and payment boundaries from combat; Rogue balance remains independently authored.
"""
import copy
import json
import shutil
import sqlite3
import subprocess
import tomllib
import unittest
from contextlib import closing
from unittest.mock import Mock

from arena_matrix import toml_source
from checkpoint_payload import authoritative_floor_activation, decode_checkpoint_payload
from process_harness import ProcessTestCase, ROOT


def observation(frame):
    return frame["state"]["observation"]


class WanderingPopulationProcesses(ProcessTestCase):
    def package(self):
        package = self.directory / "wandering-population"
        shutil.copytree(ROOT / "scenarios/tests/rogue-geometry", package)
        manifest_path = package / "scenario.toml"
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        # Two portal-neighbor slots fit the ordinary load horizon. This process
        # test isolates scheduling; multi-region partial residency has its own
        # full-Game transition/checkpoint witnesses.
        manifest["procedural_worlds"]["dungeon"]["grid"] = [2, 1]
        manifest["generation_recipes"]["exploration"]["room_width"] = [20, 24]
        manifest["generation_recipes"]["exploration"]["extra_connections"] = [0, 0]
        magic = tomllib.loads((ROOT / "scenarios/tests/magic-items/scenario.toml")
                             .read_text(encoding="utf-8"))
        creature = copy.deepcopy(magic["characters"][0]["creature"])
        creature["name"] = "fixture reader"
        manifest["characters"][0]["creature"] = creature
        manifest["characters"][0]["loadout"] = [{"archetype": "creation", "quantity": 10}]
        manifest["creatures"] = copy.deepcopy(magic["creatures"])
        manifest["creatures"]["species"]["figure"]["behavior"] = {
            "disposition": "passive", "idle": "wait", "memory_ticks": 1000}
        neighbor = copy.deepcopy(creature)
        neighbor["name"] = "fixture neighbor"
        manifest["archetypes"] = {
            "creation": {"name": "creation page", "class": "scroll", "stackable": True,
                         "identity": "fixture.creation_page",
                         "readable": {"control": {"kind": "create_monster"},
                                      "discovery": "never"}},
            "neighbor": {"creature": neighbor},
        }
        manifest["ai_profiles"] = {"patient": {
            "idle": "wait", "doors": False, "care": False, "equipment": False,
            "loot": False, "gold": False, "flee_percent": 0}}
        manifest["floor_activation"] = {
            "version": 1, "protagonist": "selected_character",
            "families": {"dungeon": "neighbors"},
            "wandering": {"stage_id": "neighbors", "excluded_wandering_content": [],
                          "initial_delay": [1000, 1000], "retry_ticks": 100,
                          "chance": {"numerator": 1, "denominator": 1}, "living_cap": 8},
            "profiles": {"neighbors": {"version": 1, "stages": []}},
        }
        manifest["floor_activation"]["profiles"]["neighbors"]["stages"].append({
            "id": "neighbors", "version": 1, "kind": "actors", "count": [0, 0],
            "ai": "patient", "avoid_protagonist_sight": True,
            "bands": [{"depth": {"kind": "open", "minimum": 1},
                       "choices": [{"archetype": "neighbor", "weight": 1}]}],
        })
        manifest_path.write_text(toml_source(manifest), encoding="utf-8")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return package

    def saved_population(self, client):
        self.assertIsNone(self.request(client, {"type": "save"})["error"])
        with closing(sqlite3.connect(self.save.resolve().as_uri() + "?mode=ro", uri=True)) as database:
            sequence, payload = database.execute("SELECT sequence,payload FROM checkpoint").fetchone()
            self.assertEqual(database.execute(
                "SELECT count(*) FROM journal WHERE sequence > ?", (sequence,)).fetchone()[0], 0)
        value = json.loads(decode_checkpoint_payload(payload))
        game, shared = value["game"], value["shared"]
        activation = authoritative_floor_activation(game["floor_activation"], shared)
        return {"tick": game["tick"], "rules_rng": game["rules_rng"],
                "population": activation["wandering"],
                "actors": shared["actors"][game["actors"]],
                "items": shared["items"][game["items"]]}

    def snapshot(self, client):
        frame = self.request(client, {"type": "snapshot"})
        self.assertIsNone(frame["error"])
        return frame

    def paid(self, client, action):
        before = self.snapshot(client)
        def complete(frame):
            # act() acknowledges started work, not necessarily its paid effect.
            # These passive fixtures must not resume or replace interrupted work.
            self.assertIsNone(frame["error"])
            message = frame.get("message") or {}
            self.assertNotEqual(message.get("type"), "error", message)
            body = (message.get("update") or {}).get("body") or {}
            rejected = {"paused", "suspended", "failed", "cancelled"}
            if body.get("type") == "intention":
                self.assertNotIn(body["status"]["phase"], rejected, body)
            self.assertFalse(rejected.intersection(
                status["phase"] for status in frame["intentions"]), frame["intentions"])
            view = observation(frame)
            self.assertFalse(view["combat"]["terminal"], view["combat"])
            readiness = frame["readiness"]
            return (view["ready"] and not view["combat"]["preparation_active"]
                    and view["combat"]["preparation_remaining"] is None
                    and not frame["intentions"] and readiness["admission"]
                    and not readiness["resume"] and not readiness["cancel"])

        accepted = self.act(client, action)
        if not complete(accepted):
            self.frame(client, complete)
        after = self.snapshot(client)
        self.assertGreater(int(observation(after)["tick"]), int(observation(before)["tick"]))
        self.assertTrue(observation(after)["ready"])
        self.assertFalse(after["intentions"])
        self.assertFalse(after["state"]["wizard_game"])
        return after

    @staticmethod
    def origins(saved, origin):
        return {identity for identity, birth in saved["population"]["births"].items()
                if birth["origin"] == origin}

    def test_scheduler_restart_cap_and_paid_created_births_then_blocked_consumption(self):
        package = self.package()
        server = self.server("--checkpoint-interval", 1, scenario=package)
        client, initial = self.client()
        scroll, = [item for item in observation(initial)["inventory"]
                   if item["name"] == "creation page"]
        self.assertEqual(scroll["quantity"], "10")
        self.paid(client, {"type": "wait"})
        saved = self.saved_population(client)
        clock, = saved["population"]["clocks"].values()
        self.assertEqual(clock["remaining"], 1000 - saved["tick"])
        self.assertGreater(clock["remaining"], 0)
        self.assertFalse(saved["population"]["births"])
        # Observation and checkpoint queries must not sample a trial or reset time.
        self.snapshot(client)
        self.assertEqual(self.saved_population(client), saved)
        client.stop()
        server.stop(save=False)
        package.rename(self.directory / "retained-population-source")
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        client, restored = self.client()
        self.assertEqual(self.saved_population(client), saved)
        # Ordinary paid waits drive the real server scheduler. No privileged
        # birth, clock, position or RNG mutation establishes the cap witness.
        for _ in range(240):
            self.paid(client, {"type": "wait"})
            saved = self.saved_population(client)
            wandering = self.origins(saved, "wandering")
            self.assertLessEqual(len(wandering), 8)
            if len(wandering) == 8:
                break
        else:
            self.fail("ordinary scheduler did not produce eight live wandering neighbors within 240 waits")
        for _ in range(12):
            self.paid(client, {"type": "wait"})
        capped = self.saved_population(client)
        self.assertEqual(self.origins(capped, "wandering"), wandering)
        self.assertFalse(self.origins(capped, "created"))
        current = self.snapshot(client)
        scroll, = [item for item in observation(current)["inventory"]
                   if item["name"] == "creation page"]
        after = self.paid(client, {"type": "read", "item": scroll["id"]})
        created = self.saved_population(client)
        self.assertEqual(len(self.origins(created, "created")), 1)
        self.assertEqual(self.origins(created, "wandering"), wandering)
        self.assertEqual(sum(item["quantity"] for item in created["items"].values()), 9,
                         "the only authored stock was ten pages; births create no carried loot")
        self.assertTrue(any(actor["name"] == "fixture neighbor"
                            for actor in observation(after)["visible_actors"]))
        # Patient same-faction neighbors do not move. At most eight adjacent
        # anchors can receive created actors; the ninth read must be paid and
        # consumed without a replacement birth or privileged blocker disclosure.
        previous = created
        for number in range(2, 10):
            after = self.paid(client, {"type": "read", "item": scroll["id"]})
            current = self.saved_population(client)
            self.assertEqual(sum(item["quantity"] for item in current["items"].values()), 10 - number)
            self.assertEqual(self.origins(current, "wandering"), wandering)
            self.assertGreaterEqual(len(self.origins(current, "created")), len(self.origins(previous, "created")))
            if number == 9:
                self.assertEqual(self.origins(current, "created"), self.origins(previous, "created"))
            previous = current
        self.assertLessEqual(len(self.origins(previous, "created")), 8)
        self.assertEqual(len(self.origins(previous, "wandering")), 8)
        self.assertEqual(observation(after)["inventory"][0]["quantity"], "1")


class PaidPopulationCompletionTests(unittest.TestCase):
    """The real paid caller must not confuse started work with completion."""

    @staticmethod
    def frame(tick, *, phase=None, remaining=None, admission=True,
              cancel=(), resume=(), error=None, terminal=False, active=None):
        return {"type": "update", "error": error,
                "state": {"wizard_game": False, "observation": {
                    "tick": str(tick), "ready": remaining is None,
                    "combat": {"terminal": terminal,
                               "preparation_remaining": remaining,
                               "preparation_active": phase == "started" if active is None else active}}},
                "readiness": {"admission": admission, "cancel": list(cancel),
                              "resume": list(resume), "revision": "2"},
                "intentions": ([] if phase is None else [
                    {"intention": "read-1", "phase": phase}])}

    def harness(self, accepted, updates=()):
        owner = WanderingPopulationProcesses(
            "test_scheduler_restart_cap_and_paid_created_births_then_blocked_consumption")
        before, complete = self.frame(9600), self.frame(9700)
        settled = [accepted is complete or (accepted["error"] is None
                   and accepted["state"]["observation"]["tick"] == "9700")]
        snapshots = [0]

        def snapshot(_client):
            snapshots[0] += 1
            if snapshots[0] == 1:
                return before
            self.assertTrue(settled[0], "paid caller queried a snapshot before completion")
            return complete

        def wait(_client, predicate, seconds=15):
            self.assertEqual(seconds, 15)
            for candidate in updates:
                if predicate(candidate):
                    settled[0] = candidate == complete
                    return candidate
            self.fail("completion predicate did not accept the final fixture frame")

        owner.snapshot = Mock(side_effect=snapshot)
        owner.act = Mock(return_value=accepted)
        owner.frame = Mock(side_effect=wait)
        return owner, complete

    def test_already_complete_paid_action_keeps_one_submission(self):
        accepted = self.frame(9700)
        owner, complete = self.harness(accepted)
        action = {"type": "read", "item": "creation-page"}
        self.assertEqual(owner.paid("client", action), complete)
        owner.act.assert_called_once_with("client", action)
        owner.frame.assert_not_called()
        self.assertEqual(owner.snapshot.call_count, 2)

    def test_started_read_waits_through_resolution_and_permission_before_snapshot(self):
        started = self.frame(9600, phase="started", remaining="100",
                             admission=False, cancel=("read-1",))
        resolved = self.frame(9700, cancel=("read-1",))
        complete = self.frame(9700)
        owner, _ = self.harness(started, [started, resolved, complete])
        action = {"type": "read", "item": "creation-page"}
        self.assertEqual(owner.paid("client", action), complete)
        owner.act.assert_called_once_with("client", action)
        owner.frame.assert_called_once()
        self.assertEqual(owner.snapshot.call_count, 2)

    def test_permission_before_effect_and_final_lifecycle_needs_no_extra_revision(self):
        started = self.frame(9600, phase="started", remaining="100", admission=False)
        early_permission = self.frame(9600, phase="started", remaining="100")
        effect = self.frame(9700, phase="started", active=False)
        complete = self.frame(9700)
        owner, _ = self.harness(started, [early_permission, effect, complete])
        action = {"type": "read", "item": "creation-page"}
        self.assertEqual(owner.paid("client", action), complete)
        owner.act.assert_called_once_with("client", action)
        owner.frame.assert_called_once()
        self.assertEqual(owner.snapshot.call_count, 2)

    def test_inactive_failure_and_raw_error_messages_cannot_look_complete(self):
        started = self.frame(9600, phase="started", remaining="100", admission=False)
        messages = [{"type": "update", "update": {"body": {
            "type": "intention", "status": {"intention": "read-1", "phase": phase}}}}
                    for phase in ("failed", "cancelled")]
        messages.append({"type": "error", "message": "rejected"})
        for message in messages:
            with self.subTest(message=message):
                stopped = self.frame(9700)
                stopped["message"] = message
                owner, _ = self.harness(started, [stopped, self.frame(9700)])
                with self.assertRaises(AssertionError):
                    owner.paid("client", {"type": "read", "item": "creation-page"})
                owner.act.assert_called_once()
                owner.frame.assert_called_once()
                self.assertEqual(owner.snapshot.call_count, 1)

    def test_delayed_rejection_death_and_interruption_fail_before_snapshot(self):
        started = self.frame(9600, phase="started", remaining="100",
                             admission=False, cancel=("read-1",))
        stops = [self.frame(9600, error="rejected"),
                 self.frame(9600, terminal=True),
                 self.frame(9600, phase="paused", remaining="100", resume=("read-1",)),
                 self.frame(9600, phase="suspended", remaining="100"),
                 self.frame(9600, phase="failed"),
                 self.frame(9600, phase="cancelled")]
        for stopped in stops:
            with self.subTest(stopped=stopped):
                owner, _ = self.harness(started, [stopped, self.frame(9700)])
                with self.assertRaises(AssertionError):
                    owner.paid("client", {"type": "read", "item": "creation-page"})
                owner.act.assert_called_once()
                owner.frame.assert_called_once()
                self.assertEqual(owner.snapshot.call_count, 1)


if __name__ == "__main__":
    unittest.main()
