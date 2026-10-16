"""Explicit catalog pilot: requires TOR_SERVICE_CONTRACT_HOST, never auto-builds.

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

from service_contract_host import HostError, ServiceHost

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
        self.assertEqual(result["live"], live, "catalog must paginate, never disconnect")
        if live:
            self.assertIsNone(result["closing"])
        for frame in result["frames"]:
            self.assertLessEqual(len(frame.encode("utf-8")), self.frame_bytes)
        return result["frames"] if raw else [json.loads(frame) for frame in result["frames"]]

    def connect(self, role="wizard"):
        connection = self.host.call("connect", case=self.case, role=role,
                                    user=role, actors=[1])["connection"]
        self.drain(connection)
        self.host.request(case=self.case, connection=connection,
                       request_id="attach", request={"type": "attach", "actor": "1"})
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
        for message in ('not json', '{"type":"hello","protocol":44,"token":"x","frontend":"test"}',
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
