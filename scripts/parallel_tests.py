"""Run a unittest selection across worker processes without changing what runs.

Every selected test runs exactly once, in a fresh `test_runner.py` worker, with
ordinary unittest outcome accounting. Scheduling only changes when and where:

- Classes with their own class or module fixtures stay together in one batch.
  Other tests are scheduled individually and short ones are packed into batches.
- Graphical classes (`graphical = True`) run one batch at a time in their own
  lane, so native windows never compete with each other for input focus.
- Batches from priority modules (`TOR_TEST_PRIORITY`, the tests a change touched)
  start first; the rest start longest first using recorded durations, which only
  affect order, never selection or deadlines.

The combined output keeps unittest's `FAIL:`/`ERROR:` lines and ends with one
`Ran`/`OK`/`FAILED` summary. With `TOR_TEST_TIMINGS`, worker records are merged
into that file followed by a single suite record, as a serial run writes.
"""
from collections import Counter
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading
import time
import unittest
import uuid

import graphical_waiver

SCRIPTS = Path(__file__).resolve().parent
ROOT = SCRIPTS.parent
DURATIONS = SCRIPTS / "test_durations.json"
# Batches of short tests amortize worker start-up (interpreter, imports, a
# no-op build check); this is a packing target, not a time limit.
BATCH_SECONDS = 20.0
DEFAULT_SECONDS = {"process": 3.0, "tool": 0.05}
SUMMARY_FIELDS = ("failures", "errors", "skipped", "expected_failures", "unexpected_successes")


def flatten(suite):
    for test in suite:
        if isinstance(test, unittest.TestSuite):
            yield from flatten(test)
        else:
            yield test


def has_own_fixture(cls):
    """Class/module fixtures beyond the shared harness must run once per group."""
    module = sys.modules.get(cls.__module__)
    if module is not None and (hasattr(module, "setUpModule") or hasattr(module, "tearDownModule")):
        return True
    for base in cls.__mro__:
        if base.__module__ in ("unittest.case", "process_harness"):
            continue
        if "setUpClass" in vars(base) or "tearDownClass" in vars(base):
            return True
    return False


def load_durations(path=DURATIONS):
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def estimate(test_id, durations):
    if test_id in durations:
        return float(durations[test_id])
    module = test_id.split(".")[0]
    return DEFAULT_SECONDS["process" if module.endswith("_process") else "tool"]


class Batch:
    def __init__(self, tests, graphical, priority, seconds):
        self.tests = tests
        self.graphical = graphical
        self.priority = priority
        self.seconds = seconds

    @property
    def ids(self):
        return [test.id() for test in self.tests]


def plan(tests, durations, priority_modules=(), workers=1):
    """Group tests into batches; returns (headless batches, graphical batches) in start order."""
    groups = {}
    for test in tests:
        cls = type(test)
        key = (cls.__module__, cls.__qualname__) if has_own_fixture(cls) else test.id()
        groups.setdefault(key, []).append(test)
    units = []
    for members in groups.values():
        cls = type(members[0])
        units.append(Batch(members, getattr(cls, "graphical", False) is True,
                           cls.__module__ in priority_modules,
                           sum(estimate(test.id(), durations) for test in members)))

    def ordered(batches):
        return sorted(batches, key=lambda b: (not b.priority, -b.seconds))

    def pack(batches):
        packed, small = [], []
        for batch in ordered(batches):
            if batch.seconds >= BATCH_SECONDS:
                packed.append(batch)
            else:
                small.append(batch)
        # Keep each module's short tests together in discovery order so a
        # batch imports few modules; priority short tests still start first.
        # Smaller batches when the short tests alone would leave workers idle.
        target = min(BATCH_SECONDS, max(1.0, sum(b.seconds for b in small) / (4 * workers)))
        current = None
        for batch in sorted(small, key=lambda b: (not b.priority, b.tests[0].id())):
            if current is None or current.priority != batch.priority or current.seconds + batch.seconds > target:
                current = Batch([], batch.graphical, batch.priority, 0.0)
                packed.append(current)
            current.tests.extend(batch.tests)
            current.seconds += batch.seconds
        return ordered(packed)

    return pack([u for u in units if not u.graphical]), pack([u for u in units if u.graphical])


def module_paths(tests):
    paths = []
    for test in tests:
        module = sys.modules.get(type(test).__module__)
        directory = str(Path(module.__file__).resolve().parent) if module and getattr(module, "__file__", None) else None
        if directory and directory not in paths:
            paths.append(directory)
    return paths


FAILING_OUTCOMES = ("failed", "error", "interrupted", "unexpected_success")


def nonnegative(value):
    return value if isinstance(value, int) and not isinstance(value, bool) and value >= 0 else 0


def combined_counts(summary, tests):
    """A batch's unittest counts: its summary's, but never below what its test records show.

    A failing subtest counts once per subtest in unittest's totals, so a valid
    summary normally equals or exceeds the records; a missing or understated
    summary cannot hide a recorded failure, error or unexpected success.
    """
    derived = dict.fromkeys(SUMMARY_FIELDS, 0)
    for row in tests:
        outcome = row["outcome"]
        derived["failures"] += max(nonnegative(row.get("failures")), outcome == "failed")
        derived["errors"] += max(nonnegative(row.get("errors")), outcome in ("error", "interrupted"))
        derived["skipped"] += (outcome == "skipped") + nonnegative(row.get("subtests_skipped"))
        derived["expected_failures"] += outcome == "expected_failure"
        derived["unexpected_successes"] += outcome == "unexpected_success"
    summary = summary or {}
    return {name: max(derived[name], nonnegative(summary.get(name))) for name in SUMMARY_FIELDS}


def summary_line(counts):
    infos = [f"{name.replace('_', ' ')}={counts[name]}" for name in SUMMARY_FIELDS if counts[name]]
    failed = counts["failures"] or counts["errors"] or counts["unexpected_successes"]
    return ("FAILED" if failed else "OK") + (f" ({', '.join(infos)})" if infos else "")


class Scheduler:
    def __init__(self, headless, graphical, workers, *, stream, sink, failfast, verbosity, pythonpath):
        self.lanes = [list(headless), list(graphical)]
        self.workers = workers
        self.stream = stream
        self.sink = sink
        self.failfast = failfast
        self.verbosity = verbosity
        self.pythonpath = pythonpath
        self.lock = threading.Lock()
        self.stopped = False
        self.records = []
        self.counts = []
        self.problems = []
        self.recorded = set()
        self.planned = [test_id for lane in self.lanes for batch in lane for test_id in batch.ids]
        self.failfast_skipped = []
        self.unstarted = []
        self.scratch = ROOT / "target" / "test-runner" / uuid.uuid4().hex
        self.scratch.mkdir(parents=True)
        self.counter = 0

    def take(self, lane):
        with self.lock:
            if self.stopped or not self.lanes[lane]:
                return None
            self.counter += 1
            return self.counter, self.lanes[lane].pop(0)

    def execute(self, number, batch):
        """Run one worker; returns its exit status, output, parsed records and record problems."""
        ids_file = self.scratch / f"{number:04d}.ids"
        timings = self.scratch / f"{number:04d}.jsonl"
        ids_file.write_text("\n".join(batch.ids) + "\n", encoding="utf-8")
        env = {**os.environ, "TOR_TEST_TIMINGS": str(timings),
               "PYTHONPATH": os.pathsep.join([*self.pythonpath, os.environ.get("PYTHONPATH", "")]).rstrip(os.pathsep)}
        command = [sys.executable, str(SCRIPTS / "test_runner.py"), "--from-file", str(ids_file)]
        if self.verbosity > 1:
            command.append("-v")
        if self.failfast:
            command.append("--failfast")
        completed = subprocess.run(command, cwd=os.getcwd(), env=env, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace")
        rows, problems = [], []
        if timings.exists():
            lines = timings.read_text(encoding="utf-8", errors="replace").splitlines()
            for line_number, line in enumerate(lines, 1):
                if not line.strip():
                    continue
                try:
                    row = json.loads(line)
                except ValueError:
                    problems.append(f"malformed timing record on line {line_number}: {line[:200]!r}")
                    continue
                if not isinstance(row, dict) or not isinstance(row.get("kind"), str):
                    problems.append(f"timing record on line {line_number} is not a record: {line[:200]!r}")
                    continue
                if row["kind"] == "test" and (not isinstance(row.get("test_id"), str)
                                              or not isinstance(row.get("outcome"), str)):
                    problems.append(f"test record on line {line_number} lacks an id or outcome: {line[:200]!r}")
                    continue
                rows.append(row)
        return completed.returncode, completed.stdout or "", rows, problems

    def run_batch(self, number, batch):
        """Reconcile one worker's results against exactly the tests it was given."""
        started = time.monotonic()
        try:
            returncode, output, rows, problems = self.execute(number, batch)
        except Exception as error:  # A launch or read failure fails the batch; it never drops it.
            returncode, output, rows = None, "", []
            problems = [f"worker could not run: {type(error).__name__}: {error}"]
        expected = set(batch.ids)
        results, kept = {}, []
        for row in rows:
            if row["kind"] == "test":
                test_id = row["test_id"]
                if test_id not in expected:
                    problems.append(f"result for a test this batch did not run: {test_id}")
                    continue
                if test_id in results:
                    problems.append(f"duplicate result for {test_id}")
                    continue
                results[test_id] = row
            kept.append(row)
        missing = [test_id for test_id in batch.ids if test_id not in results]
        orderly = returncode == 1 and not problems and any(row["kind"] == "suite" for row in kept)
        skipped_by_failfast = []
        if missing and self.failfast and orderly:
            # The worker finished normally and stopped at its first failure.
            skipped_by_failfast, missing = missing, []
        reason = "; ".join(problems) or f"worker exited with status {returncode} without recording this test"
        if missing:
            # A worker that exits without recording a test (import failure, a
            # crash, an interrupt, unreadable records) must fail it visibly.
            output += "".join(f"\nERROR: {test_id.rsplit('.', 1)[-1]} ({test_id})\n{reason}\n"
                              for test_id in missing)
            kept += [{"schema_version": 1, "profile": os.environ.get("TOR_TEST_PROFILE", "debug"), "kind": "test",
                      "test_id": test_id, "outcome": "error", "subtests_failed": 0, "subtests_skipped": 0,
                      "failures": 0, "errors": 1, "seconds": 0.0, "worker_returncode": returncode,
                      "worker_problem": reason}
                     for test_id in missing]
        if returncode != 0 and not missing and not skipped_by_failfast and not any(
                row["outcome"] in FAILING_OUTCOMES for row in results.values()):
            # The worker's own verdict must agree with its recorded results.
            problems.append(f"worker exited with status {returncode} although its recorded tests passed")
        summaries = [row for row in kept if row["kind"] == "suite"]
        summary = summaries[0] if len(summaries) == 1 else None
        if returncode is not None and not missing:
            # A worker that recorded every test must end with one summary that
            # agrees with its records and exit status; a class fixture that
            # exits early (tearDownClass) or extra summary lines must not pass
            # silently. Unrecorded tests are already errors of their own.
            if not summaries:
                problems.append(f"worker exited with status {returncode} without a suite summary")
            elif summary is None:
                problems.append(f"worker wrote {len(summaries)} suite summaries")
            else:
                if summary.get("tests_run") != len(results):
                    problems.append(f"suite summary reports {summary.get('tests_run')!r} tests run "
                                    f"but {len(results)} were recorded")
                if summary.get("successful") is not (returncode == 0):
                    problems.append(f"suite summary successful={summary.get('successful')!r} "
                                    f"disagrees with exit status {returncode}")
        # Recorded outcomes set a floor that a summary can never lower.
        tests = [row for row in kept if row["kind"] == "test"]
        batch_counts = combined_counts(summary, tests)
        failed = returncode != 0 or bool(missing) or bool(problems)
        with self.lock:
            lane = "graphical" if batch.graphical else "headless"
            self.stream.write(f"=== batch {number} ({lane}, {len(batch.ids)} tests, "
                              f"{time.monotonic() - started:.0f}s){' FAILED' if failed else ''}\n")
            if self.verbosity > 1 or failed:
                self.stream.write(output.rstrip() + "\n")
            for problem in problems:
                self.stream.write(f"BATCH ERROR: batch {number}: {problem}\n")
            self.stream.flush()
            self.problems += [f"batch {number}: {problem}" for problem in problems]
            self.failfast_skipped += skipped_by_failfast
            self.counts.append(batch_counts)
            for row in kept:
                if row["kind"] == "suite":
                    continue
                if row["kind"] == "local_graphical_waiver":
                    continue
                if row["kind"] == "test":
                    self.recorded.add(row["test_id"])
                self.records.append(row)
                if self.sink:
                    self.sink(row)
            if failed and self.failfast:
                self.stopped = True

    def fail_batch(self, number, batch, error):
        """Last resort: an unexpected reconciliation error still fails every unrecorded test."""
        with self.lock:
            problem = f"batch {number}: reconciliation failed: {type(error).__name__}: {error}"
            self.problems.append(problem)
            self.stream.write(f"BATCH ERROR: {problem}\n")
            for test_id in batch.ids:
                if test_id in self.recorded:
                    continue
                self.recorded.add(test_id)
                self.stream.write(f"ERROR: {test_id.rsplit('.', 1)[-1]} ({test_id})\n{problem}\n")
                row = {"schema_version": 1, "profile": os.environ.get("TOR_TEST_PROFILE", "debug"), "kind": "test",
                       "test_id": test_id, "outcome": "error", "subtests_failed": 0, "subtests_skipped": 0,
                       "failures": 0, "errors": 1, "seconds": 0.0, "worker_problem": problem}
                self.records.append(row)
                self.counts.append(combined_counts(None, [row]))
                if self.sink:
                    self.sink(row)
            if self.failfast:
                self.stopped = True

    def lane_worker(self, lane):
        while (item := self.take(lane)) is not None:
            try:
                self.run_batch(*item)
            except Exception as error:
                self.fail_batch(*item, error)

    def run(self):
        threads = [threading.Thread(target=self.lane_worker, args=(0,)) for _ in range(self.workers)]
        if self.lanes[1]:
            threads.append(threading.Thread(target=self.lane_worker, args=(1,)))
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        self.unstarted = [*self.failfast_skipped,
                          *(test_id for lane in self.lanes for batch in lane for test_id in batch.ids)]
        shutil.rmtree(self.scratch, ignore_errors=True)


def run(suite, *, workers, stream, sink, failfast=False, verbosity=1, manifest=None):
    """Run `suite` in parallel; returns True when every test succeeded."""
    started = time.monotonic()
    waived = []
    if manifest is not None:
        # Validate exactly as a serial run would, then record the exclusions here.
        with graphical_waiver.apply(suite, manifest["tests"], manifest["reason"]):
            pass
        waived = manifest["tests"]
        stream.write("LOCAL GRAPHICAL WAIVERS (not run): " + ", ".join(waived) + "\n")
        if sink:
            sink({"kind": "local_graphical_waiver", **manifest})
    tests = [test for test in flatten(suite) if test.id() not in waived]
    # Loader failures (for example an import error) have no runnable name in a
    # worker; run them here so they report exactly as unittest would.
    local = [test for test in tests if type(test).__module__ == "unittest.loader"]
    tests = [test for test in tests if test not in local]
    priority = {name for name in os.environ.get("TOR_TEST_PRIORITY", "").split(",") if name}
    headless, graphical = plan(tests, load_durations(), priority, workers)
    if ("TOR_TEST_BINARIES" not in os.environ and "process_harness" in sys.modules
            and any(type(test).__module__.endswith("_process") for test in tests)):
        # Build once here; workers reuse the result instead of each checking it.
        os.environ["TOR_TEST_BINARIES"] = str(sys.modules["process_harness"].binaries())
    stream.write(f"parallel: {len(tests)} tests in {len(headless)} headless and {len(graphical)} graphical "
                 f"batches; {workers} headless workers, graphical serialized\n")
    stream.flush()
    scheduler = Scheduler(headless, graphical, workers, stream=stream, sink=sink, failfast=failfast,
                          verbosity=verbosity, pythonpath=module_paths(tests))
    scheduler.run()
    counts = {name: 0 for name in SUMMARY_FIELDS}
    local_result = None
    if local:
        local_result = unittest.TextTestRunner(stream=stream, verbosity=verbosity).run(unittest.TestSuite(local))
        counts["errors"] += len(local_result.errors)
        counts["failures"] += len(local_result.failures)
    outcomes = [row for row in scheduler.records if row.get("kind") == "test"]
    # Each batch contributes its worker's unittest counts (a failing subtest
    # counts once per subtest, as in a serial run), floored by its recorded
    # outcomes; unrecorded tests are already error records in that floor.
    for batch in scheduler.counts:
        for name in SUMMARY_FIELDS:
            counts[name] += batch[name]
    recorded_failures = [row["test_id"] for row in outcomes if row["outcome"] in FAILING_OUTCOMES]
    # Success requires exactly one result for every selected test, or an
    # explicit fail-fast "not started"; nothing may silently disappear.
    accounted = Counter(row["test_id"] for row in outcomes)
    unaccounted = sorted(set(scheduler.planned) - set(accounted) - set(scheduler.unstarted))
    extra = sorted(test_id for test_id, count in accounted.items()
                   if count > 1 or test_id not in scheduler.planned)
    problems = list(scheduler.problems)
    if unaccounted:
        problems.append(f"{len(unaccounted)} selected tests have no result: " + ", ".join(unaccounted[:20]))
    if extra:
        problems.append("unexpected or repeated results: " + ", ".join(extra[:20]))
    for problem in problems[len(scheduler.problems):]:
        stream.write(f"BATCH ERROR: {problem}\n")
    counts["errors"] += len(problems)
    for test_id in waived:
        row = {"kind": "test", "test_id": test_id, "outcome": "waived", "subtests_failed": 0,
               "subtests_skipped": 0, "failures": 0, "errors": 0, "seconds": 0.0,
               "skip_reason": graphical_waiver.REASON_PREFIX + manifest["reason"]}
        if sink:
            sink(row)
    if scheduler.unstarted:
        stream.write(f"fail-fast: {len(scheduler.unstarted)} tests not started after a failure\n")
    counts["skipped"] += len(waived)
    ran = len(outcomes) + len(waived) + (local_result.testsRun if local_result else 0)
    seconds = time.monotonic() - started
    stream.write("-" * 70 + f"\nRan {ran} tests in {seconds:.3f}s\n\n{summary_line(counts)}\n")
    stream.flush()
    successful = not (counts["failures"] or counts["errors"] or counts["unexpected_successes"]
                      or recorded_failures or scheduler.unstarted or problems)
    if sink:
        sink({"kind": "suite", "seconds": seconds, "test_seconds": sum(row.get("seconds", 0.0) for row in outcomes),
              "tests_run": ran, "successful": successful, "failures": counts["failures"], "errors": counts["errors"],
              "skipped": counts["skipped"], "waived": len(waived), "waived_tests": list(waived),
              "expected_failures": counts["expected_failures"],
              "unexpected_successes": counts["unexpected_successes"], "workers": workers,
              "not_started": scheduler.unstarted, "batch_errors": problems})
    return successful


def record_durations(paths, destination=DURATIONS):
    """Refresh hints with each test's slowest passed observation in these runs.

    Combining platforms/profiles must not let input order hide a long test.
    Replace older hints for observed tests so a historical outlier can age out;
    keep hints for tests absent from the supplied runs. Hints only order work.
    """
    durations = load_durations(destination)
    observed = {}
    for path in paths:
        for line in Path(path).read_text(encoding="utf-8").splitlines():
            row = json.loads(line)
            if row.get("kind") == "test" and row.get("outcome") == "passed":
                test_id = row["test_id"]
                seconds = row.get("seconds")
                if (isinstance(seconds, bool) or not isinstance(seconds, (int, float))
                        or not math.isfinite(seconds) or seconds < 0):
                    raise ValueError(f"Expected finite nonnegative test seconds for {test_id} in {path}")
                observed[test_id] = max(observed.get(test_id, 0.0), seconds)
    durations.update({test_id: round(seconds, 2) for test_id, seconds in observed.items()})
    Path(destination).write_text(json.dumps(dict(sorted(durations.items())), indent=0) + "\n",
                                 encoding="utf-8", newline="\n")
    return durations


if __name__ == "__main__":
    if len(sys.argv) < 2:
        raise SystemExit("usage: parallel_tests.py TIMINGS.jsonl...  (refreshes scripts/test_durations.json)")
    print(f"{len(record_durations(sys.argv[1:]))} recorded durations")
