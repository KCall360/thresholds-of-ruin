"""Every test scenario package is used by name by at least one test.

The package invariants play every package, so an unused one would still pass
there; this check finds packages nothing else tests. Tests name packages in
full (not built from fragments) so that a search finds every user.
"""
from pathlib import Path
import re
import hashlib
import json
import unittest

ROOT = Path(__file__).resolve().parents[1]


def test_sources():
    yield from (ROOT / "scripts").glob("test_*.py")
    yield from (ROOT / "crates").glob("*/tests/**/*.rs")
    yield from (ROOT / "crates").glob("*/src/**/*.rs")


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
        text = "\n".join(path.read_text(encoding="utf-8") for path in test_sources()
                         if path.name != "test_scenario_references.py")
        unused = [name for name in packages if not re.search(r"(?<![\w-])" + re.escape(name) + r"(?![\w-])", text)]
        self.assertEqual(unused, [], "test packages no test names")

    def test_test_package_names_are_lowercase_words_joined_by_hyphens(self):
        for package in (ROOT / "scenarios/tests").iterdir():
            self.assertRegex(package.name, r"^[a-z0-9]+(-[a-z0-9]+)*$")
            self.assertNotRegex(package.name, r"-setup$", "name a package for what it sets up")


if __name__ == "__main__":
    unittest.main()
