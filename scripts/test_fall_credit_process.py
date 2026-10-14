"""Ordinary fatal-fall ownership through actual server journal/checkpoint replay."""
import json
import sqlite3
import unittest

from checkpoint_payload import decode_checkpoint_payload
from fall_credit_package import fall_credit_package
from process_harness import ProcessTestCase


def view(frame):
    return frame["state"]["observation"]


class FallCreditProcesses(ProcessTestCase):
    def growth(self, owner):
        page = self.command(owner, {"type": "advancement", "command": "overview"})
        self.assertIsNone(page["error"])
        return page["advancement_page"]["entries"][0]["value"]["growth"]

    def wait_one_decision(self, owner, victim):
        actors = [(client, self.request(client, {"type": "snapshot"}))
                  for client in (owner, victim)]
        ready = [(client, frame) for client, frame in actors
                 if view(frame)["ready"] and not view(frame)["combat"]["dead"]]
        self.assertEqual(len(ready), 1, "fixture has one ordinary due decision")
        self.assertIsNone(self.act(ready[0][0], {"type": "wait"})["error"])

    def test_immediate_fatal_fall_owner_survives_journal_only_and_checkpoint_tail(self):
        for checkpointed in (False, True):
            with self.subTest(checkpointed=checkpointed):
                name = "fall-checkpoint" if checkpointed else "fall-journal"
                package = fall_credit_package(self, name)
                self.save = self.directory / f"{name}.db"
                initial_interval = 1 if checkpointed else 0
                server = self.server("--checkpoint-interval", initial_interval, scenario=package)
                owner, initial = self.client()
                victim, victim_initial = self.client(actor=2)
                self.assertEqual(view(victim_initial)["combat"]["hp"], 5)
                target = next(row["id"] for row in view(initial)["visible_actors"]
                              if row["name"] == "practice target")
                started = self.act(owner, {
                    "type": "use_ability", "id": "push_probe",
                    "invocation": {"kind": "actor_target", "target": target},
                })
                self.assertIsNone(started["error"])
                for _ in range(20):
                    current = self.request(victim, {"type": "snapshot"})
                    if view(current)["combat"]["hp"] == 1:
                        break
                    self.wait_one_decision(owner, victim)
                else:
                    self.fail("ordinary push never reached its live one-Health fall boundary")
                self.assertEqual(view(current)["tick"], "110")
                self.assertFalse(view(current)["combat"]["dead"])
                self.assertEqual(self.growth(owner)["earned_xp"], "0")
                self.assertFalse(current["state"]["wizard_game"])
                for hidden in ("active_fall", "next_fall_episode"):
                    self.assertNotIn(hidden, json.dumps(current))
                self.assertIsNone(self.request(owner, {"type": "save"})["error"])
                with sqlite3.connect(self.save) as database:
                    row = database.execute("SELECT sequence, payload FROM checkpoint").fetchone()
                    if checkpointed:
                        self.assertIsNotNone(row)
                        selected_sequence, payload = row
                        checkpoint = json.loads(decode_checkpoint_payload(payload))
                        actors = checkpoint["shared"]["actors"][checkpoint["game"]["actors"]]
                        cause = actors["2"]["motion"]["active_fall"]
                        self.assertEqual(cause["owner"], 1)
                        self.assertGreater(cause["episode"], 0)
                        self.assertEqual(cause["started_at"], 110)
                    else:
                        self.assertIsNone(row)
                        selected_sequence = 0
                    self.assertGreater(database.execute("SELECT count(*) FROM journal").fetchone()[0], 0)
                # Keep the mid-fall checkpoint fixed, then append a genuine ordinary
                # journal tail under the supported runtime capture-interval policy.
                server.stop(save=False)
                owner.stop()
                victim.stop()
                interval = 4096 if checkpointed else 0
                server = self.server("--checkpoint-interval", interval, scenario=package, seed=None)
                owner, _ = self.client()
                victim, restored = self.client(actor=2)
                self.assertEqual(restored["state"], current["state"])
                self.wait_one_decision(owner, victim)
                tail = self.request(victim, {"type": "snapshot"})
                self.assertEqual(view(tail)["tick"], "120")
                self.assertEqual(view(tail)["combat"]["hp"], 1)
                self.assertIsNone(self.request(owner, {"type": "save"})["error"])
                with sqlite3.connect(self.save) as database:
                    selected = database.execute("SELECT sequence FROM checkpoint").fetchone()
                    self.assertEqual(selected, (selected_sequence,) if checkpointed else None)
                    self.assertGreater(database.execute(
                        "SELECT count(*) FROM journal WHERE sequence > ?", (selected_sequence,),
                    ).fetchone()[0], 0)
                server.stop(save=False)
                owner.stop()
                victim.stop()
                server = self.server("--checkpoint-interval", interval, scenario=package, seed=None)
                owner, _ = self.client()
                victim, continued = self.client(actor=2)
                self.assertEqual(continued["state"], tail["state"])
                self.assertEqual(self.growth(owner)["earned_xp"], "0")
                for _ in range(200):
                    dead = self.request(victim, {"type": "snapshot"})
                    if view(dead)["combat"]["dead"]:
                        break
                    self.wait_one_decision(owner, victim)
                else:
                    self.fail("ordinary gravity never reached the lethal floor impact")
                self.assertEqual(view(dead)["combat"]["hp"], 0)
                awarded = self.growth(owner)
                self.assertEqual(awarded["earned_xp"], "25")
                self.assertEqual(awarded["held_xp"], "25")
                self.assertIsNone(self.request(owner, {"type": "save"})["error"])
                server.stop(save=False)
                owner.stop()
                victim.stop()
                server = self.server("--checkpoint-interval", interval, scenario=package, seed=None)
                owner, _ = self.client()
                victim, dead_again = self.client(actor=2)
                self.assertTrue(view(dead_again)["combat"]["dead"])
                self.assertEqual(self.growth(owner), awarded, "replay awards the fatal boundary once")
                for _ in range(5):
                    self.assertIsNone(self.act(owner, {"type": "wait"})["error"])
                self.assertEqual(self.growth(owner), awarded, "later corpse impacts cannot award XP")
                server.stop(save=False)
                owner.stop()
                victim.stop()


if __name__ == "__main__":
    unittest.main()
