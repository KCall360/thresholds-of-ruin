"""Conservative disclosed-geometry choices for ordinary Rogue acceptance.

These flat-floor drivers do not reproduce the engine's general contact solver.
The server still validates every submitted target and reports any refusal.
"""


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
            "invocation": {"kind": "actor_target", "target": adjacent[0]["id"]}}


def ground_sight_candidate(actor, distance):
    """One disclosed ground-body witness inside the preset eye's Manhattan range.

    Both authored Rogue presets have an eye at [0, 0, 1]. Observed positions
    are feet-relative, so a ground occurrence is one vertical cell from the eye.
    This conservative selection may decline another valid body-cell witness.
    """
    p = actor["position"]
    return p["z"] == 0 and abs(p["x"]) + abs(p["y"]) + 1 <= distance


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
