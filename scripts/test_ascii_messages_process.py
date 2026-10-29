"""The ASCII client's message area and single map through the actual server and
native window: combat reads as messages that stay until the next command, the
map merges heights into one view, and the NetHack-style screens open on demand."""
from scalar_wire import u64
import copy
import json
import shutil
import subprocess
import time
import tomllib
import unittest
from types import SimpleNamespace
from unittest.mock import patch

from rogue_test_support import disclosed_frames, ordered_combat_events
from arena_matrix import toml_source
from process_harness import (
    ProcessTestCase, ROOT, ascii_input_completion, headless_fight_completion, non_graphical,
)


def headless_combat_boundary(test, intention=None):
    """Completed combat or permission to resume the same interrupted paid work.

    This fixture policy does not make a paused intention completed. The next
    bounded input must explicitly resume it, rather than replace its attack.
    """
    def stopped(frame):
        nonlocal intention
        if frame.get("error") is not None:
            return True
        view = (frame.get("state") or {}).get("observation")
        if view is None:
            return False
        pending = frame["intentions"]
        test.assertLessEqual(len(pending), 1, "Ambiguous combat work")
        if pending:
            status = pending[0]
            test.assertIn(status["phase"], ("queued", "started", "paused"))
            if intention is None:
                intention = status["intention"]
            test.assertEqual(status["intention"], intention, "Combat work changed identity")
        if headless_fight_completion(frame):
            return True
        combat = view["combat"]
        return (view["ready"] and not combat["preparation_active"]
                and combat["preparation_remaining"] is not None
                and len(pending) == 1 and pending[0]["phase"] == "paused"
                and frame["readiness"]["resume"] == [intention])

    return stopped


class AsciiMessageProcesses(ProcessTestCase):
    graphical = True

    def combat_package(self):
        # Preserve the shipped creatures, equipment and species-owned behavior.
        # Their explore/wake policy may leave the distant dungeon doorway before
        # combat starts. A small closed chamber makes continuous sight a fixture
        # property without changing aggression, retreat or the ordinary fight.
        package = self.directory / "ascii-message-combat"
        shutil.copytree(ROOT / "scenarios/first-dungeon", package)
        manifest = package / "scenario.toml"
        document = tomllib.loads(manifest.read_text(encoding="utf-8"))
        document["id"] = "ascii-message-combat"
        del document["objective"]
        manifest.write_text(toml_source(document), encoding="utf-8", newline="\n")
        for region in (package / "regions").glob("*.toml"):
            region.unlink()
        (package / "regions/1.toml").write_text(toml_source({
            "id": 1, "name": "Combat chamber", "size": [3, 1, 2],
            "chamber": True, "gravity": [0, 0, -1],
            "anchors": {"exit": [0, 0, 0]},
            "actors": [{"id": 3, "at": [2, 0, 0], "archetype": "dungeon.scout",
                        "controller": "ai", "ai": "tactical", "turn_ticks": 80}],
            "items": [{"id": 101, "at": [0, 0, 0], "archetype": "dungeon.delver_blade",
                       "carried_by": 1, "equipped_slot": 0}],
        }), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=20)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        return package

    @non_graphical
    def test_combat_fixture_keeps_scout_visible_until_defeated(self):
        # This exercises the native test's combat precondition through the real
        # server without replacing its separate native presentation coverage.
        self.server(scenario=self.combat_package())
        player, current = self.client(omit_memory=True)
        self.assertFalse(current["state"]["wizard_game"])
        directions = {(0, -1): "north", (1, -1): "north_east", (1, 0): "east",
                      (1, 1): "south_east", (0, 1): "south", (-1, 1): "south_west",
                      (-1, 0): "west", (-1, -1): "north_west"}
        trace = []
        attack_turns = 0
        death = None
        paused = None
        deadline = time.monotonic() + 30

        for _ in range(32):
            view = current["state"]["observation"]
            before_tick = int(view["tick"])
            target = next((actor for actor in view["visible_actors"]
                           if actor["name"] == "ruin scout" and actor["position"]["z"] == 0), None)
            self.assertIsNotNone(target, f"Scout disappeared before combat completed: {trace!r}")
            self.assertFalse(view["combat"]["dead"], trace)
            remaining = deadline - time.monotonic()
            self.assertGreater(remaining, 0, trace)
            x, y = target["position"]["x"], target["position"]["y"]
            direction = directions[((x > 0) - (x < 0), (y > 0) - (y < 0))]
            # The native direction key chooses Attack for an adjacent creature;
            # headless actions spell out that same ordinary server operation.
            action = ({"type": "use_ability", "id": "basic_melee",
                       "invocation": {"aim": {"kind": "actor_target", "target": target["id"]}, "weapon": {"kind": "default"}}}
                      if max(abs(x), abs(y)) == 1 else {"type": "move", "direction": direction})
            start = len(player.transcript)
            if paused is None:
                current = self.act(player, action)
            else:
                # Resume is an input in this same 32-input/30-second budget,
                # not another attack or an unbounded automatic retry.
                player.write(json.dumps({"type": "resume_intention"}))
                current = self.frame(player, lambda frame: frame["type"] == "ready",
                                     seconds=remaining)
            self.assertIsNone(current["error"])
            stopped = headless_combat_boundary(self, paused)
            if not stopped(current):
                remaining = deadline - time.monotonic()
                self.assertGreater(remaining, 0, trace)
                current = self.frame(player, stopped, seconds=remaining)
            self.assertIsNone(current["error"])
            paused = (current["intentions"][0]["intention"]
                      if current["intentions"] else None)
            current = self.picture_after(player, current)
            view = current["state"]["observation"]
            self.assertGreater(int(view["tick"]), before_tick, trace)
            trace.append({"direction": direction, "tick": view["tick"],
                          "scout": [actor["position"] for actor in view["visible_actors"]
                                    if actor["id"] == target["id"]]})
            events = list(ordered_combat_events(disclosed_frames(player, start)))
            attacked = any(event["type"] == "ability"
                           and event["caster"] == view["self_target"]
                           and event["target"] == target["id"] for event in events)
            death = next((event for event in events if event["type"] == "died"
                          and event["actor"] == target["id"]), death)
            attack_turns += attacked
            if death is not None:
                break
        self.assertIsNotNone(death, trace)
        self.assertFalse(current["state"]["observation"]["combat"]["dead"], trace)
        self.assertFalse(any(actor["id"] == target["id"]
                             for actor in current["state"]["observation"]["visible_actors"]), trace)
        # Native message-log coverage requires a combat turn before the fatal one.
        self.assertGreaterEqual(attack_turns, 2, trace)

    def window(self):
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"])
        frame = self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"])
        return window, frame

    def test_fight_messages_persist_and_the_single_map_names_creatures(self):
        self.server(scenario=self.combat_package())
        window, initial = self.window()
        tiles = initial["map_tiles"]
        # One @, and the player's head (disclosed one cell up) is no creature.
        self.assertEqual([t["glyph"] for t in tiles].count("@"), 1)
        self.assertEqual(len({(t["position"]["x"], t["position"]["y"]) for t in tiles}), len(tiles))
        scout = next(t for t in tiles if t["kind"] == "creature")
        self.assertEqual(scout["glyph"], "&")
        health = initial["state"]["observation"]["combat"]
        self.assertEqual(u64(health["own_stats"]["hit_dice"]["total"]), 4)
        self.assertEqual(initial["status_lines"][1].split()[0],
                         f"HP:{u64(health['hp'])}({u64(health['max_hp'])})")

        # Follow the disclosed scout, including its ordinary low-Health retreat.
        # The closed chamber keeps the encounter visible without replacing AI.
        # Wait through intention completion and recovery before the next input.
        # Keep the bounded input/deadline budget for stale-revision retries,
        # rather than assuming the first hit kills or an acknowledgement ends it.
        # Every turn's messages are only what happened since that command.
        directions = {(0, -1): "up", (1, -1): "north_east", (1, 0): "right",
                      (1, 1): "south_east", (0, 1): "down", (-1, 1): "south_west",
                      (-1, 0): "left", (-1, -1): "north_west"}
        killed = None
        seen = []
        trace = []
        current = initial
        deadline = time.monotonic() + 30
        for _ in range(32):
            view = current["state"]["observation"]
            target = next((a for a in view["visible_actors"]
                           if a["name"] == "ruin scout" and a["position"]["z"] == 0), None)
            self.assertIsNotNone(target, f"Scout disappeared without death narration: {trace!r}")
            self.assertFalse(view["combat"]["dead"], trace)
            remaining = deadline - time.monotonic()
            self.assertGreater(remaining, 0, f"Fight input deadline: {trace!r}")
            x, y = target["position"]["x"], target["position"]["y"]
            key = directions[((x > 0) - (x < 0), (y > 0) - (y < 0))]
            # Use the same native automation event and completion predicate as
            # key(), but cap this wait by the remaining whole-fight deadline.
            window.write(json.dumps({"type": "key", "key": key}))
            frame = self.ascii_frame(window, ascii_input_completion(key, wait_for_ready=True),
                                     seconds=remaining)
            current = frame
            turn = frame["messages"]["turn"]
            self.assertNotIn("You can act again", turn)
            self.assertNotIn("You move east", turn)
            self.assertNotIn("HP ", turn)
            self.assertNotIn("You prepare to use Attack.", turn)
            self.assertNotIn("You used Attack on", turn)
            seen.append(turn)
            observation = frame["state"]["observation"]
            trace.append({"key": key, "input_done": frame.get("input_done"),
                          "frame": frame["frame"], "tick": observation["tick"],
                          "ready": observation["ready"], "busy": frame["busy"],
                          "status": frame.get("status"),
                          "intentions": [i["phase"] for i in (frame.get("intentions") or [])],
                          "scout": [a["position"] for a in observation["visible_actors"]
                                    if a["name"] == "ruin scout" and a["position"]["z"] == 0],
                          "turn": turn})
            if "The ruin scout died." in turn:
                killed = frame
                break
        self.assertIsNotNone(killed, {"messages": seen, "input_trace": trace})
        self.assertIn("You hit the ruin scout.", killed["messages"]["turn"])
        self.assertEqual(killed["messages"]["shown"][0][:24], killed["messages"]["turn"][:24])
        # Earlier turns went to the log.
        self.assertTrue(any("ruin scout" in turn for turn in killed["messages"]["log"]))

        # Look costs no time and names what's there.
        before = killed["state"]
        self.key(window, "look")
        looked = self.key(window, "look")
        self.assertEqual(looked["state"], before)
        self.assertEqual(looked["messages"]["turn"].split("  ")[-1], "That's you.")

        # The inventory and message log are screens, not panels.
        shown = self.key(window, "inventory")
        self.assertTrue(shown["screen"]["inventory"])
        closed = self.key(window, "inventory")
        self.assertFalse(closed["screen"]["inventory"])
        logged = self.key(window, "message_log")
        self.assertEqual(logged["screen"]["message_log"], 0)
        self.assertEqual(self.key(window, "escape")["screen"]["message_log"], None)
        self.assertEqual(self.key(window, "save_quit")["state"], before)
        self.assertEqual(window.child.wait(timeout=10), 0)


class HeadlessCombatFixtureContracts(unittest.TestCase):
    """Run the actual visibility fixture against ordered public publications."""

    def run_fixture(self, mode="complete", order="lifecycle_first"):
        case = AsciiMessageProcesses("test_combat_fixture_keeps_scout_visible_until_defeated")
        player = SimpleNamespace(transcript=[])
        inputs, pending, clock = [], [], [0.0]
        state = {"resumed": False, "last_tick": 0, "waits": [], "sequence": 0}
        caster, scout = "a_" + "11" * 32, "a_" + "22" * 32

        def frame(tick, *, phase=None, remaining=None, active=False, ready=True,
                  resume=(), identity="paid-attack", events=(), dead_scout=False, kind="update"):
            value = {"type": kind, "error": None,
                    "state": {"wizard_game": False, "revision": str(tick), "observation": {
                        "tick": str(tick), "ready": ready, "self_target": caster,
                        "visible_actors": [] if dead_scout else [{
                            "id": scout, "name": "ruin scout", "position": {"x": 1, "y": 0, "z": 0}}],
                        "combat": {"dead": False, "preparation_active": active,
                                   "preparation_remaining": remaining, "events": []}}},
                    "intentions": [] if phase is None else [{"intention": identity, "phase": phase}],
                    "readiness": {"admission": True, "resume": list(resume),
                                  "cancel": [identity] if phase else [], "revision": str(tick)}}
            if events:
                # Real headless output retains the received Outcome independently
                # of its picture. Keep complete event-time fact fields here: an
                # observation's combat.events is never a completion receipt.
                value["message"] = {"type": "update", "update": {
                    "context": {"stream": "mock-combat", "epoch": "0"},
                    "actor": "1", "branch": "mock-branch",
                    "cursor": {"sequence": "0", "tick": str(tick)},
                    "body": {"type": "outcome", "visual_changed": True, "event": None,
                             "facts": {"self_target": caster,
                                       "status": {"hp": "50", "max_hp": "50", "dead": False,
                                                  "victory": False, "terminal": False,
                                                  "sequence": None, "preparation_remaining": remaining,
                                                  "preparation_active": active, "recovery_remaining": "0"},
                                       "actor_names": [{"actor": scout, "name": "ruin scout", "asset": None,
                                                        "current_disclosed_appearance": None if dead_scout else {
                                                            "name": "ruin scout", "asset": None}}],
                                       "facts": [{"kind": "combat", "event": event,
                                                  "basic_attack": event["type"] == "ability"}
                                                 for event in events]}}}}
            return value

        hit = {"type": "ability", "caster": caster, "target": scout, "strike": None,
               "ability": {"id": "basic_melee", "name": "Basic Melee"}, "outcome": "applied"}
        death = {"type": "died", "actor": scout}

        def publish(value):
            state["sequence"] += 1
            if "message" in value:
                update = value["message"]["update"]
                update["cursor"]["sequence"] = str(state["sequence"])
                if mode == "picture_only":
                    value["state"]["observation"]["combat"]["events"] = [
                        fact["event"] for fact in update["body"]["facts"]["facts"]]
                    del value["message"]
            state["last_tick"] = int(value["state"]["observation"]["tick"])
            if mode == "elapsed" and value["readiness"]["resume"] == ["paid-attack"]:
                clock[0] = 20
            player.transcript.append(json.dumps(value))
            return value

        def act(_player, action):
            self.assertIs(_player, player)
            self.assertEqual(action["type"], "use_ability")
            self.assertEqual(action["id"], "basic_melee")
            self.assertEqual(action["invocation"]["aim"]["target"], scout)
            self.assertFalse(pending, "A new attack replaced unresolved paid work")
            inputs.append("attack")
            if mode == "bound":
                self.assertEqual(len(inputs), 1, "Paused work must be resumed, not attacked again")
                return publish(frame(1, phase="paused", remaining="41", resume=("paid-attack",)))
            if len(inputs) == 1:
                if mode == "deadline":
                    clock[0] = 31
                elif mode == "elapsed":
                    clock[0] = 5
                return publish(frame(100, events=(hit,)))
            self.assertEqual(len(inputs), 2, "Only the original two attacks may be admitted")
            if mode in ("complete", "picture_only"):
                return publish(frame(200, events=(hit, death), dead_scout=True))
            if mode == "elapsed":
                clock[0] = 12
            paused = frame(102, phase="paused", remaining="41", resume=("paid-attack",))
            if mode == "already_paused":
                return publish(paused)
            observation = frame(102, phase="started", remaining="41")
            lifecycle = frame(102, phase="paused", remaining="41")
            if mode == "wrong_identity":
                paused["intentions"][0]["intention"] = "other-paid-work"
                paused["readiness"]["resume"] = ["other-paid-work"]
            elif mode == "ambiguous":
                paused["intentions"].append({"intention": "other-paid-work", "phase": "paused"})
            elif mode == "wrong_permission":
                paused["readiness"]["resume"] = ["other-paid-work"]
            if order == "permission_first":
                early_permission = copy.deepcopy(observation)
                early_permission["readiness"]["resume"] = ["paid-attack"]
                pending.extend((observation, early_permission, paused))
            else:
                pending.extend((observation, lifecycle, paused))
            return publish(frame(101, phase="started", remaining="43", active=True, ready=False))

        def write(line):
            self.assertEqual(json.loads(line), {"type": "resume_intention"})
            self.assertFalse(pending, "Resume preceded its ordered permission boundary")
            latest = json.loads(player.transcript[-1])
            self.assertEqual(latest["intentions"], [{"intention": "paid-attack", "phase": "paused"}])
            self.assertEqual(latest["readiness"]["resume"], ["paid-attack"])
            inputs.append("resume")
            state["resumed"] = True
            tick = state["last_tick"]
            pending.append(frame(tick, phase="queued", remaining="41", kind="ready"))
            if mode == "bound":
                pending.append(frame(tick + 1, phase="paused", remaining="40", resume=("paid-attack",)))
            else:
                pending.extend((frame(103, phase="started", remaining="40", active=True, ready=False),
                                frame(200, events=(hit, death), dead_scout=True)))

        def wait(_player, predicate, seconds=15):
            self.assertIs(_player, player)
            self.assertGreater(seconds, 0)
            self.assertLessEqual(seconds, 30 - clock[0], "Whole-fight deadline was reset")
            state["waits"].append(seconds)
            while pending:
                value = publish(pending.pop(0))
                if predicate(value):
                    return value
            case.fail("Paid work needs its explicit authorized Resume, not another passive wait")

        player.write = write
        case.combat_package = lambda: "unchanged-combat-package"
        case.server = lambda **kwargs: self.assertEqual(kwargs, {"scenario": "unchanged-combat-package"})
        case.client = lambda **kwargs: (player, frame(0))
        case.act, case.frame = act, wait
        self.inputs, self.publications, self.fixture_state = inputs, player.transcript, state
        with patch(__name__ + ".time.monotonic", side_effect=lambda: clock[0]):
            case.test_combat_fixture_keeps_scout_visible_until_defeated()

    def test_already_completed_baseline_needs_two_attacks_and_no_resume(self):
        self.run_fixture()
        self.assertEqual(self.inputs, ["attack", "attack"])

    def test_ordered_outcomes_not_picture_events_certify_combat_completion(self):
        self.run_fixture()
        frames = [json.loads(line) for line in self.publications]
        self.assertTrue(all(not frame["state"]["observation"]["combat"]["events"] for frame in frames))
        self.assertEqual([event["type"] for event in ordered_combat_events(frames)],
                         ["ability", "ability", "died"])
        sequences = [frame["message"]["update"]["cursor"]["sequence"]
                     for frame in frames if "message" in frame]
        self.assertEqual(len(sequences), len(set(sequences)))
        with self.assertRaisesRegex(AssertionError, "Scout disappeared before combat completed"):
            self.run_fixture("picture_only")
        self.assertEqual(self.inputs, ["attack", "attack"])

    def test_actual_fixture_resumes_only_after_matching_lifecycle_and_permission(self):
        for order in ("lifecycle_first", "permission_first"):
            with self.subTest(order=order):
                self.run_fixture("interrupted", order)
                self.assertEqual(self.inputs, ["attack", "attack", "resume"])

    def test_already_published_pause_resumes_without_waiting_for_nonexistent_completion(self):
        self.run_fixture("already_paused")
        self.assertEqual(self.inputs, ["attack", "attack", "resume"])

    def test_changed_identity_ambiguous_work_and_unrelated_permission_never_resume(self):
        for mode, message in (("wrong_identity", "changed identity"),
                              ("ambiguous", "Ambiguous combat work"),
                              ("wrong_permission", "explicit authorized Resume")):
            with self.subTest(mode=mode), self.assertRaisesRegex(AssertionError, message):
                self.run_fixture(mode)
            self.assertEqual(self.inputs, ["attack", "attack"])

    def test_resumes_are_charged_to_the_original_32_input_ceiling(self):
        with self.assertRaisesRegex(AssertionError, "unexpectedly None"):
            self.run_fixture("bound")
        self.assertEqual(self.inputs, ["attack"] + ["resume"] * 31)

    def test_original_absolute_deadline_cannot_be_restarted(self):
        with self.assertRaisesRegex(AssertionError, "not greater than 0"):
            self.run_fixture("deadline")
        self.assertEqual(self.inputs, ["attack"])

    def test_interruption_and_resume_share_the_remaining_original_deadline(self):
        self.run_fixture("elapsed")
        self.assertEqual(self.inputs, ["attack", "attack", "resume"])
        self.assertEqual(self.fixture_state["waits"], [18, 10, 10])


if __name__ == "__main__":
    unittest.main()
