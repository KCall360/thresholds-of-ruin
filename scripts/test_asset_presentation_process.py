"""Disclosed assets reach the actual Text/native clients without renaming things.

The ordinary package deliberately gives differently named things a common
appearance. Palette-only recovery is owned by client-common's scripted transport
witness and ASCII's network-event/model tests; this proves the real UI bridge,
native input, repeated screens, spectator reconnect and save/resume boundary.
"""
import shutil
import subprocess
import threading
import tomllib
import unittest

from arena_matrix import toml_source
from stream_relay import StreamRelay
from process_harness import ProcessTestCase, ROOT, SPECTATOR_TOKEN, non_graphical


def prepare(test):
    package = test.directory / "asset-presentation"
    shutil.copytree(ROOT / "scenarios/first-dungeon", package,
                    ignore=shutil.ignore_patterns(".compiled"))
    manifest = package / "scenario.toml"
    document = tomllib.loads(manifest.read_text(encoding="utf-8"))
    document["id"] = "asset-presentation"
    del document["objective"]
    document["themes"] = ["presentation"]
    document["assets"] = {"presentation": ["creature.equine.quagga", "item.amulet"]}
    manifest.write_text(toml_source(document), encoding="utf-8", newline="\n")
    actors = package / "catalog/actors.toml"
    document = tomllib.loads(actors.read_text(encoding="utf-8"))
    document["actors"]["dungeon.scout"]["asset"] = "creature.equine.quagga"
    actors.write_text(toml_source(document), encoding="utf-8", newline="\n")
    items = package / "catalog/items.toml"
    document = tomllib.loads(items.read_text(encoding="utf-8"))
    document["items"]["dungeon.delver_blade"]["asset"] = "item.amulet"
    items.write_text(toml_source(document), encoding="utf-8", newline="\n")
    for region in (package / "regions").glob("*.toml"):
        region.unlink()
    (package / "regions/1.toml").write_text(toml_source({
        "id": 1, "name": "Presentation chamber", "size": [3, 1, 2],
        "chamber": True, "gravity": [0, 0, -1], "anchors": {"exit": [0, 0, 0]},
        "actors": [{"id": 3, "at": [2, 0, 0], "archetype": "dungeon.scout",
                    "controller": "external", "turn_ticks": 80}],
        "items": [
            {"id": 101, "at": [0, 0, 0], "archetype": "dungeon.delver_blade",
             "carried_by": 1, "equipped_slot": 0},
            {"id": 102, "at": [1, 0, 0], "archetype": "dungeon.delver_blade"},
        ],
    }), encoding="utf-8", newline="\n")
    validated = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                               capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(validated.returncode, 0, validated.stderr)
    return package


class MissingSelfAssetPalette(StreamRelay):
    """Omit one nested-only asset from the first catalog; allow normal repair."""
    asset = "creature.humanoid.mage"

    def __init__(self, address):
        self.masked = threading.Event()
        self.repaired = threading.Event()
        self.repaired_revision = None
        super().__init__(address)

    def corrupt_observation(self, message):
        if message.get("type") != "palette":
            return False
        body = message["palette"]["body"]
        if body["type"] != "full":
            return False
        if not self.masked.is_set():
            if self.asset not in body["assets"]:
                raise AssertionError("fixture must advertise the real player appearance")
            body["assets"].remove(self.asset)
            self.masked.set()
            return True
        if message.get("request_id") and self.asset in body["assets"]:
            self.repaired_revision = int(message["palette"]["revision"])
            self.repaired.set()
        return False


class AssetPresentationProcesses(ProcessTestCase):
    graphical = True

    @non_graphical
    def test_text_preserves_authored_names_and_commands_with_shared_assets(self):
        self.server(scenario=prepare(self))
        text, welcome = self.adventure()
        self.assertIn("ruin scout", welcome)
        self.assertNotIn("quagga", welcome)
        self.assertIn("ruin scout", self.say(text, "examine ruin scout"))
        self.assertIn("iron greatsword", self.say(text, "inventory"))
        self.assertNotIn("amulet", self.say(text, "examine iron greatsword"))

    def test_native_client_repairs_an_asset_disclosed_only_in_self_appearance(self):
        package = prepare(self)
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        manifest["characters"][0]["asset"] = MissingSelfAssetPalette.asset
        # Own-body disclosure exists even with no extra visible actor cells.
        manifest["characters"][0]["body"] = {"cells": [[0, 0, 0]], "eye": [0, 0, 0], "mass": 80}
        manifest["assets"]["presentation"].append(MissingSelfAssetPalette.asset)
        path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.server(scenario=package)
        relay = MissingSelfAssetPalette(self.address)
        self.addCleanup(relay.close)
        window = self.launch("tor-client-ascii", ["--connect", relay.address, "--report-frames"])
        self.assertTrue(relay.masked.wait(10), "initial palette was not intercepted")
        self.assertTrue(relay.repaired.wait(10), "nested self appearance did not request palette repair")
        frame = self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"]
                                 and f["palette"]["revision"] == relay.repaired_revision
                                 and not f["palette"]["stale"])
        observation = frame["state"]["observation"]
        self.assertEqual(observation["self_appearance"]["asset"], MissingSelfAssetPalette.asset)
        self.assertFalse(any(a.get("asset") == MissingSelfAssetPalette.asset
                             for a in observation["visible_actors"]), "fixture must exercise the nested-only carrier")
        self.assertEqual(relay.errors, [])

    def settled_palette(self, window):
        return self.ascii_frame(window, lambda frame: frame["state"] is not None
                                and not frame["busy"] and frame["palette"]["revision"] == 1)

    def assert_asset_map(self, frame):
        creature = next(tile for tile in frame["map_tiles"] if tile["kind"] == "creature")
        item = next(tile for tile in frame["map_tiles"] if tile["kind"] == "item")
        self.assertEqual(creature["glyph"], "u")
        self.assertEqual(item["glyph"], '"')
        self.assertTrue(any(actor.get("asset") == "creature.equine.quagga"
                            for actor in frame["state"]["observation"]["visible_actors"]))
        self.assertEqual(frame["state"]["observation"]["inventory"][0]["name"], "iron greatsword")

    def test_native_map_look_inventory_and_resumed_spectator_share_disclosed_assets(self):
        server = self.server(scenario=prepare(self))
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        initial = self.settled_palette(window)
        self.assert_asset_map(initial)
        native = self.native_keys(window)
        for _ in range(2):
            inventory = self.native_press(window, native, "i", "inventory")
            self.assertTrue(inventory["screen"]["inventory"])
            self.assertIn('" an iron greatsword', "\n".join(inventory["inventory_rows"]))
            closed = self.native_press(window, native, "Escape", "escape")
            self.assertFalse(closed["screen"]["inventory"])
        self.native_press(window, native, "semicolon", "look")
        self.native_press(window, native, "Right", "right")
        self.native_press(window, native, "Right", "right")
        looked = self.native_press(window, native, "semicolon", "look")
        self.assertIn("u - a ruin scout", looked["messages"]["turn"])
        self.assertEqual(looked["state"], initial["state"])
        window.stop()
        # The harness requests and confirms an ordinary save before stopping
        # the server. A new read-only native attachment then rebuilds its palette.
        server.stop()
        self.server()
        observer = self.launch("tor-client-ascii", ["--connect", self.address,
                               "--observe", "--report-frames"], token=SPECTATOR_TOKEN)
        resumed = self.settled_palette(observer)
        self.assert_asset_map(resumed)
        self.assertEqual(resumed["role"], "spectator")
        self.assertFalse(resumed["has_control"])
        self.assertDisclosedStateEqual(resumed["state"], initial["state"])


if __name__ == "__main__":
    unittest.main()
