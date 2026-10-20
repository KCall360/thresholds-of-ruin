"""Real native-window tests. Linux requires DISPLAY (use xvfb-run).

Automation injects UI input events, not OS keystrokes. Frames are reported only
after the real native window successfully presents its pixel buffer.
"""
import json
import os
import shutil
import sqlite3
import subprocess
import tomllib
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT, TOKEN, SPECTATOR_TOKEN
from scalar_wire import u64


class AsciiProcesses(ProcessTestCase):
    graphical = True

    def test_stats_screen_presents_without_submitting_gameplay(self):
        client, initial = self.ascii()
        opened = self.key(client, "stats")
        self.assertTrue(opened["screen"]["stats"])
        self.assertTrue(opened["window_open"])
        self.assertGreater(opened["frame"], initial["frame"])
        self.assertEqual(opened["stats_rows"], ["No creature stats are available."])
        for key in ["down", "recent_history", "attack"]:
            frame = self.key(client, key)
            self.assertTrue(frame["screen"]["stats"])
            self.assertEqual(frame["state"], initial["state"])
            self.assertEqual(frame["screen"]["stats_scroll"], 0)
        closed = self.key(client, "escape")
        self.assertFalse(closed["screen"]["stats"])
        self.assertEqual(closed["state"], initial["state"])

    def test_ability_menu_requires_a_disclosed_personal_grant(self):
        client, initial = self.ascii(observe=False)
        refused = self.key(client, "abilities")
        self.assertEqual(refused["status"], "No abilities are available.")
        self.assertEqual(refused["ability_choices"], [])
        self.assertIsNone(refused["selected_ability"])
        self.assertEqual(refused["state"], initial["state"])

    def setUp(self):
        super().setUp()
        self.game = self.server(spectator=False)

    def ascii(self, observe=True):
        process = self.launch("tor-client-ascii", ["--connect", self.address, "--automation", *(["--observe"] if observe else [])])
        frame = self.ascii_frame(process, lambda f: f["state"] is not None and not f["busy"])
        self.assertGreater(frame["frame"], 0)
        self.assertTrue(frame["window_open"])
        self.assertNotIn(TOKEN, str(frame))
        return process, frame

    def test_read_only_spectator_window_streams_actions_and_resumes(self):
        self.game.stop()
        self.game = self.server()
        player, _ = self.text_client()
        spectator = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"], token=SPECTATOR_TOKEN)
        initial = self.ascii_frame(spectator, lambda f: f["state"] is not None and not f["busy"])
        self.assertEqual(initial["role"], "spectator")
        self.assertFalse(initial["has_control"])
        self.assertIn("read-only", initial["status"])
        self.assertIn("stone tablet", str(initial))
        actions = ["take token", "wait", "east", "east", "east", "east", "east"]
        for revision, action in enumerate(actions, 1):
            player.command(action)
            watched = self.ascii_frame(spectator, lambda f: int(f["state"]["revision"]) == revision)
            entries = [e for e in watched["history"] if e["content"]["type"] == "action"]
            self.assertEqual(len(entries), revision)
            self.assertIn("event", entries[-1]["content"])
            self.assertFalse(watched["has_control"])
        self.assertEqual(next(i["position"] for i in watched["state"]["observation"]["ground_items"] if i["item"]["name"] == "stone tablet"), {"x":2,"y":0,"z":0})
        player.command("note Secret")
        player.command("annotate user actor note here Public progress")
        shared = self.ascii_frame(spectator, lambda f: "Public progress" in str(f["history"]))
        self.assertNotIn("Secret", str(shared))
        self.flush_save()
        before = self.save.read_bytes()
        for key in ["control", "release", "right", "pickup", "wait", "note", "resume_intention", "cancel_intention"]:
            denied = self.key(spectator, key)
            self.assertIn("read-only", denied["status"])
            self.assertFalse(denied["has_control"])
            self.assertIsNone(denied["note"])
            self.assertEqual(denied["state"], watched["state"])
        history = self.key(spectator, "history")
        self.assertIn("Public progress", str(history))
        self.assertNotIn("Secret", str(history))
        self.key(spectator, "escape")
        self.key(spectator, "escape")
        self.assertEqual(spectator.child.wait(timeout=10), 0)
        self.assertEqual(self.save.read_bytes(), before)
        player.stop()
        self.game.stop()
        self.game = self.server()
        resumed = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"], token=SPECTATOR_TOKEN)
        restored = self.ascii_frame(resumed, lambda f: f["state"] is not None and not f["busy"])
        self.assertEqual(restored["role"], "spectator")
        self.assertEqual(restored["state"], watched["state"])
        self.assertEqual(restored["history"], shared["history"])
        self.assertIn("read-only", self.key(resumed, "control")["status"])

    def test_text_to_window_control_transfer_and_save_resume(self):
        text, _ = self.text_client()
        ascii_client, initial = self.ascii()
        self.assertIn("stone tablet", str(initial))
        self.assertFalse(initial["has_control"])
        text.command("take token")
        self.ascii_frame(ascii_client, lambda f: f["state"]["revision"] == '1')
        text.command("note Return through the entry.")
        self.ascii_frame(ascii_client, lambda f: "Return through the entry." in str(f["history"]))
        denied = self.key(ascii_client, "control")
        self.assertEqual(denied["status"], "Another player has control; you can watch until they release it.")
        text.command("release")
        self.assertTrue(self.key(ascii_client, "control")["has_control"])
        for _ in range(5):
            moved = self.key(ascii_client, "right")
        self.assertEqual(next(i["position"] for i in moved["state"]["observation"]["ground_items"] if i["item"]["name"] == "stone tablet"), {"x":2,"y":0,"z":0})
        self.assertEqual(moved["state"]["observation"]["tick"], '550')
        self.assertIn("stone tablet", str(moved))
        self.assertIn("token", str(moved["state"]["observation"]["inventory"]))
        # Sync is a protocol barrier after all pushes to the observing text client.
        self.assertIn("Your surroundings", text.command("sync"))
        before = moved["state"]
        self.key(ascii_client, "release")
        self.assertIn("Control: yours", text.command("control"))
        self.key(ascii_client, "escape", wait_for_simulation=False)
        self.assertEqual(ascii_client.child.wait(timeout=10), 0)
        text.stop()
        self.game.stop()
        self.game = self.server(spectator=False)
        resumed_text, welcome = self.text_client(observe=True)
        resumed_ascii, restored = self.ascii()
        self.assertEqual(restored["state"], before)
        self.assertEqual(restored["history"], moved["history"])
        self.assertEqual(restored["branch"], moved["branch"])
        self.assertIn("Your surroundings", welcome)
        self.assertIn("tick 550.", welcome)
        self.assertIn("token", resumed_text.command("inventory"))
        self.assertIn("Return through the entry.", resumed_text.command("history"))
        self.assertTrue(self.key(resumed_ascii, "control")["has_control"])

    def test_graphical_pickup_notes_invalid_move_and_disconnect(self):
        client, _ = self.ascii(observe=False)
        taken = self.key(client, "pickup")
        self.assertEqual(taken["state"]["revision"], '1')
        self.assertIn("token", str(taken["state"]["observation"]["inventory"]))
        self.key(client, "note")
        client.child.stdin.write(json.dumps({"type":"text", "text":"A graphical note"}) + "\n")
        client.child.stdin.flush()
        noted = self.key(client, "enter")
        self.assertIn("A graphical note", str(noted["history"]))
        self.assertEqual(noted["state"]["revision"], '1')
        invalid = self.key(client, "ascend")
        self.assertEqual(invalid["status"], "You can't go that way.")
        self.assertEqual(invalid["state"], noted["state"])
        self.game.stop()
        self.ascii_frame(client, lambda f: not f["connected"])
        self.assertNotEqual(client.child.wait(timeout=15), 0)

    def test_bad_authentication_discloses_no_state(self):
        result = subprocess.run([str(self.bin / ("tor-client-ascii" + self.suffix)), "--connect", self.address, "--automation"], env={**os.environ, "TOR_SERVER_TOKEN":"wrong-test-token"}, capture_output=True, text=True, timeout=20)
        (self.directory / "bad-authentication.stdout.log").write_text(result.stdout, encoding="utf-8")
        (self.directory / "bad-authentication.stderr.log").write_text(result.stderr, encoding="utf-8")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Connection rejected: Unauthorized", result.stderr)
        self.assertNotIn("Entry chamber", result.stdout)
        self.assertNotIn("wrong-test-token", result.stdout + result.stderr)

    def test_native_keyboard_pickup_and_close(self):
        # This path uses real OS key messages, not the JSON automation driver.
        capture = self.save.parent / "native-frame.ppm"
        client = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames", "--capture", capture])
        self.ascii_frame(client, lambda f: f["has_control"] and not f["busy"])
        key_event = self.native_keys(client)
        key_event("g", True)
        picked = self.ascii_frame(client, lambda f: f["state"]["revision"] == '1' and not f["busy"])
        key_event("g", False)
        self.assertIn("token", str(picked["state"]["observation"]["inventory"]))
        key_event("Escape", True)
        self.assertEqual(client.child.wait(timeout=15), 0)
        data = capture.read_bytes()
        self.assertTrue(data.startswith(b"P6\n1200 800\n255\n"))
        pixels = data.split(b"\n", 3)[3]
        self.assertEqual(len(pixels), 1200 * 800 * 3)
        self.assertGreater(len(set(pixels)), 8)


class AsciiHudPickupProcesses(ProcessTestCase):
    graphical = True

    def assert_displayed_status(self, frame, gold):
        """Check the fitted rows passed to the real renderer, not the raw model text."""
        rows = frame["rendered_status_lines"]
        self.assertEqual(len(rows), 3)
        self.assertTrue(all(len(row) <= 72 for row in rows), rows)
        observation = frame["state"]["observation"]
        combat = observation["combat"]
        stats = combat["own_stats"]
        attributes = stats["attributes"]
        labels = {"strength": "Str", "speed": "Spd", "intellect": "Int",
                  "willpower": "Wil", "awareness": "Awr", "presence": "Pre"}
        for attribute, label in labels.items():
            self.assertIn(f"{label}:{attributes[attribute]}", rows[0].split())
        self.assertIn(f"HD:{stats['hit_dice']['total']}", rows[0].split())
        fields = " ".join(rows[1:]).split()
        self.assertIn(f"HP:{combat['hp']}({combat['max_hp']})", fields)
        self.assertIn(f"Gold:{gold}", fields)
        resources = {pool["resource"]["id"]: pool for pool in stats["resources"]}
        for resource, label in {"stamina": "Stm", "focus": "Foc", "mana": "Mana"}.items():
            pool = resources[resource]
            expected = f"{label}:{pool['balance']}/{pool['maximum']}"
            if pool["reserved"]:
                expected += f"[-{pool['reserved']}]"
            self.assertIn(expected, fields)

    def test_native_single_stack_pickup_updates_typed_gold_and_fitted_resource_hud(self):
        # Ordinary validated content supplies real personal stats and item capabilities.
        # Keep the original native token test and its two-room assumptions unchanged.
        package = self.directory / "ascii-hud-pickup"
        shutil.copytree(ROOT / "scenarios/tests/interactions", package)
        manifest_path = package / "scenario.toml"
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["id"] = "ascii-hud-pickup"
        manifest["characters"][0]["turn_ticks"] = 100
        manifest["archetypes"]["hud_tender"] = {
            "name": "unusual tender", "class": "coin", "stackable": True,
            "currency": "gold"}
        manifest["archetypes"]["hud_decoy"] = {
            "name": "gold coin", "class": "coin", "stackable": True}
        manifest_path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        region_path = package / "regions/1.toml"
        region = tomllib.loads(region_path.read_text(encoding="utf-8"))
        region["items"] = [
            {"id": 10, "at": [1, 1, 0], "archetype": "hud_tender", "quantity": 7},
            {"id": 11, "at": [1, 1, 0], "archetype": "hud_decoy", "quantity": 11,
             "carried_by": 1}]
        region_path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        validated = subprocess.run(
            [self.bin / ("tor-scenario" + self.suffix), "validate", package],
            capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.server(scenario=package)
        observer, _ = self.client(SPECTATOR_TOKEN)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        initial = self.ascii_frame(window, lambda frame: frame["has_control"] and not frame["busy"])
        self.assertTrue(initial["window_open"])
        before = initial["state"]["observation"]
        self.assertEqual(len(before["ground_items"]), 1)
        ground = before["ground_items"][0]
        self.assertTrue(ground["reachable"])
        tender = ground["item"]
        self.assertEqual((tender["name"], tender["known_currency"], u64(tender["quantity"])),
                         ("unusual tender", "gold", 7))
        self.assertEqual(len(before["inventory"]), 1)
        decoy = before["inventory"][0]
        self.assertEqual((decoy["name"], decoy["known_currency"], u64(decoy["quantity"])),
                         ("gold coin", None, 11))
        self.assert_displayed_status(initial, gold=0)

        native = self.native_keys(window)
        picked = self.native_press(window, native, "g", "pickup", wait_for_ready=True)
        self.assertTrue(picked["window_open"])
        self.assertFalse(picked["busy"])
        self.assertFalse(picked["screen"]["pickup"], "one stack must not open a picker")
        after = picked["state"]["observation"]
        self.assertEqual(after["ground_items"], [])
        self.assertEqual(len(after["inventory"]), 2)
        carried = next(item for item in after["inventory"] if item["id"] == tender["id"])
        self.assertEqual((carried["known_currency"], u64(carried["quantity"])), ("gold", 7))
        self.assertEqual(next(item for item in after["inventory"] if item["id"] == decoy["id"]),
                         decoy)
        self.assertEqual(u64(after["tick"]) - u64(before["tick"]), 50,
                         "one ordinary take costs half of the authored 100-tick turn")
        self.assertEqual(picked["intentions"], [])
        self.assert_displayed_status(picked, gold=7)
        actions = [entry["content"] for entry in picked["history"]
                   if entry["content"]["type"] == "action"]
        self.assertEqual(len(actions), 1)
        self.assertEqual(actions[0]["action"],
                         {"type": "take", "item": tender["id"], "quantity": None})
        self.assertEqual(actions[0]["event"],
                         {"type": "taken", "item": tender["id"],
                          "result": carried["id"], "quantity": "7"})
        authoritative = self.request(observer, {"type": "snapshot"})["state"]["observation"]
        for field in ("tick", "inventory", "ground_items"):
            self.assertEqual(authoritative[field], after[field])

        # The acknowledged native release plus durable admission count catches repeats.
        self.flush_save()
        with sqlite3.connect(self.save) as db:
            records = db.execute(
                "SELECT sequence,frame FROM journal WHERE sequence>0 "
                "UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 "
                "ORDER BY sequence").fetchall()
        records = [json.loads(blob[24:])["record"] for _, blob in records]
        admissions = [record for record in records
                      if record["entry"]["content"]["type"] == "intention_admitted"]
        self.assertEqual(len(admissions), 1, "one native g must admit exactly one take")
        self.assertEqual(admissions[0]["receipt"]["command"]["action"],
                         {"type": "take", "item": 10, "quantity": None})


if __name__ == "__main__":
    unittest.main()
