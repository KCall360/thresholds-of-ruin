"""Candidate parameters preserve encounter identity and shared-rule validation."""
import copy
import unittest
from arena_search_parameters import validate_spec, apply_document


def specification(path=None):
    return {"format": "tor-arena-search-parameters-v1", "parameters": {
        "strength": {"space": {"type": "integer", "low": 0, "high": 7},
                     "targets": [{"file": "scenario.toml", "path": path or
                                  ["characters", 0, "creature", "attributes", "strength"]}]}}}


class SearchParameterTests(unittest.TestCase):
    def test_scalar_patch_is_immutable_and_same_assignment_applies_to_both_orientations(self):
        spec, space = validate_spec(specification())
        original = {"characters": [{"anchor": "1/start", "creature": {"name": "subject", "faction": "blue",
                    "attributes": {"strength": 5}}}], "arena": {"actions": 10000}}
        snapshot = copy.deepcopy(original)
        patched = apply_document(original, "scenario.toml", spec, {"strength": 4})
        self.assertEqual(patched["characters"][0]["creature"]["attributes"]["strength"], 4)
        self.assertEqual(patched["characters"][0]["anchor"], "1/start")
        self.assertEqual(patched["arena"], original["arena"])
        self.assertEqual(original, snapshot)
        self.assertEqual(space["strength"]["high"], 7)

    def test_geometry_identity_caps_path_traversal_and_conflicting_targets_are_rejected(self):
        for path in [["arena", "actions"], ["factions", "blue", 0],
                     ["characters", 0, "anchor"], ["characters", 0, "creature", "faction"],
                     ["characters", 0, "body", "mass"]]:
            with self.subTest(path=path), self.assertRaises(ValueError):
                validate_spec(specification(path))
        bad = specification()
        bad["parameters"]["strength"]["targets"][0]["file"] = "../outside.toml"
        with self.assertRaises(ValueError):
            validate_spec(bad)
        duplicate = specification()
        duplicate["parameters"]["another"] = copy.deepcopy(duplicate["parameters"]["strength"])
        with self.assertRaises(ValueError):
            validate_spec(duplicate)

    def test_missing_leaf_wrong_scalar_type_and_outside_distribution_do_not_mutate(self):
        spec, _ = validate_spec(specification())
        original = {"characters": [{"creature": {"attributes": {"strength": 5}}}]}
        for values in [{"strength": 8}, {"strength": True}, {"wrong": 4}]:
            with self.assertRaises(ValueError):
                apply_document(original, "scenario.toml", spec, values)
        with self.assertRaises(ValueError):
            apply_document({}, "scenario.toml", spec, {"strength": 4})
        self.assertEqual(original["characters"][0]["creature"]["attributes"]["strength"], 5)


class MaterializationRecoveryTests(unittest.TestCase):
    def test_resume_reuses_validated_packages_and_rejects_input_or_result_drift(self):
        import json
        from pathlib import Path
        import subprocess
        import tempfile
        from unittest.mock import patch
        import arena_search_parameters as parameters
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            compiler = root / "compiler"
            compiler.write_bytes(b"fixed compiler")
            inputs = {}
            for orientation in ("forward", "mirrored"):
                source = root / orientation
                source.mkdir()
                (source / "scenario.toml").write_text(
                    f"# {orientation}\n[[characters]]\n[characters.creature.attributes]\nstrength = 5\n", encoding="utf-8")
                inputs[orientation] = str(source)
            plan = root / "plan.json"
            plan.write_text(json.dumps({"baseline": inputs, "candidate": inputs,
                                        "seeds": ["42"], "faction": "blue"}), encoding="utf-8")
            output = root / "candidate"
            valid = subprocess.CompletedProcess([], 0, "valid", "")
            with patch.object(parameters.subprocess, "run", side_effect=[valid, valid, InterruptedError("stop")]):
                with self.assertRaises(InterruptedError):
                    parameters.materialize(compiler, plan, specification(), {"strength": 4}, output)
            completed = output / "baseline-forward/scenario.toml"
            retained = completed.read_bytes(), completed.stat().st_mtime_ns
            completed.write_bytes(b"altered")
            with patch.object(parameters.subprocess, "run") as execute, self.assertRaisesRegex(ValueError, "validated"):
                parameters.materialize(compiler, plan, specification(), {"strength": 4}, output, resume=True)
            execute.assert_not_called()
            completed.write_bytes(retained[0])
            retained = completed.read_bytes(), completed.stat().st_mtime_ns
            with patch.object(parameters.subprocess, "run") as execute, self.assertRaisesRegex(ValueError, "inputs"):
                parameters.materialize(compiler, plan, specification(), {"strength": 3}, output, resume=True)
            execute.assert_not_called()
            source_manifest = Path(inputs["forward"]) / "scenario.toml"
            source_original = source_manifest.read_bytes()
            source_manifest.write_bytes(b"changed source")
            with patch.object(parameters.subprocess, "run") as execute, self.assertRaisesRegex(ValueError, "inputs"):
                parameters.materialize(compiler, plan, specification(), {"strength": 4}, output, resume=True)
            execute.assert_not_called()
            source_manifest.write_bytes(source_original)
            compiler.write_bytes(b"different compiler")
            with patch.object(parameters.subprocess, "run") as execute, self.assertRaisesRegex(ValueError, "inputs"):
                parameters.materialize(compiler, plan, specification(), {"strength": 4}, output, resume=True)
            execute.assert_not_called()
            compiler.write_bytes(b"fixed compiler")
            (output / ".materializing").write_bytes(b"truncated staging data")
            failed = subprocess.CompletedProcess([], 2, "illegal creature build", "")
            with patch.object(parameters.subprocess, "run", side_effect=[failed, valid]) as execute:
                result = parameters.materialize(compiler, plan, specification(), {"strength": 4}, output, resume=True)
            self.assertEqual(execute.call_count, 2)
            self.assertEqual(result["status"], "invalid")
            self.assertFalse((output / "plan.json").exists())
            self.assertEqual(result["validation_failures"], [{"package": "candidate-forward", "variant": "candidate",
                                                             "message": "illegal creature build"}])
            self.assertEqual((completed.read_bytes(), completed.stat().st_mtime_ns), retained)
            with patch.object(parameters.subprocess, "run") as execute:
                self.assertEqual(parameters.materialize(compiler, plan, specification(), {"strength": 4}, output, resume=True), result)
            execute.assert_not_called()
