import unittest
from performance_report import PRELOAD_BOUNDS, SPEC, STREAM_BOUNDS, expected_actions, validate, validate_work


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
        with self.assertRaises(AssertionError):
            validate_work(profile, {"type":"wait"}, 8)

    def test_movement_contract_rejects_repeated_projection(self):
        profile = dict(actors_observed=16, perception_calls=16, scene_calls=16,
                       candidate_captures=1, rollback_snapshots=1)
        validate_work(profile, {"type":"move"}, 8)
        profile["scene_calls"] = 17
        with self.assertRaises(AssertionError):
            validate_work(profile, {"type":"move"}, 8)


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
                                 history_end=index+1, client_memory=index+1))
            rows.append(dict(kind="traversal_end", case=case, history_end=index+1, client_memory=index+1))
        return rows

    def test_complete_discovery_is_validated_independently_of_mixed_cases(self):
        validate(self.rows(), quick=True, discovery_only=True)

    def test_missing_completion_is_not_a_successful_benchmark(self):
        rows = self.rows()
        rows.pop()
        with self.assertRaises(AssertionError):
            validate(rows, quick=True, discovery_only=True)

    def test_reordered_actions_and_duplicate_metadata_fail(self):
        rows = self.rows()
        rows[1], rows[2] = rows[2], rows[1]
        with self.assertRaises(AssertionError):
            validate(rows, quick=True, discovery_only=True)
        rows = self.rows()
        rows.append(rows[0])
        with self.assertRaises(AssertionError):
            validate(rows, quick=True, discovery_only=True)

class CompleteWireValidationTests(unittest.TestCase):
    def rows(self):
        rows = DiscoveryValidationTests().rows()
        for row in rows:
            if row["kind"] == "traversal":
                row["wire_profile_version"] = 2
            elif row["kind"] == "sample":
                row["observation_wire"] = dict(version=2, full_bytes=1000, sent_bytes=400, delta=True)
                row["phases_ms"] = dict(wire_encoding=0.2, wire_decoding=0.3)
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
            lambda r: r[-1].update(sent_bytes_total=1),
            lambda r: r.pop(),
            lambda r: r[1]["observation_wire"].update(sent_bytes=1001),
            lambda r: r[1]["observation_wire"].update(delta=1),
            lambda r: r[1]["phases_ms"].update(wire_decoding=float("nan")),
            lambda r: r[1]["phases_ms"].pop("wire_encoding"),
            lambda r: r[0].pop("wire_profile_version"),
            lambda r: r[-1].update(wire_profile_version=1),
            lambda r: r[1]["phases_ms"].update(delta_encoding=0.1),
            lambda r: r[-1].update(sent_p95_bytes=401),
            lambda r: r[1].pop("observation_wire"),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(index=index):
                rows = self.rows()
                mutate(rows)
                with self.assertRaises(AssertionError):
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
            with self.assertRaises(AssertionError):
                validate_saved_discovery(meta,samples,end)

    def test_invalid_diagnostic_timing_fails(self):
        from performance_report import validate_saved_discovery
        meta, samples, end = self.fixture()
        end["persistence"]["checkpoint_diagnostic_ms"] = float("nan")
        with self.assertRaises(AssertionError):
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
                     "profile": sample_profile})
        history += 1
    rows.append({"kind": "stream_end", "case": "stream-r16-memory", "history_end": history,
                 "recovery": {"records_loaded": history}})
    return rows


class StreamingValidationTests(unittest.TestCase):
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
                with self.assertRaises(AssertionError):
                    validate(changed, selected_case='stream-r16-memory')

    def test_a_complete_walk_with_bounded_work_passes(self):
        cases, samples, ends = validate(stream_rows(), selected_case="stream-r16-memory")
        self.assertEqual(200, len(samples["stream-r16-memory"]))

    def test_unbounded_transition_work_fails(self):
        with self.assertRaises(AssertionError):
            validate(stream_rows(horizon_regions_expanded=64), selected_case="stream-r16-memory")

    def test_a_preloading_walk_with_bounded_preload_work_passes(self):
        validate(stream_rows(preloading=True), selected_case="stream-r16-memory")

    def test_unbounded_or_missing_preload_work_fails(self):
        with self.assertRaises(AssertionError):
            validate(stream_rows(preloading=True, preload_links_examined=64),
                     selected_case="stream-r16-memory")
        rows = stream_rows(preloading=True)
        for row in rows:
            if row["kind"] == "sample":
                row["profile"]["preload_jobs"] = 0
        with self.assertRaises(AssertionError):
            validate(rows, selected_case="stream-r16-memory")

    def test_a_short_walk_or_missing_completion_fails(self):
        rows = stream_rows()
        with self.assertRaises(AssertionError):
            validate(rows[:-2] + rows[-1:], selected_case="stream-r16-memory")
        with self.assertRaises(AssertionError):
            validate(rows[:-1], selected_case="stream-r16-memory")
