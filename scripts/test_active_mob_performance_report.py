import copy
import unittest

import active_mob_performance_report as report
import perf_compare


def complete_row():
    actor = dict(executions=2, moves=1, combat_starts=1, waits=0,
                 portal_crossings=0, first_tick=0, last_tick=100)
    region = {key: value * 5 if key not in ("first_tick", "last_tick") else value
              for key, value in actor.items()}
    return dict(workload="active-mobs-v1", version=1, mode="direct", seed=42, regions=20, ai=100,
                steps=8, all_active=True, all_alive=True, activity_complete=True, replay_equal=True,
                activity=dict(actors={str(i): dict(actor) for i in range(2, 102)},
                              regions={str(i): dict(region) for i in range(1, 21)},
                              player_moves=8, player_crossings=1),
                command_ms=[1.] * 208, ai_command_ms=[1.] * 200, player_command_ms=[1.] * 8,
                player_cycle_ms=[26.] * 8, projection_ms=[.1] * 8, serialization_ms=[.1] * 8,
                work=dict(route_searches=20, strike_resolutions=100), phase_totals_ms=dict(simulation=20.),
                package_ms=1., cold_activation_ms=1., workload_ms=208., save_ms=1., resume_ms=1.,
                save_status=dict(pending_bytes=0, error=None, accepted_sequence=417, durable_sequence=417),
                saved_bytes=4096, retained_history=417, state_sha256="a" * 64)


class ActiveMobReportTests(unittest.TestCase):
    def test_complete_report_and_comparison_extraction(self):
        row = complete_row()
        self.assertIs(report.validate(row), row)
        unit = perf_compare.parse_cases(["active-mobs"], 5)[0]
        timings, counts, version = perf_compare.extract(unit, [row])
        self.assertEqual(version, 1)
        self.assertEqual(len(timings["r20-ai100-s8"]["ai_command_ms"]), 200)
        self.assertEqual(counts["r20-ai100-s8"]["player_crossings"], 1)
        self.assertEqual(report.distribution(list(range(100)))["p99_ms"], 98)

    def test_enabled_preloader_diagnostic_cannot_qualify_as_baseline(self):
        row = complete_row()
        row["diagnostic_preloading"] = False
        self.assertIs(report.validate(row), row)
        for value in (True, 0, "false"):
            with self.subTest(value=value):
                row["diagnostic_preloading"] = value
                with self.assertRaisesRegex(ValueError, "enabled-preloader diagnostic"):
                    report.validate(row)

    def test_frozen_passive_or_partial_population_cannot_qualify(self):
        mutations = [lambda r: r.update(all_active=False),
                     lambda r: r["activity"]["actors"].pop("101"),
                     lambda r: r["activity"]["regions"].pop("20"),
                     lambda r: r["activity"]["regions"]["20"].update(combat_starts=0),
                     lambda r: r["activity"]["actors"]["101"].update(last_tick=0),
                     lambda r: r["work"].update(route_searches=0),
                     lambda r: r["work"].update(strike_resolutions=0),
                     lambda r: r["activity"].update(player_crossings=0),
                     lambda r: r.update(steps=1, activity_complete=False),
                     lambda r: r.update(replay_equal=False)]
        for mutate in mutations:
            with self.subTest(mutation=mutate):
                row = copy.deepcopy(complete_row())
                mutate(row)
                with self.assertRaises(ValueError):
                    report.validate(row)

    def test_wait_only_actor_cannot_hide_behind_active_region_totals(self):
        row = complete_row()
        actors = row["activity"]["actors"]
        # Independent positive fixture: two owners each have two productive and
        # two wait executions. Reconcile its samples before testing a mutation.
        for identity in ("100", "101"):
            actors[identity].update(executions=4, waits=2)
        region = row["activity"]["regions"]["20"]
        region.update(executions=14, moves=5, combat_starts=5, waits=4)
        row["ai_command_ms"].extend([1.] * 4)
        row["command_ms"].extend([1.] * 4)
        self.assertIs(report.validate(row), row)
        expected_region = copy.deepcopy(region)
        # Transfer the victim's productive work to its peer. Region and whole-
        # run totals, elapsed ticks and every timing count remain unchanged.
        actors["101"].update(moves=0, combat_starts=0, waits=4)
        actors["100"].update(moves=2, combat_starts=2, waits=0)
        for field in ("executions", "moves", "combat_starts", "waits", "portal_crossings"):
            self.assertEqual(region[field], sum(actors[str(i)][field] for i in range(97, 102)))
        self.assertEqual(region, expected_region)
        with self.assertRaisesRegex(ValueError, "AI 101 only waited"):
            report.validate(row)

    def test_every_crossing_matches_corridor_geometry(self):
        # Start x=1 in width-eight regions: the first portal is step seven,
        # then one every eight steps. Expected values are independently listed.
        for moves, crossings in ((7, 1), (8, 1), (14, 1), (15, 2), (16, 2), (159, 20), (160, 20)):
            with self.subTest(moves=moves):
                activity = complete_row()["activity"]
                activity.update(player_moves=moves, player_crossings=crossings)
                report.validate_activity(activity, moves)
                activity["player_crossings"] += 1
                with self.assertRaisesRegex(ValueError, "exact corridor route"):
                    report.validate_activity(activity, moves)

    def test_incomplete_timings_or_storage_are_not_success(self):
        for field, value in (("command_ms", [1.]), ("projection_ms", [float("nan")] * 8),
                             ("ai", True), ("seed", 43), ("saved_bytes", 0)):
            row = complete_row()
            row[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                report.validate(row)
        row = complete_row()
        row["save_status"]["durable_sequence"] -= 1
        with self.assertRaises(ValueError):
            report.validate(row)
        with self.assertRaises(ValueError):
            report.extract([complete_row(), complete_row()])




class DemandDrivenActiveMobReportTests(unittest.TestCase):
    def row(self):
        from performance_report import ACTION_OBSERVER_COUNTS, EAGER_DISPLAY_COUNTS, OBSERVER_WORK_CONTRACT
        row = complete_row()
        row.update(profile_version=3, observer_work_contract=OBSERVER_WORK_CONTRACT)
        row["work"].update(actors_observed=0,
                           **{field: 20800 for field in ACTION_OBSERVER_COUNTS},
                           **{field: 0 for field in EAGER_DISPLAY_COUNTS})
        return row

    def test_current_and_historical_contracts_are_explicit_and_both_valid(self):
        report.validate(complete_row())
        report.validate(self.row())

    def test_partial_or_mixed_ownership_never_qualifies_as_legacy(self):
        from performance_report import ACTION_OBSERVER_COUNTS, EAGER_DISPLAY_COUNTS
        for field in ("profile_version", "observer_work_contract"):
            row = self.row()
            del row[field]
            with self.subTest(missing=field), self.assertRaises(ValueError):
                report.validate(row)
        for field in (*ACTION_OBSERVER_COUNTS, *EAGER_DISPLAY_COUNTS):
            row = self.row()
            del row["work"][field]
            with self.subTest(missing=field), self.assertRaises(ValueError):
                report.validate(row)
            row = self.row()
            row["work"][field] += 1
            with self.subTest(mismatch=field), self.assertRaises(ValueError):
                report.validate(row)
        row = self.row()
        row["work"].update({field: 0 for field in ACTION_OBSERVER_COUNTS})
        with self.assertRaises(ValueError):
            report.validate(row)
        # New accounting never weakens the independent workload/replay contract.
        for field in ("all_active", "all_alive", "activity_complete", "replay_equal"):
            row = self.row()
            row[field] = False
            with self.subTest(missing=field), self.assertRaises(ValueError):
                report.validate(row)


if __name__ == "__main__":
    unittest.main()
