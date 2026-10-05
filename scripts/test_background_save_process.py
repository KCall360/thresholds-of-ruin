"""Real-process save barriers, acknowledged rollback, and transaction interruption."""
import shutil
import sqlite3
import subprocess
import sys
import time
import unittest

from process_harness import ProcessTestCase


class BackgroundSaveProcesses(ProcessTestCase):
    def saving_server(self, target=3600000, maximum=7200000, idle=750):
        """A server with explicit background-save timing (milliseconds)."""
        return self.server("--save-target-ms", target, "--save-max-ms", maximum, "--save-idle-ms", idle,
                           seed=None, spectator=False)

    def test_acknowledged_tail_is_lost_but_explicitly_saved_prefix_resumes(self):
        server = self.saving_server()
        client, _ = self.client()
        first = self.act(client,{"type":"wait"})
        self.assertEqual(first["state"]["observation"]["tick"],100)
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT max(sequence) FROM journal").fetchone()[0],0)
        self.assertIsNone(self.request(client,{"type":"save"})["error"])
        self.assertEqual(self.act(client,{"type":"wait"})["state"]["observation"]["tick"],200)
        server.child.kill(); server.child.wait(timeout=10) # deliberately bypass save-and-quit
        server = self.saving_server()
        _, resumed = self.client()
        self.assertEqual(resumed["state"]["observation"]["tick"],100)
        self.assertEqual(resumed["history"],first["history"])

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
            self.assertEqual(result["state"]["observation"]["tick"],200)
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
            self.assertEqual(snapshot["state"]["observation"]["tick"],100)
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
            self.assertEqual(resumed["state"]["observation"]["tick"],100 if prefix==2 else 0)
            self.assertEqual(len(resumed["intentions"]), 1 if prefix==1 else 0)
            if prefix==1:
                self.assertEqual(resumed["intentions"][0]["phase"], "suspended")
            self.assertEqual(len(resumed["history"]), 1 if prefix==2 else 0)
            observer.stop();current.stop()


if __name__ == "__main__":
    unittest.main()
