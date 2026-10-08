"""Knowledge-limited item decisions through actual autonomous server execution."""
import shutil
import subprocess
import unittest

from process_harness import ProcessTestCase, ROOT, WIZARD_TOKEN


class AiInteractionProcesses(ProcessTestCase):
    def test_human_knowledge_does_not_make_an_unknown_ai_potion_usable(self):
        package = self.save.parent / "unknown-ai"
        shutil.copytree(ROOT / "scenarios/tests/ai-interactions", package)
        region = package / "regions/1.toml"
        region.write_text(region.read_text().replace('known_identities = ["healing"]', 'known_identities = []'))
        manifest = package / "scenario.toml"
        manifest.write_text(manifest.read_text().replace('characters = [{ id = 1,', 'characters = [{ id = 1, known_identities = ["healing"],'))
        subprocess.run([self.bin / "tor-scenario", "validate", package], check=True, capture_output=True, text=True)
        self.server(scenario=package, wizard=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        player, initial = self.client()
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"] if actor["name"] == "ruin guard")
        self.act(player, {"type": "attack", "target": target})
        for _ in range(4):
            self.act(player, {"type": "wait"})
        state = self.request(observer, {"type": "snapshot"})["state"]["observation"]
        self.assertEqual(state["combat"]["hp"], 15)
        self.assertEqual(state["inventory"][0]["quantity"], "2")
        self.assertEqual(state["inventory"][0]["name"], "red potion")
        self.assertFalse(state["inventory"][0]["identified"])

    def test_wounded_ai_uses_its_known_healing_and_restores_consumed_stack(self):
        server = self.server(scenario="ai-interactions", wizard=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        player, initial = self.client()
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"] if actor["name"] == "ruin guard")
        self.act(player, {"type": "attack", "target": target})
        healed = None
        for _ in range(5):
            state = self.request(observer, {"type": "snapshot"})
            potions = [item for item in state["state"]["observation"]["inventory"] if item["class"] == "potion"]
            if potions and potions[0]["quantity"] == "1":
                healed = state
                break
            self.act(player, {"type": "wait"})
        self.assertIsNotNone(healed, "autonomous AI must complete known healing")
        self.assertEqual(healed["state"]["observation"]["combat"]["hp"], 25)
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        observer.stop()
        player.stop()
        server.stop()
        self.server(scenario="ai-interactions", wizard=True, seed=None)
        restored, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        state = self.request(restored, {"type": "snapshot"})
        self.assertEqual(state["state"]["observation"]["combat"]["hp"], 25)
        self.assertEqual([item["quantity"] for item in state["state"]["observation"]["inventory"]], ["1"])


if __name__ == "__main__":
    unittest.main()
