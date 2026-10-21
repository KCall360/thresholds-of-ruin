"""Current bounded Rogue controller using only ordinary disclosures.

This module performs no I/O and never reads a save, population receipt, diagnostic
flow, species rule or generated geometry. The adapter owns initial-kit/purchase
verification, readiness/resumes, timeout classification and post-stop exact HD
history audit. An ``earned_hd2`` decision is the disclosed milestone candidate;
complete evidence still requires that independent audit.

Call observe(raw_state, owner_overview) at each settled state, choose(raw_state),
then consume_command(kind, action=decision.action) immediately BEFORE sending a
mutating command, including a command that is subsequently refused. Queries cost
nothing. A refused command must fail the attempt, never be retried by this policy.
"""
from collections import Counter, deque
from copy import deepcopy
from dataclasses import dataclass

from scalar_wire import u64
from rogue_test_support import (
    DisclosedFloorGraph, disclosed_body_cells, disclosed_melee_candidate, disclosed_step_action,
    ground_sight_candidate,
)

POLICY_REVISION = 7
CHECKPOINT_DEPTHS = (1, 5, 8, 11, 17, 26, 27)
CHARACTERS = {1: ("Warrior", "warrior", "rattling_blow"),
              2: ("Mage", "mage", "spark_lance_cold")}


class ControllerError(ValueError):
    """An invalid disclosure, checkpoint or policy invocation; never survival."""


@dataclass(frozen=True)
class Decision:
    action: dict | None = None
    phase: str = "terminal"
    outcome: str | None = None


def first_purchase(character):
    """The sole permitted preview; the adapter separately confirms it once."""
    if character not in CHARACTERS or isinstance(character, bool):
        raise ControllerError("Only actual Warrior actor 1 and Mage actor 2 are planned")
    return ("purchase 1 advancement talent rattling_blow" if character == 1 else
            "purchase 1 advancement talent evocation_initiate form spark_lance_cold")


def fixed_cohort():
    """New independent inventory containing every planned slot before launch."""
    return [{"arm": arm, "character": CHARACTERS[actor][0], "actor": actor,
             "seed": seed, "outcome": None, "evidence_status": "unrun"}
            for arm in ("production", "candidate") for actor in (1, 2) for seed in (0, 1)]


def _overview(value):
    if "advancement_page" in value:
        value = value["advancement_page"]
    if "entries" in value:
        rows = [row["value"] for row in value["entries"] if row.get("type", "overview") == "overview"]
        if len(rows) != 1:
            raise ControllerError("Expected exactly one ordinary owner overview")
        value = rows[0]
    for field in ("earned", "spent", "unspent"):
        u64(value["points"][field])
    for field in ("highest_earned_tier", "highest_applied_tier", "restorable_hit_dice",
                  "peak_threat", "next_threshold", "earned_xp", "spent_xp", "held_xp"):
        u64(value["growth"][field])
    return value


def _position(row):
    p = row["position"]
    return p["x"], p["y"], p["z"]


def _here(state):
    cells = [row for row in state["observation"]["visible_cells"] if _position(row) == (0, 0, 0)]
    if len(cells) != 1 or cells[0]["wall"]:
        raise ControllerError("Missing unique disclosed walkable underfoot cell")
    return cells[0]


def _actor_key(row):
    x, y, z = _position(row)
    return (abs(x) + abs(y), x, y, z, row.get("name", ""),
            row.get("description", ""), row.get("asset") or "")


def _item_key(row):
    # Content IDs describe disclosed capability, unlike randomized target IDs.
    return ((row.get("known_supply") or {}).get("id", ""), row.get("class", ""),
            row.get("name", ""), row.get("appearance", ""), row.get("description", ""),
            row.get("identified", False), int(row["quantity"]))


def _ground_key(row):
    return (*_position(row), *_item_key(row["item"]))


def _foes(view):
    hostile = {row["actor"] for row in view["combat"]["actors"] if row["hostile"]}
    occurrences = {}
    for row in view["visible_actors"]:
        if row["id"] in hostile:
            previous = occurrences.get(row["id"])
            if previous is None or _actor_key(row) < _actor_key(previous):
                occurrences[row["id"]] = row
    # Equality handles merge occurrences only. Opaque IDs never break ties;
    # genuinely indistinguishable eligible targets cause an explicit wait.
    return sorted(occurrences.values(), key=_actor_key)


def _deficient(view):
    combat = view["combat"]
    return (u64(combat["hp"]) < u64(combat["max_hp"]) or any(
        row["balance"] < row["maximum"] for row in combat["own_stats"]["resources"]))


def _stock(view):
    result = Counter()
    for row in view["inventory"]:
        if row.get("known_supply"):
            result[row["known_supply"]["id"]] += int(row["quantity"])
    return result


def _available(view, identity):
    known = next((row for row in view["combat"]["own_stats"]["abilities"]
                  if row["id"] == identity), None)
    if known is None or int(known["cooldown_remaining"]) != 0:
        return None
    cost = known["cost"]
    if cost:
        resource = next((row for row in view["combat"]["own_stats"]["resources"]
                         if row["resource"]["id"] == cost["resource"]["id"]), None)
        if resource is None or resource["available"] < cost["start"] + cost["resolution"]:
            return None
    stock = _stock(view)
    if any(stock[cost["supply"]] < int(cost["quantity"]) for cost in known["supply_costs"]):
        return None
    return known


def _use(known, target, *, carried=False):
    result = {"type": "use_ability", "id": known["id"],
              "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}}
    if carried or known["supply_costs"]:
        result["supplies"] = {"kind": "carried"}
    return result


class _Floor:
    """Serialization wrapper; the shared graph never gains private inputs."""
    def __init__(self):
        self.graph = DisclosedFloorGraph()
        self.visited = set()
        self.supplies = {}

    def observe(self, state):
        self.graph.observe(state)
        self.visited.add(_here(state)["key"])
        view = state["observation"]
        visible = {_position(row): row["key"] for row in view["visible_cells"]
                   if row["position"]["z"] == 0 and not row["wall"]}
        current = {}
        for row in view["ground_items"]:
            item = row["item"]
            if ((item.get("known_supply") or {}).get("id") in ("bandage", "ration")
                    and int(item["quantity"]) > 0 and _position(row) in visible):
                current[item["id"]] = visible[_position(row)]
        visible_keys = set(visible.values())
        self.supplies = {identity: key for identity, key in self.supplies.items()
                         if key not in visible_keys}
        self.supplies.update(current)

    def dump(self):
        return {"edges": deepcopy(self.graph.edges), "stairs_down": sorted(self.graph.stairs_down),
                "visited": sorted(self.visited), "supplies": dict(sorted(self.supplies.items()))}

    @classmethod
    def load(cls, value):
        if set(value) != {"edges", "stairs_down", "visited", "supplies"}:
            raise ControllerError("Invalid disclosed floor checkpoint fields")
        result = cls()
        result.graph.edges = deepcopy(value["edges"])
        result.graph.stairs_down = set(value["stairs_down"])
        result.visited = set(value["visited"])
        result.supplies = deepcopy(value["supplies"])
        keys = set(result.graph.edges)
        if (not result.visited <= keys or not result.graph.stairs_down <= keys
                or not set(result.supplies.values()) <= keys):
            raise ControllerError("Checkpoint memory references an undisclosed cell")
        for key, edges in result.graph.edges.items():
            for direction, neighbor in edges.items():
                if direction not in DisclosedFloorGraph.reverse or neighbor not in keys:
                    raise ControllerError("Invalid disclosed checkpoint edge")
                reverse = DisclosedFloorGraph.reverse[direction]
                if result.graph.edges[neighbor].get(reverse) != key:
                    raise ControllerError("Nonreciprocal disclosed checkpoint edge")
        return result

    def route(self, start, targets, blocked):
        """The graph's west/east/north/south BFS with disclosed occupancy blocked."""
        if not blocked:
            return self.graph.route_to_any(start, targets)
        parents, pending = {start: None}, deque([start])
        while pending:
            here = pending.popleft()
            if here in targets:
                result = []
                while here != start:
                    parent, direction = parents[here]
                    result.append((here, direction))
                    here = parent
                return result[::-1]
            for _, _, direction in DisclosedFloorGraph.directions:
                neighbor = self.graph.edges.get(here, {}).get(direction)
                if neighbor is not None and neighbor not in parents and neighbor not in blocked:
                    parents[neighbor] = (here, direction)
                    pending.append(neighbor)
        return None


class Controller:
    """One original cohort member, across first-HD and source-free continuation."""
    def __init__(self, character, *, first_limit=2500, floor_limit=2500, run_limit=65000):
        first_purchase(character)
        if any(type(value) is not int or value < 0 for value in (first_limit, floor_limit, run_limit)):
            raise ControllerError("Command bounds must be nonnegative integers")
        self.character = character
        self.first_limit, self.floor_limit, self.run_limit = first_limit, floor_limit, run_limit
        self.segment = "first_hd"
        self.commands = 0
        self.first_commands = 0
        self.floor_commands = {}
        self.command_kinds = {}
        self.depth = 1
        self.first_entries = {}
        self.floors = {}
        self.rest_active = False
        self.care_attempted = False
        self.rest_episodes = 0
        self.growth_history = []
        self.earned_hd2 = False
        self._advancement = None
        self._last_key = None
        self._planned = None
        self._pending_stair = None
        self._dead = False

    def observe(self, state, advancement):
        """Refresh only settled ordinary facts, preserving all input objects."""
        if state.get("wizard_game"):
            raise ControllerError("Wizard games are not ordinary earned-route evidence")
        view, here = state["observation"], _here(state)
        combat = view["combat"]
        u64(combat["hp"])
        u64(combat["max_hp"])
        if (combat["preparation_remaining"] is not None or not view["ready"]) and not combat["terminal"]:
            raise ControllerError("Observe requires the same paid intention to settle first")
        if self._pending_stair is not None:
            transition = self._pending_stair
            opposite = "stairs_up" if transition["direction"] == "down" else "stairs_down"
            if combat["dead"] and here["key"] == transition["from"]:
                # The actor died before the paid stair action moved it. Keep the
                # command in the spent tail, but do not invent a floor entry.
                pass
            elif here["key"] == transition["from"] or not here[opposite]:
                raise ControllerError("Ordinary stair transition was not verified by its settled disclosure")
            else:
                self.depth += 1 if transition["direction"] == "down" else -1
            if self.depth < 1:
                raise ControllerError("Route cannot ascend beyond the authored depth-one start")
            self._pending_stair = None
            self._end_rest()
        self.first_entries.setdefault(self.depth, {"commands": self.commands, "tick": view["tick"]})
        self.floors.setdefault(self.depth, _Floor()).observe(state)
        self.floor_commands.setdefault(self.depth, 0)
        self._last_key = here["key"]
        self._advancement = deepcopy(_overview(advancement))
        growth = self._advancement["growth"]
        summary = combat["own_stats"]["hit_dice"]
        total = u64(summary["total"])
        source_counts = [(row, u64(row["count"])) for row in summary["sources"]]
        if any(count == 0 for _, count in source_counts) or sum(count for _, count in source_counts) != total:
            raise ControllerError("Inconsistent disclosed hit-die source counts")
        expected = CHARACTERS[self.character][1]
        if not self.growth_history and (total != 1 or len(summary["sources"]) != 1
                or summary["sources"][0]["source_kind"] != "class"
                or summary["sources"][0]["source"]["id"] != expected
                or u64(summary["sources"][0]["count"]) != 1):
            raise ControllerError("Initial ordinary observation must be the actual authored class HD one")
        witness = {"hit_dice": deepcopy(summary), "growth": deepcopy(growth),
                   "commands": self.commands, "tick": view["tick"]}
        if not self.growth_history or any(self.growth_history[-1][field] != witness[field]
                                         for field in ("hit_dice", "growth")):
            self.growth_history.append(witness)
        class_hd = sum(count for row, count in source_counts
                       if row["source_kind"] == "class" and row["source"]["id"] == expected)
        baseline = self.growth_history[0]
        if (len(self.growth_history) >= 2 and total >= 2 and class_hd == total
                and u64(baseline["growth"]["earned_xp"]) == 0
                and u64(baseline["growth"]["spent_xp"]) == 0
                and u64(growth["earned_xp"]) > 0 and u64(growth["spent_xp"]) > 0
                and u64(growth["highest_earned_tier"]) >= 2 and u64(growth["highest_applied_tier"]) >= 2):
            self.earned_hd2 = True
        self._dead = bool(combat["dead"] or u64(combat["hp"]) == 0)
        if combat["terminal"] or _foes(view) or not _deficient(view):
            self._end_rest()

    def _end_rest(self):
        self.rest_active = False
        self.care_attempted = False

    def _at_budget(self):
        if self.commands >= self.run_limit:
            return True
        if self.segment == "first_hd":
            return self.first_commands >= self.first_limit
        return self.floor_commands.get(self.depth, 0) >= self.floor_limit

    def _decision(self, action=None, phase="terminal", outcome=None):
        self._planned = deepcopy(action)
        return Decision(deepcopy(action), phase, outcome)

    def _step_decision(self, view, direction, phase):
        action = disclosed_step_action(view, direction)
        return self._decision(action, "open_door" if action["type"] == "set_door" else phase)

    def choose(self, state):
        """Choose one fixed-policy action; never issue or pay for it here."""
        view, here = state["observation"], _here(state)
        combat = view["combat"]
        u64(combat["hp"])
        u64(combat["max_hp"])
        if self._advancement is None or here["key"] != self._last_key:
            raise ControllerError("Observe the settled state before choosing")
        if state.get("wizard_game") or ((combat["preparation_remaining"] is not None or not view["ready"]) and not combat["terminal"]):
            raise ControllerError("Selection requires settled ordinary play")
        if combat["dead"] or u64(combat["hp"]) == 0:
            self._end_rest()
            return self._decision(outcome="death")
        if combat["terminal"]:
            raise ControllerError("Unexpected terminal state in bounded Rogue route")
        if self.segment == "first_hd" and self.earned_hd2:
            return self._decision(outcome="earned_hd2")
        if self.segment == "continuation" and self.depth >= 27:
            self._end_rest()
            return self._decision(outcome="checkpoint_depth_27")
        if self._at_budget():
            self._end_rest()
            return self._decision(outcome="censored_action_budget")
        floor = self.floors[self.depth]
        cells = {_position(row): row for row in view["visible_cells"]
                 if row["position"]["z"] == 0 and not row["wall"]}
        body = disclosed_body_cells(view)
        # Translate only disclosed own-body offsets. A hovering actor blocks a
        # tall body's head, but an actor on a separate height is not projected
        # onto this floor. Known wall cells constrain the same exact 3D volume.
        obstacles = {_position(row) for row in view["visible_actors"]
                     if row["id"] != view["self_target"]}
        obstacles.update(_position(row) for row in view["visible_cells"] if row["wall"])
        occupied = {anchor for anchor in cells if any(
            tuple(a + b for a, b in zip(anchor, own)) in obstacles for own in body)}
        blocked = {cells[p]["key"] for p in occupied if p in cells}
        blocked.update(row["key"] for row in view["visible_cells"] if row["wall"])
        foes = _foes(view)
        if foes:
            self._end_rest()
            if u64(combat["hp"]) * 4 <= u64(combat["max_hp"]):
                if self.depth > 1 and here["stairs_up"]:
                    return self._decision({"type": "move", "direction": "up"}, "retreat")
                distance = lambda x, y: min(abs(x - row["position"]["x"]) +
                    abs(y - row["position"]["y"]) for row in foes)
                best, direction = distance(0, 0), None
                for x, y, candidate in DisclosedFloorGraph.directions:
                    if (x, y, 0) in cells and (x, y, 0) not in occupied and distance(x, y) > best:
                        best, direction = distance(x, y), candidate
                if direction is not None:
                    return self._step_decision(view, direction, "retreat")
            counts = Counter(_actor_key(target) for target in foes)
            for target in foes:
                if counts[_actor_key(target)] > 1:
                    return self._decision({"type": "wait"}, "ambiguous_actor")
                target_cells = [row for row in view["visible_actors"] if row["id"] == target["id"]]
                # Representative sorting still chooses the same actor. Its
                # other disclosed body cells may independently witness reach.
                if any(disclosed_melee_candidate(view, row) for row in target_cells):
                    known = _available(view, "rattling_blow") if self.character == 1 else None
                    if known is not None and known["reach"]["kind"] != "melee":
                        known = None
                    known = known or _available(view, "basic_melee")
                    if known is not None:
                        return self._decision(_use(known, target["id"]), "combat")
                elif self.character == 2:
                    known = _available(view, "spark_lance_cold")
                    if (known is not None and known["reach"]["kind"] == "sight"
                            and ground_sight_candidate(target, known["reach"]["range"])):
                        return self._decision(_use(known, target["id"]), "combat")
                target_positions = {_position(row) for row in target_cells}
                neighbors = {cell["key"] for anchor, cell in cells.items()
                    if anchor not in occupied and any(
                        sum(abs(a + b - t) for a, b, t in zip(anchor, own, target_cell)) == 1
                        for own in body for target_cell in target_positions)}
                route = floor.route(here["key"], neighbors, blocked)
                if route:
                    return self._step_decision(view, route[0][1], "combat_approach")
            # Visibility of a foot need not disclose any cardinal approach.
            # Explore one already disclosed frontier step, then reassess from
            # the next ordinary state. Never infer hidden connecting terrain.
            route = floor.route(here["key"], floor.graph.edges.keys() - floor.visited, blocked)
            if route:
                return self._step_decision(view, route[0][1], "combat_explore")
            return self._decision(outcome="censored_no_disclosed_combat_route")
        supplies = sorted((row for row in view["ground_items"]
            if row["reachable"] and int(row["item"]["quantity"]) > 0
            and (row["item"].get("known_supply") or {}).get("id") in ("bandage", "ration")),
            key=_ground_key)
        if supplies:
            if len(supplies) > 1 and _ground_key(supplies[0]) == _ground_key(supplies[1]):
                return self._decision({"type": "wait"}, "ambiguous_supply")
            item = supplies[0]["item"]
            return self._decision({"type": "take", "item": item["id"], "quantity": str(item["quantity"])}, "loot")
        nutrition = combat["own_stats"]["nutrition"]
        edible = {row["item"] for row in view["interactions"]["inventory"] if row["edible"]}
        if nutrition.get("hunger") in ("hungry", "weak", "empty"):
            rations = sorted((row for row in view["inventory"]
                if row["id"] in edible and int(row["quantity"]) > 0
                and (row.get("known_supply") or {}).get("id") == "ration"), key=_item_key)
            if rations:
                if len(rations) > 1 and _item_key(rations[0]) == _item_key(rations[1]):
                    return self._decision({"type": "wait"}, "ambiguous_food")
                return self._decision({"type": "eat", "item": rations[0]["id"]}, "eat")
        if _deficient(view):
            if not self.rest_active:
                self.rest_active, self.care_attempted = True, False
                self.rest_episodes += 1
            known = _available(view, "bind_wounds")
            if (not self.care_attempted and u64(combat["hp"]) < u64(combat["max_hp"])
                    and _stock(view)["bandage"] > 0 and known is not None):
                return self._decision(_use(known, view["self_target"], carried=True), "care")
            return self._decision({"type": "wait"}, "rest")
        self._end_rest()
        for targets, phase in [(set(floor.supplies.values()) - {here["key"]}, "supply_approach"),
                (floor.graph.edges.keys() - floor.visited, "explore"),
                (floor.graph.stairs_down, "descend")]:
            route = floor.route(here["key"], targets, blocked)
            if route:
                return self._step_decision(view, route[0][1], phase)
            if route == [] and phase == "descend" and here["stairs_down"]:
                return self._decision({"type": "move", "direction": "down"}, "descend")
        return self._decision(outcome="censored_route_exhausted")

    def consume_command(self, kind="action", *, action=None):
        """Account before submission, so rejected actions still consume bounds."""
        if kind == "query":
            return
        if kind not in ("confirmation", "action", "resume"):
            raise ControllerError("Unknown command category")
        if self._at_budget():
            raise ControllerError("No mutating command permitted beyond the deterministic budget")
        self.commands += 1
        self.command_kinds[kind] = self.command_kinds.get(kind, 0) + 1
        if self.segment == "first_hd":
            self.first_commands += 1
        else:
            self.floor_commands[self.depth] = self.floor_commands.get(self.depth, 0) + 1
        if kind != "action":
            return
        action = deepcopy(self._planned if action is None else action)
        if action is None:
            return
        if action.get("type") == "use_ability" and action.get("id") == "bind_wounds":
            self.care_attempted = True
        if action.get("type") == "move" and action.get("direction") in ("up", "down"):
            if self._pending_stair is not None:
                raise ControllerError("A second stair action cannot replace unsettled work")
            self._pending_stair = {"direction": action["direction"], "from": self._last_key}

    def begin_continuation(self):
        """Resume the same surviving saved controller; never make another build."""
        if self.segment != "first_hd" or not self.earned_hd2 or self._dead:
            raise ControllerError("Continuation requires the original surviving earned-HD milestone")
        self.segment = "continuation"
        self._planned = None

    def dump(self):
        """Return independent JSON-safe controller memory, including spent bounds."""
        result = {"version": POLICY_REVISION}
        for name in ("character", "first_limit", "floor_limit", "run_limit", "segment", "commands",
                     "first_commands", "command_kinds", "depth", "rest_active", "care_attempted",
                     "rest_episodes", "growth_history", "earned_hd2", "_advancement", "_last_key",
                     "_planned", "_pending_stair", "_dead"):
            result[name] = deepcopy(getattr(self, name))
        result["floor_commands"] = {str(k): v for k, v in sorted(self.floor_commands.items())}
        result["first_entries"] = {str(k): deepcopy(v) for k, v in sorted(self.first_entries.items())}
        result["floors"] = {str(k): v.dump() for k, v in sorted(self.floors.items())}
        return result

    @classmethod
    def load(cls, value):
        """Restore current-format memory exactly; no historical defaults/migration."""
        try:
            if value["version"] != POLICY_REVISION:
                raise ControllerError("Unsupported earned-route checkpoint version")
            result = cls(value["character"], first_limit=value["first_limit"],
                         floor_limit=value["floor_limit"], run_limit=value["run_limit"])
            if set(value) != set(result.dump()):
                raise ControllerError("Invalid earned-route checkpoint fields")
            for name in value:
                if name not in ("version", "floor_commands", "first_entries", "floors"):
                    setattr(result, name, deepcopy(value[name]))
            result.floor_commands = {int(k): v for k, v in value["floor_commands"].items()}
            result.first_entries = {int(k): deepcopy(v) for k, v in value["first_entries"].items()}
            result.floors = {int(k): _Floor.load(v) for k, v in value["floors"].items()}
            counts = [result.commands, result.first_commands, *result.floor_commands.values(),
                      *result.command_kinds.values(), result.rest_episodes]
            if (any(type(n) is not int or n < 0 for n in counts)
                    or result.commands != sum(result.command_kinds.values())
                    or result.commands != result.first_commands + sum(result.floor_commands.values())
                    or result.segment not in ("first_hd", "continuation")
                    or type(result.depth) is not int or result.depth < 1):
                raise ControllerError("Invalid earned-route checkpoint budgets or segment")
            return result
        except (KeyError, TypeError, AttributeError) as error:
            raise ControllerError(f"Malformed earned-route checkpoint: {error}") from error
