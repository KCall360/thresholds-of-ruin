"""Timed equipment and effects through actual server and headless client processes."""
import unittest

from process_harness import ProcessTestCase


class InteractionProcesses(ProcessTestCase):
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
