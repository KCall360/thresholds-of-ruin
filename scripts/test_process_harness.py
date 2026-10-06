"""Behavior of the shared actual-process harness without launching applications."""
import unittest
from unittest.mock import patch

from process_harness import ProcessTestCase


class ActionCompletion(unittest.TestCase):
    def test_decimal_readiness_revisions_wait_for_the_newer_permission_frame(self):
        for revision in [9, 99, 9007199254740991, 9007199254740992, 18446744073709551614]:
            with self.subTest(revision=revision):
                harness = ProcessTestCase()
                accepted = {"intentions": [{"phase": "queued", "intention": "admission-1"}]}
                executed = {
                    "message": {"type": "update", "update": {"body": {
                        "type": "intention", "status": {
                            "intention": "admission-1", "phase": "resolved"}}}},
                    "readiness": {"revision": str(revision)},
                }
                newer = {"readiness": {"revision": str(revision + 1)}}
                batches = [[executed], [executed, newer]]

                def frame(client, predicate):
                    selected = next((item for item in batches.pop(0) if predicate(item)), None)
                    self.assertIsNotNone(selected, "the newer permission frame was not selected")
                    return selected

                with patch.object(harness, "command", return_value=accepted), \
                        patch.object(harness, "frame", side_effect=frame):
                    self.assertIs(harness.act(object(), {"type": "wait"}), newer)
                self.assertEqual(batches, [])


if __name__ == "__main__":
    unittest.main()
