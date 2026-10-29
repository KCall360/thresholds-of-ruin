"""Raw production Service witnesses for exact held-link selection and privacy."""
import copy
import json
import os
import unittest

from linked_hold_package import LINK, NAMES, fixture
from service_contract_host import ServiceHost


def objects(value):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from objects(child)
    elif isinstance(value, list):
        for child in value:
            yield from objects(child)


class LinkedHoldServiceContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        executable = os.environ.get("TOR_SERVICE_CONTRACT_HOST")
        digest = os.environ.get("TOR_SERVICE_CONTRACT_SHA256")
        if not executable or not digest:
            raise RuntimeError("A qualified Service host and SHA-256 are required; no auto-build")
        cls.host = ServiceHost(executable, expected_sha256=digest)
        cls.addClassCleanup(cls.host.close)

    def setUp(self):
        self.case = None
        self.serial = 0
        self.peers = {}
        self.subject_frames = []
        self.addCleanup(self.close_case)

    def close_case(self):
        if self.case is not None:
            self.host.call("close", case=self.case)
            self.case = None

    def drain(self, connection, *, errors=False):
        result = self.host.call("drain", case=self.case, connection=connection)
        self.assertTrue(result["live"])
        self.assertIsNone(result["closing"])
        rows = [json.loads(raw) for raw in result["frames"]]
        if connection == self.peers.get(1):
            self.subject_frames.extend(rows)
        if not errors:
            self.assertFalse([row for row in rows if row["type"] == "error"], rows)
        return rows

    def request(self, actor, request, *, identity=None, errors=False):
        self.serial += 1
        identity = identity or f"request-{self.serial}"
        connection = self.peers[actor]
        self.host.request(case=self.case, connection=connection, request_id=identity, request=request)
        return self.drain(connection, errors=errors)

    def snapshot(self, actor):
        rows = self.request(actor, {"type": "snapshot"})
        snapshots = [row["snapshot"] for row in rows if row["type"] == "snapshot"]
        self.assertEqual(len(snapshots), 1)
        return snapshots[0]

    @staticmethod
    def command(snapshot, action):
        return {"type": "command", "context": {"stream": snapshot["context"],
            "readiness_revision": snapshot["readiness"]["revision"]}, "branch": snapshot["branch"],
            "command": {"type": "act", "action": action}}

    def audit(self, actor=1):
        return self.host.call("audit", case=self.case, actor=actor, user=f"player-{actor}")

    def step_until(self, predicate, *, actor=1):
        for _ in range(40):
            snapshots = {actor: self.snapshot(actor) for actor in self.peers}
            if predicate(snapshots[actor]):
                return snapshots[actor]
            for owner, current in snapshots.items():
                if current["readiness"]["admission"] and current["state"]["observation"]["ready"]:
                    self.request(owner, self.command(current, {"type": "wait"}))
            result = self.host.call("step", case=self.case)
            for peer in self.peers.values():
                self.drain(peer)
            self.assertEqual(result, {"status": "progress"}, "authored peers must advance the pending work")
        self.fail("bounded linked-hold fixture did not reach its required behavior")

    def establish(self, *, hidden=True, success=True):
        self.case = self.host.call("create", **fixture(hidden=hidden, success=success))["case"]
        for actor in (1, 2, 3):
            connection = self.host.call("connect", case=self.case, role="player", user=f"player-{actor}", actors=[actor])["connection"]
            self.peers[actor] = connection
            self.drain(connection)
            self.request(actor, {"type": "attach", "actor": str(actor)})
            self.request(actor, {"type": "acquire_control"})
        if hidden:
            initial = self.snapshot(1)["state"]["observation"]
            visible_names = {actor["name"] for actor in initial["visible_actors"]}
            self.assertTrue(visible_names.isdisjoint(NAMES[1:]),
                "the privacy fixture must hide both holders under ordinary sight before contact")
        for actor in (2, 3):
            current = self.snapshot(actor)
            target = next(row["id"] for row in current["state"]["observation"]["visible_actors"] if row["name"] == NAMES[0])
            self.request(actor, self.command(current, {"type": "use_ability", "id": LINK,
                "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}}))
        current = self.step_until(lambda value: len(value["state"]["observation"]["linked_holds"]) == 2
            and value["readiness"]["admission"] and value["state"]["observation"]["ready"])
        offers = current["state"]["observation"]["linked_holds"]
        self.assertNotEqual(offers[0]["target"], offers[1]["target"])
        self.assertTrue(all(row["target"].startswith("h_") for row in offers))
        self.assertEqual([row["holder"] is None for row in offers], [hidden, hidden])
        return current

    def assert_private(self):
        for row in self.subject_frames:
            encoded = json.dumps(row)
            for name in NAMES[1:]:
                self.assertNotIn(name, encoded, "pending/completed/history frames must not reveal hidden holder names")
            for value in objects(row):
                if value.get("type") in ("escape", "escape_started", "escape_resolved"):
                    self.assertLessEqual(set(value), {"type", "hold", "success"})
                if "linked_holds" in value:
                    self.assertTrue(all(offer["holder"] is None for offer in value["linked_holds"]))

    def assert_escape_lifecycle(self, selected, success):
        self.request(1, {"type": "history", "before": None, "limit": 100})
        values = list(objects(self.subject_frames))
        pending = [value for value in values if isinstance(value.get("action"), dict)
            and value["action"].get("type") == "escape" and "remaining" in value]
        self.assertTrue(pending, "the actual pending preparation must reach the observer")
        for value in pending:
            self.assertEqual(value["action"], {"type": "escape", "hold": selected})
            self.assertLessEqual(set(value), {"action", "remaining", "active"})
        self.assertIn({"type": "escape_started", "hold": selected}, values)
        self.assertIn({"type": "escape_resolved", "hold": selected, "success": success}, values)
        self.assert_private()

    def test_hidden_selected_hold_success_retry_and_context_rejections_are_exact(self):
        self.assert_hidden_escape_retry_and_context(move=False)

    def test_hidden_move_selects_oldest_hold_without_moving_or_retargeting_retry(self):
        self.assert_hidden_escape_retry_and_context(move=True)

    def assert_hidden_escape_retry_and_context(self, *, move):
        current = self.establish()
        offers = current["state"]["observation"]["linked_holds"]
        selected, other = (offers[0]["target"], offers[1]["target"]) if move else (
            offers[1]["target"], offers[0]["target"])
        baseline = self.audit()
        action = {"type": "move", "direction": "west"} if move else {
            "type": "escape", "hold": selected}
        canonical = self.command(current, action)
        for field in ("revision", "readiness", "branch", "stream"):
            invalid = copy.deepcopy(canonical)
            if field == "revision":
                invalid["command"]["expected_revision"] = str(int(current["state"]["revision"]) + 1)
            elif field == "readiness":
                invalid["context"]["readiness_revision"] = str(int(current["readiness"]["revision"]) + 1)
            elif field == "stream":
                invalid["context"]["stream"]["epoch"] = str(int(current["context"]["epoch"]) + 1)
            else:
                invalid["branch"] = "00000000-0000-4000-8000-000000000001"
            rows = self.request(1, invalid, errors=True)
            self.assertEqual(len([row for row in rows if row["type"] == "error"]), 1, field)
            self.assertEqual(self.audit(), baseline, field)
        unknown = self.command(current, {"type": "escape", "hold": "h_" + "ff" * 32})
        unavailable = self.request(1, unknown, errors=True)
        peer = self.snapshot(2)
        foreign = self.request(2, self.command(peer, {"type": "escape", "hold": selected}), errors=True)
        error = lambda rows: [(row["code"], row["message"]) for row in rows if row["type"] == "error"]
        self.assertEqual(error(unavailable), error(foreign))
        self.assertEqual(self.audit(), baseline)
        accepted = self.request(1, canonical, identity="exact-escape")
        receipt = next(row["receipt"] for row in accepted if row["type"] == "ack")
        self.assertEqual(receipt["type"], "admitted")
        after = self.step_until(lambda value: [row["target"] for row in value["state"]["observation"]["linked_holds"]] == [other])
        self.assertNotIn(selected, json.dumps(after["state"]["observation"]["linked_holds"]))
        completed = self.audit()
        retried = self.request(1, canonical, identity="exact-escape")
        replay = next(row["receipt"] for row in retried if row["type"] == "ack")
        for field in ("actor", "branch", "entry_id", "intention"):
            self.assertEqual(replay[field], receipt[field], field)
        self.assertEqual(self.audit(), completed, "retry must not escape the remaining holder")
        self.assert_escape_lifecycle(selected, True)
        if move:
            self.assertFalse(any(value.get("type") == "moved"
                for value in objects(self.subject_frames)),
                "a Move resolved as Escape must not emit a movement event")

    def test_hidden_failed_escape_keeps_both_links_and_generic_completion(self):
        current = self.establish(success=False)
        offers = current["state"]["observation"]["linked_holds"]
        selected = offers[0]["target"]
        self.subject_frames.clear()
        self.request(1, self.command(current, {"type": "escape", "hold": selected}))
        def resolved(value):
            return any(event.get("type") == "escape_resolved" and event["hold"] == selected
                for event in value["state"]["observation"]["combat"]["events"])
        after = self.step_until(resolved)
        self.assertEqual(after["state"]["observation"]["linked_holds"], offers)
        results = [event for event in after["state"]["observation"]["combat"]["events"] if event.get("type") == "escape_resolved"]
        self.assertEqual(results, [{"type": "escape_resolved", "hold": selected, "success": False}])
        self.assert_escape_lifecycle(selected, False)

    def test_same_hold_offer_tracks_only_current_ordinary_holder_visibility(self):
        current = self.establish()
        targets = [offer["target"] for offer in current["state"]["observation"]["linked_holds"]]
        self.assert_private()
        ready = lambda value: value["readiness"]["admission"] and value["state"]["observation"]["ready"]
        mover = self.step_until(ready, actor=2)
        self.request(2, self.command(mover, {"type": "move", "direction": "south"}))
        visible = self.step_until(lambda value: any(offer["holder"] is not None
            for offer in value["state"]["observation"]["linked_holds"]))
        offers = visible["state"]["observation"]["linked_holds"]
        self.assertEqual([offer["target"] for offer in offers], targets)
        disclosed = [offer for offer in offers if offer["holder"] is not None]
        self.assertEqual(len(disclosed), 1)
        self.assertTrue(any(actor["id"] == disclosed[0]["holder"] and actor["name"] == NAMES[1]
            for actor in visible["state"]["observation"]["visible_actors"]))
        mover = self.step_until(ready, actor=2)
        self.request(2, self.command(mover, {"type": "move", "direction": "north"}))
        hidden = self.step_until(lambda value: all(offer["holder"] is None
            for offer in value["state"]["observation"]["linked_holds"]))
        self.assertEqual([offer["target"] for offer in hidden["state"]["observation"]["linked_holds"]], targets)
        self.subject_frames.clear()
        self.snapshot(1)
        self.assert_private()
