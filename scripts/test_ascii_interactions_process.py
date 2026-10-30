"""Equipment and consumption through the ASCII client's native keyboard."""
import json
import unittest

from process_harness import ProcessTestCase, read_capture


class AsciiInteractionProcesses(ProcessTestCase):
    graphical = True

    def test_native_equipment_removal_and_unknown_potion_choices(self):
        self.server(scenario="interactions")
        capture = self.save.parent / "interactions.ppm"
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames", "--capture", capture])
        initial = self.ascii_frame(window, lambda f: f["state"] and not f["busy"] and f["can_admit_intention"])
        native = self.native_keys(window)

        def press(name, action, **completion):
            if name in ("W", "T"):
                native("Shift_L", True)
                try:
                    return self.native_press(window, native, name.lower(), action, **completion)
                finally:
                    native("Shift_L", False)
            return self.native_press(window, native, name, action, **completion)

        def view(frame):
            return frame["state"]["observation"]

        mail = next(item["id"] for item in view(initial)["inventory"] if item["name"] == "mail")
        # W always asks, even with one eligible armor item.
        asking = press("W", "wear")
        self.assertEqual(view(asking), view(initial), "opening a selection costs no turn")
        equipped = press("Return", "enter", wait_for_ready=True, wait_for_admission=True)
        self.assertEqual(next(item for item in view(equipped)["interactions"]["inventory"] if item["item"] == mail)["equipped_slot"], 1)
        self.assertEqual(int(view(equipped)["tick"]), 300)
        self.assertEqual(equipped["messages"]["turn"].count("You finish equipping the mail."), 1)
        self.assertEqual(equipped["messages"]["turn"], "You finish equipping the mail.")
        selected = press("T", "take_off")
        self.assertEqual(view(selected), view(equipped), "opening a selection costs no turn")
        self.assertEqual(len(selected["item_rows"]), 1, "T excludes equipped weapons and accessories")
        removed = press("Return", "enter", wait_for_ready=True, wait_for_admission=True)
        self.assertIsNone(next(item for item in view(removed)["interactions"]["inventory"] if item["item"] == mail)["equipped_slot"])
        self.assertEqual(int(view(removed)["tick"]), 600)
        choosing = press("q", "drink")
        self.assertEqual(view(choosing), view(removed))
        self.assertNotIn("potion of healing", json.dumps(choosing["state"]))
        self.assertNotIn("potion of poison", json.dumps(choosing["state"]))
        cancelled = press("Escape", "escape")
        self.assertEqual(view(cancelled), view(removed))
        press("q", "drink")
        consumed = press("Return", "enter", wait_for_ready=True, wait_for_admission=True)
        potions = [item for item in view(consumed)["inventory"] if item["class"] == "potion"]
        self.assertEqual(sorted(int(item["quantity"]) for item in potions), [1, 2])
        self.assertEqual(int(view(consumed)["tick"]), 700)
        self.assertEqual(consumed["messages"]["turn"].count("You finish drinking the red potion."), 1)
        first_potion = potions[0]["id"]
        press("q", "drink")
        final_unit = press("Return", "enter", wait_for_ready=True, wait_for_admission=True)
        self.assertFalse(any(item["id"] == first_potion for item in view(final_unit)["inventory"]))
        self.assertEqual(final_unit["messages"]["turn"].count("You finish drinking the red potion."), 1)
        pixels = read_capture(capture).split(b"\n", 3)[3]
        self.assertEqual(len(pixels), 1200 * 800 * 3)
        self.assertGreater(len(set(pixels)), 8)


if __name__ == "__main__":
    unittest.main()
