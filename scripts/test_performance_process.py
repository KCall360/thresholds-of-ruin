"""The benchmark trace driven through real headless clients and ASCII frames."""
import json
import os
import subprocess
import unittest
from performance_driver import JsonProcess, run_demo

from process_harness import ProcessTestCase, ROOT, SUFFIX, TOKEN


class PerformanceProcesses(ProcessTestCase):
    def test_rogue_benchmark_preserves_floor_work_and_residency_bounds(self):
        from performance_report import validate_rogue
        build = ['cargo', 'build', '-p', 'tor-server', '--example', 'rogue_bench', '--locked']
        if os.environ.get('TOR_TEST_PROFILE', 'debug') == 'release':
            build.append('--release')
        with (self.directory/'rogue-build.log').open('w', encoding='utf-8') as log:
            subprocess.run(build, cwd=ROOT, stdout=log, stderr=log, check=True, timeout=600)
        with (self.directory/'rogue.jsonl').open('w', encoding='utf-8') as log, (self.directory/'rogue-stderr.log').open('w', encoding='utf-8') as errors:
            subprocess.run([self.bin/'examples'/('rogue_bench'+SUFFIX), '1'],
                           cwd=ROOT, stdout=log, stderr=errors, check=True, timeout=180)
        rows = [json.loads(line) for line in (self.directory/'rogue.jsonl').read_text(encoding='utf-8').splitlines()]
        validate_rogue(rows)
        with self.assertRaises(AssertionError):
            validate_rogue(rows[:-1])

    def test_multiphase_attack_timing_reaches_authoritative_outcome_and_fresh_context(self):
        self.server(scenario="dungeon-loop", seed=None)
        client = JsonProcess(self.bin / ("tor-client-headless" + SUFFIX),
                             ["--connect", self.address],
                             {**os.environ, "TOR_SERVER_TOKEN": TOKEN}, self.directory, "timed-player")
        self.addCleanup(client.stop)
        initial, _ = client.until(lambda frame: frame.get("type") == "ready")
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                      if actor["name"] == "ruin guard")
        completed, *_ = client.send({"type": "act", "action": {"type": "attack", "target": target}},
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
