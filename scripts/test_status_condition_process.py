"""Generic actor-active restrictions through real server, headless and Text apps."""
import json
import shutil
import unittest

from process_harness import ProcessTestCase, ROOT, SPECTATOR_TOKEN, WIZARD_TOKEN, resource, underfoot
from status_condition_package import (
    AbilityRetryRelay, DURATIONS, LAPSE, SELF_ABILITIES, STATUS_ABILITIES, STILLNESS, WORK, WOUND,
    actor_clock, actor_creature, checkpoint, combat, condition_remaining, observation, prepare,
)


def ability(frame, identity, target=None):
    return {"type": "use_ability", "id": identity,
            "invocation": {"kind": "actor_target", "target": target or observation(frame)["self_target"]},
            "supplies": {"kind": "carried"}}


def condition(saved, identity=STILLNESS, actor=1):
    return next((row for row in actor_creature(saved, actor)["conditions"]
                 if row["condition"] == identity), None)


def cooldown(frame):
    return int(next(row["cooldown_remaining"] for row in combat(frame)["own_stats"]["abilities"]
                    if row["id"] == WORK))


class StatusConditionHelpers:
    def snapshot(self, client):
        return self.request(client, {"type": "snapshot"})

    def retry_client(self):
        upstream = self.address
        relay = AbilityRetryRelay(upstream)
        self.addCleanup(relay.close)
        self.address = relay.address
        try:
            client, initial = self.client()
        finally:
            self.address = upstream
        return client, initial, relay

    def raw_command(self, client, command):
        """Bypass convenience input gates to exercise actual server authority."""
        before = self.snapshot(client)
        result = self.request(client, {"type": "command", "context": before["input_context"],
            "branch": before["branch"], "command": {
                "expected_revision": before["state"]["revision"], **command}})
        return before, result

    def assert_atomic_denial(self, client, command):
        before, denied = self.raw_command(client, command)
        self.assertIsNotNone(denied["error"])
        for field in ("state", "intentions", "history", "cursor"):
            self.assertEqual(denied[field], before[field], field)
        return denied

    def settled(self, client, frame):
        def ready(value):
            return (value.get("state") is not None and observation(value)["ready"]
                    and combat(value)["preparation_remaining"] is None
                    and int(combat(value)["recovery_remaining"]) == 0)
        return frame if ready(frame) else self.frame(client, ready)

    def finish(self, subject, peer):
        for _ in range(40):
            current = self.snapshot(subject)
            if (combat(current)["preparation_remaining"] is None and observation(current)["ready"]
                    and int(combat(current)["recovery_remaining"]) == 0):
                return current
            if combat(current)["preparation_remaining"] is not None:
                self.assertTrue(combat(current)["preparation_active"], "finishing requires explicit Resume")
            self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
        self.fail("The authored work did not finish within its bounded active duration")

    def resume(self, subject):
        accepted = self.command(subject, {"type": "resume_intention"})
        self.assertIsNone(accepted["error"])
        if not combat(accepted)["preparation_active"]:
            self.frame(subject, lambda frame: frame.get("state") is not None
                       and combat(frame)["preparation_active"])

    def inspection(self, wizard, actor=1):
        start = len(wizard.transcript)
        self.wizard_command(wizard, f"creature inspect {actor}")
        messages = [json.loads(line).get("message") for line in wizard.transcript[start:]
                    if line.startswith("{")]
        return next(message["report"] for message in reversed(messages)
                    if message and message["type"] == "creature_inspection")

    def injure(self, subject, peer):
        before = self.snapshot(subject)
        potion = next(item["id"] for item in observation(before)["inventory"] if item["class"] == "potion")
        self.assertIsNone(self.act(subject, {"type": "drink", "item": potion})["error"])
        result = self.finish(subject, peer)
        self.assertEqual(combat(result)["max_hp"] - combat(result)["hp"], 5)
        return result

    def paused_work(self, subject, peer, identity=STILLNESS, *, actor=1):
        before = self.snapshot(subject)
        started = self.act(subject, ability(before, WORK))
        self.assertIsNone(started["error"])
        self.assertTrue(combat(started)["preparation_active"])
        original = checkpoint(self, subject)
        source = self.snapshot(peer)
        name = "first subject" if actor == 1 else "second subject"
        target = next(row["id"] for row in observation(source)["visible_actors"] if row["name"] == name)
        applied = self.act(peer, ability(source, STATUS_ABILITIES[identity], target))
        self.assertIsNone(applied["error"])
        self.settled(peer, applied)
        paused = self.snapshot(subject)
        self.assertFalse(combat(paused)["preparation_active"])
        self.assertIsNotNone(combat(paused)["preparation_remaining"])
        self.assertEqual(paused["intentions"][0]["phase"], "paused")
        saved = checkpoint(self, subject)
        status = condition(saved, identity, actor)
        self.assertEqual(status["phase"], "active")
        # The cast has recovered for 25 ticks at this input boundary. Stored
        # creature operands remain lazy at their application anchor; observation
        # projects that elapsed active time without mutating the checkpoint.
        self.assertEqual(status["remaining"], DURATIONS[identity])
        self.assertEqual(actor_clock(saved, actor), saved["tick"])
        self.assertEqual(saved["tick"] - saved["actors"][str(actor)]["active_since"], 25)
        remaining = condition_remaining(saved, identity, actor)
        self.assertEqual(remaining, DURATIONS[identity] - 25)
        disclosed = next(row for row in combat(paused)["own_stats"]["conditions"]
                         if row["condition"]["id"] == identity)
        self.assertEqual(int(disclosed["remaining_ticks"]), remaining)
        pending = saved["actors"][str(actor)]["pending"]
        self.assertEqual(pending["plan"], original["actors"][str(actor)]["pending"]["plan"])
        self.assertEqual(pending["intention"], original["actors"][str(actor)]["pending"]["intention"])
        return before, paused, original, saved

    def expiry_boundary(self, subject, peer, saved, identity=STILLNESS, *, actor=1):
        self.assertNotIn(str(actor), saved["frozen_at"], "expiry requires an active actor")
        deadline = saved["tick"] + condition_remaining(saved, identity, actor)
        while int(observation(self.snapshot(subject))["tick"]) < deadline:
            self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
        result = self.snapshot(subject)
        self.assertEqual(int(observation(result)["tick"]), deadline)
        return result

    def expire(self, subject, peer, saved, identity=STILLNESS, *, actor=1):
        result = self.expiry_boundary(subject, peer, saved, identity, actor=actor)
        self.assertFalse(combat(result)["preparation_active"], "expiry must not resume paid work")
        return result, checkpoint(self, subject)

    def assert_no_private_operands(self, value):
        if isinstance(value, dict):
            for key, child in value.items():
                self.assertNotIn(key, ("rules_rng", "holds", "item_payment", "reserved_by",
                                       "on_success", "on_failure", "qualification", "raw_sample"))
                self.assert_no_private_operands(child)
        elif isinstance(value, list):
            for child in value:
                self.assert_no_private_operands(child)


class StatusConditionProcesses(StatusConditionHelpers, ProcessTestCase):
    def test_zero_damage_pause_retains_holds_and_resumes_once_after_source_free_restart(self):
        package = prepare(self)
        server = self.server("--checkpoint-interval", 1, scenario=package)
        player, _, relay = self.retry_client()
        peer, _ = self.client(actor=2)
        injured = self.injure(player, peer)
        before, paused, original, saved = self.paused_work(player, peer)
        identity = paused["intentions"][0]["intention"]
        original_frame, original_id = relay.saved_frame, relay.request_id
        self.assertIsNotNone(original_frame)
        self.assertEqual(combat(paused)["hp"], combat(injured)["hp"], "status itself deals no damage")
        pending = saved["actors"]["1"]["pending"]
        self.assertFalse(pending["active"])
        self.assertEqual(pending["remaining"], 250)
        for field in ("plan", "hold", "item_payment", "progress"):
            self.assertEqual(pending[field], original["actors"]["1"]["pending"][field])
        self.assertEqual(pending["item_payment"], {"kind": "held", "refs": [{"cost_ordinal": 0, "item": 11}]})
        self.assertEqual(saved["items"]["11"]["quantity"], 2)
        self.assertEqual(len(saved["items"]["11"]["holds"]), 1)
        self.assertEqual(resource(paused, "stamina")["balance"], resource(before, "stamina")["balance"] - 1)
        self.assertEqual(resource(paused, "stamina")["reserved"], 2)
        # Admission advertises an available queue slot, not action legality.
        # The active restriction independently rejects every voluntary action.
        self.assertTrue(paused["readiness"]["admission"])
        self.assertEqual(paused["readiness"]["resume"], [])
        self.assertIn(identity, paused["readiness"]["cancel"])
        for action in ({"type": "wait"}, {"type": "move", "direction": "west"}, ability(paused, WORK)):
            self.assert_atomic_denial(player, {"type": "act", "action": action})
        self.assert_atomic_denial(player, {"type": "resume_intention", "intention": identity})
        for _ in range(3):
            self.assertIsNone(self.request(player, {"type": "history", "limit": 50, "before": None})["error"])
            self.assertIsNone(self.command(player, {"type": "inspect"})["error"])
        self.assertEqual(checkpoint(self, player), saved, "denied work and queries are clock/RNG/hold neutral")
        acknowledgements = [json.loads(line).get("message") for line in player.transcript
                            if line.startswith("{")]
        original_receipt = next(message["receipt"] for message in acknowledgements
                                if message and message["type"] == "ack" and message["request_id"] == original_id)
        relay.retry()
        frozen_retry = self.frame(player, lambda frame: (frame.get("message") or {}).get("type") == "ack"
                                  and frame["message"]["request_id"] == original_id)
        self.assertEqual(frozen_retry["message"]["receipt"],
                         {**original_receipt, "phase": "paused"})
        self.assertEqual(checkpoint(self, player), saved, "receipt lookup precedes the new restriction")

        observer, watched = self.client(observe=True)
        spectator, spectated = self.client(SPECTATOR_TOKEN)
        outsider, other = self.client(SPECTATOR_TOKEN, observe=True, actor=2)
        for frame in (watched, spectated):
            self.assertEqual(combat(frame)["sequence"], combat(paused)["sequence"])
            self.assertEqual(combat(frame)["preparation_remaining"], combat(paused)["preparation_remaining"])
            self.assertFalse(frame["has_control"])
            self.assertEqual(frame["readiness"]["resume"], [])
            self.assertEqual(frame["readiness"]["cancel"], [])
        self.assertIsNone(combat(other)["sequence"])
        for client in (player, observer, spectator, outsider):
            self.assert_no_private_operands(self.snapshot(client))
            for line in client.transcript:
                if line.startswith("{"):
                    self.assert_no_private_operands(json.loads(line))
        self.assert_atomic_denial(spectator, {"type": "resume_intention", "intention": identity})
        for client in (observer, spectator, outsider, player, peer):
            client.stop()
        server.stop()
        # Saved content owns all operands. Retain only immutable compiler blobs
        # for lazy-region restoration; no original rules or manifest survives.
        for path in package.iterdir():
            if path.name != ".compiled":
                shutil.rmtree(path) if path.is_dir() else path.unlink()
        self.assertFalse((package / "rules.toml").exists())
        server = self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        player, restored, retry = self.retry_client()
        peer, _ = self.client(actor=2)
        self.assertEqual(restored["intentions"][0]["intention"], identity)
        self.assertEqual(checkpoint(self, player), saved)
        thawed, protection = self.expire(player, peer, saved)
        self.assertEqual(condition(protection)["phase"], "protection")
        self.assertEqual(condition(protection)["remaining"], 400)
        for field in ("plan", "hold", "item_payment", "progress", "remaining", "intention"):
            self.assertEqual(protection["actors"]["1"]["pending"][field], pending[field])
        self.assertEqual(protection["items"]["11"], saved["items"]["11"])
        self.assertEqual(resource(thawed, "stamina"), resource(paused, "stamina"))
        elapsed = protection["tick"] - saved["tick"]
        self.assertEqual(cooldown(paused) - cooldown(thawed), elapsed)
        self.assertEqual(combat(thawed)["own_stats"]["nutrition"]["reserve"],
                         combat(paused)["own_stats"]["nutrition"]["reserve"] - elapsed // 25)
        self.assertEqual(int(combat(thawed)["own_stats"]["health_recovery"]["elapsed_ticks"]),
                         int(combat(paused)["own_stats"]["health_recovery"]["elapsed_ticks"]) + elapsed)
        self.assertIn(identity, thawed["readiness"]["resume"])
        player.stop(); peer.stop(); server.stop()
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", seed=None)
        player, protected_restart, final_retry = self.retry_client()
        peer, _ = self.client(actor=2)
        self.assertEqual(protected_restart["intentions"][0]["intention"], identity)
        self.assertFalse(combat(protected_restart)["preparation_active"])
        self.assertEqual(checkpoint(self, player), protection, "reopen preserves protection and all paid operands")
        self.resume(player)
        finished = self.finish(player, peer)
        self.assertEqual(combat(finished)["hp"], combat(paused)["hp"] + 3)
        self.assertEqual(resource(finished, "stamina")["balance"], resource(before, "stamina")["balance"] - 3)
        self.assertEqual(resource(finished, "stamina")["reserved"], 0)
        complete = checkpoint(self, player)
        self.assertEqual(complete["items"]["11"]["quantity"], 1)
        self.assertEqual(complete["items"]["11"]["holds"], [])
        final_retry.retry(original_frame)
        repeated = self.frame(player, lambda frame: (frame.get("message") or {}).get("type") == "ack"
                              and frame["message"]["request_id"] == original_id)
        self.assertEqual(repeated["message"]["receipt"]["intention"], identity)
        self.assertEqual(repeated["message"]["receipt"],
                         {**original_receipt, "phase": "resolved"})
        self.assertEqual(checkpoint(self, player), complete, "same request never repays or repeats work")
        self.assertEqual(relay.errors + retry.errors + final_retry.errors, [])

    def test_paid_prefix_denies_atomic_escape_but_allows_in_place_cancellation(self):
        package = prepare(self, "status-prefix", prefix=True)
        self.server("--checkpoint-interval", 1, scenario=package)
        player, _ = self.client()
        peer, _ = self.client(actor=2)
        injured = self.injure(player, peer)
        before, paused, _, saved = self.paused_work(player, peer)
        identity = paused["intentions"][0]["intention"]
        self.assertEqual(combat(paused)["sequence"]["completed"], 1)
        self.assertEqual(combat(paused)["sequence"]["next_strike_remaining"], "75")
        self.assertEqual(combat(paused)["hp"], combat(injured)["hp"] + 1)
        self.assertEqual(saved["items"]["11"]["quantity"], 1)
        self.assertEqual(saved["items"]["11"]["holds"], [])
        self.assertEqual(resource(paused, "stamina")["balance"], resource(before, "stamina")["balance"] - 3)
        self.assertEqual(resource(paused, "stamina")["reserved"], 0)
        self.assert_atomic_denial(player, {"type": "cancel_intention", "intention": identity, "step": "west"})
        self.assertEqual(checkpoint(self, player), saved)
        location = underfoot(paused["state"])["key"]
        debt = int(combat(paused)["sequence"]["cancel_recovery_remaining"])
        cancelled = self.command(player, {"type": "cancel_intention"})
        self.assertIsNone(cancelled["error"])
        self.assertIsNone(combat(cancelled)["preparation_remaining"])
        self.assertEqual(underfoot(cancelled["state"])["key"], location)
        self.assertEqual(int(combat(cancelled)["recovery_remaining"]), debt)
        after = checkpoint(self, player)
        self.assertEqual(condition(after), condition(saved))
        self.assertEqual(after["items"], saved["items"])
        self.assertEqual(actor_creature(after)["pools"], actor_creature(saved)["pools"])
        self.assertIsNone(after["actors"]["1"]["pending"])
        self.assert_atomic_denial(player, {"type": "act", "action": {"type": "wait"}})

    def test_protection_is_targetwide_does_not_refresh_and_ends_after_400_active_ticks(self):
        package = prepare(self, "status-protection")
        self.server("--checkpoint-interval", 1, scenario=package, wizard=True)
        player, _ = self.client()
        peer, _ = self.client(actor=2)
        _, _, _, saved = self.paused_work(player, peer)
        wizard = self.wizard()
        active_report = self.inspection(wizard)
        active = next(row for row in active_report["conditions"] if row["condition"]["id"] == STILLNESS)
        self.assertEqual(active["phase"], "active")
        self.assertEqual(active["source"], "2")
        text, _ = self.text_client(WIZARD_TOKEN, observe=True)
        description = text.command("wizard creature inspect 1")
        self.assertIn("Crystal Pause from actor 2", description)
        self.assertNotIn("Protection against Crystal Pause", description)
        # A second ordinary cast while active cannot extend the first deadline.
        source = self.snapshot(peer)
        target = next(row["id"] for row in observation(source)["visible_actors"] if row["name"] == "first subject")
        cast = self.act(peer, ability(source, STATUS_ABILITIES[STILLNESS], target))
        self.settled(peer, cast)
        repeated = checkpoint(self, player)
        self.assertEqual(repeated["tick"] + condition_remaining(repeated, STILLNESS),
                         saved["tick"] + condition_remaining(saved, STILLNESS))
        self.assertEqual(condition(repeated)["source"], condition(saved)["source"])
        _, protected = self.expire(player, peer, repeated)
        self.assertEqual(condition(protected)["phase"], "protection")
        self.assertEqual(condition(protected)["remaining"], 400)
        self.assertEqual(protected["actors"]["1"]["pending"], saved["actors"]["1"]["pending"])
        protected_report = self.inspection(wizard)
        shown = next(row for row in protected_report["conditions"] if row["condition"]["id"] == STILLNESS)
        self.assertEqual(shown, {"condition": {"id": STILLNESS, "name": "Crystal Pause"},
                                "phase": "protection", "source": "2", "remaining_ticks": "400"})
        description = text.command("wizard creature inspect 1")
        self.assertIn("Protection against Crystal Pause from actor 2: 400 active ticks remaining", description)
        self.assertEqual(checkpoint(self, player), protected, "privileged inspection remains read-only")
        # Cancel the held work explicitly before a genuinely new self application.
        self.assertIsNone(self.command(player, {"type": "cancel_intention"})["error"])
        cast = self.act(player, ability(self.snapshot(player), SELF_ABILITIES[STILLNESS]))
        self.assertIsNone(cast["error"])
        self.finish(player, peer)
        other_source = checkpoint(self, player)
        self.assertEqual(condition(other_source)["phase"], "protection")
        self.assertEqual(condition(other_source)["source"], condition(saved)["source"])
        deadline = protected["tick"] + 400
        self.assertEqual(other_source["tick"] + condition_remaining(other_source, STILLNESS), deadline)
        # Ordinary Wait is legal in protection. Advance both real actors, rather
        # than manufacturing elapsed time or creating a second timer driver.
        while int(observation(self.snapshot(player))["tick"]) < deadline:
            self.assertIsNone(self.act(player, {"type": "wait"})["error"])
            self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
        ended = checkpoint(self, player)
        self.assertEqual(ended["tick"], deadline)
        self.assertIsNone(condition(ended))
        self.assertIsNone(self.act(player, ability(self.snapshot(player), SELF_ABILITIES[STILLNESS]))["error"])
        # This time the fresh condition really applies; a peer boundary retains it.
        for _ in range(2):
            self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
        active = checkpoint(self, player)
        self.assertEqual(condition(active)["phase"], "active")
        self.assertEqual(condition(active)["source"], 1)
        self.assertGreater(condition(active)["remaining"], 0)

    def test_short_lapse_removes_without_protection_and_text_keeps_the_paid_prefix_paused(self):
        package = prepare(self, "status-lapse", prefix=True)
        self.server("--checkpoint-interval", 1, scenario=package)
        player, _ = self.client()
        peer, _ = self.client(actor=2)
        _, paused, _, saved = self.paused_work(player, peer, LAPSE)
        identity = paused["intentions"][0]["intention"]
        self.assert_atomic_denial(player, {"type": "act", "action": {"type": "wait"}})
        self.assert_atomic_denial(player, {"type": "resume_intention", "intention": identity})
        text, welcome = self.adventure(SPECTATOR_TOKEN)
        self.assertIn("Sequence paused after strike 1 of 3", welcome)
        thawed, expired = self.expire(player, peer, saved, LAPSE)
        self.assertIsNone(condition(expired, LAPSE))
        self.assertEqual(expired["actors"]["1"]["pending"], saved["actors"]["1"]["pending"])
        self.assertEqual(combat(thawed)["sequence"], combat(paused)["sequence"])
        looked = self.say(text, "look")
        self.assertIn("Sequence paused after strike 1 of 3", looked)
        self.assertIn("Next strike in 75 ticks", looked)
        self.assertEqual(checkpoint(self, player), expired)
        self.resume(player)
        finished = self.finish(player, peer)
        self.assertIsNone(combat(finished)["sequence"])
        self.assertEqual(checkpoint(self, player)["items"]["11"]["quantity"], 1)
        ordinals = []
        for line in player.transcript:
            if line.startswith("{"):
                frame = json.loads(line)
                body = ((frame.get("message") or {}).get("update") or {}).get("body") or {}
                if body.get("type") in ("observation", "observation_delta"):
                    ordinals.extend(event["strike"] for event in combat(frame)["events"]
                                    if event["type"] == "ability" and event["ability"]["id"] == WORK)
        self.assertEqual(ordinals, [0, 1, 2], "Resume must not replay the already-resolved prefix")

    def test_a_sole_blocked_actor_advances_to_expiry_without_wait_input(self):
        for identity in (STILLNESS, LAPSE):
            with self.subTest(condition=identity):
                package = prepare(self, "status-solo-" + identity, peer=False)
                self.save = self.directory / (identity + ".db")
                server = self.server("--checkpoint-interval", 1, scenario=package)
                player, initial = self.client()
                cast = self.act(player, ability(initial, SELF_ABILITIES[identity]))
                self.assertIsNone(cast["error"])
                deadline = 50 + DURATIONS[identity]
                def expired(frame):
                    return (frame.get("state") is not None and int(observation(frame)["tick"]) == deadline
                            and frame["readiness"]["admission"])
                ready = cast if expired(cast) else self.frame(player, expired)
                self.assertEqual(ready["intentions"], [])
                self.assertIsNone(combat(ready)["preparation_remaining"])
                saved = checkpoint(self, player)
                self.assertEqual(saved["tick"], deadline)
                if identity == STILLNESS:
                    self.assertEqual(condition(saved, identity)["phase"], "protection")
                    self.assertEqual(condition(saved, identity)["remaining"], 400)
                else:
                    self.assertIsNone(condition(saved, identity))
                self.assertEqual(combat(ready)["own_stats"]["nutrition"]["reserve"], 1000 - deadline // 25)
                player.stop(); server.stop()

    def test_region_frozen_time_preserves_status_work_holds_and_all_creature_clocks(self):
        package = prepare(self, "status-streaming", streaming=True)
        server = self.server("--checkpoint-interval", 1, scenario=package, wizard=True)
        mover, initial = self.client()
        target = next(row["id"] for row in observation(initial)["visible_actors"]
                      if row["name"] == "second subject")
        wounded = self.act(mover, ability(initial, WOUND, target))
        self.assertIsNone(wounded["error"])
        wounded = self.settled(mover, wounded)
        # Positive authored preparation and recovery each cost 25 ticks. The
        # subject's ordinary first movement settles before this input boundary,
        # so its subsequent healing decision can own the paid preparation.
        self.assertEqual(int(observation(wounded)["tick"]) - int(observation(initial)["tick"]), 50)
        subject, working = self.client(observe=True, actor=2)
        self.assertFalse(working["has_control"])
        self.assertTrue(combat(working)["preparation_active"], "ordinary healing AI must start paid work")
        self.assertEqual(combat(working)["max_hp"] - combat(working)["hp"], 110)
        original = checkpoint(self, mover)
        pending = original["actors"]["2"]["pending"]
        self.assertEqual(pending["plan"]["ability"], WORK)
        self.assertEqual(pending["item_payment"], {"kind": "held", "refs": [{"cost_ordinal": 0, "item": 12}]})
        self.assertEqual(original["items"]["12"]["quantity"], 2)
        self.assertEqual(len(original["items"]["12"]["holds"]), 1)
        cast = self.act(mover, ability(self.snapshot(mover), STATUS_ABILITIES[STILLNESS], target))
        self.assertIsNone(cast["error"])
        self.settled(mover, cast)
        paused = self.snapshot(subject)
        saved = checkpoint(self, mover)
        self.assertFalse(combat(paused)["preparation_active"])
        self.assertEqual(combat(paused)["hp"], combat(working)["hp"], "status itself deals no damage")
        self.assertEqual(int(combat(working)["preparation_remaining"])
                         - int(combat(paused)["preparation_remaining"]), 50)
        self.assertEqual(condition(saved, actor=2)["phase"], "active")
        self.assertEqual(saved["tick"] - saved["actors"]["2"]["active_since"], 25)
        self.assertEqual(condition_remaining(saved, STILLNESS, 2), 175)
        for field in ("plan", "hold", "item_payment", "progress", "intention"):
            self.assertEqual(saved["actors"]["2"]["pending"][field], pending[field], field)
        self.assertEqual(saved["items"]["12"], original["items"]["12"])
        self.assertEqual(resource(paused, "stamina"), resource(working, "stamina"))
        elapsed = saved["tick"] - original["tick"]
        self.assertEqual(elapsed, 75)
        self.assertEqual(cooldown(working) - cooldown(paused), elapsed)
        self.assertEqual(combat(working)["own_stats"]["nutrition"]["reserve"]
                         - combat(paused)["own_stats"]["nutrition"]["reserve"], elapsed // 25)
        self.assertEqual(int(combat(paused)["own_stats"]["health_recovery"]["elapsed_ticks"])
                         - int(combat(working)["own_stats"]["health_recovery"]["elapsed_ticks"]), elapsed)
        subject.stop()
        wizard = self.wizard()
        active_report = self.inspection(wizard, actor=2)
        self.wizard_command(wizard, "teleport 1 3 16 1 0")
        frozen = checkpoint(self, mover)
        # Freezing settles the lazy prefix before recording the frozen stamp.
        # Compare authoritative projected stats across that representation
        # change, then require exact raw equality throughout frozen world time.
        self.assertEqual(frozen["frozen_at"]["2"], saved["tick"])
        self.assertEqual(actor_clock(frozen, 2), actor_clock(saved, 2))
        self.assertEqual(condition_remaining(frozen, STILLNESS, 2),
                         condition_remaining(saved, STILLNESS, 2))
        frozen_report = self.inspection(wizard, actor=2)
        self.assertEqual({key: value for key, value in frozen_report.items() if key != "revision"},
                         {key: value for key, value in active_report.items() if key != "revision"})
        self.assertEqual(frozen["actors"]["2"]["pending"], saved["actors"]["2"]["pending"])
        self.assertEqual(frozen["items"]["12"], saved["items"]["12"])
        for restart in (False, True):
            for _ in range(40):
                self.assertIsNone(self.act(mover, {"type": "wait"})["error"])
            advanced = checkpoint(self, mover)
            self.assertGreaterEqual(advanced["tick"] - frozen["tick"], 1000)
            self.assertEqual(actor_clock(advanced, 2), actor_clock(frozen, 2))
            self.assertEqual(condition_remaining(advanced, STILLNESS, 2),
                             condition_remaining(frozen, STILLNESS, 2))
            self.assertEqual(actor_creature(advanced, 2), actor_creature(frozen, 2))
            self.assertEqual(advanced["actors"]["2"]["pending"], frozen["actors"]["2"]["pending"])
            self.assertEqual(advanced["items"]["12"], frozen["items"]["12"])
            if not restart:
                mover.stop(); wizard.stop(); server.stop()
                server = self.server("--checkpoint-interval", 1, scenario=package, wizard=True, seed=None)
                mover, _ = self.client()
                wizard = self.wizard()
                self.assertEqual(checkpoint(self, mover), advanced, "restart preserves the exact frozen checkpoint")
        self.wizard_command(wizard, "teleport 1 1 1 1 0")
        thawed = checkpoint(self, mover)
        self.assertNotIn("2", thawed["frozen_at"])
        self.assertEqual(thawed["actors"]["2"]["active_since"], thawed["tick"])
        self.assertEqual(actor_creature(thawed, 2), actor_creature(frozen, 2))
        subject, restored = self.client(observe=True, actor=2)
        self.assertFalse(combat(restored)["preparation_active"])
        self.assertEqual(condition(thawed, actor=2), condition(frozen, actor=2))
        boundary = self.expiry_boundary(subject, mover, thawed, actor=2)
        deadline = thawed["tick"] + condition_remaining(thawed, STILLNESS, 2)
        # Expiry itself leaves paid work paused. The autonomous patient then
        # makes a new ordinary healing decision, selecting its matching owned
        # plan. Human expiry tests above still require explicit Resume.
        def resumed(frame):
            return (frame.get("state") is not None and int(observation(frame)["tick"]) == deadline
                    and combat(frame)["preparation_active"])
        continued = boundary if resumed(boundary) else self.frame(subject, resumed)
        expired = checkpoint(self, mover)
        self.assertEqual(expired["tick"], deadline)
        self.assertEqual(condition(expired, actor=2)["phase"], "protection")
        self.assertEqual(condition(expired, actor=2)["remaining"], 400)
        held = thawed["actors"]["2"]["pending"]
        pending = expired["actors"]["2"]["pending"]
        self.assertTrue(pending["active"])
        self.assertEqual(pending["started"], deadline)
        self.assertNotEqual(pending["intention"], held["intention"], "AI selects work through a new admission")
        for field in ("plan", "origin", "hold", "item_payment", "progress", "remaining"):
            self.assertEqual(pending[field], held[field], field)
        self.assertEqual(combat(continued)["sequence"], combat(restored)["sequence"])
        self.assertEqual(combat(continued)["hp"], combat(restored)["hp"])
        self.assertEqual(expired["items"]["12"], thawed["items"]["12"], "Resume cannot spend or re-hold a bandage")
        self.assertEqual(resource(continued, "stamina"), resource(restored, "stamina"))
        self.assertEqual(actor_creature(expired, 2)["holds"], actor_creature(thawed, 2)["holds"])
        self.assertEqual(expired["rules_rng"], thawed["rules_rng"])
        elapsed = deadline - thawed["tick"]
        self.assertEqual(cooldown(restored) - cooldown(continued), elapsed, "Resume does not restart cooldown")
        self.assertEqual(combat(restored)["own_stats"]["nutrition"]["reserve"]
                         - combat(continued)["own_stats"]["nutrition"]["reserve"], elapsed // 25)
        self.assertEqual(int(combat(continued)["own_stats"]["health_recovery"]["elapsed_ticks"])
                         - int(combat(restored)["own_stats"]["health_recovery"]["elapsed_ticks"]), elapsed)


class NativeStatusConditionProcesses(StatusConditionHelpers, ProcessTestCase):
    graphical = True

    def test_native_private_inspection_distinguishes_active_status_from_protection(self):
        package = prepare(self, "status-native")
        self.server("--checkpoint-interval", 1, scenario=package, wizard=True)
        player, _ = self.client()
        peer, _ = self.client(actor=2)
        _, _, _, saved = self.paused_work(player, peer)
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--observe", "--report-frames"],
                             token=WIZARD_TOKEN)
        self.ascii_frame(window, lambda frame: frame["state"] is not None and not frame["busy"])
        key = self.native_keys(window)

        def inspect():
            opened = self.native_press(window, key, "F7", "wizard")
            self.assertEqual(opened["wizard_command"], "")
            self.native_text(window, "creature inspect 1")
            self.ascii_frame(window, lambda frame: frame.get("wizard_command") == "creature inspect 1")
            shown = self.native_press(window, key, "Return", "enter")
            self.assertTrue(shown["screen"]["stats"])
            # The inspection viewport is 24 rows. Present its final condition
            # row using the real native scroll key, not merely the backing list.
            last = max(0, len(shown["stats_rows"]) - 24)
            # Page through the finite list, observing each press and release
            # before sending the next edge to the native event pump.
            for _ in range((last + 23) // 24):
                expected = min(last, shown["screen"]["stats_scroll"] + 24)
                shown = self.native_press(window, key, "PageDown", "recent_history")
                self.assertEqual(shown["screen"]["stats_scroll"], expected)
            self.assertEqual(shown["screen"]["stats_scroll"], last)
            return "\n".join(shown["stats_rows"][last:last + 24])

        active = inspect()
        self.assertIn("Crystal Pause from actor 2", active)
        self.assertNotIn("Protection against Crystal Pause", active)
        self.assertEqual(checkpoint(self, player), saved)
        closed = self.native_press(window, key, "Escape", "escape")
        self.assertFalse(closed["screen"]["stats"])
        _, protected = self.expire(player, peer, saved)
        description = inspect()
        self.assertIn("Protection against Crystal Pause from actor 2: 400 active ticks remaining", description)
        self.assertEqual(checkpoint(self, player), protected)


if __name__ == "__main__":
    unittest.main()
