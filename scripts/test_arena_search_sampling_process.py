"""Verify retained TPE history through the real CLI in a fresh interpreter."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from arena_search_sampling import initial_state, next_proposal, append_result

ROOT = Path(__file__).resolve().parents[1]


class SearchSamplingProcesses(unittest.TestCase):
    def test_cli_reconstructs_real_tpe_history_and_rejects_changed_evidence(self):
        state = initial_state({"strength": {"type": "integer", "low": 0, "high": 5}}, "42")
        for ordinal in range(14):
            params = next_proposal(state)
            state = append_result(state, params, -abs(params["strength"] - 3), {"coverage": 0})
        with tempfile.TemporaryDirectory(dir=ROOT / ".local") as directory:
            path = Path(directory) / "sampling.json"
            path.write_text(json.dumps(state), encoding="utf-8")
            original = path.read_bytes()
            def verify():
                return subprocess.run([sys.executable, ROOT / "scripts/arena_search_sampling.py", path],
                                      capture_output=True, text=True, encoding="utf-8", timeout=30)
            first = verify()
            self.assertEqual(first.returncode, 0, first.stderr)
            self.assertEqual(json.loads(first.stdout), next_proposal(state))
            self.assertEqual(verify().stdout, first.stdout)
            self.assertEqual(path.read_bytes(), original)
            bad = copy.deepcopy(state)
            bad["records"][-1]["value"] = 100
            path.write_text(json.dumps(bad), encoding="utf-8")
            rejected = verify()
            self.assertEqual(rejected.returncode, 2)
            self.assertIn("integrity", rejected.stderr)
