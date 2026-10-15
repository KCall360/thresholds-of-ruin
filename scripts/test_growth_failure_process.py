"""Hard earned-growth refusal preserves actual queued execution and saved authority."""
import copy
import json
import shutil
import sqlite3
import subprocess
import tomllib
import unittest
from contextlib import closing

from arena_matrix import toml_source
from process_harness import ProcessTestCase, creature_package
from rogue_roster_package import unrelated_fallback
from status_condition_package import actor_creature, checkpoint


class GrowthFailureProcesses(ProcessTestCase):
    def package(self, *, reject_growth):
        """A valid racial creature whose next selected class changes an ability.

        Neither current creature owns a Mage HD. Only earned growth activates
        its modifier, making the currently harmless non-hostile melee invalid.
        The separate hostile finisher is always valid and has no random check.
        """
        name = "growth-refusal" if reject_growth else "growth-control"
        package = creature_package(self, name, abilities=())
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        manifest["id"] = name
        manifest["factions"] = {"owner": ["foe"], "foe": ["owner"]}
        owner = manifest["characters"][0]["creature"]
        owner.update(faction="owner", hit_dice=[{"source": "racial"}],
                     future_growth={"kind": "selected", "source": {"kind": "class", "class": "mage"}})
        species = manifest["creatures"]["species"]["human"]
        species["attributes"]["speed"] = 0
        species["behavior"] = {"disposition": "passive", "idle": "wait", "memory_ticks": 1000}
        damage = species["melee"]["strikes"][0]["on_success"][0]["damage"]
        damage["components"][0]["amount"] = {"kind": "fixed", "amount": 0}
        manifest["ai_profiles"]["practice"] = {
            "care": False, "equipment": False, "loot": False, "flee_percent": 0}
        path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")

        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        region["actors"][0]["turn_ticks"] = 1000
        region["actors"][0]["creature"].update(faction="foe", future_growth={"kind": "disabled"})
        region["items"] = [
            {"id": 20, "at": [2, 1, 0], "carried_by": 2, "archetype": "token"},
            {"id": 21, "at": [1, 1, 0], "carried_by": 1, "archetype": "tablet"},
        ]
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")

        path = package / "rules.toml"
        rules = tomllib.loads(path.read_text(encoding="utf-8"))
        rules["growth"].update(threshold_base=100, threshold_per_tier=0, reward_base=100,
                               reward_per_relative_tier=0, minimum_reward=100, maximum_reward=100)
        melee = next(row for row in rules["abilities"] if row["id"] == "basic_melee")
        melee.update(interaction="non_hostile",
                     strikes={"kind": "natural_melee", "additional_riders": []})
        finisher = copy.deepcopy(melee)
        finisher.update(id="fixture_growth_finisher", name="Growth finisher", basic_attack=False,
                        interaction="hostile_to_target", reach={"kind": "sight", "range": 6},
                        timing={"source": {"kind": "fixed", "prepare": 1, "recover": 1}},
                        strikes={"kind": "authored", "strikes": [{
                            "offset": 0, "on_failure": [], "descriptors": [], "riders": [],
                            "on_success": [{"kind": "damage", "damage": {
                                "kind": "components", "components": [{"category": "impact",
                                    "amount": {"kind": "rolled", "dice": {"count": 1, "sides": 2, "bonus": 1000}}}]}}]}]})
        finisher.pop("cost", None)
        finisher.pop("cooldown", None)
        rules["abilities"].append(finisher)
        rules["constants"]["default_abilities"].append(finisher["id"])
        if reject_growth:
            mage = next(row for row in rules["classes"] if row["id"] == "mage")
            mage["modifiers"].append({"scope": "always", "op": {"kind": "add", "stat": {
                "kind": "ability", "ability": "basic_melee", "stat": "flat_bonus"}, "value": 1}})
        path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        return package

    def prepared(self, package):
        server = self.server("--checkpoint-interval", 1, scenario=package)
        player, _ = self.client()
        waited = self.act(player, {"type": "wait"})
        self.assertIsNone(waited["error"])

        def ready(frame):
            view = frame["state"]["observation"]
            return view["ready"] and view["tick"] == "100"

        if not ready(waited):
            self.frame(player, ready)
        before = self.request(player, {"type": "snapshot"})
        self.assertTrue(ready(before))
        self.assertFalse(before["state"]["wizard_game"])
        saved = checkpoint(self, player)
        creature = actor_creature(saved)
        self.assertEqual([row["source"] for row in creature["build"]["hd"]], [{"kind": "racial"}])
        self.assertEqual(creature["build"]["future_growth"], {
            "kind": "selected", "source": {"kind": "class", "class": "mage"}})
        self.assertEqual(creature["earned_progress"]["earned_xp"], 0)
        self.assertEqual(saved["actors"]["2"]["ready_at"], 1000)
        self.assertFalse(actor_creature(saved, 2)["dead"])
        self.assertEqual(saved["items"]["20"]["location"], {"Carried": 2})
        target = next(row["id"] for row in before["state"]["observation"]["visible_actors"]
                      if row["name"] == "practice target")
        action = {"type": "use_ability", "id": "fixture_growth_finisher",
                  "invocation": {"kind": "actor_target", "target": target}, "supplies": {"kind": "carried"}}
        return server, player, before, saved, action

    def saved_records(self, player):
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with closing(sqlite3.connect(self.save.resolve().as_uri() + "?mode=ro", uri=True)) as database:
            rows = database.execute(
                "SELECT sequence,frame FROM journal WHERE sequence>0 "
                "UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 "
                "ORDER BY sequence").fetchall()
        return [json.loads(frame[24:])["record"] for _, frame in rows]

    def execution_refusal(self, player, since):
        # Admission/resume may consume the scheduler's asynchronous error before
        # its ready frame. Inspect those actual messages before waiting for one.
        for line in player.transcript[since:]:
            if line.startswith("{"):
                message = json.loads(line).get("message") or {}
                if message.get("type") == "error":
                    break
        else:
            frame = self.frame(player, lambda row: (row.get("message") or {}).get("type") == "error")
            message = frame["message"]
        self.assertIsNone(message["request_id"], "the admitted execution, not admission, must fail")
        self.assertEqual(message["code"], "invalid_action")
        for private_detail in ("InvalidInteraction", "basic_melee", "mage", "flat_bonus"):
            self.assertNotIn(private_detail, message["message"])
        return message

    def test_control_fatal_kill_earns_selected_class_hd_and_drops_carried_item(self):
        package = self.package(reject_growth=False)
        _, player, _, before, action = self.prepared(package)
        self.assertIsNone(self.act(player, action)["error"])
        after = checkpoint(self, player)
        self.assertTrue(actor_creature(after, 2)["dead"])
        owner = actor_creature(after)
        self.assertEqual(owner["earned_progress"]["earned_xp"], 100)
        self.assertEqual(owner["earned_progress"]["spent_xp"], 100)
        self.assertEqual(owner["earned_progress"]["held_xp"], 0)
        self.assertEqual([row["source"] for row in owner["build"]["hd"]], [
            {"kind": "racial"}, {"kind": "class", "id": "mage"}])
        self.assertNotEqual(after["rules_rng"], before["rules_rng"], "the fatal control consumes combat RNG")
        self.assertIn("Ground", after["items"]["20"]["location"])
        self.assertEqual(after["items"]["21"], before["items"]["21"])
        self.assertGreater(len(after["items"]), len(before["items"]), "fatal control creates a corpse")

    def test_hard_growth_refusal_preserves_candidate_and_original_queue_after_restart(self):
        package = self.package(reject_growth=True)
        server, player, before, saved, action = self.prepared(package)
        before_records = self.saved_records(player)
        start = len(player.transcript)
        admitted = self.command(player, {"type": "act", "action": action})
        self.assertIsNone(admitted["error"])
        refused = self.execution_refusal(player, start)
        pending = self.request(player, {"type": "snapshot"})
        self.assertEqual(pending["state"], before["state"])
        self.assertEqual(checkpoint(self, player), saved,
                         "failed growth rolls back damage, death, XP, HD, loot, time and RNG")
        queued = [row for row in pending["intentions"] if row["phase"] == "queued"]
        self.assertEqual(len(queued), 1)
        records = self.saved_records(player)
        self.assertEqual(records[:-1], before_records, "only the original admission is committed")
        self.assertEqual(records[-1]["entry"]["id"], queued[0]["entry_id"])
        self.assertEqual(pending["history"], admitted["history"])

        # Retain the flushed queue while its controller is still connected.
        # Startup must suspend the same identity and restore pinned source only.
        server.stop(); player.stop()
        shutil.rmtree(package)
        self.assertFalse(package.exists())
        server = self.server("--checkpoint-interval", 1, scenario=unrelated_fallback(self), seed=None)
        player, restored = self.client()
        self.assertEqual(restored["state"], before["state"])
        self.assertEqual(restored["intentions"], [dict(queued[0], phase="suspended")])
        self.assertEqual(checkpoint(self, player), saved)
        suspended_records = self.saved_records(player)
        self.assertEqual(suspended_records[:-1], records)
        suspended = suspended_records[-1]
        self.assertEqual(suspended["entry"]["content"]["type"], "intention_changed")
        self.assertEqual(suspended["entry"]["content"]["change"], "suspended")
        self.assertEqual(suspended["entry"]["content"]["admission"], queued[0]["entry_id"])
        self.assertIsNone(suspended["receipt"])

        start = len(player.transcript)
        resumed = self.command(player, {"type": "resume_intention"})
        self.assertIsNone(resumed["error"])
        self.assertEqual(resumed["intentions"], pending["intentions"])
        retried = self.execution_refusal(player, start)
        self.assertEqual(retried["code"], refused["code"])
        self.assertEqual(retried["message"], refused["message"])
        after_retry = self.request(player, {"type": "snapshot"})
        self.assertEqual(after_retry["state"], before["state"])
        self.assertEqual(after_retry["intentions"], pending["intentions"])
        self.assertEqual(after_retry["history"], resumed["history"])
        self.assertEqual(checkpoint(self, player), saved)
        retried_records = self.saved_records(player)
        self.assertEqual(retried_records[:-1], suspended_records)
        resumed_record = retried_records[-1]
        self.assertEqual(resumed_record["entry"]["content"]["type"], "intention_changed")
        self.assertEqual(resumed_record["entry"]["content"]["change"], "resumed")
        self.assertEqual(resumed_record["entry"]["content"]["admission"], queued[0]["entry_id"])
        self.assertFalse(any(record["entry"]["content"]["type"] in
                             ("intention_failed", "intention_continuation_failed")
                             for record in retried_records))
        cancelled = self.command(player, {"type": "cancel_intention"})
        self.assertIsNone(cancelled["error"])
        self.assertEqual(cancelled["intentions"], [])
        self.assertEqual(cancelled["state"], before["state"])
        self.assertEqual(checkpoint(self, player), saved)
        cancelled_records = self.saved_records(player)
        self.assertEqual(cancelled_records[:-1], retried_records)
        cancellation = cancelled_records[-1]["entry"]["content"]
        self.assertEqual(cancellation["type"], "intention_changed")
        self.assertEqual(cancellation["change"], "cancelled")
        self.assertEqual(cancellation["admission"], queued[0]["entry_id"])
        player.stop(); server.stop()


if __name__ == "__main__":
    unittest.main()
