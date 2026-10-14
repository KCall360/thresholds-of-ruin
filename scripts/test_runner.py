"""Run ordinary unittest selections with optional append-only timing evidence.

Accepts unittest's existing arguments unchanged. TOR_TEST_TIMINGS names a JSONL
file for observations only: timings never choose tests or alter pass/fail limits.
Bounded failure diagnostics are flushed there before teardown and cleanup, while
each test's single outcome is still recorded only after its cleanup finishes.
Suite time includes class/module setup (including shared binary builds); individual
test times include that test's setup, body, teardown and cleanup. Their difference
is overhead, not an inferred compilation measurement.
"""
import argparse
import json
import os
from pathlib import Path
import sys
import time
import unittest

import graphical_waiver


class TimedResult(unittest.TextTestResult):
    def __init__(self, *args, sink=None, **kwargs):
        super().__init__(*args, **kwargs)
        self.sink = sink
        self.result_started = time.monotonic()
        self.current = None
        self.test_seconds = 0.0
        self.waived = []

    def startTest(self, test):
        self.started = time.monotonic()
        self.current = {'kind': 'test', 'test_id': test.id(), 'outcome': 'passed',
                        'subtests_failed': 0, 'subtests_skipped': 0, 'failures': 0, 'errors': 0}
        super().startTest(test)

    def outcome(self, value):
        if self.current is not None:
            if value == 'failed':
                self.current['failures'] += 1
            elif value == 'error':
                self.current['errors'] += 1
            prior = self.current['outcome']
            self.current['outcome'] = ('error' if self.current['errors'] else
                                       'failed' if self.current['failures'] else
                                       'unexpected_success' if prior == 'unexpected_success' else value)

    def addFailure(self, test, err):
        self.outcome('failed')
        super().addFailure(test, err)
        self.diagnostic(test, err, self.failures[-1][1])

    def addError(self, test, err):
        self.outcome('error')
        super().addError(test, err)
        self.diagnostic(test, err, self.errors[-1][1])

    def diagnostic(self, test, err, traceback, *, subtest=None):
        if self.sink:
            self.sink({'kind': 'diagnostic', 'test_id': test.id(),
                       'seconds': time.monotonic() - self.result_started,
                       'exception_type': err[0].__name__,
                       'traceback': traceback[-65536:],
                       'traceback_truncated': len(traceback) > 65536,
                       **({'subtest_id': subtest.id()} if subtest is not None else {})})

    def addSkip(self, test, reason):
        if self.current is not None:
            if test.id() == self.current['test_id']:
                marker = getattr(test, '_tor_graphical_waiver', None)
                waived = marker is not None and marker == reason
                self.outcome('waived' if waived else 'skipped')
                self.current['skip_reason'] = reason
                if waived:
                    self.waived.append(test.id())
            else:
                self.current['subtests_skipped'] += 1
        super().addSkip(test, reason)

    def addExpectedFailure(self, test, err):
        self.outcome('expected_failure')
        super().addExpectedFailure(test, err)

    def addUnexpectedSuccess(self, test):
        self.outcome('unexpected_success')
        super().addUnexpectedSuccess(test)

    def addSubTest(self, test, subtest, err):
        if err is not None and self.current is not None:
            self.current['subtests_failed'] += 1
            self.outcome('failed' if issubclass(err[0], test.failureException) else 'error')
        super().addSubTest(test, subtest, err)
        if err is not None:
            reports = self.failures if issubclass(err[0], test.failureException) else self.errors
            self.diagnostic(test, err, reports[-1][1], subtest=subtest)

    def stopTest(self, test):
        try:
            if self.current is not None:
                # unittest calls stopTest in its finally even when an interrupt
                # escapes the method/setup/cleanup. Completed skipped subtests
                # have no addSuccess callback, so absence of one is not proof
                # of interruption. Record the propagating exception instead.
                if isinstance(sys.exc_info()[1], KeyboardInterrupt):
                    self.current['outcome'] = 'interrupted'
                elapsed = time.monotonic() - self.started
                self.test_seconds += elapsed
                self.current['seconds'] = elapsed
                if self.sink:
                    self.sink(self.current)
        finally:
            self.current = None
            super().stopTest(test)


class TimedRunner(unittest.TextTestRunner):
    def __init__(self, *args, sink=None, **kwargs):
        super().__init__(*args, **kwargs)
        self.sink = sink

    def _makeResult(self):
        options = {"durations": self.durations} if hasattr(self, "durations") else {}
        return TimedResult(self.stream, self.descriptions, self.verbosity, sink=self.sink, **options)

    def run(self, test):
        started = time.monotonic()
        result = super().run(test)
        if self.sink:
            self.sink({'kind': 'suite', 'seconds': time.monotonic() - started,
                       'test_seconds': result.test_seconds, 'tests_run': result.testsRun,
                       'successful': result.wasSuccessful(), 'failures': len(result.failures),
                       'errors': len(result.errors), 'skipped': len(result.skipped),
                       'waived': len(result.waived), 'waived_tests': result.waived,
                       'expected_failures': len(result.expectedFailures),
                       'unexpected_successes': len(result.unexpectedSuccesses)})
        return result


def main(argv=None):
    # Direct execution starts sys.path in scripts; preserve unittest's ability to
    # resolve named tests in the caller's cwd as well as discovery's start path.
    sys.path.insert(0, str(Path.cwd()))
    parser = argparse.ArgumentParser(add_help=False, allow_abbrev=False)
    parser.add_argument('--waive-local-graphical', action='append', metavar='MANIFEST')
    local, arguments = parser.parse_known_args(sys.argv[1:] if argv is None else argv)
    manifest = None
    if local.waive_local_graphical:
        if os.environ.get('CI'):
            parser.error('--waive-local-graphical is local only')
        if len(local.waive_local_graphical) != 1:
            parser.error('--waive-local-graphical requires one manifest')
        try:
            manifest = graphical_waiver.read_manifest(local.waive_local_graphical[0])
        except (OSError, ValueError) as error:
            parser.error(str(error))
    path = os.environ.get('TOR_TEST_TIMINGS')
    handle = None
    try:
        if path:
            destination = Path(path)
            destination.parent.mkdir(parents=True, exist_ok=True)
            handle = destination.open('a', encoding='utf-8')

        def record(row):
            if handle is not None:
                handle.write(json.dumps({'schema_version': 1, 'profile': os.environ.get('TOR_TEST_PROFILE', 'debug'),
                                         **row}, allow_nan=False) + '\n')
                handle.flush()

        # unittest expects a runner class or instance, rather than an arbitrary
        # factory, and supplies verbosity/buffering/failfast to the class.
        class ConfiguredRunner(TimedRunner):
            def __init__(self, **kwargs):
                super().__init__(sink=record if handle else None, **kwargs)

            def run(self, suite):
                if manifest is None:
                    return super().run(suite)
                with graphical_waiver.apply(suite, manifest['tests'], manifest['reason']) as selected:
                    self.stream.writeln('LOCAL GRAPHICAL WAIVERS (not run): ' + ', '.join(manifest['tests']))
                    record({'kind': 'local_graphical_waiver', **manifest})
                    return super().run(selected)

        try:
            program = unittest.main(module=None, argv=[sys.argv[0], *arguments],
                                    testRunner=ConfiguredRunner, exit=False)
        except ValueError as error:
            parser.error(str(error))
        return 0 if program.result.wasSuccessful() else 1
    finally:
        if handle is not None:
            handle.close()


if __name__ == '__main__':
    sys.exit(main())
