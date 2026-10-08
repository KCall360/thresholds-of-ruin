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

from process_harness import ROOT, SPECTATOR_TOKEN, ProcessTestCase
from stream_relay import StreamRelay

PROTOCOL = int(re.search(r"PROTOCOL_VERSION: u32 = (\d+);", (ROOT / "crates/protocol/src/wire.rs").read_text()).group(1))


def kind(message):
    """A server message's kind, with an update's body kind."""
    if message["type"] == "update":
        return "update." + message["update"]["body"]["type"]
    if message["type"] == "ack":
        return "ack." + message["receipt"]["type"]
    return message["type"]


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
            {"type": "rename_place", "expected_revision": "3", "key": "place-key", "name": "Lantern Hall"},
            {"type": "resume_intention", "expected_revision": "3", "intention": "admission-1"},
            {"type": "cancel_intention", "expected_revision": "3", "intention": "admission-1"},
            {"type": "travel", "expected_revision": "3", "destination": "cell-key"},
            {"type": "wizard", "expected_revision": "3", "operation": "rewind initial"},
            *({"type": "act", "expected_revision": "3", "action": action} for action in [
                {"type": "equip", "item": "i_" + "04" * 32, "slot": 1},
                {"type": "unequip", "item": "i_" + "04" * 32},
                {"type": "drink", "item": "i_" + "04" * 32},
                {"type": "attack", "target": "a_" + "02" * 32},
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
        self.server("--outbound-frame-bytes", 512, "--outbound-client-bytes", 65536,
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
