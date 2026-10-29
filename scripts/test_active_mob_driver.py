"""Pure contracts for active-mob pacing, completed movement and activity evidence."""
import copy
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

import active_mob_driver as driver


class ScriptedRun:
    """Bounded process/setup doubles; never performance or application evidence."""
    def __init__(self, root, *, fail_move=False, fail_cleanup=False):
        self.root, self.fail_move, self.fail_cleanup = root, fail_move, fail_cleanup
        self.now, self.moves = 100., 0
        self.stops, self.tools = [], []
        self.output = root / "output"
        root.mkdir()
        suffix = ".exe" if driver.os.name == "nt" else ""
        binaries = {name: root / (name + suffix)
                    for name in ("tor-server", "tor-client-headless", "active_mob_bench")}
        self.benchmark = binaries["active_mob_bench"]
        for name, binary in binaries.items():
            binary.write_bytes(("scripted fixture: " + name).encode())
        self.manifest = root / "manifest.json"
        self.manifest.write_text(json.dumps(dict(
            source_commit="a" * 40, source_tree="b" * 40, dirty=False,
            profile="release", toolchain="scripted-test-fixture",
            binaries={name: driver.perf_ledger.sha256_file(binary) for name, binary in binaries.items()})),
            encoding="utf-8")

    def current(self):
        value = frame(center=f"cell-{self.moves}", target=f"cell-{self.moves + 1}")
        value.update(type="ready", branch="scripted-branch", history=[])
        value["state"]["revision"] = str(self.moves + 1)
        return value

    def sleep(self, seconds):
        assert seconds >= 0
        self.now += seconds

    def tool(self, binary, mode, save, output, label, env):
        self.tools.append((mode, label))
        if mode == "--bootstrap":
            package = output / "game.package"
            package.mkdir()
            return dict(workload="active-mobs-v1", mode="bootstrap", version=1,
                        seed=42, regions=20, ai=100, all_active=True, initial_records=0,
                        package=str(package))
        assert mode == "--inspect"
        return activity(self.moves)

    def run(self):
        fixture = self
        class Process:
            def __init__(self, binary, args, env, directory, name):
                assert binary.is_file(), "Scripted run must use the qualified fixture bytes"
                self.name, self.stopped = name, False
                self.child = SimpleNamespace(pid=1)
                self.last_reader_work_ms = self.last_queue_delay_ms = 0.

            def until(self, predicate, seconds=30):
                value = {"address": "scripted:0"} if self.name.endswith("server") else fixture.current()
                assert predicate(value), "Scripted process did not satisfy the real driver boundary"
                return value, fixture.now

            def send(self, value, *, wait_for_completion=False):
                began = fixture.now
                if value["type"] == "act":
                    assert value == {"type": "act", "action": {"type": "move", "direction": "east"}}
                    assert wait_for_completion
                    if fixture.fail_move and fixture.moves == 2:
                        raise ValueError("movement failed before authoritative outcome")
                    fixture.moves += 1
                    fixture.now += .01
                    self.action_timing = dict(outcome="resolved", request_to_admission_ms=1.,
                                              request_to_execution_ms=2., request_to_outcome_ms=3.,
                                              request_to_context_ms=4.)
                else:
                    assert value == {"type": "request", "request": {"type": "save"}}
                return fixture.current(), began, began + .001, fixture.now

            def snapshot(self):
                return fixture.current()

            def stop(self):
                if self.stopped:
                    return
                self.stopped = True
                fixture.stops.append(self.name)
                if fixture.fail_cleanup and self.name == "initial-headless":
                    raise RuntimeError("diagnostic log flush failed")

        with patch.object(driver, "JsonProcess", Process), \
                patch.object(driver, "run_tool", self.tool), \
                patch.object(driver, "rss", return_value=None), \
                patch.object(driver.perf_ledger, "probe_machine", return_value={"scripted": True}), \
                patch.object(driver.time, "perf_counter", side_effect=lambda: self.now), \
                patch.object(driver.time, "sleep", side_effect=self.sleep):
            return driver.run(self.root, self.benchmark, self.output, self.manifest,
                              duration=3.2, rate=5., burst=4, smoke=True)


def activity(moves=300):
    action = dict(executions=moves, moves=1, combat_starts=moves - 1, waits=0,
                  first_tick=0, last_tick=(moves - 1) * 100, portal_crossings=0)
    region = {**action, **{field: action[field] * 5 for field in
                            ("executions", "moves", "combat_starts", "waits", "portal_crossings")}}
    sustained = dict(meaningful_actions=moves, first_tick=0,
                     last_tick=(moves - 1) * 100, max_gap_ticks=100)
    return dict(workload=driver.WORKLOAD, version=1, mode="inspect", seed=42,
                regions=20, ai=100, all_active=True, all_alive=True,
                actor_ids=[str(i) for i in range(1, 102)], state_sha256="a" * 64,
                activity=dict(actors={str(i): action.copy() for i in range(2, 102)},
                              regions={str(i): region.copy() for i in range(1, 21)},
                              player_moves=moves, player_crossings=(moves + 1) // 8),
                sustained_activity=dict(player_moves=moves, player_turn_ticks=100,
                                        player_first_tick=0, player_last_tick=(moves - 1) * 100,
                                        max_gap_limit_ticks=200,
                                        actors={str(i): sustained.copy() for i in range(2, 102)}))


def sample(index=0, lateness=0):
    return dict(sample=index, started_seconds=index * .2 + lateness / 1000,
                next_ready_seconds=index * .2 + lateness / 1000 + .01,
                scheduled_seconds=index * .2, deadline_lateness_ms=lateness,
                before_cell=f"cell-{index}", expected_cell=f"cell-{index + 1}", after_cell=f"cell-{index + 1}",
                request_to_admission_ms=1, request_to_execution_ms=2, request_to_outcome_ms=3,
                request_to_context_ms=4, request_to_next_ready_ms=10)


def frame(*, ready=True, admission=True, center="old", target="new"):
    return dict(has_control=True, synchronized=True, error=None,
                readiness=dict(admission=admission), state=dict(revision="1", observation=dict(
                    ready=ready, visible_cells=[
                        dict(key=center, position=dict(x=0, y=0, z=0), wall=False),
                        dict(key=target, position=dict(x=1, y=0, z=0), wall=False)])))


class PacingAndTimings(unittest.TestCase):
    def test_smoke_has_finite_count_based_budget_without_relaxing_acceptance(self):
        self.assertEqual(driver.paced_completion_budget(60., 300, smoke=False), 90.)
        self.assertEqual(driver.paced_completion_budget(120., 1200, smoke=False), 150.)
        self.assertAlmostEqual(driver.paced_completion_budget(3.2, 16, smoke=True), 483.2)
        self.assertGreater(driver.paced_completion_budget(3.2, 16, smoke=True), 33.2)
        for duration, count, smoke in ((float("inf"), 16, True), (3.2, 0, True),
                                        (3.2, True, True), (3.2, 16, 1)):
            with self.subTest(inputs=(duration, count, smoke)), self.assertRaises(ValueError):
                driver.paced_completion_budget(duration, count, smoke=smoke)

    def test_smoke_cannot_claim_load_target_even_with_perfect_metrics(self):
        paced = driver.summarize([sample(i) for i in range(300)], 60., 5.)
        paced["completed_within_schedule"] = True
        self.assertTrue(driver.load_target_met(paced, 2., smoke=False))
        self.assertFalse(driver.load_target_met(paced, 2., smoke=True))
        self.assertFalse(driver.load_target_met(paced, 1., smoke=False))
        paced["completed_within_schedule"] = False
        self.assertFalse(driver.load_target_met(paced, 2., smoke=False))
        paced["completed_within_schedule"] = True
        paced["timings"]["request_to_next_ready_ms"]["p99_ms"] = 200.
        self.assertFalse(driver.load_target_met(paced, 2., smoke=False))

    def test_failure_receipt_preserves_original_error_and_all_cleanup_attempts(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            calls = []
            def good():
                calls.append("good")
            def bad():
                calls.append("bad")
                raise RuntimeError("reader failed")
            owned = [SimpleNamespace(name="good", stop=good), SimpleNamespace(name="bad", stop=bad)]
            result = {"complete": False, "samples": [sample()]}
            with self.assertRaisesRegex(RuntimeError, "cleanup/diagnostics failed"):
                driver.finalize_owned(owned, output, result, "original movement failure")
            self.assertEqual(calls, ["bad", "good"])
            failure = json.loads((output / "failure.json").read_text())
            self.assertEqual(failure["error"], "original movement failure")
            self.assertEqual(failure["cleanup_errors"], ["bad: reader failed"])
            self.assertEqual(failure["partial_result"]["cleanup"]["status"], "failed")
            self.assertEqual(len(failure["partial_result"]["samples"]), 1)
            self.assertFalse(failure["partial_result"]["complete"])
            self.assertFalse((output / "result.json").exists())

    def test_terminal_failure_receipt_records_successful_or_prior_failed_cleanup(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            result = {"complete": False}
            driver.finalize_owned([], output, result, "original failure")
            failure = json.loads((output / "failure.json").read_text())
            self.assertEqual(failure["error"], "original failure")
            self.assertEqual(failure["cleanup_errors"], [])
            self.assertEqual(failure["partial_result"]["cleanup"]["status"], "complete")
            # A save-and-stop failure cannot disappear when a repeated stop is
            # an idempotent no-op during final cleanup.
            result["diagnostics_error"] = ["client: previous reader failure"]
            driver.finalize_owned([], output, result, "save-and-stop failure")
            failure = json.loads((output / "failure.json").read_text())
            self.assertEqual(failure["error"], "save-and-stop failure")
            self.assertEqual(failure["cleanup_errors"], ["client: previous reader failure"])
            self.assertEqual(failure["partial_result"]["cleanup"]["status"], "failed")

    def test_successful_cleanup_does_not_create_a_failure_or_complete_the_run(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            result = {"complete": False}
            driver.finalize_owned([], output, result, None)
            self.assertEqual(result["cleanup"], {"status": "complete", "errors": []})
            self.assertFalse(result["complete"])
            self.assertFalse((output / "failure.json").exists())
            self.assertFalse((output / "result.json").exists())

    def test_deadlines_remain_anchored_after_a_slow_command(self):
        self.assertEqual(driver.deadline(100, 1, 5), 100.2)
        self.assertEqual(driver.deadline(100, 2, 5), 100.4)
        # Finishing slot one at100.7 does not reset slot two to100.9.
        rows = [sample(0), sample(1, 500), sample(2, 600), sample(3, 700)]
        result = driver.summarize(rows, 1., 5.)
        self.assertEqual(result["maximum_driver_backlog_slots"], 3)
        self.assertEqual(result["late_minus_early_median_ms"], 700)
        self.assertEqual(result["completed_hz"], 4.)

    def test_p99_is_nearest_rank_not_interpolated_or_dropped(self):
        result = driver.distribution(list(range(1, 101)))
        self.assertEqual(result, dict(n=100, p50_ms=50, p95_ms=95, p99_ms=99, max_ms=100))
        for invalid in ([], [True], [-1], [float("nan")], [float("inf")]):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                driver.distribution(invalid)

    def test_complete_moves_and_ordered_timing_boundaries_are_required(self):
        driver.summarize([sample()], .2, 5)
        for mutation in (lambda r: r.update(sample=1), lambda r: r.update(sample=True),
                         lambda r: r.update(after_cell=r["before_cell"]),
                         lambda r: r.update(request_to_next_ready_ms=1),
                         lambda r: r.update(request_to_outcome_ms=None),
                         lambda r: r.update(next_ready_seconds=-1)):
            broken = sample()
            mutation(broken)
            with self.subTest(broken=broken), self.assertRaises(ValueError):
                driver.summarize([broken], .2, 5)

    def test_readiness_wait_includes_simulation_round_not_only_admission(self):
        self.assertFalse(driver.ready_for_next(frame(ready=False, admission=True)))
        self.assertFalse(driver.ready_for_next(frame(ready=True, admission=False)))
        self.assertTrue(driver.ready_for_next(frame()))
        self.assertFalse(driver.ready_for_next(dict(state_omitted=True, error=None)))
        for changes in (dict(has_control=False), dict(error="rejected")):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                driver.ready_for_next({**frame(), **changes})

    def test_move_waits_for_next_ready_and_keeps_admission_distinct(self):
        before = frame()
        context = frame(ready=False, center="new", target="third")
        context["state"]["revision"] = "2"
        ready = copy.deepcopy(context)
        ready["state"]["observation"]["ready"] = True
        seen = []
        def send(value, **options):
            seen.append((value, options))
            return context, 10., 10.001, 10.004
        client = SimpleNamespace(send=send, until=lambda predicate: (ready, 10.010),
                                 action_timing=dict(outcome="resolved", request_to_admission_ms=1,
                                                    request_to_execution_ms=2, request_to_outcome_ms=3,
                                                    request_to_context_ms=4),
                                 last_reader_work_ms=0, last_queue_delay_ms=0)
        after, row = driver.move(client, before, 10., 0, 10.)
        self.assertIs(after, ready)
        self.assertAlmostEqual(row["request_to_next_ready_ms"], 10.)
        self.assertEqual(row["request_to_admission_ms"], 1)
        self.assertEqual(seen, [({"type": "act", "action": {"type": "move", "direction": "east"}},
                                 {"wait_for_completion": True})])
        client.action_timing["outcome"] = "failed"
        with self.assertRaisesRegex(ValueError, "authoritatively resolved"):
            driver.move(client, before, 10., 0, 10.)

    def test_move_waits_past_ready_control_with_retained_old_picture(self):
        before = frame()
        fresh = frame(center="new", target="third")
        fresh["state"]["revision"] = "2"
        def until(predicate):
            self.assertFalse(predicate(before))
            self.assertTrue(predicate(fresh))
            return fresh, 10.010
        client = SimpleNamespace(
            send=lambda *args, **kwargs: (before, 10., 10.001, 10.004), until=until,
            action_timing=dict(outcome="resolved", request_to_admission_ms=1,
                              request_to_execution_ms=2, request_to_outcome_ms=3,
                              request_to_context_ms=4),
            last_reader_work_ms=0, last_queue_delay_ms=0)
        after, row = driver.move(client, before, 10., 0, 10.)
        self.assertIs(after, fresh)
        self.assertAlmostEqual(row["request_to_next_ready_ms"], 10.)
        fresh["state"]["observation"]["visible_cells"][0]["key"] = "wrong"
        with self.assertRaisesRegex(ValueError, "disclosed destination"):
            driver.move(client, before, 10., 0, 10.)

    def test_restart_ignores_only_ephemeral_picture_identity(self):
        before = dict(frame(), branch="same", history=[{"id": "record"}])
        before["state"]["snapshot"] = "old"
        after = copy.deepcopy(before)
        after["state"].update(snapshot="fresh", revision="9")
        self.assertTrue(driver.same_disclosed_state(before, after))
        for mutate in (lambda x: x["state"]["observation"].update(ready=False),
                       lambda x: x.update(branch="other"),
                       lambda x: x["history"].append({"id": "extra"})):
            changed = copy.deepcopy(after)
            mutate(changed)
            self.assertFalse(driver.same_disclosed_state(before, changed))

    def test_cell_oracle_uses_disclosed_opaque_destination(self):
        current = frame(center="opaque-old", target="opaque-east")
        self.assertEqual(driver.center(current), "opaque-old")
        self.assertEqual(driver.east_destination(current), "opaque-east")
        current["state"]["observation"]["visible_cells"][1]["wall"] = True
        with self.assertRaises(ValueError):
            driver.east_destination(current)

    def test_memory_growth_keeps_server_and_driver_accounting_separate(self):
        result = driver.memory_summary([
            dict(seconds=0, server_rss_bytes=1000, client_rss_bytes=None, driver_rss_bytes=100),
            dict(seconds=10, server_rss_bytes=2000, client_rss_bytes=None, driver_rss_bytes=400)])
        self.assertEqual(result["server_rss_bytes"]["endpoint_growth_bytes_per_second"], 100)
        self.assertEqual(result["driver_rss_bytes"]["growth_bytes"], 300)
        self.assertIsNone(result["client_rss_bytes"])


class ActivityEvidence(unittest.TestCase):
    def test_all_active_identities_and_live_regions_are_required(self):
        expected = activity()
        self.assertEqual(driver.validate_activity(expected, 300), expected["activity"])
        mutations = [
            lambda r: r.update(all_active=False), lambda r: r.update(all_alive=False),
            lambda r: r.update(version=True), lambda r: r["actor_ids"].pop(),
            lambda r: r["activity"]["actors"].pop("101"),
            lambda r: r["activity"]["regions"].pop("20"),
            lambda r: r["activity"]["actors"]["2"].update(executions=1),
            lambda r: r["activity"]["actors"]["2"].update(last_tick=0),
            lambda r: r["activity"]["actors"]["2"].update(moves=0, combat_starts=0),
            lambda r: r["activity"]["regions"]["20"].update(executions=1),
            lambda r: r["activity"]["regions"]["20"].update(moves=0),
            lambda r: r["activity"]["regions"]["20"].update(combat_starts=0),
            lambda r: r["activity"].update(player_moves=299),
            lambda r: r["activity"].update(player_crossings=0),
            lambda r: r.update(state_sha256="missing")]
        for mutate in mutations:
            broken = copy.deepcopy(expected)
            mutate(broken)
            with self.subTest(mutation=mutate), self.assertRaises(ValueError):
                driver.validate_activity(broken, 300)

    def test_sustained_activity_rejects_hidden_middle_gap_without_changing_totals(self):
        expected = activity()
        expected["sustained_activity"]["actors"]["101"]["max_gap_ticks"] = 200
        driver.validate_activity(expected, 300)
        broken = copy.deepcopy(expected)
        broken["sustained_activity"]["actors"]["101"]["max_gap_ticks"] = 201
        self.assertEqual(broken["activity"], expected["activity"])
        for key in ("meaningful_actions", "first_tick", "last_tick"):
            self.assertEqual(broken["sustained_activity"]["actors"]["101"][key],
                             expected["sustained_activity"]["actors"]["101"][key])
        with self.assertRaisesRegex(ValueError, "AI 101 meaningful action gap"):
            driver.validate_activity(broken, 300)

    def test_sustained_activity_requires_late_work_canonical_window_and_all_ais(self):
        expected = activity()
        expected["sustained_activity"]["actors"]["101"]["last_tick"] = 29700
        driver.validate_activity(expected, 300)  # Exactly 200-tick trailing gap is allowed.
        broken = copy.deepcopy(expected)
        broken["sustained_activity"]["actors"]["101"]["last_tick"] = 29699
        self.assertEqual(broken["activity"], expected["activity"])
        with self.assertRaisesRegex(ValueError, "AI 101 stopped meaningful activity"):
            driver.validate_activity(broken, 300)
        for mutate in (
                lambda r: r.pop("sustained_activity"),
                lambda r: r["sustained_activity"].update(player_last_tick=29901),
                lambda r: r["sustained_activity"].update(player_turn_ticks=101),
                lambda r: r["sustained_activity"].update(max_gap_limit_ticks=201),
                lambda r: r["sustained_activity"]["actors"].pop("101"),
                lambda r: r["sustained_activity"]["actors"]["101"].update(first_tick=1),
                lambda r: r["sustained_activity"]["actors"]["101"].update(meaningful_actions=299)):
            broken = activity()
            mutate(broken)
            with self.subTest(mutation=mutate), self.assertRaises(ValueError):
                driver.validate_activity(broken, 300)

    def test_scripted_paths_cover_both_driver_platform_branches(self):
        # Replace only the driver's OS facade: pathlib and the host filesystem
        # remain native, and no applications are launched by these doubles.
        host_os = driver.os
        with tempfile.TemporaryDirectory() as directory:
            for platform, suffix in (("posix", ""), ("nt", ".exe")):
                with self.subTest(platform=platform), patch.object(driver, "os", SimpleNamespace(
                        name=platform, environ=host_os.environ, getpid=host_os.getpid)):
                    scripted = ScriptedRun(Path(directory) / platform)
                    self.assertEqual(scripted.benchmark.name, "active_mob_bench" + suffix)
                    self.assertTrue((scripted.root / ("tor-server" + suffix)).is_file())
                    self.assertTrue((scripted.root / ("tor-client-headless" + suffix)).is_file())
                    self.assertTrue(scripted.run()["complete"])
                    self.assertEqual(scripted.stops, ["initial-headless", "initial-server",
                                                     "restart-headless", "restart-server"])

    def test_run_preserves_causal_failure_and_records_terminal_cleanup(self):
        # First establish the existing full run path, including actual local
        # manifest validation, pacing/move checks, activity validation and stop_all.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            positive = ScriptedRun(root / "positive")
            result = positive.run()
            self.assertTrue(result["complete"])
            self.assertTrue(result["restart_equal"])
            self.assertTrue(result["source_package_removed_from_original_path"])
            self.assertEqual((len(result["samples"]), len(result["burst_samples"])), (16, 4))
            self.assertEqual(positive.moves, 20)
            self.assertEqual(positive.stops, ["initial-headless", "initial-server",
                                              "restart-headless", "restart-server"])
            self.assertEqual(positive.tools, [("--bootstrap", "bootstrap"),
                                              ("--inspect", "measured-inspect"),
                                              ("--inspect", "final-inspect")])
            self.assertTrue((positive.output / "result.json").exists())
            self.assertFalse((positive.output / "failure.json").exists())

            for cleanup_fails in (False, True):
                with self.subTest(cleanup_fails=cleanup_fails):
                    broken = ScriptedRun(root / str(cleanup_fails), fail_move=True, fail_cleanup=cleanup_fails)
                    with self.assertRaises(RuntimeError if cleanup_fails else ValueError):
                        broken.run()
                    failure = json.loads((broken.output / "failure.json").read_text())
                    self.assertEqual(broken.stops, ["initial-headless", "initial-server"])
                    self.assertEqual(len(failure["partial_result"]["samples"]), 2)
                    self.assertFalse(failure["partial_result"]["complete"])
                    self.assertFalse((broken.output / "result.json").exists())
                    # Independent causal strings: the run failure must survive
                    # even when stop_all also reports a diagnostic failure.
                    self.assertEqual(failure["error"], "movement failed before authoritative outcome")
                    errors = ["initial-headless: diagnostic log flush failed"] if cleanup_fails else []
                    self.assertEqual(failure["partial_result"].get("cleanup"),
                                     {"status": "failed" if cleanup_fails else "complete", "errors": errors})
                    self.assertEqual(failure.get("cleanup_errors"), errors)

    def test_burst_only_activity_cannot_stand_for_paced_phase(self):
        burst_only = activity(320)
        with self.assertRaises(ValueError):
            driver.validate_activity(burst_only, 300)
        driver.validate_activity(burst_only, 320)

    def test_build_manifest_binds_release_bytes_not_a_filename(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binaries = {name: root / name for name in ("tor-server", "tor-client-headless", "active_mob_bench")}
            for name, binary in binaries.items():
                binary.write_bytes(name.encode())
            manifest = dict(source_commit="a" * 40, source_tree="b" * 40, dirty=False,
                            profile="release", toolchain="fixture-toolchain",
                            binaries={name: driver.perf_ledger.sha256_file(path) for name, path in binaries.items()})
            path = root / "manifest.json"
            path.write_text(json.dumps(manifest), encoding="utf-8")
            driver.verify_build_manifest(path, binaries)
            binaries["tor-server"].write_bytes(b"different executable")
            with self.assertRaisesRegex(ValueError, "executable bytes"):
                driver.verify_build_manifest(path, binaries)
            binaries["tor-server"].write_bytes(b"tor-server")
            manifest["profile"] = "debug"
            path.write_text(json.dumps(manifest), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "release"):
                driver.verify_build_manifest(path, binaries)
            driver.verify_build_manifest(path, binaries, smoke=True)
            with self.assertRaisesRegex(ValueError, "explicit"):
                driver.verify_build_manifest(path, binaries, smoke=1)
            manifest["profile"] = "custom"
            path.write_text(json.dumps(manifest), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "release"):
                driver.verify_build_manifest(path, binaries, smoke=True)


if __name__ == "__main__":
    unittest.main()
