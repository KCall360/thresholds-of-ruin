"""Candidate edits are compiled, executed and replayed by the real backend."""
import json
import shutil
import subprocess
import sys
import tomllib
from process_harness import ProcessTestCase, ROOT, WIZARD_TOKEN
from arena_evaluation import files, run_batch, replay
from arena_matrix import create_matrix


class SearchParameterProcesses(ProcessTestCase):
    def test_health_curve_candidates_are_compiled_and_conserve_actual_injury(self):
        from arena_matrix import toml_source
        from arena_search_parameters import materialize
        compiler = self.bin / ("tor-scenario" + self.suffix)
        binary = self.bin / ("tor-arena" + self.suffix)
        matrix = self.directory / "health-matrix"
        inventory = create_matrix(matrix, compiler, str(ROOT / "scenarios/mob-arena/rules.toml"), ["42"],
                                  families=["strength_heavy"], levels=[4])
        plan = matrix / inventory["cases"][0]["plan"]
        for path in plan.parent.glob("*/scenario.toml"):
            document = tomllib.loads(path.read_text(encoding="utf-8"))
            document["arena"]["actions"] = 32
            path.write_text(toml_source(document), encoding="utf-8", newline="\n")
        fields = ("base_period", "reduction_per_hd", "minimum_period")
        specification = {"format": "tor-arena-search-parameters-v2", "parameters": {
            field: {"space": {"type": "integer", "low": 0, "high": 1000000},
                    "targets": [{"file": "rules.toml", "path": ["health_recovery", field]}]}
            for field in fields}}
        values = dict(zip(fields, (1, 0, 1)))
        output = self.directory / "health-candidate"
        self.assertEqual(materialize(compiler, plan, specification, values, output)["status"], "valid")
        batch = run_batch(binary, output / "plan.json", self.directory / "health-batch")
        self.assertTrue(all(run["status"] == "ok" for run in batch["runs"]))
        for run in batch["runs"]:
            participants = run["report"]["participants"]
            for actor in participants:
                self.assertEqual(actor["initial"]["injury"] + int(actor["damage_received"]),
                                 actor["final_state"]["injury"] + int(actor["healing_received"])
                                 + int(actor["health_recovered"]))
            if run["variant"] == "candidate":
                self.assertGreater(sum(int(actor["health_recovered"]) for actor in participants), 0)
        invalid = materialize(compiler, plan, specification, {**values, "base_period": 0},
                              self.directory / "invalid-health")
        self.assertEqual(invalid["status"], "invalid")
        self.assertTrue(invalid["validation_failures"])

    def test_ruleset_economy_and_talent_edits_change_actual_backend_outcomes(self):
        from arena_matrix import encounter, toml_source
        from arena_search_parameters import materialize
        compiler = self.bin / ("tor-scenario" + self.suffix)
        binary = self.bin / ("tor-arena" + self.suffix)
        sources = self.directory / "rules-sources"
        rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text(encoding="utf-8"))
        # Preserve independent named talent parameters through two tiny local
        # modifiers, each purchased with a real owned point. Neither is shipped.
        for identity, stat, value in [
            ("fixture_mana_capacity", {"kind": "resource_max", "resource": "mana"}, 2),
            ("fixture_spell_flat", {"kind": "ability", "ability": "spark_lance_fire", "stat": "flat_bonus"}, 1),
        ]:
            rules["talents"].append({"id": identity, "name": identity, "prerequisites": [],
                "requirement": {"kind": "total_hd", "minimum": 1}, "form_choices": [],
                "modifiers": [{"scope": "always", "op": {"kind": "add", "stat": stat, "value": value}}]})
        inputs = {}
        for orientation in ["forward", "mirrored"]:
            package = sources / orientation
            (package / "regions").mkdir(parents=True)
            manifest, region = encounter("sustain", 8, True, orientation == "mirrored", "rules.toml")
            manifest["arena"]["ticks"] = 10000
            subject = manifest["characters"][0]["creature"]
            for owner, identity in [(1, "fixture_mana_capacity"), (2, "fixture_spell_flat")]:
                subject["hit_dice"][owner]["purchases"] = [{"pool": "advancement",
                    "selection": {"kind": "talent", "talent": identity}}]
            manifest["creatures"]["species"]["opponent"]["modifiers"] = [
                {"scope": "always", "op": {"kind": "set", "stat": {"kind": "health"}, "value": 100000}}]
            # Measure spell economy while both actors remain in range. Idle
            # exploration otherwise makes mobility and lost sight the limit.
            for species in manifest["creatures"]["species"].values():
                species["behavior"]["idle"] = "wait"
            region["anchors"]["start"] = [6 if orientation == "mirrored" else 2, 2, 0]
            region["actors"][0]["at"] = [2 if orientation == "mirrored" else 6, 2, 0]
            region["actors"][0]["body"] = manifest["characters"][0]["body"].copy()
            region["walls"] = [[4, y, 0] for y in range(5)]
            # Prove the same named edits survive opposite catalog storage orders.
            import copy
            authored_rules = copy.deepcopy(rules)
            if orientation == "mirrored":
                for catalog in ["resources", "abilities", "talents"]:
                    authored_rules[catalog].reverse()
            for name, document in [("scenario.toml", manifest), ("rules.toml", authored_rules), ("regions/1.toml", region)]:
                (package / name).write_text(toml_source(document), encoding="utf-8", newline="\n")
            inputs[orientation] = str(package)
        plan = self.directory / "rules-plan.json"
        plan.write_text(json.dumps({"baseline": inputs, "candidate": inputs,
                                    "seeds": ["42"], "faction": "blue"}), encoding="utf-8")
        changes = {
            "mana_period": (["resources", "mana", "recovery_period"], 400),
            "mana_capacity": (["resources", "mana", "capacity", "base"], 3),
            "spell_resolution_cost": (["abilities", "spark_lance_fire", "cost", "resolution"], 0),
            "fixture_mana_capacity": (["talents", "fixture_mana_capacity", "modifiers", 0, "op", "value"], 3),
            "fixture_spell_flat": (["talents", "fixture_spell_flat", "modifiers", 0, "op", "value"], 2),
        }
        spec = {"format": "tor-arena-search-parameters-v2", "parameters": {
            name: {"space": {"type": "integer", "low": 0, "high": 1000},
                   "targets": [{"file": "rules.toml", "path": path}]}
            for name, (path, _) in changes.items()}}
        values = {name: value for name, (_, value) in changes.items()}
        output = self.directory / "rules-candidate"
        self.assertEqual(materialize(compiler, plan, spec, values, output)["status"], "valid")
        batch_path = self.directory / "rules-batch"
        batch = run_batch(binary, output / "plan.json", batch_path)
        self.assertTrue(all(run["status"] == "ok" for run in batch["runs"]))
        for run in batch["runs"]:
            package = output / (run["variant"] + "-" + run["orientation"])
            source = (package / "rules.toml").read_text(encoding="utf-8")
            document = tomllib.loads(source)
            for name, (path, value) in changes.items():
                actual, original = document, rules
                for depth, key in enumerate(path):
                    if depth == 1:
                        actual = next(item for item in actual if item.get("id", item.get("resource")) == key)
                        original = next(item for item in original if item.get("id", item.get("resource")) == key)
                    else:
                        actual, original = actual[key], original[key]
                self.assertNotEqual(value, original, name + " must change behavior, not only reserialize")
                self.assertEqual(actual, value if run["variant"] == "candidate" else original)
            certificate = json.loads((package / "validation.json").read_text(encoding="utf-8"))
            self.assertEqual(run["report"]["ruleset_hash"], certificate["ruleset_hash"])
            self.assertEqual(run["report"]["termination"], {"type": "tick_limit"})
        for orientation in ["forward", "mirrored"]:
            actors = {run["variant"]: next(actor for actor in run["report"]["participants"]
                                           if actor["faction"] == "blue")
                      for run in batch["runs"] if run["orientation"] == orientation}
            mana_maximum = {variant: next(resource["maximum"] for resource in actor["initial"]["resources"]
                                         if resource["resource"] == "mana")
                            for variant, actor in actors.items()}
            self.assertEqual(mana_maximum["candidate"] - mana_maximum["baseline"], 2)
            self.assertGreater(int(actors["candidate"]["abilities"]["spark_lance_fire"]["resolutions"]),
                               int(actors["baseline"]["abilities"]["spark_lance_fire"]["resolutions"]))
            self.assertGreater(int(actors["candidate"]["damage_dealt"]), int(actors["baseline"]["damage_dealt"]))
            self.assertTrue(all("basic_melee" not in actor["abilities"] for actor in actors.values()))
        # Planned sustain limits are metrics observations, never win/loss results.
        self.assertEqual(batch["summary"]["paired_seeds"], 0)
        self.assertEqual(batch["summary"]["excluded_seeds"], ["42"])
        self.assertEqual(replay(binary, batch_path), (True, 4))

    def test_point_grant_parameters_change_inspected_budget_and_reject_zero_grants(self):
        from arena_search_parameters import materialize
        compiler = self.bin / ("tor-scenario" + self.suffix)
        matrix = self.directory / "grant-matrix"
        inventory = create_matrix(matrix, compiler, str(ROOT / "scenarios/mob-arena/rules.toml"), ["42"],
                                  families=["strength_heavy"], levels=[1])
        plan = matrix / inventory["cases"][0]["plan"]
        spec = {"format": "tor-arena-search-parameters-v2", "parameters": {
            "points": {"space": {"type": "integer", "low": 0, "high": 2}, "targets": [
                {"file": "rules.toml", "path": ["classes", "warrior", "point_grants", "advancement", "points"]}]}}}
        output = self.directory / "larger-grant"
        self.assertEqual(materialize(compiler, plan, spec, {"points": 2}, output)["status"], "valid")
        for variant, points in (("baseline", 1), ("candidate", 2)):
            path = output / (variant + "-forward") / "rules.toml"
            rules = tomllib.loads(path.read_text(encoding="utf-8"))
            warrior = next(value for value in rules["classes"] if value["id"] == "warrior")
            self.assertEqual(warrior["point_grants"], [{"pool": "advancement", "points": points}])
        # A normal paused package exposes the materialized budget through the
        # real server rather than inferring runtime behavior from authored TOML.
        from arena_matrix import toml_source
        package = self.directory / "grant-inspection"
        shutil.copytree(output / "candidate-forward", package)
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        manifest["arena"]["start_paused"] = True
        path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        validated = subprocess.run([compiler, "validate", package], capture_output=True,
                                   text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.server(scenario=package, wizard=True)
        wizard, _ = self.client(WIZARD_TOKEN, observe=True)
        self.assertIsNone(self.command(wizard, {"type": "wizard", "command": "creature inspect 1"})["error"])
        messages = [json.loads(line).get("message") for line in wizard.transcript if line.startswith("{")]
        report = next(message["report"] for message in reversed(messages)
                      if message and message["type"] == "creature_inspection")
        self.assertEqual(report["hit_dice"][0]["point_pools"][0]["granted"], 2)
        self.assertEqual(report["hit_dice"][0]["point_pools"][0]["spent"], 1)
        rejected = self.directory / "zero-grant"
        self.assertEqual(materialize(compiler, plan, spec, {"points": 0}, rejected)["status"], "invalid")
        self.assertFalse((rejected / "plan.json").exists())

    def test_imported_sources_materialize_into_replayable_detached_arena_inputs(self):
        from arena_matrix import toml_source
        from arena_search_parameters import materialize
        from ruleset_sources import load_rules
        compiler = self.bin / ("tor-scenario" + self.suffix)
        binary = self.bin / ("tor-arena" + self.suffix)
        imported = self.directory / "import.toml"
        # Exercise authored Windows line endings on every process-test host.
        imported.write_bytes(b'import = "interactions-v37"\r\noverrides = [{op = "set", path = ["resources", "mana", "recovery_period"], value = 897}]\r\n')
        expected = load_rules(imported, compiler)
        matrix = self.directory / "import-matrix"
        inventory = create_matrix(matrix, compiler, str(imported), ["42"], families=["sustain"], levels=[4])
        self.assertEqual(set(inventory["rules_dependencies"]), {"interactions-v37"})
        plan = matrix / inventory["cases"][0]["plan"]
        for variant in ["baseline", "candidate"]:
            for orientation in ["forward", "mirrored"]:
                path = plan.parent / (variant + "-" + orientation) / "rules.toml"
                self.assertEqual(tomllib.loads(path.read_text()), expected)
                # Exercise parameter materialization on authored imports too.
                path.write_text(imported.read_text(), encoding="utf-8")
        mana = next(i for i, resource in enumerate(expected["resources"]) if resource["resource"] == "mana")
        spec = {"format": "tor-arena-search-parameters-v2", "parameters": {
            "mana_period": {"space": {"type": "integer", "low": 1, "high": 1000},
                            "targets": [{"file": "rules.toml", "path": ["resources", "mana", "recovery_period"]}]}}}
        prepared = self.directory / "import-candidate"
        result = materialize(compiler, plan, spec, {"mana_period": 719}, prepared)
        self.assertEqual(result["status"], "valid")
        self.assertTrue(all(result["rules_dependencies"].values()))
        batch_path = self.directory / "import-batch"
        batch = run_batch(binary, prepared / "plan.json", batch_path)
        self.assertTrue(all(run["status"] == "ok" for run in batch["runs"]))
        for run in batch["runs"]:
            package = prepared / (run["variant"] + "-" + run["orientation"])
            source = (package / "rules.toml").read_text()
            document = tomllib.loads(source)
            self.assertNotIn("import", document)
            self.assertEqual(document["resources"][mana]["recovery_period"],
                             719 if run["variant"] == "candidate" else 897)
            certificate = json.loads((package / "validation.json").read_text(encoding="utf-8"))
            self.assertEqual(run["report"]["ruleset_hash"], certificate["ruleset_hash"])
        # Deliberately alter the authored import: retained replay uses only its
        # validated standalone snapshots and cannot resolve the changed source.
        for variant in ["baseline", "candidate"]:
            for orientation in ["forward", "mirrored"]:
                (plan.parent / (variant + "-" + orientation) / "rules.toml").write_text(
                    'import = "unknown"\n', encoding="utf-8")
        self.assertEqual(replay(binary, batch_path), (True, 4))
        self.assertEqual(tomllib.loads(toml_source(expected)), expected)

    def test_cli_preserves_sources_and_compiler_rejects_illegal_ordinary_attributes(self):
        compiler = self.bin / ("tor-scenario" + self.suffix)
        binary = self.bin / ("tor-arena" + self.suffix)
        matrix = self.directory / "matrix"
        inventory = create_matrix(matrix, compiler, str(ROOT / "scenarios/mob-arena/rules.toml"), ["42", "43"],
                                   families=["strength_heavy"], levels=[4])
        plan = matrix / inventory["cases"][0]["plan"]
        case = plan.parent
        before = {key: files(case / key) for key in ["baseline-forward", "baseline-mirrored",
                                                    "candidate-forward", "candidate-mirrored"]}
        definition = self.directory / "parameters.json"
        definition.write_text(json.dumps({"format": "tor-arena-search-parameters-v2", "parameters": {
            "strength": {"space": {"type": "integer", "low": 0, "high": 7}, "targets": [
                {"file": "scenario.toml", "path": ["characters", 1, "creature", "attributes", "strength"]}]}}}),
            encoding="utf-8")
        values = self.directory / "values.json"
        def apply(value, output):
            values.write_text(json.dumps({"strength": value}), encoding="utf-8")
            return subprocess.run([sys.executable, ROOT / "scripts/arena_search_parameters.py",
                                   "--compiler", compiler, "--plan", plan, "--spec", definition,
                                   "--values", values, "--output", output], capture_output=True,
                                  text=True, encoding="utf-8", timeout=30)
        output = self.directory / "legal"
        applied = apply(4, output)
        self.assertEqual(applied.returncode, 0, applied.stderr)
        for orientation in ["forward", "mirrored"]:
            original = tomllib.loads((case / ("candidate-" + orientation) / "scenario.toml").read_text(encoding="utf-8"))
            actual = tomllib.loads((output / ("candidate-" + orientation) / "scenario.toml").read_text(encoding="utf-8"))
            original["characters"][0]["creature"]["attributes"]["strength"] = 4
            self.assertEqual(actual, original)
            baseline = tomllib.loads((output / ("baseline-" + orientation) / "scenario.toml").read_text(encoding="utf-8"))
            self.assertEqual(baseline["characters"][0]["creature"]["attributes"]["strength"], 5)
        batch_path = self.directory / "batch"
        batch = run_batch(binary, output / "plan.json", batch_path)
        self.assertEqual(len(batch["runs"]), 8)
        self.assertTrue(all(run["status"] == "ok" for run in batch["runs"]))
        for value in ["42", "43"]:
            for orientation in ["forward", "mirrored"]:
                paired = {run["variant"]: run for run in batch["runs"]
                          if run["seed"] == value and run["orientation"] == orientation}
                health = {variant: next(p["initial"]["maximum_health"] for p in run["report"]["participants"]
                                         if p["actor"] == "1") for variant, run in paired.items()}
                self.assertEqual(health["baseline"] - health["candidate"], 1)
        self.assertEqual(replay(binary, batch_path), (True, 8))
        invalid = self.directory / "illegal"
        rejected = apply(6, invalid)
        self.assertEqual(rejected.returncode, 1, rejected.stderr)
        evidence = json.loads((invalid / "candidate.json").read_text(encoding="utf-8"))
        self.assertEqual(evidence["status"], "invalid")
        self.assertEqual({row["package"] for row in evidence["validation_failures"]},
                         {"candidate-forward", "candidate-mirrored"})
        self.assertFalse((invalid / "plan.json").exists())
        collision = apply(4, output)
        self.assertEqual(collision.returncode, 2)
        from unittest.mock import patch
        import arena_search_parameters as parameters
        resumed_output = self.directory / "resumed"
        compile_package = parameters.subprocess.run
        calls = 0
        def interrupted(*args, **kwargs):
            nonlocal calls
            # Resolution also launches the compiler. Interrupt only publication
            # after two validated packages, not read-only rules resolution.
            if args[0][1] == "validate":
                calls += 1
                if calls == 3:
                    raise InterruptedError("injected preparation interruption")
            return compile_package(*args, **kwargs)
        spec = json.loads(definition.read_text(encoding="utf-8"))
        with patch.object(parameters.subprocess, "run", side_effect=interrupted), self.assertRaises(InterruptedError):
            parameters.materialize(compiler, plan, spec, {"strength": 4}, resumed_output)
        self.assertEqual(calls, 3)
        checkpoint = json.loads((resumed_output / "materialization.json").read_text(encoding="utf-8"))
        self.assertEqual(set(checkpoint["packages"]), {"baseline-forward", "baseline-mirrored"})
        retained = {key: {p.relative_to(resumed_output / key).as_posix(): (p.read_bytes(), p.stat().st_mtime_ns)
                         for p in (resumed_output / key).rglob("*") if p.is_file()}
                    for key in ("baseline-forward", "baseline-mirrored")}
        values.write_text(json.dumps({"strength": 4}), encoding="utf-8")
        def resume():
            return subprocess.run([sys.executable, ROOT / "scripts/arena_search_parameters.py",
                                   "--compiler", compiler, "--plan", plan, "--spec", definition,
                                   "--values", values, "--output", resumed_output, "--resume"],
                                  capture_output=True, text=True, encoding="utf-8", timeout=30)
        recovered = resume()
        self.assertEqual(recovered.returncode, 0, recovered.stderr)
        self.assertEqual(json.loads((resumed_output / "candidate.json").read_text(encoding="utf-8")),
                         json.loads((output / "candidate.json").read_text(encoding="utf-8")))
        for key, entries in retained.items():
            for filename, state in entries.items():
                path = resumed_output / key / filename
                self.assertEqual((path.read_bytes(), path.stat().st_mtime_ns), state)
        # Recover publication interrupted between candidate.json and plan.json.
        (resumed_output / "plan.json").unlink()
        recovered = resume()
        self.assertEqual(recovered.returncode, 0, recovered.stderr)
        self.assertEqual(json.loads((resumed_output / "plan.json").read_text(encoding="utf-8")),
                         json.loads((output / "plan.json").read_text(encoding="utf-8")))
        validation_record = resumed_output / "candidate.json"
        validation_record.write_text("{}", encoding="utf-8")
        rejected = resume()
        self.assertEqual(rejected.returncode, 2)
        self.assertIn("Published candidate evidence differs", rejected.stderr)
        for key, fingerprint in before.items():
            self.assertEqual(files(case / key), fingerprint)
