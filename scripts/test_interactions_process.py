"""Timed equipment and effects through actual server and headless client processes."""
import unittest

from process_harness import ProcessTestCase


class InteractionProcesses(ProcessTestCase):
    def test_text_commands_use_carried_affordances_and_opaque_choices(self):
        self.server(scenario="interactions")
        probe, initial = self.client(observe=True)
        text, welcome = self.text_client()
        self.assertNotIn("potion of healing", welcome)
        self.assertIn("Which item", text.command("drink red potion"))
        self.assertIn("ItemStarted", text.command("wear mail"))
        view = self.request(probe, {"type": "snapshot"})["state"]["observation"]
        mail = next(item["id"] for item in view["inventory"] if item["name"] == "mail")
        self.assertEqual(next(item for item in view["interactions"]["inventory"] if item["item"] == mail)["equipped_slot"], 1)
        self.assertIn("ItemStarted", text.command("remove mail"))
        potion = next(item for item in view["inventory"] if item["class"] == "potion")
        self.assertIn("ItemStarted", text.command(f"drink #{potion['id']}"))
        after = self.request(probe, {"type": "snapshot"})["state"]["observation"]
        self.assertEqual(next(item for item in after["inventory"] if item["id"] == potion["id"])["quantity"], "1")
        self.assertIsNone(next(item for item in after["interactions"]["inventory"] if item["item"] == mail)["equipped_slot"])

    def test_adventure_equips_removes_and_drinks_without_hidden_names(self):
        self.server(scenario="interactions")
        probe, _ = self.client(observe=True)
        player, welcome = self.adventure()
        self.assertNotIn("potion of healing", welcome)
        passage = self.say(player, "wear mail").lower()
        self.assertIn("begin", passage)
        self.assertIn("finish equipping", passage)
        view = self.request(probe, {"type": "snapshot"})["state"]["observation"]
        mail = next(item["id"] for item in view["inventory"] if item["name"] == "mail")
        self.assertEqual(next(item for item in view["interactions"]["inventory"] if item["item"] == mail)["equipped_slot"], 1)
        self.assertIn("begin", self.say(player, "take off mail").lower())
        self.assertIn("can't", self.say(player, "drink mail").lower())
        # Adventure already treats indistinguishable stacks as interchangeable;
        # its representative follows disclosed order and consumes one unit.
        self.assertIn("begin", self.say(player, "drink red potion").lower())
        after = self.request(probe, {"type": "snapshot"})["state"]["observation"]
        self.assertEqual(sorted(int(item["quantity"]) for item in after["inventory"] if item["class"] == "potion"), [1, 2])
        self.assertIsNone(next(item for item in after["interactions"]["inventory"] if item["item"] == mail)["equipped_slot"])

    def test_equipment_consumption_disclosure_and_checkpoint_restart(self):
        server = self.server("--checkpoint-interval", 1, scenario="interactions")
        player, initial = self.client()
        view = initial["state"]["observation"]
        self.assertEqual(view["interactions"]["slots"], ["weapon", "body_armor", "ring", "ring"])
        items = {item["name"]: item["id"] for item in view["inventory"]}
        ring = next(item for item in view["interactions"]["inventory"] if item["item"] == items["silver ring"])
        self.assertEqual(ring["equipped_slot"], 2)
        self.assertIsNone(ring["known_equipment"])
        mail = items["mail"]
        equipped = self.act(player, {"type": "equip", "item": mail, "slot": 1})
        self.assertIsNone(equipped["error"])
        view = equipped["state"]["observation"]
        self.assertEqual(view["interactions"]["completed"], [{"type": "equip", "item": mail, "slot": 1}])
        self.assertEqual(int(view["tick"]), 300)
        self.assertEqual(next(item for item in view["interactions"]["inventory"] if item["item"] == mail)["equipped_slot"], 1)
        denied = self.act(player, {"type": "drop", "item": mail})
        self.assertIsNotNone(denied["error"])
        self.assertEqual(denied["state"], equipped["state"])
        removed = self.act(player, {"type": "unequip", "item": mail})
        self.assertIsNone(removed["error"])
        self.assertEqual(int(removed["state"]["observation"]["tick"]), 600)
        # Both unknown potions have the same physical affordance; identify only
        # the instance that produces an observable effect.
        potions = [item for item in view["inventory"] if item["class"] == "potion"]
        final = removed
        for potion in potions:
            final = self.act(player, {"type": "drink", "item": potion["id"]})
            self.assertIsNone(final["error"])
        view = final["state"]["observation"]
        self.assertEqual(sorted(int(item["quantity"]) for item in view["inventory"] if item["class"] == "potion"), [1, 1])
        self.assertTrue(any(item["identified"] for item in view["inventory"] if item["class"] == "potion"))
        self.assertEqual(final["intentions"], [])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        self.server("--checkpoint-interval", 1, scenario="interactions", seed=None)
        _, restored = self.client()
        self.assertEqual(restored["state"], final["state"])
        self.assertEqual(restored["history"], final["history"])


if __name__ == "__main__":
    unittest.main()
