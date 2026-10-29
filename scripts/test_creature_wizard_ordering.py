"""Driver ordering contracts; no applications, sockets or Cargo are launched."""
import copy
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import test_creature_process as creature_process


class ReachedThirdRemoval(Exception):
    """Stop after proving all three real helper mutation boundaries."""


class OwnedHitDieWizardOrdering(unittest.TestCase):
    def exercise(self, ability, granted_hd, *, update_before_ready, sync_error=False):
        subject = creature_process.CreatureAbilityProcesses(
            "test_wizard_preserves_paid_preparation_until_its_owned_hit_die_is_removed")
        commands = []
        observer_requests = []
        state = {"remaining": 3, "fresh": False}

        def frame(remaining=3, *, prepared=True):
            active = prepared and remaining >= granted_hd
            return {"error": None, "history": [{"content": {"event": {"type": "ability_started",
                    "ability": {"id": ability}}}}] if prepared else [],
                "state": {"observation": {"tick": "0", "combat": {
                    "own_stats": {"hit_dice": {"total": str(remaining)},
                        "abilities": [{"id": ability}] if active else [],
                        "resources": [{"resource": {"id": "mana"},
                            "balance": 9 if prepared else 10, "reserved": int(active)}]},
                    "preparation_active": active,
                    "preparation_remaining": "72" if active else None}}}}

        def command(value):
            commands.append(value)
            if value == "wizard arena step":
                state["fresh"] = update_before_ready
                return ("Attack preparation: 72 ticks remaining.\n" if update_before_ready else "") + "Done.\nReady."
            if value == "sync":
                if sync_error:
                    return "Server error: snapshot refused\nReady."
                state["fresh"] = True
                return "Snapshot of the current paused revision.\nReady."
            self.assertEqual(value, "wizard creature remove-hd 1")
            if not state["fresh"]:
                return "Server error StaleRevision: Refresh before a wizard operation\nReady."
            state["fresh"] = False
            state["remaining"] -= 1
            if state["remaining"] == 0:
                raise ReachedThirdRemoval()
            return "Done.\nReady."

        wizard, observer = SimpleNamespace(command=command), object()
        subject.server = Mock()
        subject.text_client = Mock(return_value=(wizard, "Ready."))
        subject.client = Mock(return_value=(observer, frame(prepared=False)))

        def observed(client, predicate):
            self.assertIs(client, observer)
            value = frame()
            self.assertTrue(predicate(value))
            # Receiving this other socket's update does not change Text freshness.
            return value

        def request(client, value):
            self.assertIs(client, observer)
            expected = "observe" if not observer_requests else "snapshot"
            self.assertEqual(value, {"type": expected})
            observer_requests.append(value["type"])
            # This read refreshes only the observer, never the Text mutator.
            return copy.deepcopy(frame(state["remaining"]))

        subject.frame, subject.request = observed, request
        with tempfile.TemporaryDirectory() as directory:
            subject.directory, subject.bin, subject.suffix = Path(directory), Path(directory), ""
            with patch.object(creature_process.subprocess, "run",
                              return_value=SimpleNamespace(returncode=0, stderr="")):
                if sync_error:
                    with self.assertRaisesRegex(AssertionError, "Server error"):
                        subject.check_owned_hit_die_preparation(ability, start_x=1, granted_hd=granted_hd)
                    self.assertEqual(commands, ["wizard arena step", "sync"])
                    self.assertEqual(state["remaining"], 3)
                    self.assertEqual(observer_requests, ["observe"])
                else:
                    with self.assertRaises(ReachedThirdRemoval):
                        subject.check_owned_hit_die_preparation(ability, start_x=1, granted_hd=granted_hd)
                    self.assertEqual(observer_requests, ["observe", "snapshot", "snapshot"])
                    self.assertEqual(commands, ["wizard arena step"] +
                        [value for _ in range(3) for value in ("sync", "wizard creature remove-hd 1")])

    def test_each_owned_hd_mutation_synchronizes_its_own_text_socket_in_both_orders(self):
        for ability, hd in (("falter", 3), ("spark_lance_fire", 2)):
            for before in (False, True):
                with self.subTest(ability=ability, update_before_ready=before):
                    self.exercise(ability, hd, update_before_ready=before)

    def test_rejected_sync_stops_before_any_mutation_without_retry(self):
        self.exercise("spark_lance_fire", 2, update_before_ready=False, sync_error=True)
