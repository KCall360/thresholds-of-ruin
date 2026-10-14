"""Generate real packages and evaluate/replay the curated matrix.

Every run compiles and checks the complete inventory. Release runs (every CI
run) play and replay all cases; debug runs play a stratified sample covering
every family and band (see `plays_complete_matrix`)."""
from process_harness import ProcessTestCase
from arena_evaluation import run_batch, replay
from arena_matrix import FAMILIES, HD_LEVELS
from arena_matrix_process_harness import (assert_matrix_ability_coverage, compile_matrix_families,
                                          complete_matrix_cases, plays_complete_matrix, stratified_cases)


class ArenaMatrixProcesses(ProcessTestCase):
    def test_compiler_cli_then_normal_engine_and_replay_for_every_curated_case(self):
        compiler = self.bin / ("tor-scenario" + self.suffix)
        binary = self.bin / ("tor-arena" + self.suffix)
        matrices = compile_matrix_families(compiler, self.directory / "matrix")
        cases = complete_matrix_cases(matrices)
        expected = {(family, hd) for family in FAMILIES for hd in HD_LEVELS
                    if not (hd == 1 and family in ("mixed", "warrior_mage_dip", "mage_warrior_dip"))}
        self.assertEqual({(case["family"], case["hit_dice"]) for _, case in cases}, expected)
        self.assertEqual(len(cases), len(expected))
        self.assertEqual(sum(len(inventory["exclusions"]) for _, inventory in matrices), 3)
        levels = set()
        families = set()
        reasons = set()
        damage = 0
        participant_abilities = []
        played = cases if plays_complete_matrix() else stratified_cases(cases)
        for matrix, case in played:
            with self.subTest(case=case["id"]):
                levels.add(case["hit_dice"])
                families.add(case["family"])
                batch_path = self.directory / "batches" / case["id"]
                batch = run_batch(binary, matrix / case["plan"], batch_path)
                self.assertEqual(len(batch["runs"]), 4)
                self.assertEqual({(run["seed"], run["variant"], run["orientation"]) for run in batch["runs"]},
                                 {("42", variant, orientation) for variant in ("baseline", "candidate")
                                  for orientation in ("forward", "mirrored")})
                failures = [run for run in batch["runs"] if run["status"] != "ok"]
                self.assertEqual(failures, [], f"{case['id']}: {failures!r}; retained batch: {batch_path}")
                for run in batch["runs"]:
                    report = run["report"]
                    reasons.add(report["termination"]["type"])
                    participants = report["participants"]
                    self.assertEqual({p["hit_dice"] for p in participants}, {case["hit_dice"]})
                    self.assertEqual({p["faction"] for p in participants}, {"blue", "red"})
                    for participant in participants:
                        damage += int(participant["damage_dealt"])
                        participant_abilities.append(participant["abilities"])
                        self.assertTrue(all(resource["reserved"] == 0
                                            for resource in participant["final_state"]["resources"]))
                equal, count = replay(binary, batch_path)
                self.assertTrue(equal, case["id"])
                self.assertEqual(count, 4)
        self.assertEqual(levels, set(HD_LEVELS))
        self.assertEqual(families, set(FAMILIES))
        self.assertGreater(damage, 0)
        if played is cases:
            # Matrix-wide content properties need every case; the deterministic
            # release run checks them on every CI run.
            assert_matrix_ability_coverage(participant_abilities)
            self.assertIn("elimination", reasons)
            self.assertTrue({"stalemate", "tick_limit", "action_limit"}.intersection(reasons))
