"""Pure checks for authoritative survival-process observation boundaries."""
import copy
import json
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import tomllib
import unittest
import zlib
from contextlib import closing
from unittest.mock import patch

from checkpoint_payload import HEADER
from nutrition_package import observed_meal_complete
from status_condition_package import WORK, WOUND, actor_clock, checkpoint, condition_remaining, prepare


class SavedCheckpointHandles(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.save = Path(directory.name) / "saved facts #1.db"
        self.expected = {"tick": 75, "rules_rng": 42, "actors": {"1": {"active_since": 50}},
                         "items": {}, "frozen_at": {"1": 50}}
        value = {"game": {"tick": 75, "rules_rng": 42, "actors": 0, "items": 0, "lifecycle": 0},
                 "shared": {"actors": [self.expected["actors"]], "items": [{}],
                            "lifecycles": [{"stamps": self.expected["frozen_at"]}]}}
        raw = json.dumps(value).encode("utf-8")
        payload = HEADER + zlib.compress(raw) + len(raw).to_bytes(4, "little")
        with closing(sqlite3.connect(self.save)) as database:
            database.execute("CREATE TABLE checkpoint (sequence INTEGER, payload BLOB)")
            database.execute("CREATE TABLE journal (sequence INTEGER)")
            database.execute("INSERT INTO checkpoint VALUES (1, ?)", (payload,))
            database.commit()

    def check_closes(self, *, missing):
        if missing:
            with closing(sqlite3.connect(self.save)) as database:
                database.execute("DELETE FROM checkpoint")
                database.commit()
        retained = []
        connect = sqlite3.connect

        def retain(*args, **kwargs):
            connection = connect(*args, **kwargs)
            retained.append(connection)
            self.addCleanup(connection.close)
            return connection

        before = self.save.read_bytes()
        player = object()
        with patch.object(self, "request", create=True, return_value={"error": None}) as request, \
                patch("status_condition_package.sqlite3.connect", side_effect=retain) as opened:
            if missing:
                with self.assertRaisesRegex(AssertionError, "Checkpoint fixture requires"):
                    checkpoint(self, player)
            else:
                self.assertEqual(checkpoint(self, player), self.expected)
        request.assert_called_once_with(player, {"type": "save"})
        self.assertEqual(len(retained), 1)
        # Hold a strong reference: refcounting/garbage collection cannot hide a
        # leaked Windows handle before the process fixture stops or restarts.
        with self.assertRaises(sqlite3.ProgrammingError):
            retained[0].execute("SELECT 1")
        opened.assert_called_once_with(self.save.resolve().as_uri() + "?mode=ro", uri=True)
        self.assertEqual(self.save.read_bytes(), before)

    def test_checkpoint_closes_retained_read_only_handle_after_success(self):
        self.check_closes(missing=False)

    def test_checkpoint_closes_retained_read_only_handle_after_assertion(self):
        self.check_closes(missing=True)


class SavedConditionClocks(unittest.TestCase):
    @staticmethod
    def saved(*, tick=100, active_since=75, remaining=200, frozen_at=None, phase="active"):
        return {"tick": tick, "frozen_at": frozen_at or {}, "actors": {
            "2": {"active_since": active_since, "combat": {"creature": {"conditions": [{
                "condition": "fixture", "phase": phase, "remaining": remaining}]}}}}}

    def test_lazy_active_remaining_uses_actor_anchor_without_rewriting_save(self):
        saved = self.saved()
        before = copy.deepcopy(saved)
        self.assertEqual(actor_clock(saved, 2), 100)
        self.assertEqual(condition_remaining(saved, "fixture", 2), 175)
        self.assertEqual(saved["tick"] + condition_remaining(saved, "fixture", 2), 275)
        self.assertEqual(saved, before)

    def test_exact_peer_turn_elapsed_reduces_effective_remaining_by_25(self):
        before, after = self.saved(), self.saved(tick=125)
        self.assertEqual(condition_remaining(before, "fixture", 2)
                         - condition_remaining(after, "fixture", 2), 25)
        self.assertEqual(before["actors"], after["actors"])

    def test_frozen_actor_uses_its_own_stamp_instead_of_world_time(self):
        saved = self.saved(tick=2100, frozen_at={"1": 50, "2": 100})
        self.assertEqual(actor_clock(saved, 2), 100)
        self.assertEqual(condition_remaining(saved, "fixture", 2), 175)
        saved["tick"] += 1000
        self.assertEqual(condition_remaining(saved, "fixture", 2), 175)

    def test_freeze_settlement_and_thaw_anchor_preserve_effective_remaining(self):
        active = self.saved()
        frozen = self.saved(tick=2100, active_since=100, remaining=175, frozen_at={"2": 100})
        thawed = self.saved(tick=2100, active_since=2100, remaining=175)
        for saved in (active, frozen, thawed):
            self.assertEqual(condition_remaining(saved, "fixture", 2), 175)

    def test_protection_uses_the_same_lazy_actor_clock(self):
        saved = self.saved(remaining=400, phase="protection")
        self.assertEqual(condition_remaining(saved, "fixture", 2), 375)

    def test_invalid_or_unsettled_phase_is_exposed_instead_of_clamped(self):
        for saved in (self.saved(active_since=101), self.saved(tick=275), self.saved(tick=300)):
            with self.subTest(saved=saved), self.assertRaises(AssertionError):
                condition_remaining(saved, "fixture", 2)


class ObservedMealCompletion(unittest.TestCase):
    @staticmethod
    def frame(*, reserve=2000, inventory=(), remaining=None, ready=True, terminal=False):
        return {"has_control": False, "readiness": {"admission": False, "resume": []},
                "intentions": [], "state": {"observation": {
                    "ready": ready, "inventory": list(inventory), "combat": {
                        "preparation_remaining": remaining, "terminal": terminal,
                        "own_stats": {"nutrition": {"reserve": reserve}}}}}}

    def test_completed_meal_is_visible_without_owner_permission(self):
        self.assertTrue(observed_meal_complete(self.frame(), "meal-1", 1300))

    def test_start_pause_and_partial_effects_are_not_completion(self):
        for frame in (self.frame(remaining="100"), self.frame(remaining="50"),
                      self.frame(reserve=1300), self.frame(inventory=({"id": "meal-1"},)),
                      self.frame(ready=False), self.frame(terminal=True), {"state": None}):
            with self.subTest(frame=frame):
                self.assertFalse(observed_meal_complete(frame, "meal-1", 1300))

    def test_consumption_tracks_exact_source_not_other_remaining_items(self):
        self.assertTrue(observed_meal_complete(
            self.frame(inventory=({"id": "meal-2"},)), "meal-1", 1300))


class StreamingSubjectContent(unittest.TestCase):
    def test_freezable_subject_is_autonomous_and_prefers_the_paid_healing_work(self):
        rules = {"resources": [], "conditions": [], "abilities": []}
        with tempfile.TemporaryDirectory() as directory, \
                patch("status_condition_package.load_rules", return_value=rules), \
                patch("status_condition_package.subprocess.run", return_value=
                      subprocess.CompletedProcess([], 0, stdout="", stderr="")):
            self.directory, self.bin, self.suffix = Path(directory), Path("unused-bin"), ""
            package = prepare(self, streaming=True)
            manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
            region = tomllib.loads((package / "regions/1.toml").read_text(encoding="utf-8"))
            subject, = region["actors"]
            self.assertEqual((subject["id"], subject["controller"], subject["ai"]), (2, "ai", "patient"))
            self.assertEqual(manifest["ai_profiles"]["patient"]["flee_percent"], 0)
            work = next(row for row in rules["abilities"] if row["id"] == WORK)
            self.assertEqual(work["ai"]["priority"], 1000)
            self.assertFalse(work["ai"]["hostile"])
            self.assertEqual(work["supply_costs"], [{"supply": "bandage", "quantity": 1,
                "access": "carried_only", "spend_at": "first_resolution"}])
            wound = next(row for row in rules["abilities"] if row["id"] == WOUND)
            self.assertEqual(wound["timing"]["source"], {"kind": "fixed", "prepare": 25, "recover": 25})
            self.assertTrue(wound["ai"]["hostile"])
            self.assertEqual(wound["strikes"]["strikes"][0]["on_success"][0]["damage"]["components"],
                             [{"category": "vital", "amount": {"kind": "fixed", "amount": 110}}])


if __name__ == "__main__":
    unittest.main()
