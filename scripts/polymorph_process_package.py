"""Source-only Polymorph acceptance fixture and exact real-client retry capture.

Fixture operands are not a live form roster or balance decision. The package is
compiled by the process test; no save injection, fabricated reply or wizard edit.
"""
import copy
import json
import queue
import shutil
import struct
import subprocess
import threading
import time
import tomllib

from arena_matrix import toml_source
from process_harness import ROOT
from stream_relay import StreamRelay, exact

BASE = "private.body.birth"
FORM = "private.body.changed"


def prepare(test, *, diagnostic_ability=False):
    package = test.directory / "polymorph-source"
    shutil.copytree(ROOT / "scenarios/tests/magic-items", package)
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    changed = copy.deepcopy(manifest["creatures"]["species"]["figure"])
    changed["attributes"].update(strength=3, intellect=5, willpower=5, awareness=5, presence=5)
    manifest["creatures"]["species"]["changed"] = changed
    racial = {"source": "racial", "purchases": []}
    manifest["creatures"]["body_packages"] = {
        BASE: {"species": "figure", "body": {"cells": [[0, 0, 0]], "eye": [0, 0, 0], "mass": 80},
               "hit_dice": [copy.deepcopy(racial)], "polymorph_eligible": False},
        FORM: {"species": "changed", "body": {"cells": [[0, 0, 0], [0, 0, 1]], "eye": [0, 0, 0], "mass": 120},
               "hit_dice": [copy.deepcopy(racial), copy.deepcopy(racial)], "polymorph_eligible": True}}
    creature = manifest["characters"][0]["creature"]
    creature.update(body_package=BASE, hit_dice=[racial, {"source": "warrior", "purchases": []}])
    wand = manifest["archetypes"]["ember"]
    wand.update(name="wand of polymorph", identity="fixture.polymorph")
    wand["device"]["discovery"] = "discernible_effect"
    path.write_text(toml_source(manifest), encoding="utf-8")
    path = package / "rules.toml"
    rules = tomllib.loads(path.read_text(encoding="utf-8"))
    ability = rules["overrides"][0]["value"]
    test.assertEqual(ability["id"], "magic.ember_ray")
    ability.update(invocation="directional_contact_ray", reach={"kind": "until_obstruction"})
    strike, = ability["strikes"]["strikes"]
    test.assertEqual(strike["check"], {"actor": "target", "skill": {"kind": "named", "skill": "agility"},
                                      "against": {"kind": "opposed", "skill": {"kind": "named", "skill": "lore"}}})
    strike.update(on_success=[], on_failure=[{"kind": "baleful_polymorph", "forms": [FORM]}], descriptors=[], riders=[])
    if diagnostic_ability:
        # The ordinary wand case owns paid self-Zap. This separately granted
        # authored self ability exercises the existing combat-diagnostic path.
        diagnostic = copy.deepcopy(ability)
        diagnostic.update(id="fixture.polymorph", name="Polymorph diagnostic",
                          invocation="actor_target", reach={"kind": "self_only"})
        diagnostic["strikes"]["strikes"] = [{"offset": 0,
            "on_success": [{"kind": "baleful_polymorph", "forms": [FORM]}],
            "on_failure": [], "descriptors": [], "riders": []}]
        rules["overrides"].append({"op": "add", "path": ["abilities", "fixture.polymorph"],
                                   "value": diagnostic})
        # Keep the diagnostic grant on both fixture bodies: this case records
        # the effect, not ability-loss interruption or invalidated recovery.
        for species in ("figure", "changed"):
            manifest["creatures"]["species"][species]["modifiers"].append({"scope": "always",
                "op": {"kind": "grant_ability", "ability": "fixture.polymorph"}})
        (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8")
    path.write_text(toml_source(rules), encoding="utf-8")
    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    region["items"] = [row for row in region["items"] if row["id"] in (20, 30)]
    path.write_text(toml_source(region), encoding="utf-8")
    validated = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                               capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(validated.returncode, 0, validated.stdout + validated.stderr)
    return package


def personal(creature):
    build = creature["build"]
    return [(row, choices) for row, choices in zip(build["hd"], build["choices"], strict=True)
            if row["owner"]["kind"] == "personal"]


class PolymorphRetryRelay(StreamRelay):
    """Capture the actual Text self-Zap bytes; replay them unchanged after load."""
    def __init__(self, upstream):
        self.send_lock = threading.Lock()
        self.captured = threading.Event()
        self.saved_frame = None
        self.saved_message = None
        self.request_id = None
        self.replies = queue.Queue()
        super().__init__(upstream, on_server_message=self.replies.put)

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
                    raise ValueError("Invalid real client frame")
                mask, payload = exact(source, 4), exact(source, length)
                frame = prefix + extra + mask + payload
                if prefix[0] == 0x81 and self.saved_frame is None:
                    message = json.loads(bytes(value ^ mask[index % 4] for index, value in enumerate(payload)))
                    action = ((message.get("request") or {}).get("command") or {}).get("action") or {}
                    if action.get("type") == "zap" and action.get("aim", {}).get("kind") == "self_target":
                        self.saved_frame, self.request_id = frame, message["request_id"]
                        self.saved_message = copy.deepcopy(message)
                        self.captured.set()
                with self.send_lock:
                    target.sendall(frame)
        except (EOFError, OSError):
            pass
        except Exception as error:
            self.errors.append(error)
        finally:
            self.close_sockets()

    def retry(self, frame, request_id):
        with self.send_lock:
            self._server_socket.sendall(frame)
        deadline = time.monotonic() + 15
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise AssertionError("No actual retry receipt from reopened server")
            try:
                reply = self.replies.get(timeout=remaining)
            except queue.Empty as error:
                raise AssertionError("No actual retry receipt from reopened server") from error
            if reply.get("request_id") == request_id:
                if reply.get("type") != "ack":
                    raise AssertionError("Retry rejected: " + repr(reply))
                return reply["receipt"]
