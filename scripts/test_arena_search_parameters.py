"""Candidate parameters preserve encounter identity and shared-rule validation."""
import copy
import unittest
from arena_search_parameters import validate_spec, apply_document


def specification(path=None):
    return {"format": "tor-arena-search-parameters-v2", "parameters": {
        "strength": {"space": {"type": "integer", "low": 0, "high": 7},
                     "targets": [{"file": "scenario.toml", "path": path or
                                  ["characters", 1, "creature", "attributes", "strength"]}]}}}


class SearchParameterTests(unittest.TestCase):
    def test_positional_v1_format_is_rejected(self):
        old = specification()
        old["format"] = "tor-arena-search-parameters-v1"
        with self.assertRaisesRegex(ValueError, "format"):
            validate_spec(old)

    def test_scalar_patch_is_immutable_and_same_assignment_applies_to_both_orientations(self):
        spec, space = validate_spec(specification())
        original = {"characters": [{"id": 1, "anchor": "1/start", "creature": {"name": "subject", "faction": "blue",
                    "attributes": {"strength": 5}}}], "arena": {"actions": 10000}}
        snapshot = copy.deepcopy(original)
        patched = apply_document(original, "scenario.toml", spec, {"strength": 4})
        self.assertEqual(patched["characters"][0]["creature"]["attributes"]["strength"], 4)
        self.assertEqual(patched["characters"][0]["anchor"], "1/start")
        self.assertEqual(patched["arena"], original["arena"])
        self.assertEqual(original, snapshot)
        self.assertEqual(space["strength"]["high"], 7)

    def test_ruleset_numeric_parameters_preserve_ids_and_reject_structure_edits(self):
        document = {"id": "test", "resources": [{"resource": "mana", "recovery_period": 1000}]}
        spec = specification(["resources", "mana", "recovery_period"])
        spec["parameters"]["strength"]["targets"][0]["file"] = "rules.toml"
        spec["parameters"]["strength"]["space"] = {"type": "integer", "low": 600, "high": 1000}
        patched = apply_document(document, "rules.toml", spec, {"strength": 600})
        self.assertEqual(patched["resources"][0]["recovery_period"], 600)
        self.assertEqual(patched["id"], "test")
        self.assertEqual(document["resources"][0]["recovery_period"], 1000)
        for path in [["id"], ["abilities", 0, "id"], ["abilities", 0, "check", "actor"],
                     ["talents", 0, "modifiers", 0, "op", "kind"],
                     ["resources", 0, "resource"]]:
            spec["parameters"]["strength"]["targets"][0]["path"] = path
            with self.subTest(path=path), self.assertRaises(ValueError):
                validate_spec(spec)

    def test_health_recovery_curve_is_a_scalar_tuning_surface(self):
        original = {"health_recovery": {"base_period": 1200, "reduction_per_hd": 40,
                                        "minimum_period": 200}}
        for field in original["health_recovery"]:
            spec = specification(["health_recovery", field])
            parameter = spec["parameters"]["strength"]
            parameter["targets"][0]["file"] = "rules.toml"
            parameter["space"] = {"type": "integer", "low": 0, "high": 2000}
            validate_spec(spec)
            patched = apply_document(original, "rules.toml", spec, {"strength": 17})
            self.assertEqual(patched["health_recovery"][field], 17)
            self.assertNotEqual(original["health_recovery"][field], 17)
        for path in [["health_recovery"], ["health_recovery", "enabled"],
                     ["health_recovery", "base_period", "extra"]]:
            parameter["targets"][0]["path"] = path
            with self.subTest(path=path), self.assertRaises(ValueError):
                validate_spec(spec)

    def test_capacity_cost_and_every_numeric_talent_family_are_scalar_parameters(self):
        from pathlib import Path
        import tomllib
        original = tomllib.loads((Path(__file__).resolve().parents[1]
                                 / "crates/simulation/data/default-ruleset.toml").read_text(encoding="utf-8"))
        snapshot = copy.deepcopy(original)
        paths = [["resources", resource["resource"], "capacity", "base"] for resource in original["resources"]]
        paths += [["abilities", ability["id"], "cost", phase]
                  for index, ability in enumerate(original["abilities"]) if "cost" in ability
                  for phase in ["start", "resolution"]]
        paths += [["talents", talent["id"], "modifiers", modifier, "op", field]
                  for index, talent in enumerate(original["talents"])
                  for modifier, value in enumerate(talent["modifiers"])
                  for field in ["value", "amount"] if field in value["op"]]
        for path in paths:
            with self.subTest(path=path):
                value = original
                for depth, key in enumerate(path):
                    value = (next(item for item in value if item.get("id", item.get("resource")) == key)
                             if depth == 1 else value[key])
                spec = specification(path)
                spec["parameters"]["strength"]["targets"][0]["file"] = "rules.toml"
                spec["parameters"]["strength"]["space"] = {"type": "integer", "low": value, "high": value + 1}
                actual = apply_document(original, "rules.toml", spec, {"strength": value + 1})
                expected = copy.deepcopy(original)
                parent = expected
                for depth, key in enumerate(path[:-1]):
                    parent = (next(item for item in parent if item.get("id", item.get("resource")) == key)
                              if depth == 1 else parent[key])
                parent[path[-1]] = value + 1
                self.assertEqual(actual, expected)
        self.assertEqual(original, snapshot)

    def test_geometry_identity_caps_path_traversal_and_conflicting_targets_are_rejected(self):
        for path in [["arena", "actions"], ["factions", "blue", 0],
                     ["characters", 1, "anchor"], ["characters", 1, "creature", "faction"],
                     ["characters", 1, "body", "mass"]]:
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
        original = {"characters": [{"id": 1, "creature": {"attributes": {"strength": 5}}}]}
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
                    f"# {orientation}\n[[characters]]\nid = 1\n[characters.creature.attributes]\nstrength = 5\n", encoding="utf-8")
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

class StableIdentityTests(unittest.TestCase):
    def test_catalog_and_actor_edits_follow_identity_after_reordering(self):
        for file, collection, identity, field in [("rules.toml", "resources", "mana", "resource"),
                                                  ("scenario.toml", "characters", 17001, "id"),
                                                  ("regions/1.toml", "actors", 17, "id")]:
            path = [collection, identity] + (["recovery_period"] if file == "rules.toml" else
                                           ["creature", "attributes", "strength"])
            spec = specification(path)
            spec["parameters"]["strength"]["targets"][0]["file"] = file
            member = {field: identity, "recovery_period": 5, "creature": {"attributes": {"strength": 5}}}
            other = {**copy.deepcopy(member), field: "health" if file == "rules.toml" else 9}
            for members in [[member, other], [other, member]]:
                actual = apply_document({collection: members}, file, spec, {"strength": 4})
                changed = next(value for value in actual[collection] if value[field] == identity)
                self.assertEqual(changed["recovery_period"] if file == "rules.toml" else
                                 changed["creature"]["attributes"]["strength"], 4)

    def test_catalog_indices_and_missing_or_duplicate_identities_are_rejected(self):
        spec = specification(["resources", 0, "recovery_period"])
        spec["parameters"]["strength"]["targets"][0]["file"] = "rules.toml"
        with self.assertRaises(ValueError):
            validate_spec(spec)
        spec = specification(["characters", 17, "creature", "attributes", "strength"])
        member = {"id": 17, "creature": {"attributes": {"strength": 5}}}
        for members in [[{**member, "id": 9}], [member, copy.deepcopy(member)]]:
            with self.subTest(members=members), self.assertRaises(ValueError):
                apply_document({"characters": members}, "scenario.toml", spec, {"strength": 4})
