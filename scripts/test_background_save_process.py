"""Real-process save barriers, acknowledged rollback, and transaction interruption."""
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import time
import unittest
import test_text_process as support
import test_headless_process as headless

class BackgroundSaveProcesses(unittest.TestCase):
    setUpClass = classmethod(support.TextProcesses.setUpClass.__func__)
    launch = support.TextProcesses.launch
    setUp = headless.HeadlessProcesses.setUp
    client = headless.HeadlessProcesses.client
    frame = headless.HeadlessProcesses.frame
    command = headless.HeadlessProcesses.command
    act = headless.HeadlessProcesses.act
    request = headless.HeadlessProcesses.request

    def server(self, target=3600000, maximum=7200000, idle=750):
        server = self.launch("tor-server",["--listen","127.0.0.1:0","--save",self.save,
            "--save-target-ms",str(target),"--save-max-ms",str(maximum),"--save-idle-ms",str(idle)])
        self.address = json.loads(server.until(lambda line:line.startswith("{")))["address"]
        return server

    def test_acknowledged_tail_is_lost_but_explicitly_saved_prefix_resumes(self):
        server = self.server()
        client, _ = self.client()
        first = self.act(client,{"type":"wait"})
        self.assertEqual(first["state"]["observation"]["tick"],100)
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT max(sequence) FROM journal").fetchone()[0],0)
        self.assertIsNone(self.request(client,{"type":"save"})["error"])
        self.assertEqual(self.act(client,{"type":"wait"})["state"]["observation"]["tick"],200)
        server.child.kill(); server.child.wait(timeout=10) # deliberately bypass save-and-quit
        server = self.server()
        _, resumed = self.client()
        self.assertEqual(resumed["state"]["observation"]["tick"],100)
        self.assertEqual(resumed["history"],first["history"])

    def test_maximum_age_saves_even_when_idle_opportunity_is_unavailable(self):
        self.server(target=100, maximum=300, idle=300)
        client, _ = self.client()
        self.act(client,{"type":"wait"})
        deadline = time.monotonic()+10
        while time.monotonic()<deadline:
            with sqlite3.connect(self.save) as db:
                if db.execute("SELECT max(sequence) FROM journal").fetchone()[0] > 0: break
            time.sleep(.02)
        else: self.fail("Background save did not reach its deadline")

    def test_normal_client_quit_saves_pending_play(self):
        self.server()
        client, _ = self.client()
        self.act(client,{"type":"wait"})
        client.child.stdin.write('{"type":"quit"}\n');client.child.stdin.flush()
        self.assertEqual(client.child.wait(timeout=10),0)
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT max(sequence) FROM journal").fetchone()[0],1)

    def test_slow_writer_does_not_hold_the_session_lock(self):
        self.server(target=1, maximum=1000, idle=0)
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
        self.server(target=1, maximum=5000, idle=0)
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
        server = self.server(); client, _ = self.client()
        empty = self.save.with_name("empty.db");shutil.copyfile(self.save,empty)
        self.act(client,{"type":"wait"});self.request(client,{"type":"save"})
        with sqlite3.connect(self.save) as db:
            record = db.execute("SELECT frame FROM journal WHERE sequence=1").fetchone()[0]
        frame_path = self.save.with_name("frame.bin");frame_path.write_bytes(record)
        server.stop()
        for committed in (False,True):
            shutil.copyfile(empty,self.save)
            code = """import sqlite3,sys
from pathlib import Path
c=sqlite3.connect(sys.argv[1]);c.execute('PRAGMA cache_size=1');c.execute('PRAGMA synchronous=EXTRA');c.execute('BEGIN IMMEDIATE')
b=Path(sys.argv[2]).read_bytes();c.execute('INSERT INTO journal VALUES (1,?)',(b,))
if sys.argv[3]=='yes': c.commit()
else:
 for n in range(2,80): c.execute('INSERT INTO journal VALUES (?,?)',(n,b*30))
print('written',flush=True);sys.stdin.readline()
"""
            child = subprocess.Popen([sys.executable,"-c",code,str(self.save),str(frame_path),"yes" if committed else "no"],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
            try:
                self.assertEqual(child.stdout.readline().strip(),"written")
                child.kill();child.wait(timeout=10)
            finally:
                if child.poll() is None: child.kill();child.wait(timeout=10)
                child.stdin.close();child.stdout.close();child.stderr.close()
            current=self.server(); observer, resumed=self.client()
            self.assertEqual(resumed["state"]["observation"]["tick"],100 if committed else 0)
            observer.stop();current.stop()
