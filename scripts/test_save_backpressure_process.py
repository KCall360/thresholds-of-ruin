"""Lagging storage through real processes: saves answer in time and play waits.

An exclusive SQLite transaction from this test stands in for a slow disk. It is
released before the server's two-second busy timeout, so the writer is late, not
failed.
"""
import json
import os
import sqlite3
import subprocess
import threading
import time
import unittest

from process_harness import ProcessTestCase, ROOT, SUFFIX, TOKEN, binaries

REPLY_MS = 300
LOCK_SECONDS = 1.6


def save_once(address):
    """One explicit save through a real headless client; returns its errors."""
    result = subprocess.run(
        [str(binaries() / ("tor-client-headless" + SUFFIX)), "--connect", address, "--observe"],
        input=json.dumps({"type": "request", "request": {"type": "save"}}) + "\n"
        + json.dumps({"type": "quit"}) + "\n",
        text=True, capture_output=True, timeout=35, cwd=ROOT,
        env={**os.environ, "TOR_SERVER_TOKEN": TOKEN})
    rows = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
    errors = [row["error"] for row in rows if row.get("error")]
    return result.returncode, errors or [result.stderr.strip()[-500:]] if result.returncode else errors


class SaveBackpressure(ProcessTestCase):
    def test_lagging_storage_answers_save_behind_and_holds_commands(self):
        self.server("--save-target-ms", 100, "--save-max-ms", 200, "--save-idle-ms", 100,
                    "--save-reply-ms", REPLY_MS, "--checkpoint-interval", 0)
        player, _ = self.client()
        self.act(player, {"type": "wait"})
        self.flush_save()
        lock = sqlite3.connect(self.save, timeout=0, isolation_level=None)
        self.addCleanup(lock.close)
        lock.execute("BEGIN EXCLUSIVE")
        locked = time.monotonic()
        try:
            # Admitted before saving is overdue; its journal entry cannot be written.
            self.act(player, {"type": "wait"})
            time.sleep(0.4)
            code, errors = save_once(self.address)
            self.assertTrue(errors and all(str(error).startswith("SaveBehind") for error in errors), errors)
            self.assertLess(time.monotonic() - locked, LOCK_SECONDS, "the save answered in time")
            finished = {}

            def held_command():
                self.act(player, {"type": "wait"})
                finished["at"] = time.monotonic()
            command = threading.Thread(target=held_command)
            command.start()
            time.sleep(max(0.0, locked + LOCK_SECONDS - time.monotonic()))
        finally:
            lock.execute("COMMIT")
        released = time.monotonic()
        command.join(timeout=30)
        self.assertFalse(command.is_alive())
        # The command waited for saving to catch up instead of outrunning it.
        self.assertGreaterEqual(finished["at"], released - 0.05)
        code, errors = save_once(self.address)
        self.assertEqual((code, errors), (0, []))


if __name__ == "__main__":
    unittest.main()
