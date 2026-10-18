"""The authored model showcase uses the same asset channel as gameplay."""
from pathlib import Path
import hashlib
import json
import tomllib
import unittest

CLIENT = Path(__file__).resolve().parents[1]
PACKAGE = CLIENT / "scenarios/menagerie"


def document(relative):
    return tomllib.loads((PACKAGE / relative).read_text(encoding="utf-8"))


class ShowcaseAssets(unittest.TestCase):
    def test_certificate_matches_normalized_source_bytes(self):
        certificate = json.loads((PACKAGE / "validation.json").read_text(encoding="utf-8"))
        for relative, expected in certificate["files"].items():
            with self.subTest(file=relative):
                source = (PACKAGE / relative).read_bytes().replace(b"\r\n", b"\n")
                self.assertEqual(hashlib.sha256(source).hexdigest(), expected,
                                 "regenerate the showcase certificate after source edits")

    def test_showcase_models_are_explicitly_disclosed_and_forecast(self):
        manifest = document("scenario.toml")
        actors = document("catalog/actors.toml")["actors"]
        items = document("catalog/items.toml")["items"]
        entries = list(actors.values()) + list(items.values()) + manifest["characters"]
        available = set()
        for theme in manifest.get("themes", []):
            available.update(manifest.get("assets", {}).get(theme, []))
        for entry in entries:
            with self.subTest(entry=entry.get("creature", {}).get("name", entry.get("name", entry.get("id")))):
                self.assertIn("asset", entry, "the showcase cannot select its anatomy from an English name")
                self.assertIn(entry["asset"], available, "the palette must forecast each disclosed model")


if __name__ == "__main__":
    unittest.main()
