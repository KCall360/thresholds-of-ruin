"""Actual clients exercise checkpoint rotation, crash rollback and restart."""
import copy
import os
import sqlite3
import subprocess
import json
import unittest
import zlib

from checkpoint_payload import HEADER, crc32c, decode_checkpoint_payload
from process_harness import ProcessTestCase, SPECTATOR_TOKEN, TOKEN, creature_package


class CheckpointFixture:
    def saving_server(self, target=3600000, maximum=7200000, *, writer_barrier=False):
        """A server that checkpoints every four actions, with explicit save timing."""
        # Eight journal records cover four completed gameplay intentions. Keep a
        # nonempty execution tail so this exercises checkpoint-plus-tail replay.
        return self.server("--checkpoint-interval", 8, "--save-target-ms", target, "--save-max-ms", maximum,
                           "--save-idle-ms", 0, seed=None, writer_barrier=writer_barrier)



class CheckpointProcesses(CheckpointFixture, ProcessTestCase):
    def read_checkpoint(self):
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("PRAGMA user_version").fetchone()[0], 33)
            payload, checksum = db.execute("SELECT payload, checksum FROM checkpoint").fetchone()
        self.assertEqual(crc32c(b"123456789"), 0xe3069283)
        self.assertEqual(crc32c(payload), checksum)
        checkpoint = json.loads(decode_checkpoint_payload(payload))
        self.assertEqual(checkpoint["version"], 33)
        return checkpoint

    def write_checkpoint(self, checkpoint):
        raw = json.dumps(checkpoint, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        payload = HEADER + zlib.compress(raw, 1) + len(raw).to_bytes(4, "little")
        self.assertEqual(decode_checkpoint_payload(payload), raw)
        checksum = crc32c(payload)
        with sqlite3.connect(self.save) as db:
            updated = db.execute("UPDATE checkpoint SET payload = ?, checksum = ?",
                                 (payload, checksum))
            self.assertEqual(updated.rowcount, 1)
            self.assertEqual(db.execute("SELECT payload, checksum FROM checkpoint").fetchone(),
                             (payload, checksum))

    def assert_invalid_save(self, *, scenario=None):
        before = self.save.read_bytes()
        result = subprocess.run(
            [self.bin / ("tor-server" + self.suffix), "--listen", "127.0.0.1:0",
             *(["--scenario", scenario] if scenario is not None else []), "--save", self.save],
            capture_output=True, text=True, encoding="utf-8", timeout=15,
            env={**os.environ, "TOR_SERVER_TOKEN": TOKEN})
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('"address"', result.stdout, "an invalid save must never advertise a listener")
        self.assertIn("InvalidArchive", result.stderr)
        self.assertIn("unsupported or inconsistent game journal", result.stderr.lower())
        self.assertEqual(self.save.read_bytes(), before)

    def test_previous_health_ledger_save_archive_and_checkpoint_are_rejected_before_rewrite(self):
        server = self.server("--checkpoint-interval", 1)
        player, _ = self.client()
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        checkpoint = self.read_checkpoint()
        original = self.save.read_bytes()
        for obsolete in (28, 29, 32):
            for boundary in ("sqlite", "archive", "checkpoint"):
                with self.subTest(boundary=boundary, obsolete=obsolete):
                    self.save.write_bytes(original)
                    if boundary == "sqlite":
                        with sqlite3.connect(self.save) as db:
                            db.execute(f"PRAGMA user_version = {obsolete}")
                    elif boundary == "archive":
                        with sqlite3.connect(self.save) as db:
                            frame = db.execute("SELECT frame FROM journal WHERE sequence=0").fetchone()[0]
                            self.assertEqual(frame[:4], b"TORB")
                            self.assertEqual(int.from_bytes(frame[4:6], "little"), 6)
                            self.assertEqual(int.from_bytes(frame[6:16], "little"), 0)
                            self.assertEqual(int.from_bytes(frame[16:20], "little"), len(frame) - 24)
                            self.assertEqual(int.from_bytes(frame[20:24], "little"),
                                             crc32c(frame[4:20] + frame[24:]))
                            base = json.loads(frame[24:])
                            self.assertEqual(base["archive"]["version"], 33)
                            base["archive"]["version"] = obsolete
                            raw = json.dumps(base, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
                            header = frame[:16] + len(raw).to_bytes(4, "little")
                            changed = header + crc32c(header[4:] + raw).to_bytes(4, "little") + raw
                            self.assertNotEqual(changed, frame)
                            db.execute("UPDATE journal SET frame = ? WHERE sequence=0", (changed,))
                    else:
                        changed = copy.deepcopy(checkpoint)
                        changed["version"] = obsolete
                        self.write_checkpoint(changed)
                    self.assert_invalid_save()

    def test_checkpoint_requires_health_recovery_and_build_fields_before_listening(self):
        package = creature_package(self, "required-health-ledger-checkpoint", abilities=("fear",))
        server = self.server("--checkpoint-interval", 1, scenario=package)
        player, _ = self.client()
        saved = self.act(player, {"type": "wait"})
        self.assertIsNone(saved["error"])
        saved = self.frame(player, lambda frame: frame.get("state") is not None
                           and frame["state"]["observation"]["ready"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        checkpoint = self.read_checkpoint()
        # Positive control: the same test encoder and CRC must produce a real,
        # restartable save before removing logical fields one at a time.
        self.write_checkpoint(checkpoint)
        server = self.server(scenario=package, seed=None)
        player, resumed = self.client()
        self.assertEqual(resumed["state"], saved["state"])
        player.stop()
        server.stop()
        checkpoint = self.read_checkpoint()
        original = self.save.read_bytes()
        for path in (
            ("health_recovery",),
            ("health_recovery", "elapsed"),
            ("health_recovery", "nutrition"),
            ("build", "starting_skills"),
            ("build", "restoration"),
            ("build", "choices", 0, "purchases"),
        ):
            with self.subTest(field=path):
                self.save.write_bytes(original)
                broken = copy.deepcopy(checkpoint)
                changed = 0
                for actors in broken["shared"]["actors"]:
                    for actor in actors.values():
                        if actor.get("combat") is None:
                            continue
                        parent = actor["combat"]["creature"]
                        for part in path[:-1]:
                            parent = parent[part]
                        self.assertIn(path[-1], parent)
                        del parent[path[-1]]
                        changed += 1
                self.assertGreater(changed, 0)
                self.write_checkpoint(broken)
                self.assert_invalid_save(scenario=package)

    def test_d20_save_format_is_rejected_before_replay_or_rewrite(self):
        import subprocess
        server = self.saving_server()
        player, _ = self.client()
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("PRAGMA user_version").fetchone()[0], 33)
            # The previous current format owns a different deterministic check
            # stream. Even otherwise valid payloads must not cross this gate.
            db.execute("PRAGMA user_version = 27")
        before = self.save.read_bytes()
        result = subprocess.run(
            [self.bin / ("tor-server" + self.suffix), "--listen", "127.0.0.1:0",
             "--save", self.save], capture_output=True, text=True, encoding="utf-8",
            timeout=15, env={**os.environ, "TOR_SERVER_TOKEN": TOKEN})
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('"address"', result.stdout)
        self.assertIn("InvalidArchive", result.stderr)
        self.assertEqual(self.save.read_bytes(), before)

    def test_restart_rejects_legacy_raw_combat_without_rewriting_the_save(self):
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
        checkpoint = self.read_checkpoint()
        changed = 0
        for actors in checkpoint["shared"]["actors"]:
            for actor in actors.values():
                if actor.get("combat") is not None:
                    self.assertEqual(set(actor["combat"]), {"identity", "creature"})
                    actor["combat"] = legacy
                    changed += 1
        self.assertGreater(changed, 0)
        self.write_checkpoint(checkpoint)
        self.assert_invalid_save(scenario=package)


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
            self.assertEqual(checkpoint["version"], 33)
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


    def test_checkpoint_writer_wait_does_not_block_new_actions(self):
        self.saving_server(target=1, maximum=1000, writer_barrier=True)
        player, _ = self.client()
        # Reach the checkpoint boundary using a durable prefix, then hold the
        # next batch while more accepted actions execute.
        for _ in range(3):
            self.act(player, {"type": "wait"})
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with self.held_save_writer() as barrier:
            self.act(player, {"type": "wait"})
            barrier.wait()
            for _ in range(2):
                result = self.act(player, {"type": "wait"})
            self.assertEqual(result["state"]["observation"]["tick"], "600")
            barrier.assert_held()
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        barrier.assert_released()
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], 8)



class NativeCheckpointProcesses(CheckpointFixture, ProcessTestCase):
    graphical = True

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




if __name__ == "__main__":
    unittest.main()
