"""Record one real server message of each kind for the protocol's wire samples.

Plays a short game through the real server and headless clients and writes
`crates/protocol/tests/fixtures/wire-v<protocol>.json`: the server messages as
the server sent them, and client messages written here. The protocol crate's
`wire_samples` test round-trips every sample and requires one of each kind.
Rerun this when the protocol version changes and review the diff:

    python scripts/record_wire_samples.py
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import tempfile
import unittest

from process_harness import ROOT, SPECTATOR_TOKEN, WIZARD_TOKEN, ProcessTestCase
from stream_relay import StreamRelay
from projectile_package import projectile_package
from strike_sequence_package import sequence_package
from supply_package import prepare as supply_package
from public_plan_package import paid_preparation, public_plan_package
from nutrition_package import ZERO_WORD_SEED, prepare as nutrition_package, prepare_trigger
from status_condition_package import combat, observation

PROTOCOL = int(re.search(r"PROTOCOL_VERSION: u32 = (\d+);", (ROOT / "crates/protocol/src/wire.rs").read_text()).group(1))


def run_recording_case(case, attribute):
    """Return one artifact only after the actual case and every cleanup pass.

    A suite owns class fixtures as well as TestCase setup/teardown/cleanups.
    Unlike wasSuccessful(), this gate rejects skips and expected failures too:
    neither establishes an actual recording. Subtest failures remain failures.
    """
    result = unittest.TestResult()
    unittest.TestSuite([case]).run(result)
    groups = {"failure": result.failures, "error": result.errors,
              "skip": result.skipped, "expected failure": result.expectedFailures}
    if (result.testsRun != 1 or result.shouldStop or result.unexpectedSuccesses
            or any(groups.values())):
        details = [f"{kind}: {test.id()}: {detail}"
                   for kind, entries in groups.items() for test, detail in entries]
        details.extend("unexpected success: " + test.id() for test in result.unexpectedSuccesses)
        raise AssertionError(f"Wire recording did not complete ({result.testsRun} tests):\n"
                             + "\n".join(details))
    if not hasattr(case, attribute) or getattr(case, attribute) is None:
        raise AssertionError(f"Successful recording did not produce {attribute}")
    return copy.deepcopy(getattr(case, attribute))


def amulet_recording_case():
    """Reuse the bounded non-graphical acceptance route, never another driver."""
    from test_amulet_process import AmuletProcesses
    return AmuletProcesses(
        "test_unique_identity_revisits_source_free_restore_and_text_surface_escape")


def utility_wands_recording_case():
    """Reuse the real utility-zap/Text/reopen acceptance case, not another driver."""
    from test_rogue_utility_wands_process import RogueUtilityWandsProcesses
    return RogueUtilityWandsProcesses(
        "test_persistent_first_contact_utility_state_displays_in_text_and_reopens_without_source")


def self_device_recording_case():
    """Reuse real Text Nothing/Identify/self-aim acceptance and its raw snapshot."""
    from test_magic_items_process import MagicItemProcesses
    return MagicItemProcesses("test_text_nothing_unknown_direction_and_identified_self_skip")


def polymorph_recording_case():
    """Reuse actual Text self-Zap, source deletion and exact request replay."""
    from test_magic_items_process import MagicItemProcesses
    return MagicItemProcesses("test_text_paid_self_polymorph_reopens_and_retries_without_source_or_private_ledger")


def polymorph_diagnostic_recording_case():
    """Minimal authored self-ability witness for the privileged effect variant."""
    from test_magic_items_process import MagicItemProcesses
    return MagicItemProcesses("test_polymorph_self_ability_privileged_capture_has_changed_only_effect")


def drain_refusal_recording_case():
    """Reuse actual imported Drain Text refusal; never synthesize wire facts."""
    from test_magic_items_process import MagicItemProcesses
    return MagicItemProcesses("test_imported_drain_weak_text_attempt_spends_time_not_charge_or_identity")


def first_contact_recording_case():
    """Reuse the actual paid device and privileged capture, not synthetic DTOs."""
    from test_magic_items_process import MagicItemProcesses
    return MagicItemProcesses("test_imported_cancellation_privileged_capture_records_actual_reach_and_effect")


def hallucination_recording_case():
    """Reuse the genuine Text/cadence/source-free case and its raw snapshots."""
    from test_magic_items_process import MagicItemProcesses
    return MagicItemProcesses("test_imported_hallucination_text_visual_cadence_and_source_free_reopen")


def detection_recording_case():
    """Genuine paid Detection/Mapping and source-deleted spectator recovery."""
    from test_magic_items_process import MagicItemProcesses
    return MagicItemProcesses("test_imported_detection_history_empty_retention_source_free_spectator")


def growth_recording_case():
    """Actual granted self-ability diagnostics; production Drinks are separate."""
    from test_magic_items_process import MagicItemProcesses
    return MagicItemProcesses("test_growth_self_abilities_privileged_capture_records_actual_acquisition_and_credit")


def haste_recording_case():
    """Actual paid self-ability outcomes; production potion acceptance is separate."""
    from test_magic_items_process import MagicItemProcesses
    return MagicItemProcesses("test_haste_self_ability_privileged_capture_records_both_real_branches")


def torch_recording_case():
    """Actual adventure control and fuel states, not synthetic server frames."""
    from test_magic_items_process import MagicItemProcesses
    return MagicItemProcesses("test_imported_torch_text_explicit_control_fuel_drop_and_source_free_reopen")


def discovery_recording_case():
    """Reuse Text examination/opening and source-free checkpoint acceptance."""
    from test_discovery_process import DiscoveryProcesses
    return DiscoveryProcesses(
        "test_text_examination_and_secret_open_survive_source_free_checkpoint")


def scare_pickup_recording_case():
    """Reuse paid partial Scare pickup/crumble and source-free recovery."""
    from test_living_dungeon_process import LivingDungeonProcesses
    return LivingDungeonProcesses(
        "test_paid_scare_pickup_curse_crumble_remove_curse_and_restart")


def kind(message):
    """A server message's kind, with an update's body kind."""
    if message["type"] == "update":
        return "update." + message["update"]["body"]["type"]
    if message["type"] == "ack":
        return "ack." + message["receipt"]["type"]
    return message["type"]


def has_condition_event(message, condition):
    """Select an actual standalone event, never an ability's nested outcome."""
    if not message or message["type"] != "update":
        return False
    body = message["update"]["body"]
    if body["type"] == "observation":
        view = body["state"]["observation"].get("combat")
    elif body["type"] == "observation_delta":
        view = body["state"].get("combat")
    else:
        return False
    return any(event["type"] == "condition" and event["condition"]["id"] == condition
               for event in (view or {}).get("events", []))


SAMPLE_BASIS = {"actor": "1", "branch": "branch-1", "stream": "sample-attachment",
                "reset": "1", "build_epoch": "1"}
SAMPLE_ADVANCEMENT_COMMANDS = [
    {"type": "act", "expected_revision": "3", "action": {"type": "choose_intellect_expertise",
        "basis": SAMPLE_BASIS, "choice": {"threshold": 1, "expertise": "rogue.knife"}}},
    {"type": "act", "expected_revision": "3", "action": {"type": "choose_growth_source", "basis": SAMPLE_BASIS,
        "source": {"kind": "racial"}}},
    {"type": "advancement", "expected_revision": "3", "query": {"kind": "overview"}},
    {"type": "act", "expected_revision": "3", "action": {"type": "purchase", "basis": SAMPLE_BASIS,
        "purchase": {"owner": "0", "pool": "practice",
                     "selection": {"kind": "skill_rank", "skill": "perception"}}}},
    {"type": "act", "expected_revision": "3", "action": {"type": "purchase", "basis": SAMPLE_BASIS,
        "purchase": {"owner": "0", "pool": "advancement",
                     "selection": {"kind": "talent_form", "talent": "evocation_initiate", "form": "spark_lance_cold"}}}},
    {"type": "act", "expected_revision": "3", "action": {"type": "respec", "basis": SAMPLE_BASIS,
        "replacement": {"source": {"owner": "0", "index": 0}, "replacement": None},
        "supplies": {"kind": "carried"}}},
]


# Maintained schema examples, not captured outbound client transcripts.
CLIENT = [
    {"type": "hello", "protocol": PROTOCOL, "token": "a-sample-credential", "frontend": "headless"},
    *({"type": "request", "request_id": f"sample-{i}", "request": request} for i, request in enumerate([
        {"type": "continue"},
        {"type": "save"},
        {"type": "history_branch", "branch": "branch-1", "before": None, "limit": 50},
        {"type": "attach", "actor": "1"},
        {"type": "acquire_control"},
        {"type": "release_control"},
        {"type": "snapshot"},
        {"type": "palette"},
        {"type": "map", "query": {
            "context": {"stream": "sample-attachment", "epoch": "1"},
            "branch": "branch-1", "base": {"cursor": {"sequence": "1", "tick": "1"}, "revision": "3"},
            "window": {"min": {"x": -31, "y": -16, "z": -2}, "max": {"x": 31, "y": 16, "z": 3}},
            "cursor": None}},
        {"type": "history", "before": "entry-1", "limit": 20},
        *({"type": "command", "context": {
            "stream": {"stream": "sample-attachment", "epoch": "1"}, "readiness_revision": "3"},
            "branch": "branch-1", "command": command} for command in [
            *SAMPLE_ADVANCEMENT_COMMANDS,
            {"type": "preview_ability_weapon", "expected_revision": "3", "ability": "basic_melee",
             "invocation": {"aim": {"kind": "actor_target", "target": "a_" + "02" * 32}, "weapon": {"kind": "default"}},
             "supplies": {"kind": "carried"}},
            {"type": "projectile_choices", "expected_revision": "3",
             "query": {"ability": "rogue.throw", "phase": {"kind": "sources"}, "offset": 0}},
            {"type": "rename_place", "expected_revision": "3", "key": "place-key", "name": "Lantern Hall"},
            {"type": "resume_intention", "expected_revision": "3", "intention": "admission-1"},
            {"type": "cancel_intention", "expected_revision": "3", "intention": "admission-1", "step": None},
            {"type": "cancel_intention", "expected_revision": "3", "intention": "admission-1", "step": "west"},
            {"type": "inspect_strike_source", "expected_revision": "3", "actor": "1",
             "target_revision": "3", "source": {"kind": "natural"}, "cursor": None},
            {"type": "travel", "expected_revision": "3", "destination": "cell-key"},
            {"type": "wizard", "expected_revision": "3", "operation": "rewind initial"},
            *({"type": "act", "expected_revision": "3", "action": action} for action in [
                {"type": "equip", "item": "i_" + "04" * 32, "slot": 1},
                {"type": "unequip", "item": "i_" + "04" * 32},
                {"type": "drink", "item": "i_" + "04" * 32},
                {"type": "read", "item": "i_" + "04" * 32},
                {"type": "ignite", "item": "i_" + "04" * 32},
                {"type": "extinguish", "item": "i_" + "04" * 32},
                {"type": "zap", "item": "i_" + "04" * 32, "aim": {"kind": "direction", "direction": "east"}},
                {"type": "identify", "choice": "choice-sample", "item": "i_" + "04" * 32},
                {"type": "identify", "choice": "choice-sample", "item": None},
                {"type": "eat", "item": "i_" + "04" * 32},
                *({"type": "use_ability", "id": ability, "invocation": {"aim": {"kind": "actor_target", "target": "a_" + "02" * 32}, "weapon": {"kind": "default"}},
                   "supplies": {"kind": "carried"}}
                  for ability in ("basic_melee", "rattling_blow", "spark_lance_fire", "falter")),
                {"type": "use_ability", "id": "guarded_step", "invocation": {"aim": {"kind": "self_step", "direction": "north_east"}, "weapon": {"kind": "default"}},
                 "supplies": {"kind": "carried"}},
                {"type": "use_ability", "id": "fixture_directional_ray",
                 "invocation": {"aim": {"kind": "directional_ray", "direction": "east"}, "weapon": {"kind": "default"}},
                 "supplies": {"kind": "carried"}},
                {"type": "use_ability", "id": "bind_wounds", "invocation": {"aim": {"kind": "actor_target", "target": "a_" + "02" * 32}, "weapon": {"kind": "default"}},
                 "supplies": {"kind": "explicit", "items": [
                     {"cost_ordinal": 0, "item": "i_" + "04" * 32}]}},
                {"type": "set_door", "door": "d_" + "07" * 32, "open": True},
                {"type": "examine_trap", "trap": "t_" + "0a" * 32},
                {"type": "move", "direction": "north_east"},
                {"type": "take", "item": "i_" + "04" * 32, "quantity": "2"},
                {"type": "drop", "item": "i_" + "04" * 32, "quantity": None},
                {"type": "escape", "hold": "h_" + "09" * 32},
                {"type": "wait"},
            ]),
            {"type": "annotate", "anchor": {"type": "state", "revision": "3"}, "text": "A note",
             "source": "user", "audience": "private", "category": "note"},
            {"type": "annotate", "anchor": {"type": "entry", "id": "entry-1"}, "text": "An explanation",
             "source": "frontend", "audience": "actor", "category": "explanation"},
        ]),
    ])),
]


class Recorder(ProcessTestCase):
    def record_projectile_choices(self, server_samples):
        self.save = self.save.parent / "projectile-samples.db"
        server = self.server(scenario=projectile_package(self))
        owner, current = self.client()
        query = {"ability": "rogue.throw", "phase": {"kind": "sources"}, "offset": 0}
        result = self.request(owner, {"type": "command", "context": current["input_context"],
            "branch": current["branch"], "command": {"type": "projectile_choices",
                "expected_revision": current["state"]["revision"], "query": query}})
        self.assertIsNone(result["error"])
        messages = [json.loads(line).get("message") for line in owner.transcript if line.startswith("{")]
        sample = next(message for message in reversed(messages)
                      if message and message["type"] == "projectile_choices")
        self.assertEqual(sample["page"]["query"], query)
        # This ordinary character has not purchased knife expertise yet: an
        # actual empty owner page records the capability gate without auto-grants.
        self.assertEqual(sample["page"]["choices"], [])
        server_samples["projectile_choices"] = sample
        owner.stop()
        server.stop()

    def record_amulet_escape(self, server_samples):
        sample = run_recording_case(amulet_recording_case(), "escape_sample")
        self.assertEqual(sample["type"], "snapshot")
        server_samples["zz.amulet_escape"] = sample

    def record_self_device(self, server_samples):
        sample = run_recording_case(self_device_recording_case(), "self_device_sample")
        self.assertEqual(sample["type"], "snapshot")
        state = sample["snapshot"]["state"]
        self.assertFalse(state["wizard_game"])
        rows = state["observation"]["interactions"]["inventory"]
        self.assertTrue(any(row["device_activation"] == "self_target" for row in rows),
                        "capture the actual known-self device affordance")
        server_samples["zz.self_device"] = sample

    def record_polymorph(self, server_samples):
        ordinary = run_recording_case(polymorph_recording_case(), "polymorph_samples")
        diagnostic = run_recording_case(polymorph_diagnostic_recording_case(), "polymorph_diagnostic_sample")
        self.assertEqual(set(ordinary), {"affordance", "request"})
        sample, request = ordinary["affordance"], ordinary["request"]
        self.assertEqual(sample["type"], "snapshot")
        state = sample["snapshot"]["state"]
        self.assertFalse(state["wizard_game"])
        self.assertEqual(request["type"], "request")
        self.assertEqual(request["request"]["type"], "command")
        command = request["request"]["command"]
        self.assertEqual(command["type"], "act")
        action = command["action"]
        self.assertEqual(action["type"], "zap")
        self.assertEqual(action["aim"], {"kind": "self_target"})
        row, = [row for row in state["observation"]["interactions"]["inventory"] if row["item"] == action["item"]]
        self.assertEqual(row["device_activation"], "direction_or_self")
        self.assertEqual(diagnostic["type"], "combat_diagnostics")
        self.assertTrue(diagnostic["report"]["enabled"])
        record, = [row for row in diagnostic["report"]["records"] if row.get("ability", {}).get("id") == "fixture.polymorph"]
        effect, = [row for row in record["outcome"]["effects"] if row["type"] == "baleful_polymorph"]
        self.assertEqual(effect, {"type": "baleful_polymorph", "target": "1", "changed": True})
        for secret in ("private.body.birth", "private.body.changed", "body_build", "personal_seed_offset", "purchase_highwater"):
            self.assertNotIn(secret, json.dumps([sample, request, diagnostic]))
        # Validate both genuine cases and every cleanup before publishing either
        # server sample or the captured request (never a handcrafted envelope).
        self.recorded_client_samples = [*getattr(self, "recorded_client_samples", []), request]
        server_samples.update({"zz.polymorph_affordance": sample, "zz.polymorph_effect": diagnostic})

    def record_drain_refusal(self, server_samples):
        sample = run_recording_case(drain_refusal_recording_case(), "drain_refusal_sample")
        self.assertEqual(sample["type"], "snapshot")
        state = sample["snapshot"]["state"]
        self.assertFalse(state["wizard_game"])
        interactions = state["observation"]["interactions"]
        self.assertTrue(interactions["refused"], "capture a real nonempty paid refusal")
        self.assertFalse(interactions["completed"], "refusal is not successful completion")
        for refusal in interactions["refused"]:
            self.assertEqual(refusal["reason"], "too_weak")
            self.assertEqual(refusal["action"]["type"], "zap")
        server_samples["zz.drain_refusal"] = sample

    def record_first_contact(self, server_samples):
        samples = run_recording_case(first_contact_recording_case(), "first_contact_samples")
        self.assertEqual(set(samples), {"reach", "effect"})
        self.assertEqual(samples["reach"]["type"], "snapshot")
        abilities = samples["reach"]["snapshot"]["state"]["observation"]["combat"]["own_stats"]["abilities"]
        self.assertTrue(any(row["id"] == "rogue.wand_cancellation"
                            and row["reach"] == {"kind": "until_obstruction"}
                            and row["target_kind"] == "directional_ray"
                            and not row["basic_attack"] and not row["weapon_selection"] for row in abilities))
        self.assertEqual(samples["effect"]["type"], "combat_diagnostics")
        self.assertTrue(any(effect.get("type") == "cancel_specials" and effect["changed"]
                            for record in samples["effect"]["report"]["records"]
                            for effect in record["outcome"]["effects"]))
        # Publish both only after cleanup and both source-positive validations.
        server_samples.update({"zz.first_contact_" + key: value for key, value in samples.items()})

    def record_hallucination(self, server_samples):
        samples = run_recording_case(hallucination_recording_case(), "hallucination_samples")
        self.assertEqual(set(samples), {"visual", "extended", "cured"})
        observations = {}
        deadlines = {}
        for stage, sample in samples.items():
            # Require the actual protocol envelope, not a normalized client
            # frame or a fabricated partial appearance document.
            self.assertEqual(sample["type"], "snapshot")
            self.assertIsInstance(sample["request_id"], str)
            self.assertTrue(sample["request_id"])
            observation = sample["snapshot"]["state"]["observation"]
            observations[stage] = observation
            encoded = json.dumps(sample)
            for private in ("remaining_to_shift", "presentation_catalog", "palette_index",
                            "hallucination_phase", "hallucination"):
                self.assertNotIn('"' + private + '"', encoded)
            marked = [row for row in observation["combat"]["own_stats"]["conditions"]
                      if row["condition"]["id"] == "rogue.hallucinating"]
            if stage == "cured":
                self.assertEqual(marked, [])
            else:
                self.assertEqual(len(marked), 1)
                condition = marked[0]
                self.assertIsInstance(condition["remaining_ticks"], str)
                self.assertGreater(int(condition["remaining_ticks"]), 0)
                self.assertIsInstance(observation["tick"], str)
                deadlines[stage] = int(observation["tick"]) + int(condition["remaining_ticks"])
            ground, = observation["ground_items"]
            self.assertEqual(ground["item"]["class"], "coin")
            self.assertEqual(ground["item"]["quantity"], "7")
            if stage == "cured":
                self.assertIsNone(ground["presentation"])
                self.assertEqual(ground["item"]["name"], "gold coin")
        self.assertEqual(observations["visual"]["tick"], "100")
        self.assertEqual(observations["extended"]["tick"], "200")
        self.assertIn(deadlines["extended"] - deadlines["visual"], range(80800, 89201, 100))
        for stage, name, asset, object_name, object_asset, cosmetic_class in (
                ("visual", "emu", "creature.bird.emu", "scroll titled NEM AL", "item.scroll", "scroll"),
                ("extended", "bat", "creature.bat", "scroll titled IV MEM", "item.scroll", "scroll")):
            observation = observations[stage]
            peers = [row for row in observation["visible_actors"] if row["id"] != observation["self_target"]]
            # Body cells are separate lawful occurrences of one target. Keep
            # every raw row while requiring one identity and exact cosmetics.
            self.assertEqual(len({row["id"] for row in peers}), 1)
            for peer in peers:
                self.assertEqual((peer["name"], peer.get("asset"), peer["description"]), (name, asset, ""))
                self.assertEqual(peer["appearance_seed"], "p_" + "00" * 32)
            ground, = observation["ground_items"]
            self.assertEqual(ground["presentation"], {"name": object_name, "asset": object_asset,
                "kind": {"kind": "item", "class": cosmetic_class}})
            self.assertEqual((ground["item"]["name"], ground["item"]["appearance"],
                              ground["item"]["description"]), ("object", "object", ""))
            self.assertIsNone(ground["item"].get("asset"))
            self.assertIsNone(ground["item"].get("corpse"))
        cured = observations["cured"]
        peers = [row for row in cured["visible_actors"] if row["id"] != cured["self_target"]]
        self.assertEqual(len({row["id"] for row in peers}), 1)
        for peer in peers:
            self.assertEqual((peer["name"], peer.get("asset"), peer["description"]),
                             ("actual visible peer", None, ""))
        # Every source-positive frame and cleanup must succeed before publishing
        # any member. Preserve raw messages byte-semantically, never synthesize.
        server_samples.update({"zz.hallucination_" + stage: sample for stage, sample in samples.items()})

    def record_detection(self, server_samples):
        from checkpoint_payload import decode_checkpoint_payload
        from detection_process_package import historical_payload, validate_pages
        samples = run_recording_case(detection_recording_case(), "detection_samples")
        self.assertEqual(set(samples), {"query", "page", "restored_query", "restored_page",
                                      "mapping", "restored_mapping", "checkpoint_payload"})
        clients, captured = [], {}
        for prefix in ("", "restored_"):
            request, reply = samples[prefix + "query"], samples[prefix + "page"]
            self.assertEqual(request["type"], "request")
            self.assertEqual(request["request"]["type"], "chart")
            self.assertEqual(reply["type"], "chart_page")
            self.assertEqual(reply["request_id"], request["request_id"])
            query = request["request"]["query"]
            self.assertIsNone(query["cursor"])
            validate_pages([reply["page"]], query)
            self.assertEqual((reply["page"]["kind"], reply["page"]["mode"]), ("magic", "historical"))
            self.assertEqual(len([row for row in reply["page"]["records"] if row["record"] == "row"]), 1)
            clients.append(request)
            captured["chart_page" if not prefix else "zz.detection_restored"] = reply
        self.assertEqual(historical_payload([samples["page"]["page"]]),
                         historical_payload([samples["restored_page"]["page"]]))
        mapping_payloads = []
        for stage in ("mapping", "restored_mapping"):
            exchange = samples[stage]
            self.assertEqual(set(exchange), {"queries", "pages"})
            self.assertEqual(len(exchange["queries"]), len(exchange["pages"]))
            self.assertTrue(exchange["queries"])
            cursor = None
            boundary = exchange["queries"][0]["request"]["query"]
            for index, (request, reply) in enumerate(zip(exchange["queries"], exchange["pages"], strict=True)):
                self.assertEqual((request["type"], request["request"]["type"], reply["type"]),
                                 ("request", "chart", "chart_page"))
                self.assertEqual(reply["request_id"], request["request_id"])
                actual_query = request["request"]["query"]
                self.assertEqual(actual_query["cursor"], cursor)
                self.assertEqual({key: actual_query[key] for key in ("context", "branch", "base", "chart")},
                                 {key: boundary[key] for key in ("context", "branch", "base", "chart")})
                self.assertEqual(reply["page"]["kind"], "mapping")
                cursor = reply["page"]["next"]
                clients.append(request)
                captured[f"zz.{stage}.{index:03}"] = reply
            self.assertIsNone(cursor)
            pages = [reply["page"] for reply in exchange["pages"]]
            validate_pages(pages, exchange["queries"][0]["request"]["query"])
            mapping_payloads.append(historical_payload(pages))
        self.assertEqual(mapping_payloads[0], mapping_payloads[1])
        mapping_rows = [row for row in mapping_payloads[0]["records"] if row["record"] == "row"]
        self.assertEqual(len(mapping_rows), 1024)
        self.assertTrue(all(row["mark"]["fact"]["kind"] == "wall_face"
                            and row["mark"]["fact"]["normal"][2] == 0 for row in mapping_rows))
        self.assertIsInstance(samples["checkpoint_payload"], bytes)
        saved = json.loads(decode_checkpoint_payload(samples["checkpoint_payload"]))
        pool = saved["shared"]["chart_history"]
        bindings = pool["owners"][saved["game"]["chart_history"]]
        owner, = [row for row in bindings if str(row["actor"]) == "1"]
        for kind in ("food", "magic", "mapping"):
            slot = owner["slots"][kind]
            self.assertIsNotNone(slot["latest"])
            self.assertEqual(slot["attempt"]["status"], {"kind": "positive" if kind == "mapping" else "empty"})
            root = pool["roots"][slot["latest"]]
            self.assertEqual(root["kind"], kind)
            self.assertTrue(root["points"])
            self.assertTrue(all((point["mapping"] is not None) == (kind == "mapping") for point in root["points"]))
        # No partial publication on case/cleanup, page correlation, source-free
        # equality or genuine production-writer book validation failure.
        server_samples.update(captured)
        self.recorded_client_samples = [*getattr(self, "recorded_client_samples", []), *clients]
        self.detection_checkpoint_payload = samples["checkpoint_payload"]

    def record_growth(self, server_samples):
        samples = run_recording_case(growth_recording_case(), "growth_samples")
        self.assertEqual(set(samples), {"gain_strength", "raise_level"})
        for kind, sample in samples.items():
            self.assertEqual(sample["type"], "combat_diagnostics")
            self.assertTrue(sample["report"]["enabled"])
            record, = [row for row in sample["report"]["records"]
                       if row.get("ability", {}).get("id") == "fixture." + kind]
            effect, = [row for row in record["outcome"]["effects"] if row["type"] == kind]
            self.assertEqual(str(effect["target"]), "1")
            if kind == "gain_strength":
                self.assertEqual(effect["change"], "acquired")
            else:
                amount = effect["amount"]
                self.assertIsInstance(amount, str)
                self.assertEqual(str(int(amount)), amount)
                self.assertGreater(int(amount), 0)
                self.assertLessEqual(int(amount), (1 << 64) - 1)
        # No partial publication if either actual artifact or cleanup is invalid.
        server_samples.update({"zz.growth_" + key: value for key, value in samples.items()})

    def record_haste(self, server_samples):
        samples = run_recording_case(haste_recording_case(), "haste_samples")
        self.assertEqual(set(samples), {"hasted", "overdose"})
        for branch, sample in samples.items():
            self.assertEqual(sample["type"], "combat_diagnostics")
            self.assertTrue(sample["report"]["enabled"])
            records = [record for record in sample["report"]["records"]
                       if any(effect["type"] == "haste_dose" and effect["outcome"]["type"] == branch
                              for effect in record["outcome"]["effects"])]
            record, = records
            self.assertEqual(record["ability"]["id"], "fixture.haste_dose")
            effect, = [effect for effect in record["outcome"]["effects"] if effect["type"] == "haste_dose"]
            self.assertEqual(str(effect["target"]), "1")
            self.assertEqual(effect["condition"]["id"], "rogue.hasted")
            self.assertEqual(effect["rest"]["id"], "rogue.haste_exhaustion")
            duration = effect["outcome"]["duration" if branch == "hasted" else "rest"]
            self.assertIsInstance(duration, str)
            self.assertIn(int(duration), range(400, 701, 100) if branch == "hasted" else range(0, 701, 100))
        server_samples.update({"zz.haste_" + key: value for key, value in samples.items()})

    def record_torch(self, server_samples):
        samples = run_recording_case(torch_recording_case(), "torch_samples")
        self.assertEqual(set(samples), {"lit", "off"})
        for name, lit, remaining in (("lit", True, "100000"), ("off", False, "99800")):
            sample = samples[name]
            self.assertEqual(sample["type"], "snapshot")
            state = sample["snapshot"]["state"]
            self.assertFalse(state["wizard_game"])
            row, = state["observation"]["interactions"]["inventory"]
            self.assertEqual(row["portable_light"], {"remaining": remaining, "lit": lit,
                                                    "ignitable": not lit, "extinguishable": lit})
        server_samples.update({"zz.torch_" + key: value for key, value in samples.items()})

    def record_scare_crumble(self, server_samples):
        sample = run_recording_case(scare_pickup_recording_case(), "crumble_sample")
        self.assertEqual(sample["type"], "snapshot")
        self.assertFalse(sample["snapshot"]["state"]["wizard_game"])
        events = [entry["content"].get("event")
                  for entry in sample["snapshot"]["history"]["entries"]
                  if entry["content"]["type"] == "action"]
        crumbs = [event for event in events if event and event["type"] == "crumbled"]
        self.assertTrue(crumbs, "record actual paid Scare crumble")
        for event in crumbs:
            self.assertEqual(set(event), {"type", "item", "quantity"})
            self.assertTrue(event["item"].startswith("i_"))
            self.assertEqual(event["quantity"], "1")
        server_samples["zz.scare_crumble"] = sample

    def record_persistent_conditions(self, server_samples):
        case = utility_wands_recording_case()
        sample = run_recording_case(case, "persistent_condition_sample")
        self.assertEqual(sample["type"], "snapshot")
        snapshot = sample["snapshot"]
        self.assertEqual(snapshot["actor"], "3")
        self.assertFalse(snapshot["state"]["wizard_game"])
        rows = snapshot["state"]["observation"]["combat"]["own_stats"]["conditions"]
        self.assertEqual({row["condition"]["id"] for row in rows},
                         {"rogue.hasted", "rogue.invisible"})
        self.assertEqual(len(rows), 2)
        for row in rows:
            self.assertEqual(row["phase"], "active")
            self.assertIn("remaining_ticks", row)
            self.assertIsNone(row["remaining_ticks"], "A persistent sample must contain an explicit null deadline")
        # The same successful process case also retains its live recipient
        # feedback; both artifacts publish only after every cleanup passed.
        from utility_wands_package import assert_anonymous_recipient_feedback
        anonymous = copy.deepcopy(getattr(case, "anonymous_ability_sample", None))
        self.assertIsNotNone(anonymous, "The utility case must retain its real anonymous event")
        self.assertEqual(anonymous["type"], "update")
        self.assertEqual(anonymous["update"]["body"]["type"], "observation_delta")
        recipient = copy.deepcopy(getattr(case, "anonymous_recipient_context", None))
        self.assertIsNotNone(recipient, "The event needs its actual pre-effect disclosure context")
        assert_anonymous_recipient_feedback(self, [anonymous], recipient)
        server_samples["zz.condition_persistent"] = sample
        server_samples["zz.anonymous_utility"] = anonymous

    def record_discovery(self, server_samples):
        from discovery_package import doors, traps
        samples = run_recording_case(discovery_recording_case(), "discovery_samples")
        self.assertEqual(set(samples), {"presence", "family", "opened", "restored"})
        presence = samples["presence"]["snapshot"]
        first = next(iter(traps(presence).values()))
        self.assertEqual(set(first), {"id", "family", "reachable"})
        self.assertIsNone(first["family"])
        self.assertTrue(first["id"].startswith("t_"))
        self.assertFalse(next(iter(doors(presence).values()))["open"])
        self.assertEqual(traps(samples["family"]["snapshot"])[first["id"]]["family"], "rust")
        opened = samples["opened"]["snapshot"]
        self.assertTrue(next(iter(doors(opened).values()))["open"])
        self.assertEqual(len(traps(opened)), 2)
        self.assertEqual(samples["restored"]["snapshot"]["state"], opened["state"])
        for sample in samples.values():
            self.assertEqual(sample["type"], "snapshot")
        server_samples.update({"zz.discovery_" + name: sample for name, sample in samples.items()})

    def record_public_plan(self, server_samples):
        self.save = self.save.parent / "public-plan-samples.db"
        server = self.server(scenario=public_plan_package(self))
        observer, initial = self.client()
        subject, _ = self.client(actor=2)
        paid_preparation(self, observer, initial, subject)
        snapshot = self.request(observer, {"type": "snapshot"})
        self.assertIsNone(snapshot["error"])
        self.assertIsNone(snapshot["state"]["observation"]["combat"]["public_plan"]["state"]["ability"])
        messages = [json.loads(line).get("message") for line in observer.transcript if line.startswith("{")]
        server_samples["zz.public_plan_paid"] = next(message for message in reversed(messages)
            if message and message["type"] == "snapshot"
            and message["snapshot"]["state"]["observation"]["combat"]["public_plan"] is not None)
        observer.stop()
        subject.stop()
        server.stop()

    def record_magic_items(self, server_samples):
        self.save = self.save.parent / "magic-item-samples.db"
        server = self.server(scenario="magic-items")
        player, initial = self.client()
        current = initial
        for row in initial["state"]["observation"]["ground_items"]:
            current = self.act(player, {"type": "take", "item": row["item"]["id"], "quantity": None})
            self.assertIsNone(current["error"])
        inventory = current["state"]["observation"]["inventory"]
        scroll = next(row["id"] for row in inventory if row["class"] == "scroll")
        wand = next(row["id"] for row in inventory if row["class"] == "wand")
        current = self.act(player, {"type": "read", "item": scroll})
        self.assertIsNone(current["error"])
        pending = current["state"]["observation"]["interactions"]["identification"]
        self.assertEqual(pending["classes"], ["wand"])
        self.request(player, {"type": "snapshot"})
        messages = [json.loads(line).get("message") for line in player.transcript if line.startswith("{")]
        server_samples["zz.magic_paid_choice"] = next(message for message in reversed(messages)
            if message and message["type"] == "snapshot")
        current = self.act(player, {"type": "identify", "choice": pending["choice"], "item": wand})
        self.assertIsNone(current["error"])
        current = self.act(player, {"type": "zap", "item": wand, "aim": {"kind": "direction", "direction": "east"}})
        self.assertIsNone(current["error"])
        rows = current["state"]["observation"]["interactions"]["inventory"]
        self.assertEqual(next(row["charges"] for row in rows if row["item"] == wand), 2)
        self.request(player, {"type": "snapshot"})
        messages = [json.loads(line).get("message") for line in player.transcript if line.startswith("{")]
        server_samples["zz.magic_known_charges"] = next(message for message in reversed(messages)
            if message and message["type"] == "snapshot")
        player.stop()
        server.stop()

    def record_nutrition(self, server_samples):
        self.save = self.save.parent / "nutrition-samples.db"
        package = nutrition_package(self)
        server = self.server(scenario=package, spectator=False)
        player, initial = self.client()
        meal = next(item for item in initial["state"]["observation"]["inventory"] if item["name"] == "fixture meal")
        final = self.act(player, {"type": "eat", "item": meal["id"]})
        self.assertIsNone(final["error"])
        snapshot = self.request(player, {"type": "snapshot"})
        self.assertEqual(snapshot["state"]["observation"]["combat"]["own_stats"]["nutrition"]["reserve"], 12)
        messages = [json.loads(line).get("message") for line in player.transcript if line.startswith("{")]
        server_samples["zz.nutrition"] = next(message for message in reversed(messages) if message and message["type"] == "snapshot")
        player.stop(); server.stop()

    def record_personal_conditions(self, server_samples):
        self.save = self.save.parent / "condition-samples.db"
        server = self.server(scenario=prepare_trigger(self, protection_ticks=400), seed=ZERO_WORD_SEED)
        player, _ = self.client()
        peer, _ = self.client(actor=2)
        # Real Wait inputs reach the first empty-reserve deadline. The exempt
        # peer holds the scheduler at each 100-tick boundary while fainting.
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
        active = self.request(player, {"type": "snapshot"})
        self.assertIsNone(active["error"])
        self.assertFalse(active["state"]["wizard_game"])
        self.assertEqual(combat(active)["own_stats"]["conditions"], [{
            "condition": {"id": "fixture_stupor", "name": "Fixture Faint"},
            "phase": "active", "remaining_ticks": "1100"}])
        messages = [json.loads(line).get("message") for line in player.transcript if line.startswith("{")]
        server_samples["zz.condition_active"] = next(message for message in reversed(messages)
            if message and message["type"] == "snapshot")
        server_samples["zz.condition_event"] = next(message for message in messages
            if has_condition_event(message, "fixture_stupor"))
        for _ in range(11):
            self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
        protected = self.request(player, {"type": "snapshot"})
        self.assertIsNone(protected["error"])
        self.assertEqual(int(observation(protected)["tick"]) - int(observation(active)["tick"]), 1100)
        self.assertEqual(combat(protected)["own_stats"]["conditions"], [{
            "condition": {"id": "fixture_stupor", "name": "Fixture Faint"},
            "phase": "protection", "remaining_ticks": "400"}])
        messages = [json.loads(line).get("message") for line in player.transcript if line.startswith("{")]
        server_samples["zz.condition_protection"] = next(message for message in reversed(messages)
            if message and message["type"] == "snapshot")
        other = self.request(peer, {"type": "snapshot"})
        self.assertEqual(combat(other)["own_stats"]["conditions"], [])
        self.assertFalse(any(has_condition_event(json.loads(line).get("message"), "fixture_stupor")
                             for line in peer.transcript if line.startswith("{")))
        player.stop(); peer.stop(); server.stop()

    def record_advancement(self, server_samples):
        self.save = self.save.parent / "advancement-samples.db"
        owner_server = self.server(scenario="advancement-owner")
        captured = []

        def capture_owner_page(message):
            if message["type"] == "advancement_page":
                captured.append(message)

        owner_relay = StreamRelay(self.address, on_server_message=capture_owner_page)
        self.addCleanup(owner_relay.close)
        owner = self.launch("tor-client-headless", ["--connect", owner_relay.address])
        initial = self.frame(owner, lambda frame: frame["type"] == "ready")
        self.assertFalse(initial["state"]["wizard_game"])
        for key, text in [("advancement_page", "overview"), ("zz.growth_source_preview", "source racial"), ("zz.advancement_preview", "remove 1 1")]:
            previous = len(captured)
            result = self.command(owner, {"type": "advancement", "command": text})
            self.assertIsNone(result["error"])
            self.assertEqual(len(captured), previous + 1)
            server_samples[key] = captured[-1]
        accepted = self.command(owner, {"type": "advancement", "command": "confirm using carried"})
        self.assertIsNone(accepted["error"])
        if any(row["phase"] == "queued" for row in accepted["intentions"]):
            self.frame(owner, lambda frame: not any(row["phase"] == "queued" for row in frame["intentions"]))
        previous = len(captured)
        result = self.command(owner, {"type": "advancement", "command": "overview"})
        self.assertIsNone(result["error"])
        self.assertEqual(len(captured), previous + 1)
        self.assertTrue(captured[-1]["page"]["entries"][0]["value"]["respec"]["active"])
        server_samples["zz.advancement_progress"] = captured[-1]
        owner.stop()
        owner_relay.close()
        owner_server.stop()

    def runTest(self):
        server = self.server(scenario="generated-filler", seed=5)
        player, initial = self.client()
        # Capture one actual automatic map page before later observations can
        # supersede its hydration. Ordinary readiness remains independent.
        if not initial.get("map_hydration", {}).get("complete"):
            self.frame(player, lambda frame: frame.get("map_hydration", {}).get("complete"))
        watcher, _ = self.client(SPECTATOR_TOKEN)
        self.request(player, {"type": "palette"})
        self.request(player, {"type": "history", "limit": 5, "before": None})
        self.act(player, {"type": "move", "direction": "east"})
        self.act(player, {"type": "move", "direction": "east"})
        rejected = self.act(player, {"type": "take", "item": "i_" + "00" * 32})
        self.assertTrue(rejected["error"].startswith("InvalidAction:"))
        state = self.request(player, {"type": "snapshot"})
        state = self.request(player, {"type": "command", "context": state["input_context"], "branch": state["branch"], "command": {
            "type": "annotate", "anchor": {"type": "state", "revision": state["state"]["revision"]},
            "text": "Shared sample note", "audience": "actor"}})
        self.assertIsNone(state["error"])
        # Simulation effects may arrive between the snapshot and note receipt.
        # Build travel from the latest applied frame, not the earlier snapshot.
        destination = state["state"]["observation"]["visible_cells"][0]["key"]
        accepted = self.request(player, {"type": "command", "context": state["input_context"], "branch": state["branch"], "command": {
            "type": "travel", "expected_revision": state["state"]["revision"], "destination": destination}})
        self.assertIsNone(accepted["error"])
        self.frame(player, lambda f: f.get("travel") and f["travel"]["phase"] != "active")
        self.request(player, {"type": "release_control"})
        self.request(watcher, {"type": "snapshot"})
        # The headless client consumes the welcome itself, so it's written here.
        server_samples = {"welcome": {"type": "welcome", "protocol": PROTOCOL, "user": "sample-user",
                                      "actors": ["1"], "role": "player", "capabilities": initial["capabilities"]}}
        for client in (player, watcher):
            for line in client.transcript + [line for line in list(client.lines.queue) if line]:
                if line.startswith("{"):
                    message = json.loads(line).get("message")
                    if message:
                        server_samples.setdefault(kind(message), message)
        player.stop()
        watcher.stop()
        server.stop()
        bounded = self.server("--outbound-frame-bytes", 512, "--outbound-client-bytes", 65536,
                    "--outbound-total-bytes", 262144, scenario="generated-filler", seed=5)
        captured = []

        def capture_part(message):
            if message["type"] == "snapshot_part" and not captured:
                captured.append(message)

        relay = StreamRelay(self.address, on_server_message=capture_part)
        self.addCleanup(relay.close)
        peer = self.launch("tor-client-headless", ["--connect", relay.address, "--observe"],
                           token=SPECTATOR_TOKEN)
        self.frame(peer, lambda frame: frame["type"] == "ready")
        peer.stop()
        relay.close()
        self.assertEqual(len(captured), 1, "record an actual bounded snapshot transfer part")
        server_samples["snapshot_part"] = captured[0]
        bounded.stop()
        self.save = self.save.parent / "item-samples.db"
        item_server = self.server(scenario="interactions", wizard=True)
        item_client, state = self.client()
        potion = next(item for item in state["state"]["observation"]["inventory"] if item["class"] == "potion")
        result = self.act(item_client, {"type": "drink", "item": potion["id"]})
        self.assertEqual(result["state"]["observation"]["interactions"]["completed"], [{"type": "drink", "item": potion["id"]}])
        self.request(item_client, {"type": "snapshot"})
        messages = [json.loads(line).get("message") for line in item_client.transcript if line.startswith("{")]
        server_samples["zz.item_completion"] = next(message for message in reversed(messages)
            if message and message["type"] == "snapshot")
        # Preserve a real exact-instance weapon disclosure sample. Ordinary
        # archetype familiarity alone deliberately leaves numerical views absent.
        self.assertTrue(all(item["known_equipment"] is None for item in
                            state["state"]["observation"]["interactions"]["inventory"]))
        item_wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        self.wizard_command(item_wizard, "identify 1 10")
        known = self.request(item_client, {"type": "snapshot"})
        self.assertTrue(any(item["known_equipment"] is not None and
                            item["known_equipment"]["attack"] is not None for item in
                            known["state"]["observation"]["interactions"]["inventory"]))
        messages = [json.loads(line).get("message") for line in item_client.transcript if line.startswith("{")]
        server_samples["zz.known_weapon"] = next(message for message in reversed(messages)
            if message and message["type"] == "snapshot")
        item_wizard.stop()
        item_client.stop()
        item_server.stop()
        self.save = self.save.parent / "inspection-samples.db"
        inspection_server = self.server(scenario="mob-arena", wizard=True)
        inspector, _ = self.client(WIZARD_TOKEN, observe=True)
        result = self.command(inspector, {"type": "wizard", "command": "creature list"})
        self.assertFalse(result.get("error"), result.get("error"))
        messages = [json.loads(line).get("message") for line in inspector.transcript if line.startswith("{")]
        server_samples["creature_catalog"] = next(message for message in reversed(messages)
            if message and message["type"] == "creature_catalog")
        result = self.command(inspector, {"type": "wizard", "command": "creature inspect 1"})
        self.assertFalse(result.get("error"), result.get("error"))
        messages = [json.loads(line).get("message") for line in inspector.transcript if line.startswith("{")]
        server_samples["creature_inspection"] = next(message for message in reversed(messages)
            if message and message["type"] == "creature_inspection")
        report = server_samples["creature_inspection"]["report"]
        result = self.request(inspector, {"type": "command", "context": result["input_context"],
            "branch": result["branch"], "command": {"type": "inspect_strike_source",
                "expected_revision": result["state"]["revision"], "actor": report["actor"],
                "target_revision": report["revision"], "source": {"kind": "natural"}, "cursor": None}})
        self.assertFalse(result.get("error"), result.get("error"))
        messages = [json.loads(line).get("message") for line in inspector.transcript if line.startswith("{")]
        server_samples["creature_strike_page"] = next(message for message in reversed(messages)
            if message and message["type"] == "creature_strike_page")
        result = self.command(inspector, {"type": "wizard", "command": "combat capture on"})
        self.assertFalse(result.get("error"), result.get("error"))
        messages = [json.loads(line).get("message") for line in inspector.transcript if line.startswith("{")]
        server_samples["combat_diagnostics"] = next(message for message in reversed(messages)
            if message and message["type"] == "combat_diagnostics")
        inspector.stop()
        inspection_server.stop()
        self.save = self.save.parent / "sequence-samples.db"
        sequence_server = self.server(scenario=sequence_package(self, autonomous=False, natural_source=True, conditional_rider=True), wizard=True)
        inspector, _ = self.client(WIZARD_TOKEN, observe=True)
        result = self.command(inspector, {"type": "wizard", "command": "creature inspect 1"})
        self.assertFalse(result.get("error"), result.get("error"))
        messages = [json.loads(line).get("message") for line in inspector.transcript if line.startswith("{")]
        summary = next(message for message in reversed(messages) if message and message["type"] == "creature_inspection")
        self.assertTrue(any(row["source"]["kind"] == "natural_melee" for row in summary["report"]["ability_strikes"]))
        server_samples["zz.natural_melee_inspection"] = summary
        report = summary["report"]
        result = self.request(inspector, {"type": "command", "context": result["input_context"],
            "branch": result["branch"], "command": {"type": "inspect_strike_source",
                "expected_revision": result["state"]["revision"], "actor": report["actor"],
                "target_revision": report["revision"], "source": {"kind": "ability", "id": "triple_strike"}, "cursor": None}})
        self.assertFalse(result.get("error"), result.get("error"))
        messages = [json.loads(line).get("message") for line in inspector.transcript if line.startswith("{")]
        server_samples["zz.natural_melee_page"] = next(message for message in reversed(messages)
            if message and message["type"] == "creature_strike_page")
        self.assertTrue(any(row["kind"] == "rider_header" and row["condition"] == {
            "kind": "sequence_successful_hits", "minimum": 2}
            for row in server_samples["zz.natural_melee_page"]["page"]["entries"]),
            "record the real encoder's required non-null condition")
        inspector.stop()
        attacker, initial = self.client()
        defender, _ = self.client(actor=2)
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                      if actor["name"] == "Sequence sentinel")
        current = self.request(attacker, {"type": "snapshot"})
        preview = self.request(attacker, {"type": "command", "context": current["input_context"],
            "branch": current["branch"], "command": {"type": "preview_ability_weapon",
                "expected_revision": current["state"]["revision"], "ability": "triple_strike",
                "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}},
                "supplies": {"kind": "carried"}}})
        self.assertIsNone(preview["error"])
        messages = [json.loads(line).get("message") for line in attacker.transcript if line.startswith("{")]
        server_samples["ability_weapon_preview"] = next(message for message in reversed(messages)
            if message and message["type"] == "ability_weapon_preview")
        admitted = self.act(attacker, {"type": "use_ability", "id": "triple_strike", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
        self.assertIsNone(admitted["error"])
        self.assertIsNone(self.act(defender, {"type": "wait"})["error"])
        self.frame(attacker, lambda frame: frame.get("state") is not None
            and (frame["state"]["observation"]["combat"]["sequence"] or {}).get("completed") == 1)
        self.request(attacker, {"type": "snapshot"})
        messages = [json.loads(line).get("message") for line in attacker.transcript if line.startswith("{")]
        server_samples["zz.sequence_prefix"] = next(message for message in reversed(messages)
            if message and message["type"] == "snapshot"
            and (message["snapshot"]["state"]["observation"]["combat"]["sequence"] or {}).get("completed") == 1)
        def witnessed_primary(value):
            if isinstance(value, dict):
                return (value.get("type") == "ability" and value.get("strike") == 0
                        or any(witnessed_primary(item) for item in value.values()))
            return isinstance(value, list) and any(witnessed_primary(item) for item in value)
        server_samples["zz.primary_ordinal"] = next(message for message in messages
            if message and witnessed_primary(message))
        attacker.stop()
        defender.stop()
        sequence_server.stop()
        self.save = self.save.parent / "care-samples.db"
        care_server = self.server(scenario=supply_package(self))
        carer, initial = self.client()
        partner, _ = self.client(actor=2)
        observation = initial["state"]["observation"]
        bandage = next(row["item"]["id"] for row in observation["ground_items"]
                       if (row["item"]["known_supply"] or {}).get("id") == "bandage")
        accepted = self.act(carer, {"type": "use_ability", "id": "bind_wounds",
            "invocation": {"aim": {"kind": "actor_target", "target": observation["self_target"]}, "weapon": {"kind": "default"}}, "supplies": {"kind": "explicit", "items": [
                {"cost_ordinal": 0, "item": bandage}]}})
        self.assertIsNone(accepted["error"])
        prepared = self.request(carer, {"type": "snapshot"})
        self.assertTrue(prepared["state"]["observation"]["combat"]["preparation_active"])
        messages = [json.loads(line).get("message") for line in carer.transcript if line.startswith("{")]
        server_samples["zz.care_preparation"] = next(message for message in reversed(messages)
            if message and message["type"] == "snapshot")
        current = prepared
        for _ in range(8):
            if current["state"]["observation"]["combat"]["preparation_remaining"] is None:
                break
            self.assertIsNone(self.act(partner, {"type": "wait"})["error"])
            current = self.request(carer, {"type": "snapshot"})
        self.assertIsNone(current["state"]["observation"]["combat"]["preparation_remaining"])
        self.assertFalse(any(row["item"]["id"] == bandage
                             for row in current["state"]["observation"]["ground_items"]))
        self.assertFalse(current["state"]["wizard_game"])
        messages = [json.loads(line).get("message") for line in carer.transcript if line.startswith("{")]
        server_samples["zz.care_spent"] = next(message for message in reversed(messages)
            if message and message["type"] == "snapshot")
        carer.stop()
        partner.stop()
        care_server.stop()
        self.record_projectile_choices(server_samples)
        self.record_advancement(server_samples)
        self.record_public_plan(server_samples)
        self.record_nutrition(server_samples)
        self.record_magic_items(server_samples)
        self.record_self_device(server_samples)
        self.record_polymorph(server_samples)
        self.record_drain_refusal(server_samples)
        self.record_first_contact(server_samples)
        self.record_torch(server_samples)
        self.record_haste(server_samples)
        self.record_growth(server_samples)
        self.record_hallucination(server_samples)
        self.record_detection(server_samples)
        self.record_personal_conditions(server_samples)
        self.record_persistent_conditions(server_samples)
        self.record_amulet_escape(server_samples)
        self.record_discovery(server_samples)
        self.record_scare_crumble(server_samples)
        self.server_samples = server_samples


def publish_bytes(output, payload):
    """Replace one artifact atomically, only after complete recording validation."""
    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    staged = None
    try:
        with tempfile.NamedTemporaryFile(mode="wb", dir=output.parent,
                                         prefix="." + output.name + ".", suffix=".pending",
                                         delete=False) as stream:
            staged = Path(stream.name)
            stream.write(payload)
        os.replace(staged, output)
    finally:
        if staged is not None:
            staged.unlink(missing_ok=True)


def record_samples(*, output=None, detection_checkpoint=None):
    """Publish only complete recordings, including successful process cleanup."""
    case = Recorder()
    server_samples = run_recording_case(case, "server_samples")
    client_samples = [*CLIENT, *copy.deepcopy(case.recorded_client_samples)]
    document = json.dumps({
        "protocol": PROTOCOL, "client": client_samples,
        "server": [server_samples[k] for k in sorted(server_samples)],
    }, indent=1, sort_keys=True) + "\n"
    output = Path(output) if output is not None else ROOT / f"crates/protocol/tests/fixtures/wire-v{PROTOCOL}.json"
    exports = []
    if detection_checkpoint is not None:
        from checkpoint_payload import decode_checkpoint_payload
        # These are the unchanged bytes captured from the production writer in
        # the successful source-deleted reopen case. Never manufacture save DTOs.
        raw = case.detection_checkpoint_payload
        decoded = decode_checkpoint_payload(raw)
        json.loads(decoded)  # Validate before publishing any output member.
        target = Path(detection_checkpoint)
        provenance = {"producer": "MagicItemProcesses.test_imported_detection_history_empty_retention_source_free_spectator",
                      "raw_bytes": len(raw), "raw_sha256": hashlib.sha256(raw).hexdigest(),
                      "decoded_bytes": len(decoded), "decoded_sha256": hashlib.sha256(decoded).hexdigest()}
        exports = [(target, decoded), (Path(str(target) + ".torc"), raw),
                   (Path(str(target) + ".provenance.json"),
                    (json.dumps(provenance, indent=2, sort_keys=True) + "\n").encode())]
    # Each file is atomically replaced. Every case and class cleanup, all book
    # checks and export decoding precede publication; no partial lifecycle earns
    # a fixture. A filesystem failure is reported, never called a recording pass.
    for target, payload in [*exports, (output, document.encode("utf-8"))]:
        publish_bytes(target, payload)
    shown = output.relative_to(ROOT) if output.is_relative_to(ROOT) else output
    print(f"{shown}: {len(CLIENT)} client schema examples, {len(case.recorded_client_samples)} captured client queries and {len(server_samples)} captured server samples: "
          + ", ".join(sorted(server_samples)))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--detection-checkpoint", type=Path,
                        help="also export the genuine decoded Detection/Mapping save plus .torc and SHA256 provenance")
    arguments = parser.parse_args()
    record_samples(detection_checkpoint=arguments.detection_checkpoint)
