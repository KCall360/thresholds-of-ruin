"""Pure fixture/page/relay contracts; no applications or sockets are launched."""
import copy
import os
from pathlib import Path
import threading
import tomllib
import unittest
from unittest.mock import patch

import detection_process_package as detection


def public_pages(count=513, kind="food"):
    query = {"context": {"stream": "actual-stream", "epoch": "1"}, "branch": "actual-branch",
             "base": {"cursor": {"sequence": "7", "tick": "100"}, "revision": "4"},
             "chart": "chart-token", "cursor": None}
    mark = {"kind": kind}
    if kind == "monsters":
        mark = {"kind": "monster", "name": "snake", "asset": "creature.serpent.snake"}
    if kind == "mapping":
        mark = {"kind": "mapping", "fact": {"kind": "wall_face", "normal": [-1, 0, 0]}}
    records = [{"record": "component", "component": "0", "alignment": "aligned"}] + [
        {"record": "row", "component": "0", "position": ["2", "0", "0"], "mark": copy.deepcopy(mark)}
        for _ in range(count)]
    pages = []
    for start in range(0, len(records), 512):
        end = min(start + 512, len(records))
        pages.append({**{key: copy.deepcopy(query[key]) for key in ("context", "branch", "base", "chart")},
            "kind": kind, "mode": "live" if kind == "monsters" else "historical", "acquired_at": "100",
            "total_components": "1", "total_records": str(len(records)), "start": str(start),
            "records": records[start:end], "next": str(end) if end < len(records) else None})
    return query, pages


class DetectionProcessHelperContracts(unittest.TestCase):
    def test_query_uses_observation_base_not_later_readiness_cursor(self):
        requested, _ = public_pages(1)
        frame = {"input_context": {"stream": requested["context"]}, "branch": requested["branch"],
                 "cursor": {"sequence": "99", "tick": "100"}, "observation_base": requested["base"],
                 "state": {"observation": {"charts": [{"kind": "food", "chart": "chart-token"}]}}}
        self.assertEqual(detection.query(frame, "food"), requested)
        frame["observation_base"] = None
        with self.assertRaises(AssertionError):
            detection.query(frame, "food")

    def test_complete_pages_preserve_equal_projected_occurrences_without_deduplication(self):
        query, pages = public_pages()
        records = detection.validate_pages(pages, query)
        self.assertEqual(len(pages), 2)
        self.assertEqual(len(records), 514)
        self.assertEqual(len(detection.rows(pages)), 513)
        self.assertEqual(len({repr(row) for row in detection.rows(pages)}), 1)
        self.assertEqual(detection.historical_payload(pages)["records"], records)

    def test_incomplete_extra_reordered_and_changed_boundary_pages_reject(self):
        for mutation in (lambda p: p.pop(), lambda p: p.append(copy.deepcopy(p[-1])),
                         lambda p: p.reverse(), lambda p: p[1].update(start="511"),
                         lambda p: p[1]["base"].update(revision="5"),
                         lambda p: p[1].update(chart="different"),
                         lambda p: p[0].update(next=None),
                         lambda p: p[0].update(total_records="0514")):
            with self.subTest(mutation=mutation):
                query, pages = public_pages()
                mutation(pages)
                with self.assertRaises(AssertionError):
                    detection.validate_pages(pages, query)

    def test_private_or_actionable_fields_and_missing_declarations_reject(self):
        for mutation in (lambda p: p[0].update(world="private"),
                         lambda p: p[0]["records"][1].update(target="a_secret"),
                         lambda p: p[0]["records"][1]["mark"].update(item_id="i_secret"),
                         lambda p: p[0]["records"][1].update(component="1"),
                         lambda p: p[0]["records"][0].update(record="row"),
                         lambda p: p[0]["records"][1].update(position=[0, 0, 0])):
            query, pages = public_pages(1)
            mutation(pages)
            with self.assertRaises(AssertionError):
                detection.validate_pages(pages, query)

    def test_mapping_strict_cardinal_fact_and_unavailable_orientation(self):
        query, pages = public_pages(1, "mapping")
        detection.validate_pages(pages, query)
        for normal in ([1, 1, 0], [0, 0, 0], [True, 0, 0], None):
            changed = copy.deepcopy(pages)
            changed[0]["records"][1]["mark"]["fact"]["normal"] = normal
            with self.assertRaises(AssertionError):
                detection.validate_pages(changed, query)
        pages[0]["records"][0]["alignment"] = "inconsistent"
        pages[0]["records"][1]["position"] = None
        pages[0]["records"][1]["mark"]["fact"]["normal"] = None
        detection.validate_pages(pages, query)

    def test_initial_doses_bind_food_to_real_scroll_id_not_an_equal_quantity_potion(self):
        inventory = [{"id": "actual-" + key, "class": "scroll" if key == "food" else "potion",
                      "quantity": str(quantity), "identified": False}
                     for key, (_, _, quantity) in detection.DOSES.items()]
        inventory.append({"id": "other-potion", "class": "potion", "quantity": "5", "identified": False})
        frame = {"state": {"observation": {"inventory": inventory}}}
        ids = detection.initial_doses(self, frame)
        self.assertEqual(ids, {key: "actual-" + key for key in detection.DOSES})

    def test_canonical_wire_integer_boundaries(self):
        self.assertEqual(detection.wire_integer(str((1 << 64) - 1)), (1 << 64) - 1)
        self.assertEqual(detection.wire_integer(str(-(1 << 63)), signed=True), -(1 << 63))
        for value in (1, True, "01", "-0", "1 ", "-1", str(1 << 64)):
            with self.assertRaises(AssertionError):
                detection.wire_integer(value)

    def test_multicell_visual_actor_rows_all_check_distinct_channel_literals(self):
        query, pages = public_pages(1, "monsters")
        for phase in (0, 1):
            name, asset = detection.VISUAL[phase]
            peers = [{"id": "same-real-peer", "name": name, "asset": asset,
                      "description": "", "appearance_seed": "p_" + "00" * 32} for _ in range(2)]
            frame = {"state": {"observation": {"self_target": "owner", "visible_actors": peers}}}
            name, asset = detection.MONSTER_SENSE[phase]
            pages[0]["records"][1]["mark"].update(name=name, asset=asset)
            detection.assert_monster_sample(self, frame, pages, phase)
            peers[1]["name"] = "true personal sibling"
            with self.assertRaises(AssertionError):
                detection.assert_monster_sample(self, frame, pages, phase)

    def test_relay_selection_preserves_actual_envelopes_and_correlations(self):
        query, pages = public_pages(1)
        request = {"type": "request", "request_id": "actual-id", "request": {"type": "chart", "query": query}}
        reply = {"type": "chart_page", "request_id": "actual-id", "context": query["context"], "page": pages[0]}
        relay = object.__new__(detection.ChartCaptureRelay)  # no socket/thread constructor
        relay.chart_lock = threading.Lock()
        relay.chart_requests = [copy.deepcopy(request)]
        relay.chart_replies = []
        relay.observe_server(reply)
        self.assertEqual(relay.pair(query), (request, reply))
        self.assertEqual(relay.stream(query), {"queries": [request], "pages": [reply]})
        relay.chart_replies[0]["request_id"] = "wrong-id"
        with self.assertRaises(AssertionError):
            relay.pair(query)


class DetectionAuthoredFixtureContracts(unittest.TestCase):
    def source(self):
        return Path(os.environ.get("TOR_DETECTION_TEST_SOURCE", detection.PRODUCTION))

    def test_real_production_effects_conditions_species_and_opt_in_stock_are_not_rewritten(self):
        root = self.source()
        with patch("subprocess.run", side_effect=AssertionError("pure fixture must not compile")):
            fixture = detection.fixture(production=root)
        source_items = tomllib.loads((root / "catalog/items.toml").read_text())["items"]
        source_species = tomllib.loads((root / "catalog/creatures.toml").read_text())["creatures"]["species"]
        self.assertEqual({key: fixture["manifest"]["archetypes"][key] for key in source_items}, source_items)
        self.assertEqual({key: fixture["manifest"]["creatures"]["species"][key] for key in source_species}, source_species)
        self.assertEqual(fixture["rules"], (root / "rules.toml").read_text())
        self.assertEqual(source_items["rogue.food_detection"]["class"], "scroll")
        self.assertIn("readable", source_items["rogue.food_detection"])
        self.assertNotIn("consumable", source_items["rogue.food_detection"])
        for _, kind, quantity in detection.DOSES.values():
            self.assertIn("rogue." + kind, source_items)
            self.assertIn(quantity, [row["quantity"] for row in fixture["regions"][0]["items"] if "carried_by" in row])

    def test_paging_uses_513_distinct_far_true_floor_cells_not_item_multiplicity(self):
        fixture = detection.fixture(paging=True, production=self.source())
        region = fixture["regions"][0]
        food = [row for row in region["items"] if row["archetype"] == "rogue.ration"]
        self.assertEqual(len(food), 513)
        self.assertEqual(len({tuple(row["at"]) for row in food}), 513)
        self.assertEqual(len({row["id"] for row in food}), 513)
        self.assertTrue(all(row["at"][0] >= 70 and row["at"][2] == 0 for row in food))
        self.assertTrue(all(0 <= value < region["size"][axis] for row in food
                            for axis, value in enumerate(row["at"])))
        self.assertEqual(fixture["manifest"]["spatial_scopes"]["study"]["rooms"]["1"][0]["max"], [129, 8, 1])

    def test_mapping_has_lawful_optional_certificate_and_remote_geometry(self):
        present = detection.fixture(mapping=True, production=self.source())
        absent = detection.fixture(mapping=True, certified=False, production=self.source())
        self.assertEqual(present["regions"], absent["regions"])
        self.assertEqual(present["regions"][0]["size"], [90, 5, 2])
        paging = detection.fixture(paging=True, mapping=True, production=self.source())
        self.assertEqual(paging["regions"][0]["size"], [130, 9, 2])
        self.assertEqual(2 * (90 + 5) * 2, 380)
        self.assertEqual(2 * (130 + 9) * 2, 556)
        self.assertEqual(present["manifest"]["spatial_scopes"]["study"].pop("mapping"),
                         {"room_frames": [{"room": 1, "region": 1, "up": "up"}]})
        self.assertEqual(present, absent)
        self.assertEqual([row["quantity"] for row in present["regions"][0]["items"]
                          if row["archetype"] == "rogue.magic_mapping"], [2])


if __name__ == "__main__":
    unittest.main()
