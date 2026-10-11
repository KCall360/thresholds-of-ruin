"""Bundled scenarios share the canonical current rules content."""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]


class RulesetSources(unittest.TestCase):
    def test_every_manifest_resolves_its_bundled_current_rules_source(self):
        canonical = (ROOT / "crates/simulation/data/default-ruleset.toml").read_bytes()
        for path in (ROOT / "scenarios").rglob("scenario.toml"):
            with self.subTest(scenario=path.parent.name):
                manifest = tomllib.loads(path.read_text(encoding="utf-8"))
                self.assertEqual(manifest["ruleset"], "rules.toml")
                self.assertEqual((path.parent / manifest["ruleset"]).read_bytes(), canonical)
                for definitions in manifest.get("creatures", {}).values():
                    for definition in definitions.values():
                        self.assertNotIn("grants", definition)
                        for modifier in definition.get("modifiers", []):
                            self.assertEqual(set(modifier), {"scope", "op"})
                            self.assertIn("kind", modifier["op"])
