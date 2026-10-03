"""The ordinary adventure interface through real server and text processes."""
import unittest

from process_harness import ProcessTestCase, SPECTATOR_TOKEN


class AdventureProcesses(ProcessTestCase):
    def test_ordinary_prose_examination_directional_travel_pickup_and_restart(self):
        server = self.server()
        player, welcome = self.adventure()
        self.assertIn("chamber of stone", welcome)
        self.assertIn("An open wooden door leads east.", welcome)
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
        # The tablet is in the next room, so arriving there describes it.
        fetched = self.say(player, "get tablet")
        self.assertTrue(fetched.startswith("You walk over to the stone tablet and pick it up.\nHidden Promise\nYou are in "), fetched)
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
        self.assertIn("A copper token lies on the floor nearby; a stone tablet lies to the east.", description)
        self.assertIn("An open wooden door leads east.", description)
        self.assertNotIn("west", description)
        self.assertEqual("You can't see a way north.\n> ", self.say(player, "north"))
        self.assertEqual("You walk over to the copper token and pick it up.\n> ", self.say(player, "get token"))
        help_text = self.say(player, "help")
        for extra in ("wizard", "control", "step", "sync", "history", "Ready."):
            self.assertNotIn(extra, help_text)

    def test_a_turn_ends_before_the_next_command_and_spectators_cannot_act(self):
        self.server()
        player, _ = self.adventure()
        spectator, _ = self.adventure(SPECTATOR_TOKEN)
        self.flush_save()
        before = self.save.read_bytes()
        self.assertIn("read-only", self.say(spectator, "take tablet"))
        self.assertEqual(self.save.read_bytes(), before)
        # Typed ahead, the next command waits for the whole turn.
        self.send(player, "take tablet")
        self.send(player, "inventory")
        told = player.until(lambda line: "You are carrying" in line)
        taken = told.index("You walk over to the stone tablet and pick it up.")
        self.assertLess(taken, told.index("You are carrying a stone tablet."))
        # The spectator is told what the player did.
        spectator.until(lambda line: "pick up a stone tablet" in line)
        observer, initial = self.client(SPECTATOR_TOKEN)
        self.assertEqual(initial["travel"]["phase"], "arrived")

    def test_directional_interruption_is_one_narrative_response(self):
        self.server(scenario="text-adventure-hazard")
        player, _ = self.adventure()
        self.assertEqual(
            "You set off east. A figure comes into view to the east, and you stop warily.\n> ",
            self.say(player, "go east"),
        )

    def test_rotated_approach_and_stairs_use_ordinary_backend_routes(self):
        self.server(scenario="portal-geometry")
        player, welcome = self.adventure()
        self.assertIn("chamber of stone", welcome)
        self.assertIn("made of stone", self.say(player, "examine walls"))
        fetched = self.say(player, "get tablet")
        self.assertTrue(fetched.startswith("You walk over to the stone tablet and pick it up.\n"), fetched)
        # The first token is told plainly, a second one as another.
        self.assertIn("A copper token lies on the floor nearby; another copper token lies to the south.", fetched)
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
                self.assertIn("figure comes into view to the east", interrupted)
                self.assertIn("picking it up", interrupted)
                self.assertNotIn("and pick it up", interrupted)
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
        question = self.say(player, "take thing")
        self.assertIn("Which do you mean, the copper token or the stone tablet?", question)
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
        self.assertIn("You feel fine.", self.say(player, "diagnose"))
        # Read
        self.assertIn("worn spiral", self.say(player, "read copper token"))
        self.assertIn("There's nothing written on the floor.", self.say(player, "read floor"))
        # Command repetition with again and g
        self.assertIn("Time passes.", self.say(player, "wait"))
        self.assertIn("Time passes.", self.say(player, "again"))
        self.assertIn("Time passes.", self.say(player, "g"))
        # Ditransitive put on floor
        self.assertIn("You pick up the copper token.", self.say(player, "take copper token"))
        self.assertIn("You drop the copper token.", self.say(player, "put copper token on floor"))
        # What the game can't do yet is said plainly.
        self.assertIn("You can't talk with anyone yet.", self.say(player, "talk to myself"))

    def test_conversational_clarification_and_pronouns_in_real_process(self):
        self.server(wizard=True, scenario="text-adventure-clarification")
        player, _ = self.adventure()
        # 1. Things that can be told apart need a choice; asking takes no time.
        question = self.say(player, "take thing")
        self.assertIn("Which do you mean, the copper token or the stone tablet?", question)
        # 2. An answer that fits nothing asks again without forgetting.
        invalid = self.say(player, "gold")
        self.assertIn("Please choose the copper token or the stone tablet", invalid)
        # 3. An ordinal answers it.
        self.assertIn("You pick up the copper token.", self.say(player, "the first one"))
        # 4. Alike things need no choice: either copper token will do.
        self.assertIn("You pick up the copper token.", self.say(player, "take token"))
        # 5. A plural names them all, and "them" refers back to them.
        self.assertIn("You drop the two copper tokens.", self.say(player, "drop tokens"))
        self.assertIn("You pick up the two copper tokens.", self.say(player, "take them"))
        # 6. "It" is the last thing mentioned.
        self.say(player, "examine tablet")
        self.assertIn("You walk over to the stone tablet and pick it up.", self.say(player, "take it"))

    def test_session_commands_and_sensory_in_real_process(self):
        self.server()
        player, _ = self.adventure()
        # Sensory inspection
        # The place's own sound and smell, the same each time.
        listen = self.say(player, "listen")
        self.assertTrue(listen.endswith(".\n> ") and "can't" not in listen, listen)
        self.assertEqual(listen, self.say(player, "listen"))
        smell = self.say(player, "smell")
        self.assertTrue(smell.endswith(".\n> ") and "can't" not in smell, smell)
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
        self.assertIn("chamber of stone", welcome)
        self.assertIn("An open wooden door leads east.", welcome)
        # Directional navigation into Region 2 (Broken gallery)
        walk_east = self.say(player, "east")
        self.assertIn("You walk east.", walk_east)
        # In Region 2, Broken gallery is presented and includes an exit back west
        self.assertIn("An open wooden door leads west.", walk_east)
        # Moving back west through portal
        walk_west = self.say(player, "west")
        self.assertIn("You walk west.", walk_west)

    def test_a_fight_is_told_blow_by_blow_and_the_corpse_can_be_taken(self):
        # Regression: after a fight, "take corpse" walked over and then gave
        # up because the character was still recovering.
        self.server(scenario="first-dungeon")
        player, _ = self.adventure()
        self.assertEqual(
            "You set off east. A ruin scout comes into view to the east, and you stop warily.\n> ",
            self.say(player, "east"),
        )
        fight = []
        for _ in range(12):
            fight.append(self.say(player, "attack scout"))
            if "falls dead" in fight[-1]:
                break
        told = "".join(fight)
        self.assertIn("falls dead", told)
        for mechanic in ("tick", "must wait", "act again", "preparation", "Recovering"):
            self.assertNotIn(mechanic, told)
        # Each turn is one passage ending with the prompt.
        for passage in fight:
            self.assertTrue(passage.endswith("> "), passage)
            self.assertEqual(passage.count("> "), 1, passage)
        # The corpse lies in the next room's doorway, so that room follows.
        taken = self.say(player, "take corpse")
        self.assertTrue(taken.startswith("You walk over to the ruin scout corpse and pick it up.\n"), taken)
        self.assertIn("ruin scout corpse", self.say(player, "inventory"))

    def test_rooms_are_described_from_their_extent_and_doors_are_their_ways(self):
        self.server()
        player, welcome = self.adventure()
        self.assertRegex(welcome, r"You are in a small, \w+ chamber of stone\. \S")
        self.assertNotIn("Stone Hall", welcome)
        # Atmosphere is fixed for the place.
        room = next(line for line in welcome.split("\n") if line.startswith("You are in"))
        self.assertIn(room, self.say(player, "look"))
        # Through the open door, into the other room, and back.
        arrived = self.say(player, "east")
        self.assertIn("You walk east.", arrived)
        self.assertIn("An open wooden door leads west.", arrived)
        self.assertIn("You walk west.", self.say(player, "west"))
        self.assertIn("You walk over to the wooden door and close it.", self.say(player, "close door"))
        self.assertEqual("The wooden door to the east is closed.\n> ", self.say(player, "east"))

    def test_places_are_told_in_prose_and_named_briefly_when_seen_before(self):
        self.server()
        player, welcome = self.adventure()
        self.assertIn("A copper token lies at your feet; a stone tablet lies to the east.", welcome)
        for listing in ("You see", "You can head"):
            self.assertNotIn(listing, welcome)
        room = next(line for line in welcome.split("\n") if line.startswith("You are in"))
        there = self.say(player, "east")
        self.assertTrue(there.startswith("You walk east.\nHidden Promise\nYou are in "), there)
        self.assertIn("An open wooden door leads west.", there)
        # Back in a place already described: its name, ways and contents.
        back = self.say(player, "west")
        self.assertEqual(
            "You walk west.\nHollow Promise\nAn open wooden door leads east.\n"
            "A copper token lies on the floor nearby; a stone tablet lies to the east.\n> ",
            back,
        )
        # Looking describes it in full, with the same atmosphere as before.
        self.assertIn(room, self.say(player, "look"))
        self.assertIn("every time", self.say(player, "verbose"))
        self.say(player, "east")
        self.assertIn(room, self.say(player, "west"))
        # A remembered place can be gone back to by name.
        self.say(player, "brief")
        self.say(player, "east")
        back = self.say(player, "go to hollow promise")
        self.assertTrue(back.startswith("You make your way back to Hollow Promise.\nHollow Promise\nAn open wooden door leads east."), back)
        self.assertEqual("You're already in Hollow Promise.\n> ", self.say(player, "go to Hollow Promise"))

    def test_directions_cross_open_ground_without_walls(self):
        self.server(scenario="streaming-corridor")
        player, welcome = self.adventure()
        self.assertIn("You can head off in any direction.", welcome)
        # The same open place throughout: no new description on arrival.
        self.assertEqual("You walk east.\n> ", self.say(player, "east"))
        self.assertEqual("You walk west.\n> ", self.say(player, "west"))

    def test_a_journey_goes_round_a_doorway_corner_a_body_cannot_cut(self):
        # Regression: from beside the wall, the route cut the doorway's corner
        # diagonally, which a two-cell body may not, and stopped "blocked".
        self.server(scenario="first-dungeon")
        player, _ = self.adventure()
        for step in ("step ne", "step e", "step e"):
            self.say(player, step)
        self.assertEqual(
            "You set off east. A ruin scout comes into view to the east, and you stop warily.\n> ",
            self.say(player, "east"),
        )


if __name__ == "__main__":
    unittest.main()

