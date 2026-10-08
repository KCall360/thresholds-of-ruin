"""Real-process save barriers, acknowledged rollback, and transaction interruption."""
import json
import os
import shutil
import sqlite3
import subprocess
import sys
import time
import unittest

from process_harness import ProcessTestCase, TOKEN, package_path


class BackgroundSaveProcesses(ProcessTestCase):
    def test_saved_package_index_rejects_duplicate_and_missing_canonical_fields(self):
        server = self.server(scenario="two-room")
        player, _ = self.client()
        completed = self.act(player, {"type": "wait"})
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        server = self.server(scenario="two-room")
        player, restored = self.client()
        self.assertEqual(restored["state"], completed["state"])
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
                env = {key: value for key, value in os.environ.items()
                       if key not in ("TOR_SPECTATOR_TOKEN", "TOR_WIZARD_TOKEN")}
                try:
                    result = subprocess.run(
                        [self.bin / ("tor-server" + self.suffix), "--listen", "127.0.0.1:0",
                         "--save", corrupted, "--scenario", package_path("two-room")],
                        env={**env, "TOR_SERVER_TOKEN": TOKEN}, capture_output=True,
                        text=True, encoding="utf-8", timeout=5)
                except subprocess.TimeoutExpired as error:
                    output = error.stdout or b""
                    if isinstance(output, bytes):
                        output = output.decode("utf-8")
                    self.assertIn('"address":', output, "failure must prove the server reached its listener")
                    self.fail(f"Server accepted noncanonical saved package index: {name}")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("InvalidArchive", result.stderr)
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
            self.assertEqual(restored["state"], completed["state"])
            self.assertEqual(restored["history"], completed["history"])

    def saving_server(self, target=3600000, maximum=7200000, idle=750):
        """A server with explicit background-save timing (milliseconds)."""
        return self.server("--save-target-ms", target, "--save-max-ms", maximum, "--save-idle-ms", idle,
                           seed=None, spectator=False)

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
            "command": {"type": "annotate", "anchor": {"type": "state", "revision": completed["state"]["revision"]},
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
        self.assertIs(type(stored_note["content"]["anchor"]["revision"]), int)
        server.stop()
        self.saving_server()
        _, restored = self.client()
        self.assertEqual(restored["state"], note["state"])
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
        self.saving_server(target=1, maximum=1000, idle=0)
        client, _ = self.client()
        with sqlite3.connect(self.save) as db:
            db.execute("BEGIN EXCLUSIVE")
            self.act(client,{"type":"wait"})
            # The worker is waiting on the database lock while queries and another
            # action still complete. Its SQLite lock timeout is two seconds.
            time.sleep(.1)
            start=time.monotonic()
            result=self.act(client,{"type":"wait"})
            self.assertLess(time.monotonic()-start,1.5)
            self.assertEqual(result["state"]["observation"]["tick"],"200")
            db.rollback()
        self.assertIsNone(self.request(client,{"type":"save"})["error"])

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
            self.assertEqual(snapshot["state"]["observation"]["tick"],"100")
            db.rollback()
        self.assertIsNone(self.request(replacement,{"type":"save"})["error"])

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
            child = subprocess.Popen([sys.executable,"-c",code,str(self.save),str(prefix),*map(str,frame_paths)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
            try:
                self.assertEqual(child.stdout.readline().strip(),"written")
                child.kill();child.wait(timeout=10)
            finally:
                if child.poll() is None: child.kill();child.wait(timeout=10)
                child.stdin.close();child.stdout.close();child.stderr.close()
            current=self.saving_server(); observer, resumed=self.client(observe=True)
            self.assertEqual(resumed["state"]["observation"]["tick"],"100" if prefix==2 else "0")
            self.assertEqual(len(resumed["intentions"]), 1 if prefix==1 else 0)
            if prefix==1:
                self.assertEqual(resumed["intentions"][0]["phase"], "suspended")
            self.assertEqual(len(resumed["history"]), 1 if prefix==2 else 0)
            observer.stop();current.stop()


if __name__ == "__main__":
    unittest.main()
