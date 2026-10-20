"""Encoded production Service discovery/disclosure authority (not transport)."""
import json
import os
import unittest

from discovery_package import doors, fixture, observation, observer_fixture, search, traps
from service_contract_host import ServiceHost
from utility_potions_service_contracts import UtilityServiceHarness


class DiscoveryServiceContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        executable = os.environ.get("TOR_SERVICE_CONTRACT_HOST")
        digest = os.environ.get("TOR_SERVICE_CONTRACT_SHA256")
        if not executable or not digest:
            raise RuntimeError("A qualified Service host and SHA-256 are required; no auto-build")
        cls.host = ServiceHost(executable, expected_sha256=digest)
        cls.addClassCleanup(cls.host.close)

    def start(self, dc):
        self.case = self.host.call("create", **fixture(dc=dc))["case"]
        self.addCleanup(lambda: self.host.call("close", case=self.case))
        self.connection = self.host.call("connect", case=self.case, role="player", user="discovery", actors=[1])["connection"]
        self.serial = 0
        self.drain()
        self.request({"type": "attach", "actor": "1"})
        self.request({"type": "acquire_control"})
        return self.snapshot()

    def drain(self):
        result = self.host.call("drain", case=self.case, connection=self.connection)
        self.assertTrue(result["live"])
        self.assertIsNone(result["closing"])
        rows = [json.loads(row) for row in result["frames"]]
        self.assertFalse([row for row in rows if row["type"] == "error"], rows)
        return rows

    def request(self, request):
        self.serial += 1
        self.host.request(case=self.case, connection=self.connection,
                          request_id="discovery-" + str(self.serial), request=request)
        return self.drain()

    def snapshot(self):
        rows = self.request({"type": "snapshot"})
        return next(row["snapshot"] for row in rows if row["type"] == "snapshot")

    def act(self, action):
        current = self.snapshot()
        request = {"type": "command", "context": {"stream": current["context"],
            "readiness_revision": current["readiness"]["revision"]}, "branch": current["branch"],
            "command": {"type": "act", "expected_revision": current["state"]["revision"], "action": action}}
        rows = self.request(request)
        receipts = [row["receipt"] for row in rows if row["type"] == "ack"]
        self.assertEqual(len(receipts), 1)
        self.assertEqual(receipts[0]["type"], "admitted")
        identity = receipts[0]["intention"]
        for _ in range(16):
            statuses = [row["update"]["body"]["status"] for row in rows
                        if row["type"] == "update" and row["update"]["body"]["type"] == "intention"
                        and row["update"]["body"]["status"]["intention"] == identity]
            self.assertFalse([row for row in statuses if row["phase"] in ("failed", "cancelled")], statuses)
            if any(row["phase"] == "resolved" for row in statuses):
                return self.snapshot()
            self.assertEqual(self.host.call("step", case=self.case), {"status": "progress"})
            rows = self.drain()
        self.fail("Discovery action exceeded the bounded fixture scheduler work")

    def test_known_presence_has_no_family_until_explicit_examination_and_open_is_shared_geometry(self):
        initial = self.start(0)
        first = next(iter(traps(initial).values()))
        self.assertIsNone(first["family"])
        self.assertTrue(first["reachable"])
        self.assertEqual(len(traps(initial)), 1, "Opaque entrance must conceal the rear trap")
        examined = self.act({"type": "examine_trap", "trap": first["id"]})
        self.assertEqual(traps(examined)[first["id"]]["family"], "rust")
        self.assertEqual(observation(examined)["tick"], observation(initial)["tick"])
        adjacent = self.act({"type": "move", "direction": "east"})
        entrance = next(iter(doors(adjacent).values()))
        opened = self.act({"type": "set_door", "door": entrance["id"], "open": True})
        self.assertTrue(doors(opened)[entrance["id"]]["open"])
        self.assertEqual(len(traps(opened)), 2)
        self.assertEqual(sum(row["family"] is None for row in traps(opened).values()), 1)

    def test_failed_passive_queries_and_repeatable_search_disclose_no_hidden_counts(self):
        initial = self.start(100)
        self.assertFalse(traps(initial))
        self.assertFalse(doors(initial))
        for _ in range(3):
            self.assertEqual(self.snapshot()["state"], initial["state"])
        for _ in range(2):
            before = self.snapshot()
            after = self.act(search(before))
            self.assertEqual(int(observation(after)["tick"]) - int(observation(before)["tick"]), 100)
            self.assertFalse(traps(after))
            self.assertFalse(doors(after))
            text = json.dumps(after)
            self.assertNotIn("trap_examined", text)
            self.assertNotIn("sleeping_gas", text)


class DiscoveryObserverServiceContracts(UtilityServiceHarness):
    make_fixture = staticmethod(observer_fixture)
    actors = (1, 2)

    def deny_unseen(self, actor, action):
        before = self.snapshot(actor)
        self.serial += 1
        self.host.request(case=self.case, connection=self.peers[actor],
                          request_id="discovery-denied-" + str(self.serial),
                          request=self.command(before, action))
        result = self.host.call("drain", case=self.case, connection=self.peers[actor])
        self.assertTrue(result["live"])
        self.assertIsNone(result["closing"])
        rows = [json.loads(raw) for raw in result["frames"]]
        errors = [row for row in rows if row["type"] == "error"]
        self.assertEqual(len(errors), 1)
        self.assertEqual(errors[0]["code"], "invalid_action")
        self.assertFalse(any(row["type"] == "ack" for row in rows))
        self.assertEqual(self.snapshot(actor)["state"], before["state"])
        return errors[0]["message"]

    def test_actor_private_discovery_and_family_survive_other_observer_shared_open_and_close(self):
        owner, peer = self.snapshot(1), self.snapshot(2)
        entrance = next(iter(doors(owner).values()))
        hazard = next(iter(traps(owner).values()))
        self.assertFalse(entrance["open"])
        self.assertIsNone(hazard["family"])
        self.assertFalse(doors(peer))
        self.assertFalse(traps(peer))
        concealed = next(cell for cell in observation(peer)["visible_cells"]
                         if cell["position"] == {"x": 2, "y": 1, "z": 0})
        self.assertTrue(concealed["wall"])
        self.assertIsNone(concealed["door"])
        self.assertEqual(self.deny_unseen(2, {"type": "set_door", "door": entrance["id"], "open": True}),
                         self.deny_unseen(2, {"type": "set_door", "door": "d_" + "00" * 32, "open": True}))
        self.deny_unseen(2, {"type": "examine_trap", "trap": hazard["id"]})
        examined = self.act(1, {"type": "examine_trap", "trap": hazard["id"]})
        self.assertEqual(traps(examined)[hazard["id"]]["family"], "rust")
        self.assertEqual(observation(examined)["tick"], observation(owner)["tick"])
        self.assertFalse(traps(self.snapshot(2)), "Another actor's examination grants no trap facts")
        self.act(1, {"type": "move", "direction": "east"})
        activated = self.snapshot(2)
        peer_hazard = next(iter(traps(activated).values()))
        self.assertEqual(peer_hazard["family"], "rust", "A witnessed activation is a new perceptible fact")
        self.assertNotEqual(peer_hazard["id"], hazard["id"])
        self.assertFalse(doors(activated), "Trap activation does not reveal a concealed entrance")
        self.act(1, {"type": "set_door", "door": entrance["id"], "open": True})
        opened = self.snapshot(2)
        seen = next(iter(doors(opened).values()))
        self.assertTrue(seen["open"], "Opening changes shared geometry, not private physics")
        self.assertNotEqual(seen["id"], entrance["id"], "Targets remain actor scoped")
        self.assertEqual(set(traps(opened)), {peer_hazard["id"]},
                         "The newly exposed rear trap still requires this observer's own discovery")
        self.act(1, {"type": "set_door", "door": entrance["id"], "open": False})
        closed = self.snapshot(2)
        self.assertFalse(doors(closed)[seen["id"]]["open"],
                         "Seeing an open secret entrance remains knowledge when it closes")
        self.assertEqual(set(traps(closed)), {peer_hazard["id"]})
        for _ in range(2):
            self.assertEqual(self.snapshot(2)["state"], closed["state"])
        searched = self.act(2, search(closed))
        self.assertEqual(set(traps(searched)), {peer_hazard["id"]})
        self.assertFalse(doors(searched)[seen["id"]]["open"])
        self.assertFalse(searched["state"]["wizard_game"])
