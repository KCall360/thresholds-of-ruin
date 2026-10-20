"""Actual server/headless utility doses and source-free recovery.

The source potion effects/conditions and Phantom remain unchanged. Geometry,
finite carried stock, exempt fixture nutrition and the stationary external foe
are explicit test setup. Service owns dose permutations; existing
test_magic_items_process and test_restorable_loss_process retain Text/native
Drink input. This witness adds no new frontend control or balance claim.
"""
import shutil
import subprocess
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT
from utility_potions_package import (
    assert_extra_healing, assert_poison, assert_sight, condition, deadline, fixture, initial_items, inventory_item, observation,
)


class RogueUtilityPotionsProcesses(ProcessTestCase):
    def snapshot(self, client):
        frame = self.request(client, {"type": "snapshot"})
        self.assertIsNone(frame["error"])
        return frame

    def package(self):
        source = fixture()
        package = self.directory / "utility-potions"
        (package / "regions").mkdir(parents=True)
        (package / "scenario.toml").write_text(toml_source(source["manifest"]), encoding="utf-8")
        (package / "regions/1.toml").write_text(toml_source(source["regions"][0]), encoding="utf-8")
        (package / "rules.toml").write_text(source["rules"], encoding="utf-8")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return package

    def drink(self, player, phantom, identity):
        self.assertIsNone(self.act(player, {"type": "drink", "item": identity})["error"])
        for _ in range(24):
            current = self.snapshot(player)
            combat = observation(current)["combat"]
            if (observation(current)["ready"] and not current["intentions"]
                    and combat["preparation_remaining"] is None and int(combat["recovery_remaining"]) == 0):
                return current
            peer = self.snapshot(phantom)
            if observation(peer)["ready"] and peer["readiness"]["admission"]:
                self.assertIsNone(self.act(phantom, {"type": "wait"})["error"])
        self.fail("Ordinary potion work exceeded its bounded external-peer decisions")

    def test_poison_sight_cure_extension_and_unknown_identity_survive_source_free_reopen(self):
        package = self.package()
        server = self.server("--checkpoint-interval", 1, scenario=package, character=1, seed=42)
        player, initial = self.client(actor=1, omit_memory=True)
        phantom, _ = self.client(actor=3, omit_memory=True)
        ids = initial_items(self, initial)
        poisoned = self.drink(player, phantom, ids["poison"])
        deficit = assert_poison(self, poisoned, ids["poison"])
        blind = self.drink(player, phantom, ids["blind"])
        healed = self.drink(player, phantom, ids["extra"])
        assert_extra_healing(self, blind, healed, ids["extra"])
        blind = self.drink(player, phantom, ids["blind"])
        self.assertIsNotNone(condition(blind, "rogue.blindness"))
        first = self.drink(player, phantom, ids["sight"])
        assert_sight(self, first, ids["sight"])
        second = self.drink(player, phantom, ids["sight"])
        assert_sight(self, second, ids["sight"])
        self.assertEqual(inventory_item(second, ids["sight"])["quantity"], "1")
        self.assertGreaterEqual(deadline(second) - deadline(first), 80800)
        self.assertLessEqual(deadline(second) - deadline(first), 89200)
        self.assertEqual(observation(second)["combat"]["own_stats"]["restorable_losses"]["attributes"]["strength"], deficit)
        self.assertFalse(second["state"]["wizard_game"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop(save=False)
        phantom.stop(save=False)
        server.stop(save=False)
        shutil.rmtree(package)
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        player, restored = self.client(actor=1, omit_memory=True)
        assert_poison(self, restored, ids["poison"])
        assert_sight(self, restored, ids["sight"])
        self.assertEqual(observation(restored)["inventory"], observation(second)["inventory"])
        self.assertEqual(observation(restored)["combat"]["own_stats"]["conditions"],
                         observation(second)["combat"]["own_stats"]["conditions"])
        self.assertEqual(deadline(restored), deadline(second))
        self.assertFalse(restored["state"]["wizard_game"])


if __name__ == "__main__":
    unittest.main()
