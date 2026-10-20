"""Shared imports resolve deterministically and preserve immutable general rules."""
import copy
import io
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch

from ruleset_sources import load_rules, resolve_document, resolve_with_dependencies

ROOT = Path(__file__).resolve().parents[1]


class RulesetSources(unittest.TestCase):
    def test_windows_stdin_preserves_authored_newlines_and_rejects_malformed_toml(self):
        run = subprocess.run

        def windows_pipe(command, **kwargs):
            # Windows' text-mode stdin translates every LF, including the LF
            # already paired with an authored CR. Exercise that transport on
            # every host while retaining the real compiler as the parser.
            if kwargs.pop("text", False):
                encoding = kwargs.pop("encoding", "utf-8")
                buffer = io.BytesIO()
                writer = io.TextIOWrapper(buffer, encoding=encoding, newline="\r\n")
                writer.write(kwargs["input"])
                writer.flush()
                kwargs["input"] = buffer.getvalue()
                result = run(command, **kwargs)
                result.stdout = result.stdout.decode(encoding)
                result.stderr = result.stderr.decode(encoding)
                return result
            return run(command, **kwargs)

        expected = resolve_with_dependencies('import = "interactions-v49"\n')
        with tempfile.TemporaryDirectory() as directory, patch(
            "ruleset_sources.subprocess.run", side_effect=windows_pipe
        ):
            path = Path(directory) / "rules.toml"
            for ending in ("\n", "\r\n"):
                authored = f'import = "interactions-v49"{ending}'
                path.write_bytes(authored.encode("utf-8"))
                for source in (authored, path):
                    with self.subTest(ending=repr(ending), source_type=type(source).__name__):
                        self.assertEqual(resolve_with_dependencies(source), expected)
            for ending in ("\r", "\r\r\n"):
                authored = f'import = "interactions-v49"{ending}'
                path.write_bytes(authored.encode("utf-8"))
                for source in (authored, path):
                    with self.subTest(ending=repr(ending), source_type=type(source).__name__):
                        with self.assertRaisesRegex(ValueError, "carriage return"):
                            resolve_with_dependencies(source)

    def test_every_manifest_resolves_the_shared_current_rules_source(self):
        canonical = load_rules(ROOT / "crates/simulation/data/default-ruleset.toml")
        for path in (ROOT / "scenarios").rglob("scenario.toml"):
            with self.subTest(scenario=path.parent.name):
                manifest = tomllib.loads(path.read_text(encoding="utf-8"))
                self.assertEqual(manifest["ruleset"], "rules.toml")
                source = path.parent / manifest["ruleset"]
                authored = tomllib.loads(source.read_text(encoding="utf-8"))
                self.assertEqual(authored["import"], canonical["id"])
                self.assertLessEqual(set(authored), {"import", "overrides"}, "Scenarios must not duplicate general catalogs")
                self.assertEqual(load_rules(source)["id"], canonical["id"])
                for definitions in manifest.get("creatures", {}).values():
                    for definition in definitions.values():
                        self.assertNotIn("grants", definition)
                        for modifier in definition.get("modifiers", []):
                            self.assertEqual(set(modifier), {"scope", "op"})
                            self.assertIn("kind", modifier["op"])

    def test_import_without_overrides_resolves_the_exact_general_document(self):
        canonical = load_rules(ROOT / "crates/simulation/data/default-ruleset.toml")
        self.assertEqual(resolve_document({"import": canonical["id"]}), canonical)
        self.assertEqual(resolve_document({"import": canonical["id"], "overrides": []}), canonical)

    def test_named_overrides_are_order_independent_of_catalog_storage_and_leave_inputs_unchanged(self):
        canonical = load_rules(ROOT / "crates/simulation/data/default-ruleset.toml")
        source = {"import": canonical["id"], "overrides": [
            {"op": "set", "path": ["resources", "mana", "recovery_period"], "value": 817},
            {"op": "add", "path": ["descriptors", "test_aura"], "value": {"id": "test_aura", "name": "Test Aura"}},
            # Remove an unreferenced leaf, preserving every descriptor used by
            # acquired forms and species. This is a catalog operation test.
            {"op": "remove", "path": ["talents", "hardy_recovery"]},
        ]}
        snapshot = copy.deepcopy(source)
        actual = resolve_document(source)
        self.assertEqual(next(value for value in actual["resources"] if value["resource"] == "mana")["recovery_period"], 817)
        self.assertIn({"id": "test_aura", "name": "Test Aura"}, actual["descriptors"])
        self.assertNotIn("hardy_recovery", {value["id"] for value in actual["talents"]})
        self.assertEqual(source, snapshot)
        self.assertEqual(load_rules(ROOT / "crates/simulation/data/default-ruleset.toml"), canonical)

    def test_strict_operations_reject_unknown_imports_paths_identity_drift_and_overlap(self):
        valid = {"import": "interactions-v49", "overrides": []}
        bad_inputs = [{"import": "../rules.toml"}, {"import": "https://example.test/rules"},
                      {"import": "interactions-v35"}, {"import": "interactions-v38"},
                      {"import": "interactions-v49", "unknown": True}, {"import": "interactions-v49", "overrides": {}}]
        operations = [
            {"op": "set", "path": ["id"], "value": "other"},
            {"op": "set", "path": [], "value": 1},
            {"op": "set", "path": ["resources", 2, "recovery_period"], "value": 1},
            {"op": "set", "path": ["resources", "missing", "recovery_period"], "value": 1},
            {"op": "set", "path": ["talents", "steady_recovery", "modifiers", "0", "op", "value"], "value": 2},
            {"op": "set", "path": ["descriptors", "cold"], "value": {"id": "renamed", "name": "Cold"}},
            {"op": "add", "path": ["descriptors", "cold"], "value": {"id": "cold", "name": "Cold"}},
            {"op": "add", "path": ["descriptors", "new"], "value": {"id": "wrong", "name": "Wrong"}},
            {"op": "remove", "path": ["descriptors", "missing"]},
            {"op": "remove", "path": ["descriptors", "cold"], "value": 0},
            {"op": "set", "path": ["constants", "base_defense"]},
            {"op": "multiply", "path": ["constants", "base_defense"], "value": 2},
            {"op": "set", "path": ["x" * 129], "value": 1},
        ]
        bad_inputs += [{**valid, "overrides": [op]} for op in operations]
        ancestor = {"op": "set", "path": ["resources", "mana"], "value": {"resource": "mana"}}
        child = {"op": "set", "path": ["resources", "mana", "recovery_period"], "value": 900}
        bad_inputs += [{**valid, "overrides": ops} for ops in [[child, child], [ancestor, child], [child, ancestor]]]
        for source in bad_inputs:
            with self.subTest(source=source), self.assertRaises(ValueError):
                resolve_document(source)

    def test_catalog_identity_cannot_be_changed_through_a_nested_field(self):
        for path, value in [(["talents", "warding_initiate", "id"], "renamed_ward"),
                            (["resources", "mana", "resource"], "focus")]:
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, "identity"):
                resolve_document({"import": "interactions-v49", "overrides": [
                    {"op": "set", "path": path, "value": value}]})
        # A resource reference inside an ability's cost is not catalog identity.
        result = resolve_document({"import": "interactions-v49", "overrides": [
            {"op": "set", "path": ["abilities", "spark_lance_fire", "cost", "resource"], "value": "focus"}]})
        self.assertEqual(next(value for value in result["abilities"] if value["id"] == "spark_lance_fire")["cost"]["resource"], "focus")

    def test_dependency_identity_tracks_general_source_and_standalone_snapshots_are_detached(self):
        import hashlib
        from arena_matrix import toml_source
        source = {"import": "interactions-v49"}
        resolved, dependencies = resolve_with_dependencies(source)
        self.assertEqual(dependencies, {"interactions-v49": hashlib.sha256(
            (ROOT / "crates/simulation/data/default-ruleset.toml").read_bytes().replace(b"\r\n", b"\n")).hexdigest()})
        standalone = toml_source(resolved)
        self.assertEqual(resolve_with_dependencies(standalone), (resolved, {}))
        # Python's source path cannot change the compiler's immutable rules bundle.
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "general.toml"
            file.write_text('id = "fake-rules"\n', encoding="utf-8")
            with patch("ruleset_sources.DEFAULT_RULESET", file):
                self.assertEqual(resolve_with_dependencies(source), (resolved, dependencies))
        changed = resolve_document({"import": "interactions-v49", "overrides": [
            {"op": "set", "path": ["talents", "brace_for_impact", "modifiers"], "value": []}]})
        self.assertEqual(next(value for value in changed["talents"] if value["id"] == "brace_for_impact")["modifiers"], [])

    def test_missing_compiler_is_reported_without_build_or_python_fallback(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, "Cannot run rules resolver"):
                resolve_document({"import": "interactions-v49"}, Path(directory) / "missing-compiler")

    def test_oversized_source_is_rejected_before_launching_compiler(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "large.toml"
            file.write_bytes(b"#" * (8 * 1024 * 1024 + 1))
            with self.assertRaisesRegex(ValueError, "source limit"):
                resolve_document(file, Path(directory) / "missing-compiler")
