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


if __name__ == "__main__":
    unittest.main()
