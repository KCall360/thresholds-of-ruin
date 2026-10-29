import copy
import unittest
from workload_report import InvalidWorkload
import json
import subprocess
import sys
from pathlib import Path
from performance_report import PRELOAD_BOUNDS, SPEC, STREAM_BOUNDS, expected_actions, validate, validate_work, validate_region_acquisition


class PerformanceReportTests(unittest.TestCase):
    def test_single_actor_cycle_has_exact_actions_and_free_blocked_attempts(self):
        actions = expected_actions(1,1,0,1)
        self.assertEqual(len(actions),29)
        self.assertEqual(sum(a[3] != "blocked" for a in actions),27)
        self.assertEqual(sum(a[1] == "change_elevation" for a in actions),2)
        self.assertEqual(sum(a[1] == "open_or_close_door" for a in actions),4)

    def test_seeded_partial_round_starts_with_the_ready_actor_and_preserves_mix(self):
        actions = expected_actions(8,8,100,1)
        self.assertEqual([a[0] for a in actions[:5]], [5,6,7,8,1])
        primary = [a for a in actions if a[0] == 1]
        self.assertEqual(primary,expected_actions(8,1,0,1))
        self.assertEqual({a[0] for a in actions},set(range(1,9)))


class PhaseDWorkTests(unittest.TestCase):
    def test_wait_contract_rejects_reintroduced_scene_work(self):
        profile = dict(actors_observed=0, perception_calls=0, scene_calls=0,
                       navigation_refreshes=0, revision_comparisons=8,
                       candidate_captures=1, rollback_snapshots=1)
        validate_work(profile, {"type":"wait"}, 8)
        profile["scene_calls"] = 1
        with self.assertRaises(InvalidWorkload):
            validate_work(profile, {"type":"wait"}, 8)

    def test_movement_contract_rejects_repeated_projection(self):
        profile = dict(actors_observed=16, perception_calls=16, scene_calls=16,
                       candidate_captures=1, rollback_snapshots=1)
        validate_work(profile, {"type":"move"}, 8)
        profile["scene_calls"] = 17
        with self.assertRaises(InvalidWorkload):
            validate_work(profile, {"type":"move"}, 8)

    def test_movement_can_disclose_fresh_views_from_cached_scene_projections(self):
        profile = dict(actors_observed=16, perception_calls=16, scene_calls=0,
                       candidate_captures=1, rollback_snapshots=1)
        validate_work(profile, {"type":"move"}, 8)

    def test_observation_and_projection_counts_keep_separate_integer_work_bounds(self):
        profile = dict(actors_observed=16, perception_calls=16, scene_calls=0,
                       candidate_captures=1, rollback_snapshots=1)
        for field in ('scene_calls', 'perception_calls'):
            for value in (True, False, -1, 0.0, 1.5, 17):
                with self.subTest(field=field, value=value):
                    with self.assertRaises(InvalidWorkload):
                        validate_work({**profile, field: value}, {"type":"move"}, 8)
        # Door validation can request one extra observation and projection.
        validate_work({**profile, 'perception_calls': 17, 'scene_calls': 17},
                      {"type":"set_door"}, 8)

    def test_cached_projections_still_require_fresh_disclosure_for_each_actor(self):
        profile = dict(actors_observed=16, perception_calls=8, scene_calls=0,
                       candidate_captures=1, rollback_snapshots=1)
        validate_work(profile, {"type":"move"}, 8)
        for calls in (0, 7):
            with self.subTest(perception_calls=calls):
                with self.assertRaises(InvalidWorkload):
                    validate_work({**profile, 'perception_calls': calls}, {"type":"move"}, 8)


class BoundaryProofWorkTests(unittest.TestCase):
    proof_fields = ("observation_changes_proven", "observations_unchanged_proven",
                    "observation_projection_fallbacks")

    def profile(self):
        # Eight observers: two proven changed, four proven unchanged, two cold
        # comparisons. One additional rich view records the mover's map.
        return dict(actors_observed=5, perception_calls=3, scene_calls=1,
                    revision_comparisons=8, candidate_captures=1, rollback_snapshots=1,
                    observation_changes_proven=2, observations_unchanged_proven=4,
                    observation_projection_fallbacks=2)

    def test_complete_boundary_proofs_replace_eager_disclosure_requirements(self):
        # Establish the legacy fixture before exercising the new valid contract.
        legacy = dict(actors_observed=16, perception_calls=8, scene_calls=0,
                      candidate_captures=1, rollback_snapshots=1)
        validate_work(legacy, {"type": "move"}, 8)
        validate_work(self.profile(), {"type": "move"}, 8)
        validate_work({**self.profile(), "perception_calls": 5}, {"type": "move"}, 8)
        validate_work({**self.profile(), "observation_changes_proven": 8,
                       "observations_unchanged_proven": 0, "observation_projection_fallbacks": 0,
                       "actors_observed": 1, "perception_calls": 1}, {"type": "move"}, 8)
        validate_work({**self.profile(), "observation_changes_proven": 0,
                       "observations_unchanged_proven": 0, "observation_projection_fallbacks": 8,
                       "actors_observed": 16, "perception_calls": 8}, {"type": "move"}, 8)

    def test_proof_fields_are_atomic_and_strict_nonnegative_integer_counts(self):
        # Use otherwise-valid legacy work so missing fields cannot be rejected
        # merely by the old eager counts while silently bypassing schema checks.
        legacy = dict(actors_observed=16, perception_calls=8, scene_calls=0,
                      revision_comparisons=8, candidate_captures=1, rollback_snapshots=1)
        for field in self.proof_fields:
            with self.subTest(single_field=field), self.assertRaises(InvalidWorkload):
                validate_work({**legacy, field: 0}, {"type": "move"}, 8)
        for missing in self.proof_fields:
            profile = {**self.profile(), **legacy}
            del profile[missing]
            with self.subTest(missing=missing), self.assertRaises(InvalidWorkload):
                validate_work(profile, {"type": "move"}, 8)
        for field in (*self.proof_fields, "revision_comparisons"):
            for value in (True, False, -1, 0.0, 1.5, None, "2"):
                with self.subTest(field=field, value=value), self.assertRaises(InvalidWorkload):
                    validate_work({**self.profile(), field: value}, {"type": "move"}, 8)
        profile = self.profile()
        del profile["revision_comparisons"]
        with self.assertRaises(InvalidWorkload):
            validate_work(profile, {"type": "move"}, 8)

    def test_every_observer_requires_exactly_one_proof_or_cold_comparison(self):
        for field, value in (("observation_changes_proven", 1),
                             ("observations_unchanged_proven", 5),
                             ("observation_projection_fallbacks", 1),
                             ("revision_comparisons", 7), ("revision_comparisons", 9)):
            with self.subTest(field=field, value=value), self.assertRaises(InvalidWorkload):
                validate_work({**self.profile(), field: value}, {"type": "move"}, 8)

    def test_proofs_do_not_hide_missing_or_repeated_rich_work(self):
        # Every fallback accounts for two logical views and at least one actual
        # fresh view; its old view alone may be supplied by the committed cache.
        for field, values in (("actors_observed", (3, 11)),
                              ("perception_calls", (2, 6)), ("scene_calls", (17,))):
            for value in values:
                with self.subTest(field=field, value=value), self.assertRaises(InvalidWorkload):
                    validate_work({**self.profile(), field: value}, {"type": "move"}, 8)
        for field in ("actors_observed", "perception_calls", "scene_calls"):
            for value in (True, False, -1, 0.0, 1.5):
                with self.subTest(field=field, value=value), self.assertRaises(InvalidWorkload):
                    validate_work({**self.profile(), field: value}, {"type": "move"}, 8)

    def test_door_validation_keeps_its_single_extra_projection_allowance(self):
        profile = {**self.profile(), "observation_changes_proven": 8,
                   "observations_unchanged_proven": 0, "observation_projection_fallbacks": 0,
                   "actors_observed": 8, "perception_calls": 9, "scene_calls": 8}
        validate_work(profile, {"type": "set_door"}, 8)
        with self.assertRaises(InvalidWorkload):
            validate_work({**profile, "perception_calls": 10}, {"type": "set_door"}, 8)
        with self.assertRaises(InvalidWorkload):
            validate_work({**profile, "scene_calls": 18}, {"type": "set_door"}, 8)

    def test_wait_retains_its_zero_work_contract_with_zero_boundary_proofs(self):
        profile = {**self.profile(), **dict.fromkeys(self.proof_fields, 0),
                   "actors_observed": 0, "perception_calls": 0, "scene_calls": 0,
                   "navigation_refreshes": 0}
        validate_work(profile, {"type": "wait"}, 8)
        for field in (*self.proof_fields, "actors_observed", "perception_calls", "scene_calls",
                      "navigation_refreshes"):
            with self.subTest(field=field), self.assertRaises(InvalidWorkload):
                validate_work({**profile, field: 1}, {"type": "wait"}, 8)
        for field in ("revision_comparisons", "navigation_refreshes"):
            with self.subTest(field=field), self.assertRaises(InvalidWorkload):
                validate_work({**profile, field: False}, {"type": "wait"}, 8)

    def test_absent_proof_fields_cannot_relax_legacy_work_requirements(self):
        profile = self.profile()
        for field in self.proof_fields:
            del profile[field]
        with self.assertRaises(InvalidWorkload):
            validate_work(profile, {"type": "move"}, 8)


class DiscoveryValidationTests(unittest.TestCase):
    def rows(self):
        rows = []
        for regions in (8, 256):
            case = f"traversal-r{regions}"
            rows.append(dict(kind="traversal", case=case, cycles=2, regions=regions, trace_version=1, profile_version=1))
            for index, step in enumerate(SPEC["traversal"] * 2):
                action = step["action"].copy()
                if action["type"] == "door":
                    action = dict(type="set_door", open=action["open"], door=1)
                rows.append(dict(kind="sample", case=case, actor=1, label=step["label"],
                                 action=action, expected=step["expected"], history_start=index,
                                 history_end=index+1, client_memory=index+1,
                                 phases_ms={"authoritative_total":1.}))
            rows.append(dict(kind="traversal_end", case=case, history_end=index+1, client_memory=index+1))
        return rows

    def test_complete_discovery_is_validated_independently_of_mixed_cases(self):
        validate(self.rows(), quick=True, discovery_only=True)

    def test_all_present_phases_require_finite_nonnegative_nonboolean_measurements(self):
        for field in ("authoritative_total", "command_call", "disclosure_projection", "rendering"):
            for value in (float("nan"), float("inf"), -1, True, None, "1"):
                rows = self.rows()
                rows[1]["phases_ms"][field] = value
                with self.subTest(field=field, value=value), self.assertRaises(InvalidWorkload):
                    validate(rows, quick=True, discovery_only=True)
        for value in ({}, None):
            rows = self.rows()
            rows[1]["phases_ms"] = value
            with self.subTest(value=value), self.assertRaises(InvalidWorkload):
                validate(rows, quick=True, discovery_only=True)
        rows = self.rows()
        rows[1].pop("phases_ms")
        with self.assertRaises(InvalidWorkload):
            validate(rows, quick=True, discovery_only=True)

    def test_counts_and_profile_durations_cannot_be_boolean_or_malformed(self):
        for field in ("actor", "history_start", "history_end", "client_memory"):
            rows = self.rows()
            rows[1][field] = bool(rows[1][field])
            with self.subTest(field=field), self.assertRaises(InvalidWorkload):
                validate(rows, quick=True, discovery_only=True)
        for value in (0, True, 1.0, {}, {"secs":0}, {"secs":True, "nanos":0}, {"secs":0, "nanos":1_000_000_000}):
            rows = self.rows()
            rows[1]["profile"] = {"authoritative_total":value}
            with self.subTest(value=value), self.assertRaises(InvalidWorkload):
                validate(rows, quick=True, discovery_only=True)

    def test_scalar_end_measurements_cannot_masquerade_as_phase_maps(self):
        for value in ({}, {"fake_phase":1.}, [], True, None):
            rows = self.rows()
            rows[-1]["restart_replay_ms"] = value
            with self.subTest(value=value), self.assertRaises(InvalidWorkload):
                validate(rows, quick=True, discovery_only=True)

    def test_actual_optimized_interpreter_keeps_work_and_report_checks(self):
        cases = [
            ("validate_work", [dict(actors_observed=0, perception_calls=0, scene_calls=0,
                navigation_refreshes=0, revision_comparisons=8, candidate_captures=1, rollback_snapshots=1),
                {"type":"wait"}, 8], {}, lambda args: args[0].update(scene_calls=999)),
            ("validate", [self.rows()], {"quick":True, "discovery_only":True}, lambda args: args[0].pop()),
        ]
        for entry, args, kwargs, corrupt in cases:
            for valid in (True, False):
                if not valid:
                    corrupt(args)
                child = subprocess.run([sys.executable, "-O", "-c",
                    "import json,sys,performance_report; p=json.load(sys.stdin); "
                    "getattr(performance_report,p['entry'])(*p['args'],**p['kwargs'])"],
                    input=json.dumps(dict(entry=entry,args=args,kwargs=kwargs)), text=True,
                    capture_output=True, cwd=Path(__file__).parent, timeout=15)
                with self.subTest(entry=entry, valid=valid):
                    self.assertEqual(child.returncode == 0, valid, child.stderr)
                    if not valid:
                        self.assertIn("InvalidWorkload:", child.stderr)

    def test_missing_completion_is_not_a_successful_benchmark(self):
        rows = self.rows()
        rows.pop()
        with self.assertRaises(InvalidWorkload):
            validate(rows, quick=True, discovery_only=True)

    def test_reordered_actions_and_duplicate_metadata_fail(self):
        rows = self.rows()
        rows[1], rows[2] = rows[2], rows[1]
        with self.assertRaises(InvalidWorkload):
            validate(rows, quick=True, discovery_only=True)
        rows = self.rows()
        rows.append(rows[0])
        with self.assertRaises(InvalidWorkload):
            validate(rows, quick=True, discovery_only=True)

class CompleteWireValidationTests(unittest.TestCase):
    def rows(self):
        rows = DiscoveryValidationTests().rows()
        for row in rows:
            if row["kind"] == "traversal":
                row["wire_profile_version"] = 2
            elif row["kind"] == "sample":
                row["observation_wire"] = dict(version=2, full_bytes=1000, sent_bytes=400, delta=True)
                row["phases_ms"].update(wire_encoding=0.2, wire_decoding=0.3)
        for case in ("traversal-r8", "traversal-r256"):
            n = sum(r["kind"] == "sample" and r["case"] == case for r in rows)
            rows.append(dict(kind="wire", case=case, wire_profile_version=2, n=n, deltas=n,
                             full_bytes_total=1000*n, sent_bytes_total=400*n,
                             sent_p50_bytes=400, sent_p95_bytes=400, sent_max_bytes=400))
        return rows

    def test_complete_envelope_profile_is_valid(self):
        validate(self.rows(), quick=True, discovery_only=True)

    def test_corrupt_or_incomplete_wire_measurements_fail(self):
        mutations = [
            ("wrong total", lambda r: r[-1].update(sent_bytes_total=1)),
            ("missing summary", lambda r: r.pop()),
            ("enlarged delta", lambda r: r[1]["observation_wire"].update(sent_bytes=1001)),
            ("nonboolean delta flag", lambda r: r[1]["observation_wire"].update(delta=1)),
            ("nonfinite decoding", lambda r: r[1]["phases_ms"].update(wire_decoding=float("nan"))),
            ("missing encoding", lambda r: r[1]["phases_ms"].pop("wire_encoding")),
            ("missing profile version", lambda r: r[0].pop("wire_profile_version")),
            ("mixed summary versions", lambda r: r[-1].update(wire_profile_version=1)),
            ("legacy construction timing", lambda r: r[1]["phases_ms"].update(delta_encoding=0.1)),
            ("wrong percentile", lambda r: r[-1].update(sent_p95_bytes=401)),
            ("missing envelope", lambda r: r[1].pop("observation_wire")),
        ]
        for name, mutate in mutations:
            with self.subTest(corruption=name):
                rows = self.rows()
                mutate(rows)
                with self.assertRaises(InvalidWorkload):
                    validate(rows, quick=True, discovery_only=True)


class SavedDiscoveryTests(unittest.TestCase):
    def fixture(self):
        samples = [dict(history_end=i, profile=dict(checkpoint_captures=int(i % 2 == 0), records_serialized=1),
                        save_status=dict(error=None, accepted_sequence=i, durable_sequence=0)) for i in range(1,6)]
        status = dict(error=None, pending_bytes=0, accepted_sequence=5, durable_sequence=5,
                      checkpoint_sequence=4, checkpoint_bytes=4096)
        end = dict(persistence=dict(save_status=status, final_save_bytes=8192,
                   checkpoint_json_bytes=5000, checkpoint_diagnostic_ms=1., final_flush_ms=2., restart_replay_ms=3.,
                   recovery=dict(records_loaded=5, records_replayed=1, checkpoint_sequence=4)))
        return dict(checkpoint_interval=2), samples, end

    def test_committed_exploration_and_bounded_replay(self):
        from performance_report import validate_saved_discovery
        validate_saved_discovery(*self.fixture())

    def test_unsaved_prefix_missing_checkpoint_and_oversize_fail(self):
        from performance_report import validate_saved_discovery
        for field, value in (("durable_sequence",4),("checkpoint_sequence",0),("checkpoint_bytes",64*1024*1024+1)):
            meta, samples, end = self.fixture()
            end["persistence"]["save_status"][field] = value
            with self.assertRaises(InvalidWorkload):
                validate_saved_discovery(meta,samples,end)

    def test_invalid_diagnostic_timing_fails(self):
        from performance_report import validate_saved_discovery
        meta, samples, end = self.fixture()
        end["persistence"]["checkpoint_diagnostic_ms"] = float("nan")
        with self.assertRaises(InvalidWorkload):
            validate_saved_discovery(meta,samples,end)


def stream_rows(cycles=1, preloading=False, **work):
    profile = {name: 0 for name in STREAM_BOUNDS | PRELOAD_BOUNDS}
    profile.update(simulation_transitions=1, candidate_captures=1, rollback_snapshots=1)
    rows = [{"kind": "stream", "case": "stream-r16-memory", "workload": "streaming-v1", "cycles": cycles,
             "steps_per_cycle": 200, "storage": "memory", "preloading": preloading}]
    history = 0
    labels = (["walk_east"]*100 + ["walk_west"]*100) * cycles
    for index, label in enumerate(labels):
        sample_profile = dict(profile, region_changes=int(index == 50),
                              preload_jobs=int(preloading and index == 50), **work)
        rows.append({"kind": "sample", "case": "stream-r16-memory", "actor": 1, "label": label,
                     "history_start": history, "history_end": history + 1, "rewind_count": 0,
                     "profile": sample_profile, "phases_ms": {"authoritative_total": 1.}})
        history += 1
    rows.append({"kind": "stream_end", "case": "stream-r16-memory", "history_end": history,
                 "recovery": {"records_loaded": history}})
    return rows


class StreamingValidationTests(unittest.TestCase):
    def test_group_acquisition_counts_nine_records_and_nested_wait(self):
        profile = self.acquisition_rows()[1]['profile']
        profile.update(regions_built=21, regions_prepared=11)
        acquisition = profile['region_acquisition']
        acquisition.update(groups_committed=2, prepared_groups=1, demand_groups=1,
                           group_wait=dict(secs=0, nanos=2), group_build=dict(secs=0, nanos=3))
        validate_region_acquisition(profile, required=True)
        acquisition['groups_committed'] = 3
        with self.assertRaises(InvalidWorkload):
            validate_region_acquisition(profile, required=True)
        acquisition['groups_committed'] = 2
        acquisition['group_wait']['nanos'] = 20
        with self.assertRaises(InvalidWorkload):
            validate_region_acquisition(profile, required=True)

    def acquisition_rows(self):
        rows = stream_rows()
        rows[0]['region_acquisition_version'] = 1
        zero = dict(secs=0, nanos=0)
        for row in rows:
            if row['kind'] == 'sample':
                row['profile']['regions_prepared'] = 0
                row['profile']['region_transition'] = dict(secs=0, nanos=20)
                row['profile']['region_acquisition'] = dict(
                    fallback_reads=0, fallback_builds=0, prepared_reads=0,
                    prepared_builds=0, fallback_read=zero.copy(), fallback_build=zero.copy())
        profile = rows[1]['profile']
        profile.update(region_records_read=2, regions_built=3, regions_prepared=2)
        profile['region_acquisition'].update(fallback_reads=1, prepared_reads=1,
            fallback_builds=2, prepared_builds=1, fallback_read=dict(secs=0,nanos=3),
            fallback_build=dict(secs=0,nanos=4))
        return rows

    def test_acquisition_attribution_is_complete_and_nested(self):
        validate(self.acquisition_rows(), selected_case='stream-r16-memory')

    def test_inconsistent_missing_negative_and_double_counted_acquisitions_fail(self):
        import copy
        rows = self.acquisition_rows()
        changes = [
            lambda p: p.pop('region_acquisition'),
            lambda p: p['region_acquisition'].update(fallback_reads=2),
            lambda p: p['region_acquisition'].update(prepared_builds=-1),
            lambda p: p['region_acquisition'].update(fallback_reads=True),
            lambda p: p['region_acquisition'].update(fallback_read=dict(secs=0,nanos=30)),
            lambda p: p['region_acquisition'].update(fallback_read=dict(secs=0,nanos=1000000000)),
            lambda p: p['region_acquisition']['fallback_read'].pop('secs'),
            lambda p: p.update(region_records_read=0, region_acquisition=dict(
                fallback_reads=0, prepared_reads=0, fallback_builds=3, prepared_builds=0,
                fallback_read=dict(secs=0,nanos=1), fallback_build=dict(secs=0,nanos=4)), regions_prepared=0),
        ]
        for change in changes:
            with self.subTest(change=change):
                changed = copy.deepcopy(rows)
                change(changed[1]['profile'])
                with self.assertRaises(InvalidWorkload):
                    validate(changed, selected_case='stream-r16-memory')

    def test_a_complete_walk_with_bounded_work_passes(self):
        cases, samples, ends = validate(stream_rows(), selected_case="stream-r16-memory")
        self.assertEqual(200, len(samples["stream-r16-memory"]))

    def test_unbounded_transition_work_fails(self):
        with self.assertRaises(InvalidWorkload):
            validate(stream_rows(horizon_regions_expanded=64), selected_case="stream-r16-memory")

    def test_a_preloading_walk_with_bounded_preload_work_passes(self):
        validate(stream_rows(preloading=True), selected_case="stream-r16-memory")

    def test_unbounded_or_missing_preload_work_fails(self):
        with self.assertRaises(InvalidWorkload):
            validate(stream_rows(preloading=True, preload_links_examined=64),
                     selected_case="stream-r16-memory")
        rows = stream_rows(preloading=True)
        for row in rows:
            if row["kind"] == "sample":
                row["profile"]["preload_jobs"] = 0
        with self.assertRaises(InvalidWorkload):
            validate(rows, selected_case="stream-r16-memory")

    def test_a_short_walk_or_missing_completion_fails(self):
        rows = stream_rows()
        with self.assertRaises(InvalidWorkload):
            validate(rows[:-2] + rows[-1:], selected_case="stream-r16-memory")
        with self.assertRaises(InvalidWorkload):
            validate(rows[:-1], selected_case="stream-r16-memory")


class RogueEvidenceValidation(unittest.TestCase):
    def rows(self):
        rows = [dict(kind="rogue", version=2, workload="rogue-descriptor-v2", family="dungeon",
                     domain="open", sampled_depths=list(range(1, 27)), records_per_floor=9,
                     initial_group_limit=3, materialized_group_limit=4, prepared_limit=32, samples=1)]
        rows += [dict(kind="generation", sample=0, depth=depth, records=9, elapsed=dict(secs=0,nanos=1))
                 for depth in range(1,27)]
        for mode in ("demand", "prepared", "racing"):
            for durable in (False, True):
                rows.append(dict(kind="rogue_initial", sample=0, mode=mode, durable=durable,
                                 groups=1, residency=dict(active=1, frozen=8, detached=0)))
                profile = dict(region_records_read=0, regions_built=9, regions_prepared=0,
                               region_transition=dict(secs=0,nanos=10), region_acquisition=dict(
                    fallback_reads=0, fallback_builds=0, prepared_reads=0, prepared_builds=0,
                    groups_committed=1, prepared_groups=0, demand_groups=1,
                    fallback_read=dict(secs=0,nanos=0), fallback_build=dict(secs=0,nanos=0),
                    group_wait=dict(secs=0,nanos=0), group_build=dict(secs=0,nanos=1)))
                rows.append(dict(kind="acquisition", sample=0, mode=mode, durable=durable, step=0,
                                 profile=profile, residency=dict(active=9,frozen=0,detached=9)))
                rows.append(dict(kind="rogue_end", sample=0, mode=mode, durable=durable, steps=1, groups=1,
                                 barrier=dict(secs=0,nanos=1), residency=dict(active=9,frozen=0,detached=9)))
        return rows

    def test_complete_rogue_measurements_and_optimized_rejection(self):
        from performance_report import validate_rogue
        rows = self.rows()
        validate_rogue(rows)
        for valid in (True, False):
            child = subprocess.run([sys.executable, "-O", "-c",
                "import json,sys; from performance_report import validate_rogue; validate_rogue(json.load(sys.stdin))"],
                input=json.dumps(rows if valid else rows[:-1]), text=True, capture_output=True,
                cwd=Path(__file__).parent, timeout=15)
            self.assertEqual(child.returncode == 0, valid, child.stderr)
            if not valid:
                self.assertIn("InvalidWorkload", child.stderr)

    def test_generation_durations_and_identity_counts_are_required_and_strict(self):
        from performance_report import validate_rogue
        for field, value in (("elapsed", None), ("elapsed", {}), ("elapsed", {"secs":0,"nanos":True}),
                             ("sample", False), ("depth", True), ("records", True)):
            rows = self.rows()
            rows[1][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(InvalidWorkload):
                validate_rogue(rows)

    def test_descriptor_workload_identity_and_sampled_depths_are_strict(self):
        from performance_report import validate_rogue
        for field, value in (("version", 1), ("version", True), ("workload", "rogue-v1"),
                             ("family", "other"), ("domain", "finite"),
                             ("sampled_depths", list(range(1, 26))),
                             ("sampled_depths", [True, *range(2, 27)]),
                             ("sampled_depths", list(reversed(range(1, 27))))):
            rows = self.rows()
            rows[0][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(InvalidWorkload):
                validate_rogue(rows)

    def test_each_streaming_case_must_demonstrate_actual_detachment(self):
        from performance_report import validate_rogue
        for value in (0, -1, True):
            rows = self.rows()
            acquisition = next(row for row in rows if row['kind'] == 'acquisition')
            acquisition['residency']['detached'] = value
            with self.subTest(value=value), self.assertRaises(InvalidWorkload):
                validate_rogue(rows)

    def test_group_accounting_requires_complete_initial_cases_and_conserved_work(self):
        from performance_report import validate_rogue
        for mutation in ("missing", "duplicate", "initial_limit", "final_limit", "partial_group", "conservation"):
            rows = self.rows()
            initial = next(row for row in rows if row['kind'] == 'rogue_initial')
            acquisition = next(row for row in rows if row['kind'] == 'acquisition')
            if mutation == "missing":
                rows.remove(initial)
            elif mutation == "duplicate":
                rows.append(copy.deepcopy(initial))
            elif mutation == "initial_limit":
                initial['groups'] = 4
                initial['residency']['frozen'] = 35
            elif mutation == "final_limit":
                acquisition['residency']['detached'] = 36
            elif mutation == "partial_group":
                acquisition['residency']['detached'] = 8
            else:
                initial['groups'] = 2
                initial['residency']['frozen'] = 17
            with self.subTest(mutation=mutation), self.assertRaises(InvalidWorkload):
                validate_rogue(rows)


class DemandDrivenObserverWorkTests(unittest.TestCase):
    def profile(self, cohort=8):
        from performance_report import ACTION_OBSERVER_COUNTS, EAGER_DISPLAY_COUNTS
        return dict(actors_observed=0, perception_calls=0, scene_calls=0,
                    navigation_refreshes=0, candidate_captures=1, rollback_snapshots=1,
                    **{field: cohort for field in ACTION_OBSERVER_COUNTS},
                    **{field: 0 for field in EAGER_DISPLAY_COUNTS})

    def validate(self, profile, action="move", **metadata):
        from performance_report import OBSERVER_WORK_CONTRACT
        options = dict(profile_version=3, observer_work_contract=OBSERVER_WORK_CONTRACT)
        options.update(metadata)
        validate_work(profile, {"type": action}, 8, **options)

    def test_explicit_v3_reowns_complete_observer_work_without_eager_display(self):
        self.validate(self.profile())
        self.validate(self.profile(), "set_door")
        self.validate(self.profile(0), "wait")

    def test_marker_and_version_cannot_be_missing_unknown_or_mixed(self):
        for metadata in ({"observer_work_contract": None}, {"observer_work_contract": "unknown"},
                         {"profile_version": 2}, {"profile_version": 4}, {"profile_version": True}):
            with self.subTest(metadata=metadata), self.assertRaises(InvalidWorkload):
                self.validate(self.profile(), **metadata)
        with self.assertRaises(InvalidWorkload):
            validate_work(self.profile(), {"type": "move"}, 8)

    def test_every_stage_is_required_exactly_once_and_eager_work_stays_zero(self):
        from performance_report import ACTION_OBSERVER_COUNTS, EAGER_DISPLAY_COUNTS
        for field in (*ACTION_OBSERVER_COUNTS, *EAGER_DISPLAY_COUNTS):
            missing = self.profile()
            del missing[field]
            with self.subTest(missing=field), self.assertRaises(InvalidWorkload):
                self.validate(missing)
            for value in (True, -1, 1.5, 7 if field in ACTION_OBSERVER_COUNTS else 1):
                with self.subTest(field=field, value=value), self.assertRaises(InvalidWorkload):
                    self.validate({**self.profile(), field: value})
        with self.assertRaises(InvalidWorkload):
            self.validate(self.profile(0))
        with self.assertRaises(InvalidWorkload):
            self.validate(self.profile(8), "wait")
        with self.assertRaises(InvalidWorkload):
            self.validate({**self.profile(), "actors_observed": 1})
        with self.assertRaises(InvalidWorkload):
            self.validate({**self.profile(), "scene_calls": 17})
        self.validate({**self.profile(), "scene_calls": 17}, "set_door")
