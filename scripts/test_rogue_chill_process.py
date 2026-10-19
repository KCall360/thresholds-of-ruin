"""New reconstruction: exact CH1 and actual Mage kit through real applications.

Controlled ice uses unchanged checked content. A bounded seed witness proves the
lifecycle; it is not an autonomous fight, food study or accepted win rate.
"""
from scalar_wire import u64
import shutil
import unittest
from process_harness import AdventureProcess, ProcessTestCase, SPECTATOR_TOKEN, underfoot
from rogue_chill_package import CHILL, FROZEN, chill_package
from rogue_roster_package import unrelated_fallback
from status_condition_package import checkpoint


def view(frame):
    return frame["state"]["observation"]


def phase(frame):
    return next((row["phase"] for row in view(frame)["combat"]["own_stats"]["conditions"]
                 if row["condition"]["id"] == FROZEN), None)


class RogueChillProcesses(ProcessTestCase):
    def test_actual_checked_chill_preserves_care_holds_through_restart_and_explicit_thaw_resume(self):
        package = chill_package(self)
        witness = None
        outcomes = []
        # Keep the actual defense-16 Mage kit. Seed 0 is a miss control; 749
        # supplies the deterministic double-check witness without altered stats.
        for seed in (0, 749):
            self.save = self.directory / f"chill-{seed}.db"
            server = self.server("--checkpoint-interval", 1, scenario=package, character=2, seed=seed)
            player, initial = self.client(actor=2)
            ice, ice_initial = self.client(actor=3)
            self.assertFalse(initial["state"]["wizard_game"])
            started = self.act(player, {"type": "use_ability", "id": "bind_wounds",
                "invocation": {"aim": {"kind": "actor_target", "target": view(initial)["self_target"]}, "weapon": {"kind": "default"}},
                "supplies": {"kind": "carried"}})
            self.assertIsNone(started["error"])
            original = checkpoint(self, player)
            target = next(row["id"] for row in view(ice_initial)["visible_actors"] if row["name"] == "Mage")
            cast = self.act(ice, {"type": "use_ability", "id": CHILL,
                "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
            self.assertIsNone(cast["error"])
            for _ in range(12):
                current = self.request(player, {"type": "snapshot"})
                if phase(current) == "active" or not current["intentions"]:
                    break
                self.assertIsNone(self.act(ice, {"type": "wait"})["error"])
            frozen = phase(current) == "active"
            outcomes.append({"seed": seed, "frozen_care": frozen})
            if frozen:
                witness = (server, player, ice, initial, current, original)
                break
            ice.stop(); player.stop(); server.stop()
        self.assertIsNotNone(witness, f"No actual-kit freeze witness in bounded seeds: {outcomes}")
        self.assertEqual(outcomes, [{"seed": 0, "frozen_care": False},
                                    {"seed": 749, "frozen_care": True}])
        server, player, ice, initial, paused, original = witness
        saved = checkpoint(self, player)
        self.assertTrue(any(row["phase"] == "paused" for row in paused["intentions"]))
        self.assertEqual(u64(view(paused)["combat"]["hp"]), u64(view(initial)["combat"]["hp"]))
        pending = saved["actors"]["2"]["pending"]
        for field in ("plan", "hold", "item_payment", "progress"):
            self.assertEqual(pending[field], original["actors"]["2"]["pending"][field])
        self.assertEqual(saved["items"], original["items"])
        here = underfoot(paused["state"])["key"]
        denied = self.command(player, {"type": "cancel_intention", "step": "west"})
        self.assertIsNotNone(denied["error"])
        self.assertEqual(underfoot(denied["state"])["key"], here)
        self.assertEqual(denied["intentions"], paused["intentions"])
        text = self.launch("tor-client-text", ["--connect", self.address, "--actor", "2"],
                           cls=AdventureProcess, token=SPECTATOR_TOKEN)
        welcome = text.until(lambda line: line == "> ")
        self.assertIn("ice monster", welcome)
        self.assertIn("Frozen", self.say(text, "stats"))
        text.stop()
        self.flush_save(actor=2)
        ice.stop(); player.stop(); server.stop()
        shutil.rmtree(package)
        server = self.server("--checkpoint-interval", 1, scenario=unrelated_fallback(self),
                             seed=None, save_actor=2)
        player, restored = self.client(actor=2)
        ice, _ = self.client(actor=3)
        self.assertEqual(checkpoint(self, player), saved)
        self.assertFalse(view(restored)["combat"]["preparation_active"])
        for _ in range(12):
            thawed = self.request(player, {"type": "snapshot"})
            if phase(thawed) == "protection":
                break
            self.assertIsNone(self.act(ice, {"type": "wait"})["error"])
        else:
            self.fail("CH1 failed to reach its shared protection phase")
        self.assertFalse(view(thawed)["combat"]["preparation_active"], "Thaw must not auto-resume human care")
        self.assertEqual(checkpoint(self, player)["actors"]["2"]["pending"], pending)
        self.assertLess(view(thawed)["combat"]["own_stats"]["nutrition"]["reserve"],
                        view(initial)["combat"]["own_stats"]["nutrition"]["reserve"])
        resumed = self.command(player, {"type": "resume_intention"})
        self.assertIsNone(resumed["error"])
        for _ in range(12):
            complete = self.request(player, {"type": "snapshot"})
            if not complete["intentions"]:
                break
            self.assertIsNone(self.act(ice, {"type": "wait"})["error"])
        else:
            self.fail("Explicitly resumed care failed to complete")
        bandage = next(row for row in view(complete)["inventory"] if row["name"] == "clean bandage")
        self.assertEqual(int(bandage["quantity"]), 2)
        ice.stop(); player.stop(); server.stop()


if __name__ == "__main__":
    unittest.main()
