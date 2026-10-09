from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import perf_compare as compare


def latency_rows(case="r8-a1-h100-memory", times=(1.0, 2.0, 3.0), blocked=5.0, save_bytes=4096):
    samples = [{"kind": "sample", "case": case, "expected": "moved", "phases_ms": {"authoritative_total": t},
                "profile": {"scene_calls": 2, "records_serialized": 1, "timed_out": False}, "client_memory": 10 + i}
               for i, t in enumerate(times)]
    samples.append({"kind": "sample", "case": case, "expected": "blocked",
                    "phases_ms": {"authoritative_total": blocked}, "profile": None, "client_memory": 99})
    return [
        {"kind": "case", "case": case, "trace_version": 1},
        *samples,
        {"kind": "case_end", "case": case, "history_end": 103, "rewind_count": 3, "final_save_bytes": save_bytes,
         "final_flush_ms": 0.5, "restart_replay_ms": 7.0,
         "recovery": {"records_loaded": 103, "records_replayed": 3, "checkpoint_sequence": 100},
         "save_status": {"journal_bytes": 600, "checkpoints": 1, "checkpoint_bytes": 900, "error": None}},
        {"kind": "summary", "case": case, "label": "mixed", "phase": "authoritative_total", "p95_ms": 3.0},
    ]


class CaseParsing(unittest.TestCase):
    def test_latency_cases_are_separate_units(self):
        units = compare.parse_cases(["r8-a1-h100-memory", "r64-a8-h100-durable"], cycles=3)
        self.assertEqual(["latency:r8-a1-h100-memory", "latency:r64-a8-h100-durable"], [u.id for u in units])
        self.assertEqual(("--case", "r8-a1-h100-memory", "--cycles", "3"), units[0].args)
        self.assertEqual(("r8-a1-h100-memory",), units[0].groups)

    def test_streaming_cases_are_latency_units(self):
        units = compare.parse_cases(["stream-r256-durable"], cycles=2)
        self.assertEqual(["latency:stream-r256-durable"], [u.id for u in units])
        self.assertEqual(("--case", "stream-r256-durable", "--cycles", "2"), units[0].args)

    def test_streaming_rows_use_the_latency_extractor(self):
        rows = [{"kind": "stream", "case": "stream-r16-memory", "workload": "streaming-v1"},
                {"kind": "sample", "case": "stream-r16-memory", "expected": "moved",
                 "phases_ms": {"authoritative_total": 0.5}, "profile": {"pinned_actors": 1}, "client_memory": 3},
                {"kind": "stream_end", "case": "stream-r16-memory", "history_end": 1, "restart_replay_ms": 4.0,
                 "recovery": {"records_replayed": 1}}]
        timings, counts, version = compare.extract_latency(rows)
        self.assertEqual("streaming-v1", version)
        self.assertEqual([0.5], timings["stream-r16-memory"]["authoritative_total"])
        self.assertEqual([4.0], timings["stream-r16-memory"]["restart_replay_ms"])
        self.assertEqual(1, counts["stream-r16-memory"]["profile.pinned_actors"])

    def test_workload_groups_share_one_complete_run(self):
        units = compare.parse_cases(["combat:a8-h1000", "combat:a2-h0", "combat:a8-h1000", "physics"], cycles=5)
        self.assertEqual(["combat", "physics"], [u.id for u in units])
        self.assertEqual(("a8-h1000", "a2-h0"), units[0].groups)
        self.assertEqual((), units[0].args, "Validators require the complete matrix")
        self.assertEqual((), units[1].groups, "A bare workload shows every group")
        self.assertEqual((), compare.parse_cases(["combat:a8-h0", "combat"], 5)[0].groups)

    def test_unknown_cases_are_rejected(self):
        for case in ("r8-a1-memory", "latency", "latency:r8-a1-h100-memory", "nonexistent:x"):
            with self.subTest(case=case), self.assertRaises(ValueError):
                compare.parse_cases([case], cycles=5)

    def test_validator_flags_follow_the_benchmark(self):
        latency = compare.parse_cases(["r8-a1-h100-memory"], 5)[0]
        extra = ["--phase-d", "--save-target-ms", "10"]
        self.assertEqual(["--case", "r8-a1-h100-memory", "--phase-d"], latency.validator_args(extra))
        client = compare.parse_cases(["client"], 5)[0]
        self.assertEqual(["--narration"], client.validator_args(["--narration"]))
        self.assertEqual([], compare.parse_cases(["combat"], 5)[0].validator_args(extra))


class Scheduling(unittest.TestCase):
    def test_balanced_rounds_reverse_each_pair_without_changing_coverage(self):
        units = compare.parse_cases(["physics", "combat"], 5)
        for order in ("balanced", "head-first"):
            actual = [(r, u.id, side) for r, u, side in compare.schedule(units, 4, order)]
            expected = []
            for round_index in range(1, 5):
                sides = ("head", "base") if order == "head-first" or round_index % 2 == 0 else ("base", "head")
                expected.extend((round_index, unit.id, side) for unit in units for side in sides)
            self.assertEqual(expected, actual)
        with self.assertRaises(ValueError):
            compare.schedule(units, 2, "unknown")

    def test_rounds_interleave_base_and_head(self):
        units = compare.parse_cases(["r8-a1-h100-memory", "combat"], 5)
        order = [(r, u.id, side) for r, u, side in compare.schedule(units, 2)]
        self.assertEqual([
            (1, "latency:r8-a1-h100-memory", "base"), (1, "latency:r8-a1-h100-memory", "head"),
            (1, "combat", "base"), (1, "combat", "head"),
            (2, "latency:r8-a1-h100-memory", "base"), (2, "latency:r8-a1-h100-memory", "head"),
            (2, "combat", "base"), (2, "combat", "head"),
        ], order)


class Extraction(unittest.TestCase):
    def test_request_and_disclosure_times_include_rejections_without_relabeling(self):
        rows = latency_rows(times=(1.0,))
        rows[1]["phases_ms"].update(command_call=1.2, disclosure_projection=0.4)
        rows[2]["phases_ms"].update(command_call=0.1, disclosure_projection=0.3)
        timings, _, _ = compare.extract_latency(rows)
        phases = timings["r8-a1-h100-memory"]
        self.assertEqual(phases["authoritative_total"], [1.0])
        self.assertEqual(phases["command_call"], [1.2, 0.1])
        self.assertEqual(phases["disclosure_projection"], [0.4, 0.3])
        legacy, _, _ = compare.extract_latency(latency_rows())
        self.assertNotIn("disclosure_projection", legacy["r8-a1-h100-memory"])

    def test_complete_wire_timings_and_bytes_keep_distinct_measurement_names(self):
        case = "r8-a1-h100-memory"
        rows = latency_rows()
        rows[0]["wire_profile_version"] = 2
        rows[1]["phases_ms"].update(wire_encoding=0.2, wire_decoding=0.3)
        rows.append(dict(kind="wire", case=case, wire_profile_version=2,
                         n=1, deltas=1, full_bytes_total=1000, sent_bytes_total=400))
        timings, counts, _ = compare.extract_latency(rows)
        self.assertEqual([0.2], timings[case].get("wire_encoding"))
        self.assertEqual([0.3], timings[case].get("wire_decoding"))
        self.assertEqual(1000, counts[case].get("wire.full_envelope_bytes"))
        self.assertEqual(400, counts[case].get("wire.sent_envelope_bytes"))
        self.assertEqual(1, counts[case].get("wire.observations"))
        legacy = latency_rows()
        legacy[1]["phases_ms"]["delta_encoding"] = 0.01
        legacy.append(dict(kind="wire", case=case, n=1, deltas=1,
                           full_bytes_total=900, sent_bytes_total=300))
        old_timings, old_counts, _ = compare.extract_latency(legacy)
        self.assertNotIn("wire_encoding", old_timings[case])
        self.assertNotIn("wire.full_envelope_bytes", old_counts[case])
        self.assertEqual(900, old_counts[case].get("wire.legacy_full_state_bytes"))
        self.assertEqual(300, old_counts[case].get("wire.legacy_update_bytes"))

    def test_latency_uses_the_mixed_distribution_and_deterministic_counts(self):
        timings, counts, version = compare.extract_latency(latency_rows())
        case = "r8-a1-h100-memory"
        self.assertEqual(1, version)
        self.assertEqual([1.0, 2.0, 3.0], timings[case]["authoritative_total"], "Blocked attempts are excluded")
        self.assertEqual([7.0], timings[case]["restart_replay_ms"])
        self.assertEqual(6, counts[case]["profile.scene_calls"])
        self.assertNotIn("profile.timed_out", counts[case], "Booleans are not counts")
        self.assertEqual(99, counts[case]["client_memory"])
        self.assertEqual((4096, 3, 1, 600), (counts[case]["final_save_bytes"], counts[case]["recovery.records_replayed"],
                                             counts[case]["save_status.checkpoints"],
                                             counts[case]["save_status.journal_bytes"]))

    def test_saved_traversals_read_nested_persistence(self):
        rows = [{"kind": "traversal", "case": "traversal-r8", "trace_version": 1},
                {"kind": "sample", "case": "traversal-r8", "expected": "moved",
                 "phases_ms": {"authoritative_total": 1.5}, "profile": {"scene_calls": 1}, "client_memory": 5},
                {"kind": "traversal_end", "case": "traversal-r8", "history_end": 1, "client_memory": 5,
                 "persistence": {"final_save_bytes": 77, "checkpoint_json_bytes": 88, "restart_replay_ms": 2.0,
                                 "final_flush_ms": 1.0, "recovery": {"records_replayed": 1},
                                 "save_status": {"journal_bytes": 50}}}]
        timings, counts, _ = compare.extract_latency(rows)
        self.assertEqual([2.0], timings["traversal-r8"]["restart_replay_ms"])
        self.assertEqual((77, 88, 1, 50), tuple(counts["traversal-r8"][k] for k in (
            "final_save_bytes", "checkpoint_json_bytes", "recovery.records_replayed", "save_status.journal_bytes")))

    def test_row_workloads_split_timings_from_counts(self):
        rows = [{"workload": "combat", "version": 1, "actors": 8, "history": 0, "sample": s,
                 "command_ms": [1.0, 2.0], "save_ms": 4.0, "phase_totals_ms": {"simulation": 1.0},
                 "scenes": 10, "saved_bytes": 100, "falling": True} for s in range(2)]
        timings, counts, version = compare.extract_rows(rows, compare.WORKLOADS["combat"])
        self.assertEqual(1, version)
        self.assertEqual({"command_ms": [1.0, 2.0, 1.0, 2.0], "save_ms": [4.0, 4.0]}, timings["a8-h0"])
        self.assertEqual({"scenes": 20, "saved_bytes": 200}, counts["a8-h0"])

    def test_group_names(self):
        self.assertEqual("a8-i128-c8-falling", compare.WORKLOADS["physics"].group(
            {"actors": 8, "items": 128, "cells": 8, "falling": True}))
        self.assertEqual("i1000-id256", compare.WORKLOADS["items"].group({"items": 1000, "identities": 256}))
        self.assertEqual("c20956-b64", compare.WORKLOADS["client"].group({"cells": 20956, "burst": 64}))
        places = compare.WORKLOADS["places"].group
        self.assertEqual("rooms64-rename", places({"extra_rooms": 64, "kind": "sample", "label": "rename"}))
        self.assertEqual("rooms64-recovery", places({"extra_rooms": 64, "kind": "recovery"}))

    def test_items_version_field(self):
        rows = [{"workload_version": 1, "items": 16, "identities": 8, "sample": 0, "transfers": 20,
                 "transfer_ms": [0.5], "scenes": 3}]
        timings, counts, version = compare.extract_rows(rows, compare.WORKLOADS["items"])
        self.assertEqual((1, {"scenes": 3}), (version, counts["i16-id8"]))


class Pooling(unittest.TestCase):
    def test_rounds_are_pooled_before_percentiles(self):
        first = compare.extract_latency(latency_rows(times=[1.0] * 10))[:2]
        second = compare.extract_latency(latency_rows(times=[1.0] * 9 + [50.0]))[:2]
        stats = compare.pool([first, second])["r8-a1-h100-memory"]["timings"]["authoritative_total"]
        self.assertEqual((20, 1.0, 1.0, 50.0), (stats["n"], stats["p50_ms"], stats["p95_ms"], stats["max_ms"]))
        self.assertEqual([1.0, 50.0], stats["round_p95_ms"])

    def test_varying_counts_are_flagged(self):
        first = compare.extract_latency(latency_rows(save_bytes=4096))[:2]
        second = compare.extract_latency(latency_rows(save_bytes=8192))[:2]
        counts = compare.pool([first, second])["r8-a1-h100-memory"]["counts"]
        self.assertEqual(103, counts["history_end"])
        self.assertEqual({"min": 4096, "max": 8192, "missing_rounds": 0}, counts["final_save_bytes"])

    def test_counts_missing_from_a_round_are_not_reported_as_stable(self):
        first = ({}, {"g": {"scenes": 4}})
        second = ({}, {"g": {}})
        self.assertEqual({"min": 4, "max": 4, "missing_rounds": 1}, compare.pool([first, second])["g"]["counts"]["scenes"])


class Formatting(unittest.TestCase):
    def test_side_by_side_table(self):
        base = compare.pool([compare.extract_latency(latency_rows(times=[2.0] * 20))[:2]])["r8-a1-h100-memory"]
        head = compare.pool([compare.extract_latency(latency_rows(times=[1.0] * 20, save_bytes=5000))[:2]])[
            "r8-a1-h100-memory"]
        text = compare.format_group("latency r8-a1-h100-memory", base, head)
        row = next(line for line in text.splitlines() if line.strip().startswith("head") and "-50.0%" in line)
        self.assertIn("1.000", row)
        self.assertRegex(text, r"final_save_bytes\s+4096\s+5000\s+\+904")
        self.assertRegex(text, r"history_end\s+103\s+103\s+=")

    def test_missing_side_and_varying_counts(self):
        head = {"timings": {"x_ms": {"n": 1, "p50_ms": 1.0, "p95_ms": 1.0, "max_ms": 1.0, "round_p95_ms": [1.0]}},
                "counts": {"bytes": {"min": 1, "max": 2, "missing_rounds": 0}}}
        text = compare.format_group("t", {"timings": {}, "counts": {"bytes": 1}}, head)
        self.assertRegex(text, r"x_ms\s+base\s+-")
        self.assertRegex(text, r"bytes\s+1\s+varies 1\.\.2\s+\?")


class Changes(unittest.TestCase):
    def test_changes_below_timer_resolution_are_equal(self):
        self.assertEqual("=", compare._change(0.0001, 0.0003))
        self.assertEqual("+100.0%", compare._change(1.0, 2.0))
        self.assertEqual("n/a", compare._change(0.0, 1.0))


class Processes(unittest.TestCase):
    def test_process_listing_parsers(self):
        windows = '"cargo.exe","1234","Console","1","10,000 K"\n"explorer.exe","2","Console","1","1 K"\n'
        self.assertEqual(["cargo.exe", "explorer.exe"], compare.parse_process_names(windows))
        self.assertEqual(["rustc", "bash"], compare.parse_process_names("/usr/bin/rustc\nbash\n\n"))


class ComparisonOwnership(unittest.TestCase):
    def run_comparison(self, directory, *, build=False, failed_run=False, interrupted=False, shared_target=False,
                       order="base-first"):
        head, base = directory / "head", directory / "base"
        head_target = directory / "head-cache" if build else head / "target"
        if shared_target:
            head_target = base / "target"
        output = directory / "report"
        parent = directory / "caller-storage"
        parent.mkdir(exist_ok=True)
        sentinel = parent / "unrelated-save.db"
        sentinel.write_bytes(b"preserve caller data")
        # Existing default-path outputs expose stale snapshot selection as
        # well as accidental inheritance of the head's ambient build target.
        for tree, contents in [(base, b"base-built"), (head, b"stale-head")]:
            binary = tree / "target/release/examples" / f"latency_bench{compare.EXE}"
            binary.parent.mkdir(parents=True)
            binary.write_bytes(contents)
        actual_head = head_target / "release/examples" / f"latency_bench{compare.EXE}"
        actual_head.parent.mkdir(parents=True, exist_ok=True)
        actual_head.write_bytes(b"head-built")
        saves = []
        self.save_directories = saves

        def execute(command, *, cwd=None, env=None, stdout=None, **kwargs):
            if command[0] == "cargo":
                if shared_target:
                    self.fail("Shared build targets must be rejected before any compilation")
                target = (Path(command[command.index("--target-dir") + 1])
                          if "--target-dir" in command else Path(os.environ["CARGO_TARGET_DIR"]))
                binary = target / "release/examples" / f"latency_bench{compare.EXE}"
                binary.parent.mkdir(parents=True, exist_ok=True)
                binary.write_bytes(b"base-built" if cwd == base else b"head-built")
            elif command[0] == "rustc":
                return subprocess.CompletedProcess(command, 0, stdout="fixture rustc", stderr="")
            elif stdout is not None:
                saves.append(Path(env["TMP"]))
                if interrupted:
                    raise KeyboardInterrupt("synthetic interruption")
                stdout.write("\n".join(json.dumps(row) for row in latency_rows()) + "\n")
                return subprocess.CompletedProcess(command, int(failed_run))
            return subprocess.CompletedProcess(command, 0, stdout="", stderr="")

        def git(*args, **kwargs):
            return "" if args[0] == "status" else ("b" * 40 if "--verify" in args else "a" * 40)

        machine = {"fingerprint": "fixture", "cpu": "fixture", "ram_gib": 1,
                   "os": "fixture", "os_build": "fixture", "storage": {"type": "fixture", "model": "fixture"}}
        args = ["baseline", "--case", "r8-a1-h100-memory", "--rounds", "1", "--order", order,
                "--output", str(output), "--temp-dir", str(parent)]
        printed = io.StringIO()
        if not build:
            args.append("--no-build")
        with patch.object(compare, "ROOT", head), patch.object(compare, "git", git), \
                patch.object(compare, "prepare_worktree", return_value=base), \
                patch.object(compare, "competing_processes", return_value=[]), \
                patch.object(compare.perf_ledger, "probe_machine", return_value=machine), \
                patch.object(compare.subprocess, "run", side_effect=execute), \
                patch.dict(os.environ, {"CARGO_TARGET_DIR": str(head_target)}), redirect_stdout(printed):
            code = compare.main(args)
        self.comparison_stdout = printed.getvalue()
        return code, output, parent, sentinel, saves

    def test_printed_summary_reports_the_selected_pair_order(self):
        for order, label in [("base-first", "ABAB"), ("head-first", "BABA"), ("balanced", "ABBA")]:
            with self.subTest(order=order), tempfile.TemporaryDirectory() as directory:
                code, output, _, _, _ = self.run_comparison(Path(directory), order=order)
                self.assertEqual(code, 0)
                self.assertIn(f"1 {label} rounds", self.comparison_stdout)
                self.assertEqual(json.loads((output / "comparison.json").read_text())["order"], label)

    def test_successful_comparison_preserves_caller_storage_and_cleans_only_owned_saves(self):
        with tempfile.TemporaryDirectory() as directory:
            code, output, parent, sentinel, saves = self.run_comparison(Path(directory))
            self.assertEqual(code, 0)
            self.assertEqual(sentinel.read_bytes(), b"preserve caller data")
            self.assertTrue((output / "comparison.json").is_file())
            self.assertTrue(list(output.glob("*.tar.gz")))
            self.assertEqual(len(saves), 2)
            self.assertEqual(saves[0].parent, saves[1].parent)
            self.assertTrue(saves[0].parent.parent.samefile(parent),
                            "Save paths must use the caller-selected storage directory")
            self.assertFalse(saves[0].parent.exists())

    def test_failed_workload_preserves_caller_data_and_retained_failure_reports(self):
        with tempfile.TemporaryDirectory() as directory:
            code, output, _, sentinel, saves = self.run_comparison(Path(directory), failed_run=True)
            self.assertEqual(code, 1)
            self.assertEqual(sentinel.read_bytes(), b"preserve caller data")
            report = json.loads((output / "comparison.json").read_text())
            self.assertEqual(len(report["failures"]), 2)
            self.assertTrue(list((output / "raw").glob("*.jsonl.gz")))
            self.assertFalse(saves[0].parent.exists())

    def test_builds_and_snapshots_keep_baseline_distinct_from_ambient_head_target(self):
        with tempfile.TemporaryDirectory() as directory:
            code, output, _, _, _ = self.run_comparison(Path(directory), build=True)
            self.assertEqual(code, 0)
            self.assertEqual((output / f"bin/base-latency_bench{compare.EXE}").read_bytes(), b"base-built")
            self.assertEqual((output / f"bin/head-latency_bench{compare.EXE}").read_bytes(), b"head-built")

    def test_shared_target_is_rejected_before_either_build_can_overwrite_it(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(SystemExit, "distinct build target"):
                self.run_comparison(Path(directory), build=True, shared_target=True)

    def test_interruption_cleans_owned_saves_and_preserves_caller_storage(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(KeyboardInterrupt, "synthetic interruption"):
                self.run_comparison(root, interrupted=True)
            self.assertEqual((root / "caller-storage/unrelated-save.db").read_bytes(), b"preserve caller data")
            self.assertFalse(self.save_directories[0].parent.exists())
            self.assertTrue((root / "report/raw").is_dir())


if __name__ == "__main__":
    unittest.main()
