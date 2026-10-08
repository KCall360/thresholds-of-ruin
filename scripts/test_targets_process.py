"""Opaque entity disclosure through the real server and clients."""
from process_harness import ProcessTestCase, door


class TargetProcesses(ProcessTestCase):
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
        self.assertEqual(restored['state'], rewound['state'])
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
        self.assertEqual(continued['state'], taken['state'])

    def test_disclosed_actor_handle_drives_queued_attack_and_history(self):
        self.server(scenario="dungeon-loop", seed=None)
        player, initial = self.client()
        observation = initial["state"]["observation"]
        target = next(actor["id"] for actor in observation["visible_actors"]
                      if actor["name"] == "ruin guard")
        self.assertRegex(target, r"^a_[0-9a-f]{64}$")
        self.assertNotEqual(target, observation["self_target"])
        started = self.act(player, {"type": "attack", "target": target})
        self.assertIsNone(started["error"])
        action = next(entry["content"] for entry in started["history"]
                      if entry["content"].get("action", {}).get("type") == "attack")
        self.assertEqual(action["action"]["target"], target)
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
        self.assertEqual(restored["state"], taken["state"])

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
