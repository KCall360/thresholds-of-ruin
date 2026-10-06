"""The benchmark trace driven through real headless clients and ASCII frames."""
import json
import os
import subprocess
import unittest
from performance_driver import run_demo

from process_harness import ProcessTestCase, ROOT, SUFFIX


class PerformanceProcesses(ProcessTestCase):
    def test_streaming_benchmark_emits_valid_nested_acquisition_profiles(self):
        from performance_report import validate
        build = ['cargo', 'build', '-p', 'tor-server', '--example', 'latency_bench', '--locked']
        if os.environ.get('TOR_TEST_PROFILE', 'debug') == 'release':
            build.append('--release')
        with (self.directory/'benchmark-build.log').open('w', encoding='utf-8') as log:
            subprocess.run(build, cwd=ROOT, stdout=log, stderr=log, check=True, timeout=600)
        output = self.directory/'acquisition.jsonl'
        with output.open('w', encoding='utf-8') as log, (self.directory/'benchmark-stderr.log').open('w', encoding='utf-8') as errors:
            subprocess.run([self.bin/'examples'/('latency_bench'+SUFFIX), '--case',
                'stream-r16-durable', '--cycles', '1', '--checkpoint-interval', '64'],
                cwd=ROOT, stdout=log, stderr=errors, check=True, timeout=120)
        rows = [json.loads(line) for line in output.read_text(encoding='utf-8').splitlines()]
        self.assertTrue(all(type(row['actor']) is int for row in rows if row['kind'] == 'sample'))
        metadata, samples, _ = validate(rows, selected_case='stream-r16-durable')
        self.assertEqual(metadata['stream-r16-durable']['region_acquisition_version'], 1)
        profiles = [sample['profile'] for sample in samples['stream-r16-durable']]
        self.assertGreater(sum(profile['regions_built'] for profile in profiles), 0)
        self.assertTrue(all('region_acquisition' in profile for profile in profiles))
        phases = {row['phase'] for row in rows if row['kind'] == 'summary'}
        self.assertTrue({'region_fallback_read', 'region_fallback_build'} <= phases)

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



class NativeWorkloadProcesses(ProcessTestCase):
    graphical = True

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
                rows = [json.loads(line) for line in (output / player_log).read_text(encoding="utf-8").splitlines()]
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


if __name__ == "__main__":
    unittest.main()
