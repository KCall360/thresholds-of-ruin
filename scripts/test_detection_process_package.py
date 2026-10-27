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
    def test_live_text_spectator_attaches_after_the_paid_phase_zero_boundary(self):
        # Source-owned lifecycle contract, not a fabricated native prompt proof.
        # Real Adventure emits prompts for asynchronous paid narration; attaching
        # before the dose leaves those prompts queued ahead of a later command.
        import ast
        import inspect
        import textwrap
        from test_magic_items_process import MagicItemProcesses
        method = MagicItemProcesses.test_imported_detection_monsters_adventure_hallucination_channel_and_refresh
        tree = ast.parse(textwrap.dedent(inspect.getsource(method)))
        calls = [node for node in ast.walk(tree) if isinstance(node, ast.Call)]
        attach, = [node.lineno for node in calls if isinstance(node.func, ast.Attribute)
                   and node.func.attr == "adventure"]
        phase_zero, = [node.lineno for node in calls if isinstance(node.func, ast.Attribute)
                       and node.func.attr == "assert_monster_sample"
                       and isinstance(node.args[-1], ast.Constant) and node.args[-1].value == 0]
        commands = [node.lineno for node in calls if isinstance(node.func, ast.Attribute)
                    and node.func.attr == "say"]
        self.assertGreater(attach, phase_zero, "attach after paid events, not with queued earlier prompts")
        self.assertLess(attach, min(commands))

    def test_same_selected_chart_after_snapshot_consumes_new_epoch_auto_request_without_competing_write(self):
        from types import SimpleNamespace
        from test_magic_items_process import MagicItemProcesses
        requested, pages = public_pages(513, "food")
        requested["context"]["epoch"] = "8"
        for page in pages:
            page["context"]["epoch"] = "8"
        frame = {"input_context": {"stream": requested["context"]}, "branch": requested["branch"],
                 "observation_base": requested["base"], "state": {"observation": {
                     "charts": [{"kind": "food", "chart": requested["chart"]}]}}}
        subject = MagicItemProcesses("test_imported_detection_food_complete_multipage_adventure_and_pure_reads")
        subject.snapshot = lambda client: copy.deepcopy(frame)
        client = SimpleNamespace(transcript=[])
        relay = object.__new__(detection.ChartCaptureRelay)
        relay.chart_lock, relay.errors = threading.Lock(), []
        relay.chart_requests, relay.chart_replies = [], []
        # Old epoch7 has a lawful full transfer with identical base/token. It
        # cannot fulfill a read at snapshot epoch8; no prefix/row normalization.
        for epoch in ("7", "8"):
            cursor = None
            for index, page in enumerate(pages):
                query = {**copy.deepcopy(requested), "cursor": cursor}
                query["context"]["epoch"] = epoch
                reply_page = copy.deepcopy(page)
                reply_page["context"]["epoch"] = epoch
                identity = "managed-" + epoch + "-" + str(index)
                relay.chart_requests.append({"type": "request", "request_id": identity,
                    "request": {"type": "chart", "query": query}})
                relay.chart_replies.append({"type": "chart_page", "request_id": identity, "page": reply_page})
                cursor = page["next"]
        subject.detection_relays = {id(client): relay}
        subject.detection_selected = {id(client): "food"}
        subject.detection_boundaries = {id(client): {**requested["context"], "epoch": "7"}}
        subject.request = lambda *args: self.fail("competing explicit Chart would collide with managed active.pending")
        observed, query, actual = subject.detection_pages(client, "food")
        self.assertEqual((observed, query, actual), (frame, requested, pages))
        self.assertEqual(subject.detection_last_request, "managed-8-0")
        # A repeated caller that did not acquire a fresh snapshot epoch must not
        # recycle a completed transfer just because the chart kind still matches.
        with self.assertRaisesRegex(AssertionError, "fresh attachment epoch"):
            subject.detection_pages(client, "food")

    def test_managed_request_can_be_locally_pending_before_capture_and_waits_for_complete_pages(self):
        from types import SimpleNamespace
        from test_magic_items_process import MagicItemProcesses
        requested, pages = public_pages(513, "food")
        requested["context"]["epoch"] = "8"
        for page in pages:
            page["context"]["epoch"] = "8"
        frame = {"input_context": {"stream": requested["context"]}, "branch": requested["branch"],
                 "observation_base": requested["base"], "state": {"observation": {
                     "charts": [{"kind": "food", "chart": requested["chart"]}]}}}
        subject = MagicItemProcesses("test_imported_detection_food_complete_multipage_adventure_and_pure_reads")
        subject.snapshot = lambda client: copy.deepcopy(frame)
        client = SimpleNamespace(transcript=[])
        relay = object.__new__(detection.ChartCaptureRelay)
        relay.chart_lock, relay.errors = threading.Lock(), []
        relay.chart_requests, relay.chart_replies = [], []
        subject.detection_relays = {id(client): relay}
        subject.detection_selected = {id(client): "food"}
        subject.detection_boundaries = {id(client): {**requested["context"], "epoch": "7"}}
        subject.request = lambda *args: self.fail("unsent managed request is not permission to send another")
        delivered = []
        def receive(client, predicate):
            index = len(delivered)
            self.assertLess(index, len(pages), "must stop once the exact transfer completes")
            page = copy.deepcopy(pages[index])
            query = {**copy.deepcopy(requested), "cursor": None if index == 0 else pages[index - 1]["next"]}
            identity = "managed-" + str(index)
            relay.chart_requests.append({"type": "request", "request_id": identity,
                                          "request": {"type": "chart", "query": query}})
            reply = {"type": "chart_page", "request_id": identity, "page": page}
            relay.chart_replies.append(reply)
            value = {"type": "update", "message": reply}
            self.assertTrue(predicate(value))
            delivered.append(value)
            return value
        subject.frame = receive
        observed, query, actual = subject.detection_pages(client, "food")
        self.assertEqual((observed, query, actual), (frame, requested, pages))
        self.assertEqual(len(delivered), 2)
        self.assertEqual(subject.detection_last_request, "managed-0")

    def test_managed_initial_lookup_requires_exact_epoch_and_unique_request_identity(self):
        requested, _ = public_pages(1, "monsters")
        requested["context"]["epoch"] = "8"
        old = copy.deepcopy(requested)
        old["context"]["epoch"] = "7"
        relay = object.__new__(detection.ChartCaptureRelay)
        relay.chart_lock, relay.errors = threading.Lock(), []
        relay.chart_requests = [{"type": "request", "request_id": "old",
                                  "request": {"type": "chart", "query": old}}]
        self.assertIsNone(relay.initial_request(requested))
        actual = {"type": "request", "request_id": "new",
                  "request": {"type": "chart", "query": requested}}
        relay.chart_requests.append(copy.deepcopy(actual))
        self.assertEqual(relay.initial_request(requested), actual)
        relay.chart_requests.append({**copy.deepcopy(actual), "request_id": "competing"})
        with self.assertRaisesRegex(AssertionError, "multiple initial"):
            relay.initial_request(requested)

    def test_headless_explicit_reply_is_not_concatenated_with_an_automatic_refresh(self):
        from types import SimpleNamespace
        from test_magic_items_process import MagicItemProcesses
        requested, pages = public_pages(1, "monsters")
        frame = {"input_context": {"stream": requested["context"]}, "branch": requested["branch"],
                 "observation_base": requested["base"], "state": {"observation": {
                     "charts": [{"kind": "monsters", "chart": requested["chart"]}]}}}
        subject = MagicItemProcesses("test_imported_detection_monsters_adventure_hallucination_channel_and_refresh")
        subject.snapshot = lambda client: copy.deepcopy(frame)
        client = SimpleNamespace(transcript=[])
        relay = object.__new__(detection.ChartCaptureRelay)
        relay.chart_lock, relay.errors = threading.Lock(), []
        relay.chart_requests, relay.chart_replies = [], []
        subject.detection_relays = {id(client): relay}
        def request(client, query):
            self.assertEqual(query, {"type": "chart", "query": requested})
            for kind, identity in (("update", "automatic-refresh"), ("response", "explicit-query")):
                reply = {"type": "chart_page", "request_id": identity, "page": copy.deepcopy(pages[0])}
                relay.chart_requests.append({"type": "request", "request_id": identity,
                                             "request": {"type": "chart", "query": copy.deepcopy(requested)}})
                relay.chart_replies.append(reply)
                client.transcript.append(__import__("json").dumps({"type": kind, "message": reply}))
            return {"error": None}
        subject.request = request
        observed, query, actual = subject.detection_pages(client, "monsters")
        self.assertEqual((observed, query, actual), (frame, requested, pages))

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

    def test_ascii_capture_keeps_one_complete_request_bound_transfer_not_repeated_hydrations(self):
        from test_ascii_process import DetectionChartAsciiProcesses
        query, pages = public_pages(1024, "mapping")
        relay = object.__new__(detection.ChartCaptureRelay)
        relay.chart_lock, relay.errors = threading.Lock(), []
        relay.chart_requests, relay.chart_replies = [], []
        for prefix in ("prior-hydration", "selected-hydration"):
            cursor = None
            for index, page in enumerate(pages):
                identity = prefix + str(index)
                relay.chart_requests.append({"type": "request", "request_id": identity,
                    "request": {"type": "chart", "query": {**copy.deepcopy(query), "cursor": cursor}}})
                relay.chart_replies.append({"type": "chart_page", "request_id": identity, "page": copy.deepcopy(page)})
                cursor = page["next"]
        subject = DetectionChartAsciiProcesses("test_source_free_food_and_mapping_chart_modal_pages_capture_and_never_submit_actions")
        actual = subject.chart_wire_pages(relay, "mapping")
        self.assertEqual(actual, detection.rows(pages))
        self.assertEqual(len(actual), 1024)

    def test_explicit_response_requires_exact_base_and_single_reply_identity(self):
        query, pages = public_pages(1, "monsters")
        reply = {"type": "chart_page", "request_id": "explicit", "page": pages[0]}
        earlier = copy.deepcopy(reply)
        earlier["request_id"] = "automatic"
        earlier["page"]["base"]["revision"] = "3"
        frames = [{"type": "update", "message": earlier}, {"type": "response", "message": reply}]
        self.assertEqual(detection.explicit_chart_reply(frames, query), reply)
        with self.assertRaises(AssertionError):
            detection.explicit_chart_reply([frames[0]], query)
        with self.assertRaises(AssertionError):
            detection.explicit_chart_reply([*frames, copy.deepcopy(frames[1])], query)
        wrong = copy.deepcopy(frames[1])
        wrong["message"]["page"]["base"]["revision"] = "5"
        with self.assertRaises(AssertionError):
            detection.explicit_chart_reply([wrong], query)

    def test_relay_isolates_one_complete_transfer_and_rejects_corrupt_continuations(self):
        query, pages = public_pages(1024, "mapping")
        def relay_fixture():
            relay = object.__new__(detection.ChartCaptureRelay)
            relay.chart_lock, relay.errors = threading.Lock(), []
            relay.chart_requests, relay.chart_replies = [], []
            for prefix in ("auto", "explicit", "later"):
                cursor = None
                for index, page in enumerate(pages):
                    current = {**copy.deepcopy(query), "cursor": cursor}
                    identity = prefix + str(index)
                    relay.chart_requests.append({"type": "request", "request_id": identity,
                        "request": {"type": "chart", "query": current}})
                    relay.chart_replies.append({"type": "chart_page", "request_id": identity, "page": copy.deepcopy(page)})
                    cursor = page["next"]
            return relay
        relay = relay_fixture()
        exchange = relay.stream(query, "explicit0")
        self.assertEqual([row["request_id"] for row in exchange["queries"]], ["explicit0", "explicit1", "explicit2"])
        self.assertEqual([row["page"] for row in exchange["pages"]], pages)
        self.assertEqual(len(detection.rows([row["page"] for row in exchange["pages"]])), 1024)
        # Synthetic repeated-coordinate rows are preserved, not deduplicated.
        self.assertEqual(len({repr(row) for row in detection.rows(pages)}), 1)
        mutations = (
            lambda r: r.chart_replies.pop(4),
            lambda r: r.chart_replies.append(copy.deepcopy(r.chart_replies[4])),
            lambda r: r.chart_requests[4]["request"]["query"].update(cursor="511"),
            lambda r: r.chart_requests[4]["request"]["query"]["base"].update(revision="5"),
            lambda r: r.chart_replies[4]["page"].update(start="511"),
            lambda r: r.chart_replies[4]["page"]["base"].update(revision="5"),
            lambda r: r.chart_replies[4]["page"].update(next=None),
            lambda r: r.chart_requests[4].update(request_id="explicit0"),
            lambda r: r.chart_replies[4].update(request_id="explicit0"),
            lambda r: r.chart_replies.__setitem__(slice(3, 5), list(reversed(r.chart_replies[3:5]))),
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                changed = relay_fixture()
                mutation(changed)
                with self.assertRaises(AssertionError):
                    changed.stream(query, "explicit0")
        pending = relay_fixture()
        pending.chart_requests = pending.chart_requests[:5]
        pending.chart_replies = pending.chart_replies[:4]
        partial = pending.stream(query, "explicit0", complete=False)
        self.assertEqual(len(partial["pages"]), 1)
        with self.assertRaises(AssertionError):
            pending.stream(query, "explicit0")

    def test_relay_selection_preserves_actual_envelopes_and_correlations(self):
        query, pages = public_pages(1)
        request = {"type": "request", "request_id": "actual-id", "request": {"type": "chart", "query": query}}
        reply = {"type": "chart_page", "request_id": "actual-id", "context": query["context"], "page": pages[0]}
        relay = object.__new__(detection.ChartCaptureRelay)  # no socket/thread constructor
        relay.chart_lock = threading.Lock()
        relay.errors = []
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

    def test_all_fixture_dimensions_obey_real_source_bounds(self):
        for paging, mapping in ((False, False), (True, False), (False, True), (True, True)):
            with self.subTest(paging=paging, mapping=mapping):
                fixture = detection.fixture(paging=paging, mapping=mapping, production=self.source())
                for region in fixture["regions"]:
                    self.assertTrue(1 <= region["size"][0] <= 32, region["size"])
                    self.assertTrue(1 <= region["size"][1] <= 32, region["size"])
                    self.assertTrue(1 <= region["size"][2] <= 8, region["size"])

    def test_real_production_effects_conditions_species_and_opt_in_stock_are_not_rewritten(self):
        root = self.source()
        with patch("subprocess.run", side_effect=AssertionError("pure fixture must not compile")):
            fixture = detection.fixture(production=root)
        source_items = tomllib.loads((root / "catalog/items.toml").read_text())["items"]
        source_species = tomllib.loads((root / "catalog/creatures.toml").read_text())["creatures"]["species"]
        self.assertEqual({key: fixture["manifest"]["archetypes"][key] for key in source_items}, source_items)
        self.assertEqual({key: fixture["manifest"]["creatures"]["species"][key] for key in source_species}, source_species)
        source_bodies = tomllib.loads((root / "catalog/bodies.toml").read_text())["creatures"]["body_packages"]
        actual_bodies = fixture["manifest"]["creatures"]["body_packages"]
        self.assertEqual({key: actual_bodies[key] for key in source_bodies}, source_bodies)
        owner = fixture["manifest"]["characters"][0]
        body = actual_bodies[owner["creature"]["body_package"]]
        self.assertEqual(body, {**source_bodies["rogue.body.human"], "species": "fixture.detection"})
        self.assertEqual(owner["creature"]["hit_dice"],
                         tomllib.loads((root / "scenario.toml").read_text())["characters"][0]["creature"]["hit_dice"])
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
        self.assertTrue(all(abs(row["at"][0] - 2) + abs(row["at"][1] - 2) + 1 > 16 and row["at"][2] == 0 for row in food))
        self.assertTrue(all(0 <= value < region["size"][axis] for row in food
                            for axis, value in enumerate(row["at"])))
        self.assertEqual(fixture["manifest"]["spatial_scopes"]["study"]["rooms"]["1"][0]["max"], [31, 31, 7])

    def test_mapping_has_lawful_optional_certificate_and_remote_geometry(self):
        present = detection.fixture(mapping=True, production=self.source())
        absent = detection.fixture(mapping=True, certified=False, production=self.source())
        self.assertEqual(present["regions"], absent["regions"])
        self.assertEqual(present["regions"][0]["size"], [32, 32, 8])
        paging = detection.fixture(paging=True, mapping=True, production=self.source())
        self.assertEqual(paging["regions"][0]["size"], [32, 32, 8])
        self.assertEqual(2 * (32 + 32) * 8, 1024)
        self.assertEqual(present["manifest"]["spatial_scopes"]["study"].pop("mapping"),
                         {"room_frames": [{"room": 1, "region": 1, "up": "up"}]})
        self.assertEqual(present, absent)
        self.assertEqual([row["quantity"] for row in present["regions"][0]["items"]
                          if row["archetype"] == "rogue.magic_mapping"], [2])


if __name__ == "__main__":
    unittest.main()
