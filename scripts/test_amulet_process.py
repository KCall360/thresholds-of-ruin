"""Unique retrieval and deliberate surface escape through the real clients.

Small, lit, enemy-free floors isolate objective mechanics. The actual selected
Mage keeps its ordinary build and kit, with an explicit nutrition exemption.
This is not an earned-descent or production survival/balance claim. Navigation
uses current disclosed cells only; private save evidence is audited afterwards.
"""
from collections import deque
import copy
import json
import shutil
import sqlite3
import subprocess
import time
import unittest
from contextlib import closing

from arena_matrix import toml_source
from checkpoint_payload import decode_checkpoint_payload
from process_harness import (
    AdventureProcess, DIRECTIONS, ProcessTestCase, SPECTATOR_TOKEN, inspect_save, non_graphical,
    settle_rogue_action, underfoot,
)
from process_work_budget import ProcessBudgetExceeded, ProcessWorkBudget
from rogue_roster_package import PRODUCTION, encounter_sources, unrelated_fallback


AMULET = "Amulet of Yendor"
ACTOR = 2


def observation(frame):
    return frame["state"]["observation"]


class AmuletProcesses(ProcessTestCase):
    graphical = True
    # Independent ceilings include all requests, observed participant work,
    # emitted process logs and total wall time, not just individual route loops.
    # First real run used 32.2 MiB at 105 requests, after 23/25 ascent steps.
    # Current history is repeated in frames even with map memory omitted. These
    # measured diagnostic allowances retain all work/time bounds and assertions.
    limits = {
        "test_unique_identity_revisits_source_free_restore_and_text_surface_escape":
            (384, 40000, 64 * 1024 * 1024, 240),
        "test_native_up_escapes_only_from_the_disclosed_surface_endpoint":
            (256, 25000, 48 * 1024 * 1024, 180),
    }

    def setUp(self):
        super().setUp()
        requests, ticks, output, self.wall_seconds = self.limits[self._testMethodName]
        self.started = time.monotonic()
        self.work_budget = ProcessWorkBudget(
            requests=requests, actor_ticks=ticks, transcript_bytes=output)

    def remaining(self, seconds=15):
        elapsed = time.monotonic() - self.started
        if elapsed >= self.wall_seconds:
            raise ProcessBudgetExceeded("wall_seconds", round(elapsed, 3), self.wall_seconds,
                                        last=self.work_budget.last)
        self.work_budget.check()
        return min(seconds, self.wall_seconds - elapsed)

    def tearDown(self):
        report = self.work_budget.report()
        report["limits"]["wall_seconds"] = self.wall_seconds
        report["used"]["wall_seconds"] = round(time.monotonic() - self.started, 3)
        (self.directory / "work-budget.json").write_text(
            json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("Process budget " + self.id() + ": " + json.dumps(report, sort_keys=True), flush=True)
        if not self._failed():
            self.remaining()
        super().tearDown()

    def launch(self, *args, **kwargs):
        self.remaining()
        process = super().launch(*args, **kwargs)
        self.work_budget.output.paths += (
            process.stdout_path, process.diagnostics / "stderr.log",
            *((process.startup_path,) if process.startup_path else ()),
        )
        self.remaining()
        return process

    def client(self, *args, **kwargs):
        # From the first connection: these tests never inspect map memory.
        kwargs["omit_memory"] = True
        kwargs.setdefault("actor", ACTOR)
        client, frame = super().client(*args, **kwargs)
        self.work_budget.track(client, frame)
        return client, frame

    def frame(self, client, predicate, seconds=15):
        return self.work_budget.frame_wait(
            super().frame, client, predicate, seconds=self.remaining(seconds))

    def command(self, client, value):
        self.remaining()
        self.work_budget.before()
        return super().command(client, value)

    def say(self, process, text):
        self.work_budget.before()
        process.write(text)
        result = process.until(lambda line: line == "> ", seconds=self.remaining())
        self.remaining()
        return result

    def fixture(self):
        manifest = copy.deepcopy(encounter_sources()["manifest"])
        selected = next(row for row in manifest["characters"] if row["id"] == ACTOR)
        selected["anchor"] = "dungeon/26/up"
        selected["creature"]["nutrition"] = {"kind": "exempt"}
        selected["known_identities"].append("amulet-of-yendor")
        manifest.update(id="amulet-process", default_character=ACTOR, characters=[selected])
        manifest.pop("objective", None)
        manifest.pop("arena", None)
        manifest["generation_recipes"] = {"exploration": {
            "kind": "grid_rooms", "version": 1, "room_width": [5, 5],
            "room_height": [3, 3], "extra_connections": [0, 0],
        }}
        manifest["procedural_worlds"]["dungeon"].update(
            grid=[1, 1], size=[7, 5, 2], domain={"kind": "open", "max_depth": 30})
        manifest["floor_activation"] = {
            "version": 1, "protagonist": "selected_character",
            "families": {"dungeon": "empty"},
            "profiles": {"empty": {"version": 1, "stages": []}},
        }
        manifest.setdefault("archetypes", {})["amulet"] = {
            "name": AMULET, "identity": "amulet-of-yendor", "class": "amulet",
            "stackable": False,
        }
        manifest["unique_retrieval"] = {
            "version": 1, "protagonist": "selected_character", "family": "dungeon",
            "minimum_depth": 26, "archetype": "amulet", "exit_anchor": "dungeon/1/up",
        }
        package = self.directory / "amulet-package"
        (package / "catalog").mkdir(parents=True)
        for module in manifest["modules"]:
            shutil.copy2(PRODUCTION / module, package / module)
        shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
        (package / "scenario.toml").write_text(
            toml_source(manifest), encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                capture_output=True, text=True, encoding="utf-8",
                                timeout=self.remaining(30))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.remaining()
        return package

    def ordinary(self, player, action):
        current = self.act(player, action)
        self.assertIsNone(current["error"])
        current = settle_rogue_action(self, player, current)
        self.assertFalse(observation(current)["combat"]["terminal"])
        return current

    def walk_to(self, player, current, targets):
        """Route solely within the fully disclosed tiny room, at most 14 moves."""
        view = observation(current)
        floor = {(cell["position"]["x"], cell["position"]["y"]): cell["key"]
                 for cell in view["visible_cells"]
                 if cell["position"]["z"] == 0 and not cell["wall"]}
        queue = deque([((0, 0), [])])
        seen = {(0, 0)}
        route = None
        while queue:
            position, path = queue.popleft()
            if floor[position] in targets:
                route = path
                break
            for dx, dy, direction in DIRECTIONS:
                neighbor = (position[0] + dx, position[1] + dy)
                if neighbor in floor and neighbor not in seen:
                    seen.add(neighbor)
                    queue.append((neighbor, [*path, (floor[neighbor], direction)]))
        self.assertIsNotNone(route, "The lit single-room fixture must disclose a route")
        self.assertLessEqual(len(route), 14)
        for key, direction in route:
            current = self.ordinary(player, {"type": "move", "direction": direction})
            self.assertEqual(underfoot(current["state"])["key"], key)
        return current

    def stairs(self, player, current, direction):
        targets = {cell["key"] for cell in observation(current)["visible_cells"]
                   if cell["position"]["z"] == 0 and cell["stairs_" + direction]}
        self.assertEqual(len(targets), 1, "Exactly one disclosed endpoint on the current floor")
        return self.walk_to(player, current, targets)

    def transition(self, player, current, direction):
        current = self.stairs(player, current, direction)
        before = underfoot(current["state"])["key"]
        current = self.ordinary(player, {"type": "move", "direction": direction})
        self.assertNotEqual(underfoot(current["state"])["key"], before)
        return current

    def ground_amulet(self, current):
        matches = [entry for entry in observation(current)["ground_items"]
                   if entry["item"]["name"] == AMULET]
        self.assertEqual(len(matches), 1)
        self.assertEqual(matches[0]["item"]["quantity"], "1")
        return matches[0]

    def take_amulet(self, player, current, identity=None):
        amulet = self.ground_amulet(current)
        if identity is not None:
            self.assertEqual(amulet["item"]["id"], identity)
        target = next(cell["key"] for cell in observation(current)["visible_cells"]
                      if cell["position"] == amulet["position"])
        current = self.walk_to(player, current, {target})
        current = self.ordinary(player, {"type": "take", "item": amulet["item"]["id"]})
        self.assert_carried(current, amulet["item"]["id"])
        return current, amulet["item"]["id"]

    def assert_carried(self, current, identity):
        matches = [entry for entry in observation(current)["inventory"] if entry["name"] == AMULET]
        self.assertEqual([(item["id"], item["quantity"]) for item in matches], [(identity, "1")])
        self.assertFalse(any(entry["item"]["name"] == AMULET
                             for entry in observation(current)["ground_items"]))

    def retrieval_receipts(self, player):
        """Audit committed decisions; never return private terrain to the driver."""
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        entries = list(inspect_save(self.save)["initial_retrievals"])
        with closing(sqlite3.connect(self.save.resolve().as_uri() + "?mode=ro", uri=True)) as db:
            rows = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 "
                              "UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 "
                              "ORDER BY sequence").fetchall()
        for _, frame in rows:
            payload = json.loads(frame[24:])
            if "record" in payload:
                entries.extend(payload["record"]["retrievals"])
        return entries

    def bound_authority(self, player):
        """Inspect only bound identity/current ownership after the action decision."""
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with closing(sqlite3.connect(self.save.resolve().as_uri() + "?mode=ro", uri=True)) as db:
            payload = db.execute("SELECT payload FROM checkpoint").fetchone()[0]
        checkpoint = json.loads(decode_checkpoint_payload(payload))
        self.assertFalse(checkpoint["wizard_game"])
        retrieval = checkpoint["game"]["unique_retrieval"]
        items = checkpoint["shared"]["items"][checkpoint["game"]["items"]]
        matches = {identity: item for identity, item in items.items()
                   if item["spec"]["class"] == "amulet"}
        self.assertEqual(list(matches), [str(retrieval["item"])])
        return {"retrieval": retrieval, "items": matches}

    def surface(self, player, current, identity):
        # Start at 26, then prove all 25 ordinary upward transitions. No private
        # depth/terrain lookup, teleport, save edit, or duplicate descent.
        for _ in range(25):
            current = self.transition(player, current, "up")
            self.assert_carried(current, identity)
        current = self.stairs(player, current, "up")
        self.assertEqual(observation(current)["combat"]["exit"], underfoot(current["state"])["key"])
        self.assertFalse(observation(current)["combat"]["victory"], "Standing at the exit is not escape")
        waited = self.ordinary(player, {"type": "wait"})
        self.assertFalse(observation(waited)["combat"]["victory"], "Waiting at the exit is not escape")
        return waited

    def assert_escape(self, before, escaped, identity):
        self.assert_carried(escaped, identity)
        combat = observation(escaped)["combat"]
        self.assertTrue(combat["victory"])
        self.assertTrue(combat["terminal"])
        self.assertFalse(combat["dead"])
        # Accepted surface escape is an immediate terminal endpoint: it does
        # not advance dungeon time or fabricate a destination beyond the exit.
        self.assertEqual(observation(escaped)["tick"], observation(before)["tick"])
        self.assertEqual(underfoot(escaped["state"])["key"], underfoot(before["state"])["key"])
        events = [row["content"]["event"] for row in escaped["history"]
                  if row["content"]["type"] == "action"]
        self.assertEqual([event for event in events if event["type"] == "escaped"],
                         [{"type": "escaped", "item": identity}])
        self.assertFalse(escaped["state"]["wizard_game"])

    @non_graphical
    def test_unique_identity_revisits_source_free_restore_and_text_surface_escape(self):
        package = self.fixture()
        server = self.server("--checkpoint-interval", 1, scenario=package, character=ACTOR)
        player, current = self.client()
        self.assertEqual(observation(current)["combat"]["objective"], "retrieve_and_exit")
        self.assertEqual(observation(current)["combat"]["own_stats"]["nutrition"]["kind"], "exempt")
        identity = self.ground_amulet(current)["item"]["id"]
        created = self.retrieval_receipts(player)
        self.assertEqual([(row["scope"], row["result"]["kind"]) for row in created],
                         [("dungeon/26", "created")])

        # Spectator snapshots and reconnects must not activate the objective.
        observer, watched = self.client(token=SPECTATOR_TOKEN, observe=True)
        self.assertDisclosedStateEqual(watched["state"], current["state"])
        self.assertDisclosedStateEqual(self.request(observer, {"type": "snapshot"})["state"], current["state"])
        self.assertEqual(self.retrieval_receipts(player), created)
        observer.stop()
        player.stop()
        player, reopened = self.client()
        self.assertDisclosedStateEqual(reopened["state"], current["state"])
        self.assertEqual(self.retrieval_receipts(player), created)

        current = self.transition(player, reopened, "down")
        self.assertEqual(self.ground_amulet(current)["item"]["id"], identity)
        current = self.transition(player, current, "up")
        self.assertEqual(self.ground_amulet(current)["item"]["id"], identity)
        # Internal ascent is legal before retrieval, and a real reentry into
        # already-populated 26 must relocate the same ordinary identity again.
        current = self.transition(player, current, "up")
        current = self.transition(player, current, "down")
        current, _ = self.take_amulet(player, current, identity)
        current = self.transition(player, current, "down")
        self.assert_carried(current, identity)
        current = self.transition(player, current, "up")
        self.assert_carried(current, identity)
        receipts = self.retrieval_receipts(player)
        self.assertEqual([row["scope"] for row in receipts],
                         ["dungeon/26", "dungeon/27", "dungeon/26", "dungeon/26", "dungeon/27", "dungeon/26"])
        self.assertEqual([row["result"]["kind"] for row in receipts],
                         ["created", "relocated", "relocated", "relocated", "already_carried", "already_carried"])
        self.assertEqual(len({row["result"]["item"] for row in receipts}), 1)
        self.assertTrue(all(row["entrant"] == ACTOR for row in receipts))
        bound = self.bound_authority(player)
        self.assertEqual(bound["retrieval"]["entry_count"], 6)
        self.assertEqual(bound["retrieval"]["placement_count"], 4)
        before = self.request(player, {"type": "snapshot"})
        player.stop()
        server.stop(save=False)
        shutil.rmtree(package)
        server = self.server("--checkpoint-interval", 1, scenario=unrelated_fallback(self),
                             seed=None, save_actor=ACTOR)
        player, current = self.client()
        self.assertDisclosedStateEqual(current["state"], before["state"])
        self.assertEqual(current["history"], before["history"])
        self.assertEqual(self.bound_authority(player), bound)
        self.assertEqual(self.retrieval_receipts(player), receipts)

        current = self.surface(player, current, identity)
        self.assertEqual(self.retrieval_receipts(player), receipts, "Subthreshold ascent cannot respawn the Amulet")
        dropped = self.ordinary(player, {"type": "drop", "item": identity})
        self.assertEqual(self.ground_amulet(dropped)["item"]["id"], identity)
        player.stop()
        text = self.launch("tor-client-text", ["--connect", self.address, "--actor", str(ACTOR)],
                           cls=AdventureProcess)
        welcome = text.until(lambda line: line == "> ", seconds=self.remaining())
        self.assertIn("carry it up through the surface exit", welcome)
        self.assertIn("You are standing at the exit.", welcome)
        observer, watched = self.client(token=SPECTATOR_TOKEN, observe=True)
        self.assertDisclosedStateEqual(watched["state"], dropped["state"])
        refused = self.say(text, "up")
        self.assertNotIn("You leave the dungeon", refused)
        unchanged = self.request(observer, {"type": "snapshot"})
        self.assertDisclosedStateEqual(unchanged["state"], dropped["state"], "Missing artifact refusal must cost zero time")
        self.assertEqual(unchanged["history"], dropped["history"])
        taken = self.say(text, "take Amulet of Yendor")
        self.assertEqual(taken, "You pick up the amulet of yendor.\n> ")
        carried = self.request(observer, {"type": "snapshot"})
        self.assert_carried(carried, identity)
        self.assertFalse(observation(carried)["combat"]["victory"])
        escaped_text = self.say(text, "up")
        self.assertTrue(escaped_text.startswith("You go up. You leave the dungeon with the " + AMULET
                        + ". You have done it! This run has ended.\n"), escaped_text)
        self.assertEqual(escaped_text.count("*** You have won ***"), 1)
        self.assertIn("This session recorded", escaped_text)
        self.assertIn("goals attempted", escaped_text)
        self.assertIn("observed HP loss", escaped_text)
        self.assertTrue(escaped_text.endswith("\n> "), escaped_text)
        start = len(observer.transcript)
        escaped = self.request(observer, {"type": "snapshot"})
        self.assert_escape(carried, escaped, identity)
        # request() returns a ready frame with no message. Preserve the actual
        # server response emitted during this request for wire-sample recording.
        responses = [json.loads(line).get("message") for line in observer.transcript[start:]
                     if line.startswith("{")]
        snapshots = [message for message in responses if message and message["type"] == "snapshot"]
        self.assertEqual(len(snapshots), 1)
        self.escape_sample = copy.deepcopy(snapshots[0])
        self.assertEqual(self.escape_sample["type"], "snapshot")
        self.assertEqual(self.escape_sample["snapshot"]["state"], escaped["state"])
        text.stop()
        observer.stop()
        server.stop(save=False)

    def test_native_up_escapes_only_from_the_disclosed_surface_endpoint(self):
        server = self.server("--checkpoint-interval", 1, scenario=self.fixture(), character=ACTOR)
        player, current = self.client()
        current, identity = self.take_amulet(player, current)
        current = self.surface(player, current, identity)
        player.stop()
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--actor", str(ACTOR),
                                                  "--report-frames"])
        initial = self.ascii_frame(window, lambda frame: frame.get("state") and not frame["busy"])
        self.work_budget.track(window, initial)
        self.assertTrue(initial["window_open"])
        self.assertTrue(initial["has_control"])
        self.assertDisclosedStateEqual(initial["state"], current["state"])
        self.assertIn("surface exit", " ".join(initial["status_lines"]))
        self.assertTrue(underfoot(initial["state"])["stairs_up"])
        self.assertEqual(observation(initial)["combat"]["exit"], underfoot(initial["state"])["key"])
        native = self.native_keys(window)
        # Native input first pages any unread opening narration. Read those
        # presented rows explicitly so the one deliberate Up cannot be eaten
        # by --More-- and mistaken for an attempted escape.
        for _ in range(8):
            if not initial["messages"]["more"]:
                break
            self.work_budget.before()
            initial = self.native_press(window, native, "Return", "enter",
                                        seconds=self.remaining(20), wait_for_simulation=False)
            self.assertDisclosedStateEqual(initial["state"], current["state"])
        self.assertFalse(initial["messages"]["more"], "Opening narration exceeded eight native pages")
        self.work_budget.before()
        try:
            native("Shift_L", True)
            escaped = self.native_press(window, native, "comma", "ascend",
                                        seconds=self.remaining(20))
        finally:
            native("Shift_L", False)
        if not observation(escaped)["combat"]["terminal"]:
            escaped = self.ascii_frame(window, lambda frame: frame.get("state")
                                       and observation(frame)["combat"]["terminal"] and not frame["busy"])
        self.assert_escape(initial, escaped, identity)
        shown = " ".join(escaped["messages"]["shown"])
        self.assertIn("You leave the dungeon with the " + AMULET + ".", shown)
        self.assertIn("Victory! This run has ended.", shown)
        window.stop()
        server.stop(save=False)


if __name__ == "__main__":
    unittest.main()
