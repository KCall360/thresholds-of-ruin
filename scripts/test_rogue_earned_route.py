"""Pure disclosure, finite-policy and checkpoint regressions for earned routes."""
import copy
import json
import random
import unittest

from rogue_earned_route import Controller, ControllerError, fixed_cohort, first_purchase


def cell(key, x=0, y=0, *, wall=False, up=False, down=False, z=0):
    return {"key": key, "position": {"x": x, "y": y, "z": z}, "wall": wall,
            "stairs_up": up, "stairs_down": down}


def ability(identity, *, cost=None, cooldown=0, reach=None, supplies=()):
    return {"id": identity, "cost": cost, "cooldown_remaining": str(cooldown),
            "reach": reach or {"kind": "melee"}, "supply_costs": list(supplies)}


def item(identity, supply, quantity=1):
    return {"id": identity, "known_supply": {"id": supply} if supply else None,
            "quantity": str(quantity)}


def foe(identity, x, y, z=0, **extra):
    return {"id": identity, "position": {"x": x, "y": y, "z": z}, **extra}


def overview(character=1, hd=1, xp=0, spent=0):
    return {"points": {"earned": str(hd), "spent": "1", "unspent": str(hd - 1)}, "growth": {
        "highest_earned_tier": str(hd), "highest_applied_tier": str(hd), "earned_xp": str(xp),
        "spent_xp": str(spent), "held_xp": str(xp - spent),
        "restorable_hit_dice": "0", "restoration_blocker": None,
        "peak_threat": str(hd), "next_threshold": "100",
        "future": {"kind": "selected", "source": {"kind": "class", "class":
                   "warrior" if character == 1 else "mage"}}}}


def state(character=1, *, cells=None, foes=(), hp=100, maximum=100, hd=1,
          inventory=(), ground=(), hunger="nourished", abilities=(), resources=()):
    return {"wizard_game": False, "observation": {"tick": "0",
        "self_target": "actor:self", "ready": True,
        "visible_cells": cells if cells is not None else [cell("cell:home"), cell("cell:west", -1)],
        "visible_actors": list(foes), "inventory": list(inventory), "ground_items": list(ground),
        "interactions": {"inventory": [{"item": row["id"], "edible":
            (row.get("known_supply") or {}).get("id") == "ration"} for row in inventory]},
        "combat": {"hp": str(hp), "max_hp": str(maximum), "dead": hp == 0, "terminal": hp == 0,
            "actors": [{"actor": row["id"], "hostile": True} for row in foes],
            "preparation_remaining": None, "events": [], "own_stats": {
                "hit_dice": {"total": str(hd), "sources": [{"source_kind": "class", "source": {
                    "id": "warrior" if character == 1 else "mage"}, "count": str(hd)}]},
                "abilities": [ability("basic_melee"), *abilities],
                "resources": list(resources), "nutrition": {"kind": "required", "hunger": hunger}}}}}


def pool(identity="stamina", *, balance=10, available=10, maximum=10):
    return {"resource": {"id": identity}, "balance": balance,
            "available": available, "maximum": maximum}


def ground(identity, supply, x=0, y=0, *, reachable=True, quantity=1):
    return {"item": item(identity, supply, quantity), "position": {"x": x, "y": y, "z": 0},
            "reachable": reachable, "supply_reachable": reachable}


class EarnedRouteControllerTests(unittest.TestCase):
    def choose(self, current=None, *, character=1, controller=None, advancement=None):
        current = current if current is not None else state(character)
        controller = controller or Controller(character)
        controller.observe(current, advancement or overview(character))
        return controller, controller.choose(current)

    def test_health_threshold_uses_exact_current_wire_decimals(self):
        current = state(hp=2**53 + 1, maximum=4 * (2**53 + 1), foes=[foe("actor:f", 1, 0)])
        _, decision = self.choose(current)
        self.assertEqual(decision.phase, "retreat")
        for invalid in (1, True, "01", "-1", "+1", "１", str(2**64)):
            with self.subTest(value=invalid), self.assertRaises(ValueError):
                current = state()
                current["observation"]["combat"]["hp"] = invalid
                self.choose(current)

    def test_owner_overview_rejects_numeric_and_noncanonical_wide_scalars(self):
        for section, fields in (("points", ("earned", "spent", "unspent")),
                ("growth", ("highest_earned_tier", "highest_applied_tier", "restorable_hit_dice",
                            "peak_threat", "next_threshold", "earned_xp", "spent_xp", "held_xp"))):
            for field in fields:
                for invalid in (1, "01", "-1", str(2**64)):
                    with self.subTest(section=section, field=field, value=invalid), self.assertRaises(ValueError):
                        advance = overview()
                        advance[section][field] = invalid
                        self.choose(advancement=advance)

    def test_injury_uses_numeric_order_when_health_decimal_text_sorts_later(self):
        current = state(hp=9, maximum=10, inventory=[item("item:b", "bandage")],
                        abilities=[ability("bind_wounds")])
        _, decision = self.choose(current)
        self.assertEqual(decision.action["id"], "bind_wounds")

    def test_fixed_cohort_keeps_all_eight_original_unrun_slots_in_order(self):
        rows = fixed_cohort()
        self.assertEqual([(r["arm"], r["actor"], r["seed"]) for r in rows],
            [(arm, actor, seed) for arm in ("production", "candidate") for actor in (1, 2) for seed in (0, 1)])
        self.assertTrue(all(r["evidence_status"] == "unrun" and r["outcome"] is None for r in rows))
        rows[0]["evidence_status"] = "failed"
        self.assertEqual(fixed_cohort()[0]["evidence_status"], "unrun")

    def test_first_purchases_keep_actual_actor_identity_and_fixed_cold_form(self):
        self.assertEqual(first_purchase(1), "purchase 1 advancement talent rattling_blow")
        self.assertEqual(first_purchase(2), "purchase 1 advancement talent evocation_initiate form spark_lance_cold")
        with self.assertRaises(ValueError):
            first_purchase(3)

    def test_observation_and_owner_pages_are_never_mutated(self):
        current, advance = state(), overview()
        originals = copy.deepcopy((current, advance))
        self.choose(current, advancement=advance)
        self.assertEqual((current, advance), originals)

    def test_actor_ties_use_public_coordinates_before_names_and_ignore_opaque_ids(self):
        _, selected = self.choose(state(foes=[foe("actor:z", -1, 0, name="weak"),
                                                 foe("actor:a", 1, 0, name="immune")]))
        self.assertEqual(selected.action["invocation"]["target"], "actor:z")

    def test_nearest_actor_precedes_public_coordinate_ties(self):
        _, selected = self.choose(state(foes=[foe("actor:a", 8, 0), foe("actor:z", 1, 0)]))
        self.assertEqual(selected.action["invocation"]["target"], "actor:z")

    def test_rattling_blow_uses_full_available_start_and_resolution_cost(self):
        known = ability("rattling_blow", cost={"resource": {"id": "stamina"}, "start": 2, "resolution": 3})
        for available, expected in [(4, "basic_melee"), (5, "rattling_blow")]:
            with self.subTest(available=available):
                _, decision = self.choose(state(foes=[foe("actor:f", 1, 0)], abilities=[known],
                                               resources=[pool(available=available)]))
                self.assertEqual(decision.action["id"], expected)

    def test_rattling_blow_unowned_or_on_cooldown_falls_back(self):
        for abilities in [[], [ability("rattling_blow", cooldown=1)]]:
            _, decision = self.choose(state(foes=[foe("actor:f", 1, 0)], abilities=abilities))
            self.assertEqual(decision.action["id"], "basic_melee")

    def test_rattling_blow_requires_owned_disclosed_supply_quantity(self):
        known = ability("rattling_blow", supplies=[{"supply": "bandage", "quantity": "2"}])
        for quantity, expected in [(0, "basic_melee"), (1, "basic_melee"), (2, "rattling_blow")]:
            _, decision = self.choose(state(foes=[foe("actor:f", 1, 0)], abilities=[known],
                                           inventory=[item("item:b", "bandage", quantity)]))
            self.assertEqual(decision.action["id"], expected)
            if quantity == 2:
                self.assertEqual(decision.action["supplies"], {"kind": "carried"})

    def test_diagonal_or_vertical_foe_never_uses_cardinal_melee(self):
        for position in [(1, 1, 0), (1, 0, 1)]:
            _, decision = self.choose(state(foes=[foe("actor:f", *position)],
                                           abilities=[ability("rattling_blow")]))
            self.assertNotEqual(decision.action.get("type"), "use_ability")

    def test_cold_lance_respects_ground_eye_witness_and_melee_exclusion(self):
        lance = ability("spark_lance_cold", reach={"kind": "sight", "range": 4})
        for x, z, expected in [(1, 0, "basic_melee"), (3, 0, "spark_lance_cold"),
                               (4, 0, None), (2, 1, None)]:
            with self.subTest(x=x, z=z):
                _, decision = self.choose(state(2, foes=[foe("actor:f", x, 0, z)], abilities=[lance]), character=2)
                self.assertEqual(decision.action.get("id"), expected)

    def test_cold_lance_uses_availability_and_cooldown_without_species_lookup(self):
        lance = ability("spark_lance_cold", reach={"kind": "sight", "range": 8},
                        cost={"resource": {"id": "mana"}, "start": 1, "resolution": 2})
        for available, cooldown, expected in [(3, 0, "spark_lance_cold"), (2, 0, None), (3, 1, None)]:
            lance["cooldown_remaining"] = str(cooldown)
            _, decision = self.choose(state(2, foes=[foe("actor:f", 3, 0, name="cold immune")],
                abilities=[lance], resources=[pool("mana", available=available)]), character=2)
            self.assertEqual(decision.action.get("id"), expected)

    def test_hostile_relation_allows_passive_foe_but_neutral_is_not_attacked(self):
        current = state(foes=[foe("actor:f", 1, 0, disposition="passive")])
        _, decision = self.choose(current)
        self.assertEqual(decision.action["id"], "basic_melee")
        current["observation"]["combat"]["actors"][0]["hostile"] = False
        _, decision = self.choose(current)
        self.assertEqual(decision.phase, "explore")

    def test_retreat_at_exact_quarter_uses_strict_distance_improvement(self):
        current = state(hp=25, foes=[foe("actor:f", 1, 0)])
        _, decision = self.choose(current)
        self.assertEqual(decision.action, {"type": "move", "direction": "west"})
        self.assertEqual(decision.phase, "retreat")
        current["observation"]["combat"]["hp"] = "26"
        _, decision = self.choose(current)
        self.assertEqual(decision.action["id"], "basic_melee")

    def test_retreat_direction_ties_follow_west_east_north_south(self):
        cells = [cell("home"), cell("west", -1), cell("east", 1), cell("south", 0, 1)]
        _, decision = self.choose(state(cells=cells, hp=1, foes=[foe("actor:f", 0, -1)]))
        self.assertEqual(decision.action["direction"], "west")

    def test_retreat_excludes_occupied_walls_and_remembered_invisible_cells(self):
        current = state(hp=1, cells=[cell("home"), cell("west", -1, wall=True),
                cell("north", 0, -1), cell("south", 0, 1)],
                foes=[foe("actor:f", 1, 0), foe("actor:block", 0, -1)])
        _, decision = self.choose(current)
        self.assertEqual(decision.action["direction"], "south")

    def test_trapped_retreat_and_depth_one_up_stair_fall_back_to_combat(self):
        _, decision = self.choose(state(hp=1, cells=[cell("home", up=True)], foes=[foe("actor:f", 1, 0)]))
        self.assertEqual(decision.action["id"], "basic_melee")

    def test_visible_combat_preempts_food_care_and_recovery(self):
        _, decision = self.choose(state(hp=50, hunger="empty", foes=[foe("actor:f", 1, 0)],
            inventory=[item("item:r", "ration"), item("item:b", "bandage")], abilities=[ability("bind_wounds")]))
        self.assertEqual(decision.action["id"], "basic_melee")

    def test_unreachable_preferred_foe_tries_next_target(self):
        current = state(cells=[cell("home"), cell("west", -1), cell("farwest", -2)],
                        foes=[foe("actor:a", 2, 0), foe("actor:z", -3, 0)])
        _, decision = self.choose(current)
        self.assertEqual(decision.action, {"type": "move", "direction": "west"})
        self.assertEqual(decision.phase, "combat_approach")

    def test_no_legal_combat_approach_and_no_frontier_is_an_explicit_censor(self):
        _, decision = self.choose(state(cells=[cell("home")], foes=[foe("actor:f", 3, 0)]))
        self.assertIsNone(decision.action)
        self.assertEqual(decision.outcome, "censored_no_disclosed_combat_route")

    def test_unapproachable_visible_foe_takes_one_disclosed_bfs_frontier_step(self):
        # A visible foot alone does not disclose a legal adjacent approach.
        current = state(cells=[cell("home"), cell("east", 1), cell("west", -1),
                               cell("foe:foot", 12, -2)], foes=[foe("actor:f", 12, -2)])
        _, decision = self.choose(current)
        self.assertEqual(decision.action, {"type": "move", "direction": "west"})
        self.assertEqual(decision.phase, "combat_explore")
        self.assertIsNone(decision.outcome)

    def test_combat_frontier_step_reassesses_newly_disclosed_legal_target(self):
        current = state(cells=[cell("home"), cell("west", -1), cell("foot", 12, -2)],
                        foes=[foe("actor:f", 12, -2)])
        controller, decision = self.choose(current)
        self.assertEqual(decision.action, {"type": "move", "direction": "west"})
        controller.consume_command()
        arrived = state(cells=[cell("west"), cell("home", 1), cell("new:foot", -1)],
                        foes=[foe("actor:f", -1, 0)])
        controller.observe(arrived, overview())
        decision = controller.choose(arrived)
        self.assertEqual(decision.action["id"], "basic_melee")
        self.assertEqual(decision.phase, "combat")

    def test_combat_frontier_does_not_route_through_an_occupied_cell(self):
        current = state(cells=[cell("home"), cell("west", -1), cell("east", 1), cell("foot", 12, -2)],
                        foes=[foe("actor:f", 12, -2)])
        current["observation"]["visible_actors"].append(foe("actor:neutral", -1, 0))
        _, decision = self.choose(current)
        self.assertEqual(decision.action, {"type": "move", "direction": "east"})
        self.assertEqual(decision.phase, "combat_explore")

    def test_combat_frontier_exhaustion_does_not_descend_or_revisit_forever(self):
        controller, _ = self.choose(state(cells=[cell("west"), cell("home", 1)]))
        current = state(cells=[cell("home", down=True), cell("west", -1)], foes=[foe("actor:f", 12, -2)])
        controller.observe(current, overview())
        decision = controller.choose(current)
        self.assertIsNone(decision.action)
        self.assertEqual(decision.outcome, "censored_no_disclosed_combat_route")

    def test_combat_frontier_keeps_combat_priority_over_food_care_and_loot(self):
        current = state(hp=50, hunger="empty", foes=[foe("actor:f", 12, -2)],
                        inventory=[item("item:r", "ration"), item("item:b", "bandage")],
                        ground=[ground("item:loot", "ration")], abilities=[ability("bind_wounds")])
        _, decision = self.choose(current)
        self.assertEqual(decision.action, {"type": "move", "direction": "west"})
        self.assertEqual(decision.phase, "combat_explore")

    def test_selection_queries_preserve_inputs_budgets_and_python_random_state(self):
        current = state(cells=[cell("home"), cell("east", 1), cell("west", -1), cell("foot", 12, -2)],
                        foes=[foe("actor:f", 12, -2)])
        owner = overview()
        originals, rng = copy.deepcopy((current, owner)), random.getstate()
        controller, first = self.choose(current, advancement=owner)
        for _ in range(4):
            controller.consume_command("query")
            controller.observe(current, owner)
            self.assertEqual(controller.choose(current), first)
        self.assertEqual(first.phase, "combat_explore")
        self.assertEqual((current, owner), originals)
        self.assertEqual(random.getstate(), rng)
        self.assertEqual(controller.commands, 0)
        self.assertEqual(controller.command_kinds, {})

    def test_reachable_supplies_take_whole_stack_in_public_fact_order(self):
        _, decision = self.choose(state(ground=[ground("item:z", "ration", quantity=3),
                                                ground("item:a", "bandage", quantity=7)]))
        self.assertEqual(decision.action, {"type": "take", "item": "item:a", "quantity": "7"})

    def test_unknown_unreachable_and_zero_supplies_are_not_taken(self):
        _, decision = self.choose(state(ground=[ground("item:a", None),
            ground("item:b", "bandage", reachable=False), ground("item:c", "ration", quantity=0)]))
        self.assertNotEqual(decision.action["type"], "take")

    def test_hunger_eats_exactly_one_owned_edible_ration(self):
        for hunger in ["hungry", "weak", "empty"]:
            current = state(hunger=hunger, inventory=[item("item:z", "ration", 5), item("item:a", "ration", 8)])
            _, decision = self.choose(current)
            self.assertEqual(decision.action, {"type": "eat", "item": "item:z"})

    def test_nourished_unowned_and_inedible_food_is_not_selected(self):
        for hunger, quantity, edible in [("nourished", 1, True), ("empty", 0, True), ("hungry", 1, False)]:
            current = state(hunger=hunger, inventory=[item("item:r", "ration", quantity)])
            current["observation"]["interactions"]["inventory"][0]["edible"] = edible
            _, decision = self.choose(current)
            self.assertNotEqual(decision.action["type"], "eat")

    def test_rest_attempts_bind_wounds_only_once_until_full_pools(self):
        current = state(hp=50, inventory=[item("item:b", "bandage", 3)], abilities=[ability("bind_wounds")])
        controller, decision = self.choose(current)
        self.assertEqual(decision.action, {"type": "use_ability", "id": "bind_wounds",
            "invocation": {"kind": "actor_target", "target": "actor:self"}, "supplies": {"kind": "carried"}})
        controller.consume_command()
        controller.observe(current, overview())
        self.assertEqual(controller.choose(current).action, {"type": "wait"})
        current["observation"]["combat"]["hp"] = "100"
        controller.observe(current, overview())
        self.assertEqual(controller.choose(current).phase, "explore")
        current["observation"]["combat"]["hp"] = "50"
        controller.observe(current, overview())
        self.assertEqual(controller.choose(current).action["id"], "bind_wounds")

    def test_bind_wounds_requires_injury_bandage_cooldown_and_affordability(self):
        for hp, stock, cooldown, available in [(100, 2, 0, 1), (50, 0, 0, 1), (50, 2, 1, 1), (50, 2, 0, 0)]:
            current = state(hp=hp, inventory=[item("item:b", "bandage", stock)],
                resources=[pool(balance=1, available=available)], abilities=[ability("bind_wounds", cooldown=cooldown,
                cost={"resource": {"id": "stamina"}, "start": 1, "resolution": 0})])
            _, decision = self.choose(current)
            self.assertEqual(decision.action, {"type": "wait"})

    def test_zero_stock_rest_retains_starvation_wait_tail(self):
        current = state(hp=50, hunger="empty")
        controller, first = self.choose(current)
        controller.consume_command()
        controller.observe(current, overview())
        self.assertEqual(first.action, {"type": "wait"})
        self.assertEqual(controller.choose(current).action, {"type": "wait"})
        self.assertTrue(controller.rest_active)

    def test_food_interrupts_rest_priority_without_resetting_care_attempt(self):
        current = state(hp=50, inventory=[item("item:b", "bandage"), item("item:r", "ration")],
                        abilities=[ability("bind_wounds")])
        controller, _ = self.choose(current)
        controller.consume_command()
        current["observation"]["combat"]["own_stats"]["nutrition"]["hunger"] = "hungry"
        controller.observe(current, overview())
        self.assertEqual(controller.choose(current).action["type"], "eat")
        controller.consume_command()
        current["observation"]["combat"]["own_stats"]["nutrition"]["hunger"] = "nourished"
        controller.observe(current, overview())
        self.assertEqual(controller.choose(current).action, {"type": "wait"})

    def test_visible_foe_interrupts_rest_and_next_clear_injury_starts_new_episode(self):
        current = state(hp=50, inventory=[item("item:b", "bandage", 2)], abilities=[ability("bind_wounds")])
        controller, _ = self.choose(current)
        controller.consume_command()
        fighting = copy.deepcopy(current)
        fighting["observation"]["visible_actors"] = [foe("actor:f", 1, 0)]
        fighting["observation"]["combat"]["actors"] = [{"actor": "actor:f", "hostile": True}]
        controller.observe(fighting, overview())
        self.assertFalse(controller.rest_active)
        self.assertEqual(controller.choose(fighting).action["id"], "basic_melee")
        controller.observe(current, overview())
        self.assertEqual(controller.choose(current).action["id"], "bind_wounds")

    def test_exploration_prefers_unvisited_bfs_west_before_disclosed_stairs(self):
        _, decision = self.choose(state(cells=[cell("home", down=True), cell("east", 1), cell("west", -1)]))
        self.assertEqual(decision.action, {"type": "move", "direction": "west"})

    def test_remembered_supplies_precede_unvisited_and_expire_when_observed_absent(self):
        current = state(cells=[cell("home"), cell("west", -1), cell("east", 1)],
                        ground=[ground("item:r", "ration", 1, reachable=False)])
        controller, decision = self.choose(current)
        self.assertEqual(decision.action["direction"], "east")
        obscured = state(cells=[cell("home"), cell("west", -1)])
        controller.observe(obscured, overview())
        self.assertEqual(controller.choose(obscured).action["direction"], "east")
        current["observation"]["ground_items"] = []
        controller.observe(current, overview())
        self.assertEqual(controller.choose(current).action["direction"], "west")

    def test_disclosed_route_exhaustion_never_uses_xp_or_hidden_floor_oracle(self):
        current = state(cells=[cell("home")])
        current["private_geometry"] = {"east": "secret"}
        _, decision = self.choose(current, advancement=overview(xp=110))
        self.assertEqual(decision.outcome, "censored_route_exhausted")
        self.assertIsNone(decision.action)

    def test_only_disclosed_down_stairs_are_used_after_exploration(self):
        _, decision = self.choose(state(cells=[cell("home", down=True)]))
        self.assertEqual(decision.action, {"type": "move", "direction": "down"})

    def test_depth_tracks_settled_ordinary_stair_commands_and_revisits(self):
        first = state(cells=[cell("floor1", down=True)])
        controller, decision = self.choose(first)
        controller.consume_command(action=decision.action)
        second = state(cells=[cell("floor2", up=True, down=True)])
        controller.observe(second, overview())
        self.assertEqual(controller.depth, 2)
        controller.observe(second, overview())
        self.assertEqual(controller.depth, 2)
        danger = state(cells=[cell("floor2", up=True)], hp=25, foes=[foe("actor:f", 1, 0)])
        controller.observe(danger, overview())
        self.assertEqual(controller.choose(danger).action, {"type": "move", "direction": "up"})
        controller.consume_command()
        controller.observe(first, overview())
        self.assertEqual(controller.depth, 1)
        self.assertEqual(set(controller.first_entries), {1, 2})

    def test_unverified_stair_transition_is_a_controller_failure(self):
        current = state(cells=[cell("home", down=True)])
        controller, _ = self.choose(current)
        controller.consume_command()
        with self.assertRaises(ControllerError):
            controller.observe(current, overview())

    def earned(self, controller=None):
        controller, _ = self.choose(controller=controller)
        current = state(hd=2)
        controller.observe(current, overview(hd=2, xp=100, spent=100))
        return controller, current

    def test_first_earned_hd2_requires_observed_start_growth_and_actual_hd(self):
        controller, current = self.earned()
        self.assertEqual(controller.choose(current).outcome, "earned_hd2")
        self.assertEqual(len(controller.growth_history), 2)
        self.assertEqual(controller.growth_history[0]["hit_dice"]["total"], "1")
        self.assertEqual(controller.growth_history[-1]["hit_dice"]["total"], "2")

    def test_synthetic_two_hd_or_points_alone_never_qualifies(self):
        for xp, spent, tier in [(0, 0, 2), (100, 0, 2), (100, 100, 1)]:
            controller, _ = self.choose()
            current = state(hd=2)
            controller.observe(current, overview(hd=tier, xp=xp, spent=spent))
            self.assertNotEqual(controller.choose(current).outcome, "earned_hd2")
        with self.assertRaises(ControllerError):
            self.choose(state(hd=2), advancement=overview(hd=2, xp=100, spent=100))

    def test_terminal_death_preempts_milestone_and_budget(self):
        controller, current = self.earned()
        current["observation"]["combat"].update(hp="0", dead=True, terminal=True)
        controller.observe(current, overview(hd=2, xp=100, spent=100))
        self.assertEqual(controller.choose(current).outcome, "death")

    def test_mutating_budget_counts_confirmation_actions_resumes_and_rejections(self):
        controller, _ = self.choose(controller=Controller(1, first_limit=3))
        controller.consume_command("confirmation")
        controller.consume_command("action")
        controller.consume_command("resume")
        for _ in range(5):
            controller.consume_command("query")
        self.assertEqual(controller.commands, 3)
        self.assertEqual(controller.choose(state()).outcome, "censored_action_budget")
        with self.assertRaises(ControllerError):
            controller.consume_command("action")

    def test_first_budget_does_not_reset_across_floors(self):
        current = state(cells=[cell("one", down=True)])
        controller, _ = self.choose(current, controller=Controller(1, first_limit=1))
        controller.consume_command()
        current = state(cells=[cell("two", up=True)])
        controller.observe(current, overview())
        self.assertEqual(controller.choose(current).outcome, "censored_action_budget")

    def test_continuation_requires_surviving_earned_milestone_and_never_buys(self):
        controller, current = self.earned()
        controller.begin_continuation()
        self.assertEqual(controller.choose(current).phase, "explore")
        self.assertNotEqual(controller.choose(current).action["type"], "advancement")
        with self.assertRaises(ControllerError):
            controller.begin_continuation()
        with self.assertRaises(ControllerError):
            Controller(1).begin_continuation()

    def test_continuation_floor_spending_and_run_limit_survive_checkpoint(self):
        controller, current = self.earned(Controller(1, floor_limit=2, run_limit=3))
        controller.consume_command("confirmation")
        controller.begin_continuation()
        controller.choose(current)
        controller.consume_command()
        restored = Controller.load(json.loads(json.dumps(controller.dump())))
        restored.observe(current, overview(hd=2, xp=100, spent=100))
        restored.choose(current)
        restored.consume_command("resume")
        self.assertEqual(restored.choose(current).outcome, "censored_action_budget")
        self.assertEqual(restored.floor_commands[1], 2)
        self.assertEqual(restored.commands, 3)

    def test_checkpoint_roundtrip_keeps_graph_visited_supply_rest_growth_and_sequence(self):
        current = state(hp=50, inventory=[item("item:b", "bandage")], abilities=[ability("bind_wounds")],
            ground=[ground("item:r", "ration", -1, reachable=False)])
        controller, _ = self.choose(current)
        controller.consume_command()
        controller.observe(current, overview())
        snapshot = controller.dump()
        restored = Controller.load(json.loads(json.dumps(snapshot)))
        self.assertEqual(restored.dump(), snapshot)
        self.assertEqual(restored.choose(current), controller.choose(current))
        self.assertEqual(restored.choose(current).action, {"type": "wait"})
        snapshot["character"] = 2
        self.assertEqual(controller.character, 1)

    def test_checkpoint_rejects_unknown_revision_and_invalid_character(self):
        checkpoint = Controller(1).dump()
        checkpoint["version"] = 999
        with self.assertRaises(ControllerError):
            Controller.load(checkpoint)
        with self.assertRaises(ValueError):
            Controller(99)

    def test_fresh_opaque_identity_and_input_order_cannot_change_actor_choice(self):
        for left, right in [("actor:a", "actor:z"), ("actor:z", "actor:a")]:
            for reverse in (False, True):
                actors = [foe(left, -1, 0, name="emu"), foe(right, 1, 0, name="emu")]
                if reverse:
                    actors.reverse()
                _, decision = self.choose(state(foes=actors))
                self.assertEqual(decision.action["invocation"]["target"], left)

    def test_fresh_opaque_identity_cannot_change_ground_supply_choice(self):
        for left, right in [("item:a", "item:z"), ("item:z", "item:a")]:
            _, decision = self.choose(state(ground=[ground(right, "ration", 1), ground(left, "ration", -1)]))
            self.assertEqual(decision.action["item"], left)

    def test_indistinguishable_actor_targets_wait_without_private_identity_tie(self):
        _, decision = self.choose(state(foes=[foe("actor:a", 1, 0), foe("actor:z", 1, 0)]))
        self.assertEqual(decision.action, {"type": "wait"})
        self.assertEqual(decision.phase, "ambiguous_actor")

    def test_indistinguishable_supply_targets_wait_without_private_identity_tie(self):
        _, decision = self.choose(state(ground=[ground("item:a", "ration"), ground("item:z", "ration")]))
        self.assertEqual(decision.action, {"type": "wait"})
        self.assertEqual(decision.phase, "ambiguous_supply")

    def test_indistinguishable_carried_food_waits_without_private_identity_tie(self):
        _, decision = self.choose(state(hunger="hungry", inventory=[item("item:a", "ration"), item("item:z", "ration")]))
        self.assertEqual(decision.action, {"type": "wait"})
        self.assertEqual(decision.phase, "ambiguous_food")

    def test_duplicate_actor_occurrences_keep_nearest_public_witness(self):
        _, decision = self.choose(state(foes=[foe("actor:a", 1, 0), foe("actor:a", 8, 0), foe("actor:b", -2, 0)]))
        self.assertEqual(decision.action["invocation"]["target"], "actor:a")

    def test_entry_and_growth_witness_use_ordinary_observation_tick(self):
        current = state()
        current["observation"]["tick"] = "1234"
        controller, _ = self.choose(current)
        self.assertEqual(controller.first_entries[1]["tick"], "1234")
        self.assertEqual(controller.growth_history[0]["tick"], "1234")

    def test_not_ready_observation_cannot_select_a_new_gameplay_action(self):
        current = state()
        current["observation"]["ready"] = False
        with self.assertRaises(ControllerError):
            self.choose(current)

    def test_current_disclosed_wall_blocks_a_previously_walkable_route(self):
        current = state()
        controller, _ = self.choose(current)
        current["observation"]["visible_cells"][1]["wall"] = True
        controller.observe(current, overview())
        self.assertEqual(controller.choose(current).outcome, "censored_route_exhausted")

    def test_death_before_a_stair_resolves_is_retained_as_death(self):
        current = state(cells=[cell("home", down=True)])
        controller, _ = self.choose(current)
        controller.consume_command()
        current["observation"]["combat"].update(hp="0", dead=True, terminal=True)
        controller.observe(current, overview())
        self.assertEqual(controller.depth, 1)
        self.assertEqual(controller.choose(current).outcome, "death")

    def test_continuation_floor_revisit_keeps_spent_budget(self):
        controller, current = self.earned(Controller(1, floor_limit=2))
        controller.begin_continuation()
        first = state(hd=2, cells=[cell("one", down=True)])
        controller.observe(first, overview(hd=2, xp=100, spent=100))
        controller.choose(first)
        controller.consume_command()
        second = state(hd=2, cells=[cell("two", up=True)], hp=25, foes=[foe("actor:f", 1, 0)])
        controller.observe(second, overview(hd=2, xp=100, spent=100))
        self.assertEqual(controller.choose(second).action["direction"], "up")
        controller.consume_command()
        controller.observe(first, overview(hd=2, xp=100, spent=100))
        self.assertEqual(controller.floor_commands, {1: 1, 2: 1})
        controller.consume_command("resume")
        self.assertEqual(controller.choose(first).outcome, "censored_action_budget")

    def test_continuation_stops_on_first_ordinary_depth_twenty_seven_entry(self):
        controller, _ = self.earned()
        controller.begin_continuation()
        for depth in range(1, 28):
            current = state(hd=2, cells=[cell(f"floor{depth}", up=depth > 1, down=True)])
            controller.observe(current, overview(hd=2, xp=100, spent=100))
            decision = controller.choose(current)
            if depth < 27:
                self.assertEqual(decision.action["direction"], "down")
                controller.consume_command()
            else:
                self.assertEqual(decision.outcome, "checkpoint_depth_27")
                self.assertIsNone(decision.action)
        self.assertEqual(set(controller.first_entries), set(range(1, 28)))

    def test_revision_three_memory_is_rejected_and_death_precedes_depth27(self):
        controller, current = self.earned()
        old = controller.dump()
        old["version"] = 3
        with self.assertRaises(ControllerError):
            Controller.load(old)
        controller.begin_continuation()
        controller.depth = 27
        current["observation"]["combat"].update(hp="0", dead=True, terminal=True)
        controller.observe(current, overview(hd=2, xp=100, spent=100))
        self.assertEqual(controller.choose(current).outcome, "death")

    def test_checkpoint_preserves_pending_paid_stair_and_counts_resume(self):
        current = state(cells=[cell("one", down=True)])
        controller, _ = self.choose(current)
        controller.consume_command()
        restored = Controller.load(json.loads(json.dumps(controller.dump())))
        restored.consume_command("resume")
        arrived = state(cells=[cell("two", up=True)])
        restored.observe(arrived, overview())
        self.assertEqual(restored.depth, 2)
        self.assertEqual(restored.commands, 2)

    def test_floor_transition_ends_the_existing_rest_episode(self):
        current = state(hp=50, cells=[cell("one", down=True)], inventory=[item("item:b", "bandage")],
                        abilities=[ability("bind_wounds")])
        controller, _ = self.choose(current)
        controller.consume_command()
        controller.consume_command(action={"type": "move", "direction": "down"})
        arrived = state(hp=50, cells=[cell("two", up=True)], inventory=[item("item:b", "bandage")],
                        abilities=[ability("bind_wounds")])
        controller.observe(arrived, overview())
        self.assertFalse(controller.rest_active)
        self.assertEqual(controller.choose(arrived).action["id"], "bind_wounds")

    def test_checkpoint_rejects_modified_budget_accounting_and_unknown_fields(self):
        controller, _ = self.choose()
        controller.consume_command()
        for field, changed in [("commands", 0), ("first_commands", -1), ("unknown", True)]:
            checkpoint = controller.dump()
            checkpoint[field] = changed
            with self.assertRaises(ControllerError):
                Controller.load(checkpoint)

    def test_checkpoint_rejects_fabricated_disclosed_edges(self):
        controller, _ = self.choose()
        checkpoint = controller.dump()
        checkpoint["floors"]["1"]["edges"]["cell:home"]["east"] = "hidden:unknown"
        with self.assertRaises(ControllerError):
            Controller.load(checkpoint)

    def test_owner_overview_page_wrapper_and_value_have_identical_decisions(self):
        current = state()
        first, decision = self.choose(current)
        second, wrapped = self.choose(current, advancement={"advancement_page": {
            "entries": [{"type": "overview", "value": overview()}]}})
        self.assertEqual(decision, wrapped)
        self.assertEqual(first.dump(), second.dump())

    def test_wizard_state_and_unsettled_action_selection_are_rejected(self):
        current = state()
        current["wizard_game"] = True
        with self.assertRaises(ControllerError):
            self.choose(current)
        current = state()
        current["observation"]["combat"]["preparation_remaining"] = "3"
        with self.assertRaises(ControllerError):
            self.choose(current)


if __name__ == "__main__":
    unittest.main()
