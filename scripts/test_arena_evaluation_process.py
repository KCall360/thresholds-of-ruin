"""Offline evaluation launches the actual arena CLI and ordinary backend."""
import json
import hashlib
import tomllib
from pathlib import Path
import shutil
import subprocess
import sys
from process_harness import ProcessTestCase, ROOT
from ruleset_sources import load_rules


class ArenaEvaluationProcesses(ProcessTestCase):
    def test_explicit_per_arm_actor_ids_round_trip_actual_reports(self):
        from arena_matrix import toml_source
        plan = {"seeds": ["42"], "faction": "blue", "participant_rosters": {}}
        for variant, selected in (("baseline", 1), ("candidate", 5)):
            plan[variant] = {}
            plan["participant_rosters"][variant] = [
                {"actor": str(selected), "faction": "blue"}, {"actor": "2", "faction": "blue"},
                {"actor": "3", "faction": "red"}, {"actor": "4", "faction": "red"}]
            for orientation in ("forward", "mirrored"):
                package = self.directory / f"{variant}-{orientation}"
                shutil.copytree(ROOT / "scenarios/mob-arena", package)
                manifest_path = package / "scenario.toml"
                manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
                manifest["default_character"] = selected
                manifest["characters"][0]["id"] = selected
                manifest["arena"].update(participants=[selected, 2, 3, 4], actions=4)
                manifest_path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
                if orientation == "mirrored":
                    region_path = package / "regions/1.toml"
                    region = tomllib.loads(region_path.read_text(encoding="utf-8"))
                    region["anchors"]["start"][0] = 8 - region["anchors"]["start"][0]
                    for actor in region["actors"]:
                        actor["at"][0] = 8 - actor["at"][0]
                    region_path.write_text(toml_source(region), encoding="utf-8", newline="\n")
                validation = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                            capture_output=True, text=True, encoding="utf-8", timeout=15)
                self.assertEqual(validation.returncode, 0, validation.stderr)
                plan[variant][orientation] = str(package)
        plan_path = self.directory / "plan.json"
        plan_path.write_text(json.dumps(plan), encoding="utf-8")
        output = self.directory / "batch"
        binary = self.bin / ("tor-arena" + self.suffix)
        def invoke(*arguments):
            return subprocess.run([sys.executable, ROOT / "scripts/arena_evaluation.py",
                                   "--binary", binary, *arguments], capture_output=True,
                                  text=True, encoding="utf-8", timeout=60)
        result = invoke("run", plan_path, output)
        self.assertEqual(result.returncode, 0, result.stderr)
        original = (output / "batch.json").read_bytes()
        batch = json.loads(original)
        self.assertEqual(batch["participant_rosters"], plan["participant_rosters"])
        self.assertEqual(len(batch["runs"]), 4)
        for run in batch["runs"]:
            self.assertEqual(run["status"], "ok")
            self.assertEqual({row["actor"]: row["faction"] for row in run["report"]["participants"]},
                             {row["actor"]: row["faction"] for row in plan["participant_rosters"][run["variant"]]})
        for variant in ("baseline", "candidate"):
            for package in plan[variant].values():
                shutil.rmtree(package)
        resumed = invoke("resume", output)
        self.assertEqual(resumed.returncode, 0, resumed.stderr)
        self.assertEqual((output / "batch.json").read_bytes(), original)
        replayed = invoke("replay", output)
        self.assertEqual(replayed.returncode, 0, replayed.stderr)
        self.assertIn("4 runs match", replayed.stdout)
        del batch["participant_rosters"]
        (output / "batch.json").write_text(json.dumps(batch), encoding="utf-8")
        rejected = invoke("resume", output)
        self.assertEqual(rejected.returncode, 2)
        self.assertIn("identities", rejected.stderr)

    def test_ordered_strikes_cli_counts_primary_strikes_and_includes_rider_damage(self):
        import copy
        from arena_evaluation import RUN_FORMAT
        from arena_matrix import toml_source
        from strike_sequence_package import sequence_package
        package = sequence_package(self, autonomous=True)
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        manifest["arena"] = {"participants": [1, 2], "control": "all_ai", "ticks": 35, "actions": 100}
        manifest["factions"] = {"blue": ["red"], "red": ["blue"]}
        attacker = manifest["creatures"]["species"]["sequence_fighter"]
        attacker["modifiers"] = [row for row in attacker["modifiers"] if row["op"]["kind"] != "grant_ability"]
        defender = copy.deepcopy(attacker)
        defender["melee"].update(wind_up=10000, recovery=10000)
        manifest["creatures"]["species"]["denominator_defender"] = defender
        def damage(amount):
            return {"kind": "damage", "damage": {"kind": "components", "components": [
                {"category": "impact", "amount": {"kind": "fixed", "amount": amount}}]}}
        rows = []
        for offset in (0, 10, 20):
            rows.append({"offset": offset, "check": {"actor": "user", "skill": {"kind": "selected_weapon"},
                         "against": {"kind": "defense", "defense": "physical"}},
                "on_success": [damage(3)], "on_failure": [], "descriptors": [],
                "riders": [{"trigger": "success", "check": {"skill": {"kind": "selected_weapon"}, "defense": "physical"},
                            "effects": [damage(amount)], "descriptors": []} for amount in (2, 4)]})
        attacker["melee"].update(bonus=1000, wind_up=10, recovery=10000, strikes=rows)
        manifest["characters"][0].update(unselected="ai", ai="sequence")
        manifest["characters"][0]["creature"]["faction"] = "blue"
        path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        region["actors"][0]["creature"].update(species="denominator_defender", faction="red")
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        validation = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                    capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validation.returncode, 0, validation.stderr)
        result = subprocess.run([self.bin / ("tor-arena" + self.suffix), "--scenario", package,
                                 "--all-ai", "--seed", "42"], capture_output=True, text=True,
                                encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["format"], RUN_FORMAT)
        self.assertEqual(report["tick"], "35")
        self.assertEqual(report["termination"], {"type": "tick_limit"})
        self.assertGreaterEqual(int(report["actions"]), 2)
        actors = {actor["actor"]: actor for actor in report["participants"]}
        source = actors["1"]
        self.assertEqual(source["abilities"], {"basic_melee": {"resolutions": "3", "applications": "3"}})
        self.assertEqual((source["attack_checks"], source["hits"]), ("3", "3"))
        self.assertEqual(source["damage_dealt"], "27")
        self.assertEqual(actors["2"]["damage_received"], "27")

    def test_mana_recovery_changes_sustained_ranged_throughput_across_a_sight_clear_barrier(self):
        import copy
        from arena_matrix import encounter, toml_source
        canonical = load_rules(ROOT / "scenarios/mob-arena/rules.toml", self.bin / ("tor-scenario" + self.suffix))
        current_period = next(value for value in canonical["resources"] if value["resource"] == "mana")["recovery_period"]
        slow_period = 1000
        self.assertLess(current_period, slow_period)
        lance = next(ability for ability in canonical["abilities"] if ability["id"] == "spark_lance_fire")
        self.assertEqual(lance["cost"]["resource"], "mana")
        self.assertEqual(len(lance["strikes"]["strikes"]), 1)
        cost = lance["cost"]["start"] + lance["cost"]["resolution"]
        self.assertGreater(cost, 0)
        # Observe a second ten-recovery window after the initial pool is spent.
        # The late-window comparison must hold independently of the burst.
        window = 10 * slow_period
        seeds = (42, 43)
        counts = {}
        for period in (slow_period, current_period):
            for ticks in (window, 2 * window):
                package = self.directory / f"sustain-{period}-{ticks}"
                (package / "regions").mkdir(parents=True)
                rules = copy.deepcopy(canonical)
                next(value for value in rules["resources"] if value["resource"] == "mana")["recovery_period"] = period
                manifest, region = encounter("sustain", 4, True, False, "rules.toml")
                manifest["arena"]["ticks"] = ticks
                manifest["creatures"]["species"]["opponent"]["modifiers"] = [
                    {"scope": "always", "op": {"kind": "set", "stat": {"kind": "health"}, "value": 100000}}]
                # A passive melee-only target cannot change exposure through
                # buffs, control spells or a different acquired form.
                manifest["archetypes"]["opponent"]["creature"]["hit_dice"] = [{"source": "racial"}]
                # Both bodies occupy the full two-cell height, with eyes above
                # the barriers. Neither can climb or wander out of spell range
                # when pursuit fails or the caster is waiting for mana.
                region["actors"][0]["body"] = copy.deepcopy(manifest["characters"][0]["body"])
                positions = [region["anchors"]["start"], region["actors"][0]["at"]]
                region["walls"] = [[x, y, 0] for x in range(9) for y in range(5)
                                   if [x, y, 0] not in positions]
                for name, document in [("scenario.toml", manifest), ("rules.toml", rules), ("regions/1.toml", region)]:
                    (package / name).write_text(toml_source(document), encoding="utf-8", newline="\n")
                validation = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                            capture_output=True, text=True, encoding="utf-8", timeout=15)
                self.assertEqual(validation.returncode, 0, validation.stderr)
                for seed in seeds:
                    with self.subTest(period=period, ticks=ticks, seed=seed):
                        result = subprocess.run([self.bin / ("tor-arena" + self.suffix), "--scenario", package,
                                                 "--all-ai", "--seed", str(seed)], capture_output=True, text=True,
                                                encoding="utf-8", timeout=30)
                        (self.directory / f"sustain-{period}-{ticks}-seed-{seed}.json").write_text(
                            result.stdout, encoding="utf-8", newline="\n")
                        self.assertEqual(result.returncode, 0, result.stderr)
                        report = json.loads(result.stdout)
                        self.assertEqual(report["tick"], str(ticks))
                        self.assertEqual(report["termination"], {"type": "tick_limit"})
                        actors = {actor["faction"]: actor for actor in report["participants"]}
                        self.assertEqual(set(actors), {"blue", "red"})
                        caster, target = actors["blue"], actors["red"]
                        self.assertFalse(caster["final_state"]["dead"])
                        self.assertFalse(target["final_state"]["dead"])
                        self.assertEqual(caster["damage_received"], "0")
                        self.assertEqual(target["abilities"], {})
                        self.assertLess(int(target["damage_received"]), target["initial"]["maximum_health"] // 2)
                        self.assertEqual(set(caster["abilities"]), {"spark_lance_fire"})
                        resolutions = int(caster["abilities"]["spark_lance_fire"]["resolutions"])
                        mana = next(resource for resource in caster["initial"]["resources"]
                                    if resource["resource"] == "mana")
                        self.assertGreater(resolutions, mana["balance"] // cost)
                        counts[period, ticks, seed] = resolutions
        for seed in seeds:
            self.assertGreater(counts[slow_period, window, seed], 4)
            for ticks in (window, 2 * window):
                self.assertGreater(counts[current_period, ticks, seed], counts[slow_period, ticks, seed])
            self.assertGreater(counts[current_period, 2 * window, seed] - counts[current_period, window, seed],
                               counts[slow_period, 2 * window, seed] - counts[slow_period, window, seed])
        # Check/damage RNG may change hits, but cannot change this workload's
        # funded casting opportunities or turn a lucky seed into a pass.
        for period in (slow_period, current_period):
            for ticks in (window, 2 * window):
                self.assertEqual(counts[period, ticks, seeds[0]], counts[period, ticks, seeds[1]])

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
        rules_source = (package / "rules.toml").read_text(encoding="utf-8")
        self.assertEqual(report["format"], "tor-arena-run-v4")
        self.assertEqual(report["ruleset"], load_rules(package / "rules.toml", self.bin / ("tor-scenario" + self.suffix))["id"])
        certificate = json.loads((package / "validation.json").read_text(encoding="utf-8"))
        self.assertEqual(report["ruleset_hash"], certificate["ruleset_hash"])
        self.assertIn("import", tomllib.loads(rules_source))
        self.assertNotEqual(report["ruleset_hash"], hashlib.sha256(rules_source.encode()).hexdigest())
        self.assertEqual(report["input"]["ruleset_hash"], report["ruleset_hash"])

        self.assertEqual(report["actions"], "32")
        self.assertEqual(report["termination"], {"type": "action_limit"})
        self.assertEqual(len(report["participants"]), 4)
        self.assertTrue(any(int(p["damage_dealt"]) for p in report["participants"]))
        for participant in report["participants"]:
            self.assertEqual(
                participant["initial"]["injury"] + int(participant["damage_received"]),
                participant["final_state"]["injury"] + int(participant["healing_received"])
                + int(participant["health_recovered"]))
        second = run("--all-ai", "--seed", "42")
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(json.loads(second.stdout), report)
        changed = run("--all-ai", "--seed", "43")
        self.assertEqual(changed.returncode, 0, changed.stderr)
        self.assertNotEqual(json.loads(changed.stdout)["input_hash"], report["input_hash"])
        manual = run("--seed", "42")
        self.assertEqual(manual.returncode, 2)
        failure = json.loads(manual.stdout)
        self.assertEqual(failure["format"], "tor-arena-run-v4")
        self.assertEqual(failure["status"], "failure")
        self.assertEqual(path.read_text(encoding="utf-8"), source, "evaluation must not edit its source")
        for arguments in [("--all-ai", "--seed", "bad"), ("--all-ai", "--all-ai"), ("--unknown",)]:
            invalid = run(*arguments)
            self.assertEqual(invalid.returncode, 2)
            failure = json.loads(invalid.stdout)
            self.assertEqual(failure["format"], "tor-arena-run-v4")
            self.assertEqual(failure["status"], "failure")

    def test_current_report_records_nonzero_actual_healing_and_natural_recovery(self):
        from arena_matrix import toml_source
        package = self.directory / "health-accounting"
        shutil.copytree(ROOT / "scenarios/mob-arena", package)
        manifest = package / "scenario.toml"
        document = tomllib.loads(manifest.read_text(encoding="utf-8"))
        document["arena"]["actions"] = 32
        manifest.write_text(toml_source(document), encoding="utf-8", newline="\n")
        rules_path = package / "rules.toml"
        rules = load_rules(rules_path, self.bin / ("tor-scenario" + self.suffix))
        rules["health_recovery"] = {"base_period": 1, "reduction_per_hd": 0, "minimum_period": 1}
        for ability in rules["abilities"]:
            if ability["strikes"]["kind"] == "authored":
                for strike in ability["strikes"]["strikes"]:
                    # Only augment damaging target branches. Source-recipient
                    # observations and self-defense keep their valid payloads.
                    if any(effect["kind"] == "damage" for effect in strike["on_success"]):
                        strike["on_success"].append({"kind": "heal", "amount": {"kind": "fixed", "amount": 1}})
        # Basic melee selects its species definition, so apply the same workload
        # payload to that authored source rather than inventing another ability.
        for species in document["creatures"]["species"].values():
            for strike in species["melee"]["strikes"]:
                strike["on_success"].append({"kind": "heal", "amount": {"kind": "fixed", "amount": 1}})
        manifest.write_text(toml_source(document), encoding="utf-8", newline="\n")
        rules_path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
        validation = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                    capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validation.returncode, 0, validation.stderr)
        result = subprocess.run([self.bin / ("tor-arena" + self.suffix), "--scenario", package,
                                 "--all-ai", "--seed", "42"], capture_output=True, text=True,
                                encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["format"], "tor-arena-run-v4")
        self.assertTrue(any(int(actor["healing_received"]) > 0 for actor in report["participants"]))
        self.assertTrue(any(int(actor["health_recovered"]) > 0 for actor in report["participants"]))
        self.assertEqual(sum(int(actor["damage_dealt"]) for actor in report["participants"]),
                         sum(int(actor["damage_received"]) for actor in report["participants"]))
        for actor in report["participants"]:
            self.assertEqual(actor["initial"]["injury"] + int(actor["damage_received"]),
                             actor["final_state"]["injury"] + int(actor["healing_received"])
                             + int(actor["health_recovered"]))

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
