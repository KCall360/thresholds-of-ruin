"""Validate the complete starter design and replay a bounded representative pilot."""
import json
import subprocess
import sys

from arena_evaluation import files, replay, run_batch
from process_harness import ProcessTestCase, ROOT


class StarterStudyProcesses(ProcessTestCase):
    def test_complete_generator_and_six_class_band_views_use_real_engine_and_exact_replay(self):
        output = self.directory / "study"
        compiler = self.bin / ("tor-scenario" + self.suffix)
        arena = self.bin / ("tor-arena" + self.suffix)
        generated = subprocess.run([sys.executable, ROOT / "scripts/arena_starter_study.py",
                                    "--compiler", compiler, "--output", output, "--seeds", "42",
                                    "--timeout-seconds", "30"], capture_output=True, text=True,
                                   encoding="utf-8", timeout=180)
        self.assertEqual(generated.returncode, 0, generated.stdout + generated.stderr)
        inventory = json.loads((output / "study.json").read_text(encoding="utf-8"))
        self.assertTrue(inventory["complete_design"])
        self.assertTrue(inventory["pilot"])
        self.assertEqual(inventory["battle_evidence"], "not_run")
        self.assertEqual(inventory["calibration"], "not_assessed")
        self.assertEqual(len(inventory["cases"]), 54)
        self.assertEqual(len({case["matchup"] for case in inventory["cases"]}), 27)
        self.assertEqual(inventory["expected_runs"], 216)
        self.assertEqual(inventory["validated_packages"], 216)
        self.assertEqual(inventory["generation_status"], "validated")
        self.assertFalse((output / "partial.json").exists())
        selected = [case for case in inventory["cases"]
                    if case["warrior"] == "warrior_talent_heavy" and case["mage"] == "mage_talent_heavy"]
        self.assertEqual({(case["hit_dice"], case["evaluated_class"]) for case in selected},
                         {(hd, evaluated) for hd in (1, 6, 11) for evaluated in ("warrior", "mage")})
        for case in inventory["cases"]:
            for package, fingerprint in case["package_inputs"].items():
                self.assertEqual(files(output / case["id"] / package), fingerprint)
        for case in selected:
            with self.subTest(case=case["id"]):
                retained = self.directory / "batches" / case["id"]
                batch = run_batch(arena, output / case["plan"], retained)
                self.assertEqual(len(batch["runs"]), 4)
                self.assertTrue(all(run["status"] == "ok" for run in batch["runs"]), batch["summary"])
                for run in batch["runs"]:
                    for participant in run["report"]["participants"]:
                        self.assertEqual(participant["hit_dice"], case["hit_dice"])
                        self.assertEqual(int(participant["initial"]["injury"])
                                         + int(participant["damage_received"])
                                         - int(participant["healing_received"])
                                         - int(participant["final_state"]["injury"]),
                                         int(participant["health_recovered"]))
                self.assertEqual(replay(arena, retained), (True, 4))
        # This test executes six comparison views at one seed, not the full 32-seed study.
