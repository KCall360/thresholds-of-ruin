"""New actual-CLI acceptance for the reconstructed actual-kit study and runner.

One fixed emu seed per mode is tooling acceptance, not a balance sample. Every
run must succeed, preserve real class identities and exactly replay its snapshot.
"""
from scalar_wire import u64
import json
import shutil
import subprocess
import sys
import unittest

from arena_evaluation import digest, load_complete
from process_harness import ProcessTestCase, ROOT


class RoguePlayableStudyProcesses(ProcessTestCase):
    def study_round_trip(self, reference):
        inputs = self.directory / "study"
        compiler = self.bin / ("tor-scenario" + self.suffix)
        binary = self.bin / ("tor-arena" + self.suffix)
        def invoke(script, *arguments):
            result = subprocess.run([sys.executable, ROOT / "scripts" / script, *arguments],
                                    capture_output=True, text=True, encoding="utf-8", timeout=150)
            (self.directory / f"{script}-{len(list(self.directory.glob('*.log')))}.log").write_text(
                result.stdout + result.stderr, encoding="utf-8")
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            return result
        invoke("rogue_roster_study.py", "--compiler", compiler, "--output", inputs,
               "--reference", reference, "--seeds", "0", "--case", "emu-hd1")
        inventory = json.loads((inputs / "study.json").read_text())
        self.assertEqual(inventory["compiler_sha256"], digest(compiler))
        self.assertEqual(inventory["validated_packages"], 4)
        self.assertEqual(inventory["required_cases"], 11)
        self.assertEqual(inventory["missing_cases"], ["snake-hd1", "hobgoblin-hd1", "zombie-hd1",
                                                       "quagga-hd1", "centaur-hd1", "yeti-hd1", "troll-hd1", "black_unicorn-hd1", "ice_monster-hd1",
                                                       "vampire-hd1"])
        self.assertEqual(inventory["battle_evidence"], "not_run")
        plan_path = inputs / "emu-hd1/plan.json"
        plan = json.loads(plan_path.read_text())
        output = self.directory / "batch"
        invoke("arena_evaluation.py", "--binary", binary, "run", plan_path, output)
        original = (output / "batch.json").read_bytes()
        batch = load_complete(binary, output)
        self.assertEqual(batch["binary_sha256"], digest(binary))
        self.assertEqual(batch["participant_rosters"], plan["participant_rosters"])
        self.assertEqual(len(batch["runs"]), 4)
        for run in batch["runs"]:
            self.assertEqual(run["status"], "ok", run)
            selected = "1" if run["variant"] == "baseline" else "2"
            self.assertEqual(run["report"]["input"]["selected"], selected)
            actors = {row["actor"]: row for row in run["report"]["participants"]}
            self.assertEqual(set(actors), {selected, "3"})
            self.assertEqual(actors[selected]["name"], "Warrior" if selected == "1" else "Mage")
            self.assertEqual(u64(actors[selected]["hit_dice"]), 1)
            self.assertEqual(actors[selected]["faction"], "delvers")
            self.assertEqual(actors["3"]["faction"], "dungeon_foes")
        shutil.rmtree(inputs)
        invoke("arena_evaluation.py", "--binary", binary, "resume", output)
        self.assertEqual((output / "batch.json").read_bytes(), original)
        replayed = invoke("arena_evaluation.py", "--binary", binary, "replay", output)
        self.assertIn("4 runs match", replayed.stdout)
        self.assertEqual(load_complete(binary, output), batch)

    def test_actual_unspent_starters_preserve_identity_through_cli_and_source_free_replay(self):
        self.study_round_trip("playable-unspent")

    def test_actual_first_purchases_preserve_identity_through_cli_and_source_free_replay(self):
        self.study_round_trip("playable-first-purchase")


if __name__ == "__main__":
    unittest.main()
