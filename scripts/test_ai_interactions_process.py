"""Knowledge-limited item decisions through actual autonomous server execution."""
import shutil
import subprocess
import unittest

from process_harness import ProcessTestCase, ROOT, WIZARD_TOKEN
from process_harness import creature_package


class AiInteractionProcesses(ProcessTestCase):
    def test_creature_ai_chooses_a_weapon_using_owned_training(self):
        package = creature_package(self, "weapon-training", abilities=("power_strike",))
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        weapons = []
        for name, skill, bonus in [("light_blade", "light_weaponry", 1),
                                   ("heavy_blade", "heavy_weaponry", 0)]:
            weapons.append(name + ' = { class = "weapon", name = "' + name + '", '
                           'equipment = { slot = "weapon", attack = { skill = "' + skill + '", '
                           'bonus = ' + str(bonus) + ', wind_up = 60, recovery = 40, damage = { '
                           'primary = { category = "impact", sides = 6 }, components = [{ '
                           'category = "impact", amount = { type = "rolled", count = 2, '
                           'sides = 6, bonus = 0 } }] } } } }')
        source = source.replace('archetypes = { ', 'archetypes = { ' + ', '.join(weapons) + ', ', 1)
        manifest.write_text(source, encoding="utf-8", newline="\n")
        region = package / "regions/1.toml"
        source = region.read_text(encoding="utf-8").replace(
            'hit_dice = [{ source = "racial" }]',
            'hit_dice = [{ source = "warrior", training = ["heavy_weaponry"] }]')
        source = ('items = [{ id = 100, at = [2,1,0], archetype = "light_blade", carried_by = 2 }, '
                  '{ id = 101, at = [2,1,0], archetype = "heavy_blade", carried_by = 2 }]\n' + source)
        region.write_text(source, encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.server(scenario=package, wizard=True)
        player, _ = self.client()
        observer, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        chosen = None
        for _ in range(12):
            self.act(player, {"type": "wait"})
            view = self.request(observer, {"type": "snapshot"})["state"]["observation"]
            names = {item["id"]: item["name"] for item in view["inventory"]}
            equipped = [item for item in view["interactions"]["inventory"]
                        if item["equipped_slot"] is not None]
            if equipped:
                self.assertEqual(len(equipped), 1)
                chosen = equipped[0]
                self.assertEqual(names[chosen["item"]], "heavy_blade")
                break
        self.assertIsNotNone(chosen, "AI must equip the actual upgrade")
        self.assertEqual(chosen["known_equipment"]["attack"]["skill"], "heavy_weaponry")

    def test_ai_takes_one_ground_healing_unit_and_keeps_the_remaining_stack(self):
        package = self.save.parent / "ground-healing"
        shutil.copytree(ROOT / "scenarios/tests/ai-interactions", package)
        region = package / "regions/1.toml"
        region.write_text(region.read_text().replace(
            'archetype = "healing", quantity = 2, carried_by = 2',
            'archetype = "healing", quantity = 3'))
        subprocess.run([self.bin / "tor-scenario", "validate", package], check=True, capture_output=True, text=True)
        self.server(scenario=package, wizard=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        player, _ = self.client()
        self.wizard_command(self.wizard(), "teleport 1 1 4 1 0")
        for _ in range(12):
            self.act(player, {"type": "wait"})
            view = self.request(observer, {"type": "snapshot"})["state"]["observation"]
            carried = [item for item in view["inventory"] if item["class"] == "potion"]
            if carried:
                break
        self.assertEqual([item["quantity"] for item in carried], ["1"])
        self.assertEqual([item["item"]["quantity"] for item in view["ground_items"] if item["item"]["class"] == "potion"], ["2"])

    def test_ai_walks_to_safe_known_ground_armor_then_replaces_old_gear(self):
        package = self.save.parent / "ground-armor"
        shutil.copytree(ROOT / "scenarios/tests/ai-interactions", package)
        region = package / "regions/1.toml"
        region.write_text(region.read_text().replace(
            '{ id = 11, at = [2, 1, 0], archetype = "mail", carried_by = 2 }',
            '{ id = 11, at = [3, 1, 0], archetype = "mail" }').replace(
            'archetype = "ring", carried_by = 2 }',
            'archetype = "ring", carried_by = 2, equipped_slot = 2 }'))
        subprocess.run([self.bin / "tor-scenario", "validate", package], check=True, capture_output=True, text=True)
        self.server(scenario=package, wizard=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        player, _ = self.client()
        self.wizard_command(self.wizard(), "teleport 1 1 5 1 0")
        equipped = None
        for _ in range(18):
            self.act(player, {"type": "wait"})
            view = self.request(observer, {"type": "snapshot"})["state"]["observation"]
            mail = next((item for item in view["inventory"] if item["name"] == "mail"), None)
            if mail:
                equipped = next(item["equipped_slot"] for item in view["interactions"]["inventory"] if item["item"] == mail["id"])
                if equipped == 0:
                    break
        self.assertEqual(equipped, 0, "AI must travel, pick up known armor and complete replacement")

    def test_ai_replaces_armor_fills_duplicate_ring_and_restores_equipment(self):
        server = self.server(scenario="ai-interactions", wizard=True)
        observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=2)
        player, _ = self.client()
        wizard = self.wizard()
        self.wizard_command(wizard, "teleport 1 1 4 1 0")
        items = {item["name"]: item["id"] for item in initial["state"]["observation"]["inventory"]}

        def equipment(view):
            return {item["item"]: item["equipped_slot"] for item in view["interactions"]["inventory"]}

        expected = {items["old mail"]: None, items["mail"]: 0,
                    items["old ring"]: 1, items["ring"]: 2, items["silver ring"]: None}
        finished = None
        for _ in range(14):
            self.act(player, {"type": "wait"})
            view = self.request(observer, {"type": "snapshot"})["state"]["observation"]
            actual = equipment(view)
            if all(actual[item] == slot for item, slot in expected.items()):
                finished = view
                break
        self.assertIsNotNone(finished, "AI must complete armor replacement and fill its second ring socket")
        hidden = next(item for item in finished["interactions"]["inventory"] if item["item"] == items["silver ring"])
        self.assertIsNone(hidden["known_equipment"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        wizard.stop()
        observer.stop()
        player.stop()
        server.stop()
        self.server(scenario="ai-interactions", wizard=True, seed=None)
        restored, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        view = self.request(restored, {"type": "snapshot"})["state"]["observation"]
        self.assertEqual({item: equipment(view)[item] for item in expected}, expected)

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
        potion = next(item for item in state["inventory"] if item["class"] == "potion")
        self.assertEqual(potion["quantity"], "2")
        self.assertEqual(potion["name"], "red potion")
        self.assertFalse(potion["identified"])

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
        self.assertEqual([item["quantity"] for item in state["state"]["observation"]["inventory"] if item["class"] == "potion"], ["1"])


if __name__ == "__main__":
    unittest.main()
