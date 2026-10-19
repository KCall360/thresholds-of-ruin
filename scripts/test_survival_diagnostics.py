"""The measurement reader must fail closed on incomplete live evidence."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from survival_diagnostics import POOLS, RESOURCES, SCALARS, read_log, validate_records


ANCHOR_KEYS = ("lineage", "branch", "history_sequence", "entry_id", "tick")


def anchor(row):
    return {key: row[key] for key in ANCHOR_KEYS}


def nutrition(reserve, elapsed=0, remainder=0):
    return {"kind": "required", "reserve": reserve, "elapsed": elapsed,
            "regeneration_remainder": remainder}


def fixture():
    common = {"survival_version": 2, "segment": "segment-1", "lineage": "lineage", "branch": "lineage",
              "history_sequence": 0, "entry_id": None, "tick": 0,
              "diagnostic_dropped": 0, "diagnostic_incomplete": False, "selected_actor": 1}
    flow = {**dict.fromkeys(SCALARS, 0), **{key: dict.fromkeys(RESOURCES, 0) for key in POOLS},
            "consumed_items": {}, "nutrition_before": nutrition(10, 0, 2),
            "nutrition_after": nutrition(9, 0, 2)}
    flow["periodic_health"] = 2
    flow["nutrition_time_demand"] = flow["nutrition_spent"] = 1
    commit = {**common, "event": "survival.commit", "history_sequence": 1, "entry_id": "entry-1",
              "start_tick": 0, "end_tick": 100, "tick": 100, "branch_changed": False, "actor": 1,
              "flows": {"incomplete": False, "actors": {"1": flow}}}
    barrier = {**common, "event": "survival.barrier", "history_sequence": 1,
               "entry_id": "entry-1", "tick": 100, "durable_sequence": 1}
    return [{**common, "event": "survival.begin"}, commit, barrier]


def restart(rows, segment="segment-2"):
    begin = {**rows[0], **anchor(rows[-1]), "event": "survival.begin", "segment": segment}
    return [begin, {**begin, "event": "survival.barrier", "durable_sequence": begin["history_sequence"]}]


def append_commit(rows, *, branch=None, end_tick=None):
    commit = copy.deepcopy(rows[1])
    previous = rows[-1]
    commit.update(anchor(previous))
    commit.update(history_sequence=previous["history_sequence"] + 1,
                  entry_id="entry-" + str(previous["history_sequence"] + 1),
                  start_tick=previous["tick"], segment=previous["segment"])
    commit["tick"] = commit["end_tick"] = previous["tick"] + 100 if end_tick is None else end_tick
    if branch is not None:
        commit["branch"] = branch
    commit["branch_changed"] = commit["branch"] != previous["branch"]
    flow = commit["flows"]["actors"]["1"]
    flow["nutrition_before"] = copy.deepcopy(flow["nutrition_after"])
    flow["nutrition_after"]["reserve"] -= 1
    barrier = {**rows[-1], **anchor(commit), "durable_sequence": commit["history_sequence"]}
    return [*rows, commit, barrier]


class SurvivalDiagnosticReports(unittest.TestCase):
    def test_projectile_refusals_are_required_bounded_and_aggregated(self):
        key = "projectile_numeric_budget_refusals"
        rows = fixture()
        rows[1]["flows"]["actors"]["1"][key] = 2
        rows = append_commit(rows)
        rows[3]["flows"]["actors"]["1"][key] = 3
        result = validate_records(rows)
        self.assertEqual(result["survival_version"], 2)
        self.assertEqual(result["actors"]["1"][key], 5)
        for value in (None, True, 1.0, -1, 2**64):
            invalid = fixture()
            invalid[1]["flows"]["actors"]["1"][key] = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                validate_records(invalid)
        missing = fixture()
        del missing[1]["flows"]["actors"]["1"][key]
        with self.assertRaises(ValueError):
            validate_records(missing)

    def test_historical_version_is_rejected_without_relabeling(self):
        rows = fixture()
        for row in rows:
            row["survival_version"] = 1
        del rows[1]["flows"]["actors"]["1"]["projectile_numeric_budget_refusals"]
        original = copy.deepcopy(rows)
        with self.assertRaises(ValueError):
            validate_records(rows)
        self.assertEqual(rows, original)

    def test_restart_aggregates_only_new_commits(self):
        rows = fixture()
        rows += restart(rows)
        result = validate_records(rows)
        self.assertEqual(result["segments"], 2)
        self.assertEqual(result["commits"], 1)
        self.assertEqual(result["actors"]["1"]["periodic_health"], 2)

    def test_loss_incomplete_gap_and_duplicate_evidence_is_rejected(self):
        variants = []
        for key, value in (("diagnostic_dropped", 1), ("diagnostic_incomplete", True),
                           ("history_sequence", 2), ("survival_version", 9)):
            rows = fixture(); rows[1][key] = value; variants.append(rows)
        rows = fixture(); rows.insert(2, copy.deepcopy(rows[1])); variants.append(rows)
        rows = fixture(); rows[1]["flows"]["incomplete"] = True; variants.append(rows)
        variants.append(fixture()[:-1])
        rows = fixture(); rows[-1]["durable_sequence"] = 0; variants.append(rows)
        for rows in variants:
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                validate_records(rows)

    def test_future_durable_sequence_cannot_certify_live_history(self):
        rows = fixture()
        rows[-1]["durable_sequence"] = 2
        with self.assertRaises(ValueError):
            validate_records(rows)

    def test_incomplete_intermediate_barrier_needs_later_matching_barrier(self):
        rows = fixture()
        rows[-1]["durable_sequence"] = 0
        with self.assertRaises(ValueError):
            validate_records(rows)
        rows.append({**rows[-1], "durable_sequence": 1})
        self.assertEqual(validate_records(rows)["commits"], 1)

    def test_commit_after_barrier_needs_another_barrier(self):
        rows = append_commit(fixture())
        with self.assertRaises(ValueError):
            validate_records(rows[:-1])
        self.assertEqual(validate_records(rows)["commits"], 2)

    def test_barrier_and_restart_must_match_the_full_live_anchor(self):
        for key, value in (("lineage", "other"), ("branch", "other"),
                           ("history_sequence", 2), ("entry_id", "other"), ("tick", 101)):
            rows = fixture()
            rows[-1][key] = value
            with self.subTest(event="barrier", key=key), self.assertRaises(ValueError):
                validate_records(rows)
            rows = fixture()
            rows += restart(rows)
            rows[-2][key] = value
            rows[-1][key] = value
            rows[-1]["durable_sequence"] = rows[-1]["history_sequence"]
            with self.subTest(event="restart", key=key), self.assertRaises(ValueError):
                validate_records(rows)

    def test_malformed_envelope_numbers_do_not_pass_python_equality(self):
        for key, values in (("survival_version", (True, 2.0)),
                            ("diagnostic_dropped", (False, 0.0)),
                            ("diagnostic_incomplete", (0, None)),
                            ("tick", (False, -1, 2**64)),
                            ("history_sequence", (False, -1, 2**64))):
            for value in values:
                rows = fixture()
                rows[0][key] = value
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    validate_records(rows)

    def test_missing_or_malformed_envelope_identities_are_rejected(self):
        for key in ("segment", "lineage", "branch"):
            for value in (None, "", [], {}, 1):
                rows = fixture()
                for row in rows:
                    row[key] = value
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    validate_records(rows)

    def test_history_zero_and_existing_history_have_exact_entry_anchors(self):
        for sequence, entry_id in ((0, "unexpected"), (1, None), (1, ""), (1, [])):
            begin = {**fixture()[0], "history_sequence": sequence, "entry_id": entry_id}
            rows = [begin, {**begin, "event": "survival.barrier", "durable_sequence": sequence}]
            with self.subTest(sequence=sequence, entry_id=entry_id), self.assertRaises(ValueError):
                validate_records(rows)
        for sequence, entry_id in ((0, None), (7, "saved-entry")):
            begin = {**fixture()[0], "history_sequence": sequence, "entry_id": entry_id}
            rows = [begin, {**begin, "event": "survival.barrier", "durable_sequence": sequence}]
            result = validate_records(rows)
            self.assertEqual(result["commits"], 0)
            self.assertEqual(result["actors"], {})

    def test_starting_saved_entry_cannot_be_republished_as_a_new_commit(self):
        rows = fixture()
        rows[0].update(history_sequence=1, entry_id="entry-1")
        rows[1]["history_sequence"] = rows[-1]["history_sequence"] = rows[-1]["durable_sequence"] = 2
        with self.assertRaises(ValueError):
            validate_records(rows)

    def test_record_objects_and_required_fields_fail_closed(self):
        for invalid in (None, [], 1, "survival.begin"):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                validate_records([invalid])
        for index, row in enumerate(fixture()):
            for key in row:
                rows = fixture()
                del rows[index][key]
                with self.subTest(event=row["event"], missing=key), self.assertRaises(ValueError):
                    validate_records(rows)

    def test_selected_and_committing_actor_ids_are_bounded(self):
        for index, key in ((0, "selected_actor"), (1, "actor")):
            for value in (False, 0, -1, 2**64, "1"):
                rows = fixture()
                rows[index][key] = value
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    validate_records(rows)

    def test_commit_start_tick_matches_previous_boundary(self):
        rows = fixture()
        rows[1]["start_tick"] = 1
        with self.assertRaises(ValueError):
            validate_records(rows)
        rows = append_commit(fixture(), end_tick=99)
        with self.assertRaises(ValueError):
            validate_records(rows)

    def test_explicit_rewind_keeps_real_gross_costs_and_can_restart(self):
        rows = append_commit(fixture(), branch="rewound-branch", end_tick=0)
        rows += restart(rows)
        result = validate_records(rows)
        self.assertEqual(result["commits"], 2)
        self.assertEqual(result["segments"], 2)
        self.assertEqual(result["actors"]["1"]["nutrition_spent"], 2)
        rows[3]["branch_changed"] = False
        with self.assertRaises(ValueError):
            validate_records(rows)

    def test_empty_missing_or_reused_segments_cannot_certify_history(self):
        for rows in ([], fixture()[1:], fixture()[:-1] + restart(fixture()),
                     fixture() + restart(fixture(), segment="segment-1")):
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                validate_records(rows)
        rows = fixture()
        rows[1]["segment"] = "unopened-segment"
        with self.assertRaises(ValueError):
            validate_records(rows)

    def test_flow_batches_and_actor_rows_have_complete_exact_shapes(self):
        for value in (None, [], {}, {"incomplete": False, "actors": {}, "future": 1}):
            rows = fixture()
            rows[1]["flows"] = value
            with self.subTest(batch=value), self.assertRaises(ValueError):
                validate_records(rows)
        for value in (None, [], {}, {**fixture()[1]["flows"]["actors"]["1"], "future": 1}):
            rows = fixture()
            rows[1]["flows"]["actors"]["1"] = value
            with self.subTest(flow=value), self.assertRaises(ValueError):
                validate_records(rows)
        for key in fixture()[1]["flows"]["actors"]["1"]:
            rows = fixture()
            del rows[1]["flows"]["actors"]["1"][key]
            with self.subTest(missing=key), self.assertRaises(ValueError):
                validate_records(rows)

    def test_actor_and_item_keys_have_canonical_nonzero_u64_identities(self):
        for identity in ("0", "01", "-1", "1.0", "١", "１", str(2**64), 1):
            rows = fixture()
            rows[1]["flows"]["actors"] = {identity: rows[1]["flows"]["actors"]["1"]}
            with self.subTest(actor=identity), self.assertRaises(ValueError):
                validate_records(rows)
            rows = fixture()
            flow = rows[1]["flows"]["actors"]["1"]
            flow["consumed_items"] = {identity: 1}
            flow["items_consumed"] = 1
            with self.subTest(item=identity), self.assertRaises(ValueError):
                validate_records(rows)

    def test_actor_batches_cannot_exceed_writer_capacity_without_loss(self):
        rows = fixture()
        flow = rows[1]["flows"]["actors"]["1"]
        rows[1]["flows"]["actors"] = {str(identity): flow for identity in range(1, 1025)}
        self.assertEqual(len(validate_records(rows)["actors"]), 1024)
        rows[1]["flows"]["actors"]["1025"] = flow
        with self.assertRaises(ValueError):
            validate_records(rows)

    def test_consumed_item_maps_cannot_exceed_writer_capacity_without_loss(self):
        rows = fixture()
        flow = rows[1]["flows"]["actors"]["1"]
        flow.update(consumed_items={str(identity): 1 for identity in range(1, 129)}, items_consumed=128)
        self.assertEqual(len(validate_records(rows)["actors"]["1"]["consumed_items"]), 128)
        flow["consumed_items"]["129"] = 1
        flow["items_consumed"] = 129
        with self.assertRaises(ValueError):
            validate_records(rows)

    def test_consumed_quantity_and_food_count_are_exact(self):
        for consumed, items, food in (({"1": 0}, 0, 0), ({"1": 1}, 2, 1),
                                      ({"1": True}, 1, 1), ({"1": 1}, 1, 2)):
            rows = fixture()
            flow = rows[1]["flows"]["actors"]["1"]
            flow.update(consumed_items=consumed, items_consumed=items, food_consumed=food)
            with self.subTest(consumed=consumed, items=items, food=food), self.assertRaises(ValueError):
                validate_records(rows)
        rows = fixture()
        flow = rows[1]["flows"]["actors"]["1"]
        flow.update(consumed_items={"4": 2, "9": 1}, items_consumed=3, food_consumed=2)
        self.assertEqual(validate_records(rows)["actors"]["1"]["consumed_items"], {"4": 2, "9": 1})

    def test_all_gross_and_resource_counters_are_bounded_integers(self):
        for key in SCALARS:
            for value in (-1, True, 1.0, 2**64, None):
                rows = fixture()
                rows[1]["flows"]["actors"]["1"][key] = value
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    validate_records(rows)
        for key in POOLS:
            for resource in RESOURCES:
                for value in (-1, False, 1.0, 2**64, None):
                    rows = fixture()
                    rows[1]["flows"]["actors"]["1"][key][resource] = value
                    with self.subTest(key=key, resource=resource, value=value), self.assertRaises(ValueError):
                        validate_records(rows)

    def test_resource_maps_require_every_known_pool_and_no_unknown_pool(self):
        for key in POOLS:
            for value in (None, [], {"stamina": 0, "focus": 0},
                          {"stamina": 0, "focus": 0, "mana": 0, "health": 0}):
                rows = fixture()
                rows[1]["flows"]["actors"]["1"][key] = value
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    validate_records(rows)

    def test_nutrition_demand_and_starvation_clipping_conserve_exact_units(self):
        rows = fixture()
        flow = rows[1]["flows"]["actors"]["1"]
        flow.update(nutrition_time_demand=3, nutrition_regeneration_demand=4,
                    nutrition_spent=2, nutrition_clipped=5,
                    nutrition_before=nutrition(1, 99, 2), nutrition_after=nutrition(-1))
        self.assertEqual(validate_records(rows)["actors"]["1"]["nutrition_clipped"], 5)
        flow["nutrition_clipped"] = 4
        with self.assertRaises(ValueError):
            validate_records(rows)

    def test_food_offer_waste_and_all_nourishment_gain_cannot_contradict(self):
        for offered, wasted, gain in ((1, 2, 0), (2, 0, 1)):
            rows = fixture()
            flow = rows[1]["flows"]["actors"]["1"]
            flow.update(nourishment_offered=offered, nourishment_wasted=wasted, nourishment_gain=gain)
            flow["nutrition_after"]["reserve"] += gain
            with self.subTest(offered=offered, wasted=wasted, gain=gain), self.assertRaises(ValueError):
                validate_records(rows)
        # Direct Nourish effects are not food offers. Debt is a separate gain.
        rows = fixture()
        flow = rows[1]["flows"]["actors"]["1"]
        flow.update(nourishment_offered=6, nourishment_wasted=2, nourishment_gain=7,
                    nourishment_debt_cleared=2, nutrition_before=nutrition(-2, 57, 2),
                    nutrition_after=nutrition(6, 57, 2))
        self.assertEqual(validate_records(rows)["actors"]["1"]["nourishment_gain"], 7)

    def test_required_nutrition_boundaries_have_exact_fractional_shapes(self):
        invalid = (False, [], {}, {"kind": "other"}, {"kind": "exempt", "reserve": 0},
                   {"kind": "required", "reserve": 1},
                   {**nutrition(1), "future": 0})
        for key in ("nutrition_before", "nutrition_after"):
            for value in invalid:
                rows = fixture()
                rows[1]["flows"]["actors"]["1"][key] = value
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    validate_records(rows)
            for field, values in (("reserve", (-2**31 - 1, 2**31, True, 1.0)),
                                  ("elapsed", (-1, 2**64, True, 1.0)),
                                  ("regeneration_remainder", (-1, 2**32, True, 1.0))):
                for value in values:
                    rows = fixture()
                    rows[1]["flows"]["actors"]["1"][key][field] = value
                    with self.subTest(key=key, field=field, value=value), self.assertRaises(ValueError):
                        validate_records(rows)

    def test_fractional_only_recovery_is_not_rounded_into_whole_nutrition(self):
        rows = fixture()
        flow = rows[1]["flows"]["actors"]["1"]
        flow.update(nutrition_time_demand=0, nutrition_spent=0,
                    nutrition_before=nutrition(10, 99, 1), nutrition_after=nutrition(10, 99, 2))
        flow["periodic_resources"]["mana"] = 1
        result = validate_records(rows)
        self.assertEqual(result["actors"]["1"]["nutrition_spent"], 0)
        self.assertEqual(result["actors"]["1"]["periodic_resources"]["mana"], 1)
        flow["nutrition_after"]["reserve"] = 9
        with self.assertRaises(ValueError):
            validate_records(rows)

    def test_noncombat_and_exempt_actor_boundaries_are_valid_without_nutrition_flow(self):
        for boundary in (None, {"kind": "exempt"}):
            rows = fixture()
            for row in rows:
                row["selected_actor"] = None
            rows[1]["flows"]["actors"]["1"].update(
                nutrition_time_demand=0, nutrition_spent=0,
                nutrition_before=boundary, nutrition_after=boundary)
            with self.subTest(boundary=boundary):
                self.assertEqual(validate_records(rows)["actors"]["1"]["nutrition_spent"], 0)

    def test_missing_required_boundaries_cannot_hide_nutrition_accounting(self):
        for boundary in (None, {"kind": "exempt"}):
            for values in ({"nutrition_spent": 1, "nutrition_time_demand": 1},
                           {"nutrition_regeneration_demand": 1, "nutrition_clipped": 1},
                           {"nourishment_gain": 1}, {"nourishment_debt_cleared": 1}):
                rows = fixture()
                flow = rows[1]["flows"]["actors"]["1"]
                flow.update(nutrition_time_demand=0, nutrition_spent=0,
                            nutrition_before=boundary, nutrition_after=boundary)
                flow.update(values)
                with self.subTest(boundary=boundary, values=values), self.assertRaises(ValueError):
                    validate_records(rows)
        rows = fixture()
        flow = rows[1]["flows"]["actors"]["1"]
        flow.update(nutrition_time_demand=0, nutrition_spent=0, nourishment_offered=3,
                    nourishment_wasted=3, nutrition_before={"kind": "exempt"},
                    nutrition_after={"kind": "exempt"})
        self.assertEqual(validate_records(rows)["actors"]["1"]["nourishment_wasted"], 3)

    def test_exact_aggregation_does_not_wrap_at_counter_width(self):
        rows = append_commit(fixture())
        for row in (rows[1], rows[3]):
            row["flows"]["actors"]["1"]["direct_health"] = 2**64 - 1
        self.assertEqual(validate_records(rows)["actors"]["1"]["direct_health"], 2 * (2**64 - 1))

    def test_stale_valid_prefix_is_rejected_against_independent_final_anchor(self):
        rows = fixture()
        expected = anchor(rows[-1])
        self.assertEqual(validate_records(rows, expected_final_anchor=expected)["commits"], 1)
        for key, value in (("lineage", "other"), ("branch", "other"),
                           ("history_sequence", 2), ("entry_id", "entry-2"), ("tick", 101)):
            with self.subTest(key=key), self.assertRaises(ValueError):
                validate_records(rows, expected_final_anchor={**expected, key: value})
        for invalid in ({}, [], {**expected, "history_sequence": True},
                        {**expected, "future": 1}):
            with self.subTest(anchor=invalid), self.assertRaises(ValueError):
                validate_records(rows, expected_final_anchor=invalid)

    def test_file_reader_rejects_missing_partial_empty_or_broken_writer_output(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "survival.jsonl"
            with self.assertRaises(OSError):
                read_log(path)
            good = "".join(json.dumps(row) + "\n" for row in fixture())
            for text in ("", "\n", good.rstrip("\n"), good + '{"event":',
                         good + "\n", good + "null\n", good + '{"event":"server_diagnostics_dropped"}\n'):
                path.write_text(text, encoding="utf-8")
                with self.subTest(text=text[-80:]), self.assertRaises(ValueError):
                    read_log(path)
            path.write_text(good, encoding="utf-8")
            self.assertEqual(read_log(path, expected_final_anchor=anchor(fixture()[-1]))["commits"], 1)

    def test_duplicate_json_keys_and_non_json_numbers_are_rejected(self):
        good = "".join(json.dumps(row) + "\n" for row in fixture())
        variants = (good.replace('"diagnostic_dropped": 0', '"diagnostic_dropped": 1, "diagnostic_dropped": 0', 1),
                    good.replace('"periodic_health": 2', '"periodic_health": 200, "periodic_health": 2'),
                    good.replace('"diagnostic_dropped": 0', '"diagnostic_dropped": 0, "unused": NaN', 1),
                    good.replace('"diagnostic_dropped": 0', '"diagnostic_dropped": 0, "unused": Infinity', 1))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "survival.jsonl"
            for text in variants:
                path.write_text(text, encoding="utf-8")
                with self.subTest(text=text[:100]), self.assertRaises(ValueError):
                    read_log(path)

    def test_divergent_restart_and_fabricated_gross_counters_are_rejected(self):
        rows = fixture()
        rows.append({**rows[0], "segment": "other"})
        with self.assertRaises(ValueError):
            validate_records(rows)
        for value in (-1, True, 2**64):
            rows = fixture(); rows[1]["flows"]["actors"]["1"]["active_ticks"] = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                validate_records(rows)


if __name__ == "__main__":
    unittest.main()
