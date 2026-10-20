"""Bounded normally compiled discovery witnesses, not numerical balance evidence.

Production Rogue character/rules/items are retained. DC 0 guarantees disclosure;
DC 100 guarantees failure for the ordinary starting character. Geometry, finite
traps and 200-tick sleeping gas are explicitly authored test operands.
"""
import copy
import subprocess

from arena_matrix import toml_source
from utility_potions_package import fixture as production_fixture


def fixture(*, dc=0):
    data = production_fixture()
    manifest = data["manifest"]
    manifest["id"] = "rogue-discovery-contract"
    character = manifest["characters"][0]
    character["loadout"] = []
    character["known_identities"] = []
    character["turn_ticks"] = 100
    region = data["regions"][0]
    region.update(name="Discovery contract chambers", actors=[],
                  walls=[[3, y, z] for y in range(5) for z in range(2) if y != 2],
                  doors=[{"id": 1, "at": [3, 2, 0], "height": 2, "open": False,
                          "concealment": {"dc": dc}}],
                  traps=[{"id": 1, "at": [2, 2, 0], "dc": dc,
                          "kind": {"kind": "rust", "slot": "body_armor", "amount": 1}},
                         {"id": 2, "at": [4, 2, 0], "dc": dc,
                          "kind": {"kind": "sleeping_gas", "condition": "rogue.sleep", "duration": 200}}])
    return data


def observer_fixture():
    """Two production observers with explicit synthetic accuracy for one only.

    DC 100 and the first actor's +100 Awareness isolate actor-private success
    from failure; neither operand is a proposed player-facing balance value.
    """
    data = fixture(dc=100)
    manifest = data["manifest"]
    owner = manifest["characters"][0]
    peer = copy.deepcopy(owner["creature"])
    peer["name"] = "Unaware observer"
    species = copy.deepcopy(manifest["creatures"]["species"][owner["creature"]["species"]])
    species.setdefault("modifiers", []).append({"scope": "always", "op": {
        "kind": "add", "stat": {"kind": "attribute", "attribute": "awareness"}, "value": 100}})
    manifest["creatures"]["species"]["fixture.perceptive_observer"] = species
    owner["creature"]["species"] = "fixture.perceptive_observer"
    data["regions"][0]["actors"] = [{"id": 2, "at": [1, 1, 0], "turn_ticks": 100,
        "controller": "external", "creature": peer, "body": copy.deepcopy(owner["body"])}]
    return data


def package(test, *, dc=0):
    return write_package(test, fixture(dc=dc), "discovery-" + str(dc))


def generated_package(test):
    """Two lit rooms per floor with guaranteed narrow corridor mouths.

    Fixed 3x3 rooms in 7x5 slots share y=2, so their sole joining corridor
    produces two distinct secret entrances. Chance 1000/DC 0 and two traps
    are explicit test operands, not production balance or seed-search results.
    """
    data = fixture()
    manifest = data["manifest"]
    manifest["id"] = "rogue-generated-discovery-contract"
    manifest["characters"][0]["anchor"] = "dungeon/1/start"
    manifest["generation_recipes"] = {"discovery": {
        "kind": "grid_rooms", "version": 1, "room_width": [3, 3],
        "room_height": [3, 3], "extra_connections": [0, 0],
        "secret_entrances": {"chance_per_mille": 1000, "maximum": 2, "dc": 0},
        "hidden_traps": {"count": [2, 2], "dc": 0, "rust_slot": "body_armor",
                         "rust_amount": 1, "sleep_condition": "rogue.sleep",
                         "sleep_duration": 200}}}
    manifest["procedural_worlds"] = {"dungeon": {
        "kind": "grid_chain", "version": 1, "base_region": 1, "grid": [2, 1],
        "size": [7, 5, 2], "recipe": "discovery",
        "domain": {"kind": "open", "max_depth": 3}}}
    data["regions"] = []
    return write_package(test, data, "generated-discovery")


def write_package(test, data, name):
    path = test.directory / name
    (path / "regions").mkdir(parents=True)
    sources = [("scenario.toml", toml_source(data["manifest"])), ("rules.toml", data["rules"])]
    sources.extend((f"regions/{region['id']}.toml", toml_source(region))
                   for region in data["regions"])
    for filename, source in sources:
        (path / filename).write_text(source, encoding="utf-8", newline="\n")
    compiled = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", path],
                              capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(compiled.returncode, 0, compiled.stdout + compiled.stderr)
    return path


def observation(frame):
    return frame["state"]["observation"]


def traps(frame):
    return {cell["trap"]["id"]: cell["trap"] for cell in observation(frame)["visible_cells"]
            if cell.get("trap")}


def doors(frame):
    return {cell["door"]["id"]: cell["door"] for cell in observation(frame)["visible_cells"]
            if cell.get("door")}


def search(frame):
    return {"type": "use_ability", "id": "rogue.search", "invocation": {
        "aim": {"kind": "actor_target", "target": observation(frame)["self_target"]},
        "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}}
