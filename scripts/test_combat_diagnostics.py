"""Window assembly and numerical replay comparison after wire validation."""
import copy
import unittest
import json
import subprocess
import sys
import tempfile
from pathlib import Path
from combat_diagnostics import collect_window, verify_export, numerical_equal, window_from_transcript


def page(start=1, end=3, *, captured=3, dropped=0):
    return {"enabled": True, "tick": "100", "captured": str(captured), "dropped": str(dropped),
            "retained": captured - dropped, "through": str(end), "records": [
                {"sequence": str(i), "tick": "100", "actor": "1", "target": "2",
                 "intention": str(i + 10), "origin_intention": str(i + 10),
                 "damage": i, "trace": {"steps": [], "truncated": False}}
                for i in range(start, end + 1)]}


class CombatDiagnosticExports(unittest.TestCase):
    def test_complete_window_orders_pages_keeps_receipts_and_verifies_hashes(self):
        result = collect_window([page(3, 3), page(1, 2)])
        self.assertEqual([r["sequence"] for r in result["records"]], ["1", "2", "3"])
        self.assertEqual(result["records"][0]["intention"], "11")
        self.assertTrue(verify_export(result))
        broken = copy.deepcopy(result)
        broken["records"][0]["damage"] += 1
        self.assertFalse(verify_export(broken))

    def test_missing_conflicting_or_changing_pages_fail(self):
        for pages in [[page(3, 3)], [page(), page(captured=4)], [page(), page(dropped=1)],
                      [page(end=2)], [page(), page(start=3, end=4)]]:
            with self.assertRaises(ValueError):
                collect_window(pages)
        conflict = page()
        conflict["records"][0]["damage"] += 1
        with self.assertRaises(ValueError):
            collect_window([page(), conflict])
        self.assertTrue(verify_export(collect_window([page(), page()])))

    def test_numeric_identity_ignores_owner_namespaces_but_preserves_presence_and_outcomes(self):
        baseline = collect_window([page()])
        changed = page()
        for record in changed["records"]:
            record["intention"] = str(int(record["intention"]) + 100)
            record["origin_intention"] = str(int(record["origin_intention"]) + 100)
        replay = collect_window([changed])
        self.assertNotEqual(baseline["sha256"], replay["sha256"])
        self.assertTrue(numerical_equal(baseline, replay))
        changed["records"][0]["intention"] = None
        self.assertFalse(numerical_equal(baseline, collect_window([changed])))
        changed = page()
        changed["records"][0]["trace"]["truncated"] = True
        self.assertFalse(numerical_equal(baseline, collect_window([changed])))

    def test_evicted_window_is_explicit_and_inputs_are_not_mutated(self):
        source = page(3, 4, captured=4, dropped=2)
        before = copy.deepcopy(source)
        result = collect_window([source])
        self.assertEqual(result["dropped"], "2")
        self.assertTrue(verify_export(result))
        self.assertEqual(source, before)

    def test_empty_disabled_window_and_bad_decimal_bounds(self):
        source = page(1, 0, captured=0)
        source["enabled"] = False
        self.assertTrue(verify_export(collect_window([source])))
        for key, value in [("captured", "01"), ("tick", "-1"), ("captured", str(2**64)),
                           ("retained", True), ("enabled", 1), ("through", "1")]:
            wrong = copy.deepcopy(source)
            wrong[key] = value
            with self.assertRaises(ValueError):
                collect_window([wrong])

    def test_transcript_context_boundaries_and_cli_export_compare(self):
        def frame(value, *, context="one", role="wizard", synchronized=True):
            return json.dumps({"role": role, "synchronized": synchronized,
                               "message": {"type": "combat_diagnostics", "context": context, "report": value}}) + "\n"
        lines = [frame(page(3, 3)), frame(page(1, 2))]
        self.assertTrue(verify_export(window_from_transcript(lines)))
        for invalid in [[frame(page(), role="player")], [frame(page(), synchronized=False)],
                        [frame(page(3, 3)), frame(page(1, 2), context="two")]]:
            with self.assertRaises(ValueError):
                window_from_transcript(invalid)
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "headless.jsonl"
            output = Path(directory) / "window.json"
            source.write_text("".join(lines), encoding="utf-8")
            script = Path(__file__).with_name("combat_diagnostics.py")
            exported = subprocess.run([sys.executable, script, "export", source, output],
                                      capture_output=True, text=True, timeout=10)
            self.assertEqual(exported.returncode, 0, exported.stderr)
            self.assertTrue(verify_export(json.loads(output.read_text(encoding="utf-8"))))
            compared = subprocess.run([sys.executable, script, "compare", output, output],
                                      capture_output=True, text=True, timeout=10)
            self.assertEqual(compared.returncode, 0, compared.stderr)
            source.write_text(frame(page(3, 3)), encoding="utf-8")
            before = output.read_bytes()
            rejected = subprocess.run([sys.executable, script, "export", source, output],
                                      capture_output=True, text=True, timeout=10)
            self.assertEqual(rejected.returncode, 2)
            self.assertEqual(output.read_bytes(), before)
            original = source.read_bytes()
            same = subprocess.run([sys.executable, script, "export", source, source],
                                  capture_output=True, text=True, timeout=10)
            self.assertEqual(same.returncode, 2)
            self.assertEqual(source.read_bytes(), original)
            self.assertEqual(list(Path(directory).glob(".combat-export-*")), [])


if __name__ == "__main__":
    unittest.main()
