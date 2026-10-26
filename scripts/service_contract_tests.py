"""Explicit Service contracts: require TOR_SERVICE_CONTRACT_HOST, never auto-build.

Run: python -m unittest discover -s scripts -p service_contract_tests.py -v
These are Service-boundary tests, not real server/client acceptance tests.
"""
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tomllib
import unittest

from arena_matrix import toml_source
from service_contract_host import HostError, ServiceHost
from linked_hold_service_contracts import LinkedHoldServiceContracts as LinkedHoldServiceContracts
from theft_service_contracts import TheftServiceContracts as TheftServiceContracts
from utility_potions_service_contracts import UtilityPotionsServiceContracts as UtilityPotionsServiceContracts
from hallucination_service_contracts import HallucinationServiceContracts as HallucinationServiceContracts
from utility_wands_service_contracts import UtilityWandsServiceContracts as UtilityWandsServiceContracts
from discovery_service_contracts import (
    DiscoveryServiceContracts as DiscoveryServiceContracts,
    DiscoveryObserverServiceContracts as DiscoveryObserverServiceContracts,
)

ROOT = Path(__file__).resolve().parent.parent


def catalog_fixture(extras=40):
    manifest = tomllib.loads((ROOT / "scenarios/tests/generated-filler/scenario.toml").read_text(encoding="utf-8"))
    manifest["characters"][0]["creature"] = None
    for index in range(extras):
        manifest["archetypes"][f"rat_{index:02}"] = copy.deepcopy(manifest["archetypes"]["rat"])
    return {"manifest": manifest, "regions": [tomllib.loads(
        "id = 1\nname = 'Catalog room'\nsize = [3,3,1]\nanchors = {start=[1,1,0]}\n")],
        "rules": (ROOT / "crates/simulation/data/default-ruleset.toml").read_text(encoding="utf-8"),
        "seed": 42, "wizard": True, "frame_bytes": 4096,
        "client_bytes": 65536, "total_bytes": 262144}


def sequence_rider_fixture():
    """Finite ordinary authored source, with one copied seven-point gated rider."""
    root = ROOT / "scenarios/two-room"
    manifest = tomllib.loads((root / "scenario.toml").read_text(encoding="utf-8"))
    rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text(encoding="utf-8"))

    def damage(amount):
        return {"kind": "damage", "damage": {"kind": "components", "components": [
            {"category": "impact", "amount": {"kind": "fixed", "amount": amount}}]}}

    strikes = [{"offset": offset, "check": {"actor": "user", "skill": {"kind": "selected_weapon"},
        "against": {"kind": "defense", "defense": "physical"}}, "on_success": [damage(1)],
        "on_failure": [], "descriptors": [], "riders": []} for offset in (0, 100, 200)]
    rules["abilities"].append({"id": "triple_strike", "name": "Triple Strike", "tags": [],
        "basic_attack": False, "timing": {"source": {"kind": "fixed", "prepare": 100, "recover": 100},
        "scaled_by": "speed"}, "reach": {"kind": "melee"}, "interaction": "hostile_to_target",
        "ai": {"priority": 100, "hostile": True, "minimum_resource_percent": 0, "range_preference": "any"},
        "strikes": {"kind": "natural_melee", "additional_riders": [{"trigger": "success",
            "condition": {"kind": "sequence_successful_hits", "minimum": 2},
            "effects": [damage(7)], "descriptors": []}]}})
    manifest["creatures"] = {"species": {"sequence_fighter": {"kind": "humanoid",
        "behavior": {"disposition": "mean", "idle": "explore", "memory_ticks": 1000},
        "attributes": dict.fromkeys(("strength", "speed", "intellect", "willpower", "awareness", "presence"), 0),
        "melee": {"skill": "heavy_weaponry", "bonus": 100, "wind_up": 100, "recovery": 100, "strikes": strikes},
        "modifiers": [{"scope": "always", "op": {"kind": "grant_ability", "ability": "triple_strike"}},
            {"scope": "always", "op": {"kind": "add", "stat": {"kind": "health"}, "value": 200}}]}}}

    def creature(name):
        return {"species": "sequence_fighter", "name": name, "faction": "neutral", "binding": "intellect",
            "starting_skills": ["heavy_weaponry", "athletics"], "restoration": "authored",
            "nutrition": {"kind": "exempt"}, "hit_dice": [{"source": "racial"}]}

    manifest["characters"][0]["creature"] = creature("Escaper")
    regions = [tomllib.loads((root / f"regions/{index}.toml").read_text(encoding="utf-8")) for index in (1, 2)]
    regions[0]["items"] = []
    regions[0]["actors"] = [{"id": 2, "at": [2, 1, 0], "turn_ticks": 100,
        "controller": "external", "creature": creature("Sequence sentinel")}]
    return {"manifest": manifest, "regions": regions, "rules": toml_source(rules), "seed": 42,
        "wizard": True, "frame_bytes": 65536, "client_bytes": 262144, "total_bytes": 524288}


def idle_roaming_fixture():
    """One authored active region; a solid wall keeps the two actors unseen."""
    manifest = tomllib.loads((ROOT / "scenarios/two-room/scenario.toml").read_text(encoding="utf-8"))
    # Intentionally omit species behavior and the enrolled actor's AI profile.
    manifest["creatures"] = {"species": {"idle_roamer": {"kind": "humanoid",
        "attributes": dict.fromkeys(("strength", "speed", "intellect", "willpower", "awareness", "presence"), 0),
        "melee": {"skill": "heavy_weaponry", "bonus": 0, "wind_up": 50, "recovery": 50,
            "strikes": [{"offset": 0, "check": {"actor": "user", "skill": {"kind": "selected_weapon"},
                "against": {"kind": "defense", "defense": "physical"}}, "on_success": [{"kind": "damage",
                    "damage": {"kind": "components", "components": [{"category": "impact",
                        "amount": {"kind": "fixed", "amount": 1}}]}}],
                "on_failure": [], "descriptors": [], "riders": []}]}}}}

    def creature(name):
        return {"species": "idle_roamer", "name": name, "faction": "neutral", "binding": "intellect",
            "starting_skills": ["heavy_weaponry", "athletics"], "restoration": "authored",
            "nutrition": {"kind": "exempt"}, "hit_dice": [{"source": "racial"}]}

    manifest["characters"][0]["creature"] = creature("Waiting player")
    region = {"id": 1, "name": "Divided idle room", "size": [13, 5, 2],
        "chamber": True, "gravity": [0, 0, -1], "anchors": {"start": [1, 2, 0]},
        "walls": [[5, y, z] for y in range(5) for z in range(2)],
        "actors": [{"id": 2, "at": [9, 2, 0], "turn_ticks": 100, "controller": "ai",
            "creature": creature("Idle roamer")}]}
    return {"manifest": manifest, "regions": [region],
        "rules": (ROOT / "crates/simulation/data/default-ruleset.toml").read_text(encoding="utf-8"),
        "seed": 42, "wizard": False,
        "frame_bytes": 65536, "client_bytes": 262144, "total_bytes": 524288}


def assert_catalog_page(test, page, after):
    """Scenario assertions, not a replacement for the production wire decoder."""
    selectors = [row["selector"] for row in page["entries"]]
    test.assertTrue(selectors)
    test.assertLessEqual(len(selectors), 32)
    test.assertEqual(selectors, sorted(set(selectors)), "page selectors must be strictly ordered")
    if after is not None:
        test.assertGreater(selectors[0], after, "continuation must advance beyond its cursor")
    if page["next"] is not None:
        test.assertEqual(page["next"], selectors[-1], "continuation must name the final row")
    return selectors


def expected_catalog_selectors():
    # Independent authored identities, not values inferred from server output.
    return ["archetype:rat", *[f"archetype:rat_{index:02}" for index in range(40)]]


def assert_catalog_complete(test, selectors):
    test.assertEqual(selectors, expected_catalog_selectors(), "complete ordered authored catalog")


class CatalogServiceContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        executable = os.environ.get("TOR_SERVICE_CONTRACT_HOST")
        if not executable:
            raise RuntimeError("Build the opt-in host and set TOR_SERVICE_CONTRACT_HOST; no tests were run")
        expected = os.environ.get("TOR_SERVICE_CONTRACT_SHA256")
        if not expected:
            raise RuntimeError("Set TOR_SERVICE_CONTRACT_SHA256 from the qualified host build receipt")
        cls.host = ServiceHost(executable, expected_sha256=expected)
        cls.addClassCleanup(cls.host.close)

    def setUp(self):
        self.frame_bytes = 4096
        self.case = self.host.call("create", **catalog_fixture())["case"]
        self.addCleanup(self.close_case)

    def close_case(self):
        if self.case is not None:
            self.host.call("close", case=self.case)
            self.case = None

    def drain(self, connection, *, live=True, raw=False):
        result = self.host.call("drain", case=self.case, connection=connection)
        self.assertEqual(result["live"], live, "Service contract must retain its live connection")
        if live:
            self.assertIsNone(result["closing"])
        for frame in result["frames"]:
            self.assertLessEqual(len(frame.encode("utf-8")), self.frame_bytes)
        return result["frames"] if raw else [json.loads(frame) for frame in result["frames"]]

    def connect(self, role="wizard", actor=1):
        connection = self.host.call("connect", case=self.case, role=role,
                                    user=role, actors=[actor])["connection"]
        self.drain(connection)
        self.host.request(case=self.case, connection=connection,
                       request_id="attach", request={"type": "attach", "actor": str(actor)})
        messages = self.drain(connection)
        snapshots = [row["snapshot"] for row in messages if row["type"] == "snapshot"]
        self.assertEqual(len(snapshots), 1, "positive fixture control: successful bounded attach")
        return connection, snapshots[0]

    def query(self, connection, snapshot, operation, request_id="query", *, raw=False):
        self.host.request(case=self.case, connection=connection, request_id=request_id,
            request={"type": "command", "context": {"stream": snapshot["context"],
                "readiness_revision": snapshot["readiness"]["revision"]},
                "branch": snapshot["branch"], "command": {"type": "wizard",
                    "expected_revision": snapshot["state"]["revision"], "operation": operation}})
        return self.drain(connection, raw=raw)

    def test_default_enrolled_ai_roams_unseen_after_encoded_player_wait(self):
        self.close_case()
        fixture = idle_roaming_fixture()
        self.frame_bytes = fixture["frame_bytes"]
        self.case = self.host.call("create", **fixture)["case"]
        player, initial_player = self.connect("player")
        observer, initial_roamer = self.connect("spectator", actor=2)

        def checked_drain(connection):
            messages = self.drain(connection)
            self.assertFalse([row for row in messages if row["type"] == "error"], messages)
            statuses = [row["update"]["body"]["status"] for row in messages
                if row["type"] == "update" and row["update"]["body"]["type"] == "intention"]
            self.assertFalse([row for row in statuses if row["phase"] in ("failed", "cancelled")], statuses)
            return messages

        def snapshot(connection):
            self.host.request(case=self.case, connection=connection,
                request_id="snapshot", request={"type": "snapshot"})
            snapshots = [row["snapshot"] for row in checked_drain(connection) if row["type"] == "snapshot"]
            self.assertEqual(len(snapshots), 1)
            return snapshots[0]

        def position(current, hidden_name):
            view = current["state"]["observation"]
            self.assertFalse(any(actor["name"] == hidden_name for actor in view["visible_actors"]),
                "the wall must preserve each actor's own disclosed knowledge")
            cells = [cell for cell in view["visible_cells"]
                if cell["position"] == {"x": 0, "y": 0, "z": 0}]
            self.assertEqual(len(cells), 1, "positive control: observer sees its actor's own floor")
            self.assertFalse(cells[0]["wall"])
            return cells[0]["key"]

        player_position = position(initial_player, "Idle roamer")
        positions = {position(initial_roamer, "Waiting player")}
        self.host.request(case=self.case, connection=player,
            request_id="acquire", request={"type": "acquire_control"})
        checked_drain(player)
        for turn in range(6):
            current = snapshot(player)
            self.assertTrue(current["has_control"])
            self.assertTrue(current["readiness"]["admission"])
            self.host.request(case=self.case, connection=player, request_id=f"wait-{turn}",
                request={"type": "command", "context": {"stream": current["context"],
                    "readiness_revision": current["readiness"]["revision"]}, "branch": current["branch"],
                    "command": {"type": "act", "expected_revision": current["state"]["revision"],
                        "action": {"type": "wait"}}})
            receipts = [row["receipt"] for row in checked_drain(player) if row["type"] == "ack"]
            self.assertEqual(len(receipts), 1)
            self.assertEqual(receipts[0]["type"], "admitted")
            intention = receipts[0]["intention"]
            resolved = []
            # Explicit bounded Service turns, not a test-host run-until-blocked shortcut.
            for _ in range(8):
                result = self.host.call("step", case=self.case)
                messages = [row for peer in (player, observer) for row in checked_drain(peer)]
                resolved.extend(row["update"]["body"]["status"] for row in messages
                    if row["type"] == "update" and row["update"]["body"]["type"] == "intention"
                    and row["update"]["body"]["status"]["intention"] == intention
                    and row["update"]["body"]["status"]["phase"] == "resolved")
                if result == {"status": "blocked"}:
                    break
                self.assertEqual(result, {"status": "progress"}, messages)
            else:
                self.fail("two-actor idle fixture exceeded its eight-step wait budget")
            self.assertEqual(len(resolved), 1, "ordinary player wait must really finish exactly once")
            self.assertEqual(position(snapshot(player), "Idle roamer"), player_position)
            roamer = snapshot(observer)
            self.assertEqual(roamer["actor"], "2")
            self.assertFalse(roamer["has_control"])
            self.assertFalse(roamer["readiness"]["admission"])
            positions.add(position(roamer, "Waiting player"))
        self.assertGreater(len(positions), 1,
            "omitted behavior and profile must produce unseen autonomous movement through Service")

    def test_encoded_sequence_gate_waits_for_two_hits_and_applies_once(self):
        self.close_case()
        fixture = sequence_rider_fixture()
        self.frame_bytes = fixture["frame_bytes"]
        self.case = self.host.call("create", **fixture)["case"]
        attacker, _ = self.connect()
        defender, _ = self.connect("player", actor=2)
        peers = (attacker, defender)

        def checked_drain(connection):
            messages = self.drain(connection)
            # A blocked scheduler or an acknowledgement is not proof of success.
            # Check the actual production error/warning and intention frames too.
            self.assertFalse([row for row in messages if row["type"] == "error"], messages)
            statuses = [row["update"]["body"]["status"] for row in messages
                if row["type"] == "update" and row["update"]["body"]["type"] == "intention"]
            self.assertFalse([row for row in statuses if row["phase"] in ("failed", "cancelled")], statuses)
            return messages

        def snapshot(connection):
            self.host.request(case=self.case, connection=connection,
                request_id="snapshot", request={"type": "snapshot"})
            snapshots = [row["snapshot"] for row in checked_drain(connection) if row["type"] == "snapshot"]
            self.assertEqual(len(snapshots), 1)
            return snapshots[0]

        def send_command(connection, current, command, request_id):
            self.host.request(case=self.case, connection=connection, request_id=request_id,
                request={"type": "command", "context": {"stream": current["context"],
                    "readiness_revision": current["readiness"]["revision"]},
                    "branch": current["branch"], "command": {"expected_revision": current["state"]["revision"],
                        **command}})
            return checked_drain(connection)

        def step(expected="progress"):
            result = self.host.call("step", case=self.case)
            messages = [row for peer in peers for row in checked_drain(peer)]
            self.assertEqual(result, {"status": expected}, messages)
            return messages

        for peer in peers:
            self.host.request(case=self.case, connection=peer,
                request_id="acquire", request={"type": "acquire_control"})
            checked_drain(peer)
        current = snapshot(attacker)
        before_query = self.host.call("audit", case=self.case, actor=1, user="wizard")
        messages = send_command(attacker, current, {"type": "inspect_strike_source", "actor": "1",
            "target_revision": current["state"]["revision"], "source": {"kind": "ability", "id": "triple_strike"},
            "cursor": None}, "gate-inspection")
        pages = [row["page"] for row in messages if row["type"] == "creature_strike_page"]
        self.assertEqual(len(pages), 1, "small authored source fits a complete encoded page")
        page = pages[0]
        self.assertIsNone(page["next"])
        self.assertEqual(page["resolved"]["kind"], "natural_melee")
        self.assertEqual(page["resolved"]["natural_riders"], [0, 0, 0])
        self.assertEqual(page["resolved"]["additional_riders"], 1)
        riders = [row for row in page["entries"] if row["kind"] == "rider_header"]
        self.assertEqual([row["strike"] for row in riders], [0, 1, 2])
        self.assertEqual([row["trigger"] for row in riders], ["success"] * 3)
        self.assertEqual([row["condition"] for row in riders],
            [{"kind": "sequence_successful_hits", "minimum": 2}] * 3)
        self.assertEqual(self.host.call("audit", case=self.case, actor=1, user="wizard"), before_query)
        target = next(row["id"] for row in current["state"]["observation"]["visible_actors"]
            if row["name"] == "Sequence sentinel")
        target_before = int(snapshot(defender)["state"]["observation"]["combat"]["hp"])
        messages = send_command(attacker, current, {"type": "act", "action": {"type": "use_ability",
            "id": "triple_strike", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}},
            "supplies": {"kind": "carried"}}}, "sequence-admission")
        receipts = [row["receipt"] for row in messages if row["type"] == "ack"]
        self.assertEqual(len(receipts), 1)
        self.assertEqual(receipts[0]["type"], "admitted")
        intention = receipts[0]["intention"]
        # Two admitted actions make the one-turn boundary observable: a hidden
        # run-until-blocked loop would also execute the wait and damage the target.
        send_command(defender, snapshot(defender), {"type": "act", "action": {"type": "wait"}},
            "target-wait-1")
        step()
        self.assertEqual(int(snapshot(defender)["state"]["observation"]["combat"]["hp"]), target_before)
        endings = []
        for completed, damage in ((1, 1), (2, 9), (3, 10)):
            if completed > 1:
                send_command(defender, snapshot(defender), {"type": "act", "action": {"type": "wait"}},
                    f"target-wait-{completed}")
            messages = step()
            endings.extend(row["update"]["body"]["status"] for row in messages
                if row["type"] == "update" and row["update"]["body"]["type"] == "intention"
                and row["update"]["body"]["status"]["intention"] == intention
                and row["update"]["body"]["status"]["phase"] == "resolved")
            target_combat = snapshot(defender)["state"]["observation"]["combat"]
            self.assertEqual(target_before - int(target_combat["hp"]), damage,
                "only hit two adds the copied rider's seven damage")
            primaries = [row for row in target_combat["events"]
                if row["type"] == "ability" and row["ability"]["id"] == "triple_strike"]
            self.assertEqual([(row["strike"], row["outcome"]) for row in primaries], [(completed - 1, "applied")])
            progress = snapshot(attacker)["state"]["observation"]["combat"]["sequence"]
            if completed < 3:
                self.assertEqual((progress["completed"], progress["total"]), (completed, 3))
            else:
                self.assertIsNone(progress)
        self.assertEqual(len(endings), 1, "one terminal result on the controlling attachment")
        step("blocked")

    def test_generous_budget_proves_catalog_requires_trimming(self):
        self.close_case()
        fixture = catalog_fixture()
        self.frame_bytes = fixture["frame_bytes"] = 16384
        self.case = self.host.call("create", **fixture)["case"]
        connection, snapshot = self.connect()
        frames = self.query(connection, snapshot, "creature list", request_id="\\" * 128, raw=True)
        catalogs = [(frame, json.loads(frame)) for frame in frames
                    if json.loads(frame)["type"] == "creature_catalog"]
        self.assertEqual(len(catalogs), 1)
        raw, response = catalogs[0]
        page = response["page"]
        self.assertEqual(assert_catalog_page(self, page, None), expected_catalog_selectors()[:32])
        self.assertEqual(page["next"], expected_catalog_selectors()[31])
        self.assertGreater(len(raw.encode("utf-8")), 4096, "fixture must exercise whole-response trimming")

    def test_complete_encoded_catalog_pages_preserve_state_and_history(self):
        connection, snapshot = self.connect()
        before = self.host.call("audit", case=self.case, actor=1, user="wizard")
        after = None
        selectors = []
        for _ in range(42):
            command = "creature list" + (f" after {after}" if after is not None else "")
            messages = self.query(connection, snapshot, command, request_id="\\" * 128)
            pages = [row["page"] for row in messages if row["type"] == "creature_catalog"]
            self.assertEqual(len(pages), 1, "positive fixture control: bounded catalog reply")
            page = pages[0]
            selectors.extend(assert_catalog_page(self, page, after))
            self.assertEqual(self.host.call("audit", case=self.case, actor=1, user="wizard"), before)
            after = page["next"]
            if after is None:
                break
        self.assertIsNone(after)
        assert_catalog_complete(self, selectors)

    def test_catalog_reply_is_private_and_spectator_cannot_query(self):
        wizard, snapshot = self.connect()
        observer, watched = self.connect("spectator")
        self.query(wizard, snapshot, "creature list")
        self.assertFalse(any(row["type"] == "creature_catalog" for row in self.drain(observer)))
        denied = self.query(observer, watched, "creature list")
        self.assertTrue(any(row["type"] == "error" for row in denied))
        self.assertFalse(any(row["type"] == "creature_catalog" for row in denied))

    def test_malformed_operation_and_duplicate_create_preserve_active_case(self):
        before = self.host.call("audit", case=self.case, actor=1, user="wizard")
        self.assertIn("error", self.host.raw("not json"))
        self.assertIn("error", self.host.raw(json.dumps({"op": "close", "case": self.case, "unexpected": True})))
        with self.assertRaises(HostError):
            self.host.call("create", **catalog_fixture())
        self.assertEqual(self.host.call("audit", case=self.case, actor=1, user="wizard"), before)

    def test_close_releases_case_and_stale_handles_cannot_reach_new_case(self):
        connection, _ = self.connect()
        old_case = self.case
        self.close_case()
        self.case = self.host.call("create", **catalog_fixture(extras=1))["case"]
        self.assertNotEqual(self.case, old_case)
        with self.assertRaises(HostError):
            self.host.call("drain", case=old_case, connection=connection)
        connection, snapshot = self.connect()
        messages = self.query(connection, snapshot, "creature list")
        page = next(row["page"] for row in messages if row["type"] == "creature_catalog")
        self.assertEqual(len(page["entries"]), 2)
        self.assertIsNone(page["next"])

    def test_explicit_disconnect_and_unknown_connection_are_distinct(self):
        connection, _ = self.connect()
        self.host.call("disconnect", case=self.case, connection=connection)
        self.drain(connection, live=False)
        with self.assertRaises(HostError):
            self.host.call("drain", case=self.case, connection=connection + 100)

    def test_invalid_fixture_and_budget_do_not_leave_a_partial_case(self):
        self.close_case()
        broken = catalog_fixture()
        broken["rules"] = "invalid rules"
        with self.assertRaises(HostError):
            self.host.call("create", **broken)
        with self.assertRaisesRegex(HostError, "budget"):
            self.host.call("create", **{**catalog_fixture(), "total_bytes": 1024 * 1024})
        self.case = self.host.call("create", **catalog_fixture())["case"]
        self.connect()

    def test_connection_limit_is_explicit_and_existing_connections_survive(self):
        connections = [self.connect()[0] for _ in range(8)]
        with self.assertRaisesRegex(HostError, "limit"):
            self.host.call("connect", case=self.case, role="wizard", user="wizard", actors=[1])
        for connection in connections:
            self.drain(connection)

    def test_shared_wire_decoder_rejects_malformed_request_before_service(self):
        connection, _ = self.connect()
        before = self.host.call("audit", case=self.case, actor=1, user="wizard")
        for message in ('not json', '{"type":"hello","protocol":47,"token":"x","frontend":"test"}',
                        json.dumps({"type": "request", "request_id": "attach",
                            "request": {"type": "attach", "actor": 1}})):
            with self.subTest(message=message):
                with self.assertRaises(HostError):
                    self.host.call("request", case=self.case, connection=connection, message=message)
        self.assertEqual(self.host.call("audit", case=self.case, actor=1, user="wizard"), before)
        self.drain(connection)

    def test_eof_closes_an_active_case_and_oversized_input_fails_explicitly(self):
        for payload, expected in ((json.dumps({"op": "create", **catalog_fixture()}) + "\n", "ok"),
                                  ("x" * (1024 * 1024 + 1) + "\n", "error")):
            with self.subTest(expected=expected):
                result = subprocess.run([str(self.host.executable)], input=payload,
                    capture_output=True, text=True, encoding="utf-8", timeout=30, check=False)
                self.assertEqual(result.returncode, 0, result.stderr)
                lines = result.stdout.splitlines()
                self.assertEqual(len(lines), 2)
                response = json.loads(lines[1])
                self.assertIn(expected, response)
                if expected == "error":
                    self.assertIn("active case closed", response["error"])

    def test_python_fixture_variants_leave_the_selected_executable_unchanged(self):
        # Both fixtures are supplied to one already-running host; there is no Cargo
        # invocation, reload, compiler fallback or generated Rust in this path.
        original_pid = self.host.process.pid
        self.close_case()
        for extras in (2, 5):
            self.case = self.host.call("create", **catalog_fixture(extras=extras))["case"]
            connection, snapshot = self.connect()
            messages = self.query(connection, snapshot, "creature list")
            page = next(row["page"] for row in messages if row["type"] == "creature_catalog")
            self.assertEqual(len(page["entries"]), extras + 1)
            self.close_case()
        self.assertEqual(self.host.process.pid, original_pid)
        self.assertEqual(hashlib.sha256(self.host.executable.read_bytes()).hexdigest(), self.host.binary_sha256)


if __name__ == "__main__":
    unittest.main()


class CatalogAssertionTests(unittest.TestCase):
    def check(self, selectors, cursor=None, after=None):
        return assert_catalog_page(self, {"entries": [{"selector": value} for value in selectors],
                                          "next": cursor}, after)

    def test_order_and_cursor_progression_reject_plausible_wrong_pages(self):
        for selectors, cursor, after in [(["b", "a"], None, None),
                                          (["a", "a"], None, None),
                                          (["a", "b"], "a", None),
                                          (["a", "b"], "b", "a")]:
            with self.subTest(selectors=selectors, cursor=cursor, after=after):
                with self.assertRaises(AssertionError):
                    self.check(selectors, cursor, after)
        self.assertEqual(self.check(["b", "c"], "c", "a"), ["b", "c"])

    def test_expected_identities_are_independent_and_complete(self):
        expected = expected_catalog_selectors()
        self.assertEqual(len(expected), 41)
        self.assertEqual(expected[0], "archetype:rat")
        self.assertEqual(expected[-1], "archetype:rat_39")
        assert_catalog_complete(self, expected)
        for wrong in (["wrong:" + value for value in expected], expected[:-1], list(reversed(expected))):
            with self.assertRaises(AssertionError):
                assert_catalog_complete(self, wrong)
