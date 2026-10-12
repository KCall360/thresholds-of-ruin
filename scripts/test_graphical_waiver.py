"""Explicit local display exclusions never widen unittest selection."""
import io
import json
from pathlib import Path
import tempfile
import unittest
from contextlib import redirect_stderr
from unittest.mock import patch

import graphical_waiver
import test_runner
import verify
from test_runner import TimedRunner


class GraphicalWaiverContract(unittest.TestCase):
    def test_waived_base_class_cannot_skip_unlisted_subclass_methods(self):
        class Native(unittest.TestCase):
            graphical = True

            def test_window(self):
                self.fail("native body must not execute")

        class Headless(Native):
            def test_new_headless(self):
                self.fail("unlisted subclass sentinel")

        native = Native("test_window")
        child = Headless("test_new_headless")
        rows = []
        with graphical_waiver.apply(unittest.TestSuite([native, child]), [native.id()], "no display") as suite:
            result = TimedRunner(stream=io.StringIO(), sink=rows.append).run(suite)
        self.assertFalse(result.wasSuccessful(), "the unlisted subclass must execute and fail")
        self.assertEqual({row["test_id"]: row["outcome"] for row in rows if row["kind"] == "test"},
                         {native.id(): "waived", child.id(): "failed"})

    def test_ordinary_none_reason_skip_is_never_reported_as_a_waiver(self):
        class Ordinary(unittest.TestCase):
            @unittest.skip(None)
            def test_skipped(self):
                pass

        rows = []
        TimedRunner(stream=io.StringIO(), sink=rows.append).run(
            unittest.TestSuite([Ordinary("test_skipped")]))
        self.assertEqual(rows[0]["outcome"], "skipped")
        self.assertEqual(rows[-1]["waived"], 0)

    def test_exact_method_keeps_headless_and_new_methods_in_mixed_class(self):
        ran = []

        class Mixed(unittest.TestCase):
            graphical = True

            def test_graphical(self):
                raise AssertionError("native method must be explicitly waived")

            def test_headless(self):
                ran.append("headless")

            def test_new(self):
                ran.append("new")

        tests = [Mixed(name) for name in ("test_graphical", "test_headless", "test_new")]
        rows = []
        with graphical_waiver.apply(unittest.TestSuite(tests), [tests[0].id()], "no display") as suite:
            result = TimedRunner(stream=io.StringIO(), sink=rows.append).run(suite)
        self.assertTrue(result.wasSuccessful())
        self.assertEqual(ran, ["headless", "new"])
        self.assertEqual([row["outcome"] for row in rows if row["kind"] == "test"],
                         ["passed", "passed", "waived"])
        self.assertEqual(rows[-1]["waived"], 1)
        self.assertNotIn("__unittest_skip__", Mixed.__dict__)
        self.assertNotIn("test_graphical", tests[0].__dict__)

    def test_all_waived_methods_do_not_start_graphical_class_fixture(self):
        class Native(unittest.TestCase):
            graphical = True

            @classmethod
            def setUpClass(cls):
                raise AssertionError("no native setup for an entirely waived selection")

            def test_window(self):
                self.fail("must not start")

        test = Native("test_window")
        with graphical_waiver.apply(unittest.TestSuite([test]), [test.id()], "no display") as suite:
            result = TimedRunner(stream=io.StringIO()).run(suite)
        self.assertTrue(result.wasSuccessful())
        self.assertEqual(len(result.skipped), 1)

    def test_unknown_duplicate_and_headless_ids_fail_closed(self):
        class Ordinary(unittest.TestCase):
            def test_headless(self):
                pass

        test = Ordinary("test_headless")
        for names in [["test_missing.Native.test_window"], [test.id()], [test.id(), test.id()]]:
            with self.subTest(names=names), self.assertRaises(ValueError):
                with graphical_waiver.apply(unittest.TestSuite([test]), names, "no display"):
                    self.fail("invalid waiver was accepted")

    def test_custom_suite_execution_cannot_be_silently_flattened(self):
        class CustomSuite(unittest.TestSuite):
            def run(self, result, debug=False):
                raise AssertionError("custom suite execution must not be discarded")

        for suite in [CustomSuite(), unittest.TestSuite([CustomSuite()])]:
            with self.subTest(suite=suite), self.assertRaises(ValueError):
                with graphical_waiver.apply(suite, [], "no display"):
                    self.fail("custom suite accepted")

    def test_manifest_is_strict_and_has_no_globs_or_empty_reason(self):
        valid = {"reason": "XOpenDisplay unavailable in authorized sandbox", "tests": [
            "test_sample.Native.test_window"]}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "waivers.json"
            path.write_text(json.dumps(valid), encoding="utf-8")
            self.assertEqual(graphical_waiver.read_manifest(path), valid)
            for invalid in [dict(valid, extra=True), dict(valid, tests=[]),
                            dict(valid, tests=["test_sample.Native.*"]), dict(valid, reason=""),
                            dict(valid, tests=valid["tests"] * 2)]:
                path.write_text(json.dumps(invalid), encoding="utf-8")
                with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                    graphical_waiver.read_manifest(path)
            for source in ['{"reason":"one","reason":"two","tests":["test_sample.Native.test_window"]}',
                           ' ' * 256_001]:
                path.write_text(source, encoding="utf-8")
                with self.assertRaises(ValueError):
                    graphical_waiver.read_manifest(path)

    def test_both_clis_reject_the_flag_in_ci_before_reading_a_manifest(self):
        with patch.dict("os.environ", {"CI": "true"}):
            for main, args in [(test_runner.main, []), (verify.main, ["push"])]:
                with self.subTest(main=main), redirect_stderr(io.StringIO()) as output:
                    with self.assertRaises(SystemExit) as error:
                        main([*args, "--waive-local-graphical", "missing.json"])
                self.assertEqual(error.exception.code, 2)
                self.assertIn("local only", output.getvalue())
        with patch.dict("os.environ", {}, clear=True), redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as error:
                verify.main(["full", "--ci-profile", "debug", "--waive-local-graphical", "missing.json"])
            self.assertEqual(error.exception.code, 2)

    def test_gate_requires_exact_waiver_accounting_not_just_zero_exit(self):
        name = "test_sample.Native.test_window"
        records = [
            {"kind": "local_graphical_waiver", "tests": [name]},
            {"kind": "test", "test_id": name, "outcome": "waived"},
            {"kind": "suite", "waived": 1, "waived_tests": [name]},
        ]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "evidence.jsonl"
            path.write_text("\n".join(map(json.dumps, records)), encoding="utf-8")
            self.assertEqual(verify.graphical_waiver_evidence(path, [name]), [name])
            for invalid in [[], records[:1], [records[0], records[2]],
                            [records[0], records[1], records[1], records[2]],
                            [*records[:2], {"kind": "test", "test_id": "test_other.Headless.test_new",
                                           "outcome": "skipped", "skip_reason": "LOCAL GRAPHICAL WAIVER: inherited"},
                             records[2]],
                            [records[0], dict(records[1], outcome="passed"), records[2]]]:
                path.write_text("\n".join(map(json.dumps, invalid)), encoding="utf-8")
                with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                    verify.graphical_waiver_evidence(path, [name])


if __name__ == "__main__":
    unittest.main()
