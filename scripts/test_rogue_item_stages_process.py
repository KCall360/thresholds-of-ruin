"""The real compiler accepts Rogue's chance and drought stages and rejects bad ones."""
import shutil
import subprocess
import tomllib
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT


class RogueItemStageProcesses(ProcessTestCase):
    def check(self, edit):
        package = self.directory / "rogue-stages"
        shutil.copytree(ROOT / "scenarios/rogue", package, dirs_exist_ok=True)
        manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
        stages = {stage["id"]: stage for stage in
                  manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]}
        edit(stages)
        (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        compiler = self.bin / ("tor-scenario" + self.suffix)
        return subprocess.run([str(compiler), "check", str(package)], capture_output=True,
                              text=True, encoding="utf-8", timeout=120)

    def test_rogue_stages_compile_and_invalid_variants_reject(self):
        accepted = self.check(lambda stages: None)
        self.assertEqual(accepted.returncode, 0, accepted.stderr)

        def no_attempts(stages):
            stages["rogue.items"]["attempts"] = 0

        def impossible_chance(stages):
            stages["rogue.items"]["chance"] = {"numerator": 2, "denominator": 1}

        def no_food(stages):
            stages["rogue.food_drought"]["food"] = []

        def duplicate_food(stages):
            stages["rogue.food_drought"]["food"] = ["rogue.ration", "rogue.ration"]

        def unknown_food(stages):
            stages["rogue.food_drought"]["food"] = ["rogue.cake"]

        def unknown_field(stages):
            stages["rogue.food_drought"]["count"] = [1, 1]

        for edit in (no_attempts, impossible_chance, no_food, duplicate_food, unknown_food,
                     unknown_field):
            with self.subTest(edit=edit.__name__):
                self.assertNotEqual(self.check(edit).returncode, 0)

    def test_grouped_missiles_use_their_own_quantity(self):
        def single_arrows(stages):
            for row in stages["rogue.items_other"]["bands"][0]["choices"]:
                row.pop("quantity", None)
        self.assertEqual(self.check(single_arrows).returncode, 0)

        def bad_quantity(stages):
            for row in stages["rogue.items_other"]["bands"][0]["choices"]:
                if row["archetype"] == "rogue.arrow":
                    row["quantity"] = {"kind": "uniform", "minimum": 9, "maximum": 2}
        self.assertNotEqual(self.check(bad_quantity).returncode, 0)


if __name__ == "__main__":
    unittest.main()
