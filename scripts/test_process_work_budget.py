"""Source-only tests for deterministic request/work/output guard boundaries."""
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest

from process_work_budget import ProcessBudgetExceeded, ProcessOutputBudget, ProcessWorkBudget


def frame(tick):
    return {"state": {"observation": {"tick": str(tick), "combat": {
        "hp": 3, "max_hp": 5, "dead": False}}}, "intentions": [], "error": None}


class ProcessWorkBudgetTests(unittest.TestCase):
    def clients(self):
        directory = TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        clients = []
        for number in range(2):
            path = Path(directory.name) / f"{number}.log"
            path.write_bytes(b"1234")
            clients.append(SimpleNamespace(transcript=SimpleNamespace(path=path)))
        return clients

    def test_exact_request_budget_is_allowed_and_next_command_is_refused_before_work(self):
        budget = ProcessWorkBudget(requests=2, actor_ticks=10, transcript_bytes=20)
        for _ in range(2):
            budget.before()
        with self.assertRaisesRegex(AssertionError, '"bound": "requests"'):
            budget.before()
        self.assertEqual(budget.requests, 2)

    def test_actor_work_accumulates_across_both_clients_without_query_double_count(self):
        budget = ProcessWorkBudget(requests=8, actor_ticks=10, transcript_bytes=20)
        first, second = self.clients()
        budget.track(first, frame(20)); budget.track(second, frame(20))
        budget.after(first, frame(25)); budget.after(first, frame(25)); budget.after(second, frame(25))
        self.assertEqual(budget.actor_ticks, 10)
        with self.assertRaisesRegex(AssertionError, '"bound": "actor_ticks"'):
            budget.after(second, frame(26))

    def test_output_budget_counts_both_clients_and_checks_before_and_after_commands(self):
        budget = ProcessWorkBudget(requests=8, actor_ticks=10, transcript_bytes=10)
        first, second = self.clients()
        budget.track(first, frame(0)); budget.track(second, frame(0))
        first.transcript.path.write_bytes(b"123456")
        budget.before()
        second.transcript.path.write_bytes(b"12345")
        with self.assertRaisesRegex(AssertionError, '"bound": "transcript_bytes"'):
            budget.after(second, frame(0))
        with self.assertRaisesRegex(AssertionError, '"bound": "transcript_bytes"'):
            budget.before()
        self.assertEqual(budget.requests, 1)

    def test_invalid_bounds_and_rewind_fail_explicitly(self):
        for invalid in (0, -1, True, 1.5):
            with self.assertRaises(ValueError):
                ProcessWorkBudget(requests=invalid, actor_ticks=10, transcript_bytes=10)
        budget = ProcessWorkBudget(requests=2, actor_ticks=10, transcript_bytes=10)
        client = self.clients()[0]
        budget.track(client, frame(2))
        with self.assertRaisesRegex(AssertionError, "rewound"):
            budget.after(client, frame(1))

    def test_output_directory_budget_counts_only_new_bytes_and_deduplicates_paths(self):
        first, second = self.clients()
        directory = first.transcript.path.parent
        budget = ProcessOutputBudget([directory, first.transcript.path], limit_bytes=4)
        self.assertEqual(budget.check(), 0)
        first.transcript.path.write_bytes(b"123456")
        second.transcript.path.write_bytes(b"123456")
        self.assertEqual(budget.check(), 4)
        third = directory / "later" / "stderr.log"
        third.parent.mkdir()
        third.write_bytes(b"x")
        with self.assertRaises(ProcessBudgetExceeded) as error:
            budget.check()
        self.assertEqual((error.exception.bound, error.exception.used, error.exception.limit),
                         ("transcript_bytes", 5, 4))

    def test_output_budget_accepts_future_paths_and_rejects_truncation_or_removal(self):
        client = self.clients()[0]
        future = client.transcript.path.parent / "future.log"
        budget = ProcessOutputBudget([future], limit_bytes=4)
        future.write_bytes(b"1234")
        self.assertEqual(budget.check(), 4)
        future.write_bytes(b"")
        with self.assertRaisesRegex(ProcessBudgetExceeded, "output_continuity"):
            budget.check()
        future.unlink()
        with self.assertRaisesRegex(ProcessBudgetExceeded, "output_continuity"):
            budget.check()

    def test_removing_old_output_cannot_offset_new_file_growth(self):
        first, second = self.clients()
        budget = ProcessOutputBudget([first.transcript.path.parent], limit_bytes=10)
        first.transcript.path.unlink()
        second.transcript.path.write_bytes(b"12345678")
        with self.assertRaisesRegex(ProcessBudgetExceeded, "output_continuity"):
            budget.check()

    def test_post_ack_settlement_frames_check_output_and_work_without_extra_request(self):
        client = self.clients()[0]
        budget = ProcessWorkBudget(requests=2, actor_ticks=10, transcript_bytes=8)
        budget.track(client, frame(0))
        budget.before()

        def settling(subject, predicate, *, seconds):
            self.assertIs(subject, client)
            self.assertEqual(seconds, 15)
            subject.transcript.path.write_bytes(b"123456789")
            predicate(frame(5))
            self.fail("Output must be rejected during the settlement frame")

        with self.assertRaisesRegex(ProcessBudgetExceeded, "transcript_bytes"):
            budget.frame_wait(settling, client, lambda value: True)
        self.assertEqual(budget.requests, 1)
        self.assertEqual(budget.actor_ticks, 5)

    def test_final_report_preserves_known_counts_without_masking_output_continuity_failure(self):
        client = self.clients()[0]
        budget = ProcessWorkBudget(requests=2, actor_ticks=10, transcript_bytes=8)
        budget.track(client, frame(0))
        budget.before()
        client.transcript.path.unlink()
        report = budget.report()
        self.assertEqual(report["used"], {"requests": 1, "actor_ticks": 0, "transcript_bytes": 4})
        self.assertIn("output_continuity", report["measurement_error"])


if __name__ == "__main__":
    unittest.main()
