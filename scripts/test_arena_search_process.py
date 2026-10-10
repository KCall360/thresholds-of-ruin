"""Campaign recovery and all stages use the real compiler and ordinary Engine."""
import copy
import json
import subprocess
import sys
from unittest.mock import patch
import arena_search as search
from arena_matrix import create_matrix
from arena_evaluation import files, read_json
from process_harness import ProcessTestCase, ROOT


class ArenaSearchProcesses(ProcessTestCase):
    def test_campaign_recovers_batch_before_history_then_screens_accepts_and_exports(self):
        compiler = self.bin / ("tor-scenario" + self.suffix)
        binary = self.bin / ("tor-arena" + self.suffix)
        matrix = self.directory / "matrix"
        inventory = create_matrix(matrix, compiler, "interactions-v26", ["42"],
                                   families=["strength_heavy"], levels=[4])
        plan = matrix / inventory["cases"][0]["plan"]
        parameter_spec = self.directory / "parameters.json"
        spec = {"format": "tor-arena-search-parameters-v1", "parameters": {
            "strength": {"space": {"type": "integer", "low": 3, "high": 6}, "targets": [
                {"file": "scenario.toml", "path": ["characters", 0, "creature", "attributes", "strength"]}]}}}
        parameter_spec.write_text(json.dumps(spec), encoding="utf-8")
        config = self.directory / "config.json"
        config.write_text(json.dumps({"format": search.FORMAT, "plan": str(plan),
                                     "parameter_spec": str(parameter_spec), "seed": "42",
                                     "candidates": 16, "training_seeds": 1, "screening_seeds": 1,
                                     "acceptance_seeds": 2, "minimum_acceptance_difference": -1,
                                     "constraints": [{"name": "budget", "coefficients": {"strength": 1}, "maximum": 5}]}),
                          encoding="utf-8")
        output = self.directory / "search"
        write = search.write_json
        def interrupt(path, value):
            if path.name == "history.json" and len(value["records"]) == 3:
                raise InterruptedError("after third batch, before history publication")
            return write(path, value)
        with patch.object(search, "write_json", side_effect=interrupt), self.assertRaises(InterruptedError):
            search.run_campaign(binary, compiler, output, config_path=config)
        self.assertEqual(len(read_json(output / "history.json")["records"]), 2)
        self.assertTrue((output / "training/0002/result.json").exists())
        self.assertTrue((output / "training/0002/batch/batch.json").exists())
        retained = {str(path.relative_to(output)): (path.read_bytes(), path.stat().st_mtime_ns)
                    for path in (output / "training").rglob("*.json")}
        # Authoring paths are no longer needed after campaign input snapshots are ready.
        plan.write_text("changed authoring plan", encoding="utf-8")
        parameter_spec.write_text("changed authoring specification", encoding="utf-8")
        def invoke():
            return subprocess.run([sys.executable, ROOT / "scripts/arena_search.py", "--binary", binary,
                                   "--compiler", compiler, "resume", output], capture_output=True,
                                  text=True, encoding="utf-8", timeout=180)
        resumed = invoke()
        self.assertEqual(resumed.returncode, 0, resumed.stderr[-2000:])
        report = read_json(output / "campaign.json")
        self.assertEqual(report["status"], "accepted")
        self.assertEqual(report["training_trials"], 16)
        self.assertEqual(len(report["finalists"]), 3)
        self.assertEqual(len(set(report["finalists"])), 3)
        history = read_json(output / "history.json")
        self.assertTrue(any(record["constraints"]["validation"] > 0 for record in history["records"]))
        self.assertEqual({history["records"][i]["params"]["strength"] for i in report["finalists"]}, {3, 4, 5})
        header = read_json(output / "inputs.json")["header"]
        self.assertEqual(len(set(value for values in header["seeds"].values() for value in values)), 4)
        self.assertEqual(len(report["screening"]), 3)
        self.assertEqual(len(report["acceptance"]["seeds"]), 2)
        review = read_json(output / report["export"])
        self.assertEqual(review["promotion"], "manual_review_required")
        for key, fingerprint in review["files"].items():
            self.assertEqual(files(output / "review" / key), fingerprint)
        for name, state in retained.items():
            path = output / name
            self.assertEqual((path.read_bytes(), path.stat().st_mtime_ns), state)
        all_records = {str(path.relative_to(output)): (path.read_bytes(), path.stat().st_mtime_ns)
                       for path in output.rglob("*.json")}
        repeated = invoke()
        self.assertEqual(repeated.returncode, 0, repeated.stderr[-2000:])
        for name, state in all_records.items():
            path = output / name
            self.assertEqual((path.read_bytes(), path.stat().st_mtime_ns), state)
        altered = copy.deepcopy(history)
        altered["records"][0]["value"] = 999
        (output / "history.json").write_text(json.dumps(altered), encoding="utf-8")
        rejected = invoke()
        self.assertEqual(rejected.returncode, 2)
        self.assertIn("integrity", rejected.stderr)
