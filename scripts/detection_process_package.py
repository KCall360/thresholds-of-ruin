"""Production detection in explicit finite process fixtures; no production stock edit.

Production effects, conditions, species, appearances and assets remain unchanged.
A distinct human-derived speed-zero fixture species and exempt external owners
isolate ordinary paid clocks. This module launches no process on import.
"""
import copy
import json
import sqlite3
import struct
import threading
import tomllib
from contextlib import closing
from pathlib import Path

from checkpoint_payload import decode_checkpoint_payload
from process_harness import ROOT
from stream_relay import StreamRelay, exact

PRODUCTION = ROOT / "scenarios/rogue-exploration"
DOSES = {"monsters": (30, "monster_detection", 6), "food": (31, "food_detection", 5),
         "magic": (32, "magic_detection", 4), "hallucination": (33, "hallucination", 9),
         "extra": (35, "extra_healing", 8)}
VISUAL = {0: ("emu", "creature.bird.emu"), 1: ("bat", "creature.bat")}
MONSTER_SENSE = {0: ("snake", "creature.serpent.snake"),
                 1: ("leprechaun", "creature.fey.leprechaun")}


def fixture(*, paging=False, mapping=False, certified=True, production=PRODUCTION):
    production = Path(production)
    def read(name):
        return tomllib.loads((production / name).read_text(encoding="utf-8"))
    source = read("scenario.toml")
    items = read("catalog/items.toml")
    species = read("catalog/creatures.toml")["creatures"]["species"]
    actors = read("catalog/actors.toml")["actors"]
    bodies = read("catalog/bodies.toml")["creatures"]["body_packages"]
    manifest = {key: copy.deepcopy(source[key]) for key in
                ("format", "version", "ruleset", "factions", "ai_profiles", "themes", "assets")}
    owner = copy.deepcopy(source["characters"][0])
    owner.update(anchor="1/start", turn_ticks=100, loadout=[], known_identities=["rogue.ration"])
    owner["creature"].update(species="fixture.detection", nutrition={"kind": "exempt"},
                             future_growth={"kind": "disabled"})
    human = copy.deepcopy(species["rogue.human"])
    human.setdefault("modifiers", []).append({"scope": "always", "op": {"kind": "set",
        "stat": {"kind": "attribute", "attribute": "speed"}, "value": 0}})
    species["fixture.detection"] = human
    # The fixture changes the species' Speed modifier, not its body/class HD
    # composition. Rebind an exact body recipe instead of dropping ownership.
    body = copy.deepcopy(bodies[owner["creature"]["body_package"]])
    body["species"] = "fixture.detection"
    bodies["fixture.body.detection"] = body
    owner["creature"]["body_package"] = "fixture.body.detection"
    peer = copy.deepcopy(owner["creature"])
    peer["name"] = "PRIVATE detection peer"
    width, depth, height = (32, 32, 8) if paging or mapping else (7, 5, 2)
    manifest.update(id="imported-detection-pages" if paging else "imported-detection",
                    default_character=1, characters=[owner], archetypes={**actors, **items["items"]},
                    creatures={"species": species, "body_packages": bodies}, appearance_pools=items["appearance_pools"],
                    spatial_scopes={"study": {"members": [1], "rooms": {"1": [
                        {"region": 1, "min": [0, 0, 0], "max": [width - 1, depth - 1, height - 1]}]},
                        "passages": [], "doorways": []}})
    if mapping and certified:
        manifest["spatial_scopes"]["study"]["mapping"] = {
            "room_frames": [{"room": 1, "region": 1, "up": "up"}]}
    stock = [{"id": identity, "at": [2, 2, 0], "archetype": "rogue." + kind,
              "quantity": quantity, "carried_by": 1} for identity, kind, quantity in DOSES.values()]
    if mapping:
        stock.append({"id": 34, "at": [2, 2, 0], "archetype": "rogue.magic_mapping",
                      "quantity": 2, "carried_by": 1})
    # Presence coalesces by TRUE location. These 513 distinct far floor cells
    # cross the wire page boundary without confusing item count with row count.
    # Source regions permit at most32x32x8. Use513 distinct supported floor
    # locations outside the genuine Manhattan sight16 from eye(2,2,1).
    food_cells = ([[x, y, 0] for y in range(depth) for x in range(width)
                   if abs(x - 2) + abs(y - 2) + 1 > 16][:513] if paging else [[2, 2, 0]])
    assert len(food_cells) == (513 if paging else 1)
    food = [{"id": 1000 + i, "at": at, "archetype": "rogue.ration", "quantity": 1}
            for i, at in enumerate(food_cells)]
    ground = food + ([] if paging else [{"id": 2000, "at": [2, 2, 0],
                                       "archetype": "rogue.wand_nothing"}])
    region = {"id": 1, "name": "Detection fixture chamber", "size": [width, depth, height],
              "chamber": True, "lit": True, "gravity": [0, 0, -1],
              "anchors": {"start": [2, 2, 0]}, "items": stock + ground,
              "actors": [{"id": 2, "at": [4, 2, 0], "turn_ticks": 100,
                          "controller": "external", "creature": peer,
                          "body": copy.deepcopy(owner["body"])}]}
    return {"manifest": manifest, "regions": [region],
            "rules": (production / "rules.toml").read_text(encoding="utf-8")}


def prepare(test, *, paging=False, mapping=False, certified=True):
    # The ordinary source writer invokes the genuine tor-scenario compiler only
    # when an actual process test runs, never during pure fixture tests/import.
    from discovery_package import write_package
    data = fixture(paging=paging, mapping=mapping, certified=certified)
    return write_package(test, data, data["manifest"]["id"])


def initial_doses(test, frame):
    rows = frame["state"]["observation"]["inventory"]
    found = {}
    for key, (_, _, quantity) in DOSES.items():
        category = "scroll" if key == "food" else "potion"
        selected = [row for row in rows if row["class"] == category and row["quantity"] == str(quantity)]
        test.assertEqual(len(selected), 1, (key, rows))
        test.assertFalse(selected[0]["identified"])
        found[key] = selected[0]["id"]
    return found


def raw_checkpoint(test, client=None):
    """Full genuine persisted checkpoint, not a fabricated reduced save mirror."""
    if client is not None:
        test.assertIsNone(test.request(client, {"type": "save"})["error"])
    with closing(sqlite3.connect(test.save.resolve().as_uri() + "?mode=ro", uri=True)) as database:
        row = database.execute("SELECT sequence,payload FROM checkpoint").fetchone()
        test.assertIsNotNone(row, "requires a paid record and checkpoint interval1")
        sequence, payload = row
        test.assertEqual(database.execute("SELECT count(*) FROM journal WHERE sequence > ?",
                                         (sequence,)).fetchone()[0], 0)
    return json.loads(decode_checkpoint_payload(payload))


def summary(frame, kind):
    return next(row for row in frame["state"]["observation"]["charts"] if row["kind"] == kind)


def query(frame, kind):
    selected = summary(frame, kind)
    if selected["chart"] is None:
        raise AssertionError("requested chart has no advertised capability")
    # Headless must expose the admitted observation boundary from common state.
    # Its ready cursor can include later readiness events and is NOT a substitute.
    base = frame["observation_base"]
    if base is None:
        raise AssertionError("missing admitted observation base")
    return {"context": copy.deepcopy(frame["input_context"]["stream"]),
            "branch": frame["branch"], "base": copy.deepcopy(base),
            "chart": selected["chart"], "cursor": None}


def explicit_chart_reply(frames, requested):
    """Last transact response is its correlated completion, unlike idle updates."""
    found = [frame["message"] for frame in frames
             if frame.get("type") == "response" and (frame.get("message") or {}).get("type") == "chart_page"
             and frame["message"]["page"]["start"] == "0"
             and all(frame["message"]["page"][key] == requested[key]
                     for key in ("context", "branch", "base", "chart"))]
    if not found:
        raise AssertionError("explicit chart request has no exact initial response")
    initial = found[-1]
    if sum((frame.get("message") or {}).get("request_id") == initial["request_id"] for frame in frames) != 1:
        raise AssertionError("duplicate explicit chart reply ID")
    return initial


def wire_integer(value, *, signed=False):
    if not isinstance(value, str):
        raise AssertionError("wire integer is not a decimal string")
    try:
        number = int(value)
    except ValueError as error:
        raise AssertionError("invalid wire integer") from error
    minimum, maximum = (-(1 << 63), (1 << 63) - 1) if signed else (0, (1 << 64) - 1)
    if str(number) != value or not minimum <= number <= maximum:
        raise AssertionError("noncanonical or out-of-range wire integer")
    return number


def validate_pages(pages, requested):
    """Independent strict transcript oracle; never deduplicate equal occurrences."""
    if not pages:
        raise AssertionError("no real chart pages")
    first = pages[0]
    if first["kind"] not in ("monsters", "food", "magic", "mapping"):
        raise AssertionError("unknown chart kind")
    if first["mode"] != ("live" if first["kind"] == "monsters" else "historical"):
        raise AssertionError("wrong chart lifetime mode")
    wire_integer(first["acquired_at"])
    fixed = ("context", "branch", "base", "chart", "kind", "mode", "acquired_at",
             "total_components", "total_records")
    for key in ("context", "branch", "base", "chart"):
        if first[key] != requested[key]:
            raise AssertionError("chart reply identity mismatch: " + key)
    components, total = wire_integer(first["total_components"]), wire_integer(first["total_records"])
    if not 0 < components <= total:
        raise AssertionError("invalid chart totals")
    records = []
    for index, page in enumerate(pages):
        if set(page) != {*fixed, "start", "records", "next"}:
            raise AssertionError("unexpected/private chart header field")
        if any(page[key] != first[key] for key in fixed) or wire_integer(page["start"]) != len(records):
            raise AssertionError("chart continuation changed identity or ordinal")
        if not 1 <= len(page["records"]) <= 512:
            raise AssertionError("invalid page count")
        records.extend(page["records"])
        expected_next = str(len(records)) if len(records) < total else None
        if page["next"] != expected_next or len(records) > total:
            raise AssertionError("invalid chart continuation cursor")
        if (index + 1 == len(pages)) != (page["next"] is None):
            raise AssertionError("incomplete or extra chart page")
    if len(records) != total:
        raise AssertionError("partial chart publication")
    alignments = {}
    previous = None
    for ordinal, record in enumerate(records):
        if ordinal < components:
            if (set(record) != {"record", "component", "alignment"}
                    or record["record"] != "component" or wire_integer(record["component"]) != ordinal
                    or record["alignment"] not in ("aligned", "inconsistent")):
                raise AssertionError("invalid component declaration prefix")
            alignments[record["component"]] = record["alignment"]
        else:
            if (set(record) != {"record", "component", "position", "mark"}
                    or record["record"] != "row" or record["component"] not in alignments):
                raise AssertionError("invalid/non-public chart row")
            position = record["position"]
            if alignments[record["component"]] == "aligned":
                if not isinstance(position, list) or len(position) != 3:
                    raise AssertionError("aligned row lacks coordinates")
                position = tuple(wire_integer(value, signed=True) for value in position)
            elif position is not None:
                raise AssertionError("inconsistent row invented coordinates")
            mark = record["mark"]
            expected_kind = "monster" if first["kind"] == "monsters" else first["kind"]
            allowed = ({"kind", "name", "asset"} if expected_kind == "monster"
                       else {"kind", "fact"} if expected_kind == "mapping" else {"kind"})
            if set(mark) != allowed or mark["kind"] != expected_kind:
                raise AssertionError("private/actionable chart mark")
            if expected_kind == "mapping":
                fact = mark["fact"]
                if fact["kind"] == "wall_face":
                    if set(fact) != {"kind", "normal"}:
                        raise AssertionError("private mapping wall field")
                    normal = fact["normal"]
                    if position is None:
                        if normal is not None:
                            raise AssertionError("inconsistent wall invented orientation")
                    elif (not isinstance(normal, list) or len(normal) != 3
                          or any(type(value) is not int or value not in (-1, 0, 1) for value in normal)
                          or sum(abs(value) for value in normal) != 1):
                        raise AssertionError("invalid cardinal mapping normal")
                elif (set(fact) != {"kind"} or fact["kind"] not in
                      ("passage", "doorway", "stairs", "secret_door", "trap_presence")):
                    raise AssertionError("private or unknown mapping fact")
            key = (wire_integer(record["component"]), () if position is None else position,
                   mark.get("name", ""), mark.get("asset") or "")
            if previous is not None and previous > key:
                raise AssertionError("chart occurrences are not ordered")
            previous = key
    return records


def historical_payload(pages):
    first = pages[0]
    return {**{key: copy.deepcopy(first[key]) for key in
               ("kind", "mode", "acquired_at", "total_components", "total_records")},
            "records": [copy.deepcopy(row) for page in pages for row in page["records"]]}


def rows(pages):
    return [row for page in pages for row in page["records"] if row["record"] == "row"]


def assert_monster_sample(test, frame, pages, phase):
    peers = [row for row in frame["state"]["observation"]["visible_actors"]
             if row["id"] != frame["state"]["observation"]["self_target"]]
    test.assertEqual(len({row["id"] for row in peers}), 1)
    for peer in peers:
        test.assertEqual((peer["name"], peer.get("asset")), VISUAL[phase])
        test.assertEqual(peer["description"], "")
        test.assertEqual(peer["appearance_seed"], "p_" + "00" * 32)
    found = rows(pages)
    test.assertEqual(len(found), 1)
    test.assertEqual(found[0]["mark"], {"kind": "monster", "name": MONSTER_SENSE[phase][0],
                                        "asset": MONSTER_SENSE[phase][1]})
    test.assertNotIn("PRIVATE", json.dumps(pages))
    test.assertNotEqual(VISUAL[phase], MONSTER_SENSE[phase])


def checkpoint_payload(test):
    """Unmodified bytes written by the actual server's checkpoint writer."""
    with closing(sqlite3.connect(test.save.resolve().as_uri() + "?mode=ro", uri=True)) as database:
        row = database.execute("SELECT payload FROM checkpoint").fetchone()
        test.assertIsNotNone(row)
        return bytes(row[0])


# This relay only observes real masked/unmasked frames. It never synthesizes,
# retries, delays or normalizes a chart request or response.
class ChartCaptureRelay(StreamRelay):
    def __init__(self, upstream):
        self.chart_requests = []
        self.chart_replies = []
        self.chart_lock = threading.Lock()
        super().__init__(upstream, on_server_message=self.observe_server)

    def observe_server(self, message):
        if message.get("type") == "chart_page":
            with self.chart_lock:
                self.chart_replies.append(copy.deepcopy(message))

    def initial_request(self, requested):
        """The unique real initial request at this exact snapshot boundary.

        None means the common client has not sent its managed request yet, not
        permission for another writer to compete with a locally bound request.
        """
        with self.chart_lock:
            if self.errors:
                raise AssertionError(self.errors)
            found = [message for message in self.chart_requests
                     if message["request"]["query"] == requested]
            if len(found) > 1:
                raise AssertionError("multiple initial chart requests at selected snapshot boundary")
            return copy.deepcopy(found[0]) if found else None

    def pair(self, requested, initial_request_id=None):
        exchange = self.stream(requested, initial_request_id)
        if len(exchange["queries"]) != 1:
            raise AssertionError("expected one actual correlated Chart reply")
        return exchange["queries"][0], exchange["pages"][0]

    def stream(self, requested, initial_request_id=None, *, complete=True):
        """One request-bound transfer, not every hydration of a reused token.

        Automatic refresh and explicit reads can lawfully share all query header
        values. The initial real request ID selects a transfer; continuation IDs
        come only from actual requests with that boundary and the previous cursor.
        No page/row is deduplicated. Missing, repeated, swapped or extra replies
        within the selected transfer remain errors.
        """
        with self.chart_lock:
            if self.errors:
                raise AssertionError(self.errors)
            candidates = [message for message in self.chart_requests
                          if all(message["request"]["query"][key] == requested[key]
                                 for key in ("context", "branch", "base", "chart"))]
            initials = [message for message in candidates if message["request"]["query"]["cursor"] is None
                        and (initial_request_id is None or message["request_id"] == initial_request_id)]
            if not initials or (initial_request_id is not None and len(initials) != 1):
                raise AssertionError("missing or duplicate genuine initial chart request")
            initial = initials[-1]
            start = next(index for index, message in enumerate(candidates) if message is initial)
            requests = []
            for message in candidates[start:]:
                if requests and message["request"]["query"]["cursor"] is None:
                    break  # a separate actual initial request, not continuation
                requests.append(message)
            if len({message["request_id"] for message in requests}) != len(requests):
                raise AssertionError("duplicate actual chart request ID")
            replies, cursor = [], None
            for index, request in enumerate(requests):
                query = request["request"]["query"]
                if query["cursor"] != cursor or (index and cursor is None):
                    raise AssertionError("actual chart continuation cursor mismatch")
                matching = [reply for reply in self.chart_replies if reply["request_id"] == request["request_id"]]
                if not matching and not complete and index + 1 == len(requests):
                    break  # actual request is in flight; never publish this prefix
                if len(matching) != 1:
                    raise AssertionError("missing or duplicate actual chart reply")
                reply = matching[0]
                if any(reply["page"][key] != query[key] for key in ("context", "branch", "base", "chart")):
                    raise AssertionError("actual chart reply changed request boundary")
                if reply["page"]["start"] != (query["cursor"] or "0"):
                    raise AssertionError("actual chart reply skipped/repeated an ordinal")
                replies.append(reply)
                cursor = reply["page"]["next"]
            selected_ids = {request["request_id"] for request in requests}
            wire_order = [reply["request_id"] for reply in self.chart_replies if reply["request_id"] in selected_ids]
            if wire_order != [reply["request_id"] for reply in replies]:
                raise AssertionError("actual chart replies repeated or arrived outside request order")
            if complete:
                if len(replies) != len(requests) or cursor is not None:
                    raise AssertionError("incomplete genuine chart exchange")
                validate_pages([reply["page"] for reply in replies], requested)
            return copy.deepcopy({"queries": requests, "pages": replies})

    def forward(self, source, target):
        try:
            header = bytearray()
            while not header.endswith(b"\r\n\r\n"):
                header.extend(exact(source, 1))
                if len(header) > 65536:
                    raise ValueError("Oversized client handshake")
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
                if prefix[0] == 0x81:
                    message = json.loads(bytes(value ^ mask[index % 4]
                                               for index, value in enumerate(payload)))
                    if (message.get("type") == "request"
                            and (message.get("request") or {}).get("type") == "chart"):
                        with self.chart_lock:
                            self.chart_requests.append(copy.deepcopy(message))
                target.sendall(frame)
        except (EOFError, OSError):
            pass
        except Exception as error:
            self.errors.append(error)
        finally:
            self.close_sockets()
