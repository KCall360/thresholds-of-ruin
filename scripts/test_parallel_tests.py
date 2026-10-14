"""The parallel runner changes when tests run, never which tests run or how they are counted."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
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


class Durations(unittest.TestCase):
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
