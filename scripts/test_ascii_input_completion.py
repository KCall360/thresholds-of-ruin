"""Input acknowledgements and simulation completion may be different frames."""
import unittest

from process_harness import ascii_input_completion


class AsciiInputCompletion(unittest.TestCase):
    def test_effect_frame_does_not_need_to_repeat_input_acknowledgement(self):
        complete = ascii_input_completion("wait")
        self.assertFalse(complete(dict(input_done=None, busy=False, intentions=[])))
        self.assertFalse(complete(dict(input_done="wait", busy=False,
                                       intentions=[dict(phase="queued")])))
        self.assertFalse(complete(dict(input_done=None, busy=True, intentions=[])))
        self.assertTrue(complete(dict(input_done=None, busy=False, intentions=[])))

    def test_other_input_and_still_queued_work_cannot_finish_action(self):
        complete = ascii_input_completion("right")
        self.assertFalse(complete(dict(input_done="left", busy=False, intentions=[])))
        self.assertFalse(complete(dict(input_done="right", busy=False,
                                       intentions=[dict(phase="queued")])))
        self.assertFalse(complete(dict(input_done=None, busy=False,
                                       intentions=[dict(phase="queued")])))
        self.assertTrue(complete(dict(input_done=None, busy=False,
                                      intentions=[dict(phase="started")])))

    def test_explicit_queue_controls_can_acknowledge_without_execution(self):
        complete = ascii_input_completion("resume_intention", wait_for_simulation=False)
        self.assertTrue(complete(dict(input_done="resume_intention", busy=False,
                                      intentions=[dict(phase="queued")])))

    def test_sequential_input_waits_for_ordered_action_permission(self):
        complete = ascii_input_completion("right", wait_for_admission=True)
        # The CI doorway trace published completed movement and ready=True
        # before its separate permission update. A second move then refused.
        executed = dict(input_done="right", busy=False, intentions=[],
                        can_admit_intention=False,
                        state=dict(observation=dict(ready=True)))
        self.assertFalse(complete(executed))
        self.assertFalse(complete({**executed, "input_done": None}))
        self.assertTrue(complete({**executed, "input_done": None,
                                  "can_admit_intention": True}))

    def test_admission_wait_requires_published_permission_and_input_acknowledgement(self):
        complete = ascii_input_completion("right", wait_for_admission=True)
        self.assertFalse(complete(dict(input_done=None, busy=False, intentions=[],
                                       can_admit_intention=True)))
        self.assertFalse(complete(dict(input_done="right", busy=False, intentions=[])))
        self.assertFalse(complete(dict(input_done=None, busy=True, intentions=[],
                                       can_admit_intention=True)))
        self.assertTrue(complete(dict(input_done=None, busy=False, intentions=[],
                                      can_admit_intention=True)))


if __name__ == "__main__":
    unittest.main()
