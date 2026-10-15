"""Ordered strike disclosure and input through real, normally validated clients."""
import json
import struct
import threading
import unittest

from process_harness import ProcessTestCase, ROOT, SPECTATOR_TOKEN, underfoot
from strike_sequence_package import sequence_package
from stream_relay import StreamRelay, exact


class ContinuationRetryRelay(StreamRelay):
    """Retain one real client continuation frame and replay its exact wire identity."""
    def __init__(self, upstream):
        self._send_lock = threading.Lock()
        self._captured = threading.Event()
        self._retry_frame = None
        self.retry_request_id = None
        super().__init__(upstream)

    def forward(self, source, target):
        try:
            header = bytearray()
            while not header.endswith(b"\r\n\r\n"):
                header.extend(exact(source, 1))
                if len(header) > 65536:
                    raise ValueError("Oversized client handshake")
            with self._send_lock:
                target.sendall(header)
            while True:
                prefix = exact(source, 2)
                length = prefix[1] & 127
                extra = b""
                if length in (126, 127):
                    extra = exact(source, 2 if length == 126 else 8)
                    length = struct.unpack("!H" if length == 126 else "!Q", extra)[0]
                if length > 16 * 1024 * 1024:
                    raise ValueError("Oversized test client frame")
                if not prefix[1] & 128:
                    raise ValueError("Unmasked client frame")
                mask = exact(source, 4)
                payload = exact(source, length)
                frame = prefix + extra + mask + payload
                if prefix[0] == 0x81 and self._retry_frame is None:
                    message = json.loads(bytes(value ^ mask[index % 4] for index, value in enumerate(payload)))
                    request = message.get("request") or {}
                    command = request.get("command") or {}
                    if request.get("type") == "command" and command.get("type") == "resume_intention":
                        self._retry_frame = frame
                        self.retry_request_id = message["request_id"]
                        self._captured.set()
                with self._send_lock:
                    target.sendall(frame)
        except (EOFError, OSError):
            pass
        except Exception as error:
            self.errors.append(error)
        finally:
            self.close_sockets()

    def retry_continuation(self):
        if not self._captured.wait(15):
            raise AssertionError("No actual headless continuation frame was captured")
        with self._send_lock:
            self._server_socket.sendall(self._retry_frame)


def combat(frame):
    return frame["state"]["observation"]["combat"]


def event(frame, ordinal, *, outcome=None):
    return next((value for value in combat(frame)["events"] if value["type"] == "ability"
        and value["ability"]["id"] == "triple_strike" and value["strike"] == ordinal
        and (outcome is None or value["outcome"] == outcome)), None)


def prefix(frame):
    progress = combat(frame)["sequence"]
    return progress is not None and progress["completed"] == 1


class StrikeSequenceProcesses(ProcessTestCase):
    def test_headless_saved_prefix_reconnect_resume_and_spectator_disclosure(self):
        package = sequence_package(self, autonomous=False)
        server = self.server(scenario=package)
        player, initial = self.client()
        target, target_initial = self.client(actor=2)
        sentinel = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                        if actor["name"] == "Sequence sentinel")
        admitted = self.act(player, {"type": "use_ability", "id": "triple_strike", "invocation": {"kind": "actor_target", "target": sentinel}})
        self.assertIsNone(admitted["error"])
        identity = admitted["intentions"][0]["intention"]
        self.act(target, {"type": "wait"})
        first = self.frame(player, prefix)
        self.assertEqual(combat(first)["sequence"]["total"], 3)
        self.assertEqual(first["intentions"][0]["phase"], "started")
        boundary = self.request(player, {"type": "snapshot"})
        self.assertTrue(prefix(boundary))
        self.assertEqual(boundary["state"]["observation"]["tick"], "150")
        self.assertEqual(combat(boundary)["sequence"], {"completed": 1, "total": 3,
            "next_strike_remaining": "50", "cancel_recovery_remaining": "250"})
        # The two-cell-tall body cannot step above this two-cell-tall chamber.
        invalid = self.command(player, {"type": "cancel_intention", "step": "up"})
        self.assertIsNotNone(invalid["error"])
        for field in ("state", "intentions", "history"):
            self.assertEqual(invalid[field], boundary[field], "invalid escape must preserve the whole prefix")
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        server.stop(); player.stop(); target.stop()
        server = self.server(scenario=package, seed=None)
        upstream = self.address
        relay = ContinuationRetryRelay(upstream)
        self.addCleanup(relay.close)
        self.address = relay.address
        try:
            player, restored = self.client()
        finally:
            self.address = upstream
        target, _ = self.client(actor=2)
        self.assertTrue(prefix(restored))
        self.assertEqual(combat(restored)["sequence"], combat(boundary)["sequence"])
        self.assertFalse(combat(restored)["preparation_active"])
        self.assertEqual(restored["intentions"][0]["intention"], identity)
        self.assertEqual(restored["intentions"][0]["phase"], "paused")
        observer, watched = self.client(SPECTATOR_TOKEN)
        self.assertEqual(combat(watched)["sequence"], combat(restored)["sequence"])
        self.assertEqual(watched["readiness"]["cancel"], [])
        denied = self.command(observer, {"type": "cancel_intention", "step": "north_west"})
        self.assertEqual(denied["error"], "Spectator access is read-only")
        resumed = self.command(player, {"type": "resume_intention"})
        self.assertIsNone(resumed["error"])
        self.assertEqual(resumed["intentions"][0]["intention"], identity)
        acknowledgements = [json.loads(line).get("message") for line in player.transcript
                            if line.startswith("{")]
        acknowledgements = [message for message in acknowledgements if message
                            and message["type"] == "ack" and message["request_id"] == relay.retry_request_id]
        self.assertEqual(len(acknowledgements), 1)
        resume_receipt = acknowledgements[0]["receipt"]
        self.assertEqual(resume_receipt["type"], "immediate")
        self.assertEqual(resume_receipt["actor"], "1")
        self.assertEqual(resume_receipt["branch"], restored["branch"])
        self.assertIsNotNone(resume_receipt["entry_id"])
        self.assertNotEqual(resume_receipt["entry_id"], identity)
        started = self.frame(player, lambda frame: prefix(frame) and combat(frame)["preparation_active"])
        repeated = self.command(player, {"type": "resume_intention"})
        self.assertEqual(repeated["error"], "No suspended action to resume")
        self.assertEqual(repeated["state"], started["state"], "repeated input must not replay the paid prefix")
        saved = self.request(player, {"type": "save"})
        self.assertIsNone(saved["error"])
        relay.retry_continuation()
        retry = self.frame(player, lambda frame: (frame.get("message") or {}).get("type") == "ack"
            and frame["message"]["request_id"] == relay.retry_request_id)
        self.assertEqual(retry["message"]["receipt"], resume_receipt)
        self.assertEqual(retry["state"], saved["state"], "same wire request must not replay a strike")
        self.assertEqual(retry["intentions"], saved["intentions"])
        self.assertEqual(retry["intentions"][0]["intention"], identity)
        self.assertEqual(retry["history"], saved["history"])
        self.assertEqual(retry["cursor"], saved["cursor"])
        self.assertEqual(retry["message"]["context"]["cursor"], saved["cursor"])
        # The other ordinary controlled actor permits the remaining due strikes.
        self.act(target, {"type": "wait"})
        second = self.frame(player, lambda frame: event(frame, 1) is not None)
        self.assertEqual(second["intentions"][0]["phase"], "started")
        self.act(target, {"type": "wait"})
        finished = self.frame(player, lambda frame: combat(frame)["sequence"] is None
                              and not frame["intentions"])
        self.assertIsNone(finished["error"])
        target_final = self.request(target, {"type": "snapshot"})
        self.assertEqual(combat(target_final)["hp"], combat(target_initial)["hp"] - 3)
        # The resumed stream never presents strike zero a second time.
        ordinals = []
        for line in player.transcript:
            if not line.startswith("{"):
                continue
            frame = json.loads(line)
            message = frame.get("message") or {}
            body = (message.get("update") or {}).get("body") or {}
            if body.get("type") in ("observation", "observation_delta"):
                ordinals.extend(value["strike"] for value in combat(frame)["events"]
                    if value["type"] == "ability" and value["ability"]["id"] == "triple_strike")
        self.assertEqual(ordinals, [1, 2])

    def test_natural_source_saved_prefix_resumes_only_unpaid_strikes(self):
        package = sequence_package(self, autonomous=False, natural_source=True)
        server = self.server(scenario=package)
        player, initial = self.client()
        target, target_initial = self.client(actor=2)
        def stock(frame):
            return sum(int(item["quantity"]) for item in frame["state"]["observation"]["inventory"]
                       if (item["known_supply"] or {}).get("id") == "bandage")
        self.assertEqual(stock(initial), 2)
        sentinel = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                        if actor["name"] == "Sequence sentinel")
        admitted = self.act(player, {"type": "use_ability", "id": "triple_strike",
            "invocation": {"kind": "actor_target", "target": sentinel}})
        self.assertIsNone(admitted["error"])
        identity = admitted["intentions"][0]["intention"]
        self.act(target, {"type": "wait"})
        boundary = self.frame(player, prefix)
        self.assertEqual(stock(boundary), 1, "first resolution spends one finite charge")
        self.assertEqual(combat(self.request(target, {"type": "snapshot"}))["hp"],
                         combat(target_initial)["hp"] - 6)
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        server.stop(); player.stop(); target.stop()
        # Saved compiled sources are sufficient even after original text is gone.
        import shutil
        import subprocess
        shutil.rmtree(package)
        fallback = self.directory / "unrelated-fallback"
        shutil.copytree(ROOT / "scenarios/two-room", fallback)
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", fallback],
                                   capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        server = self.server(scenario=fallback, seed=None)
        player, restored = self.client()
        target, _ = self.client(actor=2)
        self.assertEqual(combat(restored)["sequence"], combat(boundary)["sequence"])
        self.assertEqual(stock(restored), 1)
        self.assertEqual(restored["intentions"][0]["intention"], identity)
        observer, watched = self.client(SPECTATOR_TOKEN)
        self.assertEqual(combat(watched)["sequence"], combat(restored)["sequence"])
        self.assertIsNone(self.command(player, {"type": "resume_intention"})["error"])
        self.act(target, {"type": "wait"})
        self.frame(player, lambda frame: event(frame, 1) is not None)
        self.act(target, {"type": "wait"})
        finished = self.frame(player, lambda frame: combat(frame)["sequence"] is None and not frame["intentions"])
        self.assertEqual(stock(finished), 1, "resumed riders and strikes do not repay the finite cost")
        self.assertEqual(combat(self.request(target, {"type": "snapshot"}))["hp"],
                         combat(target_initial)["hp"] - 18)

    def test_rewind_clears_client_prefix_and_rejects_old_escape_context(self):
        package = sequence_package(self, autonomous=False)
        self.server(scenario=package, wizard=True)
        player, initial = self.client()
        target, _ = self.client(actor=2)
        sentinel = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                        if actor["name"] == "Sequence sentinel")
        self.act(player, {"type": "use_ability", "id": "triple_strike", "invocation": {"kind": "actor_target", "target": sentinel}})
        self.act(target, {"type": "wait"})
        self.frame(player, prefix)
        old = self.request(player, {"type": "snapshot"})
        identity = old["intentions"][0]["intention"]
        wizard = self.wizard()
        self.wizard_command(wizard, "rewind initial")
        reset = self.request(player, {"type": "snapshot"})
        self.assertIsNone(combat(reset)["sequence"])
        self.assertEqual(reset["intentions"], [])
        self.assertNotEqual(reset["branch"], old["branch"])
        denied = self.request(player, {"type": "command", "context": old["input_context"],
            "branch": old["branch"], "command": {"type": "cancel_intention", "intention": identity,
            "expected_revision": old["state"]["revision"], "step": "north_west"}})
        self.assertIsNotNone(denied["error"])
        self.assertEqual(denied["state"], reset["state"])

    def test_real_text_presents_prefix_debt_and_executes_explicit_escape(self):
        package = sequence_package(self, autonomous=False)
        self.server(scenario=package)
        player, initial = self.client()
        target, _ = self.client(actor=2)
        sentinel = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                        if actor["name"] == "Sequence sentinel")
        self.act(player, {"type": "use_ability", "id": "triple_strike", "invocation": {"kind": "actor_target", "target": sentinel}})
        self.act(target, {"type": "wait"})
        self.frame(player, prefix)
        self.request(player, {"type": "release_control"})
        text, welcome = self.adventure()
        self.assertIn("Sequence paused after strike 1 of 3", welcome)
        boundary = self.request(player, {"type": "snapshot"})
        debt = combat(boundary)["sequence"]["cancel_recovery_remaining"]
        self.assertIn(f"Cancel leaves {debt} ticks of recovery", welcome)
        before = underfoot(boundary["state"])["key"]
        result = self.say(text, "escape northwest")
        self.assertNotIn("not available", result)
        after = self.request(player, {"type": "snapshot"})
        self.assertIsNone(combat(after)["sequence"])
        self.assertNotEqual(underfoot(after["state"])["key"], before)
        self.assertGreaterEqual(int(combat(after)["recovery_remaining"]), int(debt))


class NativeStrikeSequenceProcesses(ProcessTestCase):
    graphical = True

    def test_native_escape_between_enemy_strikes_presents_miss_and_preserves_health(self):
        package = sequence_package(self, autonomous=True)
        self.server(scenario=package)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames"])
        initial = self.ascii_frame(window, lambda frame: frame["has_control"] and not frame["busy"])
        observer, _ = self.client(SPECTATOR_TOKEN)
        narrator, _ = self.adventure(SPECTATOR_TOKEN)
        key = self.native_keys(window)
        key("period", True)
        try:
            first = self.ascii_frame(window, lambda frame: event(frame, 0, outcome="applied") is not None)
        finally:
            key("period", False)
        hp = combat(first)["hp"]
        self.assertLess(hp, combat(initial)["hp"])
        self.assertIsNone(combat(first)["sequence"], "opponent totals must stay private")
        key("y", True)  # Genuine native northwest movement, supported on both hosts.
        try:
            missed = self.ascii_frame(window, lambda frame: event(frame, 1, outcome="miss") is not None)
        finally:
            key("y", False)
        self.assertEqual(combat(missed)["hp"], hp)
        self.assertNotEqual(underfoot(missed["state"])["key"], underfoot(initial["state"])["key"])
        prose = " ".join(missed["narration"] + missed["messages"]["log"])
        self.assertIn("Triple Strike (strike 2)", prose)
        self.assertIn("missed", prose)
        heard = narrator.until(lambda line: "Triple Strike (strike 2)" in line)
        self.assertIn("misses", heard)
        authoritative = self.request(observer, {"type": "snapshot"})
        self.assertEqual(combat(authoritative)["hp"], hp)
        self.assertFalse(authoritative["state"]["wizard_game"])


if __name__ == "__main__":
    unittest.main()
