"""Finite ground claims, cancellation and replay through ordinary real clients."""
from scalar_wire import u64
import json
import shutil
import struct
import threading

from process_harness import ProcessTestCase, SPECTATOR_TOKEN
from stream_relay import StreamRelay, exact
from supply_package import prepare


def observation(frame):
    return frame["state"]["observation"]


def combat(frame):
    return observation(frame)["combat"]


def stock(frame):
    items = observation(frame)["inventory"] + [row["item"] for row in observation(frame)["ground_items"]]
    return sum(int(item["quantity"]) for item in items
               if (item["known_supply"] or {}).get("id") == "bandage")


def cooldown(frame):
    return int(next(ability["cooldown_remaining"] for ability in combat(frame)["own_stats"]["abilities"]
                    if ability["id"] == "bind_wounds"))


def care(frame, item=None):
    if item is None:
        item = next(row["item"]["id"] for row in observation(frame)["ground_items"]
                    if (row["item"]["known_supply"] or {}).get("id") == "bandage")
    return {"type": "use_ability", "id": "bind_wounds", "invocation": {"kind": "actor_target", "target": observation(frame)["self_target"]},
            "supplies": {"kind": "explicit", "items": [{"cost_ordinal": 0, "item": item}]}}


class SupplyRetryRelay(StreamRelay):
    """Capture the actual first client care request for exact post-spend retry."""
    def __init__(self, upstream):
        self.send_lock = threading.Lock()
        self.captured = threading.Event()
        self.saved_frame = None
        self.request_id = None
        super().__init__(upstream)

    def forward(self, source, target):
        try:
            header = bytearray()
            while not header.endswith(b"\r\n\r\n"):
                header.extend(exact(source, 1))
                if len(header) > 65536:
                    raise ValueError("Oversized client handshake")
            with self.send_lock:
                target.sendall(header)
            while True:
                prefix = exact(source, 2)
                length = prefix[1] & 127
                extra = b""
                if length in (126, 127):
                    extra = exact(source, 2 if length == 126 else 8)
                    length = struct.unpack("!H" if length == 126 else "!Q", extra)[0]
                if length > 16 * 1024 * 1024 or not prefix[1] & 128:
                    raise ValueError("Invalid test client frame")
                mask = exact(source, 4)
                payload = exact(source, length)
                frame = prefix + extra + mask + payload
                if prefix[0] == 0x81 and self.saved_frame is None:
                    message = json.loads(bytes(value ^ mask[index % 4]
                                              for index, value in enumerate(payload)))
                    command = (message.get("request") or {}).get("command") or {}
                    action = command.get("action") or {}
                    if action.get("type") == "use_ability" and action.get("id") == "bind_wounds":
                        self.saved_frame = frame
                        self.request_id = message["request_id"]
                        self.captured.set()
                with self.send_lock:
                    target.sendall(frame)
        except (EOFError, OSError):
            pass
        except Exception as error:
            self.errors.append(error)
        finally:
            self.close_sockets()

    def retry(self, frame=None):
        if frame is None:
            if not self.captured.wait(15):
                raise AssertionError("No actual Bind Wounds request was captured")
            frame = self.saved_frame
        with self.send_lock:
            self._server_socket.sendall(frame)


class SupplyLifecycleProcesses(ProcessTestCase):
    def finish(self, player, partner):
        for _ in range(8):
            current = self.request(player, {"type": "snapshot"})
            if combat(current)["preparation_remaining"] is None:
                return current
            self.assertTrue(combat(current)["preparation_active"])
            # Resume and prior waits publish revisions across client connections.
            self.assertIsNone(self.request(partner, {"type": "snapshot"})["error"])
            self.assertIsNone(self.act(partner, {"type": "wait"})["error"])
        self.fail("Ordinary partner waits did not finish care")

    def test_last_ground_claim_is_private_and_cancel_releases_for_other_actor(self):
        self.server(scenario=prepare(self))
        first, initial = self.client()
        second, other = self.client(actor=2)
        before_items = observation(other)["ground_items"]
        started = self.act(first, care(initial))
        self.assertIsNone(started["error"])
        self.assertEqual(stock(started), 1)
        self.assertEqual(cooldown(started), 1200)
        other = self.request(second, {"type": "snapshot"})
        self.assertEqual(observation(other)["ground_items"], before_items)
        supplies = [row for row in before_items if row["item"]["known_supply"]]
        self.assertEqual(len(supplies), 1)
        self.assertTrue(supplies[0]["supply_reachable"])
        self.assertEqual(supplies[0]["item"]["known_supply"],
                         {"id": "bandage", "name": "Bandage"})
        for item in before_items:
            if item["item"]["known_supply"] is None:
                self.assertFalse(item["supply_reachable"])
        lost = self.act(second, care(other))
        self.assertTrue(lost["error"] is not None or any(
            (json.loads(line).get("message") or {}).get("update", {}).get("body", {}).get("status", {}).get("phase") == "failed"
            for line in second.transcript if line.startswith("{")))
        self.assertEqual(cooldown(lost), 0)
        self.assertEqual(stock(lost), 1)
        cancelled = self.command(first, {"type": "cancel_intention"})
        self.assertIsNone(cancelled["error"])
        self.assertEqual(cooldown(cancelled), 1200)
        self.assertEqual(stock(cancelled), 1)
        self.assertIsNone(self.act(first, {"type": "wait"})["error"])
        other = self.request(second, {"type": "snapshot"})
        accepted = self.act(second, care(other))
        self.assertIsNone(accepted["error"])
        finished = self.finish(second, first)
        self.assertEqual(stock(finished), 0)
        self.assertEqual(u64(combat(finished)["hp"]), u64(combat(finished)["max_hp"]))
        self.assertFalse(finished["state"]["wizard_game"])

    def test_source_deleted_restart_resumes_exact_claim_and_retries_after_spend(self):
        package = prepare(self)
        # Startup validates its supplied package even when opening a saved game.
        # Use an independently compiled, deliberately different fallback; the
        # deleted original and its finite stack must come from the save alone.
        fallback = prepare(self, bandages=0)
        server = self.server("--checkpoint-interval", 4096, scenario=package)
        upstream = self.address
        relay = SupplyRetryRelay(upstream)
        self.addCleanup(relay.close)
        self.address = relay.address
        first, initial = self.client()
        self.address = upstream
        second, _ = self.client(actor=2)
        started = self.act(first, care(initial))
        self.assertIsNone(started["error"])
        original_request, original_id = relay.saved_frame, relay.request_id
        self.assertIsNotNone(original_request)
        saved = self.request(first, {"type": "save"})
        self.assertEqual(stock(saved), 1)
        self.assertTrue(combat(saved)["preparation_active"])
        identity = saved["intentions"][0]["intention"]
        second.stop(); first.stop(); server.stop()
        shutil.rmtree(package)
        server = self.server("--checkpoint-interval", 4096, scenario=fallback, seed=None)
        upstream = self.address
        restored_relay = SupplyRetryRelay(upstream)
        self.addCleanup(restored_relay.close)
        self.address = restored_relay.address
        first, restored = self.client()
        self.address = upstream
        second, _ = self.client(actor=2)
        self.assertEqual(stock(restored), 1)
        self.assertEqual(cooldown(restored), cooldown(saved))
        self.assertEqual(restored["intentions"][0]["intention"], identity)
        self.assertFalse(combat(restored)["preparation_active"])
        observer, watched = self.client(SPECTATOR_TOKEN)
        self.assertEqual(stock(watched), 1)
        denied = self.command(observer, {"type": "resume_intention"})
        self.assertEqual(denied["error"], "Spectator access is read-only")
        resumed = self.command(first, {"type": "resume_intention"})
        self.assertIsNone(resumed["error"])
        finished = self.finish(first, second)
        self.assertEqual(stock(finished), 0)
        self.assertEqual(u64(combat(finished)["hp"]), u64(combat(finished)["max_hp"]))
        saved = self.request(first, {"type": "save"})
        restored_relay.retry(original_request)
        repeated = self.frame(first, lambda frame: (frame.get("message") or {}).get("type") == "ack"
                              and frame["message"]["request_id"] == original_id)
        self.assertEqual(repeated["state"], saved["state"])
        self.assertEqual(repeated["message"]["receipt"]["intention"], identity)
        self.assertEqual(stock(repeated), 0)
        first.stop(); second.stop(); observer.stop(); server.stop()
        self.server(scenario=fallback, seed=None)
        _, reopened = self.client()
        self.assertEqual(reopened["state"], saved["state"])
        self.assertFalse(reopened["state"]["wizard_game"])
        self.assertEqual(relay.errors + restored_relay.errors, [])

    def test_stale_selection_after_other_actor_takes_stack_is_atomic(self):
        self.server(scenario=prepare(self))
        first, initial = self.client()
        second, other = self.client(actor=2)
        stale = care(other)
        item = next(row["item"]["id"] for row in observation(initial)["ground_items"]
                    if row["item"]["known_supply"])
        self.assertIsNone(self.act(first, {"type": "move", "direction": "east"})["error"])
        self.assertIsNone(self.act(second, {"type": "wait"})["error"])
        # Another client's wait publishes a new revision asynchronously. Refresh
        # the taker's context; the second actor's explicit item selection stays
        # unchanged so the refusal below still tests the consumed supply handle.
        self.assertIsNone(self.request(first, {"type": "snapshot"})["error"])
        self.assertIsNone(self.act(first, {"type": "take", "item": item})["error"])
        before = self.request(second, {"type": "snapshot"})
        denied = self.act(second, stale)
        self.assertIsNotNone(denied["error"])
        self.assertEqual(denied["state"], before["state"])
        self.assertEqual(denied["history"], before["history"])
        self.assertEqual(cooldown(denied), 0)

    def test_ordinary_hostile_ai_interrupts_care_before_any_supply_is_spent(self):
        self.server(scenario=prepare(self, hostile=True, autonomous=True))
        player, initial = self.client()
        started = self.act(player, care(initial))
        self.assertIsNone(started["error"])

        def interrupted(frame):
            return (u64(combat(frame)["hp"]) < u64(combat(initial)["hp"])
                    and not combat(frame)["preparation_active"]
                    and combat(frame)["preparation_remaining"] is not None)

        paused = started if interrupted(started) else self.frame(player, interrupted)
        self.assertEqual(stock(paused), 1)
        self.assertGreater(cooldown(paused), 0)
        self.assertLess(cooldown(paused), 1200)
        self.assertTrue(any(event["type"] == "interrupted" for event in combat(paused)["events"]))
        cancelled = self.command(player, {"type": "cancel_intention"})
        self.assertIsNone(cancelled["error"])
        self.assertEqual(stock(cancelled), 1)
        before = self.request(player, {"type": "snapshot"})
        denied = self.act(player, care(before))
        self.assertIsNotNone(denied["error"])
        self.assertEqual(denied["state"], before["state"])
        self.assertEqual(denied["history"], before["history"])
        self.assertFalse(denied["state"]["wizard_game"])
