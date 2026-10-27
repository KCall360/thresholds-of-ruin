"""Independent Python assertions over the production Service encoder.

These are Service witnesses, not transport/native-client or durable-store tests.
"""
import json

from hallucination_package import assert_sample, fixture, initial_items
from utility_potions_package import condition, deadline, inventory_item, observation
from utility_potions_service_contracts import UtilityServiceHarness

CONDITION = "rogue.hallucinating"


class HallucinationServiceContracts(UtilityServiceHarness):
    make_fixture = staticmethod(fixture)
    actors = (1, 2)

    def drink(self, identity):
        return self.act(1, {"type": "drink", "item": identity})

    def test_exact_visual_sample_preserves_authority_and_queries_do_not_resample(self):
        initial = self.snapshot()
        ids = initial_items(self, initial)
        self.assertTrue(any(row["name"] == "actual visible peer" for row in observation(initial)["visible_actors"]))
        self.assertEqual(observation(initial)["ground_items"][0]["item"]["name"], "gold coin")
        self.assertIsNone(observation(initial)["ground_items"][0]["presentation"])
        first = self.drink(ids["hallucination"])
        self.assertEqual(observation(first)["tick"], "100")
        assert_sample(self, observation(first), 0)
        self.assertEqual(inventory_item(first, ids["hallucination"])["name"], "potion of hallucination")
        self.assertEqual(inventory_item(first, ids["hallucination"])["quantity"], "5")
        self.assertTrue(inventory_item(first, ids["hallucination"])["identified"])
        self.assertGreaterEqual(int(condition(first, CONDITION)["remaining_ticks"]), 80800)
        self.assertLessEqual(int(condition(first, CONDITION)["remaining_ticks"]), 89200)
        self.assertEqual(observation(first)["ground_items"][0]["item"]["id"],
                         observation(initial)["ground_items"][0]["item"]["id"])
        for _ in range(4):
            self.assertEqual(self.snapshot()["state"], first["state"])
        peer = self.snapshot(2)
        self.assertIsNone(condition(peer, CONDITION))
        self.assertTrue(all(row["presentation"] is None for row in observation(peer)["ground_items"]))
        self.assertFalse(first["state"]["wizard_game"])

    def test_first_blind_on_use_repeat_duration_and_exact_extra_healing_cure(self):
        ids = initial_items(self, self.snapshot())
        self.drink(ids["blind"])
        first = self.drink(ids["hallucination"])
        self.assertIsNotNone(condition(first, "rogue.blindness"))
        self.assertIsNotNone(condition(first, CONDITION))
        self.assertTrue(inventory_item(first, ids["hallucination"])["identified"])
        self.assertTrue(all(row["presentation"] is None for row in observation(first)["ground_items"]))
        self.assertTrue(all(row["object_presentation"] is None for row in observation(first)["visible_cells"]))
        second = self.drink(ids["hallucination"])
        extension = deadline(second, CONDITION) - deadline(first, CONDITION)
        self.assertGreaterEqual(extension, 80800)
        self.assertLessEqual(extension, 89200)
        ordinary = self.drink(ids["healing"])
        self.assertIsNotNone(condition(ordinary, CONDITION))
        self.drink(ids["blind"])
        sight = self.drink(ids["sight"])
        self.assertIsNotNone(condition(sight, CONDITION))
        self.drink(ids["blind"])
        cured = self.drink(ids["extra"])
        self.assertIsNone(condition(cured, CONDITION))
        self.assertIsNone(condition(cured, "rogue.blindness"))
        self.assertEqual(observation(cured)["ground_items"][0]["item"]["name"], "gold coin")
        self.assertIsNone(observation(cured)["ground_items"][0]["presentation"])
        self.assertTrue(any(row["name"] == "actual visible peer" for row in observation(cured)["visible_actors"]))

    def test_peer_time_publishes_phase_to_owner_and_follower_without_owner_snapshot(self):
        ids = initial_items(self, self.snapshot())
        first = self.drink(ids["hallucination"])
        assert_sample(self, observation(first), 0)
        follower = self.host.call("connect", case=self.case, role="spectator",
            user="hallucination-follower", actors=[1])["connection"]

        def drain_follower():
            result = self.host.call("drain", case=self.case, connection=follower)
            self.assertTrue(result["live"])
            self.assertIsNone(result["closing"])
            rows = [json.loads(raw) for raw in result["frames"]]
            self.assertFalse([row for row in rows if row["type"] == "error"])
            return rows

        drain_follower()
        self.host.request(case=self.case, connection=follower, request_id="follow",
                          request={"type": "attach", "actor": "1"})
        attached = [row["snapshot"] for row in drain_follower() if row["type"] == "snapshot"]
        self.assertEqual(len(attached), 1)
        assert_sample(self, observation(attached[0]), 0)
        self.request(1, self.command(first, {"type": "wait"}))
        owner_updates, follower_updates = [], []

        def collect(rows, output):
            for row in rows:
                if row["type"] == "update" and row["update"]["body"]["type"] in ("observation", "observation_delta"):
                    output.append(row["update"])
                    for private in ("remaining_to_shift", "palette_index", "presentation_catalog"):
                        self.assertNotIn(private, json.dumps(row))

        for _ in range(16):
            status = self.host.call("step", case=self.case)["status"]
            collect(self.drain(1), owner_updates)
            collect(drain_follower(), follower_updates)
            self.drain(2)
            if any(row["cursor"]["tick"] == "200" for row in owner_updates):
                break
            if status == "blocked":
                peer = self.snapshot(2)
                self.assertEqual(observation(peer)["tick"], "100")
                self.assertTrue(peer["readiness"]["admission"])
                self.request(2, self.command(peer, {"type": "wait"}))
            else:
                self.assertEqual(status, "progress")

        def rows(update, key):
            body = update["body"]
            if body["type"] == "observation":
                return body["state"]["observation"][key]
            return [row for edit in body["state"][key] for row in edit["insert"]]

        for updates in (owner_updates, follower_updates):
            shifted = [row for row in updates if row["cursor"]["tick"] == "200"]
            self.assertTrue(shifted, "actual unsolicited phase publication is required")
            self.assertTrue(any(row["name"] == "bat" for update in shifted for row in rows(update, "visible_actors")))
            self.assertTrue(any(row["presentation"] == {"name": "cedar staff", "asset": "item.wand",
                "kind": {"kind": "item", "class": "wand"}} for update in shifted for row in rows(update, "ground_items")))
        current = self.snapshot()
        assert_sample(self, observation(current), 1)
