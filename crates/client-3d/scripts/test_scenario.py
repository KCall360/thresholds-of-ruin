"""The authored model showcase and the crowd benchmark use the same asset
channel as gameplay."""
from pathlib import Path
import hashlib
import json
import tomllib
import unittest

CLIENT = Path(__file__).resolve().parents[1]
PACKAGES = [CLIENT / "scenarios/menagerie", CLIENT / "scenarios/crowd"]
CROWD = CLIENT / "scenarios/crowd"


def document(package, relative):
    return tomllib.loads((package / relative).read_text(encoding="utf-8"))


class ShowcaseAssets(unittest.TestCase):
    def test_certificate_matches_normalized_source_bytes(self):
        for package in PACKAGES:
            certificate = json.loads((package / "validation.json").read_text(encoding="utf-8"))
            for relative, expected in certificate["files"].items():
                with self.subTest(package=package.name, file=relative):
                    source = (package / relative).read_bytes().replace(b"\r\n", b"\n")
                    self.assertEqual(hashlib.sha256(source).hexdigest(), expected,
                                     "regenerate the package certificate after source edits")

    def test_showcase_models_are_explicitly_disclosed_and_forecast(self):
        for package in PACKAGES:
            manifest = document(package, "scenario.toml")
            actors = document(package, "catalog/actors.toml")["actors"]
            items = document(package, "catalog/items.toml")["items"]
            entries = list(actors.values()) + list(items.values()) + manifest["characters"]
            available = set()
            for theme in manifest.get("themes", []):
                available.update(manifest.get("assets", {}).get(theme, []))
            for entry in entries:
                name = entry.get("creature", {}).get("name", entry.get("name", entry.get("id")))
                with self.subTest(package=package.name, entry=name):
                    self.assertIn("asset", entry, "a package cannot select its anatomy from an English name")
                    self.assertIn(entry["asset"], available, "the palette must forecast each disclosed model")

    def test_crowd_fills_one_chamber_with_twenty_four_wandering_creatures(self):
        region = document(CROWD, "regions/1.toml")
        actors = document(CROWD, "catalog/actors.toml")["actors"]
        width, depth, _ = region["size"]
        placed = region["actors"]
        self.assertEqual(len(placed), 24, "with the character, the benchmark draws 25 creatures")
        cells = {tuple(actor["at"]) for actor in placed}
        self.assertEqual(len(cells), len(placed), "each creature stands on its own cell")
        self.assertNotIn(tuple(region["anchors"]["start"]), cells)
        for actor in placed:
            with self.subTest(actor=actor["id"]):
                x, y, z = actor["at"]
                self.assertTrue(0 < x < width - 1 and 0 < y < depth - 1 and z == 0,
                                "inside the chamber's walls, on its floor")
                self.assertEqual((actor["controller"], actor["ai"]), ("ai", "wander"))
                self.assertIn(actor["archetype"], actors)


if __name__ == "__main__":
    unittest.main()
