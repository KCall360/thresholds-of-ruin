"""Exact, audited local display waivers; never an automatic display fallback."""
from contextlib import contextmanager
import json
from pathlib import Path
import re
import unittest

REASON_PREFIX = "LOCAL GRAPHICAL WAIVER: "


def read_manifest(path):
    """Read a small explicit allowlist. Wildcards and extra keys are forbidden."""
    path = Path(path)
    with path.open("rb") as handle:
        source = handle.read(256_001)
    if len(source) > 256_000:
        raise ValueError("graphical waiver manifest exceeds 256000 bytes")

    def unique_fields(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("duplicate graphical waiver manifest field: " + key)
            result[key] = value
        return result

    value = json.loads(source.decode("utf-8"), object_pairs_hook=unique_fields)
    if not isinstance(value, dict) or set(value) != {"reason", "tests"}:
        raise ValueError("graphical waiver manifest requires exactly reason and tests")
    reason, names = value["reason"], value["tests"]
    if not isinstance(reason, str) or not reason.strip() or len(reason) > 1000:
        raise ValueError("graphical waiver reason must contain 1–1000 characters")
    if (not isinstance(names, list) or not 1 <= len(names) <= 1000
            or any(not isinstance(name, str) or len(name) > 256
                   or not re.fullmatch(r"test_\w+(?:\.\w+)+\.test_\w+", name) for name in names)
            or len(set(names)) != len(names)):
        raise ValueError("graphical waiver tests must be 1–1000 unique exact unittest IDs")
    return {"reason": reason, "tests": sorted(names)}


def cases(suite):
    if type(suite) is not unittest.TestSuite:
        raise ValueError("graphical waiver requires ordinary unittest suites; custom execution is not supported")
    for test in suite:
        if isinstance(test, unittest.TestSuite):
            yield from cases(test)
        elif isinstance(test, unittest.TestCase):
            yield test
        else:
            raise ValueError("graphical waiver cannot inspect a non-unittest test")


class _WaivedTest(unittest.TestCase):
    """A named evidence marker with no fixture relationship to the native case."""
    def __init__(self, name, reason):
        super().__init__()
        self._recorded_id = name
        self._tor_graphical_waiver = REASON_PREFIX + reason

    def id(self):
        return self._recorded_id

    def __str__(self):
        return self._recorded_id

    def runTest(self):
        self.skipTest(self._tor_graphical_waiver)


@contextmanager
def apply(suite, names, reason):
    """Retain original runnable cases in order, then report named exclusions.

    Waiver markers have no native class/module fixtures. No original instance,
    method or class is mutated, so inherited skips cannot leak into an unlisted
    subclass. Markers run last to avoid inserting artificial fixture transitions
    between ordinary methods of a mixed class.
    """
    selected = list(cases(suite))
    by_id = {test.id(): test for test in selected}
    requested = set(names)
    if len(requested) != len(names) or len(by_id) != len(selected):
        raise ValueError("duplicate graphical waiver or selected test ID")
    missing = requested - by_id.keys()
    if missing:
        raise ValueError("graphical waiver names are not selected tests: " + ", ".join(sorted(missing)))
    for name in requested:
        test = by_id[name]
        method = getattr(test, test._testMethodName)
        if (getattr(type(test), "graphical", False) is not True
                or getattr(type(test), "__unittest_skip__", False)
                or getattr(method, "__unittest_skip__", False)):
            raise ValueError("graphical waiver requires an unskipped graphical test: " + name)
    runnable = [test for test in selected if test.id() not in requested]
    markers = [_WaivedTest(name, reason) for name in sorted(requested)]
    yield unittest.TestSuite([*runnable, *markers])
