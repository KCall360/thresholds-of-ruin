"""Paired arena evidence must preserve failures and pair both orientations."""
import copy
import json
import unittest
import subprocess
import tempfile
from pathlib import Path
from unittest.mock import patch
from arena_evaluation import execute, seed, summarize


def run(seed, orientation, variant, termination):
    return {"seed": str(seed), "orientation": orientation, "variant": variant,
            "status": "ok", "report": {"termination": termination}}


def run_report(value="42", format="tor-arena-run-v5"):
    # Only the fields consumed by the Python identity/summary gate are modeled.
    # The real-process tests exercise the complete Rust report and its counters.
    return {"format": format, "input": {"seed": value},
            "participants": [{"actor": "1", "faction": "blue", "hit_dice": "1",
                "initial": {"health": "10", "maximum_health": "10", "injury": "0"},
                "final_state": {"health": "10", "maximum_health": "10", "injury": "0"}}],
            "termination": {"type": "action_limit"}}


def evaluation_fixture(root, **extra):
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
                               "seeds": ["42", "43"], "faction": "blue", **extra}), encoding="utf-8")
    return binary, plan, root / "batch"


def distinct_rosters():
    return {variant: [{"actor": actor, "faction": "blue"}, {"actor": "3", "faction": "red"}]
            for variant, actor in (("baseline", "1"), ("candidate", "2"))}


def distinct_report(binary, package, value, timeout, scratch):
    variant = "baseline" if package.name.startswith("baseline-") else "candidate"
    report = run_report(value)
    report["participants"] = [{**copy.deepcopy(report["participants"][0]), **row}
                              for row in distinct_rosters()[variant]]
    return {"status": "ok", "report": report}


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

    def execute_report(self, report, returncode=0):
        def child(*args, **kwargs):
            kwargs["stdout"].write(json.dumps(report).encode("utf-8"))
            return subprocess.CompletedProcess(args[0], returncode)
        with tempfile.TemporaryDirectory() as directory:
            with patch("arena_evaluation.subprocess.run", side_effect=child):
                return execute(Path("arena"), Path("package"), "42", 1, directory)

    def test_execute_rejects_previous_success_report_revision(self):
        result = self.execute_report(run_report(format="tor-arena-run-v2"))
        self.assertEqual(result["status"], "failure")
        self.assertIn("Unexpected arena report identity", result["message"])
        self.assertNotIn("report", result)

    def test_ordered_strike_format_rejects_previous_primary_denominator(self):
        result = self.execute_report(run_report(format="tor-arena-run-v3"))
        self.assertEqual(result["status"], "failure")
        self.assertIn("Unexpected arena report identity", result["message"])
        self.assertNotIn("report", result)

    def test_wide_scalar_report_rejects_previous_revision(self):
        result = self.execute_report(run_report(format="tor-arena-run-v4"))
        self.assertEqual(result["status"], "failure")
        self.assertIn("Unexpected arena report identity", result["message"])

    def test_current_report_requires_exact_decimal_health_and_hit_dice(self):
        for field in ("hit_dice", "health", "maximum_health", "injury"):
            for invalid in (1, True, "01", "-1", "+1", "１", str(2**64)):
                with self.subTest(field=field, value=invalid):
                    report = run_report()
                    actor = report["participants"][0]
                    (actor if field == "hit_dice" else actor["initial"])[field] = invalid
                    self.assertEqual(self.execute_report(report)["status"], "failure")
        for value in (65_535, 65_536, 65_537, 2**53 + 1, 2**64 - 1):
            report = run_report()
            actor = report["participants"][0]
            actor["hit_dice"] = str(value)
            for state in (actor["initial"], actor["final_state"]):
                state.update(health=str(value), maximum_health=str(value), injury="0")
            before = copy.deepcopy(report)
            self.assertEqual(self.execute_report(report), {"status": "ok", "report": report})
            self.assertEqual(report, before)

    def test_execute_accepts_current_success_report_revision(self):
        report = run_report()
        self.assertEqual(self.execute_report(report), {"status": "ok", "report": report})

    def test_ordered_strikes_keep_primary_counts_distinct_from_actions_and_pairing(self):
        from arena_search import trial_record
        report = run_report()
        report["actions"] = "2"
        report["participants"][0].update(attack_checks="3", hits="3", damage_dealt="27",
            abilities={"basic_melee": {"resolutions": "3", "applications": "3"}})
        self.assertEqual(self.execute_report(report), {"status": "ok", "report": report})
        observations = []
        for orientation in ("forward", "mirrored"):
            for variant in ("baseline", "candidate"):
                measured = copy.deepcopy(report)
                measured["termination"] = {"type": "elimination", "winner": "blue" if variant == "candidate" else "red"}
                measured["participants"][0]["abilities"]["basic_melee"]["resolutions"] = "16" if variant == "candidate" else "1"
                observations.append({"seed": "42", "orientation": orientation, "variant": variant,
                                     "status": "ok", "report": measured})
        summary = summarize(observations, "blue")
        self.assertEqual(summary["paired_seeds"], 1)
        self.assertEqual(summary["mean_candidate_win_difference"], 1.)
        # Campaign scoring uses paired encounter outcomes, never per-strike use counts.
        trial = trial_record({"maximum_excluded_fraction": 0, "constraints": []}, {}, summary, True, 1)
        self.assertEqual(trial["value"], 1.)
        self.assertEqual(trial["constraints"], {"validation": 0, "failures": 0, "coverage": 0})

    def test_nonzero_exit_retains_only_bounded_failure_diagnostics(self):
        for format in ("tor-arena-run-v2", "tor-arena-run-v3", "tor-arena-run-v4", "tor-arena-run-v5"):
            with self.subTest(format=format):
                report = {"format": format, "status": "failure", "message": "x" * 1001}
                self.assertEqual(self.execute_report(report, 2),
                                 {"status": "failure", "exit_code": 2, "message": "x" * 1000})
                # Even a success-shaped object cannot override a failed exit.
                self.assertEqual(self.execute_report(run_report(format=format), 2),
                                 {"status": "failure", "exit_code": 2,
                                  "message": "Evaluation failed"})

    def test_execute_report_size_bound_is_unchanged(self):
        result = self.execute_report({"message": "x" * (2 * 1024 * 1024)}, 2)
        self.assertEqual(result, {"status": "failure",
                                  "message": "Evaluation report exceeds bounded size"})

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


    def test_current_success_record_is_accepted(self):
        import arena_evaluation as arena
        identity = ("42", "forward", "baseline")
        record = dict(zip(("seed", "orientation", "variant"), identity),
                      status="ok", report=run_report())
        arena.validate_record(record, identity)

    def test_resume_rejects_previous_success_revision_in_committed_and_pending_records(self):
        import arena_evaluation as arena
        for pending, obsolete in ((False, "tor-arena-run-v2"), (True, "tor-arena-run-v2"),
                                  (False, "tor-arena-run-v3"), (True, "tor-arena-run-v3"),
                                  (False, "tor-arena-run-v4"), (True, "tor-arena-run-v4")):
            with self.subTest(pending=pending, obsolete=obsolete), tempfile.TemporaryDirectory() as directory:
                binary, plan, output = self.fixture(Path(directory))
                failure = {"status": "failure", "message": "Evaluation timeout"}
                write = arena.write_json

                def interrupt(path, value, **kwargs):
                    if pending and path.parent.name == "runs":
                        raise InterruptedError("stop before record publication")
                    return write(path, value, **kwargs)

                results = [failure, InterruptedError("stop after first record")]
                with patch.object(arena, "execute", side_effect=results), patch.object(
                        arena, "write_json", side_effect=interrupt):
                    with self.assertRaises(InterruptedError):
                        arena.run_batch(binary, plan, output)
                checkpoint = arena.read_json(output / "partial.json")
                identity = arena.identities(checkpoint)[0]
                record = dict(zip(("seed", "orientation", "variant"), identity),
                              status="ok", report=run_report(format=obsolete))
                header = {key: value for key, value in checkpoint.items()
                          if key not in {"completed_runs", "chain_sha256", "pending"}}
                record_hash = arena.chain_hash(record, arena.chain_hash(header))
                if pending:
                    checkpoint["pending"] = {"run": record, "sha256": record_hash}
                else:
                    write(arena.record_path(output, identity), record)
                    checkpoint["chain_sha256"] = record_hash
                write(output / "partial.json", checkpoint)
                # Matching checksums must not turn an obsolete success into evidence.
                before = {p.relative_to(output): p.read_bytes()
                          for p in output.rglob("*") if p.is_file()}
                with patch.object(arena, "execute") as execute:
                    with self.assertRaisesRegex(ValueError, "Invalid checkpoint report identity"):
                        arena.resume_batch(binary, output)
                execute.assert_not_called()
                self.assertEqual(before, {p.relative_to(output): p.read_bytes()
                                          for p in output.rglob("*") if p.is_file()})

    def test_completed_load_resume_and_replay_reject_current_numeric_health(self):
        import arena_evaluation as arena
        with tempfile.TemporaryDirectory() as directory:
            binary, plan, output = self.fixture(Path(directory))
            def current(binary, package, value, timeout, scratch):
                return {"status": "ok", "report": run_report(value)}
            with patch.object(arena, "execute", side_effect=current):
                batch = arena.run_batch(binary, plan, output)
            record = batch["runs"][0]
            record["report"]["participants"][0]["final_state"]["health"] = 10
            arena.write_json(arena.record_path(output, arena.identities(batch)[0]), record)
            arena.write_json(output / "batch.json", batch)
            before = {path.relative_to(output): path.read_bytes()
                      for path in output.rglob("*") if path.is_file()}
            for operation in (arena.load_complete, arena.resume_batch, arena.replay):
                with self.subTest(operation=operation.__name__), patch.object(arena, "execute") as execute:
                    with self.assertRaisesRegex(ValueError, "canonical unsigned decimal"):
                        operation(binary, output)
                    execute.assert_not_called()
            self.assertEqual(before, {path.relative_to(output): path.read_bytes()
                                      for path in output.rglob("*") if path.is_file()})

    def test_completed_resume_and_replay_reject_previous_success_revision(self):
        import arena_evaluation as arena
        for obsolete in ("tor-arena-run-v2", "tor-arena-run-v3", "tor-arena-run-v4"):
            with self.subTest(obsolete=obsolete), tempfile.TemporaryDirectory() as directory:
                binary, plan, output = self.fixture(Path(directory))
                failure = {"status": "failure", "message": "Evaluation timeout"}
                with patch.object(arena, "execute", return_value=failure):
                    batch = arena.run_batch(binary, plan, output)
                identity = arena.identities(batch)[0]
                batch["runs"][0] = dict(zip(("seed", "orientation", "variant"), identity),
                                        status="ok", report=run_report(format=obsolete))
                batch["summary"] = arena.summarize(batch["runs"], batch["faction"])
                arena.write_json(arena.record_path(output, identity), batch["runs"][0])
                arena.write_json(output / "batch.json", batch)
                for operation in (arena.load_complete, arena.resume_batch, arena.replay):
                    with self.subTest(operation=operation.__name__), patch.object(arena, "execute") as execute:
                        with self.assertRaisesRegex(ValueError, "Invalid checkpoint report identity"):
                            operation(binary, output)
                    execute.assert_not_called()


class ArenaParticipantContractTests(unittest.TestCase):
    def test_explicit_per_arm_identities_finish_load_and_replay_without_merging_class_actors(self):
        import arena_evaluation as arena
        with tempfile.TemporaryDirectory() as directory:
            binary, plan, output = evaluation_fixture(Path(directory), participant_rosters=distinct_rosters())
            with patch.object(arena, "execute", side_effect=distinct_report) as execute:
                batch = arena.run_batch(binary, plan, output)
                self.assertEqual(execute.call_count, 8)
                self.assertEqual(arena.load_complete(binary, output), batch)
                self.assertEqual(arena.resume_batch(binary, output), batch)
                self.assertEqual(arena.replay(binary, output), (True, 8))
                self.assertEqual(execute.call_count, 16)
            self.assertEqual(batch["participant_rosters"], distinct_rosters())
            self.assertEqual(batch["summary"]["terminations"], {
                "baseline": {"action_limit": 4}, "candidate": {"action_limit": 4}})
            self.assertEqual(batch["summary"]["paired_seeds"], 0)
            self.assertEqual(batch["summary"]["excluded_seeds"], ["42", "43"])

    def test_absent_contract_keeps_the_original_same_actor_identity_requirement(self):
        import arena_evaluation as arena
        with tempfile.TemporaryDirectory() as directory:
            binary, plan, output = evaluation_fixture(Path(directory))
            with patch.object(arena, "execute", side_effect=distinct_report):
                with self.assertRaisesRegex(ValueError, "identities"):
                    arena.run_batch(binary, plan, output)
            self.assertFalse((output / "batch.json").exists())

    def test_malformed_declared_rosters_reject_before_any_execution_or_output(self):
        import arena_evaluation as arena
        invalid = [None, [], {}, {"baseline": distinct_rosters()["baseline"]},
                   {**distinct_rosters(), "extra": []}]
        for actor in (0, 1, "0", "01", "+1", "-1", "18446744073709551616", "１"):
            contract = distinct_rosters()
            contract["baseline"][0]["actor"] = actor
            invalid.append(contract)
        for change in ("duplicate", "extra_field", "missing_faction", "unequal_counts", "missing_evaluated"):
            contract = distinct_rosters()
            if change == "duplicate":
                contract["baseline"].append(copy.deepcopy(contract["baseline"][0]))
            elif change == "extra_field":
                contract["baseline"][0]["unknown"] = True
            elif change == "missing_faction":
                del contract["candidate"][0]["faction"]
            elif change == "unequal_counts":
                contract["candidate"].append({"actor": "4", "faction": "red"})
            else:
                for rows in contract.values():
                    rows[0]["faction"] = "other"
            invalid.append(contract)
        for contract in invalid:
            with self.subTest(contract=contract), tempfile.TemporaryDirectory() as directory:
                binary, plan, output = evaluation_fixture(Path(directory), participant_rosters=contract)
                with patch.object(arena, "execute") as execute, self.assertRaises(ValueError):
                    arena.run_batch(binary, plan, output)
                execute.assert_not_called()
                self.assertFalse(output.exists())

    def test_duplicate_noncanonical_or_unexpected_report_actor_rejects_before_pending_commit(self):
        import arena_evaluation as arena
        for change in ("duplicate", "noncanonical", "unexpected", "faction"):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as directory:
                binary, plan, output = evaluation_fixture(Path(directory), participant_rosters=distinct_rosters())
                result = distinct_report(binary, Path("baseline-forward"), "42", 1, output)
                rows = result["report"]["participants"]
                if change == "duplicate":
                    rows.append(copy.deepcopy(rows[0]))
                elif change == "faction":
                    rows[0]["faction"] = "red"
                else:
                    rows[0]["actor"] = "01" if change == "noncanonical" else "4"
                with patch.object(arena, "execute", return_value=result), self.assertRaises(ValueError):
                    arena.run_batch(binary, plan, output)
                checkpoint = arena.read_json(output / "partial.json")
                self.assertEqual(checkpoint["completed_runs"], 0)
                self.assertIsNone(checkpoint["pending"])
                self.assertFalse((output / "batch.json").exists())

    def test_resume_rechecks_committed_and_pending_identity_even_with_matching_checksums(self):
        import arena_evaluation as arena
        for pending in (False, True):
            with self.subTest(pending=pending), tempfile.TemporaryDirectory() as directory:
                binary, plan, output = evaluation_fixture(Path(directory), participant_rosters=distinct_rosters())
                failure = {"status": "failure", "message": "Evaluation timeout"}
                write = arena.write_json
                def interrupt(path, value, **kwargs):
                    if pending and path.parent.name == "runs":
                        raise InterruptedError("stop before record publication")
                    return write(path, value, **kwargs)
                with patch.object(arena, "execute", side_effect=[failure, InterruptedError("stop")]), \
                        patch.object(arena, "write_json", side_effect=interrupt):
                    with self.assertRaises(InterruptedError):
                        arena.run_batch(binary, plan, output)
                checkpoint = arena.read_json(output / "partial.json")
                identity = arena.identities(checkpoint)[0]
                result = distinct_report(binary, Path("candidate-forward"), "42", 1, output)
                record = dict(zip(("seed", "orientation", "variant"), identity), **result)
                header = {key: value for key, value in checkpoint.items()
                          if key not in {"completed_runs", "chain_sha256", "pending"}}
                checksum = arena.chain_hash(record, arena.chain_hash(header))
                if pending:
                    checkpoint["pending"] = {"run": record, "sha256": checksum}
                else:
                    write(arena.record_path(output, identity), record)
                    checkpoint["chain_sha256"] = checksum
                write(output / "partial.json", checkpoint)
                with patch.object(arena, "execute") as execute:
                    with self.assertRaisesRegex(ValueError, "identities"):
                        arena.resume_batch(binary, output)
                execute.assert_not_called()

    def test_declared_rosters_are_bound_into_resume_checksum_header(self):
        import arena_evaluation as arena
        with tempfile.TemporaryDirectory() as directory:
            binary, plan, output = evaluation_fixture(Path(directory), participant_rosters=distinct_rosters())
            failure = {"status": "failure", "message": "Evaluation timeout"}
            with patch.object(arena, "execute", side_effect=[failure, InterruptedError("stop")]):
                with self.assertRaises(InterruptedError):
                    arena.run_batch(binary, plan, output)
            checkpoint = arena.read_json(output / "partial.json")
            checkpoint["participant_rosters"]["baseline"][0]["actor"] = "4"
            arena.write_json(output / "partial.json", checkpoint)
            with patch.object(arena, "execute") as execute, self.assertRaisesRegex(ValueError, "checksum"):
                arena.resume_batch(binary, output)
            execute.assert_not_called()

    def test_completed_load_resume_and_replay_reject_tampered_contract_or_duplicate_report(self):
        import arena_evaluation as arena
        for change in ("contract", "duplicate_report", "removed_contract"):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as directory:
                binary, plan, output = evaluation_fixture(Path(directory), participant_rosters=distinct_rosters())
                with patch.object(arena, "execute", side_effect=distinct_report):
                    batch = arena.run_batch(binary, plan, output)
                if change == "contract":
                    batch["participant_rosters"]["baseline"][0]["actor"] = "4"
                elif change == "removed_contract":
                    del batch["participant_rosters"]
                else:
                    rows = batch["runs"][0]["report"]["participants"]
                    rows.append(copy.deepcopy(rows[0]))
                    arena.write_json(arena.record_path(output, arena.identities(batch)[0]), batch["runs"][0])
                arena.write_json(output / "batch.json", batch)
                for operation in (arena.load_complete, arena.resume_batch, arena.replay):
                    with self.subTest(operation=operation.__name__), patch.object(arena, "execute") as execute:
                        with self.assertRaises(ValueError):
                            operation(binary, output)
                        execute.assert_not_called()

    def test_failed_runs_keep_their_failure_without_inventing_participant_rows(self):
        import arena_evaluation as arena
        with tempfile.TemporaryDirectory() as directory:
            binary, plan, output = evaluation_fixture(Path(directory), participant_rosters=distinct_rosters())
            failure = {"status": "failure", "message": "Evaluation timeout"}
            with patch.object(arena, "execute", return_value=failure):
                batch = arena.run_batch(binary, plan, output)
                self.assertEqual(arena.replay(binary, output), (True, 8))
            self.assertEqual(batch["summary"]["terminations"], {
                "baseline": {"failure": 4}, "candidate": {"failure": 4}})
