"""Real utility zaps, Text persistent-status display and source-free reopen.

Source devices/abilities are untouched. The explicitly accurate fixture caster
and two external collinear targets isolate persistent first-contact behavior.
Service owns cancellation/repetition; ray units own dodge/wall/reflection edges.
"""
import copy
import json
import shutil
import subprocess
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT, TOKEN
from utility_potions_package import observation
from utility_wands_package import (
    assert_anonymous_recipient_feedback, assert_invisibility, assert_persistent, fixture, initial_items,
)


class RogueUtilityWandsProcesses(ProcessTestCase):
    def snapshot(self, client):
        frame = self.request(client, {"type": "snapshot"})
        self.assertIsNone(frame["error"])
        return frame

    def capture_persistent_snapshot(self, target):
        start = len(target.transcript)
        current = self.snapshot(target)
        assert_persistent(self, current, "haste", "invisibility")
        # The ready frame is a client projection. Retain the actual server
        # response from this request, after source-free reopen and Text checks.
        responses = [json.loads(line).get("message") for line in target.transcript[start:]
                     if line.startswith("{")]
        snapshots = [message for message in responses if message and message["type"] == "snapshot"]
        self.assertEqual(len(snapshots), 1)
        sample = snapshots[0]
        self.assertEqual(sample["snapshot"]["actor"], "3")
        self.assertEqual(sample["snapshot"]["state"], current["state"])
        self.persistent_condition_sample = copy.deepcopy(sample)

    def capture_anonymous_feedback(self, target, start, recipient):
        messages = [json.loads(line).get("message") for line in target.transcript[start:]
                    if line.startswith("{")]
        # Retain the actual delta, keeping the required-kind manifest's existing
        # full-observation exclusion unchanged. No synthetic envelope is built.
        deltas = [message for message in messages if message and message.get("type") == "update"
                  and message["update"]["body"]["type"] == "observation_delta"]
        self.anonymous_ability_sample = copy.deepcopy(
            assert_anonymous_recipient_feedback(self, deltas, recipient))
        # Validation context is frozen before this effect. It is not a wire
        # sample and must not be replaced by a later attachment's observation.
        self.anonymous_recipient_context = copy.deepcopy(recipient)

    def package(self):
        data = fixture()
        package = self.directory / "utility-wands"
        (package / "regions").mkdir(parents=True)
        (package / "scenario.toml").write_text(toml_source(data["manifest"]), encoding="utf-8")
        (package / "regions/1.toml").write_text(toml_source(data["regions"][0]), encoding="utf-8")
        (package / "rules.toml").write_text(data["rules"], encoding="utf-8")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return package

    def zap(self, clients, identity):
        player, *peers = clients
        self.assertIsNone(self.act(player, {"type": "zap", "item": identity, "direction": "east"})["error"])
        for _ in range(64):
            current = self.snapshot(player)
            combat = observation(current)["combat"]
            if (observation(current)["ready"] and not current["intentions"]
                    and combat["preparation_remaining"] is None and int(combat["recovery_remaining"]) == 0):
                return current
            for peer in peers:
                ready = self.snapshot(peer)
                if observation(ready)["ready"] and ready["readiness"]["admission"]:
                    self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
        self.fail("Utility zap did not settle within 64 bounded peer decisions")

    def text_stats(self):
        text = self.launch("tor-client-text", ["--script", "--connect", self.address,
                                               "--actor", "3", "--observe"], token=TOKEN)
        text.until(lambda line: line == "Ready.")
        stats = text.command("stats")
        self.assertIn("Hasted: until removed", stats)
        self.assertIn("Invisible: until removed", stats)
        text.stop(save=False)

    def test_persistent_first_contact_utility_state_displays_in_text_and_reopens_without_source(self):
        package = self.package()
        server = self.server("--checkpoint-interval", 1, scenario=package, character=1, seed=42)
        player, initial = self.client(actor=1, omit_memory=True)
        target, _ = self.client(actor=3, omit_memory=True)
        rear, _ = self.client(actor=4, omit_memory=True)
        clients = (player, target, rear)
        ids = initial_items(self, initial)
        self.zap(clients, ids["haste"])
        recipient = self.snapshot(target)
        assert_persistent(self, recipient, "haste")
        assert_persistent(self, self.snapshot(rear))
        anonymous_start = len(target.transcript)
        before = self.zap(clients, ids["invisibility"])
        target_before, rear_before = self.snapshot(target), self.snapshot(rear)
        self.capture_anonymous_feedback(target, anonymous_start, recipient)
        assert_persistent(self, target_before, "haste", "invisibility")
        assert_invisibility(self, before, target_before, rear_before, ids["invisibility"])
        self.text_stats()
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        for client in clients:
            client.stop(save=False)
        server.stop(save=False)
        shutil.rmtree(package)
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        player, restored = self.client(actor=1, omit_memory=True)
        target, target_after = self.client(actor=3, omit_memory=True)
        rear, rear_after = self.client(actor=4, omit_memory=True)
        assert_persistent(self, target_after, "haste", "invisibility")
        assert_invisibility(self, restored, target_after, rear_after, ids["invisibility"])
        self.assertEqual(observation(restored)["inventory"], observation(before)["inventory"])
        self.assertEqual(observation(target_after)["combat"]["own_stats"]["conditions"],
                         observation(target_before)["combat"]["own_stats"]["conditions"])
        self.assertFalse(restored["state"]["wizard_game"])
        self.text_stats()
        self.capture_persistent_snapshot(target)


if __name__ == "__main__":
    unittest.main()
