"""Real-process save barriers, acknowledged rollback, and transaction interruption."""
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import time
import unittest

from process_harness import ProcessTestCase, package_path


class BackgroundSaveProcesses(ProcessTestCase):
    STARTUP_STAGES = ["scenario.begin", "scenario.end", "engine.begin",
        "restore.lock.begin", "restore.lock.end", "restore.store.begin", "restore.store.end",
        "restore.replay.begin", "restore.replay.end", "engine.end", "preloading.begin",
        "preloading.end", "simulation.begin", "simulation.end", "listener.begin", "listener.end", "ready"]

    def assert_startup_stages(self, server, *, complete):
        records = [json.loads(line) for line in server.startup_path.read_text(encoding="utf-8").splitlines()]
        stages = [record["stage"] for record in records]
        self.assertTrue(stages)
        self.assertEqual(stages, self.STARTUP_STAGES[:len(stages)])
        if complete:
            self.assertEqual(stages, self.STARTUP_STAGES)
            # Breadcrumbs must never enter the public readiness JSON protocol.
            self.assertEqual(len(server.transcript), 1)
            self.assertEqual(set(json.loads(server.transcript[0])), {"address", "protocol"})
        else:
            self.assertIn("restore.store.begin", stages)
            self.assertNotIn("ready", stages)
        self.assertTrue(all(record["pid"] == server.child.pid for record in records))
        self.assertTrue(all(isinstance(record["unix_ms"], int) and record["unix_ms"] >= 0 for record in records))
        elapsed = [record["elapsed_ms"] for record in records]
        self.assertTrue(all(isinstance(value, (int, float)) and value >= 0 for value in elapsed))
        self.assertEqual(elapsed, sorted(elapsed))

    def test_saved_package_index_rejects_duplicate_and_missing_canonical_fields(self):
        server = self.server(scenario="two-room")
        player, _ = self.client()
        self.assert_startup_stages(server, complete=True)
        completed = self.act(player, {"type": "wait"})
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        server = self.server(scenario="two-room")
        player, restored = self.client()
        self.assert_startup_stages(server, complete=True)
        self.assertDisclosedStateEqual(restored["state"], completed["state"])
        self.assertEqual(restored["history"], completed["history"])
        player.stop()
        server.stop()
        with sqlite3.connect(self.save) as db:
            chunks = db.execute("SELECT chunk,bytes FROM package ORDER BY chunk").fetchall()
        self.assertEqual([chunk for chunk, _ in chunks], [0])
        text = chunks[0][1].decode("utf-8")
        self.assertIn('"id":1', text)
        self.assertIn('"start":[1,1,0]', text)
        self.assertIn('"zone":null,', text)
        cases = {
            "duplicate-collection": text.replace('"regions":', '"regions":[],"regions":', 1),
            "duplicate-region-id": text.replace('"id":1', '"id":0,"id":1', 1),
            "duplicate-anchor": text.replace('"start":[1,1,0]', '"start":[0,0,0],"start":[1,1,0]', 1),
            "missing-null-field": text.replace('"zone":null,', '', 1),
            "unknown-field": text.replace('{', '{"unrecognized":true,', 1),
        }
        for name, malformed in cases.items():
            with self.subTest(shape=name):
                corrupted = self.directory / f"{name}.db"
                shutil.copyfile(self.save, corrupted)
                with sqlite3.connect(corrupted) as db:
                    db.execute("UPDATE package SET bytes=? WHERE chunk=0", [malformed.encode("utf-8")])
                before = corrupted.read_bytes()
                rejected = self.launch("tor-server", ["--listen", "127.0.0.1:0",
                    "--save", corrupted, "--scenario", package_path("two-room")], separate_stderr=True)
                try:
                    rejected.child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.fail(f"Server accepted noncanonical saved package index or failed to reject within its watchdog: {name}")
                finally:
                    rejected.stop(save=False)
                self.assertNotEqual(rejected.child.returncode, 0)
                self.assertIn("InvalidArchive", (rejected.diagnostics / "stderr.log").read_text(encoding="utf-8"))
                self.assertNotIn('"address"', rejected.stdout_path.read_text(encoding="utf-8"))
                self.assert_startup_stages(rejected, complete=False)
                self.assertEqual(corrupted.read_bytes(), before, "rejected save must remain unchanged")

    def test_checkpointed_shutdown_releases_the_journal_for_immediate_restarts(self):
        server = self.server("--checkpoint-interval", 1, scenario="two-room")
        player, _ = self.client()
        for _ in range(3):
            completed = self.act(player, {"type": "wait"})
            self.assertIsNone(completed["error"])
            self.assertIsNone(self.request(player, {"type": "save"})["error"])
            with sqlite3.connect(self.save) as db:
                checkpoint = db.execute("SELECT sequence FROM checkpoint").fetchone()
                self.assertIsNotNone(checkpoint)
                self.assertGreater(checkpoint[0], 0)
            player.stop()
            server.stop()
            server = self.server("--checkpoint-interval", 1, scenario="two-room")
            player, restored = self.client()
            self.assertDisclosedStateEqual(restored["state"], completed["state"])
            self.assertEqual(restored["history"], completed["history"])

    def saving_server(self, target=3600000, maximum=7200000, idle=750, *, writer_barrier=False):
        """A server with explicit background-save timing (milliseconds)."""
        return self.server("--save-target-ms", target, "--save-max-ms", maximum, "--save-idle-ms", idle,
                           seed=None, spectator=False, writer_barrier=writer_barrier)

    def test_acknowledged_tail_is_lost_but_explicitly_saved_prefix_resumes(self):
        server = self.saving_server()
        client, _ = self.client()
        first = self.act(client,{"type":"wait"})
        self.assertEqual(first["state"]["observation"]["tick"],"100")
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT max(sequence) FROM journal").fetchone()[0],0)
        self.assertIsNone(self.request(client,{"type":"save"})["error"])
        self.assertEqual(self.act(client,{"type":"wait"})["state"]["observation"]["tick"],"200")
        server.child.kill(); server.child.wait(timeout=10) # deliberately bypass save-and-quit
        server = self.saving_server()
        _, resumed = self.client()
        self.assertEqual(resumed["state"]["observation"]["tick"],"100")
        self.assertEqual(resumed["history"],first["history"])

    def test_saved_journal_shapes_and_disclosed_history_survive_restart(self):
        server = self.saving_server()
        player, _ = self.client()
        completed = self.act(player, {"type": "wait"})
        note = self.request(player, {
            "type": "command", "context": completed["input_context"], "branch": completed["branch"],
            "command": {"type": "annotate", "anchor": {"type": "state", "snapshot": completed["state"]["snapshot"]},
                        "text": "Stored schema boundary", "source": "frontend",
                        "audience": "actor", "category": "bookmark"}})
        self.assertIsNone(note["error"])
        self.assertIs(type(note["state"]["revision"]), str)
        self.assertIs(type(note["state"]["observation"]["actor"]), str)
        self.assertIs(type(note["state"]["observation"]["tick"]), str)
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            records = [json.loads(frame[24:])["record"] for (frame,) in
                       db.execute("SELECT frame FROM journal WHERE sequence>0 ORDER BY sequence")]
        self.assertEqual([record["entry"]["content"]["type"] for record in records],
                         ["intention_admitted", "intention_started", "annotation"])
        for record in records:
            self.assertIs(type(record["entry"]["actor"]), int)
            self.assertIs(type(record["entry"]["tick"]), int)
            self.assertIsInstance(record["entry"]["branch"], str)
            if record["receipt"] is not None:
                self.assertIs(type(record["receipt"]["actor"]), int)
                self.assertIsInstance(record["receipt"]["branch"], str)
        self.assertEqual(records[0]["receipt"]["command"]["action"], {"type": "wait"})
        stored_note = records[-1]["entry"]
        self.assertEqual(stored_note["author"]["type"], "frontend")
        self.assertEqual(stored_note["audience"], "actor")
        self.assertEqual(stored_note["content"]["category"], "bookmark")
        self.assertEqual(stored_note["content"]["anchor"], {
            "type": "state", "snapshot": completed["state"]["snapshot"],
            "boundary": records[1]["entry"]["id"]})
        server.stop()
        self.saving_server()
        _, restored = self.client()
        self.assertDisclosedStateEqual(restored["state"], note["state"])
        self.assertEqual(restored["history"], note["history"])
        self.assertEqual(restored["intentions"], [])

    def test_maximum_age_saves_even_when_idle_opportunity_is_unavailable(self):
        self.saving_server(target=100, maximum=300, idle=300)
        client, _ = self.client()
        self.act(client,{"type":"wait"})
        deadline = time.monotonic()+10
        while time.monotonic()<deadline:
            with sqlite3.connect(self.save) as db:
                if db.execute("SELECT max(sequence) FROM journal").fetchone()[0] > 0: break
            time.sleep(.02)
        else: self.fail("Background save did not reach its deadline")

    def test_normal_client_quit_saves_pending_play(self):
        self.saving_server()
        client, _ = self.client()
        self.act(client,{"type":"wait"})
        client.child.stdin.write('{"type":"quit"}\n');client.child.stdin.flush()
        self.assertEqual(client.child.wait(timeout=10),0)
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT max(sequence) FROM journal").fetchone()[0],2)

    def test_slow_writer_does_not_hold_the_session_lock(self):
        self.saving_server(target=1, maximum=1000, idle=0, writer_barrier=True)
        client, _ = self.client()
        with self.held_save_writer() as barrier:
            self.act(client, {"type": "wait"})
            barrier.wait()
            # The real persistence worker is held, independently of SQLite's
            # busy timeout or worker scheduling, throughout the next action.
            result = self.act(client, {"type": "wait"})
            self.assertEqual(result["state"]["observation"]["tick"], "200")
            barrier.assert_held()
        self.assertIsNone(self.request(client, {"type": "save"})["error"])
        barrier.assert_released()

    def test_failed_background_save_allows_reconnect_and_explicit_retry(self):
        self.saving_server(target=1, maximum=5000, idle=0)
        client, _ = self.client()
        with sqlite3.connect(self.save) as db:
            db.execute("BEGIN EXCLUSIVE")
            self.act(client,{"type":"wait"})
            failed = self.frame(client,lambda f: (f.get("message") or {}).get("type") == "error")
            self.assertIn("save failed",failed["message"]["message"].lower())
            # A persistent warning must follow the new attachment snapshot.
            replacement, snapshot = self.client()
            self.assertEqual(snapshot["state"]["observation"]["tick"], "100")
            messages = [json.loads(line).get("message") for line in replacement.transcript if line.startswith("{")]
            errors = [message for message in messages if message and message["type"] == "error"]
            warnings = [message for message in errors if message["code"] == "storage_failure"]
            self.assertEqual(len(warnings), 1, "attachment must deliver its remembered storage warning")
            # The original client still controls this actor, so the replacement's
            # automatic control request is refused independently of save health.
            self.assertEqual([message["code"] for message in errors
                              if message["code"] != "storage_failure"], ["control_taken"])
            self.assertFalse(snapshot["has_control"])
            warning = warnings[0]
            self.assertEqual(warning["code"], "storage_failure")
            self.assertIsNone(warning["request_id"])
            self.assertEqual(warning["message"], failed["message"]["message"])
            self.assertEqual(warning["scope"]["type"], "attached")
            context = warning["scope"]["context"]
            self.assertEqual(context["input"]["stream"], snapshot["input_context"]["stream"])
            self.assertEqual(context["branch"], snapshot["branch"])
            self.assertEqual(context["actor"], snapshot["state"]["observation"]["actor"])
            db.rollback()
        self.assertIsNone(self.request(replacement, {"type": "save"})["error"])
        clean, restored = self.client()
        # A round-trip consumes all earlier ordered attachment messages. A
        # successful retry clears the remembered warning for later attachments.
        self.assertIsNone(self.request(clean, {"type": "snapshot"})["error"])
        self.assertDisclosedStateEqual(restored["state"], snapshot["state"])
        messages = [json.loads(line).get("message") for line in clean.transcript if line.startswith("{")]
        self.assertFalse(any(message and message["type"] == "error" and
                             message["code"] == "storage_failure" for message in messages))

    def test_killed_database_transaction_recovers_atomic_prefix(self):
        server = self.saving_server(); client, _ = self.client()
        empty = self.save.with_name("empty.db");shutil.copyfile(self.save,empty)
        self.act(client,{"type":"wait"});self.request(client,{"type":"save"})
        with sqlite3.connect(self.save) as db:
            records = db.execute("SELECT frame FROM journal WHERE sequence>0 ORDER BY sequence").fetchall()
        self.assertEqual(len(records), 2)
        frame_paths = [self.save.with_name(f"frame-{n}.bin") for n in (1, 2)]
        for path, (record,) in zip(frame_paths, records):
            path.write_bytes(record)
        server.stop()
        # Rollback exposes neither fact; admission alone exposes pending work
        # without effects; committing both exposes the completed action.
        for prefix in (0, 1, 2):
            shutil.copyfile(empty,self.save)
            code = """import sqlite3,sys
from pathlib import Path
c=sqlite3.connect(sys.argv[1]);c.execute('PRAGMA cache_size=1');c.execute('PRAGMA synchronous=EXTRA');c.execute('BEGIN IMMEDIATE')
prefix=int(sys.argv[2]);b=Path(sys.argv[3]).read_bytes()
c.execute('INSERT INTO journal VALUES (1,?)',(b,))
if prefix==2: c.execute('INSERT INTO journal VALUES (2,?)',(Path(sys.argv[4]).read_bytes(),))
if prefix: c.commit()
else:
 for n in range(2,80): c.execute('INSERT INTO journal VALUES (?,?)',(n,b*30))
print('written',flush=True);sys.stdin.readline()
"""
            # Use the same bounded readiness and diagnostic capture as every
            # other child; a broken transaction helper cannot hang readline().
            child = self.launch(Path(sys.executable).name, ["-u", "-c", code, str(self.save),
                str(prefix), *map(str, frame_paths)], executable=sys.executable)
            try:
                self.assertEqual(child.until(lambda line: line == "written"), "written")
                child.child.kill()
                child.child.wait(timeout=10)
            finally:
                child.stop(save=False)
            current=self.saving_server(); observer, resumed=self.client(observe=True)
            self.assertEqual(resumed["state"]["observation"]["tick"],"100" if prefix==2 else "0")
            self.assertEqual(len(resumed["intentions"]), 1 if prefix==1 else 0)
            if prefix==1:
                self.assertEqual(resumed["intentions"][0]["phase"], "suspended")
            self.assertEqual(len(resumed["history"]), 1 if prefix==2 else 0)
            observer.stop();current.stop()


if __name__ == "__main__":
    unittest.main()
