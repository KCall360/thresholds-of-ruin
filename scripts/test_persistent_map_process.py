"""Persistent last-seen map through ordinary play and a fresh headless client."""
import json
import unittest

from process_harness import ProcessTestCase, door


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
        "ground_items": [
            {field: entry["item"][field] for field in (
                "class", "quantity", "appearance", "identified", "description", "name", "asset")}
            for entry in cell["ground_items"]
        ],
    }


class PersistentMapProcesses(ProcessTestCase):
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
                "class", "quantity", "appearance", "identified", "description", "name", "asset"})
            for value in (entry, entry["item"]):
                for field in ("id", "reachable", "supply_reachable", "known_supply"):
                    self.assertNotIn(field, value)
        self.assertEqual(remembered_appearance(restored[0]), appearance,
                         f"{REGRESSION_MARKER}: stale tile appearance changed on fresh reopen")
        fresh_client.stop()
        reopened.stop(save=False)


if __name__ == "__main__":
    unittest.main()
