"""Opaque entity disclosure through the real server and clients."""
import subprocess
import shutil
from arena_matrix import toml_source
from ruleset_sources import load_rules
from process_harness import ProcessTestCase, SPECTATOR_TOKEN, creature_package, door


class TargetProcesses(ProcessTestCase):
    def test_dark_awareness_and_lit_sight_are_distinct_on_the_wire(self):
        from process_harness import ROOT
        package = self.directory / "lighting-disclosure"
        shutil.copytree(ROOT / "scenarios/two-room", package)
        region = package / "regions/1.toml"
        region.write_text('lit = false\nlighting = [{at = [4,1,0], lit = true}]\n'
                          + region.read_text(encoding="utf-8"), encoding="utf-8", newline="\n")
        validation_log = self.directory / "lighting-validation.log"
        with validation_log.open("w", encoding="utf-8") as log:
            result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                    stdout=log, stderr=subprocess.STDOUT, timeout=30)
        self.assertEqual(result.returncode, 0, validation_log.read_text(encoding="utf-8")[-2500:])
        server = self.server(scenario=package)
        player, initial = self.client()
        observation = initial["state"]["observation"]
        cells = observation["visible_cells"]
        here = next(cell for cell in cells if cell["position"] == observation["position"])
        self.assertIs(here["lit"], False, "body awareness must not imply illumination")
        self.assertTrue(any(cell["lit"] for cell in cells), "lit sight must have a positive witness")
        self.assertTrue(all(type(cell["lit"]) is bool for cell in cells))
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop(); server.stop()
        self.server(scenario=package)
        _, restored = self.client()
        self.assertEqual(restored["state"]["observation"]["visible_cells"], cells)

    def test_appearance_seed_is_shared_between_observers_and_survives_restart(self):
        package = creature_package(self, "appearance-observers", abilities=("fixture_mana_bolt",))
        rules_path = package / "rules.toml"
        rules = load_rules(rules_path, self.bin / ("tor-scenario" + self.suffix))
        ability = next(row for row in rules["abilities"] if row["id"] == "fixture_mana_bolt")
        ability["timing"] = {"source": {"kind": "fixed", "prepare": 1, "recover": 1}, "scaled_by": "speed"}
        ability["strikes"]["strikes"][0].pop("check")
        ability["strikes"]["strikes"][0]["on_success"] = [{"kind": "damage", "damage": {"kind": "components", "components": [
            {"category": "energy", "amount": {"kind": "fixed", "amount": 1000}}]}}]
        rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        validation_log = self.directory / "appearance-validation.log"
        with validation_log.open("w", encoding="utf-8") as log:
            validation = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                        stdout=log, stderr=subprocess.STDOUT, timeout=30)
        self.assertEqual(validation.returncode, 0, validation_log.read_text(encoding="utf-8")[-2500:])
        server = self.server(scenario=package)
        player, first = self.client()
        other, second = self.client(token=SPECTATOR_TOKEN, observe=True, actor=2)
        own = first["state"]["observation"]
        other_view = second["state"]["observation"]
        subject = next(actor for actor in other_view["visible_actors"] if actor["name"] == "trainee")
        self.assertNotEqual(own["self_target"], subject["id"])
        self.assertRegex(own["self_appearance"]["seed"], r"^p_[0-9a-f]{64}$")
        self.assertEqual(own["self_appearance"]["seed"], subject["appearance_seed"])
        target = next(actor for actor in own["visible_actors"] if actor["name"] == "practice target")
        killed = self.act(player, {"type": "use_ability", "id": "fixture_mana_bolt",
                                  "invocation": {"aim": {"kind": "actor_target", "target": target["id"]}, "weapon": {"kind": "default"}}})
        self.assertIsNone(killed["error"])
        if not any(ground["item"]["class"] == "corpse" for ground in killed["state"]["observation"]["ground_items"]):
            killed = self.frame(player, lambda frame: any(ground["item"]["class"] == "corpse"
                                for ground in frame["state"]["observation"]["ground_items"]))
        corpse = next(ground["item"] for ground in killed["state"]["observation"]["ground_items"]
                      if ground["item"]["class"] == "corpse")
        self.assertEqual(corpse["corpse"]["seed"], target["appearance_seed"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop(); other.stop(); server.stop()
        self.server(scenario=package)
        _, restored = self.client()
        self.assertEqual(restored["state"]["observation"]["self_appearance"], own["self_appearance"])
        restored_corpse = next(ground["item"] for ground in restored["state"]["observation"]["ground_items"]
                              if ground["item"]["class"] == "corpse")
        self.assertEqual(restored_corpse["corpse"], corpse["corpse"])

    def test_rewind_restart_does_not_rebind_an_abandoned_item_handle(self):
        server = self.server('--checkpoint-interval', 1, wizard=True)
        player, initial = self.client()
        original = {ground['item']['id'] for ground in initial['state']['observation']['ground_items']}
        wizard = self.wizard()
        self.wizard_command(wizard, 'item token 1 1 1 0')
        first = self.request(player, {'type': 'snapshot'})
        created = {ground['item']['id'] for ground in first['state']['observation']['ground_items']} - original
        self.assertEqual(len(created), 1)
        abandoned = created.pop()
        self.wizard_command(wizard, 'rewind initial')
        rewound = self.request(player, {'type': 'snapshot'})
        self.assertEqual(rewound['state']['observation'], initial['state']['observation'])
        self.flush_save()
        player.stop(); wizard.stop(); server.stop()

        server = self.server('--checkpoint-interval', 1, wizard=True)
        player, restored = self.client()
        self.assertDisclosedStateEqual(restored['state'], rewound['state'])
        wizard = self.wizard()
        self.wizard_command(wizard, 'item tablet 1 1 1 0')
        current = self.request(player, {'type': 'snapshot'})
        created = {ground['item']['id'] for ground in current['state']['observation']['ground_items']} - original
        self.assertEqual(len(created), 1)
        replacement = created.pop()
        self.assertNotEqual(replacement, abandoned)
        rejected = self.act(player, {'type': 'take', 'item': abandoned})
        self.assertTrue(rejected['error'].startswith('InvalidAction:'))
        self.assertEqual(rejected['state'], current['state'])
        self.assertEqual(rejected['history'], current['history'])
        self.assertEqual(rejected['intentions'], [])
        taken = self.act(player, {'type': 'take', 'item': replacement})
        self.assertIsNone(taken['error'])
        self.assertEqual(taken['state']['observation']['inventory'][0]['id'], replacement)
        self.flush_save()
        player.stop(); wizard.stop(); server.stop()
        self.server(wizard=True)
        _, continued = self.client()
        self.assertDisclosedStateEqual(continued['state'], taken['state'])

    def test_disclosed_actor_handle_drives_queued_attack_and_history(self):
        self.server(scenario="dungeon-loop", seed=None)
        player, initial = self.client()
        observation = initial["state"]["observation"]
        target = next(actor["id"] for actor in observation["visible_actors"]
                      if actor["name"] == "ruin guard")
        self.assertRegex(target, r"^a_[0-9a-f]{64}$")
        self.assertNotEqual(target, observation["self_target"])
        started = self.act(player, {"type": "use_ability", "id": "basic_melee", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
        self.assertIsNone(started["error"])
        action = next(entry["content"] for entry in started["history"]
                      if entry["content"].get("action", {}).get("type") == "use_ability")
        self.assertEqual(action["action"]["invocation"]["aim"]["target"], target)
        self.assertEqual(action["event"]["target"], target)

    def test_disclosed_door_handle_drives_a_change_and_keeps_its_identity(self):
        self.server()
        player, state = self.client()
        target = door(state)["id"]
        self.assertRegex(target, r"^d_[0-9a-f]{64}$")
        for _ in range(6):
            if door(state)["reachable"]:
                break
            state = self.act(player, {"type": "move", "direction": "east"})
            self.assertIsNone(state["error"])
        self.assertTrue(door(state)["reachable"])
        changed = self.act(player, {"type": "set_door", "door": target, "open": False})
        self.assertIsNone(changed["error"])
        self.assertEqual(door(changed)["id"], target)
        self.assertFalse(door(changed)["open"])
        event = changed["history"][-1]["content"]["event"]
        self.assertEqual(event["door"], target)

    def test_disclosed_item_identity_is_opaque_and_stable_after_restart(self):
        server = self.server()
        player, initial = self.client()
        observation = initial["state"]["observation"]
        target = observation["ground_items"][0]["item"]["id"]
        self.assertRegex(target, r"^i_[0-9a-f]{64}$")
        taken = self.act(player, {"type": "take", "item": target})
        self.assertIsNone(taken["error"])
        self.assertEqual(taken["state"]["observation"]["inventory"][0]["id"], target)
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        self.server()
        _, restored = self.client()
        self.assertDisclosedStateEqual(restored["state"], taken["state"])

    def test_a_handle_from_another_save_rejects_without_admitting_work(self):
        server = self.server()
        player, first = self.client()
        foreign = first["state"]["observation"]["ground_items"][0]["item"]["id"]
        player.stop()
        server.stop()
        self.save = self.save.with_name("other-save.db")
        self.server()
        player, initial = self.client()
        local = initial["state"]["observation"]["ground_items"][0]["item"]["id"]
        self.assertNotEqual(foreign, local)
        rejected = self.act(player, {"type": "take", "item": foreign})
        self.assertTrue(rejected["error"].startswith("InvalidAction:"))
        self.assertEqual(rejected["state"], initial["state"])
        self.assertEqual(rejected["history"], initial["history"])
        self.assertEqual(rejected["intentions"], [])
        self.assertIsNone(self.act(player, {"type": "take", "item": local})["error"])
