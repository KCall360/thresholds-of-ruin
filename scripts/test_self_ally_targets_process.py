"""Self/ally target selection through ordinary compiler, server and real clients."""
import shutil
import subprocess

from process_harness import ProcessTestCase, SPECTATOR_TOKEN
from self_ally_package import self_ally_package


def observation(frame):
    return frame["state"]["observation"]


def completed_after(tick):
    def completed(frame):
        current = observation(frame)
        return (int(current["tick"]) > tick and current["ready"]
                and current["combat"]["preparation_remaining"] is None)
    return completed


class SelfAllyTargetProcesses(ProcessTestCase):
    def test_headless_scoped_self_allies_neutrals_and_restart(self):
        package = self_ally_package(self)
        portable = self.directory / "portable-care"
        exported = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "export", package, portable],
                                  capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(exported.returncode, 0, exported.stderr)
        server = self.server(scenario=package)
        player, initial = self.client()
        own = observation(initial)["self_target"]
        actors = {actor["name"]: actor["id"] for actor in observation(initial)["visible_actors"]}
        ally, neutral = actors["practice target"], actors["neutral stranger"]
        facts = {actor["actor"]: actor for actor in observation(initial)["combat"]["actors"]}
        self.assertTrue(facts[ally]["allied"])
        self.assertFalse(facts[neutral]["allied"])
        self.assertFalse(facts[neutral]["hostile"])
        self.assertNotIn("faction", str(observation(initial)))
        for target in [neutral, "a_" + "f" * 64]:
            before = self.request(player, {"type": "snapshot"})
            denied = self.act(player, {"type": "use_ability", "id": "mend", "invocation": {"kind": "actor_target", "target": target}, "supplies": {"kind": "carried"}})
            self.assertIsNotNone(denied["error"])
            self.assertEqual(denied["state"], before["state"])
            self.assertEqual(denied["history"], before["history"])
        current = initial
        for target in [own, ally]:
            tick = int(observation(current)["tick"])
            accepted = self.act(player, {"type": "use_ability", "id": "mend", "invocation": {"kind": "actor_target", "target": target}, "supplies": {"kind": "carried"}})
            self.assertIsNone(accepted["error"])
            current = accepted if completed_after(tick)(accepted) else self.frame(player, completed_after(tick))
            self.assertFalse(current["state"]["wizard_game"])
            self.assertTrue(any(entry["content"].get("action", {}).get("invocation") == {"kind": "actor_target", "target": target}
                                and entry["content"]["action"].get("id") == "mend"
                                for entry in current["history"]))
        spectator, watched = self.client(SPECTATOR_TOKEN)
        self.assertEqual(watched["state"], current["state"])
        denied = self.act(spectator, {"type": "use_ability", "id": "mend", "invocation": {"kind": "actor_target", "target": own}, "supplies": {"kind": "carried"}})
        self.assertEqual(denied["error"], "Spectator access is read-only")
        saved = self.request(player, {"type": "save"})
        self.assertIsNone(saved["error"])
        player.stop(); spectator.stop(); server.stop()
        shutil.rmtree(package)
        self.server(scenario=portable, seed=None)
        _, restored = self.client()
        self.assertEqual(restored["state"], saved["state"])

    def test_both_text_modes_select_self_and_ally_but_refuse_neutral(self):
        self.server(scenario=self_ally_package(self))
        observer, initial = self.client(observe=True)
        text, _ = self.text_client()
        own = observation(initial)["self_target"]
        for command in ["mend", "mend me", f"mend #{own}", "mend practice target"]:
            before = self.request(observer, {"type": "snapshot"})
            output = text.command(command)
            self.assertNotIn("Server error", output)
            tick = int(observation(before)["tick"])
            current = self.request(observer, {"type": "snapshot"})
            if not completed_after(tick)(current):
                current = self.frame(observer, completed_after(tick))
            self.assertFalse(current["state"]["wizard_game"])
        before = self.request(observer, {"type": "snapshot"})
        self.assertIn("No matching actor", text.command("mend neutral stranger"))
        self.assertEqual(self.request(observer, {"type": "snapshot"})["state"], before["state"])
        text.command("release"); text.stop()
        adventure, _ = self.adventure()
        for command in ["mend me", "mend practice target"]:
            before = self.request(observer, {"type": "snapshot"})
            output = self.say(adventure, command)
            # The ability's name alone does not disclose a healing mechanic.
            self.assertNotIn("Use Mend anyway?", output)
            self.assertIn("You prepare to use Mend.", output)
            tick = int(observation(before)["tick"])
            current = self.request(observer, {"type": "snapshot"})
            if not completed_after(tick)(current):
                self.frame(observer, completed_after(tick))
        before = self.request(observer, {"type": "snapshot"})
        self.assertIn("not a valid visible target", self.say(adventure, "mend neutral stranger"))
        self.assertEqual(self.request(observer, {"type": "snapshot"})["state"], before["state"])
        self.assertEqual(observation(initial)["tick"], "0")


class SelfAllyTargetNativeProcesses(ProcessTestCase):
    graphical = True

    def test_native_supportive_menu_offers_self_and_ally_without_neutral(self):
        self.server(scenario=self_ally_package(self))
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        initial = self.ascii_frame(window, lambda f: f["has_control"] and not f["busy"])
        observer, _ = self.client(observe=True)
        native = self.native_keys(window)

        def press(name, reported):
            return self.native_press(window, native, name, reported)

        menu = press("z", "abilities")
        self.assertEqual([ability["id"] for ability in menu["ability_choices"]], ["mend"])
        targets = press("Return", "enter")
        self.assertIn("yourself", targets["status"])
        self.assertEqual(targets["state"], initial["state"])
        selected = press("Down", "down")
        self.assertIn("practice target", selected["status"])
        # An ignored direction cannot advance the target picker or submit an action.
        unchanged = press("Right", "right")
        self.assertEqual(unchanged["state"], initial["state"])
        selected = press("Down", "down")
        self.assertIn("practice target", selected["status"], "neutral must never be offered as ally")
        tick = int(observation(initial)["tick"])
        accepted = press("Return", "enter")
        completed = accepted if completed_after(tick)(accepted) else self.ascii_frame(window, completed_after(tick))
        self.assertTrue(completed["window_open"])
        self.assertFalse(completed["state"]["wizard_game"])
        self.assertEqual(completed["state"], self.request(observer, {"type": "snapshot"})["state"])
        tick = int(observation(completed)["tick"])
        press("z", "abilities"); press("Return", "enter"); press("Up", "up")
        accepted = press("Return", "enter")
        completed = accepted if completed_after(tick)(accepted) else self.ascii_frame(window, completed_after(tick))
        self.assertFalse(completed["state"]["wizard_game"])
        authoritative = self.request(observer, {"type": "snapshot"})
        self.assertEqual(completed["state"], authoritative["state"])
        self.assertTrue(any(entry["content"].get("action", {}).get("id") == "mend"
                            and entry["content"]["action"].get("invocation") == {"kind": "actor_target", "target": observation(initial)["self_target"]}
                            for entry in authoritative["history"]))
