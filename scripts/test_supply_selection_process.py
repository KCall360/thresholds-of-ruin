"""Ordinary text/native finite supplies chosen from disclosed exact sources."""
from process_harness import ProcessTestCase
from supply_package import prepare


def observation(frame):
    return frame["state"]["observation"]


def bandage(frame):
    return next((row for row in observation(frame)["ground_items"]
                 if row["item"]["name"] == "clean bandage"), None)


def completed(frame):
    value = observation(frame)
    return (int(value["tick"]) > 0 and value["ready"]
            and value["combat"]["preparation_remaining"] is None)


class SupplySelectionProcesses(ProcessTestCase):
    def test_scripted_and_adventure_exact_ground_selection_spends_once(self):
        for adventure in [False, True]:
            with self.subTest(adventure=adventure):
                self.save = self.directory / f"text-supply-{adventure}.db"
                package = prepare(self, bandages=1 if adventure else 2, autonomous=True,
                                  isolate_partner=True)
                server = self.server(scenario=package)
                observer, initial = self.client(observe=True)
                source = bandage(initial)
                self.assertFalse(source["reachable"])
                self.assertTrue(source["supply_reachable"])
                item = source["item"]["id"]
                if adventure:
                    text, _ = self.adventure()
                    self.assertIn("needs 1 Bandage", self.say(text, "stats"))
                    output = self.say(text, "bind wounds me using clean bandage")
                    self.assertNotIn("Use Bind Wounds anyway?", output)
                    self.assertIn("You prepare to use Bind Wounds.", output)
                else:
                    text, _ = self.text_client()
                    self.assertIn("needs 1 Bandage", text.command("stats"))
                    output = text.command(f"bind wounds me using #{item}")
                self.assertNotIn("Server error", output)
                current = self.request(observer, {"type": "snapshot"})
                current = current if completed(current) else self.frame(observer, completed)
                self.assertFalse(current["state"]["wizard_game"])
                remaining = bandage(current)
                self.assertEqual(None if remaining is None else int(remaining["item"]["quantity"]),
                                 None if adventure else 1)
                invoked = next(row["content"]["action"] for row in current["history"]
                               if row["content"].get("action", {}).get("id") == "bind_wounds")
                self.assertEqual(invoked["supplies"], {"kind": "explicit",
                    "items": [{"cost_ordinal": 0, "item": item}]})
                ability = next(row for row in observation(current)["combat"]["own_stats"]["abilities"]
                               if row["id"] == "bind_wounds")
                self.assertGreater(int(ability["cooldown_remaining"]), 0)
                shown = self.say(text, "stats") if adventure else text.command("stats")
                self.assertIn(f"cooldown: {ability['cooldown_remaining']} active ticks", shown)
                self.assertDisclosedStateEqual(self.request(observer, {"type": "snapshot"})["state"], current["state"])
                text.stop(); observer.stop(); server.stop()


class SupplySelectionNativeProcesses(ProcessTestCase):
    graphical = True

    def test_native_exact_supply_picker_preserves_tick_until_confirmation(self):
        self.server(scenario=prepare(self, autonomous=True, isolate_partner=True))
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        initial = self.ascii_frame(window, lambda frame: frame["has_control"] and not frame["busy"])
        observer, _ = self.client(observe=True)
        item = bandage(initial)["item"]["id"]
        native = self.native_keys(window)

        def press(name, reported):
            return self.native_press(window, native, name, reported)

        menu = press("z", "abilities")
        self.assertEqual([row["id"] for row in menu["ability_choices"]], ["bind_wounds"])
        press("Return", "enter")
        press("Up", "up")
        supplies = press("Return", "enter")
        self.assertIsNotNone(supplies["supply_selection"])
        self.assertIn("Automatic carried inventory", " ".join(supplies["supply_rows"]))
        self.assertIn("clean bandage", " ".join(supplies["supply_rows"]))
        self.assertEqual(supplies["state"], initial["state"])
        press("Down", "down")
        selected = press("space", "wait")
        self.assertEqual(selected["supply_selection"]["checked"], [item])
        self.assertEqual(selected["state"], initial["state"])
        accepted = press("Return", "enter")
        current = accepted if completed(accepted) else self.ascii_frame(window, completed)
        self.assertTrue(current["window_open"])
        self.assertFalse(current["state"]["wizard_game"])
        self.assertIsNone(bandage(current))
        self.assertDisclosedStateEqual(self.request(observer, {"type": "snapshot"})["state"], current["state"])
        press("z", "abilities"); press("Return", "enter"); press("Up", "up")
        shown = press("Return", "enter")
        self.assertIn("active ticks", " ".join(shown["supply_rows"]))
        self.assertEqual(shown["supply_selection"]["candidates"], [])
        self.assertEqual(shown["state"], current["state"])
        cancelled = press("Escape", "escape")
        self.assertIsNone(cancelled["supply_selection"])
        self.assertEqual(cancelled["state"], current["state"])
