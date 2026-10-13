"""Record one real server message of each kind for the protocol's wire samples.

Plays a short game through the real server and headless clients and writes
`crates/protocol/tests/fixtures/wire-v<protocol>.json`: the server messages as
the server sent them, and client messages written here. The protocol crate's
`wire_samples` test round-trips every sample and requires one of each kind.
Rerun this when the protocol version changes and review the diff:

    python scripts/record_wire_samples.py
"""
import json
import re

from process_harness import ROOT, SPECTATOR_TOKEN, WIZARD_TOKEN, ProcessTestCase
from stream_relay import StreamRelay
from strike_sequence_package import sequence_package
from supply_package import prepare as supply_package
from public_plan_package import paid_preparation, public_plan_package

PROTOCOL = int(re.search(r"PROTOCOL_VERSION: u32 = (\d+);", (ROOT / "crates/protocol/src/wire.rs").read_text()).group(1))


def kind(message):
    """A server message's kind, with an update's body kind."""
    if message["type"] == "update":
        return "update." + message["update"]["body"]["type"]
    if message["type"] == "ack":
        return "ack." + message["receipt"]["type"]
    return message["type"]


SAMPLE_BASIS = {"actor": "1", "branch": "branch-1", "stream": "sample-attachment",
                "reset": "1", "build_epoch": "1"}
SAMPLE_ADVANCEMENT_COMMANDS = [
    {"type": "advancement", "expected_revision": "3", "query": {"kind": "overview"}},
    {"type": "act", "expected_revision": "3", "action": {"type": "purchase", "basis": SAMPLE_BASIS,
        "purchase": {"owner": 0, "pool": "practice",
                     "selection": {"kind": "skill_rank", "skill": "perception"}}}},
    {"type": "act", "expected_revision": "3", "action": {"type": "respec", "basis": SAMPLE_BASIS,
        "replacement": {"source": {"owner": 0, "index": 0}, "replacement": None},
        "supplies": {"kind": "carried"}}},
]


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
        {"type": "history", "before": "entry-1", "limit": 20},
        *({"type": "command", "context": {
            "stream": {"stream": "sample-attachment", "epoch": "1"}, "readiness_revision": "3"},
            "branch": "branch-1", "command": command} for command in [
            *SAMPLE_ADVANCEMENT_COMMANDS,
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
                *({"type": "use_ability", "id": ability, "target": "a_" + "02" * 32,
                   "supplies": {"kind": "carried"}}
                  for ability in ("basic_melee", "power_strike", "magic_bolt", "fear")),
                {"type": "use_ability", "id": "bind_wounds", "target": "a_" + "02" * 32,
                 "supplies": {"kind": "explicit", "items": [
                     {"cost_ordinal": 0, "item": "i_" + "04" * 32}]}},
                {"type": "set_door", "door": "d_" + "07" * 32, "open": True},
                {"type": "move", "direction": "north_east"},
                {"type": "take", "item": "i_" + "04" * 32, "quantity": "2"},
                {"type": "drop", "item": "i_" + "04" * 32, "quantity": None},
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
    def record_public_plan(self, server_samples):
        self.save = self.save.parent / "public-plan-samples.db"
        server = self.server(scenario=public_plan_package(self))
        observer, initial = self.client()
        subject, subject_initial = self.client(actor=2)
        paid_preparation(self, observer, initial, subject, subject_initial)
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
        for key, text in [("advancement_page", "overview"), ("zz.advancement_preview", "remove 1 1")]:
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
        item_server = self.server(scenario="interactions")
        item_client, state = self.client()
        potion = next(item for item in state["state"]["observation"]["inventory"] if item["class"] == "potion")
        result = self.act(item_client, {"type": "drink", "item": potion["id"]})
        self.assertEqual(result["state"]["observation"]["interactions"]["completed"], [{"type": "drink", "item": potion["id"]}])
        self.request(item_client, {"type": "snapshot"})
        messages = [json.loads(line).get("message") for line in item_client.transcript if line.startswith("{")]
        server_samples["zz.item_completion"] = next(message for message in reversed(messages)
            if message and message["type"] == "snapshot")
        item_client.stop()
        item_server.stop()
        self.save = self.save.parent / "inspection-samples.db"
        inspection_server = self.server(scenario="mob-arena", wizard=True)
        inspector, _ = self.client(WIZARD_TOKEN, observe=True)
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
        sequence_server = self.server(scenario=sequence_package(self, autonomous=False))
        attacker, initial = self.client()
        defender, _ = self.client(actor=2)
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"]
                      if actor["name"] == "Sequence sentinel")
        admitted = self.act(attacker, {"type": "use_ability", "id": "triple_strike", "target": target})
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
            "target": observation["self_target"], "supplies": {"kind": "explicit", "items": [
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
        self.record_advancement(server_samples)
        self.record_public_plan(server_samples)
        output = ROOT / f"crates/protocol/tests/fixtures/wire-v{PROTOCOL}.json"
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps({
            "protocol": PROTOCOL,
            "client": CLIENT,
            "server": [server_samples[k] for k in sorted(server_samples)],
        }, indent=1, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
        print(f"{output.relative_to(ROOT)}: {len(CLIENT)} client and {len(server_samples)} server samples: "
              + ", ".join(sorted(server_samples)))


if __name__ == "__main__":
    Recorder.setUpClass()
    recorder = Recorder()
    recorder.setUp()
    try:
        recorder.runTest()
    finally:
        recorder.doCleanups()
