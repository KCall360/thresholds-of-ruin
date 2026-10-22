"""Small real-log regressions for the fall acceptance driver's stopped boundaries."""
import gzip
import hashlib
import io
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import test_fall_credit_process as fall


class FallCreditRetention(unittest.TestCase):
    def test_any_unfinished_owner_rejects_before_any_raw_mutation(self):
        defects = ("stop", "exit", "reader", "stdin", "stdout", "stderr", "queue_reader", "queue_writer", "unowned")
        for defect in defects:
            with self.subTest(defect=defect), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                diagnostics = root / "process-00-client"
                diagnostics.mkdir()
                raw = diagnostics / "stdout.log"
                raw.write_bytes(b"exact raw evidence")
                handles = [SimpleNamespace(closed=True) for _ in range(5)]
                process = SimpleNamespace(_stopped=True, diagnostics=diagnostics,
                    child=SimpleNamespace(poll=lambda: 0, stdin=handles[0], stdout=handles[1], stderr=handles[2]),
                    reader=SimpleNamespace(is_alive=lambda: False),
                    lines=SimpleNamespace(reader=handles[3], writer=handles[4]))
                case = fall.FallCreditProcesses()
                case.directory, case.processes = root, [process]
                cache = fall.ArchiveVerificationCache()
                with patch.object(fall, "compress_completed_logs", return_value=[]) as positive:
                    self.assertEqual(fall.retain_stopped_fall_logs(case, cache), [])
                positive.assert_called_once_with(root, cache=cache)
                if defect == "stop":
                    process._stopped = False
                elif defect == "exit":
                    process.child.poll = lambda: None
                elif defect == "reader":
                    process.reader.is_alive = lambda: True
                elif defect == "unowned":
                    (root / "process-01-unowned").mkdir()
                else:
                    handles[("stdin", "stdout", "stderr", "queue_reader", "queue_writer").index(defect)].closed = False
                with patch.object(fall, "compress_completed_logs") as compress, self.assertRaises(AssertionError):
                    fall.retain_stopped_fall_logs(case, cache)
                compress.assert_not_called()
                self.assertEqual(raw.read_bytes(), b"exact raw evidence")
                self.assertFalse((root / "retained-raw-logs.json").exists())
                self.assertEqual(list(root.glob("**/*.gz")), [])

    def test_actual_caller_seals_all_eight_existing_sessions_before_relaunch(self):
        evidence = b"complete fall diagnostic\n" * 64
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            case = fall.FallCreditProcesses(
                "test_immediate_fatal_fall_owner_survives_journal_only_and_checkpoint_tail")
            case.directory, case.processes = root, []
            launches, originals = [], {}
            snapshot_ticks = iter((110, 120, 130, 110, 120, 130))
            growth = iter(({"earned_xp": str(x), "held_xp": str(x)}
                           for x in (0, 0, 25, 25, 25) * 2))

            def frame(tick, hp):
                return {"error": None, "state": {"wizard_game": False, "observation": {
                    "tick": str(tick), "combat": {"hp": str(hp), "dead": hp == 0},
                    "visible_actors": [{"id": 2, "name": "practice target"}],
                }}}

            def verify_retained():
                self.assertTrue(all(p._stopped for p in case.processes))
                index = root / "retained-raw-logs.json"
                self.assertTrue(index.exists(), "existing stopped boundary must retain logs before relaunch")
                rows = json.loads(index.read_text())
                self.assertEqual({row["original"] for row in rows}, set(originals))
                for row in rows:
                    raw = originals[row["original"]]
                    self.assertEqual(gzip.decompress((root / row["retained"]).read_bytes()), raw)
                    self.assertEqual(row["uncompressed_bytes"], len(raw))
                    self.assertEqual(row["uncompressed_sha256"], hashlib.sha256(raw).hexdigest())
                    self.assertFalse((root / row["original"]).exists())
                self.assertEqual((root / "unrelated.db").read_bytes(), b"not a log")
                self.assertEqual((root / "startup-00.jsonl").read_bytes(), b"startup evidence")

            def process(name):
                diagnostics = root / f"process-{len(case.processes):02d}-{name}"
                diagnostics.mkdir()
                path = diagnostics / "stdout.log"
                raw = evidence + str(len(case.processes)).encode()
                path.write_bytes(raw)
                originals[str(path.relative_to(root))] = raw
                handles = [io.BytesIO() for _ in range(5)]
                p = SimpleNamespace(_stopped=False, diagnostics=diagnostics,
                                    child=SimpleNamespace(stdin=handles[0], stdout=handles[1], stderr=handles[2]),
                                    lines=SimpleNamespace(reader=handles[3], writer=handles[4]))
                p.child.poll = lambda: 0 if p._stopped else None
                p.reader = SimpleNamespace(is_alive=lambda: not p._stopped)
                def stop(**kwargs):
                    for handle in handles:
                        handle.close()
                    p._stopped = True
                p.stop = stop
                case.processes.append(p)
                return p

            def server(*args, **kwargs):
                if case.processes:
                    verify_retained()
                launches.append((args, kwargs))
                return process("server")

            def client(*, actor=1):
                session = (len(launches) - 1) % 4
                tick, hp = ((0, 5), (110, 1), (120, 1), (130, 0))[session]
                return process(f"client-{actor}"), frame(tick, hp)

            def request(_client, command):
                if command["type"] == "snapshot":
                    tick = next(snapshot_ticks)
                    return frame(tick, 0 if tick == 130 else 1)
                self.assertEqual(command, {"type": "save"})
                return {"error": None}

            def database(path):
                checkpointed = path.stem == "fall-checkpoint"
                db = Mock()
                def execute(query, *_args):
                    if query == "SELECT sequence, payload FROM checkpoint":
                        row = (7, b"mock checkpoint payload") if checkpointed else None
                    elif query == "SELECT sequence FROM checkpoint":
                        row = (7,) if checkpointed else None
                    else:
                        self.assertIn("count(*) FROM journal", query)
                        row = (1,)
                    return SimpleNamespace(fetchone=lambda: row)
                db.execute = execute
                return SimpleNamespaceContext(db)

            (root / "unrelated.db").write_bytes(b"not a log")
            (root / "startup-00.jsonl").write_bytes(b"startup evidence")
            checkpoint = {"game": {"actors": "actors"}, "shared": {"actors": {"actors": {
                "2": {"motion": {"active_fall": {"owner": 1, "episode": 1, "started_at": 110}}}}}}}
            with patch.object(case, "server", side_effect=server), \
                    patch.object(case, "client", side_effect=client), \
                    patch.object(case, "request", side_effect=request), \
                    patch.object(case, "act", return_value={"error": None}) as act, \
                    patch.object(case, "growth", side_effect=lambda _: next(growth)), \
                    patch.object(case, "wait_one_decision") as wait, \
                    patch.object(fall, "fall_credit_package", side_effect=lambda _case, name: root / name), \
                    patch.object(fall.sqlite3, "connect", side_effect=database), \
                    patch.object(fall, "decode_checkpoint_payload", return_value=json.dumps(checkpoint)):
                case.test_immediate_fatal_fall_owner_survives_journal_only_and_checkpoint_tail()
            verify_retained()
            self.assertEqual(len(launches), 8)
            self.assertEqual(len(case.processes), 24)
            self.assertEqual([args[1] for args, _ in launches], [0, 0, 0, 0, 1, 4096, 4096, 4096])
            self.assertEqual([kw.get("seed", "default") for _, kw in launches],
                             ["default", None, None, None] * 2)
            self.assertEqual(act.call_count, 12)  # push + five final waits, twice
            self.assertEqual(wait.call_count, 2)  # one existing mid-fall decision, twice


class SimpleNamespaceContext:
    def __init__(self, value):
        self.value = value

    def __enter__(self):
        return self.value

    def __exit__(self, *_args):
        return False


if __name__ == "__main__":
    unittest.main()
