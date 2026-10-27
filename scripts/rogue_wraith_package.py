"""Authored personal-HD2 / effective-HD3 autonomous drain prefix on the retained nine-emu/W path.

Only finite placement, geometry and one empty Warrior row differ from production.
No runtime setup, source replacement, new kit, AI override or gameplay driver.
"""
import shutil
import subprocess

from arena_matrix import toml_source
from rogue_roster_package import PRODUCTION, encounter


def recovery_sources():
    manifest, region = encounter("wraith", character=1, autonomous=True)
    manifest["id"] = "rogue-wraith-earned-recovery"
    manifest["characters"][0]["creature"]["hit_dice"].append({"source": "warrior", "purchases": []})
    # Full-height closed gates keep roaming creatures in separate encounter
    # bays. The player's ordinary disclosed door actions open one bay at a time.
    # The initial two legs separate ordinary purchases from the first foe.
    path = [(1, 1)]
    bays = [(1, -1), (1, 1), (1, -1), (1, 1), (1, 1),
            (-1, 1), (-1, -1), (-1, 1), (-1, -1), (-1, 1)]
    # Fold the second half below the first, within the ordinary 32x32 bound.
    for dx, dy in [(1, 0), (0, 1)] + [direction for east, south in bays
            for direction in ((east, 0), (0, south))]:
        for _ in range(4):
            x, y = path[-1]
            path.append((x + dx, y + dy))
    floor = set(path)
    width, height = max(x for x, _ in path) + 2, max(y for _, y in path) + 2
    region.update(name="Authored personal-HD2 W recovery bays", size=[width, height, 2],
        anchors={"start": [*path[0], 0]},
        walls=[[x, y, z] for x in range(width) for y in range(height)
               for z in range(2) if (x, y) not in floor],
        doors=[{"id": 100 + bay, "at": [*path[8 + 8 * bay], 0],
                "open": False, "height": 2} for bay in range(10)],
        actors=[{"id": 3 + bay, "at": [*path[10 + 8 * bay], 0],
                 "archetype": "rogue.wraith" if bay == 8 else "rogue.emu",
                 "controller": "ai", "ai": "rogue_hostile"} for bay in range(10)])
    return manifest, region, path


def recovery_package(test):
    manifest, region, _ = recovery_sources()
    package = test.directory / "wraith-recovery"
    (package / "regions").mkdir(parents=True)
    (package / "catalog").mkdir()
    for relative in [*manifest["modules"], manifest["ruleset"]]:
        shutil.copy2(PRODUCTION / relative, package / relative)
        test.assertEqual((package / relative).read_bytes(), (PRODUCTION / relative).read_bytes())
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    log = test.directory / "wraith-validation.log"
    test.work_budget.output.paths += (log,)
    test.check_disk()
    with log.open("w", encoding="utf-8") as output:
        result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                                stdout=output, stderr=subprocess.STDOUT, timeout=30)
    test.check_disk()
    test.assertEqual(result.returncode, 0, log.read_text(encoding="utf-8")[-2000:])
    return package


def disclosed_corridor_step(frame, visited, destination=None):
    """Choose the sole public forward edge, or a caller's remembered public key."""
    directions = {(1, 0): "east", (-1, 0): "west", (0, 1): "south", (0, -1): "north"}
    candidates = []
    for cell in frame["state"]["observation"]["visible_cells"]:
        position = cell["position"]
        offset = (position["x"], position["y"])
        if position["z"] != 0 or cell["wall"] or offset not in directions:
            continue
        if (cell["key"] == destination if destination is not None else cell["key"] not in visited):
            candidates.append((cell, directions[offset]))
    if len(candidates) != 1:
        raise AssertionError("The disclosed corridor must have one selected cardinal edge")
    return candidates[0]


def drain_interval(events, *, wraith, victim, prior_earned, earned, prior_hd, hd):
    """Require the survived160 boundary before W death or another hostile input.

    CombatEventView reports the primary outcome, not the Spiritual rider's
    check. Ring mail can absorb all primary damage (Unaffected) while the rider
    drains a row, as the canonical W Service contract also requires. Exact
    personal2→1 owned paid-row loss (effective3→2) separately proves the rider;
    the event alone cannot. Callers derive personal counts from saved ownership.
    """
    if (prior_earned, prior_hd, hd) != (160, 2, 1):
        raise AssertionError("A first survived drain needs prior160/personalHD2 and personalHD1")
    contacts = [index for index, event in enumerate(events) if event["type"] == "ability"
                and event["ability"]["id"] == "rogue.energy_drain" and event["caster"] == wraith
                and event["target"] == victim and event["outcome"] in ("applied", "unaffected")
                and event["strike"] == 0]
    deaths = [index for index, event in enumerate(events)
              if event["type"] == "died" and event["actor"] == wraith]
    if not contacts:
        raise AssertionError("Actual HD loss needs the observed W Energy Drain contact")
    if earned == 160 and not deaths:
        return "separate_160_checkpoint"
    raise AssertionError("The autonomous prefix needs separate160 with W still alive")
