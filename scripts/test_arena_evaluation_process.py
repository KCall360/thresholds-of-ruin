"""Offline evaluation launches the actual arena CLI and ordinary backend."""
import json
from pathlib import Path
import shutil
import subprocess
import sys
from process_harness import ProcessTestCase, ROOT


class ArenaEvaluationProcesses(ProcessTestCase):
    def test_cli_seeded_report_limits_reproducibility_and_failures(self):
        package = self.directory / "arena"
        shutil.copytree(ROOT / "scenarios/mob-arena", package)
        path = package / "scenario.toml"
        source = path.read_text(encoding="utf-8").replace("actions = 10000", "actions = 32")
        path.write_text(source, encoding="utf-8", newline="\n")
        validation = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                    capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validation.returncode, 0, validation.stderr)
        def run(*args):
            return subprocess.run([self.bin / ("tor-arena" + self.suffix), "--scenario", package, *args],
                                  capture_output=True, text=True, encoding="utf-8", timeout=30)
        first = run("--all-ai", "--seed", "42")
        self.assertEqual(first.returncode, 0, first.stderr)
        report = json.loads(first.stdout)
        self.assertEqual(report["actions"], "32")
        self.assertEqual(report["termination"], {"type": "action_limit"})
        self.assertEqual(len(report["participants"]), 4)
        self.assertTrue(any(int(p["damage_dealt"]) for p in report["participants"]))
        second = run("--all-ai", "--seed", "42")
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(json.loads(second.stdout), report)
        changed = run("--all-ai", "--seed", "43")
        self.assertEqual(changed.returncode, 0, changed.stderr)
        self.assertNotEqual(json.loads(changed.stdout)["input_hash"], report["input_hash"])
        manual = run("--seed", "42")
        self.assertEqual(manual.returncode, 2)
        self.assertEqual(json.loads(manual.stdout)["status"], "failure")
        self.assertEqual(path.read_text(encoding="utf-8"), source, "evaluation must not edit its source")
        for arguments in [("--all-ai", "--seed", "bad"), ("--all-ai", "--all-ai"), ("--unknown",)]:
            invalid = run(*arguments)
            self.assertEqual(invalid.returncode, 2)
            self.assertEqual(json.loads(invalid.stdout)["status"], "failure")

    def test_paired_batch_snapshots_replay_and_tamper_rejection(self):
        forward = self.directory / "forward"
        mirrored = self.directory / "mirrored"
        shutil.copytree(ROOT / "scenarios/mob-arena", forward)
        manifest = forward / "scenario.toml"
        manifest.write_text(manifest.read_text(encoding="utf-8").replace(
            "actions = 10000", "actions = 32"), encoding="utf-8", newline="\n")
        shutil.copytree(forward, mirrored)
        region = mirrored / "regions/1.toml"
        region.write_text(region.read_text(encoding="utf-8").replace(
            "start = [1, 2, 0]", "start = [7, 2, 0]").replace(
            "id = 2, at = [2,3,0]", "id = 2, at = [6,3,0]").replace(
            "id = 3, at = [6,2,0]", "id = 3, at = [2,2,0]").replace(
            "id = 4, at = [6,3,0]", "id = 4, at = [2,3,0]"),
            encoding="utf-8", newline="\n")
        for package in [forward, mirrored]:
            validation = subprocess.run([self.bin / ("tor-scenario" + self.suffix),
                                        "validate", package], capture_output=True,
                                       text=True, encoding="utf-8", timeout=15)
            self.assertEqual(validation.returncode, 0, validation.stderr)
        inputs = {"forward": str(forward), "mirrored": str(mirrored)}
        plan = self.directory / "plan.json"
        plan.write_text(json.dumps({"baseline": inputs, "candidate": inputs,
                                    "seeds": ["42", "43"], "faction": "blue"}), encoding="utf-8")
        output = self.directory / "batch"
        binary = self.bin / ("tor-arena" + self.suffix)
        def invoke(*arguments):
            return subprocess.run([sys.executable, ROOT / "scripts/arena_evaluation.py",
                                   "--binary", binary, *arguments], capture_output=True,
                                  text=True, encoding="utf-8", timeout=60)
        result = invoke("run", plan, output)
        self.assertEqual(result.returncode, 0, result.stderr)
        batch = json.loads((output / "batch.json").read_text(encoding="utf-8"))
        self.assertEqual(len(batch["runs"]), 8)
        self.assertEqual({run["seed"] for run in batch["runs"]}, {"42", "43"})
        self.assertTrue(all(run["status"] == "ok" for run in batch["runs"]))
        hashes = {run["orientation"]: run["report"]["input_hash"] for run in batch["runs"]}
        self.assertNotEqual(hashes["forward"], hashes["mirrored"])
        self.assertEqual(len(list((output / "runs").glob("*.json"))), 8)
        self.assertFalse((output / "partial.json").exists())
        self.assertEqual(batch["summary"]["paired_seeds"], 0)
        self.assertEqual(batch["summary"]["excluded_seeds"], ["42", "43"])
        replay = invoke("replay", output)
        self.assertEqual(replay.returncode, 0, replay.stderr)
        self.assertIn("8 runs match", replay.stdout)
        batch_path = output / "batch.json"
        original_batch = batch_path.read_text(encoding="utf-8")
        altered = json.loads(original_batch)
        altered["binary_sha256"] = "0" * 64
        batch_path.write_text(json.dumps(altered), encoding="utf-8")
        wrong_binary = invoke("replay", output)
        self.assertEqual(wrong_binary.returncode, 2)
        self.assertIn("Executable fingerprint differs", wrong_binary.stderr)
        batch_path.write_text(original_batch, encoding="utf-8")
        record = next((output / "runs").glob("*.json"))
        original_record = record.read_text(encoding="utf-8")
        changed_record = json.loads(original_record)
        changed_record["status"] = "failure"
        record.write_text(json.dumps(changed_record), encoding="utf-8")
        wrong_record = invoke("replay", output)
        self.assertEqual(wrong_record.returncode, 2)
        self.assertIn("Run record differs", wrong_record.stderr)
        record.write_text(original_record, encoding="utf-8")
        broken = self.directory / "not-arena"
        shutil.copytree(ROOT / "scenarios/two-room", broken)
        other = self.directory / "not-arena-mirrored"
        shutil.copytree(broken, other)
        # Distinct source metadata; both fail arena admission.
        (other / "scenario.toml").write_text((other / "scenario.toml").read_text(
            encoding="utf-8") + "\n# mirrored input\n", encoding="utf-8")
        plan.write_text(json.dumps({"baseline": inputs,
                                    "candidate": {"forward": str(broken), "mirrored": str(other)},
                                    "seeds": ["42"], "faction": "blue"}), encoding="utf-8")
        failure_output = self.directory / "failed-batch"
        failed = invoke("run", plan, failure_output)
        self.assertEqual(failed.returncode, 1, failed.stderr)
        failure_batch = json.loads((failure_output / "batch.json").read_text(encoding="utf-8"))
        self.assertEqual(failure_batch["summary"]["terminations"]["candidate"], {"failure": 2})
        self.assertEqual(failure_batch["summary"]["excluded_seeds"], ["42"])
        self.assertEqual(invoke("replay", failure_output).returncode, 0)
        # Source changes cannot affect retained inputs or replay.
        manifest.write_text("broken source", encoding="utf-8")
        replay = invoke("replay", output)
        self.assertEqual(replay.returncode, 0, replay.stderr)
        collision = invoke("run", plan, output)
        self.assertEqual(collision.returncode, 2)
        retained = output / "inputs/baseline-forward/scenario.toml"
        retained.write_text("tampered", encoding="utf-8")
        tampered = invoke("replay", output)
        self.assertEqual(tampered.returncode, 2)
        self.assertIn("Input snapshot differs", tampered.stderr)


    def test_interrupted_actual_engine_batch_resumes_without_replacing_completed_records(self):
        from unittest.mock import patch
        import arena_evaluation as arena
        inputs = {}
        for orientation in arena.ORIENTATIONS:
            package = self.directory / orientation
            shutil.copytree(ROOT / "scenarios/mob-arena", package)
            manifest = package / "scenario.toml"
            manifest.write_text(manifest.read_text(encoding="utf-8").replace(
                "actions = 10000", "actions = 32") + f"\n# {orientation}\n", encoding="utf-8")
            checked = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                     capture_output=True, text=True, encoding="utf-8", timeout=15)
            self.assertEqual(checked.returncode, 0, checked.stderr)
            inputs[orientation] = str(package)
        plan = self.directory / "plan.json"
        plan.write_text(json.dumps({"baseline": inputs, "candidate": inputs,
                                    "seeds": ["42", "43"], "faction": "blue"}), encoding="utf-8")
        binary = self.bin / ("tor-arena" + self.suffix)
        output = self.directory / "batch"
        execute = arena.execute
        calls = 0
        def interrupt(*args):
            nonlocal calls
            calls += 1
            if calls == 4:
                raise InterruptedError("injected interruption between real Engine executions")
            return execute(*args)
        with patch.object(arena, "execute", side_effect=interrupt), self.assertRaises(InterruptedError):
            arena.run_batch(binary, plan, output)
        retained = {p.name: (p.read_bytes(), p.stat().st_mtime_ns) for p in (output / "runs").iterdir()}
        self.assertEqual(len(retained), 3)
        def invoke(operation):
            return subprocess.run([sys.executable, ROOT / "scripts/arena_evaluation.py",
                                   "--binary", binary, operation, output], capture_output=True,
                                  text=True, encoding="utf-8", timeout=60)
        snapshot = output / "inputs/baseline-forward/scenario.toml"
        original = snapshot.read_bytes()
        snapshot.write_bytes(b"altered")
        rejected = invoke("resume")
        self.assertEqual(rejected.returncode, 2)
        self.assertIn("Input snapshot differs", rejected.stderr)
        snapshot.write_bytes(original)
        # Mutable authoring sources are no longer authoritative after snapshotting.
        Path(inputs["forward"]).joinpath("scenario.toml").write_text("changed source", encoding="utf-8")
        resumed = invoke("resume")
        self.assertEqual(resumed.returncode, 0, resumed.stderr)
        batch = arena.read_json(output / "batch.json")
        self.assertEqual(len(batch["runs"]), 8)
        self.assertEqual(batch["summary"]["excluded_seeds"], ["42", "43"])
        for name, state in retained.items():
            path = output / "runs" / name
            self.assertEqual((path.read_bytes(), path.stat().st_mtime_ns), state)
        completed = {p.name: p.read_bytes() for p in output.iterdir() if p.is_file()}
        self.assertEqual(invoke("resume").returncode, 0)
        self.assertEqual({p.name: p.read_bytes() for p in output.iterdir() if p.is_file()}, completed)
        replay = invoke("replay")
        self.assertEqual(replay.returncode, 0, replay.stderr)
        self.assertIn("8 runs match", replay.stdout)
