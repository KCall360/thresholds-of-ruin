"""Campaign policies keep constraints, independent stages and pairing explicit."""
import copy
import unittest
from arena_search import validate_config, trial_record, finalists


class ArenaSearchTests(unittest.TestCase):
    def config(self):
        return {"format": "tor-arena-search-v1", "plan": "plan.json", "parameter_spec": "parameters.json",
                "seed": "42", "constraints": [{"name": "budget", "coefficients": {"strength": 1}, "maximum": 4}]}

    def test_defaults_and_strict_configuration(self):
        space = {"strength": {"type": "integer", "low": 0, "high": 5}}
        config = validate_config(self.config(), space)
        self.assertEqual([config[key] for key in ("candidates", "training_seeds", "screening_seeds", "acceptance_seeds")],
                         [50, 20, 20, 200])
        for key, value in (("candidates", True), ("candidates", 1000), ("training_seeds", 0), ("extra", 1),
                           ("maximum_excluded_fraction", float("nan")), ("seed", "01")):
            invalid = self.config()
            invalid[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                validate_config(invalid, space)
        invalid = self.config()
        invalid["constraints"][0]["coefficients"] = {"unknown": 1}
        with self.assertRaises(ValueError):
            validate_config(invalid, space)

    def test_failures_censoring_and_budgets_are_constraints_not_silent_losses(self):
        config = validate_config(self.config(), {"strength": {"type": "integer", "low": 0, "high": 5}})
        summary = {"paired_seeds": 2, "excluded_seeds": [], "mean_candidate_win_difference": 0.5,
                   "terminations": {"baseline": {"elimination": 4}, "candidate": {"elimination": 4}}}
        original = copy.deepcopy(summary)
        record = trial_record(config, {"strength": 5}, summary, True, 2)
        self.assertEqual(record["value"], 0.5)
        self.assertEqual(record["constraints"]["budget"], 1)
        summary["paired_seeds"] = 0
        summary["excluded_seeds"] = ["1", "2"]
        summary["mean_candidate_win_difference"] = None
        record = trial_record(config, {"strength": 4}, summary, True, 2)
        self.assertGreater(record["constraints"]["coverage"], 0)
        self.assertEqual(record["value"], 0)
        summary["terminations"]["candidate"] = {"failure": 4}
        self.assertGreater(trial_record(config, {"strength": 4}, summary, True, 2)["constraints"]["failures"], 0)
        self.assertEqual(original["paired_seeds"], 2)

    def test_finalists_are_three_distinct_feasible_builds_with_stable_ties(self):
        records = [{"params": {"strength": n}, "value": value, "constraints": {"validation": valid}}
                   for n, value, valid in [(1, 1, 0), (1, 1, 0), (2, 1, 0), (3, .5, 0), (4, 2, 1)]]
        self.assertEqual(finalists(records), [0, 2, 3])
        with self.assertRaisesRegex(ValueError, "three distinct"):
            finalists(records[:3])


    def test_unstarted_batch_snapshot_recovery_preserves_failure_records(self):
        import json
        from pathlib import Path
        import tempfile
        from unittest.mock import patch
        import arena_search as search
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "arena"
            binary.write_bytes(b"fixed binary")
            packages = {}
            for orientation in ("forward", "mirrored"):
                package = root / orientation
                package.mkdir()
                (package / "scenario.toml").write_text(orientation, encoding="utf-8")
                packages[orientation] = str(package)
            plan = root / "plan.json"
            plan.write_text(json.dumps({"baseline": packages, "candidate": packages, "seeds": ["42"],
                                        "faction": "blue", "timeout_seconds": 60}), encoding="utf-8")
            output = root / "batch"
            unfinished = output / "inputs/baseline-forward"
            unfinished.mkdir(parents=True)
            (unfinished / "scenario.toml").write_text("truncated copy", encoding="utf-8")
            search.recover_unstarted_batch(binary, plan, output)
            with patch("arena_evaluation.execute", return_value={"status": "failure", "message": "timeout"}) as execute:
                batch = search.resume_batch(binary, output)
            self.assertEqual(execute.call_count, 4)
            self.assertEqual(batch["summary"]["excluded_seeds"], ["42"])
            orphaned = root / "orphaned"
            (orphaned / "runs").mkdir(parents=True)
            (orphaned / "runs/unknown.json").write_text("{}", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "contains run records"):
                search.recover_unstarted_batch(binary, plan, orphaned)
