"""Preserve the legacy acknowledgement boundary while exposing driver delay."""
import io
import json
import queue
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch, Mock
from performance_driver import DiagnosticLog, JsonProcess, wall_time_ns, stop_all, same_disclosed_state


class DisclosedStateComparison(unittest.TestCase):
    def test_cross_attachment_identity_is_not_disclosed_content(self):
        first = dict(snapshot="first", revision="78", wizard_game=False,
                     observation=dict(tick="100", inventory=[dict(id="item", quantity="1")]))
        second = dict(first, snapshot="second", revision="1")
        self.assertTrue(same_disclosed_state(first, second))
        self.assertEqual(first["snapshot"], "first", "comparison must not mutate input")
        self.assertFalse(same_disclosed_state(None, second))
        self.assertFalse(same_disclosed_state(first, None))
        self.assertFalse(same_disclosed_state(first, dict(second, wizard_game=True)))
        self.assertFalse(same_disclosed_state(first, dict(second, observation=dict(tick="200", inventory=[]))))
        self.assertFalse(same_disclosed_state(first, dict(second, extra="must not disappear")))


    def test_wait_presentation_requires_exact_receipt_and_every_visual_field(self):
        from performance_driver import presented_action
        state = dict(snapshot="fresh", revision="2", observation=dict(tick="200", ready=True, inventory=[]))
        entry = dict(id="wait", tick="200", content=dict(event=dict(type="waited")))
        expected = dict(state=state, history=[entry])
        prior = dict(state={**state, "snapshot":"old", "revision":"1",
                            "observation":{**state["observation"], "tick":"100"}}, history=[entry])
        self.assertTrue(presented_action(prior, expected, {"type":"wait"}))
        self.assertFalse(presented_action(prior, expected, {"type":"move"}))
        self.assertFalse(presented_action({**prior, "history":[]}, expected, {"type":"wait"}))
        changed = {**prior, "state":{**prior["state"], "observation":{
            **prior["state"]["observation"], "inventory":[dict(id="unexpected")]}}}
        self.assertFalse(presented_action(changed, expected, {"type":"wait"}))


class PlacePictureDemand(unittest.TestCase):
    def test_spectator_wait_requires_every_public_entry_but_no_private_wizard_history(self):
        from place_performance_driver import presented_place_command
        state = dict(snapshot="new", revision="2", observation=dict(tick="100", inventory=[]))
        private = dict(id="setup", audience="private", content=dict(type="wizard"))
        rename = dict(id="rename", audience="actor", content=dict(type="place_renamed"))
        waited = dict(id="wait", audience="actor", content=dict(type="action", event=dict(type="waited")))
        expected = dict(state=state, history=[private, rename, waited])
        shown = dict(state={**state, "snapshot":"spectator", "revision":"1",
                            "observation":{**state["observation"], "tick":"0"}},
                     history=[rename, waited])
        action = dict(type="wait")
        self.assertTrue(presented_place_command(shown, expected, action))
        self.assertFalse(presented_place_command({**shown, "history":[waited]}, expected, action))
        self.assertFalse(presented_place_command({**shown, "history":expected["history"]}, expected, action))
        changed = {**shown, "state":{**shown["state"], "observation":{
            **shown["state"]["observation"], "inventory":[dict(id="unexpected")]}}}
        self.assertFalse(presented_place_command(changed, expected, action))

    def test_current_picture_demand_keeps_causal_ready_metrics(self):
        from place_performance_driver import send_observed_command
        causal = dict(error=None, state=dict(snapshot="old"), memory=["known"])
        current = dict(error=None, state=dict(snapshot="current"), memory=["known", "new"])
        class Client:
            last_reader_work_ms = 2
            last_queue_delay_ms = 3
            def send(self, request):
                self.request = request
                return causal, 10., 11., 12.
            def observe(self):
                self.last_reader_work_ms = 20
                self.last_queue_delay_ms = 30
                return current, 14.
        client = Client()
        request = {"type": "request", "request": {"type": "command"}}
        state, start, ack, ready, metrics = send_observed_command(client, request)
        self.assertIs(client.request, request)
        self.assertIs(state, current)
        self.assertEqual((start, ack, ready), (10., 11., 12.))
        self.assertEqual(metrics, dict(ready_reader_work_ms=2, ready_queue_delay_ms=3,
            headless_memory_cells=1, headless_ready_bytes=len(json.dumps(causal).encode("utf-8")),
            request_to_observation_ms=4000.))


class SavedExplorationInput(unittest.TestCase):
    def client(self, frames):
        client = JsonProcess.__new__(JsonProcess)
        client.name = "ascii"
        client.child = SimpleNamespace(stdin=io.StringIO(), wait=lambda **_: 1)
        client.lines = queue.Queue()
        for frame, received in frames:
            client.lines.put((frame, received))
        return client

    def frame(self, *, done=None, admit=False, phase=None, revision="76"):
        return dict(input_done=done, busy=False, connected=True, window_open=True,
                    has_control=True,
                    can_admit_intention=admit,
                    intentions=[dict(phase=phase)] if phase else [],
                    state=dict(revision=revision, observation=dict(ready=True)))

    def test_sequential_exploration_waits_past_effect_for_ordered_admission(self):
        from saved_exploration_driver import key
        # Windows release presented the right acknowledgement, then its moved
        # observation and cleared intention before the next admission permission.
        unrelated = self.frame(done="left", admit=True, revision="75")
        acknowledged = self.frame(done="right", phase="queued", revision="75")
        executed = self.frame()
        permitted = self.frame(admit=True)
        client = self.client([(unrelated, 101.), (acknowledged, 102.),
                              (executed, 103.), (permitted, 104.)])
        with patch("saved_exploration_driver.time.perf_counter", return_value=100.):
            frame, elapsed = key(client, "right")
        self.assertIs(frame, permitted, "The next key needs authoritative admission")
        self.assertEqual(elapsed, 4000., "Timing must use the returned frame's receipt")
        self.assertEqual(json.loads(client.child.stdin.getvalue()),
                         {"type": "key", "key": "right"})

    def test_exploration_waits_for_picture_after_causal_admission(self):
        from saved_exploration_driver import key
        causal = self.frame(done="right", admit=True)
        causal["state"]["snapshot"] = "before"
        picture = self.frame(admit=True)
        picture["state"]["snapshot"] = "after"
        client = self.client([(causal, 101.), (picture, 103.)])
        with patch("saved_exploration_driver.time.perf_counter", return_value=100.):
            frame, elapsed = key(client, "right", picture_after="before")
        self.assertIs(frame, picture)
        self.assertEqual(elapsed, 3000., "Presentation timing includes actual picture receipt")
        self.assertEqual(json.loads(client.child.stdin.getvalue()), {"type":"key", "key":"right"})

    def test_explicit_observe_preserves_action_timing_and_does_not_reset(self):
        expected = {"type":"ready", "error":None, "state":{"snapshot":"fresh"}}
        client = self.client([(expected, 103.)])
        client.action_timing = {"request_to_outcome_ms": 1000.}
        client.ack_line_received = 101.
        client.ack_request_id = "original-action"
        frame, received = client.observe()
        self.assertIs(frame, expected)
        self.assertEqual(received, 103.)
        self.assertEqual(client.action_timing, {"request_to_outcome_ms":1000.})
        self.assertEqual(client.ack_line_received, 101.)
        self.assertEqual(client.ack_request_id, "original-action")
        self.assertEqual(json.loads(client.child.stdin.getvalue()),
                         {"type":"request", "request":{"type":"observe"}})

    def test_already_permitted_input_returns_its_acknowledged_frame(self):
        from saved_exploration_driver import key
        expected = self.frame(done="right", admit=True)
        client = self.client([(expected, 101.)])
        with patch("saved_exploration_driver.time.perf_counter", return_value=100.):
            frame, elapsed = key(client, "right")
        self.assertIs(frame, expected)
        self.assertEqual(elapsed, 1000.)

    def test_release_returns_without_requiring_action_permission(self):
        from saved_exploration_driver import key
        expected = {**self.frame(done="release"), "has_control": False}
        client = self.client([(expected, 101.)])
        with patch("saved_exploration_driver.time.perf_counter", return_value=100.):
            frame, elapsed = key(client, "release")
        self.assertIs(frame, expected)
        self.assertEqual(elapsed, 1000.)

    def test_terminal_exploration_fails_without_waiting_for_admission(self):
        from saved_exploration_driver import key
        for changed, reason in [
                ({"connected": False}, "disconnected"),
                ({"window_open": False}, "closed"),
                ({"has_control": False}, "control"),
                ({"state": {"observation": {"combat": {"dead": True}}}}, "died"),
                ({"state": {"observation": {"combat": {"terminal": True}}}}, "ended")]:
            with self.subTest(reason=reason):
                terminal = {**self.frame(done="right"), **changed}
                poison = {"type": "fatal", "error": "Wait continued after terminal frame"}
                client = self.client([(terminal, 101.), (poison, 102.)])
                with self.assertRaisesRegex(RuntimeError, reason):
                    key(client, "right")
                self.assertEqual(client.lines.qsize(), 1)

    def test_fatal_error_and_process_exit_end_admission_wait(self):
        from saved_exploration_driver import key
        for failure, reason in [({"type": "fatal", "error": "retained reader error"},
                                 "retained reader error"),
                                (None, "Owned process exited")]:
            with self.subTest(reason=reason):
                client = self.client([(self.frame(done="right"), 101.), (failure, 102.)])
                with self.assertRaisesRegex(RuntimeError, reason):
                    key(client, "right")

    def test_missing_admission_keeps_existing_absolute_deadline(self):
        from saved_exploration_driver import key
        client = self.client([(self.frame(done="right"), 101.)])
        with patch("performance_driver.time.monotonic", side_effect=[0., 0., 0., 0., 30.]):
            with self.assertRaisesRegex(RuntimeError, "deadline"):
                key(client, "right")


class DriverDeadlines(unittest.TestCase):
    def client(self, lines):
        client = JsonProcess.__new__(JsonProcess)
        client.lines = lines
        client.name = "headless"
        client.child = SimpleNamespace(wait=lambda **_: 0)
        return client

    def test_expired_deadline_rejects_a_matching_already_queued_frame(self):
        lines = queue.Queue()
        lines.put(({"type": "ready"}, 0.))
        with patch("performance_driver.time.monotonic", return_value=0.):
            with self.assertRaisesRegex(RuntimeError, "deadline"):
                self.client(lines).until(lambda frame: True, seconds=0)
        self.assertEqual(lines.qsize(), 1, "expired waits must not drain buffered output")

    def test_frame_returned_at_deadline_cannot_be_accepted(self):
        now = [0.]
        def get(**_):
            now[0] = 1.
            return ({"type": "ready"}, 0.)
        with patch("performance_driver.time.monotonic", side_effect=lambda: now[0]):
            with self.assertRaisesRegex(RuntimeError, "deadline"):
                self.client(SimpleNamespace(get=get)).until(lambda frame: True, seconds=1)

    def test_slow_predicate_cannot_accept_after_its_absolute_deadline(self):
        now = [0.]
        lines = queue.Queue()
        lines.put(({"type": "ready"}, 0.))
        def match(frame):
            now[0] = 1.
            return True
        with patch("performance_driver.time.monotonic", side_effect=lambda: now[0]):
            with self.assertRaisesRegex(RuntimeError, "deadline"):
                self.client(lines).until(match, seconds=1)

    def test_timely_frame_keeps_its_recorded_receive_boundary(self):
        lines = queue.Queue()
        expected = {"type": "ready"}
        lines.put((expected, 12.))
        with patch("performance_driver.time.monotonic", return_value=0.):
            self.assertEqual(self.client(lines).until(lambda frame: True, seconds=1), (expected, 12.))


class DriverTiming(unittest.TestCase):
    def test_admitted_action_waits_for_matching_execution_without_changing_ack_timing(self):
        actions = [
            {"type": "act", "action": {"type": "wait"}},
            {"type": "request", "request": {"type": "command", "branch": "branch",
                "context": {"stream": {"stream": "s", "epoch": "1"}, "readiness_revision": "1"},
                "command": {"type": "act", "action": {"type": "wait"}}}},
        ]
        for value in actions:
            with self.subTest(input_type=value["type"]):
                client = JsonProcess.__new__(JsonProcess)
                client.lines = queue.Queue()
                client.child = SimpleNamespace(stdin=io.StringIO())
                receipt = dict(type="admitted", intention="original", actor="1",
                               branch="branch", phase="queued")
                client.lines.put(({"message": {"type": "ack", "receipt": receipt}}, 101.))
                client.lines.put(({"type": "ready", "history": []}, 102.))
                def update(identity, branch="branch", phase="resolved"):
                    return {"message": {"type": "update", "update": {"body": {
                        "type": "intention", "status": dict(intention=identity,
                        branch=branch, actor="1", phase=phase)}}}, "history": [identity],
                        "readiness": {"revision": "9"}}
                client.lines.put((update("other"), 103.))
                client.lines.put((update("original", branch="old"), 104.))
                client.lines.put((update("original", phase="queued"), 105.))
                client.lines.put((update("original"), 106.))
                client.lines.put(({"message": {"type": "update"}, "history": ["original"],
                                  "readiness": {"revision": "10"}, "input_context": "fresh"}, 107.))
                with patch("performance_driver.time.perf_counter", return_value=100.):
                    frame, start, ack, executed = client.send(value)
                self.assertEqual(frame["history"], ["original"])
                self.assertEqual((start, ack, executed), (100., 100., 107.))
                self.assertEqual(frame["input_context"], "fresh")
                self.assertEqual(client.ack_line_received, 101.)

    def test_completion_timing_distinguishes_execution_outcome_and_fresh_context(self):
        for phase in ("resolved", "failed", "cancelled", "suspended"):
            with self.subTest(phase=phase):
                client = JsonProcess.__new__(JsonProcess)
                client.lines = queue.Queue()
                client.child = SimpleNamespace(stdin=io.StringIO())
                receipt = dict(type="admitted", intention="original", actor="1",
                               branch="branch", phase="queued")
                client.lines.put(({"message": {"type": "ack", "receipt": receipt}}, 101.))
                client.lines.put(({"type": "ready"}, 102.))
                def update(outcome, actor="1", branch="branch", identity="original"):
                    return {"message": {"type": "update", "update": {"body": {
                        "type": "intention", "status": dict(intention=identity,
                        actor=actor, branch=branch, phase=outcome)}}},
                        "readiness": {"revision": "9"}}
                client.lines.put((update("started"), 103.))
                client.lines.put((update(phase, actor="2"), 104.))
                client.lines.put((update(phase, branch="old"), 105.))
                client.lines.put((update(phase, identity="other"), 106.))
                client.lines.put((update(phase), 107.))
                client.lines.put(({"readiness": {"revision": "9"}}, 108.))
                client.lines.put(({"readiness": {"revision": "10"},
                                  "input_context": "fresh"}, 109.))
                with patch("performance_driver.time.perf_counter", return_value=100.):
                    frame, start, ack, received = client.send(
                        {"type": "act", "action": {"type": "wait"}},
                        wait_for_completion=True)
                self.assertEqual((start, ack, received), (100., 100., 109.))
                self.assertEqual(frame["input_context"], "fresh")
                self.assertEqual(client.action_timing, {
                    "version": 1, "intention": "original", "actor": "1", "branch": "branch",
                    "outcome": phase, "request_to_admission_ms": 1000.,
                    "request_to_execution_ms": 3000., "request_to_outcome_ms": 7000.,
                    "request_to_context_ms": 9000.})

    def test_default_timing_stops_at_started_and_keeps_outcome_pending(self):
        client = JsonProcess.__new__(JsonProcess)
        client.lines = queue.Queue()
        client.child = SimpleNamespace(stdin=io.StringIO())
        receipt = dict(type="admitted", intention="original", actor="1",
                       branch="branch", phase="queued")
        client.lines.put(({"message": {"type": "ack", "receipt": receipt}}, 101.))
        client.lines.put(({"type": "ready"}, 102.))
        started = {"message": {"type": "update", "update": {"body": {
            "type": "intention", "status": {**receipt, "phase": "started"}}}}}
        client.lines.put((started, 103.))
        terminal = {"message": {"type": "update", "update": {"body": {
            "type": "intention", "status": {**receipt, "phase": "resolved"}}}}}
        client.lines.put((terminal, 104.))
        with patch("performance_driver.time.perf_counter", return_value=100.):
            frame, _, _, received = client.send({"type": "act"})
        self.assertEqual(frame, started)
        self.assertEqual(received, 103.)
        self.assertEqual(client.lines.qsize(), 1)
        self.assertIsNone(client.action_timing["outcome"])
        self.assertIsNone(client.action_timing["request_to_outcome_ms"])
        self.assertIsNone(client.action_timing["request_to_context_ms"])

    def test_failed_log_cleanup_preserves_failure_and_stops_every_owned_child(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            (path/'result.json').write_text('{}')
            good = SimpleNamespace(name='good', stop=Mock())
            bad = SimpleNamespace(name='bad', stop=Mock(side_effect=RuntimeError('log cap')))
            with self.assertRaisesRegex(RuntimeError, 'log cap'):
                stop_all([good,bad],path,dict(samples=[1]))
            good.stop.assert_called_once()
            self.assertFalse((path/'result.json').exists())
            self.assertTrue((path/'rejected-result.json').exists())
            self.assertIn('log cap',(path/'failure.json').read_text())

    def test_deferred_log_preserves_bytes_without_writing_during_measurement(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/"log"
            log = DiagnosticLog(path, True, limit=5)
            log.write("abc\n")
            log.flush()
            self.assertEqual(path.read_bytes(), b"")
            with self.assertRaisesRegex(RuntimeError, "cap exceeded"):
                log.write("xy")
            log.close()
            self.assertEqual(path.read_text(), "abc\n")

    def test_host_timestamp_uses_the_recorded_monotonic_boundary(self):
        with patch("performance_driver._host_clock_offset", return_value=42):
            self.assertEqual(wall_time_ns(1.25), 1250000042)

    def test_ack_line_timestamp_excludes_later_driver_logging_and_queue_delay(self):
        client = JsonProcess.__new__(JsonProcess)
        client.lines = queue.Queue()
        client.child = SimpleNamespace(stdin=io.StringIO())
        client.action_timing = {"outcome": "resolved"}
        client.lines.put(({"message":{"type":"ack"}}, 101.))
        client.lines.put(({"type":"ready"}, 102.))
        with patch("performance_driver.time.perf_counter", side_effect=[100.,103.,105.,106.]):
            _, start, legacy_ack, ready = client.send({"type":"act"})
        self.assertEqual((start, legacy_ack, ready), (100.,105.,102.))
        self.assertEqual(client.ack_line_received, 101.)
        self.assertIsNone(client.action_timing)
