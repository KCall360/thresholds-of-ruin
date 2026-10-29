"""The benchmark trace driven through real headless clients and ASCII frames."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from performance_driver import JsonProcess, run_demo
from performance_process_support import assert_physics_report, benchmark, benchmark_executable

from process_harness import ProcessTestCase, ROOT, SUFFIX, TOKEN


class PerformanceProcesses(ProcessTestCase):
    def benchmark(self, name, args, timeout):
        return benchmark(self, name, args, timeout)

    def test_comparison_builds_examples_from_separate_cargo_workspaces(self):
        import perf_compare as compare
        fixtures = {}
        for label, folder in [("root", self.directory), ("sub", self.directory / "isolated")]:
            folder.mkdir(exist_ok=True)
            (folder / "Cargo.toml").write_text(
                '[package]\nname = "workspace-probe"\nversion = "0.1.0"\nedition = "2021"\n'
                '[workspace]\n[[example]]\nname = "' + label + '_bench"\npath = "probe.rs"\n', encoding="utf-8")
            (folder / "Cargo.lock").write_text(
                'version = 4\n[[package]]\nname = "workspace-probe"\nversion = "0.1.0"\n', encoding="utf-8")
            (folder / "probe.rs").write_text('fn main() { println!("' + label + '"); }', encoding="utf-8")
            fixtures[label] = compare.Workload(label, "workspace-probe", label + "_bench", "unused",
                manifest="Cargo.toml" if label == "root" else "isolated/Cargo.toml")
        target = self.directory / "build-target"
        with patch.dict(compare.WORKLOADS, fixtures):
            compare.build(self.directory, compare.parse_cases(list(fixtures), 1),
                          self.directory / "workspace-build.log", target)
        for label in fixtures:
            result = subprocess.run([target / "release/examples" / (label + "_bench" + SUFFIX)],
                                    capture_output=True, text=True, check=True, timeout=30)
            self.assertEqual(result.stdout.strip(), label)

    def test_small_maintained_producers_emit_complete_valid_reports(self):
        from combat_performance_report import summarize as combat
        from item_performance_report import summarize as items
        from place_performance_report import validate as places
        from structural_performance_report import validate_horizon, validate_scenario
        producers = [
            ('combat_bench', lambda rows: combat(rows, selected_case=(2, 0))),
            ('item_bench', lambda rows: items(rows, selected_case=(16, 8))),
            ('place_bench', lambda rows: places(rows, selected_rooms=0)),
            ('scenario_bench', lambda rows: validate_scenario(rows, smoke=True)),
            ('horizon-profile', lambda rows: validate_horizon(rows, smoke=True)),
        ]
        for name, validate_report in producers:
            with self.subTest(producer=name):
                rows = self.benchmark(name, ['--smoke'], 180)
                self.assertTrue(rows)
                self.assertTrue(validate_report(rows))
                with self.assertRaises(ValueError):
                    validate_report(rows[:-1])

    def test_active_mob_producer_proves_all_live_regions_and_replay(self):
        from active_mob_performance_report import extract
        rows = self.benchmark('active_mob_bench', ['--steps', '8'], 180)
        timings, counts, version = extract(rows)
        self.assertEqual(version, 1)
        self.assertEqual(set(timings), {'r20-ai100-s8'})
        self.assertEqual(counts['r20-ai100-s8']['player_moves'], 8)
        with self.assertRaises(ValueError):
            extract([])

    def test_active_mob_headless_smoke_preserves_activity_and_source_free_restart(self):
        from active_mob_driver import run
        from perf_ledger import sha256_file
        executable = benchmark_executable(self, 'active_mob_bench')
        profile = os.environ.get('TOR_TEST_PROFILE', 'debug')
        def source(*args):
            return subprocess.check_output(['git', *args], cwd=ROOT, text=True).strip()
        binaries = {'tor-server': self.bin / ('tor-server' + SUFFIX),
                    'tor-client-headless': self.bin / ('tor-client-headless' + SUFFIX),
                    'active_mob_bench': executable}
        # self.bin is supplied by the normal harness build or its validated CI
        # application artifact; the example has its independent Cargo receipt.
        manifest = dict(source_commit=source('rev-parse', 'HEAD'),
                        source_tree=source('rev-parse', 'HEAD^{tree}'),
                        dirty=bool(source('status', '--porcelain')), profile=profile,
                        toolchain=subprocess.check_output(['rustc', '--version'], cwd=ROOT, text=True).strip(),
                        binaries={name: sha256_file(path) for name, path in binaries.items()})
        manifest_path = self.directory / 'active-mob-build.json'
        manifest_path.write_text(json.dumps(manifest), encoding='utf-8')
        output = self.directory / 'active-mob-headless'
        result = run(self.bin, executable, output, manifest_path,
                     duration=3.2, rate=5., burst=4, smoke=True)
        self.assertTrue(result['complete'])
        self.assertTrue(result['restart_equal'])
        self.assertTrue(result['source_package_removed_from_original_path'])
        self.assertEqual((len(result['samples']), len(result['burst_samples'])), (16, 4))
        self.assertEqual(result['measured_inspection']['activity']['player_moves'], 16)
        self.assertEqual(result['final_inspection']['activity']['player_moves'], 20)
        self.assertEqual(result['provenance']['manifest']['profile'], profile)
        self.assertFalse(result['provisional_load_target_met'], 'smoke is never a 60-second release acceptance claim')
        self.assertTrue((output / 'result.json').is_file())
        self.assertFalse((output / 'failure.json').exists())

    def test_rogue_benchmark_preserves_floor_work_and_residency_bounds(self):
        from performance_report import validate_rogue
        rows = self.benchmark('rogue_bench', ['1'], 180)
        validate_rogue(rows)
        metadata = next(row for row in rows if row['kind'] == 'rogue')
        self.assertEqual(metadata['workload'], 'rogue-descriptor-v2')
        self.assertEqual(metadata['scenario'], 'tests/rogue-geometry')
        self.assertEqual(metadata['population'], 'none')
        self.assertEqual(metadata['domain'], 'open')
        self.assertEqual(metadata['sampled_depths'], list(range(1, 27)))
        self.assertNotIn('floors', metadata, 'sample coverage is not a world-size limit')
        with self.assertRaises(ValueError):
            validate_rogue(rows[:-1])

    def test_physics_benchmark_reports_completed_durable_commit_timings(self):
        rows = self.benchmark('physics_bench', ['--profile'], 600)
        assert_physics_report(self, rows)

    def test_multiphase_attack_timing_reaches_authoritative_outcome_and_fresh_context(self):
        self.server(scenario="dungeon-loop", seed=None)
        client = JsonProcess(self.bin / ("tor-client-headless" + SUFFIX),
                             ["--connect", self.address],
                             {**os.environ, "TOR_SERVER_TOKEN": TOKEN}, self.directory, "timed-player")
        self.addCleanup(client.stop)
        initial, _ = client.until(lambda frame: frame.get("type") == "ready")
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                      if actor["name"] == "ruin guard")
        completed, *_ = client.send({"type": "act", "action": {"type": "use_ability", "id": "basic_melee", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}}},
                                    wait_for_completion=True)
        timing = client.action_timing.copy()
        self.assertIsNone(completed["error"])
        self.assertEqual(timing["outcome"], "resolved")
        self.assertEqual(timing["version"], 1)
        self.assertEqual(timing["branch"], initial["branch"])
        self.assertGreaterEqual(timing["request_to_execution_ms"], timing["request_to_admission_ms"])
        self.assertGreater(timing["request_to_outcome_ms"], timing["request_to_execution_ms"])
        self.assertGreaterEqual(timing["request_to_context_ms"], timing["request_to_outcome_ms"])
        self.assertFalse(any(status["intention"] == timing["intention"]
                             for status in completed["intentions"]))
        self.assertFalse(completed["state"]["observation"]["combat"]["preparation_active"])
        self.assertTrue(any(entry["content"].get("event", {}).get("target") == target
                            for entry in completed["history"]))
        noted, *_ = client.send({"type": "request", "request": {
            "type": "command", "branch": completed["branch"], "context": completed["input_context"],
            "command": {"type": "annotate", "anchor": {
                "type": "state", "revision": completed["state"]["revision"]},
                "text": "Fresh context after measured attack completion"}}})
        self.assertIsNone(noted["error"])
        self.assertEqual(noted["state"], completed["state"])
        (self.directory / "action-timing.json").write_text(json.dumps(timing), encoding="utf-8")

    def test_streaming_benchmark_emits_valid_nested_acquisition_profiles(self):
        from performance_report import validate
        rows = self.benchmark('latency_bench', ['--case', 'stream-r16-durable',
            '--cycles', '1', '--checkpoint-interval', '64'], 120)
        self.assertTrue(all(type(row['actor']) is int for row in rows if row['kind'] == 'sample'))
        metadata, samples, _ = validate(rows, selected_case='stream-r16-durable')
        self.assertEqual(metadata['stream-r16-durable']['region_acquisition_version'], 1)
        self.assertEqual(metadata['stream-r16-durable']['wire_profile_version'], 2)
        observations = [sample for sample in samples['stream-r16-durable'] if sample['observation_wire']]
        self.assertTrue(observations)
        self.assertTrue(all({'wire_encoding', 'wire_decoding'} <= sample['phases_ms'].keys()
                            for sample in observations))
        self.assertTrue(all('delta_encoding' not in sample['phases_ms'] for sample in observations))
        wire = [row for row in rows if row['kind'] == 'wire']
        self.assertEqual(len(wire), 1)
        self.assertEqual(wire[0]['n'], len(observations))
        self.assertLessEqual(wire[0]['sent_bytes_total'], wire[0]['full_bytes_total'])
        profiles = [sample['profile'] for sample in samples['stream-r16-durable']]
        self.assertGreater(sum(profile['regions_built'] for profile in profiles), 0)
        self.assertTrue(all('region_acquisition' in profile for profile in profiles))
        phases = {row['phase'] for row in rows if row['kind'] == 'summary'}
        self.assertTrue({'region_fallback_read', 'region_fallback_build'} <= phases)

class NativeWorkloadProcesses(ProcessTestCase):
    graphical = True

    def test_mixed_trace_is_presented_and_verified_for_complete_cycles(self):
        output = self.directory / "demo"
        regions = int(os.environ.get("TOR_PERFORMANCE_REGIONS", "8"))
        cycles = int(os.environ.get("TOR_PERFORMANCE_CYCLES", "1"))
        result = run_demo(self.bin, output, regions=regions, actors=1, cycles=cycles, pace=0, stay_open=False, correlate=True, defer_logs=True)
        from timing_correlation import correlate_ack
        correlated = correlate_ack(output, result)
        self.assertEqual(len(correlated), sum(s["expected"] != "blocked" for s in result["samples"]))
        self.assertEqual(result["cycles"], cycles)
        self.assertEqual(result["spectator_role"], "spectator")
        self.assertGreater(result["presented_revision"], result["initial_revision"])
        self.assertGreater(result["client_memory_end"], result["client_memory_start"])
        labels = {s["label"] for s in result["samples"]}
        self.assertTrue({"cross_region_boundary", "cross_boundary_with_los_change",
            "open_or_close_door", "change_elevation", "move_near_obstacle"} <= labels)
        self.assertTrue(all(s["request_to_ack_ms"] >= 0 for s in result["samples"] if s["expected"] != "blocked"))
        self.assertTrue(all(0 <= s["request_to_ack_line_ms"] <= s["request_to_ack_ms"]
                            for s in result["samples"] if s["expected"] != "blocked"))
        self.assertTrue(all(s["request_to_presentation_ms"] >= 0 for s in result["samples"] if s["expected"] != "blocked"))
        self.assertTrue(all(s["presentation_profile"]["version"] == 1 for s in result["samples"] if s["expected"] != "blocked"))
        self.assertTrue((output / "game.json").exists())
        self.assertEqual(json.loads((output / "result.json").read_text())["cycles"], cycles)

    def test_eight_real_clients_follow_scheduled_turns_and_visibility_changes(self):
        result = run_demo(self.bin, self.directory/"multi", regions=8, actors=8, cycles=1, pace=0)
        self.assertEqual({s["actor"] for s in result["samples"]}, set(range(1,9)))
        self.assertIn("multi_actor_visibility_change", {s["label"] for s in result["samples"]})

    def test_place_workload_uses_its_two_room_baseline_and_completes_simulated_waits(self):
        from place_performance_driver import run
        from place_performance_report import SPEC, validate_client
        for fresh in (False, True):
            with self.subTest(fresh_player=fresh):
                output = self.directory / ("fresh" if fresh else "places")
                run(self.bin, output, rooms=0, fresh_player=fresh)
                result = json.loads((output / "result.json").read_text(encoding="utf-8"))
                validate_client(result)
                self.assertEqual(len(result["final_places"]), 2)
                self.assertEqual(len(result["samples"]), SPEC["samples"])
                player_log = "fresh-player.stdout.jsonl" if fresh else "player.stdout.jsonl"
                # JsonProcess retains a killed child's partial final line, but
                # only newline-terminated records are frames. Complete malformed
                # records still fail, and the original diagnostic bytes stay intact.
                with (output / player_log).open("rb") as log:
                    rows = [json.loads(line.decode("utf-8")) for line in log if line.endswith(b"\n")]
                ready = [row for row in rows if row.get("type") == "ready"]
                self.assertTrue(ready)
                self.assertEqual(int(ready[-1]["state"]["observation"]["tick"]), SPEC["samples"] // 2 * 100)

    def test_saved_exploration_continues_after_durable_restart_with_exact_revisions(self):
        from saved_exploration_driver import run_saved_exploration, validate
        output = self.directory / "exploration"
        result = run_saved_exploration(self.bin, output, regions=8, interval=64, correlate=True, defer_logs=True)
        validate(result)
        self.assertTrue(result["restart_equal"])
        self.assertTrue(result["continued_after_restart"])
        self.assertIs(type(result["checkpoint_sequence"]), int)
        self.assertIs(type(result["journal_records"]), int)


class StoppedPlaceWorkloadFrames(unittest.TestCase):
    def ready(self, tick=None):
        from place_performance_report import SPEC
        if tick is None:
            tick = SPEC["samples"] // 2 * 100
        return json.dumps({"type": "ready", "state": {"observation": {"tick": str(tick)}}}).encode("utf-8")

    def check_log(self, raw):
        # Exercise the native fixture's actual post-run assertions without a
        # desktop. Workload execution/report validation have separate owners.
        from place_performance_report import SPEC
        with tempfile.TemporaryDirectory() as directory:
            case = NativeWorkloadProcesses(
                "test_place_workload_uses_its_two_room_baseline_and_completes_simulated_waits")
            case.directory = Path(directory)
            case.bin = case.directory / "unused-binaries"
            retained = []

            def run(binary_dir, output, *, rooms, fresh_player):
                self.assertEqual(binary_dir, case.bin)
                self.assertEqual(rooms, 0)
                output.mkdir()
                (output / "result.json").write_text(json.dumps({
                    "final_places": [{}, {}], "samples": [None] * SPEC["samples"]}), encoding="utf-8")
                name = "fresh-player.stdout.jsonl" if fresh_player else "player.stdout.jsonl"
                path = output / name
                path.write_bytes(raw)
                retained.append(path)

            try:
                with patch("place_performance_driver.run", side_effect=run), \
                        patch("place_performance_report.validate_client"):
                    case.test_place_workload_uses_its_two_room_baseline_and_completes_simulated_waits()
                self.assertEqual(len(retained), 2, "both original and fresh-player branches must run")
            finally:
                for path in retained:
                    self.assertEqual(path.read_bytes(), raw)

    def test_complete_frame_positive_control(self):
        for newline in (b"\n", b"\r\n"):
            with self.subTest(newline=newline):
                self.check_log(self.ready() + newline)

    def test_stopped_child_fragment_is_retained_but_is_not_a_frame(self):
        self.check_log(self.ready() + b'\n{"type":"ready","last_seen_revision":')

    def test_malformed_complete_records_still_fail(self):
        for record in (b'{"type":\n', b'not JSON\n', b'\n'):
            with self.subTest(record=record), self.assertRaises(json.JSONDecodeError):
                self.check_log(self.ready() + b"\n" + record)

    def test_unterminated_ready_cannot_prove_completed_waits(self):
        with self.assertRaises(AssertionError):
            self.check_log(self.ready(0) + b"\n" + self.ready())


    def test_lone_carriage_return_cannot_terminate_a_ready_frame(self):
        with self.assertRaises(AssertionError):
            self.check_log(self.ready(0) + b"\n" + self.ready() + b"\r")

    def test_stopped_child_partial_utf8_tail_is_retained_without_decoding(self):
        self.check_log(self.ready() + b'\n{"message":"\xe2\x82')

    def test_invalid_utf8_in_complete_frame_still_fails(self):
        with self.assertRaises(UnicodeDecodeError):
            self.check_log(self.ready() + b'\n{"message":"\xe2\x82\n')

    def test_complete_utf16_frame_cannot_replace_utf8_protocol_output(self):
        with self.assertRaises(UnicodeDecodeError):
            self.check_log(b"\xfe\xff" + (self.ready().decode("utf-8") + "\n").encode("utf-16-be"))


if __name__ == "__main__":
    unittest.main()
