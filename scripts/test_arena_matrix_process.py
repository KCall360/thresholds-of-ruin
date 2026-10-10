"""Generate real packages and evaluate/replay the complete curated matrix."""
import json
import subprocess
import sys
from process_harness import ProcessTestCase, ROOT
from arena_evaluation import run_batch, replay


class ArenaMatrixProcesses(ProcessTestCase):
    def test_compiler_cli_then_normal_engine_and_replay_for_every_curated_case(self):
        compiler = self.bin / ("tor-scenario" + self.suffix)
        binary = self.bin / ("tor-arena" + self.suffix)
        matrix = self.directory / "matrix"
        result = subprocess.run([sys.executable, ROOT / "scripts/arena_matrix.py",
                                 "--compiler", compiler, "--output", matrix,
                                 "--seeds", "42"], capture_output=True, text=True,
                                encoding="utf-8", timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        inventory = json.loads((matrix / "matrix.json").read_text(encoding="utf-8"))
        self.assertEqual(len(inventory["cases"]), 67)
        self.assertEqual(len(inventory["exclusions"]), 3)
        self.assertFalse((matrix / "partial.json").exists())
        levels = set()
        families = set()
        reasons = set()
        damage = 0
        abilities = set()
        for case in inventory["cases"]:
            with self.subTest(case=case["id"]):
                levels.add(case["hit_dice"])
                families.add(case["family"])
                batch_path = self.directory / "batches" / case["id"]
                batch = run_batch(binary, matrix / case["plan"], batch_path)
                self.assertEqual(len(batch["runs"]), 4)
                self.assertTrue(all(run["status"] == "ok" for run in batch["runs"]), case["id"])
                for run in batch["runs"]:
                    report = run["report"]
                    reasons.add(report["termination"]["type"])
                    participants = report["participants"]
                    self.assertEqual({p["hit_dice"] for p in participants}, {case["hit_dice"]})
                    self.assertEqual({p["faction"] for p in participants}, {"blue", "red"})
                    for participant in participants:
                        damage += int(participant["damage_dealt"])
                        abilities.update(participant["abilities"])
                        self.assertTrue(all(resource["reserved"] == 0
                                            for resource in participant["final_state"]["resources"]))
                equal, count = replay(binary, batch_path)
                self.assertTrue(equal, case["id"])
                self.assertEqual(count, 4)
        self.assertEqual(levels, {1, 2, 4, 8, 16})
        self.assertEqual(len(families), 14)
        self.assertGreater(damage, 0)
        self.assertTrue({"power_strike", "magic_bolt", "fear"}.issubset(abilities))
        self.assertIn("elimination", reasons)
        self.assertTrue({"stalemate", "tick_limit", "action_limit"}.intersection(reasons))
