import copy
import unittest
from place_performance_report import SPEC, METRICS, validate, validate_client


class PlacePerformanceReport(unittest.TestCase):
    def rows(self):
        rows = []
        for rooms in SPEC["extra_rooms"]:
            count = 2 + rooms * len(SPEC["hints"])
            rows.extend(dict(version=1, kind="discovery", extra_rooms=rooms, room=i,
                             places=2+(i+1)*len(SPEC["hints"])) for i in range(rooms))
            rows.extend(dict(version=1, kind="sample", extra_rooms=rooms, places=count,
                             sample=i, label="rename" if i % 2 == 0 else "wait", records=1,
                             navigation_refreshes=0, wire_bytes=100, **{m: 1.0 for m in METRICS})
                        for i in range(SPEC["samples"]))
            rows.append(dict(version=1, kind="recovery", extra_rooms=rooms, places=count, exact=True, checkpoint_bytes=100))
        return rows

    def test_complete_trace_and_missing_duplicate_or_invalid_samples(self):
        rows = self.rows()
        self.assertEqual(len(validate(rows)), 6)
        for broken in (rows[:-1], rows + [rows[-1]], rows[1:]):
            with self.assertRaises(ValueError):
                validate(broken)
        for field, value in (("places", 999), ("records", 2), ("command_ms", float("nan")), ("navigation_refreshes", 1)):
            bad = copy.deepcopy(rows)
            bad[0][field] = value
            with self.assertRaises(ValueError):
                validate(bad)

    def test_client_trace_rejects_missing_and_nonfinite_measurements(self):
        rows = [dict(sample=i, label="rename" if i%2==0 else "wait", places=2,
                     request_to_ack_ms=1., request_to_ready_ms=2., request_to_presentation_ms=3.)
                for i in range(SPEC["samples"])]
        result = dict(version=1, extra_rooms=0, final_places=[{}, {}], samples=rows)
        validate_client(result)
        rows[0]["request_to_ack_ms"] = float("nan")
        with self.assertRaises(ValueError):
            validate_client(result)
        result["samples"] = rows[1:]
        with self.assertRaises(ValueError):
            validate_client(result)
