"""Conservative disclosed-geometry choices for ordinary Rogue acceptance.

These flat-floor drivers do not reproduce the engine's general contact solver.
The server still validates every submitted target and reports any refusal.
"""


def cardinal_melee_candidate(actor):
    """Avoid diagonal corner reach; walk to a cardinal neighbor instead."""
    p = actor["position"]
    return p["z"] == 0 and abs(p["x"]) + abs(p["y"]) == 1


def care_combat_action(foes, *, injured):
    """Take a real injury, then clear contact; None requests disclosed approach."""
    if not foes:
        return None
    if not injured:
        return {"type": "wait"}
    target = next((foe for foe in foes if cardinal_melee_candidate(foe)), None)
    if target is None:
        return None
    return {"type": "use_ability", "id": "basic_melee",
            "invocation": {"kind": "actor_target", "target": target["id"]}}


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
