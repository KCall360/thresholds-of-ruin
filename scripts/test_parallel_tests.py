"""The parallel runner changes when tests run, never which tests run or how they are counted."""
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import uuid

import parallel_tests
import verify

SAMPLE = '''
import os
import unittest


class Plain(unittest.TestCase):
    def test_passes(self):
        pass

    def test_fails(self):
        self.assertEqual(1, 2)

    def test_errors(self):
        raise RuntimeError("boom")

    def test_subtests_fail_twice(self):
        for value in (1, 2, 3):
            with self.subTest(value=value):
                self.assertEqual(value, 3)

    @unittest.skip("not here")
    def test_skipped(self):
        pass


class Fixture(unittest.TestCase):
    shared = None

    @classmethod
    def setUpClass(cls):
        cls.shared = os.getpid()

    def test_first(self):
        self.assertIsNotNone(self.shared)

    def test_second_sees_the_same_fixture_process(self):
        self.assertEqual(self.shared, os.getpid())


class Native(unittest.TestCase):
    graphical = True

    def test_window(self):
        pass

    def test_other_window(self):
        pass
'''

CRASH = '''
import os
import unittest


class Crash(unittest.TestCase):
    def test_exits_the_interpreter(self):
        os._exit(3)
'''


PASSING = '''
import unittest


class Passing(unittest.TestCase):
    def test_one(self):
        pass

    def test_two(self):
        pass
'''

# Each sample misbehaves through the worker's own timing file, as a broken or
# interrupted worker would, while every test body passes.
MISBEHAVING = '''
import atexit
import json
import os
import unittest


def record(text):
    with open(os.environ["TOR_TEST_TIMINGS"], "a", encoding="utf-8") as timings:
        timings.write(text)


class Truncated(unittest.TestCase):
    def test_leaves_a_truncated_record(self):
        record('{"kind": "te' + "\\n")


class Malformed(unittest.TestCase):
    def test_leaves_a_non_record_line(self):
        record("[1, 2]" + "\\n")


class Duplicate(unittest.TestCase):
    def test_is_recorded_twice(self):
        record(json.dumps({"kind": "test", "test_id": self.id(), "outcome": "passed", "seconds": 0}) + "\\n")


class Foreign(unittest.TestCase):
    def test_records_a_test_it_was_not_given(self):
        record(json.dumps({"kind": "test", "test_id": "elsewhere.Other.test_x", "outcome": "passed",
                           "seconds": 0}) + "\\n")


class ExitsWithFailure(unittest.TestCase):
    def test_passes_then_the_worker_exits_1(self):
        atexit.register(os._exit, 1)


class FailsThenTeardownExits(unittest.TestCase):
    def test_fails(self):
        self.fail("recorded failure")

    @classmethod
    def tearDownClass(cls):
        os._exit(1)


class PassesThenTeardownExitsCleanly(unittest.TestCase):
    def test_passes(self):
        pass

    @classmethod
    def tearDownClass(cls):
        os._exit(0)


class ExtraSummary(unittest.TestCase):
    def test_writes_a_second_successful_summary(self):
        record(json.dumps({"kind": "suite", "tests_run": 1, "successful": True, "failures": 0,
                           "errors": 0, "skipped": 0}) + "\\n")
'''


class Planning(unittest.TestCase):
    def setUp(self):
        self.module = f"test_sample_{uuid.uuid4().hex}"
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        Path(directory.name, self.module + ".py").write_text(SAMPLE, encoding="utf-8")
        sys.path.insert(0, directory.name)
        self.addCleanup(sys.path.remove, directory.name)
        self.suite = unittest.defaultTestLoader.loadTestsFromName(self.module)
        self.tests = list(parallel_tests.flatten(self.suite))

    def test_fixture_classes_stay_together_and_graphical_tests_get_their_own_lane(self):
        headless, graphical = parallel_tests.plan(self.tests, {}, workers=1)
        batches = [batch.ids for batch in headless]
        fixture = [f"{self.module}.Fixture.test_first", f"{self.module}.Fixture.test_second_sees_the_same_fixture_process"]
        self.assertTrue(any(set(fixture) <= set(ids) for ids in batches))
        self.assertEqual(sorted(i for ids in batches for i in ids),
                         sorted(t.id() for t in self.tests if ".Native." not in t.id()))
        self.assertEqual(sorted(i for b in graphical for i in b.ids),
                         [f"{self.module}.Native.test_other_window", f"{self.module}.Native.test_window"])

    def test_priority_tests_start_first_then_the_longest(self):
        durations = {f"{self.module}.Plain.test_passes": 60.0, f"{self.module}.Plain.test_fails": 30.0}
        headless, _ = parallel_tests.plan(self.tests, durations, workers=1)
        self.assertEqual(headless[0].ids, [f"{self.module}.Plain.test_passes"])
        self.assertEqual(headless[1].ids, [f"{self.module}.Plain.test_fails"])
        headless, _ = parallel_tests.plan(self.tests, durations, {self.module}, workers=1)
        self.assertTrue(all(batch.priority for batch in headless))

    def test_every_selected_test_is_planned_exactly_once(self):
        durations = {test.id(): 1.0 for test in self.tests}
        for workers in (1, 2, 8):
            with self.subTest(workers=workers):
                headless, graphical = parallel_tests.plan(self.tests, durations, workers=workers)
                planned = [i for batch in headless + graphical for i in batch.ids]
                self.assertEqual(sorted(planned), sorted(t.id() for t in self.tests))


class Execution(unittest.TestCase):
    """Run real worker processes over a sample module, as verify.py does."""

    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.directory = Path(directory.name)
        self.module = f"test_sample_{uuid.uuid4().hex}"
        (self.directory / (self.module + ".py")).write_text(SAMPLE, encoding="utf-8")

    def runner(self, *arguments, module=None, timings=None):
        env = {**os.environ, "PYTHONUTF8": "1", "PYTHONIOENCODING": "utf-8"}
        env.pop("TOR_TEST_PRIORITY", None)
        if timings:
            env["TOR_TEST_TIMINGS"] = str(timings)
        else:
            env.pop("TOR_TEST_TIMINGS", None)
        return subprocess.run([sys.executable, str(Path(parallel_tests.__file__).with_name("test_runner.py")),
                               *arguments, module or self.module],
                              cwd=self.directory, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                              encoding="utf-8", timeout=300)

    def rows(self, path):
        return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]

    def test_parallel_counts_and_failure_lines_match_a_serial_run(self):
        serial = self.runner()
        parallel = self.runner("--workers", "3")
        self.assertEqual(serial.returncode, 1)
        self.assertEqual(parallel.returncode, 1)
        last = lambda text: text.strip().splitlines()[-1]
        self.assertEqual(last(serial.stdout), "FAILED (failures=3, errors=1, skipped=1)")
        self.assertEqual(last(parallel.stdout), last(serial.stdout))
        self.assertEqual(sorted(verify.python_failures(parallel.stdout)), sorted(verify.python_failures(serial.stdout)))
        self.assertIn("Ran 9 tests", parallel.stdout)

    def test_timings_merge_into_one_file_ending_with_one_suite_record(self):
        timings = self.directory / "timings.jsonl"
        self.runner("--workers", "2", timings=timings)
        rows = self.rows(timings)
        tests = [row for row in rows if row["kind"] == "test"]
        self.assertEqual(len(tests), 9)
        self.assertEqual(len({row["test_id"] for row in tests}), 9)
        self.assertEqual([row["kind"] for row in rows].count("suite"), 1)
        self.assertEqual(rows[-1]["kind"], "suite")
        self.assertEqual((rows[-1]["tests_run"], rows[-1]["failures"], rows[-1]["errors"], rows[-1]["skipped"]),
                         (9, 3, 1, 1))
        self.assertFalse(rows[-1]["successful"])

    def test_a_worker_that_dies_fails_its_tests_visibly(self):
        crash = f"test_crash_{uuid.uuid4().hex}"
        (self.directory / (crash + ".py")).write_text(CRASH, encoding="utf-8")
        timings = self.directory / "timings.jsonl"
        result = self.runner("--workers", "2", module=crash, timings=timings)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(verify.python_failures(result.stdout), [f"{crash}.Crash.test_exits_the_interpreter"])
        self.assertIn("worker exited with status 3", result.stdout)
        self.assertEqual(self.rows(timings)[-1]["errors"], 1)

    def test_graphical_waivers_keep_exact_verifier_accounting(self):
        waived = [f"{self.module}.Native.test_window"]
        manifest = self.directory / "waiver.json"
        manifest.write_text(json.dumps({"reason": "no display in this sandbox", "tests": waived}), encoding="utf-8")
        timings = self.directory / "timings.jsonl"
        os.environ.pop("CI", None)
        result = self.runner("--workers", "2", "--waive-local-graphical", str(manifest), timings=timings)
        self.assertIn("LOCAL GRAPHICAL WAIVERS (not run)", result.stdout)
        self.assertEqual(verify.graphical_waiver_evidence(timings, waived), waived)

    def test_fail_fast_stops_starting_tests_after_a_failure(self):
        timings = self.directory / "timings.jsonl"
        result = self.runner("--workers", "2", "--failfast", timings=timings)
        self.assertEqual(result.returncode, 1)
        suite = self.rows(timings)[-1]
        self.assertFalse(suite["successful"])
        self.assertLess(suite["tests_run"], 9)


class WorkerFailures(unittest.TestCase):
    """A run succeeds only with exactly one passing result for every selected test."""

    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.directory = Path(directory.name)
        self.passing = f"test_passing_{uuid.uuid4().hex}"
        self.misbehaving = f"test_misbehaving_{uuid.uuid4().hex}"
        (self.directory / (self.passing + ".py")).write_text(PASSING, encoding="utf-8")
        (self.directory / (self.misbehaving + ".py")).write_text(MISBEHAVING, encoding="utf-8")
        sys.path.insert(0, str(self.directory))
        self.addCleanup(sys.path.remove, str(self.directory))
        cwd = os.getcwd()
        os.chdir(self.directory)
        self.addCleanup(os.chdir, cwd)
        priority = os.environ.pop("TOR_TEST_PRIORITY", None)
        if priority is not None:
            self.addCleanup(os.environ.__setitem__, "TOR_TEST_PRIORITY", priority)

    def run_suite(self, name, workers=2):
        """Run in-process like test_runner does; returns (success, output, records)."""
        suite = unittest.defaultTestLoader.loadTestsFromName(name)
        stream, rows = io.StringIO(), []
        successful = parallel_tests.run(suite, workers=workers, stream=stream, sink=rows.append)
        return successful, stream.getvalue(), rows

    def assert_failed_with_complete_accounting(self, successful, output, rows, selected):
        self.assertFalse(successful, output)
        self.assertIn("FAILED", output.strip().splitlines()[-1])
        suite = rows[-1]
        self.assertEqual(suite["kind"], "suite")
        self.assertFalse(suite["successful"])
        self.assertGreater(suite["errors"], 0)
        recorded = [row["test_id"] for row in rows if row["kind"] == "test"]
        self.assertEqual(sorted(recorded), sorted(selected))

    def test_a_worker_that_cannot_launch_fails_every_test_it_was_given(self):
        selected = [f"{self.passing}.Passing.test_one", f"{self.passing}.Passing.test_two"]
        with patch("parallel_tests.subprocess.run", side_effect=OSError("cannot start worker")):
            successful, output, rows = self.run_suite(self.passing)
        self.assert_failed_with_complete_accounting(successful, output, rows, selected)
        self.assertIn("worker could not run: OSError: cannot start worker", output)
        self.assertEqual(sorted(verify.python_failures(output)), selected)

    def test_a_worker_that_exits_cleanly_without_results_fails_its_tests(self):
        selected = [f"{self.passing}.Passing.test_one", f"{self.passing}.Passing.test_two"]
        silent = subprocess.CompletedProcess(["worker"], 0, stdout="")
        with patch("parallel_tests.subprocess.run", return_value=silent):
            successful, output, rows = self.run_suite(self.passing)
        self.assert_failed_with_complete_accounting(successful, output, rows, selected)
        self.assertEqual(sorted(verify.python_failures(output)), selected)

    def test_an_error_while_reconciling_a_batch_fails_its_tests(self):
        selected = [f"{self.passing}.Passing.test_one", f"{self.passing}.Passing.test_two"]
        with patch.object(parallel_tests.Scheduler, "run_batch", side_effect=RuntimeError("bug")):
            successful, output, rows = self.run_suite(self.passing)
        self.assert_failed_with_complete_accounting(successful, output, rows, selected)
        self.assertIn("reconciliation failed: RuntimeError: bug", output)

    def test_truncated_or_malformed_records_fail_the_run(self):
        for case, problem in (("Truncated", "malformed timing record"), ("Malformed", "is not a record")):
            with self.subTest(case=case):
                successful, output, rows = self.run_suite(f"{self.misbehaving}.{case}")
                self.assertFalse(successful, output)
                self.assertIn(problem, output)
                self.assertFalse(rows[-1]["successful"])
                self.assertTrue(rows[-1]["batch_errors"])

    def test_duplicate_or_foreign_results_fail_the_run_and_are_not_double_counted(self):
        for case, problem in (("Duplicate", "duplicate result"), ("Foreign", "did not run: elsewhere")):
            with self.subTest(case=case):
                successful, output, rows = self.run_suite(f"{self.misbehaving}.{case}")
                self.assertFalse(successful, output)
                self.assertIn(problem, output)
                recorded = [row["test_id"] for row in rows if row["kind"] == "test"]
                self.assertEqual(len(recorded), 1)
                self.assertNotIn("elsewhere.Other.test_x", recorded)

    def test_a_worker_exiting_with_failure_after_passing_results_fails_the_run(self):
        successful, output, rows = self.run_suite(f"{self.misbehaving}.ExitsWithFailure")
        self.assertFalse(successful, output)
        self.assertIn("exited with status 1 although its recorded tests passed", output)
        self.assertIn("FAILED", output.strip().splitlines()[-1])
        self.assertFalse(rows[-1]["successful"])

    def test_a_recorded_failure_survives_a_worker_exiting_before_its_summary(self):
        name = f"{self.misbehaving}.FailsThenTeardownExits"
        successful, output, rows = self.run_suite(name)
        self.assertFalse(successful, output)
        self.assertIn("exited with status 1 without a suite summary", output)
        self.assertGreaterEqual(rows[-1]["failures"], 1)
        self.assertFalse(rows[-1]["successful"])
        self.assertEqual([row["outcome"] for row in rows if row["kind"] == "test"], ["failed"])
        # Never write into an enclosing run's timing records.
        env = {name: value for name, value in os.environ.items()
               if name not in ("TOR_TEST_TIMINGS", "TOR_TEST_PRIORITY")}
        cli = subprocess.run([sys.executable, str(Path(parallel_tests.__file__).with_name("test_runner.py")),
                              "--workers", "2", name], cwd=self.directory, stdout=subprocess.PIPE,
                             stderr=subprocess.STDOUT, text=True, encoding="utf-8", timeout=300,
                             env={**env, "PYTHONUTF8": "1"})
        self.assertEqual(cli.returncode, 1, cli.stdout)
        self.assertIn("FAILED (failures=1", cli.stdout.strip().splitlines()[-1])

    def test_a_passing_worker_without_a_summary_fails_the_run(self):
        successful, output, rows = self.run_suite(f"{self.misbehaving}.PassesThenTeardownExitsCleanly")
        self.assertFalse(successful, output)
        self.assertIn("exited with status 0 without a suite summary", output)
        self.assertFalse(rows[-1]["successful"])

    def test_an_extra_summary_fails_the_run(self):
        successful, output, rows = self.run_suite(f"{self.misbehaving}.ExtraSummary")
        self.assertFalse(successful, output)
        self.assertIn("wrote 2 suite summaries", output)

    def test_summary_counts_never_lower_recorded_outcomes(self):
        tests = [{"outcome": "failed", "failures": 1}, {"outcome": "error", "errors": 1},
                 {"outcome": "interrupted"}, {"outcome": "unexpected_success"},
                 {"outcome": "skipped", "subtests_skipped": 2}, {"outcome": "passed"}]
        understated = {name: 0 for name in parallel_tests.SUMMARY_FIELDS}
        counts = parallel_tests.combined_counts(understated, tests)
        self.assertEqual(counts, {"failures": 1, "errors": 2, "skipped": 3, "expected_failures": 0,
                                  "unexpected_successes": 1})
        subtests = {"failures": 3, "errors": 2, "skipped": 3, "expected_failures": 0, "unexpected_successes": 1}
        self.assertEqual(parallel_tests.combined_counts(subtests, tests), subtests)
        self.assertEqual(parallel_tests.combined_counts(None, tests), counts)
        self.assertEqual(parallel_tests.combined_counts({"failures": "lots", "errors": -1}, tests), counts)

    def test_a_clean_run_still_succeeds_with_every_test_accounted(self):
        successful, output, rows = self.run_suite(self.passing)
        self.assertTrue(successful, output)
        self.assertEqual(rows[-1]["batch_errors"], [])
        self.assertEqual(rows[-1]["tests_run"], 2)


class Durations(unittest.TestCase):
    def test_combined_runs_keep_the_slowest_observation_independent_of_order(self):
        with tempfile.TemporaryDirectory() as directory:
            sources = []
            for name, seconds in (("windows", 844.64), ("linux", 358.37)):
                source = Path(directory, name + ".jsonl")
                source.write_text(json.dumps({"kind": "test", "test_id": "a.B.test_route",
                                             "outcome": "passed", "seconds": seconds}) + "\n",
                                  encoding="utf-8")
                sources.append(source)
            for paths in (sources, list(reversed(sources))):
                destination = Path(directory, "durations.json")
                destination.write_text(json.dumps({"a.B.test_route": 1000.0}), encoding="utf-8")
                parallel_tests.record_durations(paths, destination)
                self.assertEqual(parallel_tests.load_durations(destination), {"a.B.test_route": 844.64})

    def test_measured_earned_route_has_an_explicit_scheduling_hint(self):
        test_id = ("test_rogue_earned_route_process.RogueEarnedRouteProcesses."
                   "test_fixed_seed42_earned_hd_reexecutes_with_exact_source_free_restore_and_gross_flows")
        durations = parallel_tests.load_durations()
        self.assertTrue(test_id in durations, "Refresh the measured long-route scheduling hint")

    def test_measured_omission_owner_starts_alone_without_changing_inventory(self):
        omission = ("test_floor_activation_process.OmittedRoomTopologyProcess."
                    "test_ordinary_search_traversal_and_source_free_revisit_keep_real_rooms")
        names = [
            "test_floor_activation_process.RogueStarterProcess.test_mage_ordinary_initial_kit_survives_source_deleted_restarts",
            omission,
            "test_floor_activation_process.RogueStarterProcess.test_warrior_ordinary_initial_kit_survives_source_deleted_restarts",
            "test_floor_activation_process.FloorActivationProcess.test_source_deleted_new_floor_has_one_entry_receipt_and_revisit_never_redecides",
        ]
        # Instantiate the registered owners, but never execute their process bodies.
        tests = list(parallel_tests.flatten(unittest.defaultTestLoader.loadTestsFromNames(names)))
        self.assertEqual([test.id() for test in tests], names)
        durations = parallel_tests.load_durations()
        fallback = {name: seconds for name, seconds in durations.items() if name != omission}
        for workers in (1, 2, 4):
            with self.subTest(workers=workers):
                old, old_graphical = parallel_tests.plan(tests, fallback, workers=workers)
                headless, graphical = parallel_tests.plan(tests, durations, workers=workers)
                self.assertEqual(headless[0].ids, [omission],
                                 "Register the measured omission owner instead of short-test packing")
                self.assertGreaterEqual(headless[0].seconds, parallel_tests.BATCH_SECONDS)
                self.assertEqual(sorted(test_id for batch in headless for test_id in batch.ids),
                                 sorted(names))
                self.assertEqual(sorted(test_id for batch in old for test_id in batch.ids),
                                 sorted(names))
                self.assertEqual(graphical, old_graphical)
                self.assertEqual(graphical, [])

    def test_invalid_observations_leave_the_destination_unchanged(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory, "run.jsonl")
            destination = Path(directory, "durations.json")
            original = '{"a.B.test_old": 2.0}\n'
            for seconds in (-1, float("nan"), float("inf"), float("-inf"), True, "3", None):
                with self.subTest(seconds=seconds):
                    destination.write_text(original, encoding="utf-8")
                    rows = [{"kind": "test", "test_id": "a.B.test_valid", "outcome": "passed", "seconds": 3},
                            {"kind": "test", "test_id": "a.B.test_invalid", "outcome": "passed", "seconds": seconds}]
                    source.write_text("\n".join(map(json.dumps, rows)) + "\n", encoding="utf-8")
                    with self.assertRaisesRegex(ValueError, "finite nonnegative"):
                        parallel_tests.record_durations([source], destination)
                    self.assertEqual(destination.read_text(encoding="utf-8"), original)

    def test_a_measured_long_method_starts_alone_before_unknown_short_methods(self):
        class Example(unittest.TestCase):
            def test_long(self):
                pass

            def test_short(self):
                pass

        tests = [Example("test_short"), Example("test_long")]
        headless, graphical = parallel_tests.plan(tests, {tests[1].id(): 844.64}, workers=2)
        self.assertEqual(headless[0].ids, [tests[1].id()])
        self.assertEqual(sorted(i for b in headless for i in b.ids), sorted(t.id() for t in tests))
        self.assertEqual(graphical, [])

    def test_recording_keeps_passed_tests_and_the_latest_value(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        source = Path(directory.name, "run.jsonl")
        source.write_text("\n".join(json.dumps(row) for row in [
            {"kind": "test", "test_id": "a.B.test_c", "outcome": "passed", "seconds": 1.234},
            {"kind": "test", "test_id": "a.B.test_d", "outcome": "failed", "seconds": 9.0},
            {"kind": "suite", "seconds": 10.0}]) + "\n", encoding="utf-8")
        destination = Path(directory.name, "durations.json")
        destination.write_text(json.dumps({"a.B.test_c": 5.0, "x.Y.test_z": 2.0}), encoding="utf-8")
        parallel_tests.record_durations([source], destination)
        self.assertEqual(json.loads(destination.read_text(encoding="utf-8")),
                         {"a.B.test_c": 1.23, "x.Y.test_z": 2.0})

    def test_the_committed_durations_are_numbers_keyed_by_test_id(self):
        durations = parallel_tests.load_durations()
        self.assertTrue(durations)
        self.assertTrue(all(isinstance(value, (int, float)) and value >= 0 for value in durations.values()))
        self.assertTrue(all(key.count(".") >= 2 for key in durations))


if __name__ == "__main__":
    unittest.main()
