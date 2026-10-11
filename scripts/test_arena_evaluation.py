"""Paired arena evidence must preserve failures and pair both orientations."""
import copy
import unittest
import subprocess
import tempfile
from pathlib import Path
from unittest.mock import patch
from arena_evaluation import execute, seed, summarize


def run(seed, orientation, variant, termination):
    return {"seed": str(seed), "orientation": orientation, "variant": variant,
            "status": "ok", "report": {"termination": termination}}


class ArenaEvaluationTests(unittest.TestCase):
    def test_pair_both_orientations_before_scoring_and_count_censored_runs(self):
        runs = []
        for value in [1, 2]:
            for orientation in ["forward", "mirrored"]:
                for variant in ["baseline", "candidate"]:
                    winner = "blue" if variant == "candidate" else "red"
                    runs.append(run(value, orientation, variant,
                                    {"type": "elimination", "winner": winner}))
        runs[-1]["report"]["termination"] = {"type": "action_limit"}
        original = copy.deepcopy(runs)
        report = summarize(runs, "blue")
        self.assertEqual(report["paired_seeds"], 1)
        self.assertEqual(report["excluded_seeds"], ["2"])
        self.assertEqual(report["mean_candidate_win_difference"], 1.0)
        self.assertEqual(report["terminations"]["candidate"],
                         {"elimination": 3, "action_limit": 1})
        self.assertEqual(runs, original)

    def test_failure_draw_stalemate_and_missing_pair_are_explicit(self):
        runs = [run(1, "forward", "baseline", {"type": "elimination", "winner": None}),
                run(1, "forward", "candidate", {"type": "stalemate"}),
                {"seed": "1", "orientation": "mirrored", "variant": "baseline",
                 "status": "failure", "message": "timeout"}]
        report = summarize(runs, "blue")
        self.assertEqual(report["paired_seeds"], 0)
        self.assertIsNone(report["mean_candidate_win_difference"])
        self.assertEqual(report["terminations"]["baseline"]["failure"], 1)
        self.assertEqual(report["terminations"]["candidate"]["stalemate"], 1)
        with self.assertRaises(ValueError):
            summarize(runs + [runs[0]], "blue")

    def test_canonical_seed_bounds_and_timeout_are_explicit(self):
        self.assertEqual(seed("18446744073709551615"), "18446744073709551615")
        self.assertEqual(seed("0"), "0")
        for invalid in ["01", "-1", "+1", "18446744073709551616", "１", 1, "1" * 10000]:
            with self.subTest(value=str(invalid)[:25]), self.assertRaises(ValueError):
                seed(invalid)
        with tempfile.TemporaryDirectory() as directory:
            with patch("arena_evaluation.subprocess.run", side_effect=subprocess.TimeoutExpired("arena", 1)):
                self.assertEqual(execute(Path("arena"), Path("package"), "1", 1, directory),
                                 {"status": "failure", "message": "Evaluation timeout"})
        with self.assertRaises(ValueError):
            summarize([{"seed": "1", "orientation": "forward", "variant": "baseline",
                        "status": "typo"}], "blue")

    def test_mirrored_draws_are_paired_before_comparison(self):
        runs = []
        for orientation in ["forward", "mirrored"]:
            runs.append(run(1, orientation, "baseline", {"type": "elimination", "winner": None}))
            runs.append(run(1, orientation, "candidate", {"type": "elimination", "winner":
                                                         "blue" if orientation == "forward" else "red"}))
        summary = summarize(runs, "blue")
        self.assertEqual(summary["paired_seeds"], 1)
        self.assertEqual(summary["mean_candidate_win_difference"], 0.0)

    def test_malformed_failed_child_report_is_a_failure_record(self):
        def malformed(*args, **kwargs):
            kwargs["stdout"].write(b"null")
            return subprocess.CompletedProcess(args[0], 2)
        with tempfile.TemporaryDirectory() as directory:
            with patch("arena_evaluation.subprocess.run", side_effect=malformed):
                result = execute(Path("arena"), Path("package"), "1", 1, directory)
        self.assertEqual(result["status"], "failure")
        self.assertIn("Invalid arena report", result["message"])


class ArenaResumeTests(unittest.TestCase):
    def fixture(self, root):
        import json
        binary = root / "arena.exe"
        binary.write_bytes(b"fixed executable")
        inputs = {}
        for orientation in ("forward", "mirrored"):
            package = root / orientation
            package.mkdir()
            (package / "scenario.toml").write_text(orientation, encoding="utf-8")
            inputs[orientation] = str(package)
        plan = root / "plan.json"
        plan.write_text(json.dumps({"baseline": inputs, "candidate": inputs,
                                   "seeds": ["42", "43"], "faction": "blue"}), encoding="utf-8")
        return binary, plan, root / "batch"

    def test_resume_retains_failures_and_rejects_modified_records(self):
        import arena_evaluation as arena
        with tempfile.TemporaryDirectory() as directory:
            binary, plan, output = self.fixture(Path(directory))
            failure = {"status": "failure", "message": "Evaluation timeout"}
            with patch.object(arena, "execute", side_effect=[failure] * 3 + [InterruptedError("stop")]):
                with self.assertRaises(InterruptedError):
                    arena.run_batch(binary, plan, output)
            records = {p.name: p.read_bytes() for p in (output / "runs").iterdir()}
            record = next((output / "runs").iterdir())
            original = record.read_bytes()
            record.write_text(original.decode().replace("timeout", "altered"), encoding="utf-8")
            with patch.object(arena, "execute") as execute, self.assertRaisesRegex(ValueError, "checkpoint"):
                arena.resume_batch(binary, output)
            execute.assert_not_called()
            record.write_bytes(original)
            with patch.object(arena, "execute", return_value=failure) as execute:
                batch = arena.resume_batch(binary, output)
            self.assertEqual(execute.call_count, 5)
            self.assertEqual(len(batch["runs"]), 8)
            self.assertEqual(batch["summary"]["excluded_seeds"], ["42", "43"])
            for name, content in records.items():
                self.assertEqual((output / "runs" / name).read_bytes(), content)
            with patch.object(arena, "execute") as execute:
                self.assertEqual(arena.resume_batch(binary, output), batch)
            execute.assert_not_called()

    def test_resume_recovers_pending_record_without_reexecuting_it(self):
        import arena_evaluation as arena
        with tempfile.TemporaryDirectory() as directory:
            binary, plan, output = self.fixture(Path(directory))
            failure = {"status": "failure", "message": "Evaluation timeout"}
            write = arena.write_json
            def interrupted(path, value, **kwargs):
                if path.name == "partial.json" and value.get("completed_runs") == 1 and value.get("pending") is None:
                    raise InterruptedError("stop after record publication")
                return write(path, value, **kwargs)
            with patch.object(arena, "execute", return_value=failure), patch.object(arena, "write_json", side_effect=interrupted):
                with self.assertRaises(InterruptedError):
                    arena.run_batch(binary, plan, output)
            with patch.object(arena, "execute", return_value=failure) as execute:
                batch = arena.resume_batch(binary, output)
            self.assertEqual(execute.call_count, 7)
            self.assertEqual(len(batch["runs"]), 8)


    def test_pending_checkpoint_corruption_is_rejected_before_execution(self):
        import arena_evaluation as arena
        with tempfile.TemporaryDirectory() as directory:
            binary, plan, output = self.fixture(Path(directory))
            failure = {"status": "failure", "message": "Evaluation timeout"}
            write = arena.write_json
            def interrupt(path, value, **kwargs):
                if path.parent.name == "runs":
                    raise InterruptedError("stop before individual record publication")
                return write(path, value, **kwargs)
            with patch.object(arena, "execute", return_value=failure), patch.object(arena, "write_json", side_effect=interrupt):
                with self.assertRaises(InterruptedError):
                    arena.run_batch(binary, plan, output)
            checkpoint = arena.read_json(output / "partial.json")
            original = copy.deepcopy(checkpoint)
            pending = checkpoint["pending"]
            pending["run"]["message"] = "altered"
            write(output / "partial.json", checkpoint)
            with patch.object(arena, "execute") as execute, self.assertRaisesRegex(ValueError, "checkpoint"):
                arena.resume_batch(binary, output)
            execute.assert_not_called()
            write(output / "partial.json", original)
            # An abrupt process exit can leave a truncated unpublished record.
            (output / "runs" / "42-forward-baseline.pending").write_bytes(b"truncated temporary JSON")
            with patch.object(arena, "execute", return_value=failure) as execute:
                self.assertEqual(len(arena.resume_batch(binary, output)["runs"]), 8)
            self.assertEqual(execute.call_count, 7)
