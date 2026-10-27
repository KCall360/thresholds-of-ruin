"""Encoded source utility rays, persistence, nonstacking and neutralization."""
import json

from utility_potions_package import observation
from utility_potions_service_contracts import UtilityServiceHarness
from utility_wands_package import (
    assert_anonymous_recipient_feedback, assert_invisibility, assert_persistent, fixture, initial_items, live_combat_events,
)


class UtilityWandsServiceContracts(UtilityServiceHarness):
    make_fixture = staticmethod(fixture)
    actors = (1, 3, 4)

    def setUp(self):
        self.spectators = {}
        self.spectator_frames = {actor: [] for actor in self.actors}
        self.player_frames = {actor: [] for actor in self.actors}
        super().setUp()
        for actor in self.actors:
            peer = self.host.call("connect", case=self.case, role="spectator",
                                  user=f"utility-observer-{actor}", actors=[actor])["connection"]
            self.spectators[actor] = peer
            welcome = self.drain_spectator(actor)
            self.assertTrue(any(row["type"] == "welcome" and row["role"] == "spectator"
                                for row in welcome))
            self.request_spectator(actor, {"type": "attach", "actor": str(actor)})

    def drain(self, actor):
        rows = super().drain(actor)
        self.player_frames[actor].extend(rows)
        # Retain every preparation/event/delta, not just the final snapshot.
        # Drain during ordinary peer advancement to keep output budgets bounded.
        for observer in self.spectators:
            self.drain_spectator(observer)
        return rows

    def drain_spectator(self, actor, *, allow_errors=False):
        result = self.host.call("drain", case=self.case, connection=self.spectators[actor])
        self.assertTrue(result["live"])
        self.assertIsNone(result["closing"])
        rows = [json.loads(raw) for raw in result["frames"]]
        self.spectator_frames[actor].extend(rows)
        if not allow_errors:
            self.assertFalse([row for row in rows if row["type"] == "error"], rows)
        return rows

    def request_spectator(self, actor, request, *, allow_errors=False):
        self.serial += 1
        self.host.request(case=self.case, connection=self.spectators[actor],
                          request_id=f"utility-spectator-{self.serial}", request=request)
        return self.drain_spectator(actor, allow_errors=allow_errors)

    def spectator_snapshot(self, actor):
        rows = self.request_spectator(actor, {"type": "snapshot"})
        snapshots = [row["snapshot"] for row in rows if row["type"] == "snapshot"]
        self.assertEqual(len(snapshots), 1)
        self.assertFalse(snapshots[0]["has_control"])
        self.assertFalse(snapshots[0]["readiness"]["admission"])
        return snapshots[0]

    def assert_private_source_streams(self, invisible_id):
        seen = {actor: self.spectator_snapshot(actor) for actor in self.actors}
        assert_invisibility(self, seen[1], seen[3], seen[4], invisible_id)
        assert_persistent(self, seen[3], "invisibility", "slow")
        for actor in self.actors:
            owner = self.snapshot(actor)
            self.assertEqual(observation(seen[actor])["combat"], observation(owner)["combat"])
            self.assertEqual(observation(seen[actor])["visible_actors"],
                             observation(owner)["visible_actors"])
            self.request(actor, {"type": "history", "before": None, "limit": 100})
            history = self.request_spectator(actor, {"type": "history", "before": None, "limit": 100})
            pages = [row["page"] for row in history if row["type"] == "history"]
            self.assertEqual(len(pages), 1)
            if actor == 1:
                self.assertTrue(pages[0]["entries"], "Read back actual utility-use history")

        # Wizard diagnostics intentionally retain raw source facts. This fixture
        # proves a real spectator cannot enter that path; positive capture and
        # private reply semantics remain owned by the existing session/IT tests.
        current = seen[1]
        request = self.command(current, {"type": "wait"})
        request["command"] = {"type": "wizard", "expected_revision": current["state"]["revision"],
                              "operation": "combat inspect"}
        denied = self.request_spectator(1, request, allow_errors=True)
        self.assertEqual([row["code"] for row in denied if row["type"] == "error"], ["unauthorized"])
        self.assertFalse(any(row["type"] == "combat_diagnostics" for row in denied))

        # Owning a device does not grant its ability for preview. Compare a
        # guessed real utility ID with an absent ID through the same wire query.
        before = self.snapshot()
        failures = []
        for ability in ("rogue.wand_invisibility", "fixture.absent_utility"):
            request = self.command(before, {"type": "wait"})
            request["command"] = {"type": "preview_ability_weapon",
                "expected_revision": before["state"]["revision"], "ability": ability,
                "invocation": {"aim": {"kind": "directional_ray", "direction": "east"},
                               "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}}
            self.serial += 1
            self.host.request(case=self.case, connection=self.peers[1],
                              request_id=f"utility-preview-{self.serial}", request=request)
            result = self.host.call("drain", case=self.case, connection=self.peers[1])
            self.assertTrue(result["live"])
            self.assertIsNone(result["closing"])
            rows = [json.loads(raw) for raw in result["frames"]]
            self.player_frames[1].extend(rows)
            errors = [row for row in rows if row["type"] == "error"]
            self.assertEqual(len(errors), 1)
            self.assertEqual(errors[0]["code"], "invalid_action")
            self.assertFalse(any(row["type"] == "ability_weapon_preview" for row in rows))
            failures.append((errors[0]["code"], errors[0]["message"]))
        self.assertEqual(failures[0], failures[1])
        self.assertEqual(self.snapshot()["state"], before["state"], "Denied previews are read-only")
        for streams in (self.player_frames, self.spectator_frames):
            for rows in streams.values():
                self.assertFalse(any(row["type"] == "combat_diagnostics" for row in rows))
                encoded = json.dumps(rows, sort_keys=True)
                for hidden in ("rogue.wand_invisibility", "Wand of Invisibility", "wand of invisibility"):
                    self.assertNotIn(hidden, encoded,
                                     "Snapshots, updates, history and denied previews must keep the source private")

    def zap(self, identity):
        return self.act(1, {"type": "zap", "item": identity, "aim": {"kind": "direction", "direction": "east"}})

    def test_source_wands_keep_single_persistent_status_cancel_to_neutral_and_stop_at_first_contact(self):
        ids = initial_items(self, self.snapshot())
        baseline = observation(self.snapshot(3))["combat"]["own_stats"]
        self.zap(ids["haste"])
        assert_persistent(self, self.snapshot(3), "haste")
        assert_persistent(self, self.snapshot(4))
        self.zap(ids["haste"])
        repeated = self.snapshot(3)
        assert_persistent(self, repeated, "haste")
        for field in ("attributes", "skills", "defenses"):
            self.assertEqual(observation(repeated)["combat"]["own_stats"][field], baseline[field], field)
        self.zap(ids["slow"])
        assert_persistent(self, self.snapshot(3))
        self.zap(ids["slow"])
        assert_persistent(self, self.snapshot(3), "slow")
        self.zap(ids["haste"])
        assert_persistent(self, self.snapshot(3))
        recipient = self.snapshot(3)
        recipient_starts = (len(self.player_frames[3]), len(self.spectator_frames[3]))
        self.zap(ids["invisibility"])
        for frames, start in zip((self.player_frames[3], self.spectator_frames[3]), recipient_starts):
            assert_anonymous_recipient_feedback(self, frames[start:], recipient)
        caster_starts = (len(self.player_frames[1]), len(self.spectator_frames[1]))
        self.zap(ids["slow"])
        for frames, start in zip((self.player_frames[1], self.spectator_frames[1]), caster_starts):
            self.assertFalse([event for message in frames[start:] for event in live_combat_events(message)],
                             "An already hidden first contact cannot disclose even an event count to the caster")
        final = self.snapshot()
        target, rear = self.snapshot(3), self.snapshot(4)
        assert_persistent(self, target, "invisibility", "slow")
        assert_invisibility(self, final, target, rear, ids["invisibility"])
        self.assertFalse(final["state"]["wizard_game"])
        self.assert_private_source_streams(ids["invisibility"])
