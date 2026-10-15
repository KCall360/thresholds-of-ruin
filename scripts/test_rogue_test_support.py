"""Disclosed reach bounds for the production Rogue acceptance drivers."""
import unittest

from rogue_test_support import cardinal_melee_candidate, ground_sight_candidate


def actor(x, y, z=0):
    return {"position": {"x": x, "y": y, "z": z}}


class RogueReachChoices(unittest.TestCase):
    def test_cardinal_contact_does_not_assume_diagonal_corner_reach(self):
        for x, y in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            self.assertTrue(cardinal_melee_candidate(actor(x, y)))
        for point in ((1, 1, 0), (-1, 1, 0), (0, 0, 0), (2, 0, 0), (1, 0, 1)):
            self.assertFalse(cardinal_melee_candidate(actor(*point)))

    def test_sight_uses_manhattan_distance_from_the_authored_eye(self):
        self.assertTrue(ground_sight_candidate(actor(4, 1), 6))
        self.assertTrue(ground_sight_candidate(actor(-4, -1), 6))
        self.assertFalse(ground_sight_candidate(actor(4, 2), 6))
        self.assertFalse(ground_sight_candidate(actor(4, 4), 6))
        self.assertFalse(ground_sight_candidate(actor(0, 0, 1), 6))


class RogueCareChoices(unittest.TestCase):
    def action(self, foes, *, injured, contacted=()):
        from rogue_test_support import care_combat_action
        return care_combat_action(foes, injured=injured, contacted=contacted)

    def test_injured_player_approaches_diagonal_foe_instead_of_waiting(self):
        foe = {"id": "visible-snake", **actor(1, 1)}
        self.assertIsNone(self.action([foe], injured=True))

    def test_healthy_player_initiates_cardinal_contact_before_waiting_for_injury(self):
        from rogue_test_support import care_combat_action
        foe = {"id": "visible-snake", **actor(1, 0)}
        self.assertEqual(care_combat_action([foe], injured=False), {
            "type": "use_ability", "id": "basic_melee",
            "invocation": {"kind": "actor_target", "target": foe["id"]}})
        self.assertEqual(self.action([foe], injured=False, contacted={foe["id"]}),
                         {"type": "wait"})

    def test_healthy_player_approaches_distant_or_diagonal_foe_before_waiting(self):
        for position in ((3, 2), (1, 1)):
            with self.subTest(position=position):
                foe = {"id": "visible-snake", **actor(*position)}
                for contacted in (set(), {foe["id"]}):
                    self.assertIsNone(self.action([foe], injured=False, contacted=contacted))

    def test_prior_contact_with_a_gone_foe_does_not_make_a_new_foe_active(self):
        foe = {"id": "new-snake", **actor(1, 0)}
        self.assertEqual(self.action([foe], injured=False, contacted={"gone-emu"}), {
            "type": "use_ability", "id": "basic_melee",
            "invocation": {"kind": "actor_target", "target": foe["id"]}})

    def test_waits_for_an_existing_contact_before_provoking_another_foe(self):
        foes = [{"id": "new-snake", **actor(-1, 0)},
                {"id": "contacted-emu", **actor(1, 0)}]
        self.assertEqual(self.action(foes, injured=False, contacted={"contacted-emu"}),
                         {"type": "wait"})

    def test_injured_player_attacks_only_disclosed_cardinal_contact(self):
        foes = [{"id": "diagonal", **actor(1, 1)},
                {"id": "cardinal", **actor(0, 1)}]
        self.assertEqual(self.action(foes, injured=True), {
            "type": "use_ability", "id": "basic_melee",
            "invocation": {"kind": "actor_target", "target": "cardinal"}})

    def test_contact_choice_uses_public_geometry_not_view_or_opaque_id_order(self):
        for west_id, east_id in (("z-opaque", "a-opaque"), ("a-new", "z-new")):
            west = {"id": west_id, **actor(-1, 0)}
            east = {"id": east_id, **actor(1, 0)}
            for foes in ([east, west], [west, east]):
                for injured in (False, True):
                    with self.subTest(west=west_id, injured=injured, foes=foes):
                        action = self.action(foes, injured=injured)
                        self.assertEqual(action["invocation"]["target"], west_id)

    def test_no_visible_foe_keeps_exploration_available(self):
        for injured in (False, True):
            with self.subTest(injured=injured):
                self.assertIsNone(self.action([], injured=injured))


class RogueActorChoices(unittest.TestCase):
    def candidates(self, actors):
        from rogue_test_support import disclosed_actor_candidates
        return disclosed_actor_candidates(actors)

    def test_body_occurrences_are_deduplicated_with_ground_contact_preferred(self):
        feet = {"id": "emu", **actor(3, 2)}
        torso = {"id": "emu", **actor(3, 2, 1)}
        other = {"id": "snake", **actor(-1, 0)}
        for rows in ([torso, feet, other, feet], [other, feet, torso]):
            self.assertEqual(self.candidates(rows), [other, feet])

    def test_upper_body_only_foe_is_still_visible_but_not_a_melee_candidate(self):
        torso = {"id": "emu", **actor(1, 0, 1)}
        self.assertEqual(self.candidates([torso]), [torso])
        self.assertFalse(cardinal_melee_candidate(self.candidates([torso])[0]))

    def test_candidate_order_is_unchanged_when_opaque_ids_and_input_order_change(self):
        for west_id, east_id in (("z-opaque", "a-opaque"), ("a-new", "z-new")):
            west = {"id": west_id, **actor(-1, 0)}
            east = {"id": east_id, **actor(1, 0)}
            for rows in ([east, west], [west, east]):
                self.assertEqual(self.candidates(rows), [west, east])


def cell(key, x, y, *, z=0, wall=False, down=False):
    return {"key": key, "position": {"x": x, "y": y, "z": z},
            "wall": wall, "stairs_down": down, "stairs_up": False}


class DisclosedFloorRoutes(unittest.TestCase):
    def graph(self, *cells):
        from rogue_test_support import DisclosedFloorGraph
        graph = DisclosedFloorGraph()
        graph.observe({"observation": {"visible_cells": list(cells)}})
        return graph

    def test_revisit_routes_only_to_recorded_keys_and_skips_unrelated_branch(self):
        graph = self.graph(cell("stairs", 0, 0), cell("supply", 1, 0),
                           cell("combat", 2, 0), cell("unrelated", -1, 0))
        self.assertEqual(graph.route_through("stairs", {"supply", "combat"}),
                         [("supply", "east"), ("combat", "east")])
        self.assertEqual(graph.route_through("combat", {"combat"}), [])

    def test_observed_links_survive_viewpoint_changes_and_stair_round_trip(self):
        graph = self.graph(cell("combat", 0, 0), cell("middle", 1, 0))
        graph.observe({"observation": {"visible_cells": [
            cell("middle", -1, 0), cell("supply", 0, 0), cell("stairs", 1, 0, down=True)]}})
        self.assertEqual(graph.route_to_any("combat", graph.stairs_down),
                         [("middle", "east"), ("supply", "east"), ("stairs", "east")])
        # Re-entering from another floor changes relative origin, not opaque keys.
        graph.observe({"observation": {"visible_cells": [cell("stairs", 0, 0, down=True)]}})
        self.assertEqual(graph.route_through("stairs", {"combat", "supply"}),
                         [("supply", "west"), ("middle", "west"), ("combat", "west")])

    def test_walls_height_and_unseen_gaps_never_become_routes(self):
        graph = self.graph(cell("start", 0, 0), cell("wall", 1, 0, wall=True),
                           cell("gap", 2, 0), cell("upper", 0, 1, z=1))
        self.assertIsNone(graph.route_to_any("start", {"gap", "wall", "upper"}))
        with self.assertRaisesRegex(ValueError, "No disclosed path.*gap"):
            graph.route_through("start", {"gap"})
        with self.assertRaisesRegex(ValueError, "No disclosed path.*missing"):
            graph.route_through("start", {"missing"})

    def test_deterministic_cover_is_bounded_by_twice_the_disclosed_tree(self):
        graph = self.graph(cell("start", 0, 0), cell("west", -1, 0),
                           cell("east", 1, 0), cell("north", 0, -1), cell("south", 0, 1))
        required = {"south", "north", "east", "west"}
        route = graph.route_through("start", required)
        self.assertEqual(route, [("west", "west"), ("start", "east"),
                                 ("east", "east"), ("start", "west"),
                                 ("north", "north"), ("start", "south"),
                                 ("south", "south")])
        self.assertLessEqual(len(route), 2 * (len(graph.edges) - 1))
        self.assertTrue(required.issubset(key for key, _ in route))


if __name__ == "__main__":
    unittest.main()
