"""The ASCII client's message area and single map through the actual server and
native window: combat reads as messages that stay until the next command, the
map merges heights into one view, and the NetHack-style screens open on demand."""
from scalar_wire import u64
import json
import shutil
import subprocess
import time
import tomllib
import unittest

from arena_matrix import toml_source
from process_harness import (
    ProcessTestCase, ROOT, ascii_input_completion, headless_fight_completion, non_graphical,
)


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
        deadline = time.monotonic() + 30

        for _ in range(32):
            view = current["state"]["observation"]
            before_tick = int(view["tick"])
            before_revision = int(current["state"]["revision"])
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
            current = self.act(player, action)
            self.assertIsNone(current["error"])
            if not headless_fight_completion(current):
                current = self.frame(player, headless_fight_completion, seconds=remaining)
            view = current["state"]["observation"]
            self.assertGreater(int(view["tick"]), before_tick, trace)
            trace.append({"direction": direction, "tick": view["tick"],
                          "scout": [actor["position"] for actor in view["visible_actors"]
                                    if actor["id"] == target["id"]]})
            attacked = False
            for line in player.transcript[start:]:
                if line.startswith("{"):
                    state = json.loads(line)["state"]
                    # Acknowledgements may repeat the preceding turn's event.
                    if int(state["revision"]) <= before_revision:
                        continue
                    events = state["observation"]["combat"]["events"]
                    attacked |= any(event["type"] == "ability"
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
        self.assertEqual(self.key(window, "escape")["state"], before)
        self.assertEqual(window.child.wait(timeout=10), 0)


if __name__ == "__main__":
    unittest.main()
