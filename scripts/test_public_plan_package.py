"""Public-plan fixture coordination without launching application processes."""
import copy
import unittest
from unittest.mock import Mock

from public_plan_package import paid_preparation


def subject_frame(*, ready=False, revision="0", admission=True, control=True,
                  intentions=(), target="observer-before-handoff"):
    # Minimal projection of the retained CI handoff: admission stays available
    # at generation 1 while the same-tick turn changes observation revision 0->1.
    return {"error": None, "has_control": control, "intentions": list(intentions),
            "readiness": {"revision": "1", "admission": admission, "resume": [], "cancel": []},
            "state": {"revision": revision, "observation": {"tick": "0", "ready": ready,
                "visible_actors": [{"name": "observer", "id": target}]}}}


class PublicPlanHandoff(unittest.TestCase):
    def setUp(self):
        self.harness = unittest.TestCase()
        self.player, self.subject = object(), object()
        self.initial = {"state": {"observation": {
            "visible_actors": [{"name": "subject", "id": "subject-target"}],
            "combat": {"own_stats": {"resources": [
                {"resource": {"id": "focus"}, "balance": 10, "reserved": 0}]}}}}}
        self.subject_initial = subject_frame()
        self.handoff = subject_frame(ready=True, revision="1", target="current-observer")
        self.observed = copy.deepcopy(self.initial)
        combat = self.observed["state"]["observation"]["combat"]
        combat["public_plan"] = {"observed_at": "100", "state": {"remaining_work": "200"}}
        combat["own_stats"]["resources"][0]["balance"] = 8
        self.frames = [self.subject_initial, self.handoff]
        self.consumed_handoff = False
        self.subject_error = None
        self.harness.act = Mock(side_effect=self.act)
        self.harness.frame = Mock(side_effect=self.frame)

    def act(self, client, action):
        if client is self.subject:
            if not self.consumed_handoff:
                return {"error": "StaleRevision: Refresh the observation before acting"}
            self.assertEqual(action, {"type": "use_ability", "id": "slow_plan",
                "invocation": {"aim": {"kind": "actor_target", "target": "current-observer"}, "weapon": {"kind": "default"}}})
            return {"error": self.subject_error}
        self.assertIs(client, self.player)
        return {"error": None}

    def frame(self, client, predicate):
        if client is self.subject:
            for frame in self.frames:
                if predicate(frame):
                    self.assertIs(frame, self.handoff)
                    self.consumed_handoff = True
                    return frame
            raise AssertionError("Timed out waiting for subject handoff")
        self.assertIs(client, self.player)
        self.assertTrue(predicate(self.observed))
        return self.observed

    def prepare(self):
        return paid_preparation(self.harness, self.player, self.initial, self.subject)

    def test_paid_preparation_consumes_subject_handoff_before_one_fresh_action(self):
        self.assertIs(self.prepare(), self.observed)
        self.assertTrue(self.consumed_handoff)
        self.assertEqual(self.harness.act.call_count, 2)
        self.assertEqual(self.harness.frame.call_count, 2)

    def test_turn_ready_alone_does_not_bypass_control_admission_or_pending_work(self):
        self.frames = [
            {"error": None, "state": None},
            self.subject_initial,
            subject_frame(ready=True, revision="1", control=False),
            subject_frame(ready=True, revision="1", admission=False),
            *[subject_frame(ready=True, revision="1", intentions=[{"phase": phase}])
              for phase in ("queued", "started", "paused", "suspended")],
            self.handoff,
        ]
        self.assertIs(self.prepare(), self.observed)
        self.assertEqual(self.harness.act.call_count, 2)

    def test_missing_handoff_times_out_without_a_subject_action(self):
        self.frames = [self.subject_initial]
        with self.assertRaisesRegex(AssertionError, "Timed out waiting for subject handoff"):
            self.prepare()
        self.assertFalse(self.consumed_handoff)
        self.assertEqual(self.harness.act.call_count, 1)

    def test_frame_error_is_not_hidden_while_waiting_for_handoff(self):
        self.frames = [{"error": "Connection refused", "state": None}, self.handoff]
        with self.assertRaisesRegex(AssertionError, "Connection refused"):
            self.prepare()
        self.assertFalse(self.consumed_handoff)
        self.assertEqual(self.harness.act.call_count, 1)

    def test_subject_refusal_is_reported_without_retry(self):
        self.subject_error = "StaleRevision: Refresh the observation before acting"
        with self.assertRaisesRegex(AssertionError, "StaleRevision"):
            self.prepare()
        self.assertTrue(self.consumed_handoff)
        self.assertEqual(self.harness.act.call_count, 2)


if __name__ == "__main__":
    unittest.main()
