"""The ordinary adventure interface through real server and text processes."""
import unittest

from process_harness import ProcessTestCase, SPECTATOR_TOKEN


class AdventureProcesses(ProcessTestCase):
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
        observer, initial = self.client(SPECTATOR_TOKEN)
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

    def test_stop_skips_the_journey_and_discards_pickup_and_spectator_cannot_travel(self):
        self.server()
        player, _ = self.adventure()
        spectator, _ = self.adventure(SPECTATOR_TOKEN)
        self.flush_save()
        before = self.save.read_bytes()
        self.assertIn("read-only", self.say(spectator, "take tablet"))
        self.assertIn("read-only", self.say(spectator, "stop"))
        self.assertEqual(self.save.read_bytes(), before)
        self.assertIn("1000 ms", self.say(player, "pace 1000"))
        self.send(player, "take tablet")
        stopped = self.say(player, "stop")
        self.assertIn("can't stop partway", stopped)
        self.assertNotIn("pick it up", stopped)
        observer, initial = self.client(SPECTATOR_TOKEN)
        self.assertEqual(initial["travel"]["phase"], "arrived")
        self.assertEqual(initial["state"]["observation"]["inventory"], [])
        self.assertIn("empty-handed", self.say(player, "inventory"))

    def test_directional_interruption_is_one_narrative_response(self):
        self.server(scenario="text-adventure-hazard")
        player, _ = self.adventure()
        self.assertEqual("You start walking east, but stop when a figure comes into view.\n> ", self.say(player, "go east"))

    def test_rotated_approach_and_stairs_use_ordinary_backend_routes(self):
        self.server(scenario="portal-geometry")
        player, welcome = self.adventure()
        self.assertIn("walls of stone", welcome)
        self.assertIn("made of stone", self.say(player, "examine walls"))
        self.assertEqual("You walk over to the stone tablet and pick it up.\n> ", self.say(player, "get tablet"))
        self.assertIn("You walk down.", self.say(player, "down"))
        self.assertIn("You walk up.", self.say(player, "up"))
        observer, state = self.client(SPECTATOR_TOKEN)
        self.assertEqual(state["state"]["observation"]["tick"], 550)
        self.assertEqual([i["name"] for i in state["state"]["observation"]["inventory"]], ["stone tablet"])
        for hidden in ("Upper gallery", "offset", "region", "quarter_turns"):
            self.assertNotIn(hidden, "\n".join(player.transcript))

    def test_wide_join_direction_then_approach_works_without_region_names(self):
        self.server(scenario="wide-join")
        player, _ = self.adventure()
        self.assertIn("You walk east.", self.say(player, "east"))
        self.assertEqual("You walk over to the stone tablet and pick it up.\n> ", self.say(player, "get tablet"))
        _, state = self.client(SPECTATOR_TOKEN)
        self.assertEqual(state["state"]["observation"]["tick"], 650)
        self.assertNotIn("East space", "\n".join(player.transcript))

    def test_new_actor_interrupts_and_arrival_does_not_override_pickup_caution(self):
        # Each variant gets a separate save/server via a subtest-owned test instance.
        for package, expected_phase in (("text-adventure-hazard", "hazard"), ("text-adventure-arrival-hazard", "arrived")):
            with self.subTest(package=package):
                self.setUp()
                server = self.server(scenario=package)
                player, welcome = self.adventure()
                self.assertNotIn("figure", welcome)
                interrupted = self.say(player, "take tablet")
                self.assertIn("stop when a figure comes into view", interrupted)
                self.assertNotIn("You arrive", interrupted)
                self.assertNotIn("pick it up", interrupted)
                observer, state = self.client(SPECTATOR_TOKEN)
                self.assertEqual(state["travel"]["phase"], expected_phase)
                self.assertEqual(state["travel"]["completed_steps"], 1)
                self.assertEqual(state["state"]["observation"]["inventory"], [])
                self.assertIn("nothing special about the figure", self.say(player, "examine figure"))
                for process in (player, observer, server): process.stop()

    def test_clarification_is_free_and_wizard_rewind_clears_pending_pickup(self):
        self.server(wizard=True, scenario="text-adventure-clarification")
        wizard = self.wizard()
        player, _ = self.adventure()
        self.flush_save()
        before = self.save.read_bytes()
        question = self.say(player, "take token")
        self.assertIn("Which do you mean?", question)
        self.assertNotIn("#", question)
        self.assertEqual(before, self.save.read_bytes())
        self.assertIn("You pick up the copper token", self.say(player, "1"))
        self.send(player, "take tablet")
        # Travel advances the attached actor while the wizard rewinds; the
        # headless client resubmits a stale command at the disclosed revision.
        self.wizard_command(wizard, "rewind initial")
        observer, state = self.client(SPECTATOR_TOKEN)
        self.assertEqual(state["state"]["observation"]["tick"], 0)
        self.assertIsNone(state["travel"])
        self.assertEqual(state["state"]["observation"]["inventory"], [])
        # Fresh attachment cannot inherit an old client's compound intent.
        player.stop()
        resumed, _ = self.adventure()
        self.assertIn("empty-handed", self.say(resumed, "inventory"))

    def test_multi_command_sentence_chains_in_real_process(self):
        self.server()
        player, _ = self.adventure()
        response = self.say(player, "examine token. take it. east")
        self.assertIn("worn spiral", response)
        self.assertIn("You pick up the copper token.", response)
        self.assertIn("You walk east.", response)

    def test_expanded_if_interactions_in_real_process(self):
        self.server()
        player, _ = self.adventure()
        # Diagnose
        self.assertIn("good health", self.say(player, "diagnose"))
        # Read
        self.assertIn("worn spiral", self.say(player, "read copper token"))
        self.assertIn("There is nothing written there.", self.say(player, "read floor"))
        # Command repetition with again and g
        self.assertIn("Time passes.", self.say(player, "wait"))
        self.assertIn("Time passes.", self.say(player, "again"))
        self.assertIn("Time passes.", self.say(player, "g"))
        # Ditransitive put on floor
        self.assertIn("You pick up the copper token.", self.say(player, "take copper token"))
        self.assertIn("You drop 1 x copper token.", self.say(player, "put copper token on floor"))
        # Conversational interaction with self
        self.assertIn("madness", self.say(player, "talk to myself"))

    def test_conversational_clarification_and_pronouns_in_real_process(self):
        self.server(wizard=True, scenario="text-adventure-clarification")
        player, _ = self.adventure()
        # 1. Ambiguous noun triggers clarification question
        question = self.say(player, "take token")
        self.assertIn("Which do you mean?", question)
        # 2. Invalid option politely re-prompts without crashing or clearing choices
        invalid = self.say(player, "gold")
        self.assertIn("There is no matching option. Which do you mean?", invalid)
        # 3. Conversational natural language ordinal resolution
        first_pickup = self.say(player, "the first one")
        self.assertIn("You pick up the copper token.", first_pickup)
        # 4. Follow-up disambiguation resolved with ordinal / candidate number
        question2 = self.say(player, "take token")
        self.assertIn("Which do you mean?", question2)
        second_pickup = self.say(player, "2")
        self.assertIn("You pick up the copper token.", second_pickup)
        # 5. Plural pronoun 'them' drops the carried items
        drop_response = self.say(player, "drop them")
        self.assertIn("You drop 1 x copper token.", drop_response)
        # 6. Singular pronoun 'it' picks the dropped item back up
        take_response = self.say(player, "take it")
        self.assertIn("You pick up the copper token.", take_response)

    def test_session_commands_and_sensory_in_real_process(self):
        self.server()
        player, _ = self.adventure()
        # Sensory inspection
        listen = self.say(player, "listen")
        self.assertTrue(any(word in listen.lower() for word in ("silence", "quiet", "sound", "hum", "hear")))
        smell = self.say(player, "smell")
        self.assertTrue(any(word in smell.lower() for word in ("scent", "air", "smell", "dust", "stone", "damp")))
        # Architectural examination
        self.assertIn("walls are made of stone", self.say(player, "examine walls"))
        self.assertIn("floor is made of stone", self.say(player, "examine floor"))
        # Places command: shows visible anchors
        places_out = self.say(player, "places")
        self.assertIn("1. Hollow Promise (in sight)", places_out)
        # Name place by index
        self.say(player, "name 1 Vault of Whispers")
        self.assertIn("1. Vault of Whispers (in sight)", self.say(player, "places"))
        # Note command
        self.say(player, "note The shadows gather near the portal")
        _, state = self.client(SPECTATOR_TOKEN)
        # Verify the note was recorded authoritatively
        kinds = [h["content"]["type"] for h in state["history"]]
        self.assertIn("annotation", kinds)
        # Save command executes clean barrier request
        self.assertEqual("> ", self.say(player, "save"))

    def test_first_dungeon_room_1_presentation_and_exit_travel(self):
        self.server()
        player, welcome = self.adventure()
        # Volumetric actor unification: observer's own multi-cell body is omitted from room entity listings
        self.assertNotIn("You see yourself above you", welcome)
        self.assertNotIn("You see yourself at your feet", welcome)
        self.assertNotIn("delver", welcome)
        # Geometry-derived exits & spatial synthesis
        self.assertIn("stone floor", welcome)
        self.assertIn("You can head east.", welcome)
        # Directional navigation into Region 2 (Broken gallery)
        walk_east = self.say(player, "east")
        self.assertIn("You walk east.", walk_east)
        # In Region 2, Broken gallery is presented and includes an exit back west
        self.assertIn("You can head", walk_east)
        # Moving back west through portal
        walk_west = self.say(player, "west")
        self.assertIn("You walk west.", walk_west)


if __name__ == "__main__":
    unittest.main()

