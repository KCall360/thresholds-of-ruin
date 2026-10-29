"""Conservative disclosed-geometry choices for ordinary Rogue acceptance.

These flat-floor drivers do not reproduce the engine's general contact solver.
The server still validates every submitted target and reports any refusal.
"""
import json
from scalar_wire import u64


def disclosed_frames(player, start):
    """Keep transient resolved events even when the later ready frame replaces them."""
    for line in player.transcript[start:]:
        if line.startswith("{"):
            frame = json.loads(line)
            if frame.get("state") is not None:
                yield frame


def ordered_combat_events(frames, *, after=None):
    """Read each actual Outcome once, preserving fact order and equal-valued hits.

    Pictures/ACKs may repeat prior presentation, but are never event authority.
    The optional baseline is a causal cursor on this exact attachment, not a
    display generation. Actor handles are never matched across attachments.
    """
    seen = {}
    for frame in frames:
        message = frame.get("message") or {}
        if message.get("type") != "update":
            continue
        update = message["update"]
        body = update["body"]
        if body["type"] != "outcome":
            continue
        context = update["context"]
        if after is not None:
            if context != after["input_context"]["stream"]:
                raise AssertionError("Combat receipts crossed their baseline attachment")
            if u64(update["cursor"]["sequence"]) <= u64(after["cursor"]["sequence"]):
                continue
        key = (context["stream"], context["epoch"], update["cursor"]["sequence"])
        if key in seen:
            if seen[key] != update:
                raise AssertionError("Conflicting Outcome payloads share one stream cursor")
            continue
        seen[key] = update
        for fact in body["facts"]["facts"]:
            if fact["kind"] == "combat":
                yield fact["event"]


def decision_boundary(frame):
    """Exact disclosed/causal state, excluding only picture/transport issuance.

    Snapshot reads themselves change stream sequence and display capabilities;
    payload, history, tick, control and live action permissions must stay exact.
    """
    state = {key: value for key, value in frame["state"].items()
             if key not in ("snapshot", "revision")}
    return (state, frame["cursor"]["tick"], frame["readiness"], frame["intentions"],
            frame["history"], frame["branch"], frame["has_control"])


def resolved_events(frames, before, identity, target):
    for event in ordered_combat_events(frames, after=before):
        if (event["type"] == "ability" and event["ability"]["id"] == identity
                and event["caster"] == before["state"]["observation"]["self_target"]
                and event["target"] == target and event["strike"] == 0):
            yield event


def assert_room_gold_receipt(test, stage, depth):
    """Check a production gold audit; never feed private receipts to navigation."""
    test.assertIs(type(depth), int)
    test.assertGreater(depth, 0)
    test.assertEqual(set(stage), {"id", "version", "result"})
    test.assertIs(type(stage["version"]), int)
    test.assertEqual((stage["id"], stage["version"]), ("rogue.room_gold", 1))
    result = stage["result"]
    test.assertEqual(set(result), {"kind", "rooms"})
    test.assertEqual(result["kind"], "rooms_evaluated")
    rooms = result["rooms"]
    room_ids = [room["room"] for room in rooms]
    for identity in room_ids:
        test.assertIs(type(identity), int)
        test.assertGreater(identity, 0)
    test.assertEqual(room_ids, sorted(set(room_ids)))
    identities = []
    for room in rooms:
        test.assertEqual(set(room), {"room", "result"})
        outcome = room["result"]
        if outcome["kind"] == "chance_miss":
            test.assertEqual(outcome, {"kind": "chance_miss"})
            continue
        test.assertEqual(set(outcome), {"kind", "decision"})
        test.assertEqual(outcome["kind"], "attempt")
        decision = outcome["decision"]
        test.assertEqual(decision["content"], "rogue.gold")
        if decision["kind"] == "skipped":
            test.assertEqual(decision, {"kind": "skipped", "content": "rogue.gold",
                                       "reason": "no_eligible_cell"})
            continue
        test.assertEqual(set(decision), {"kind", "content", "location", "entity"})
        test.assertEqual(decision["kind"], "placed")
        location = decision["location"]
        test.assertEqual(set(location), {"region", "position"})
        test.assertIs(type(location["region"]), int)
        test.assertGreater(location["region"], 0)
        test.assertEqual(set(location["position"]), {"x", "y", "z"})
        for coordinate in location["position"].values():
            test.assertIs(type(coordinate), int)
        entity = decision["entity"]
        test.assertEqual(set(entity), {"kind", "id", "quantity"})
        test.assertEqual(entity["kind"], "item")
        test.assertIs(type(entity["id"]), int)
        test.assertGreater(entity["id"], 0)
        test.assertIs(type(entity["quantity"]), int)
        test.assertGreaterEqual(entity["quantity"], 2)
        test.assertLessEqual(entity["quantity"], 51 + 10 * depth)
        identities.append(("item", entity["id"]))
    test.assertEqual(len(identities), len(set(identities)))
    return identities


def first_floor_foes():
    import tomllib
    from process_harness import ROOT
    package = ROOT / "scenarios/rogue-exploration"
    manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
    actors = tomllib.loads((package / "catalog/actors.toml").read_text(encoding="utf-8"))["actors"]
    stages = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
    eligible = {choice["archetype"] for stage in stages if stage["kind"] == "actors"
                for band in stage["bands"] if band["depth"]["minimum"] <= 1
                and (band["depth"]["kind"] == "open" or 1 <= band["depth"]["maximum"])
                for choice in band["choices"]}
    return {identity: actors[identity]["creature"]["name"] for identity in eligible}


def cardinal_melee_candidate(actor):
    """Avoid diagonal corner reach; walk to a cardinal neighbor instead."""
    p = actor["position"]
    return p["z"] == 0 and abs(p["x"]) + abs(p["y"]) == 1


def disclosed_body_cells(observation):
    """Known feet plus currently disclosed own-body occurrences, never other actors."""
    cells = {(0, 0, 0)}
    cells.update(tuple(row["position"][axis] for axis in ("x", "y", "z"))
                 for row in observation["visible_actors"]
                 if row["id"] == observation["self_target"])
    return cells


def disclosed_melee_candidate(observation, actor):
    """Use a visible face-adjacent body witness, including ordinary hovering foes."""
    target = tuple(actor["position"][axis] for axis in ("x", "y", "z"))
    body = disclosed_body_cells(observation)
    return target not in body and any(sum(abs(a - b) for a, b in zip(target, own)) == 1
                                     for own in body)


def disclosed_sight_candidate(actor, distance, *, eye_height=1):
    """Bound a disclosed body occurrence by Manhattan distance from the authored eye."""
    p = actor["position"]
    return abs(p["x"]) + abs(p["y"]) + abs(p["z"] - eye_height) <= distance


def disclosed_contact_step(observation, actor):
    """Approach current contact through one disclosed, unoccupied cardinal cell.

    This bounded fixture choice never follows a stale target position or reads
    private placement. The ordinary server still admits the resulting Move.
    """
    target = actor["position"]
    if target["z"] != 0 or cardinal_melee_candidate(actor):
        raise AssertionError("Contact approach needs a distant disclosed ground target")
    directions = {(-1, 0): "west", (1, 0): "east", (0, -1): "north", (0, 1): "south"}
    occupied = {(row["position"]["x"], row["position"]["y"])
                for row in observation["visible_actors"] if row["position"]["z"] == 0
                and row["id"] != observation["self_target"]}
    distance = abs(target["x"]) + abs(target["y"])
    choices = []
    for cell in observation["visible_cells"]:
        p = cell["position"]
        offset = (p["x"], p["y"])
        if (p["z"] != 0 or cell["wall"] or offset not in directions or offset in occupied
                or (cell.get("door") and not cell["door"]["open"])):
            continue
        remaining = abs(target["x"] - p["x"]) + abs(target["y"] - p["y"])
        if remaining < distance:
            choices.append((remaining, p["x"], p["y"], cell, directions[offset]))
    if not choices:
        raise AssertionError("No disclosed open cardinal approach to the current target")
    _, _, _, cell, direction = min(choices, key=lambda row: row[:3])
    return cell, direction


def disclosed_actor_candidates(actors):
    """Deduplicate body sightings in public geometry order, preferring feet.

    Opaque handles establish identity only; neither they nor wire row order
    choose a target. Retain upper-body-only sightings so they do not imply a
    clear treatment opportunity, but never invent a ground-contact witness.
    """
    def position_key(actor):
        p = actor["position"]
        return (p["z"] != 0, abs(p["x"]) + abs(p["y"]) + abs(p["z"]),
                p["x"], p["y"], p["z"])

    seen = set()
    candidates = []
    for actor in sorted(actors, key=position_key):
        if actor["id"] not in seen:
            seen.add(actor["id"])
            candidates.append(actor)
    return candidates


def care_combat_action(foes, *, injured, contacted=(), observation=None):
    """Provoke contact, take a real injury, then clear it; None means approach.

    A disclosed hostile relation does not imply that a passive species will
    attack first. The caller records only actually resolved ordinary strikes
    in contacted, then waits at cardinal contact for real retaliation.
    """
    adjacent = disclosed_actor_candidates(foe for foe in foes
        if (cardinal_melee_candidate(foe) if observation is None
            else disclosed_melee_candidate(observation, foe)))
    if not adjacent:
        return None
    if not injured and any(foe["id"] in contacted for foe in adjacent):
        return {"type": "wait"}
    return {"type": "use_ability", "id": "basic_melee",
            "invocation": {"aim": {"kind": "actor_target", "target": adjacent[0]["id"]}, "weapon": {"kind": "default"}}}


def ground_sight_candidate(actor, distance):
    """One disclosed ground-body witness inside the preset eye's Manhattan range.

    Both authored Rogue presets have an eye at [0, 0, 1]. Observed positions
    are feet-relative, so a ground occurrence is one vertical cell from the eye.
    This conservative selection may decline another valid body-cell witness.
    """
    p = actor["position"]
    return p["z"] == 0 and abs(p["x"]) + abs(p["y"]) + 1 <= distance


def disclosed_step_action(observation, direction, *, wait_for_restraint=False):
    """Choose one paid action toward the current disclosed cardinal destination.

    This is not a movement solver: callers still select a disclosed route and
    the server validates it. A paid opening replaces, never accompanies, the
    old move; the caller must reconsider the resulting observation. Rogue
    walkthroughs may opt into waiting out their authored bear-trap condition;
    admission readiness alone does not imply that movement is permitted.
    """
    offset = next(((dx, dy, 0) for dx, dy, name in DisclosedFloorGraph.directions
                   if name == direction), None)
    if offset is None:
        raise ValueError("Expected a cardinal disclosed step")
    if wait_for_restraint and any(
            row["condition"]["id"] == "rogue.bear_trap" and row["phase"] == "active"
            and row["remaining_ticks"] is not None and u64(row["remaining_ticks"]) > 0
            for row in observation.get("combat", {}).get("own_stats", {}).get("conditions", [])):
        return {"type": "wait"}
    closed = [cell["door"] for cell in observation["visible_cells"]
              if tuple(cell["position"][axis] for axis in ("x", "y", "z")) == offset
              and not cell["wall"] and cell.get("door") and not cell["door"]["open"]]
    if len(closed) > 1:
        raise AssertionError("Ambiguous disclosed closed destination")
    if closed:
        if not closed[0]["reachable"]:
            raise AssertionError("Disclosed closed destination is not reachable")
        return {"type": "set_door", "door": closed[0]["id"], "open": True}
    return {"type": "move", "direction": direction}


class DisclosedFloorGraph:
    """Remember flat-floor links by opaque observed keys, never saved geometry.

    Coordinates are used only within one disclosed view to join cardinal cells.
    The keys carry those links across walking and an ordinary stair round trip.
    """
    directions = ((-1, 0, "west"), (1, 0, "east"),
                  (0, -1, "north"), (0, 1, "south"))
    reverse = {"west": "east", "east": "west", "north": "south", "south": "north"}

    def __init__(self):
        self.edges = {}
        self.stairs_down = set()

    def observe(self, state):
        cells = {}
        for cell in state["observation"]["visible_cells"]:
            p = cell["position"]
            if p["z"] == 0 and not cell["wall"]:
                cells[(p["x"], p["y"])] = cell["key"]
                self.edges.setdefault(cell["key"], {})
                if cell["stairs_down"]:
                    self.stairs_down.add(cell["key"])
        for (x, y), key in cells.items():
            for dx, dy, direction in self.directions:
                neighbor = cells.get((x + dx, y + dy))
                if neighbor is not None:
                    old = self.edges[key].setdefault(direction, neighbor)
                    if old != neighbor:
                        raise ValueError(f"Disclosed floor link changed at {key}: {direction}")

    def _parents(self, start, targets=None):
        from collections import deque
        parents = {start: None}
        queue = deque([start])
        while queue:
            here = queue.popleft()
            if targets is not None and here in targets:
                return parents, here
            for _, _, direction in self.directions:
                neighbor = self.edges.get(here, {}).get(direction)
                if neighbor is not None and neighbor not in parents:
                    parents[neighbor] = (here, direction)
                    queue.append(neighbor)
        return parents, None

    def route_to_any(self, start, targets):
        """Deterministic shortest disclosed route, or None when none is known."""
        parents, target = self._parents(start, targets)
        if target is None:
            return None
        route = []
        while target != start:
            parent, direction = parents[target]
            route.append((target, direction))
            target = parent
        return list(reversed(route))

    def route_through(self, start, targets):
        """Cover every required key without exploring unrelated branches.

        Prune a deterministic breadth-first tree to the required paths, then
        walk each remaining edge at most twice. Stop at the last required key.
        The precomputed plan is bounded by 2 * (disclosed key count - 1).
        """
        targets = set(targets)
        parents, _ = self._parents(start)
        missing = targets - parents.keys()
        if missing:
            raise ValueError(f"No disclosed path to required revisit keys: {sorted(missing)}")
        needed = {start}
        for target in targets:
            while target not in needed:
                needed.add(target)
                target = parents[target][0]
        children = {key: [] for key in needed}
        for key, edge in parents.items():
            if key in needed and edge is not None:
                parent, direction = edge
                children[parent].append((key, direction))
        remaining = targets - {start}
        route = []
        stack = [(start, iter(children[start]))]
        while remaining:
            here, pending = stack[-1]
            child = next(pending, None)
            if child is None:
                stack.pop()
                parent, direction = parents[here]
                route.append((parent, self.reverse[direction]))
            else:
                key, direction = child
                route.append((key, direction))
                remaining.discard(key)
                stack.append((key, iter(children[key])))
        return route


def observe_walkthrough_boundaries(view, boundaries):
    """Remember only adjacent opaque edges actually present in public frames."""
    visible = {(cell["position"]["x"], cell["position"]["y"]): cell
               for cell in view["visible_cells"] if cell["position"]["z"] == 0}
    for (x, y), cell in visible.items():
        if cell["wall"]:
            continue
        edges = boundaries.setdefault(cell["key"], {})
        for dx, dy, direction in DisclosedFloorGraph.directions:
            neighbor = visible.get((x + dx, y + dy))
            if neighbor is not None:
                if neighbor["wall"]:
                    edges[direction] = neighbor["key"]
                else:
                    edges.pop(direction, None)


def walkthrough_search_targets(graph, searched, boundaries, scanned_boundaries):
    """Prefer a paid check covering an unscanned, publicly observed opaque edge."""
    remaining = graph.edges.keys() - searched
    preferred = {key for key in remaining
                 if set(boundaries.get(key, {}).values()) - scanned_boundaries}
    return preferred or remaining


class DisclosedSearchSweep:
    """Public-only paid Search coverage retained across transport sessions."""
    def __init__(self):
        self.searched = set()
        self.boundaries = {}
        self.scanned_boundaries = set()

    def observe(self, view):
        observe_walkthrough_boundaries(view, self.boundaries)

    def dump(self):
        return {"searched": sorted(self.searched),
                "boundaries": {key: dict(sorted(edges.items()))
                               for key, edges in sorted(self.boundaries.items())},
                "scanned_boundaries": sorted(self.scanned_boundaries)}

    def record_search(self, state):
        view = state["observation"]
        here = next(cell["key"] for cell in view["visible_cells"]
                    if cell["position"] == {"x": 0, "y": 0, "z": 0})
        self.searched.add(here)
        self.scanned_boundaries.update(self.boundaries.get(here, {}).values())

    def choose(self, state, graph):
        view = state["observation"]
        here = next(cell["key"] for cell in view["visible_cells"]
                    if cell["position"] == {"x": 0, "y": 0, "z": 0})
        remaining = graph.edges.keys() - self.searched
        targets = walkthrough_search_targets(graph, self.searched, self.boundaries,
                                             self.scanned_boundaries)
        route = graph.route_to_any(here, targets)
        if route is None and targets != remaining:
            route = graph.route_to_any(here, remaining)
        if route is None:
            raise AssertionError("Search sweep exhausted its disclosed reachable cells")
        if route:
            return disclosed_step_action(view, route[0][1], wait_for_restraint=True)
        self.record_search(state)
        return {"type": "use_ability", "id": "rogue.search", "invocation": {
            "aim": {"kind": "actor_target", "target": view["self_target"]},
            "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}}
