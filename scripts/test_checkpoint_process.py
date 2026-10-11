"""Actual clients exercise checkpoint rotation, crash rollback and restart."""
import os
import sqlite3
import json
import unittest

from checkpoint_payload import decode_checkpoint_payload
from process_harness import ProcessTestCase, SPECTATOR_TOKEN, TOKEN


class CheckpointProcesses(ProcessTestCase):
    def saving_server(self, target=3600000, maximum=7200000):
        """A server that checkpoints every four actions, with explicit save timing."""
        # Eight journal records cover four completed gameplay intentions. Keep a
        # nonempty execution tail so this exercises checkpoint-plus-tail replay.
        return self.server("--checkpoint-interval", 8, "--save-target-ms", target, "--save-max-ms", maximum,
                           "--save-idle-ms", 0, seed=None)

    def test_restart_rejects_legacy_raw_combat_without_rewriting_the_save(self):
        import subprocess
        import zlib
        from checkpoint_payload import HEADER
        from process_harness import creature_package
        package = creature_package(self, "owned-checkpoint", abilities=("fear",))
        server = self.server("--checkpoint-interval", 1, scenario=package)
        player, _ = self.client()
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        legacy = {"spec": {"name": "figure", "max_hp": 30, "defense": 10,
                           "attack": {"bonus": 2, "wind_up": 60, "recovery": 40, "damage": {"impact": 4}},
                           "immunities": [], "reductions": {}, "faction": "neutral"}, "hp": 30}
        changed = 0
        with sqlite3.connect(self.save) as db:
            payload = db.execute("SELECT payload FROM checkpoint").fetchone()[0]
            checkpoint = json.loads(decode_checkpoint_payload(payload))
            for actors in checkpoint["shared"]["actors"]:
                for actor in actors.values():
                    if actor.get("combat") is not None:
                        self.assertEqual(set(actor["combat"]), {"identity", "creature"})
                        actor["combat"] = legacy
                        changed += 1
            self.assertGreater(changed, 0)
            raw = json.dumps(checkpoint, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
            payload = HEADER + zlib.compress(raw, 1) + len(raw).to_bytes(4, "little")
            db.execute("UPDATE checkpoint SET payload = ?", (payload,))
        before = self.save.read_bytes()
        result = subprocess.run([self.bin / ("tor-server" + self.suffix), "--listen", "127.0.0.1:0",
                                 "--scenario", package, "--save", self.save], capture_output=True,
                                text=True, encoding="utf-8", timeout=15,
                                env={**os.environ, "TOR_SERVER_TOKEN": TOKEN})
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('"address"', result.stdout, "an invalid save must never advertise a listener")
        self.assertIn("InvalidArchive", result.stderr)
        self.assertIn("unsupported or inconsistent game journal", result.stderr.lower())
        self.assertEqual(self.save.read_bytes(), before)

    def test_checkpoint_tail_crash_and_restart_through_real_headless_client(self):
        server = self.saving_server()
        player, _ = self.client()
        for _ in range(10):
            saved = self.act(player, {"type": "wait"})
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], 16)
            payload = db.execute("SELECT payload FROM checkpoint").fetchone()[0]
            raw = decode_checkpoint_payload(payload)
            checkpoint = json.loads(raw)
            self.assertEqual(checkpoint["version"], 25)
            self.assertEqual(checkpoint["sequence"], 16)
            self.assertLess(len(payload), len(raw))
            self.assertEqual(db.execute("SELECT count(*) FROM history").fetchone()[0], 16)
            self.assertEqual(db.execute("SELECT count(*) FROM journal WHERE sequence>0").fetchone()[0], 4)
        for _ in range(3):
            self.act(player, {"type": "wait"})
        server.child.kill(); server.child.wait(timeout=10)
        self.saving_server()
        player, resumed = self.client()
        self.assertEqual(resumed["state"], saved["state"])
        self.assertEqual(resumed["history"], saved["history"])
        for _ in range(3):
            continued = self.act(player, {"type": "wait"})
        self.assertEqual(continued["state"]["observation"]["tick"], "1300")
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], 24)

    def test_text_player_and_native_ascii_spectator_resume_checkpointed_game(self):
        server = self.saving_server()
        player, _ = self.text_client()
        spectator = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"],
            token=SPECTATOR_TOKEN)
        self.ascii_frame(spectator, lambda f: f["state"] is not None)
        for n in range(1, 10):
            player.command("wait")
            saved = self.ascii_frame(spectator,
                lambda f: f["state"]["observation"]["tick"] == str(n*100))
        self.assertIn("Done.", player.command("save"))
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], 16)
        spectator.stop(); player.stop(); server.stop()
        self.saving_server()
        player, welcome = self.text_client()
        self.assertIn("tick 900", welcome)
        spectator = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"],
            token=SPECTATOR_TOKEN)
        resumed = self.ascii_frame(spectator, lambda f: f["state"] is not None)
        self.assertEqual(resumed["state"], saved["state"])
        self.assertEqual(resumed["history"], saved["history"])
        player.command("wait")
        self.ascii_frame(spectator,
            lambda f: f["state"]["observation"]["tick"] == "1000")

    def test_checkpoint_writer_wait_does_not_block_new_actions(self):
        import time
        self.saving_server(target=1, maximum=1000)
        player, _ = self.client()
        with sqlite3.connect(self.save) as db:
            db.execute("BEGIN EXCLUSIVE")
            started = time.monotonic()
            for _ in range(6):
                self.act(player, {"type": "wait"})
            self.assertLess(time.monotonic()-started, 1.5)
            db.rollback()
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], 8)


if __name__ == "__main__":
    unittest.main()
