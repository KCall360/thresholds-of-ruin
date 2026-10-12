"""Every test scenario package has an explicit loader reference in test code.

The package invariants play every package, so an unused one would still pass
there; this check finds packages with no other named loader use. It does not
prove gameplay coverage or meaningful assertions; those require behavior tests.
Tests name packages in full (not built from fragments) so users are auditable.
"""
from pathlib import Path
import re
import ast
import tempfile
from unittest.mock import patch
import hashlib
import json
import unittest

ROOT = Path(__file__).resolve().parents[1]


def test_sources():
    yield from (ROOT / "scripts").glob("test_*.py")
    yield from (ROOT / "crates").glob("*/tests/**/*.rs")


def scenario_uses(path):
    """Recognize loader arguments, never comments or arbitrary name mentions.

    Python's test-local loop bindings support table-driven scenario tests. Rust
    integration tests use support::load/package or explicit scenario paths.
    Production sources and generic all-package iteration are deliberately absent.
    """
    source = path.read_text(encoding="utf-8")
    if path.suffix == ".py":
        tree = ast.parse(source)
        found = set()

        def literals(node, bindings):
            if isinstance(node, ast.Constant) and isinstance(node.value, str):
                return {node.value}
            if isinstance(node, ast.Name):
                return bindings.get(node.id, set())
            if isinstance(node, (ast.List, ast.Tuple, ast.Set)):
                return set().union(*(literals(item, bindings) for item in node.elts))
            return set()

        def visit(node, bindings):
            bindings = dict(bindings)
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                bindings = {}
            if isinstance(node, ast.For):
                if isinstance(node.target, ast.Name):
                    bindings[node.target.id] = literals(node.iter, bindings)
                elif isinstance(node.target, (ast.Tuple, ast.List)) and isinstance(node.iter, (ast.Tuple, ast.List)):
                    for index, target in enumerate(node.target.elts):
                        if isinstance(target, ast.Name):
                            bindings[target.id] = set().union(*(literals(row.elts[index], bindings)
                                for row in node.iter.elts if isinstance(row, (ast.Tuple, ast.List))
                                and index < len(row.elts)))
            if isinstance(node, ast.Call):
                for keyword in node.keywords:
                    if (keyword.arg == "scenario" and isinstance(node.func, ast.Attribute)
                            and node.func.attr == "server"):
                        found.update(literals(keyword.value, bindings))
                # Process-test start/travel helpers forward their first package argument
                # to server(scenario=...). Keep this small vocabulary explicit.
                if isinstance(node.func, ast.Attribute) and node.func.attr in {"start", "travel_scenario"} and node.args:
                    found.update(literals(node.args[0], bindings))
            for child in ast.iter_child_nodes(node):
                visit(child, bindings)

        visit(tree, {})
        return found
    # Tokenize enough Rust to distinguish actual calls from loader examples in
    # strings/raw strings and nested comments. No source execution is involved.
    tokens = []
    raw_string = re.compile(r'r(#+)?"')
    quoted_string = re.compile(r'"((?:\\.|[^"\\])*)"')
    word_token = re.compile(r'[A-Za-z_][A-Za-z_0-9]*')
    index = 0
    while index < len(source):
        if source.startswith("//", index):
            end = source.find("\n", index)
            index = len(source) if end < 0 else end + 1
        elif source.startswith("/*", index):
            depth = 1
            index += 2
            while index < len(source) and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
        elif raw := raw_string.match(source, index):
            start = raw.end()
            endmark = '"' + (raw.group(1) or "")
            end = source.find(endmark, start)
            if end < 0:
                break
            tokens.append(("string", source[start:end]))
            index = end + len(endmark)
        elif source[index] == '"':
            string = quoted_string.match(source, index)
            if not string:
                break
            tokens.append(("string", string.group(1)))
            index = string.end()
        elif word := word_token.match(source, index):
            tokens.append(("word", word.group()))
            index = word.end()
        else:
            if not source[index].isspace():
                tokens.append(("punctuation", source[index]))
            index += 1
    found = set()
    for first, second, third in zip(tokens, tokens[1:], tokens[2:]):
        if first[0] != "word" or second != ("punctuation", "(") or third[0] != "string":
            continue
        if first[1] in ("package", "load") and re.fullmatch(r"[a-z0-9-]+", third[1]):
            found.add(third[1])
        if first[1] == "join":
            match = re.fullmatch(r"(?:\.\./)*scenarios/tests/([a-z0-9-]+)", third[1])
            if match:
                found.add(match.group(1))
    return found



class ScenarioReferences(unittest.TestCase):
    def test_certificates_match_repository_lf_source_bytes(self):
        mismatches = []
        for certificate_path in sorted((ROOT / "scenarios").rglob("validation.json")):
            package = certificate_path.parent
            certificate = json.loads(certificate_path.read_text(encoding="utf-8"))
            indexed = json.loads((package / "index.json").read_text(encoding="utf-8"))
            expected = dict(certificate["files"])
            expected.update((region["file"], region["hash"]) for region in indexed["regions"])
            for name, digest in expected.items():
                source = (package / name).read_bytes()
                # .gitattributes requires LF. Verify the bytes a fresh checkout
                # receives, even when a local editor accidentally wrote CRLF.
                checkout = source.replace(b"\r\n", b"\n")
                if hashlib.sha256(checkout).hexdigest() != digest:
                    mismatches.append(str((package / name).relative_to(ROOT)))
        self.assertEqual(mismatches, [],
                         "Regenerate certificates from repository LF bytes; local CRLF hashes cannot survive checkout")

    def test_every_test_package_is_named_by_a_test(self):
        packages = sorted(p.name for p in (ROOT / "scenarios/tests").iterdir() if (p / "scenario.toml").exists())
        self.assertTrue(packages)
        used = set().union(*(scenario_uses(path) for path in test_sources()
                             if path.name != "test_scenario_references.py"))
        unused = sorted(set(packages) - used)
        self.assertEqual(unused, [], "test packages with no explicit test loader reference")

    def test_test_package_names_are_lowercase_words_joined_by_hyphens(self):
        for package in (ROOT / "scenarios/tests").iterdir():
            self.assertRegex(package.name, r"^[a-z0-9]+(-[a-z0-9]+)*$")
            self.assertNotRegex(package.name, r"-setup$", "name a package for what it sets up")



class ReferenceGuardRegressions(unittest.TestCase):
    def check_source(self, path, source, expected):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package = root / "scenarios/tests/unused-example"
            package.mkdir(parents=True)
            (package / "scenario.toml").touch()
            target = root / path
            target.parent.mkdir(parents=True)
            target.write_text(source, encoding="utf-8")
            with patch.dict(globals(), ROOT=root):
                check = ScenarioReferences("test_every_test_package_is_named_by_a_test")
                if expected:
                    check.test_every_test_package_is_named_by_a_test()
                else:
                    with self.assertRaises(AssertionError):
                        check.test_every_test_package_is_named_by_a_test()

    def test_comment_docstring_unrelated_literal_and_production_mentions_do_not_count(self):
        for path, source in [
                ("scripts/test_unused.py", "# unused-example was removed\n"),
                ("scripts/test_unused.py", '\"\"\"unused-example\"\"\"\n'),
                ("scripts/test_unused.py", "label = 'unused-example'\n"),
                ("scripts/test_unused.py", "self.subTest(scenario='unused-example')\n"),
                ("scripts/test_unused.py", "unrelated(scenario='unused-example')\n"),
                ("crates/server/tests/it/unused.rs", "// support::package(\"unused-example\");\n"),
                ("crates/server/tests/it/unused.rs", '/* nested /* inner */ support::package("unused-example"); */'),
                ("crates/server/tests/it/unused.rs", 'let example = r#"support::package("unused-example");"#;'),
                ("crates/server/src/unused.rs", 'support::package("unused-example");\n'),
                ("scripts/test_unused.py", "self.server(scenario='unused-example-extra')\n")]:
            with self.subTest(path=path, source=source):
                self.check_source(path, source, False)

    def test_explicit_loader_uses_and_parameterized_scenarios_count(self):
        for path, source in [
                ("scripts/test_use.py", "self.server(scenario='unused-example')\n"),
                ("scripts/test_use.py", "self.start('unused-example')\n"),
                ("scripts/test_use.py", "for package in ['unused-example']:\n    self.server(scenario=package)\n"),
                ("crates/server/tests/it/use.rs", 'support::package("unused-example");\n'),
                ("crates/server/tests/it/use.rs", 'Path::new("root").join("../../scenarios/tests/unused-example");\n')]:
            with self.subTest(path=path, source=source):
                self.check_source(path, source, True)

if __name__ == "__main__":
    unittest.main()
