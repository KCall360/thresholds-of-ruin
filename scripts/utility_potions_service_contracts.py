"""Ordinary encoded Service composition for the production poison/sight potions.

Generic expiry/freeze owners are conditions::tests::extend_duration_preserves_one_
application_and_expires_without_protection and test_status_condition_process.
Perception's illuminated-eye and guise boundaries remain in perception::contract_tests.
"""
import json
import os
import unittest

from service_contract_host import ServiceHost
from utility_potions_package import (
    assert_extra_healing, assert_poison, assert_sight, condition, deadline, fixture, initial_items, inventory_item, observation,
)


class UtilityServiceHarness(unittest.TestCase):
    make_fixture = staticmethod(fixture)
    actors = (1, 3)

    @classmethod
    def setUpClass(cls):
        executable = os.environ.get("TOR_SERVICE_CONTRACT_HOST")
        digest = os.environ.get("TOR_SERVICE_CONTRACT_SHA256")
        if not executable or not digest:
            raise RuntimeError("A qualified Service host and SHA-256 are required; no auto-build")
        cls.host = ServiceHost(executable, expected_sha256=digest)
        cls.addClassCleanup(cls.host.close)

    def setUp(self):
        self.case = self.host.call("create", **self.make_fixture())["case"]
        self.addCleanup(lambda: self.host.call("close", case=self.case))
        self.peers = {}
        self.serial = 0
        for actor in self.actors:
            peer = self.host.call("connect", case=self.case, role="player",
                                  user=f"owner-{actor}", actors=[actor])["connection"]
            self.peers[actor] = peer
            self.drain(actor)
            self.request(actor, {"type": "attach", "actor": str(actor)})
            self.request(actor, {"type": "acquire_control"})

    def drain(self, actor):
        result = self.host.call("drain", case=self.case, connection=self.peers[actor])
        self.assertTrue(result["live"])
        self.assertIsNone(result["closing"])
        rows = [json.loads(raw) for raw in result["frames"]]
        self.assertFalse([row for row in rows if row["type"] == "error"], rows)
        return rows

    def request(self, actor, request):
        self.serial += 1
        self.host.request(case=self.case, connection=self.peers[actor],
                          request_id=f"potion-{self.serial}", request=request)
        return self.drain(actor)

    def snapshot(self, actor=1):
        rows = self.request(actor, {"type": "snapshot"})
        snapshots = [row["snapshot"] for row in rows if row["type"] == "snapshot"]
        self.assertEqual(len(snapshots), 1)
        return snapshots[0]

    @staticmethod
    def command(frame, action):
        return {"type": "command", "context": {"stream": frame["context"],
            "readiness_revision": frame["readiness"]["revision"]}, "branch": frame["branch"],
            "command": {"type": "act", "expected_revision": frame["state"]["revision"], "action": action}}

    def act(self, actor, action):
        current = self.snapshot(actor)
        rows = self.request(actor, self.command(current, action))
        receipts = [row["receipt"] for row in rows if row["type"] == "ack"]
        self.assertEqual(len(receipts), 1)
        self.assertEqual(receipts[0]["type"], "admitted")
        intention = receipts[0]["intention"]
        for _ in range(64):
            statuses = [row["update"]["body"]["status"] for row in rows
                        if row["type"] == "update" and row["update"]["body"]["type"] == "intention"
                        and row["update"]["body"]["status"]["intention"] == intention
                        and row["update"]["body"]["status"]["actor"] == str(actor)]
            self.assertFalse([row for row in statuses if row["phase"] in ("failed", "cancelled")], statuses)
            if any(row["phase"] == "resolved" for row in statuses):
                return self.snapshot(actor)
            for other in self.actors:
                if other == actor:
                    continue
                peer = self.snapshot(other)
                if peer["readiness"]["admission"] and observation(peer)["ready"]:
                    self.request(other, self.command(peer, {"type": "wait"}))
            result = self.host.call("step", case=self.case)
            rows = self.drain(actor)
            for other in self.actors:
                if other != actor:
                    self.drain(other)
            self.assertEqual(result, {"status": "progress"})
        self.fail("Ordinary utility-item work did not resolve within 64 scheduler boundaries")


class UtilityPotionsServiceContracts(UtilityServiceHarness):
    def drink(self, identity):
        return self.act(1, {"type": "drink", "item": identity})

    def test_poison_is_restorable_and_sight_cures_reveals_extends_without_identification(self):
        initial = self.snapshot()
        ids = initial_items(self, initial)
        poisoned = self.drink(ids["poison"])
        assert_poison(self, poisoned, ids["poison"])
        restored = self.drink(ids["restore"])
        self.assertEqual(observation(restored)["combat"]["own_stats"]["restorable_losses"]["attributes"]["strength"], 0)
        blind = self.drink(ids["blind"])
        self.assertIsNotNone(condition(blind, "rogue.blindness"))
        self.assertFalse(any(row["name"] == "phantom" for row in observation(blind)["visible_actors"]))
        first = self.drink(ids["sight"])
        assert_sight(self, first, ids["sight"])
        self.assertEqual(inventory_item(first, ids["sight"])["quantity"], "2")
        self.assertGreaterEqual(int(condition(first, "rogue.see_invisible")["remaining_ticks"]), 80800)
        self.assertLessEqual(int(condition(first, "rogue.see_invisible")["remaining_ticks"]), 89200)
        second = self.drink(ids["sight"])
        assert_sight(self, second, ids["sight"])
        self.assertEqual(inventory_item(second, ids["sight"])["quantity"], "1")
        self.assertGreaterEqual(deadline(second) - deadline(first), 80800)
        self.assertLessEqual(deadline(second) - deadline(first), 89200)
        self.assertFalse(second["state"]["wizard_game"])

    def test_extra_healing_at_owned_hd_cap_cures_blindness_and_identifies(self):
        ids = initial_items(self, self.snapshot())
        blind = self.drink(ids["blind"])
        healed = self.drink(ids["extra"])
        assert_extra_healing(self, blind, healed, ids["extra"])
        self.assertFalse(healed["state"]["wizard_game"])
