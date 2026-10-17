"""Narrow production movement witnesses through ordinary server actions.

The existing roster process loop owns all eighteen recipe construction checks;
server rogue_roster integration tests own F/B/K/G executed combat mapping.
"""
import shutil
import unittest
from process_harness import ProcessTestCase
from scalar_wire import u64
from rogue_roster_package import encounter_sources, roster_package, unrelated_fallback


def view(frame):
    return frame["state"]["observation"]


def height(frame, name):
    return min(row["position"]["z"] for row in view(frame)["visible_actors"]
               if row["name"] == name)


class RogueLocomotionProcesses(ProcessTestCase):
    def test_production_flytrap_rejects_voluntary_translation_without_a_paid_turn(self):
        package = roster_package(self, "venus_flytrap", height=5)
        server = self.server(scenario=package, seed=42)
        player, _ = self.client()
        foe, _ = self.client(actor=3)
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        before = self.request(foe, {"type": "snapshot"})
        for direction in ("east", "up"):
            rejected = self.command(foe, {"type": "act", "action": {"type": "move", "direction": direction}})
            self.assertIsNotNone(rejected["error"])
            self.assertEqual(rejected["state"], before["state"])
            self.assertEqual(rejected["history"], before["history"])
        self.flush_save()
        foe.stop(); player.stop(); server.stop()
        shutil.rmtree(package)
        server = self.server(scenario=unrelated_fallback(self), seed=None)
        player, _ = self.client()
        foe, restored = self.client(actor=3)
        self.assertEqual(restored["state"], before["state"])
        rejected = self.command(foe, {"type": "act", "action": {"type": "move", "direction": "east"}})
        self.assertIsNotNone(rejected["error"])
        self.assertEqual(rejected["state"], before["state"])
        foe.stop(); player.stop(); server.stop()

    def test_production_bat_kestrel_and_griffin_pay_for_unsupported_flight(self):
        source = encounter_sources()
        for species in ("bat", "kestrel", "griffin"):
            with self.subTest(species=species):
                self.save = self.directory / (species + ".db")
                package = roster_package(self, species, height=5)
                server = self.server("--checkpoint-interval", 1, scenario=package, seed=42)
                player, initial = self.client()
                foe, _ = self.client(actor=3)
                name = source["actors"]["rogue." + species]["creature"]["name"]
                start = height(initial, name)
                queued = self.command(foe, {"type": "act", "action": {"type": "move", "direction": "up"}})
                self.assertIsNone(queued["error"])
                self.assertIsNone(self.act(player, {"type": "wait"})["error"])
                climbed = self.request(player, {"type": "snapshot"})
                if height(climbed, name) != start + 1:
                    climbed = self.frame(player, lambda f: height(f, name) == start + 1)
                self.assertEqual(height(climbed, name), start + 1)
                for _ in range(2):
                    self.assertIsNone(self.command(foe, {"type": "act", "action": {"type": "wait"}})["error"])
                    self.assertIsNone(self.act(player, {"type": "wait"})["error"])
                hovering = self.request(player, {"type": "snapshot"})
                self.assertEqual(height(hovering, name), start + 1)
                self.assertGreaterEqual(u64(view(hovering)["tick"]), u64(view(initial)["tick"]) + 200)
                saved = self.request(foe, {"type": "snapshot"})
                motion = view(saved).get("motion")
                if motion is not None:
                    self.assertEqual(motion["velocity"], ["0", "0", "0"])
                if species == "griffin":
                    self.flush_save()
                foe.stop(); player.stop(); server.stop()
                if species == "griffin":
                    shutil.rmtree(package)
                    server = self.server(scenario=unrelated_fallback(self), seed=None)
                    player, _ = self.client()
                    foe, restored = self.client(actor=3)
                    self.assertEqual(restored["state"], saved["state"])
                    self.assertEqual(restored["history"], saved["history"])
                    foe.stop(); player.stop(); server.stop()


if __name__ == "__main__":
    unittest.main()
