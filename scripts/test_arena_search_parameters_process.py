"""Candidate edits are compiled, executed and replayed by the real backend."""
import json
import subprocess
import sys
import tomllib
from process_harness import ProcessTestCase, ROOT
from arena_evaluation import files, run_batch, replay
from arena_matrix import create_matrix


class SearchParameterProcesses(ProcessTestCase):
    def test_cli_preserves_sources_and_compiler_rejects_illegal_ordinary_attributes(self):
        compiler = self.bin / ("tor-scenario" + self.suffix)
        binary = self.bin / ("tor-arena" + self.suffix)
        matrix = self.directory / "matrix"
        inventory = create_matrix(matrix, compiler, "interactions-v26", ["42", "43"],
                                   families=["strength_heavy"], levels=[4])
        plan = matrix / inventory["cases"][0]["plan"]
        case = plan.parent
        before = {key: files(case / key) for key in ["baseline-forward", "baseline-mirrored",
                                                    "candidate-forward", "candidate-mirrored"]}
        definition = self.directory / "parameters.json"
        definition.write_text(json.dumps({"format": "tor-arena-search-parameters-v1", "parameters": {
            "strength": {"space": {"type": "integer", "low": 0, "high": 7}, "targets": [
                {"file": "scenario.toml", "path": ["characters", 0, "creature", "attributes", "strength"]}]}}}),
            encoding="utf-8")
        values = self.directory / "values.json"
        def apply(value, output):
            values.write_text(json.dumps({"strength": value}), encoding="utf-8")
            return subprocess.run([sys.executable, ROOT / "scripts/arena_search_parameters.py",
                                   "--compiler", compiler, "--plan", plan, "--spec", definition,
                                   "--values", values, "--output", output], capture_output=True,
                                  text=True, encoding="utf-8", timeout=30)
        output = self.directory / "legal"
        applied = apply(4, output)
        self.assertEqual(applied.returncode, 0, applied.stderr)
        for orientation in ["forward", "mirrored"]:
            original = tomllib.loads((case / ("candidate-" + orientation) / "scenario.toml").read_text(encoding="utf-8"))
            actual = tomllib.loads((output / ("candidate-" + orientation) / "scenario.toml").read_text(encoding="utf-8"))
            original["characters"][0]["creature"]["attributes"]["strength"] = 4
            self.assertEqual(actual, original)
            baseline = tomllib.loads((output / ("baseline-" + orientation) / "scenario.toml").read_text(encoding="utf-8"))
            self.assertEqual(baseline["characters"][0]["creature"]["attributes"]["strength"], 5)
        batch_path = self.directory / "batch"
        batch = run_batch(binary, output / "plan.json", batch_path)
        self.assertEqual(len(batch["runs"]), 8)
        self.assertTrue(all(run["status"] == "ok" for run in batch["runs"]))
        for value in ["42", "43"]:
            for orientation in ["forward", "mirrored"]:
                paired = {run["variant"]: run for run in batch["runs"]
                          if run["seed"] == value and run["orientation"] == orientation}
                health = {variant: next(p["initial"]["maximum_health"] for p in run["report"]["participants"]
                                         if p["actor"] == "1") for variant, run in paired.items()}
                self.assertEqual(health["baseline"] - health["candidate"], 1)
        self.assertEqual(replay(binary, batch_path), (True, 8))
        invalid = self.directory / "illegal"
        rejected = apply(6, invalid)
        self.assertEqual(rejected.returncode, 1, rejected.stderr)
        evidence = json.loads((invalid / "candidate.json").read_text(encoding="utf-8"))
        self.assertEqual(evidence["status"], "invalid")
        self.assertEqual({row["package"] for row in evidence["validation_failures"]},
                         {"candidate-forward", "candidate-mirrored"})
        self.assertFalse((invalid / "plan.json").exists())
        collision = apply(4, output)
        self.assertEqual(collision.returncode, 2)
        from unittest.mock import patch
        import arena_search_parameters as parameters
        resumed_output = self.directory / "resumed"
        compile_package = parameters.subprocess.run
        calls = 0
        def interrupted(*args, **kwargs):
            nonlocal calls
            calls += 1
            if calls == 3:
                raise InterruptedError("injected preparation interruption")
            return compile_package(*args, **kwargs)
        spec = json.loads(definition.read_text(encoding="utf-8"))
        with patch.object(parameters.subprocess, "run", side_effect=interrupted), self.assertRaises(InterruptedError):
            parameters.materialize(compiler, plan, spec, {"strength": 4}, resumed_output)
        retained = {key: {p.relative_to(resumed_output / key).as_posix(): (p.read_bytes(), p.stat().st_mtime_ns)
                         for p in (resumed_output / key).rglob("*") if p.is_file()}
                    for key in ("baseline-forward", "baseline-mirrored")}
        values.write_text(json.dumps({"strength": 4}), encoding="utf-8")
        def resume():
            return subprocess.run([sys.executable, ROOT / "scripts/arena_search_parameters.py",
                                   "--compiler", compiler, "--plan", plan, "--spec", definition,
                                   "--values", values, "--output", resumed_output, "--resume"],
                                  capture_output=True, text=True, encoding="utf-8", timeout=30)
        recovered = resume()
        self.assertEqual(recovered.returncode, 0, recovered.stderr)
        self.assertEqual(json.loads((resumed_output / "candidate.json").read_text(encoding="utf-8")),
                         json.loads((output / "candidate.json").read_text(encoding="utf-8")))
        for key, entries in retained.items():
            for filename, state in entries.items():
                path = resumed_output / key / filename
                self.assertEqual((path.read_bytes(), path.stat().st_mtime_ns), state)
        # Recover publication interrupted between candidate.json and plan.json.
        (resumed_output / "plan.json").unlink()
        recovered = resume()
        self.assertEqual(recovered.returncode, 0, recovered.stderr)
        self.assertEqual(json.loads((resumed_output / "plan.json").read_text(encoding="utf-8")),
                         json.loads((output / "plan.json").read_text(encoding="utf-8")))
        validation_record = resumed_output / "candidate.json"
        validation_record.write_text("{}", encoding="utf-8")
        rejected = resume()
        self.assertEqual(rejected.returncode, 2)
        self.assertIn("Published candidate evidence differs", rejected.stderr)
        for key, fingerprint in before.items():
            self.assertEqual(files(case / key), fingerprint)
