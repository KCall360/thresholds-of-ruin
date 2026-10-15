"""Ordinary, renamed status content and transport helpers for process acceptance.

The small timings are fixture operands, not Rogue balance. Status applications
have no damage effect, so interruption cannot accidentally pass through damage.
"""
import copy
import json
import shutil
import sqlite3
import struct
import subprocess
import threading
import tomllib
from contextlib import closing

from arena_matrix import toml_source
from checkpoint_payload import decode_checkpoint_payload
from process_harness import ROOT
from ruleset_sources import load_rules
from stream_relay import StreamRelay, exact


STILLNESS = "fixture_crystal_pause"
LAPSE = "fixture_brief_lapse"
WORK = "fixture_patient_work"
WOUND = "fixture_wound_subject"
STATUS_ABILITIES = {STILLNESS: "fixture_impose_crystal", LAPSE: "fixture_impose_lapse"}
SELF_ABILITIES = {STILLNESS: "fixture_self_crystal", LAPSE: "fixture_self_lapse"}
DURATIONS = {STILLNESS: 200, LAPSE: 75}


def observation(frame):
    return frame["state"]["observation"]


def combat(frame):
    return observation(frame)["combat"]


def actor_creature(saved, actor=1):
    return saved["actors"][str(actor)]["combat"]["creature"]


def actor_clock(saved, actor=1):
    """Read the saved actor-active clock, including a loaded frozen actor."""
    return saved["frozen_at"].get(str(actor), saved["tick"])


def condition_remaining(saved, identity, actor=1):
    """Project one retained condition's clock without rewriting raw save facts.

    Partial progress is lazy: remaining is based at active_since, not game.tick.
    The backend settles phase transitions at their due deadlines; these fixtures
    inspect a retained active/protection phase before its next deadline.
    """
    status = next(row for row in actor_creature(saved, actor)["conditions"]
                  if row["condition"] == identity)
    elapsed = actor_clock(saved, actor) - saved["actors"][str(actor)]["active_since"]
    if not 0 <= elapsed < status["remaining"]:
        raise AssertionError("Condition phase is not current at its saved actor clock")
    return status["remaining"] - elapsed


def checkpoint(test, client):
    """Read raw saved facts; callers must start with --checkpoint-interval 1.

    Save flushes accepted records. It does not force a new checkpoint or settle
    lazy creature clocks, so keep actor anchors and frozen stamps intact.
    """
    test.assertIsNone(test.request(client, {"type": "save"})["error"])
    with closing(sqlite3.connect(test.save.resolve().as_uri() + "?mode=ro", uri=True)) as database:
        row = database.execute("SELECT sequence, payload FROM checkpoint").fetchone()
        test.assertIsNotNone(row, "Checkpoint fixture requires --checkpoint-interval 1")
        sequence, payload = row
        test.assertEqual(database.execute(
            "SELECT count(*) FROM journal WHERE sequence > ?", (sequence,)).fetchone()[0], 0)
    value = json.loads(decode_checkpoint_payload(payload))
    game, shared = value["game"], value["shared"]
    lifecycle = (shared["lifecycles"][game["lifecycle"]]
                 if game.get("lifecycle") is not None else None)
    return {"tick": game["tick"], "rules_rng": game["rules_rng"],
            "actors": copy.deepcopy(shared["actors"][game["actors"]]),
            "items": copy.deepcopy(shared["items"][game["items"]]),
            "frozen_at": copy.deepcopy(lifecycle["stamps"] if lifecycle else {})}


def prepare(test, name="status-condition", *, prefix=False, peer=True, streaming=False):
    """Compile normally authored zero-damage restrictions and paid self work.

    Two externally controlled, allied actors provide ordinary input boundaries.
    Work holds one of two carried bandages until its first of three strikes.
    A 25-tick first strike exposes a paid prefix; otherwise preparation is 300.
    Both actors have every fixture grant so either can be the subject or causer.
    Streaming uses an autonomous patient: authored external actors permanently
    own observing reference points and cannot freeze after a client disconnects.
    """
    package = test.directory / name
    shutil.copytree(ROOT / "scenarios/two-room", package)
    compiler = test.bin / ("tor-scenario" + test.suffix)
    rules = load_rules(package / "rules.toml", compiler)
    rules["nutrition"] = {"kind": "enabled", "initial": 1000, "maximum": 2000,
                          "hungry_below": 7, "weak_below": 3, "starve_at": -3,
                          "unit_ticks": 25,
                          "hungry_recovery": {"kind": "slowed", "multiplier": 2},
                          "weak_recovery": {"kind": "stopped"},
                          "regeneration_cost": {"denominator": 1, "health": 0, "stamina": 0, "focus": 0, "mana": 0},
                          "empty_recovery": {"kind": "stopped"}}
    rules["health_recovery"] = {"base_period": 1000, "minimum_period": 1000,
                                "reduction_per_hd": 0}
    for resource in rules["resources"]:
        resource["recovery_period"] = 100000

    def strike(effects, offset=0):
        return {"offset": offset, "on_success": effects, "on_failure": [],
                "descriptors": [], "riders": []}

    def ability(identity, name, reach, effects, *, prepare_ticks=50):
        return {"id": identity, "name": name, "tags": [], "basic_attack": False,
                "timing": {"source": {"kind": "fixed", "prepare": prepare_ticks, "recover": 25},
                           "scaled_by": "speed"},
                "reach": {"kind": reach, **({"range": 6} if reach == "sight" else {})},
                "strikes": {"kind": "authored", "strikes": [strike(effects)]},
                "interaction": "hostile_to_target", "ai": {"priority": 0, "hostile": False, "minimum_resource_percent": 0,
                       "range_preference": "any"}}

    for identity, display, expiry in (
            (STILLNESS, "Crystal Pause", {"kind": "protect", "duration": 400}),
            (LAPSE, "Brief Lapse", {"kind": "remove"})):
        rules["conditions"].append({
            "id": identity, "family": identity, "name": display, "lifecycle": "timed",
            "timing": {"kind": "actor_active", "on_expiry": expiry},
            "restrictions": ["voluntary_actions"], "preparation": "pause",
            "stacking": {"kind": "keep"}, "source_scoped": False,
            "modifiers": [], "descriptors": [], "capacity": "reject"})
        effect = {"kind": "apply_condition", "condition": identity, "duration": DURATIONS[identity]}
        rules["abilities"].append(ability(STATUS_ABILITIES[identity], "Impose " + display,
                                          "sight", [copy.deepcopy(effect)]))
        rules["abilities"].append(ability(SELF_ABILITIES[identity], "Self " + display,
                                          "self_only", [copy.deepcopy(effect)]))
    work = ability(WORK, "Patient Work", "self_only", [], prepare_ticks=25 if prefix else 300)
    work.update(cost={"resource": "stamina", "start": 1, "resolution": 2}, cooldown=1200,
                supply_costs=[{"supply": "bandage", "quantity": 1, "access": "carried_only",
                               "spend_at": "first_resolution"}])
    work["strikes"]["strikes"] = [strike([
        {"kind": "heal", "amount": {"kind": "fixed", "amount": 1}}], offset)
        for offset in (0, 100, 200)]
    if streaming:
        # Ordinary healing AI selects this same paid work when wounded below
        # half Health. It outranks universal care and retains its finite hold.
        work["ai"]["priority"] = 1000
        wound = ability(WOUND, "Wound Subject", "sight", [{"kind": "damage", "damage": {
            "kind": "components", "components": [{"category": "vital", "amount": {
                "kind": "fixed", "amount": 110}}]}}], prepare_ticks=25)
        wound["ai"]["hostile"] = True
        rules["abilities"].append(wound)
    rules["abilities"].append(work)
    (package / "rules.toml").write_text(toml_source(rules), encoding="utf-8", newline="\n")

    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest["id"] = name
    manifest["factions"] = {"subjects": []}
    manifest["archetypes"] = {
        "bandage": {"name": "fixture bandage", "identity": "fixture_bandage", "class": "misc",
                    "stackable": True, "supply": "bandage"},
        "injury": {"name": "fixture injury", "class": "potion", "consumable": {"effects": [
            {"kind": "damage", "damage": {"kind": "components", "components": [
                {"category": "vital", "amount": {"kind": "fixed", "amount": 5}}]}}]}}}
    grants = [WORK, *STATUS_ABILITIES.values(), *SELF_ABILITIES.values()]
    if streaming:
        grants.append(WOUND)
        manifest["ai_profiles"] = {"patient": {"care": True, "equipment": True, "loot": True, "flee_percent": 0}}
    manifest["creatures"] = {"species": {"subject": {
        "kind": "humanoid", "behavior": {"disposition":"mean","idle":"explore","memory_ticks":1000}, "attributes": dict.fromkeys(
            ("strength", "speed", "intellect", "willpower", "awareness", "presence"), 0),
        "melee": {"skill": "heavy_weaponry", "bonus": 0, "wind_up": 100, "recovery": 100,
                  "strikes": [strike([{"kind": "heal", "amount": {"kind": "fixed", "amount": 0}}])]},
        "modifiers": [*({"scope": "always", "op": {"kind": "grant_ability", "ability": value}}
                        for value in grants),
                      {"scope": "always", "op": {"kind": "add", "stat": {"kind": "health"}, "value": 200}},
                      {"scope": "always", "op": {"kind": "set", "stat": {
                          "kind": "resource_max", "resource": "stamina"}, "value": 20}}]}}}

    def creature(name):
        return {"species": "subject", "name": name, "faction": "subjects", "binding": "intellect",
                "starting_skills": ["heavy_weaponry", "athletics"], "restoration": "authored",
                "nutrition": {"kind": "required"}, "hit_dice": [{"source": "racial"}]}

    manifest["characters"][0].update(creature=creature("first subject"), turn_ticks=25)
    path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    region["actors"] = ([{"id": 2, "at": [2, 1, 0], "turn_ticks": 25,
                          "controller": "ai" if streaming else "external",
                          **({"ai": "patient"} if streaming else {}),
                          "creature": creature("second subject")}] if peer else [])
    region["items"] = []
    for actor in (1, 2) if peer else (1,):
        for identity, archetype, quantity in ((10 + actor, "bandage", 2), (20 + actor, "injury", 1)):
            region["items"].append({"id": identity, "at": [actor, 1, 0], "carried_by": actor,
                                    "archetype": archetype, "quantity": quantity})
    path.write_text(toml_source(region), encoding="utf-8", newline="\n")
    # No unrelated objects or archetype dependencies remain in the second room.
    gallery = {"id": 2, "name": "Long gallery", "size": [32, 3, 2], "chamber": True,
               "gravity": [0, 0, -1], "anchors": {"landing": [0, 1, 0], "exit": [31, 1, 0]},
               "portals": [{"at": [0, 1, 0], "direction": "west", "to": "1/hall", "height": 2}]}
    if streaming:
        gallery["portals"].append({"at": [31, 1, 0], "direction": "east", "to": "3/landing", "height": 2})
        remote = {"id": 3, "name": "Distant room", "size": [32, 3, 2], "chamber": True,
                  "gravity": [0, 0, -1], "anchors": {"landing": [0, 1, 0]},
                  "portals": [{"at": [0, 1, 0], "direction": "west", "to": "2/exit", "height": 2}]}
        (package / "regions/3.toml").write_text(toml_source(remote), encoding="utf-8", newline="\n")
    (package / "regions/2.toml").write_text(toml_source(gallery), encoding="utf-8", newline="\n")
    validated = subprocess.run([compiler, "validate", package], capture_output=True,
                               text=True, encoding="utf-8", timeout=30)
    test.assertEqual(validated.returncode, 0, validated.stderr)
    return package


class AbilityRetryRelay(StreamRelay):
    """Retain a real client ability frame for an exact same-request retry."""
    def __init__(self, upstream, ability=WORK):
        self.ability = ability
        self.send_lock = threading.Lock()
        self.captured = threading.Event()
        self.saved_frame = None
        self.request_id = None
        super().__init__(upstream)

    def forward(self, source, target):
        try:
            header = bytearray()
            while not header.endswith(b"\r\n\r\n"):
                header.extend(exact(source, 1))
                if len(header) > 65536:
                    raise ValueError("Oversized client handshake")
            with self.send_lock:
                target.sendall(header)
            while True:
                prefix = exact(source, 2)
                length = prefix[1] & 127
                extra = b""
                if length in (126, 127):
                    extra = exact(source, 2 if length == 126 else 8)
                    length = struct.unpack("!H" if length == 126 else "!Q", extra)[0]
                if length > 16 * 1024 * 1024 or not prefix[1] & 128:
                    raise ValueError("Invalid test client frame")
                mask = exact(source, 4)
                payload = exact(source, length)
                frame = prefix + extra + mask + payload
                if prefix[0] == 0x81 and self.saved_frame is None:
                    message = json.loads(bytes(value ^ mask[index % 4] for index, value in enumerate(payload)))
                    command = (message.get("request") or {}).get("command") or {}
                    action = command.get("action") or {}
                    if action.get("type") == "use_ability" and action.get("id") == self.ability:
                        self.saved_frame, self.request_id = frame, message["request_id"]
                        self.captured.set()
                with self.send_lock:
                    target.sendall(frame)
        except (EOFError, OSError):
            pass
        except Exception as error:
            self.errors.append(error)
        finally:
            self.close_sockets()

    def retry(self, frame=None):
        if frame is None:
            if not self.captured.wait(15):
                raise AssertionError("The real client did not send the selected ability")
            frame = self.saved_frame
        with self.send_lock:
            self._server_socket.sendall(frame)
