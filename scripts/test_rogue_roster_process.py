"""New source-qualified actual-process roster acceptance, not balance evidence."""
from collections import Counter
import shutil
import unittest
from process_harness import ProcessTestCase, SPECTATOR_TOKEN, hit_die_counts
from rogue_roster_package import SPECIES, encounter_sources, roster_package, unrelated_fallback


def view(frame):
    return frame["state"]["observation"]


class RogueRosterProcesses(ProcessTestCase):
    def test_all_catalog_recipes_compile_and_disclose_their_real_owned_hd(self):
        source = encounter_sources()
        for species in SPECIES:
            with self.subTest(species=species):
                package = roster_package(self, species)
                self.save = self.directory / (species + ".db")
                server = self.server(scenario=package, seed=42)
                player, initial = self.client()
                foe, observed = self.client(actor=3)
                recipe = source["actors"]["rogue." + species]["creature"]
                self.assertFalse(initial["state"]["wizard_game"])
                summary = view(observed)["combat"]["own_stats"]["hit_dice"]
                self.assertEqual(summary["total"], len(recipe["hit_dice"]))
                expected = Counter(("racial" if die["source"] == "racial" else "class", die["source"])
                                   for die in recipe["hit_dice"])
                self.assertEqual(hit_die_counts(summary), dict(expected))
                self.assertEqual(view(observed)["combat"]["own_stats"]["nutrition"]["kind"], "exempt")
                self.assertIn(recipe["name"], [row["name"] for row in view(initial)["visible_actors"]])
                foe.stop(); player.stop(); server.stop()

    def test_ordinary_quagga_sequence_and_text_disclosure_survive_source_free_restart(self):
        package = roster_package(self, "quagga")
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=42)
        player, _ = self.client()
        foe, ready = self.client(actor=3)
        target = next(row["id"] for row in view(ready)["visible_actors"] if row["name"] == "Warrior")
        queued = self.command(foe, {"type": "act", "action": {
            "type": "use_ability", "id": "basic_melee",
            "invocation": {"kind": "actor_target", "target": target},
            "supplies": {"kind": "carried"}}})
        self.assertIsNone(queued["error"])
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        # Both authored offsets are zero, so the completed pair is published
        # together rather than promising an intermediate sequence snapshot.
        resolved = self.frame(foe, lambda frame: len([
            event for event in view(frame)["combat"]["events"]
            if event["type"] == "ability" and event["ability"]["id"] == "basic_melee"]) == 2)
        strikes = [event for event in view(resolved)["combat"]["events"]
                   if event["type"] == "ability" and event["ability"]["id"] == "basic_melee"]
        self.assertEqual([event["strike"] for event in strikes], [0, 1])
        self.assertTrue(all(event["target"] == target for event in strikes))
        for _ in range(4):
            current = self.request(foe, {"type": "snapshot"})
            if not current["intentions"]:
                break
            self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        else:
            self.fail("Ordinary quagga sequence did not settle within bounded waits")
        text, welcome = self.adventure(SPECTATOR_TOKEN)
        self.assertIn("quagga", welcome.lower())
        text.stop()
        before = self.request(foe, {"type": "snapshot"})
        self.flush_save()
        foe.stop(); player.stop(); server.stop()
        shutil.rmtree(package)
        server = self.server(scenario=unrelated_fallback(self), seed=None)
        player, _ = self.client()
        foe, restored = self.client(actor=3)
        self.assertEqual(restored["state"], before["state"])
        self.assertEqual(restored["history"], before["history"])
        foe.stop(); player.stop(); server.stop()


if __name__ == "__main__":
    unittest.main()
