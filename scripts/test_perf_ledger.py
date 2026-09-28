import copy
import json
import tempfile
import unittest
from pathlib import Path

import perf_ledger as ledger

MACHINE = ledger.with_fingerprint({
    "cpu": "Example CPU", "logical_cpus": 8, "ram_gib": 16.0, "os": "Example OS", "os_build": "1.0",
    "storage": {"type": "SSD", "filesystem": "NTFS", "model": "Example disk", "bus": "NVMe"},
})
ENTRY = {
    "schema": 1, "date": "2026-09-28", "commit": "a" * 40, "dirty": False,
    "workload": {"name": "performance", "version": 1}, "case": "r8-a1-h100-memory",
    "metric": "authoritative_total", "machine": MACHINE, "build": {"profile": "release"},
    "n": 10, "p50_ms": 1.0, "p95_ms": 2.0, "max_ms": 3.0, "counts": {"history_end": 110},
    "raw": {"url": "https://github.com/KCall360/thresholds-of-ruin/releases/download/perf-x/raw.tar.gz",
            "sha256": "b" * 64},
}


def line(entry):
    return json.dumps(entry) + "\n"


class CommittedLedger(unittest.TestCase):
    def test_committed_ledger_is_well_formed(self):
        text = ledger.LEDGER.read_text(encoding="utf-8")
        self.assertEqual([], ledger.validate_ledger(text))
        self.assertTrue(text.strip(), "The ledger should hold the seeded headline cases")


class EntryFormat(unittest.TestCase):
    def test_valid_entry_has_no_problems(self):
        self.assertEqual([], ledger.validate_entry(ENTRY))
        with_optional = dict(ENTRY, command="latency_bench --case r8-a1-h100-memory", note="Context.")
        self.assertEqual([], ledger.validate_entry(with_optional))

    def test_missing_and_unknown_keys(self):
        entry = dict(ENTRY)
        del entry["raw"]
        self.assertIn("missing keys ['raw']", ledger.validate_entry(entry))
        self.assertIn("unknown keys ['extra']", ledger.validate_entry(dict(ENTRY, extra=1)))

    def test_field_formats(self):
        cases = {
            "date": ("2026-9-28", "date must be YYYY-MM-DD"),
            "commit": ("abc123", "commit must be a full 40-character hash"),
            "dirty": ("no", "dirty must be a boolean"),
            "n": (0, "n must be a positive integer"),
            "p95_ms": (float("nan"), "p50_ms, p95_ms and max_ms must be finite nonnegative numbers"),
            "counts": ({"bytes": -1}, "counts must map names to nonnegative integers"),
            "case": ("", "case must be a non-empty string"),
        }
        for key, (value, problem) in cases.items():
            with self.subTest(key=key):
                self.assertIn(problem, ledger.validate_entry(dict(ENTRY, **{key: value})))

    def test_percentiles_must_be_ordered_but_have_no_thresholds(self):
        self.assertIn("percentiles must satisfy p50 <= p95 <= max",
                      ledger.validate_entry(dict(ENTRY, p50_ms=5.0)))
        self.assertEqual([], ledger.validate_entry(dict(ENTRY, p50_ms=900.0, p95_ms=5000.0, max_ms=90000.0)))

    def test_workload_and_build(self):
        self.assertTrue(ledger.validate_entry(dict(ENTRY, workload={"name": "combat-v1", "version": 1})))
        self.assertTrue(ledger.validate_entry(dict(ENTRY, workload={"name": "combat", "version": 0})))
        self.assertTrue(ledger.validate_entry(dict(ENTRY, build={"profile": "fast"})))
        self.assertEqual([], ledger.validate_entry(dict(ENTRY, build={"profile": "release", "rustc": "rustc 1"})))

    def test_raw_location_must_be_in_the_project(self):
        tagged = "https://github.com/KCall360/thresholds-of-ruin/blob/docs-history-2026-09/docs/x.jsonl.gz"
        self.assertEqual([], ledger.validate_entry(dict(ENTRY, raw={"url": tagged, "sha256": "c" * 64})))
        for url in ("https://example.com/raw.tar.gz", "http://github.com/KCall360/thresholds-of-ruin/releases/download/a/b",
                    "https://github.com/KCall360/thresholds-of-ruin/tree/main"):
            with self.subTest(url=url):
                self.assertTrue(ledger.validate_entry(dict(ENTRY, raw={"url": url, "sha256": "c" * 64})))
        self.assertTrue(ledger.validate_entry(dict(ENTRY, raw={"url": tagged, "sha256": "C" * 64})))

    def test_machine_fingerprint_must_match(self):
        machine = copy.deepcopy(MACHINE)
        machine["storage"]["type"] = "HDD"
        self.assertIn("machine: fingerprint does not match the machine fields",
                      ledger.validate_entry(dict(ENTRY, machine=machine)))
        machine = copy.deepcopy(MACHINE)
        machine["storage"]["type"] = "floppy"
        self.assertTrue(any("storage.type" in p for p in ledger.validate_entry(dict(ENTRY, machine=machine))))

    def test_unrecorded_machine_details_are_explicit(self):
        machine = ledger.with_fingerprint(dict(MACHINE, ram_gib=None, storage={
            "type": "unknown", "filesystem": "unknown", "model": "unknown", "bus": "unknown"}))
        self.assertEqual([], ledger.validate_entry(dict(ENTRY, machine=machine)))
        self.assertNotEqual(machine["fingerprint"], MACHINE["fingerprint"])


class LedgerFormat(unittest.TestCase):
    def test_duplicates_order_and_blank_lines(self):
        later = dict(ENTRY, date="2026-09-29", commit="d" * 40)
        self.assertEqual([], ledger.validate_ledger(line(ENTRY) + line(later)))
        self.assertIn("line 2: duplicate entry for performance r8-a1-h100-memory authoritative_total",
                      ledger.validate_ledger(line(ENTRY) + line(ENTRY)))
        self.assertIn("line 2: entries must be appended in date order", ledger.validate_ledger(line(later) + line(ENTRY)))
        self.assertIn("line 2: blank line", ledger.validate_ledger(line(ENTRY) + "\n"))
        self.assertIn("ledger must end with a newline", ledger.validate_ledger(line(ENTRY).rstrip()))
        self.assertTrue(ledger.validate_ledger("{not json}\n")[0].startswith("line 1: invalid JSON"))

    def test_other_machines_are_separate_series(self):
        other = ledger.with_fingerprint(dict(MACHINE, cpu="Other CPU"))
        self.assertEqual([], ledger.validate_ledger(line(ENTRY) + line(dict(ENTRY, machine=other))))

    def test_append_rejects_invalid_entries_without_writing(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "ledger.jsonl"
            ledger.append_entry(path, ENTRY)
            with self.assertRaises(ValueError):
                ledger.append_entry(path, ENTRY)
            self.assertEqual(line(ENTRY).replace(" ", ""), path.read_text(encoding="utf-8").replace(" ", ""))

    def test_entry_from_comparison(self):
        comparison = {
            "created": "2026-09-28T12:00:00+00:00", "machine": MACHINE, "rustc": "rustc 1.98.1",
            "sides": {"head": {"commit": "e" * 40, "dirty": True}},
            "units": {"latency:r8-a1-h100-memory": {"command": ["latency_bench", "--case", "r8-a1-h100-memory"]}},
            "results": {"latency:r8-a1-h100-memory": {"r8-a1-h100-memory": {
                "workload": {"head": {"name": "performance", "version": 1}},
                "head": {"timings": {"authoritative_total": {"n": 4, "p50_ms": 1, "p95_ms": 2, "max_ms": 3,
                                                             "round_p95_ms": [2]}},
                         "counts": {"history_end": 110, "final_save_bytes": {"min": 1, "max": 2}}}}}},
        }
        entry = ledger.entry_from_comparison(comparison, "latency:r8-a1-h100-memory", "r8-a1-h100-memory",
                                             "authoritative_total", "head", ENTRY["raw"]["url"], "f" * 64)
        self.assertEqual([], ledger.validate_entry(entry))
        self.assertEqual({"history_end": 110}, entry["counts"], "Counts that varied between rounds are omitted")
        self.assertEqual(("2026-09-28", True), (entry["date"], entry["dirty"]))
        self.assertEqual("latency_bench --case r8-a1-h100-memory", entry["command"])


class MachineProbe(unittest.TestCase):
    def test_nearest_rank_matches_the_rust_summaries(self):
        values = list(range(1, 21))
        self.assertEqual((10, 19, 20), (ledger.nearest_rank(values, 50), ledger.nearest_rank(values, 95), values[-1]))
        self.assertEqual(7, ledger.nearest_rank([7], 95))
        with self.assertRaises(ValueError):
            ledger.nearest_rank([], 50)

    def test_storage_classification(self):
        self.assertEqual("NVMe", ledger.storage_type("SSD", bus="NVMe"))
        self.assertEqual("HDD", ledger.storage_type("HDD", bus="RAID"))
        self.assertEqual("SSD", ledger.storage_type(rotational=False, bus="sata"))
        self.assertEqual("HDD", ledger.storage_type(rotational=True))
        self.assertEqual("unknown", ledger.storage_type("Unspecified"))

    def test_windows_probe_output(self):
        info = {
            "processor": [{"Name": " Intel CPU ", "NumberOfLogicalProcessors": 12}],
            "system": {"TotalPhysicalMemory": 17_000_000_000},
            "os": {"Caption": "Microsoft Windows 11 Home", "Version": "10.0.26200"},
            "filesystem": "NTFS",
            "disk": {"FriendlyName": "ST1000LM035-1RK172", "MediaType": "HDD", "BusType": "RAID"},
        }
        machine = ledger.parse_windows_machine(info)
        self.assertEqual([], ledger.validate_machine(machine))
        self.assertEqual(("Intel CPU", 15.8, "HDD"), (machine["cpu"], machine["ram_gib"], machine["storage"]["type"]))
        info["disk"] = None
        self.assertEqual("unknown", ledger.parse_windows_machine(info)["storage"]["model"])

    def test_linux_probe_parsers(self):
        self.assertEqual("AMD EPYC", ledger.parse_cpuinfo("processor\t: 0\nmodel name\t: AMD EPYC\n"))
        self.assertEqual(15.6, ledger.parse_meminfo("MemTotal:       16384000 kB\nMemFree: 1 kB\n"))
        self.assertIsNone(ledger.parse_meminfo(""))
        self.assertEqual("Ubuntu 24.04 LTS", ledger.parse_os_release('NAME="Ubuntu"\nPRETTY_NAME="Ubuntu 24.04 LTS"\n'))


if __name__ == "__main__":
    unittest.main()
