"""Persistent and incremental last-seen maps through ordinary real-client play."""
import json
import shutil
import subprocess
import unittest

from process_harness import ProcessTestCase, ROOT, door


REGRESSION_MARKER = "PERSISTENT_MAP_MEMORY_LOST_AFTER_REOPEN"


def remembered_appearance(cell):
    """Compare disclosed appearance without retaining actionable item handles."""
    return {
        "key": cell["key"],
        "material": cell["material"],
        "wall": cell["wall"],
        "place_hint": cell["place_hint"],
        "stairs_up": cell["stairs_up"],
        "stairs_down": cell["stairs_down"],
        "trap": None if cell["trap"] is None else {"family": cell["trap"]["family"]},
        "ground_items": [
            {field: entry["item"][field] for field in (
                "class", "quantity", "appearance", "identified", "description", "name", "asset")}
            for entry in cell["ground_items"]
        ],
    }


class PersistentMapProcesses(ProcessTestCase):
    def test_trusted_moves_hydrate_only_edges_and_revisit_an_off_sight_wall(self):
        package = self.directory / "paired-stairs"
        shutil.copytree(ROOT / "scenarios/tests/paired-stairs", package,
                        ignore=shutil.ignore_patterns(".compiled"))
        (package / "regions/1.toml").write_text(
            'id = 1\nname = "Upper floor"\nsize = [32, 5, 1]\nchamber = true\n'
            'anchors = { stair = [1, 1, 0] }\n', encoding="utf-8", newline="\n")
        validated = subprocess.run(
            [self.bin / ("tor-scenario" + self.suffix), "validate", package],
            capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stdout + validated.stderr)
        self.server(scenario=package)
        player, ready = self.client()
        normal = {"min": {"x": -31, "y": -16, "z": -2},
                  "max": {"x": 31, "y": 16, "z": 3}}
        disclosed = set()

        def finish_hydration(frame):
            if not frame["map_hydration"]["complete"]:
                frame = self.frame(player, lambda value: any(
                    value.get("map_hydration", {}).get(flag)
                    for flag in ("complete", "failed", "limited")))
            status = frame["map_hydration"]
            self.assertTrue(status["complete"], status)
            for flag in ("pending", "failed", "limited"):
                self.assertFalse(status[flag], status)
            self.assertLessEqual(status["pages"], 128)
            self.assertLessEqual(status["cells"], 12_474)
            self.assertLessEqual(status["bytes"], 8 * 1024 * 1024)
            return frame

        def consumed(start):
            # Include response and unsolicited update frames consumed by act(),
            # not only the ready result or the final hydration frame.
            frames = [json.loads(line) for line in player.transcript[start:]
                      if line.startswith("{")]
            for frame in frames:
                if frame.get("state"):
                    disclosed.update(cell["key"] for cell in
                                     frame["state"]["observation"]["visible_cells"])
            return [frame["message"]["page"] for frame in frames
                    if (frame.get("message") or {}).get("type") == "map_page"]

        def remembered(frame, key):
            cells = [cell for cell in frame["memory"] if cell["key"] == key]
            self.assertEqual(len(cells), 1, "previously disclosed wall must remain remembered")
            return cells[0]

        settled = finish_hydration(ready)
        initial_pages = consumed(0)
        self.assertTrue(initial_pages, "fresh attachment must hydrate before trusted moves")
        self.assertTrue(all(page["window"] == normal for page in initial_pages))
        self.assertTrue(all(cell["key"] in disclosed for page in initial_pages
                            for cell in page["cells"]),
                        "initial hydration must only contain already disclosed geometry")
        key = appearance = last_seen = None
        windows = []
        final_page = None
        # Chamber interiors include x=0..31; shell walls are x=-1 and x=32.
        # After returning to x=1, the extra west step reaches open x=0 and
        # pushes the known east wall to relative x=32, outside the viewport.
        # The final east step exposes the remembered x=31 edge again.
        directions = ["east"] * 16 + ["west"] * 17 + ["east"]
        for step, direction in enumerate(directions, 1):
            before = len(player.transcript)
            previous = settled["state"]["observation"]
            shifted = self.act(player, {"type": "move", "direction": direction})
            self.assertIsNone(shifted["error"], (step, direction, shifted["error"]))
            settled = finish_hydration(shifted)
            observation = settled["state"]["observation"]
            self.assertEqual(int(observation["tick"]), int(previous["tick"]) + 100,
                             "ordinary movement must actually execute")
            anchors = []
            for view in (previous, observation):
                by_key = {}
                for cell in view["visible_cells"]:
                    by_key.setdefault(cell["key"], []).append(cell["position"])
                anchors.append(by_key)
            old, current = anchors
            common = [(positions[0], current[key][0]) for key, positions in old.items()
                      if len(positions) == 1 and len(current.get(key, [])) == 1]
            self.assertGreater(len(common), 2, "ordinary views must share unique translation anchors")
            origin = common[0][0]
            vectors = [tuple(prior[axis] - origin[axis] for axis in ("x", "y", "z"))
                       for prior, _ in common[1:]]
            self.assertTrue(any(a[0] * b[1] != a[1] * b[0]
                                or a[0] * b[2] != a[2] * b[0]
                                or a[1] * b[2] != a[2] * b[1]
                                for a in vectors for b in vectors),
                            "shared anchors must certify orientation, not only a line")
            delta = -1 if direction == "east" else 1
            self.assertTrue(all(after == {"x": prior["x"] + delta,
                                          "y": prior["y"], "z": prior["z"]}
                                for prior, after in common),
                            "fixture movement must preserve a trusted translation")
            pages = consumed(before)
            self.assertEqual(len(pages), 1, (step, pages))
            page = pages[0]
            edge = 31 if direction == "east" else -31
            expected_window = {"min": {**normal["min"], "x": edge},
                               "max": {**normal["max"], "x": edge}}
            self.assertEqual(page["window"], expected_window,
                             "trusted one-cell moves must request only the uncovered edge")
            self.assertEqual(int(page["base"]["cursor"]["tick"]), int(observation["tick"]))
            self.assertTrue(page["complete"])
            self.assertFalse(page["limited"])
            self.assertEqual((page["start"], page["page"]), (0, 0))
            self.assertEqual(page["total_cells"], len(page["cells"]))
            self.assertIsNone(page["next"])
            self.assertLessEqual(len(page["cells"]), 512)
            for cell in page["cells"]:
                self.assertIn(cell["key"], disclosed, "hydration must not reveal unseen geometry")
                self.assertEqual(set(cell), {"key", "position", "wall", "material", "asset",
                                             "place_hint", "stairs_up", "stairs_down", "trap", "door",
                                             "ground_items", "object_presentation"})
                self.assertIsNone(cell["object_presentation"])
                self.assertEqual(cell["position"]["x"], edge)
                self.assertIsNone(cell["trap"])
                self.assertIsNone(cell["door"])
                self.assertEqual(cell["ground_items"], [])
            windows.append(page["window"])
            if step == 16:
                wall = [cell for cell in observation["visible_cells"]
                        if cell["position"] == {"x": 15, "y": 0, "z": 0}]
                self.assertEqual(len(wall), 1, "ordinary exploration must disclose the east wall")
                self.assertTrue(wall[0]["wall"])
                key = wall[0]["key"]
                appearance = remembered_appearance(remembered(settled, key))
            if step >= 32:
                self.assertNotIn(key, {cell["key"] for cell in observation["visible_cells"]})
                memory = remembered(settled, key)
                self.assertEqual(remembered_appearance(memory), appearance)
                timestamps = (memory["last_seen_tick"], memory["last_seen_revision"])
                if step == 32:
                    last_seen = timestamps
                    self.assertLess(int(last_seen[0]), int(observation["tick"]))
                else:
                    self.assertEqual(timestamps, last_seen,
                                     "hydration must preserve honest existing sighting timestamps")
            final_page = page
        returned = [cell for cell in final_page["cells"] if cell["key"] == key]
        self.assertEqual(len(returned), 1,
                         "off-sight remembered wall must return in the newly uncovered slab")
        self.assertEqual(returned[0]["position"], {"x": 31, "y": 0, "z": 0})
        self.assertEqual(remembered_appearance(returned[0]), appearance)
        (self.directory / "incremental-map-witness.json").write_text(json.dumps({
            "observed_key": key, "observed_appearance": appearance,
            "move_windows": windows, "returned_page": final_page,
            "preserved_last_seen": last_seen,
            "hydration": settled["map_hydration"],
        }, indent=2) + "\n", encoding="utf-8")

    def test_occluded_tablet_memory_survives_save_and_fresh_client(self):
        server = self.server()
        player, _ = self.client()
        seen = self.play(player, [{"move": "east"}] * 3)
        observation = seen["state"]["observation"]
        tablets = [entry for entry in observation["ground_items"]
                   if entry["item"]["name"] == "stone tablet"]
        self.assertEqual(len(tablets), 1, "setup: ordinary movement must reveal the tablet")
        tablet = tablets[0]
        cells = [cell for cell in observation["visible_cells"]
                 if cell["position"] == tablet["position"]]
        self.assertEqual(len(cells), 1, "setup: tablet must occupy one disclosed cell")
        key = cells[0]["key"]
        seen_memory = [cell for cell in seen["memory"] if cell["key"] == key]
        self.assertEqual(len(seen_memory), 1, "setup: the client must learn the observed tile")
        appearance = remembered_appearance(seen_memory[0])
        self.assertEqual(appearance["ground_items"][0]["name"], "stone tablet")
        self.assertTrue(door(seen)["open"], "setup: the ordinary doorway starts open")

        closed = self.act(player, {"type": "set_door", "door": door(seen)["id"], "open": False})
        self.assertIsNone(closed["error"], "setup: ordinary door closing must succeed")
        self.assertTrue(closed["readiness"]["admission"], "setup: door action must release admission")
        self.assertFalse(closed["intentions"], "setup: door action must finish before moving")
        hidden = self.act(player, {"type": "move", "direction": "west"})
        self.assertIsNone(hidden["error"], "setup: the step away from the closed doorway must succeed")
        self.assertFalse(door(hidden)["open"])
        self.assertNotIn(key, {cell["key"] for cell in hidden["state"]["observation"]["visible_cells"]},
                         "setup: target tile must be occluded before saving")
        self.assertFalse(any(entry["item"]["name"] == "stone tablet"
                             for entry in hidden["state"]["observation"]["ground_items"]))
        stale = [cell for cell in hidden["memory"] if cell["key"] == key]
        self.assertEqual(len(stale), 1, "setup: existing client must still remember the hidden tile")
        self.assertEqual(remembered_appearance(stale[0]), appearance,
                         "setup: occlusion must retain the observed appearance")
        self.assertLess(stale[0]["last_seen_revision"], int(hidden["state"]["revision"]))

        self.flush_save()
        player.stop()
        server.stop(save=False)
        reopened = self.server()
        fresh_client, fresh = self.client()
        if not fresh.get("map_hydration", {}).get("complete"):
            fresh = self.frame(fresh_client, lambda frame: frame.get("map_hydration", {}).get("complete"))
        self.assertIsNot(fresh_client, player)
        self.assertEqual(fresh["branch"], hidden["branch"], "setup: reopen must restore the same branch")
        self.assertEqual(fresh["state"], hidden["state"], "setup: reopen must restore the saved state")
        self.assertNotIn(key, {cell["key"] for cell in fresh["state"]["observation"]["visible_cells"]},
                         "setup: the remembered tile must remain out of current sight")
        restored = [cell for cell in fresh["memory"] if cell["key"] == key]
        (self.directory / "persistent-map-witness.json").write_text(json.dumps({
            "observed_key": key,
            "observed_appearance": appearance,
            "stale_appearance_before_save": remembered_appearance(stale[0]),
            "last_seen_revision": stale[0]["last_seen_revision"],
            "saved_revision": hidden["state"]["revision"],
            "occluded_before_save": True,
            "fresh_client_created": True,
            "same_saved_state_after_reopen": True,
            "occluded_after_reopen": True,
            "restored_matching_cells": len(restored),
            "restored_appearances": [remembered_appearance(cell) for cell in restored],
        }, indent=2) + "\n", encoding="utf-8")
        self.assertEqual(len(restored), 1, f"{REGRESSION_MARKER}: previously observed occluded tile {key}")
        self.assertIsNone(restored[0]["last_seen_tick"])
        self.assertIsNone(restored[0]["last_seen_revision"])
        self.assertNotIn("actors", restored[0])
        for entry in restored[0]["ground_items"]:
            self.assertEqual(set(entry), {"item", "position"})
            self.assertEqual(set(entry["item"]), {
                "class", "quantity", "appearance", "identified", "description", "name", "asset",
                "presentation"})
            self.assertIsNone(entry["item"]["presentation"])
            for value in (entry, entry["item"]):
                for field in ("id", "reachable", "supply_reachable", "known_supply"):
                    self.assertNotIn(field, value)
        self.assertEqual(remembered_appearance(restored[0]), appearance,
                         f"{REGRESSION_MARKER}: stale tile appearance changed on fresh reopen")
        fresh_client.stop()
        reopened.stop(save=False)


if __name__ == "__main__":
    unittest.main()
