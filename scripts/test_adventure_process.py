"""The ordinary adventure interface through real server and text processes."""
from test_text_process import flush_save
import json
from pathlib import Path
import unittest

import test_text_process as support
import test_headless_process as headless


class AdventureProcess(support.Process):
    def _read(self):
        # The interactive prompt is flushed without a newline. Read characters
        # so acceptance tests verify its actual bytes instead of requiring the
        # application to put the cursor on another line for the test harness.
        pending = ""
        try:
            while character := self.child.stdout.read(1):
                pending += character
                if pending == "> " or character == "\n":
                    self.lines.put(pending.removesuffix("\n"))
                    pending = ""
            if pending:
                self.lines.put(pending)
        finally:
            self.lines.put(None)


class AdventureProcesses(unittest.TestCase):
    setUpClass = classmethod(support.TextProcesses.setUpClass.__func__)
    launch = support.TextProcesses.launch
    setUp = headless.HeadlessProcesses.setUp
    server = headless.HeadlessProcesses.server
    client = headless.HeadlessProcesses.client
    frame = headless.HeadlessProcesses.frame
    request = headless.HeadlessProcesses.request
    command = headless.HeadlessProcesses.command

    def adventure(self, token=support.TOKEN):
        process = AdventureProcess(self.bin / ("tor-client-text" + self.suffix), ["--connect", self.address], token=token)
        self.addCleanup(process.stop)
        return process, process.until(lambda line: line == "> ")

    def say(self, process, text):
        process.child.stdin.write(text + "\n")
        process.child.stdin.flush()
        return process.until(lambda line: line == "> ")

    def send(self, process, text):
        process.child.stdin.write(text + "\n")
        process.child.stdin.flush()

    def wizard(self, fixture, section="setup"):
        wizard = self.launch("tor-client-text", ["--connect", self.address], token=headless.WIZARD_TOKEN)
        wizard.until(lambda line: line == "Ready.")
        scenario = json.loads((Path(__file__).parent / "scenarios" / fixture).read_text())
        for operation in scenario[section]:
            self.assertNotIn("Server error", wizard.command(operation))
        wizard.command("release")
        return wizard

    def test_ordinary_prose_examination_directional_travel_pickup_and_restart(self):
        server = self.server()
        player, welcome = self.adventure()
        self.assertIn("stone floor", welcome)
        self.assertIn("You can head east.", welcome)
        self.assertNotIn("You are in control", welcome)
        self.assertNotIn("north", welcome)
        self.assertNotIn("south", welcome)
        self.assertNotIn("west", welcome)
        self.assertNotIn("Session tools", self.say(player, "help"))
        self.assertIn("copper token", welcome)
        for diagnostic in ("tick", "offset", "#1", "Branch:", "Ready."):
            self.assertNotIn(diagnostic, welcome)
        self.assertIn("worn spiral", self.say(player, "examine token"))
        self.assertEqual("You pick up the copper token.\n> ", self.say(player, "take it"))
        self.assertIn("You walk east.", self.say(player, "east"))
        self.assertEqual("You pick up the stone tablet.\n> ", self.say(player, "take tablet"))
        player.stop()
        server.stop()
        self.server()
        resumed, welcome = self.adventure()
        inventory = self.say(resumed, "inventory")
        self.assertIn("copper token", inventory)
        self.assertIn("stone tablet", inventory)
        self.assertNotIn("head toward", welcome)

    def test_approach_then_take_is_a_travel_request_and_ordinary_saved_actions(self):
        self.server()
        player, _ = self.adventure()
        self.assertEqual("You walk over to the stone tablet and pick it up.\n> ", self.say(player, "get tablet"))
        observer, initial = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(initial["state"]["observation"]["tick"], 750)
        self.assertEqual([i["name"] for i in initial["state"]["observation"]["inventory"]], ["stone tablet"])
        self.assertEqual(initial["travel"]["phase"], "arrived")
        self.assertEqual(initial["travel"]["completed_steps"], 7)
        kinds = [h["content"]["type"] for h in initial["history"]]
        self.assertEqual(kinds, ["travel"] + ["action"] * 8)

    def test_moving_within_the_place_does_not_add_exits_and_get_token_is_one_response(self):
        self.server()
        player, _ = self.adventure()
        for _ in range(3):
            self.say(player, "step east")
        description = self.say(player, "look")
        self.assertIn("You see a copper token on the floor nearby.", description)
        self.assertIn("You see a stone tablet to the east.", description)
        self.assertIn("You can head east.", description)
        self.assertNotIn("You can head west", description)
        self.assertEqual("You can't see a way north.\n> ", self.say(player, "north"))
        self.assertEqual("You walk over to the copper token and pick it up.\n> ", self.say(player, "get token"))
        help_text = self.say(player, "help")
        for extra in ("wizard", "control", "step", "sync", "history", "Ready."):
            self.assertNotIn(extra, help_text)

    def test_stop_discards_pickup_and_spectator_cannot_start_or_stop_travel(self):
        self.server()
        player, _ = self.adventure()
        spectator, _ = self.adventure(support.SPECTATOR_TOKEN)
        flush_save(self)
        before = self.save.read_bytes()
        self.assertIn("read-only", self.say(spectator, "take tablet"))
        self.assertIn("read-only", self.say(spectator, "stop"))
        self.assertEqual(self.save.read_bytes(), before)
        self.send(player, "take tablet")
        self.assertIn("stop", self.say(player, "stop"))
        observer, initial = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(initial["travel"]["phase"], "cancelled")
        self.assertEqual(initial["state"]["observation"]["inventory"], [])
        self.assertLess(initial["travel"]["completed_steps"], 7)
        self.assertIn("empty-handed", self.say(player, "inventory"))

    def test_directional_interruption_is_one_narrative_response(self):
        self.server(wizard=True)
        self.wizard("text-adventure.json", "hazard")
        player, _ = self.adventure()
        self.assertEqual("You start walking east, but stop when a figure comes into view.\n> ", self.say(player, "go east"))

    def test_rotated_approach_and_stairs_use_ordinary_backend_routes(self):
        self.server(wizard=True)
        self.wizard("portal-geometry.json")
        player, welcome = self.adventure()
        self.assertIn("walls of stone", welcome)
        self.assertIn("made of stone", self.say(player, "examine walls"))
        self.assertEqual("You walk over to the stone tablet and pick it up.\n> ", self.say(player, "get tablet"))
        self.assertIn("You walk down.", self.say(player, "down"))
        self.assertIn("You walk up.", self.say(player, "up"))
        observer, state = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(state["state"]["observation"]["tick"], 550)
        self.assertEqual([i["name"] for i in state["state"]["observation"]["inventory"]], ["stone tablet"])
        for hidden in ("Upper gallery", "offset", "region", "quarter_turns"):
            self.assertNotIn(hidden, "\n".join(player.transcript))

    def test_wide_join_direction_then_approach_works_without_region_names(self):
        self.server(wizard=True)
        self.wizard("wide-join.json")
        player, _ = self.adventure()
        self.assertIn("You walk east.", self.say(player, "east"))
        self.assertEqual("You walk over to the stone tablet and pick it up.\n> ", self.say(player, "get tablet"))
        _, state = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(state["state"]["observation"]["tick"], 650)
        self.assertNotIn("East space", "\n".join(player.transcript))

    def test_new_actor_interrupts_and_arrival_does_not_override_pickup_caution(self):
        # Each variant gets a separate save/server via a subtest-owned test instance.
        for section, expected_phase in (("hazard", "hazard"), ("arrival_hazard", "arrived")):
            with self.subTest(section=section):
                self.setUp()
                server = self.server(wizard=True)
                wizard = self.wizard("text-adventure.json", section)
                player, welcome = self.adventure()
                self.assertNotIn("figure", welcome)
                interrupted = self.say(player, "take tablet")
                self.assertIn("stop when a figure comes into view", interrupted)
                self.assertNotIn("You arrive", interrupted)
                self.assertNotIn("pick it up", interrupted)
                observer, state = self.client(support.SPECTATOR_TOKEN)
                self.assertEqual(state["travel"]["phase"], expected_phase)
                self.assertEqual(state["travel"]["completed_steps"], 1)
                self.assertEqual(state["state"]["observation"]["inventory"], [])
                self.assertIn("unremarkable figure", self.say(player, "examine figure"))
                for process in (player, wizard, observer, server): process.stop()

    def test_clarification_is_free_and_wizard_rewind_clears_pending_pickup(self):
        self.server(wizard=True)
        wizard = self.wizard("text-adventure.json", "clarification")
        player, _ = self.adventure()
        flush_save(self)
        before = self.save.read_bytes()
        question = self.say(player, "take token")
        self.assertIn("Which do you mean?", question)
        self.assertNotIn("#", question)
        self.assertEqual(before, self.save.read_bytes())
        self.assertIn("You pick up the copper token", self.say(player, "1"))
        self.send(player, "take tablet")
        # Travel advances the attached actor while this separate wizard connection
        # receives updates. A stale command is a free rejection, not a rewind.
        for _ in range(5):
            wizard.command("sync")
            result = wizard.command("wizard rewind initial")
            if "StaleRevision" not in result:
                break
        self.assertNotIn("Server error", result)
        observer, state = self.client(support.SPECTATOR_TOKEN)
        self.assertEqual(state["state"]["observation"]["tick"], 0)
        self.assertIsNone(state["travel"])
        self.assertEqual(state["state"]["observation"]["inventory"], [])
        # Fresh attachment cannot inherit an old client's compound intent.
        player.stop()
        resumed, _ = self.adventure()
        self.assertIn("empty-handed", self.say(resumed, "inventory"))


if __name__ == "__main__":
    unittest.main()
