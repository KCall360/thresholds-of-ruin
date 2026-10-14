"""Pure regression checks for bounded arena-matrix acceptance orchestration."""
import ast
import copy
import itertools
import json
from pathlib import Path, PureWindowsPath
import subprocess
import tempfile
import os
import unittest
from unittest.mock import patch

from arena_matrix import FAMILIES, HD_LEVELS
from arena_matrix_process_harness import (EXCLUSIONS, assert_matrix_ability_coverage, compile_matrix_families,
                                          complete_matrix_cases, plays_complete_matrix, stratified_cases)


class ArenaMatrixProcessHarnessTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.output = Path(directory.name) / "matrix"
        self.compiler = Path(directory.name) / "tor-scenario"

    def inventory(self, family):
        excluded = family in ("mixed", "warrior_mage_dip", "mage_warrior_dip")
        return {"format": "tor-arena-matrix-v1", "compiler_sha256": "compiler-fingerprint",
                "ruleset": "test-rules", "rules_dependencies": {"test-rules": "rules-fingerprint"},
                "seeds": ["42"],
                "cases": [{"family": family, "hit_dice": hd, "id": f"{family}-hd{hd}",
                           "plan": f"{family}-hd{hd}/plan.json"}
                          for hd in HD_LEVELS if not (excluded and hd == 1)],
                "exclusions": ([{"family": family, "hit_dice": 1, "reason": "Requires two HD"}]
                               if excluded else [])}

    def batches(self):
        return [(self.output / family, self.inventory(family)) for family in FAMILIES]

    def successful_cli(self, command, **options):
        matrix = Path(command[command.index("--output") + 1])
        family = command[command.index("--families") + 1]
        matrix.mkdir(parents=True)
        (matrix / "matrix.json").write_text(json.dumps(self.inventory(family)), encoding="utf-8")
        options["stdout"].write(f"Validated {family}\n")
        return subprocess.CompletedProcess(command, 0)

    def test_every_family_gets_a_bounded_cli_call_with_all_bands_and_unchanged_deadline(self):
        with patch("arena_matrix_process_harness.subprocess.run", side_effect=self.successful_cli) as run:
            batches = compile_matrix_families(self.compiler, self.output)
        self.assertEqual(run.call_count, 20)
        for call, family in zip(run.call_args_list, FAMILIES, strict=True):
            command = call.args[0]
            self.assertEqual(command[command.index("--families") + 1:command.index("--hd")], [family])
            self.assertEqual(command[command.index("--hd") + 1:], [str(hd) for hd in HD_LEVELS])
            self.assertEqual(command[command.index("--seeds") + 1:command.index("--families")], ["42"])
            self.assertEqual(command[command.index("--compiler") + 1], str(self.compiler))
            self.assertEqual(call.kwargs["timeout"], 60)
            self.assertLessEqual(len(self.inventory(family)["cases"]) * 4, 28)
        cases = complete_matrix_cases(batches)
        self.assertEqual(len(cases), 137)
        self.assertEqual({case["family"] for _, case in cases}, set(FAMILIES))
        self.assertEqual({case["hit_dice"] for _, case in cases}, {1, 2, 4, 6, 8, 11, 16})
        self.assertEqual(sum(len(inventory["exclusions"]) for _, inventory in batches), 3)
        for matrix, case in cases:
            self.assertEqual(matrix, self.output / case["family"])
        self.assertFalse((self.output / "matrix.json").exists())

    def test_missing_duplicate_and_misfiled_cases_and_families_are_not_silently_dropped(self):
        mutations = {
            "missing family": lambda batches: batches.pop(),
            "duplicate family": lambda batches: batches.append(batches[0]),
            "missing case": lambda batches: batches[0][1]["cases"].pop(),
            "duplicate case": lambda batches: batches[0][1]["cases"].append(batches[0][1]["cases"][0]),
            "misfiled case": lambda batches: batches[0][1]["cases"][0].update(family=FAMILIES[1]),
            "wrong plan": lambda batches: batches[0][1]["cases"][0].update(plan="another/plan.json"),
            "wrong seed": lambda batches: batches[0][1].update(seeds=["43"]),
            "changed compiler": lambda batches: batches[-1][1].update(compiler_sha256="different"),
            "changed rules": lambda batches: batches[-1][1].update(rules_dependencies={}),
        }
        for name, mutate in mutations.items():
            with self.subTest(name=name):
                batches = self.batches()
                mutate(batches)
                with self.assertRaises(AssertionError):
                    complete_matrix_cases(batches)

    def test_exclusions_must_be_exact_and_unique(self):
        for mode in ("missing", "duplicate", "wrong HD", "missing reason"):
            with self.subTest(mode=mode):
                batches = self.batches()
                inventory = next(inventory for _, inventory in batches if inventory["exclusions"])
                exclusions = inventory["exclusions"]
                if mode == "missing":
                    exclusions.clear()
                elif mode == "duplicate":
                    exclusions.append(copy.deepcopy(exclusions[0]))
                elif mode == "wrong HD":
                    exclusions[0]["hit_dice"] = 2
                else:
                    exclusions[0]["reason"] = ""
                with self.assertRaises(AssertionError):
                    complete_matrix_cases(batches)

    def test_nonzero_timeout_and_launch_failures_preserve_full_output_and_partial_evidence(self):
        stdout = "compile output\n" * 1000 + "stdout tail\n"
        stderr = "compiler details\n" * 1000 + "stderr tail\n"
        for mode, (label, compiler) in itertools.product(
                ("exit", "timeout", "launch"),
                (("native", self.compiler), ("windows", PureWindowsPath(r"C:\fixture path\tor-scenario.exe")))):
            with self.subTest(mode=mode, path_kind=label, compiler=str(compiler)):
                output = self.output / (mode + "-" + label)
                def fail(command, **options):
                    matrix = Path(command[command.index("--output") + 1])
                    matrix.mkdir(parents=True)
                    (matrix / "partial.json").write_text('{"cases": []}', encoding="utf-8")
                    options["stdout"].write(stdout)
                    options["stderr"].write(stderr)
                    if mode == "timeout":
                        raise subprocess.TimeoutExpired(command, options["timeout"])
                    if mode == "launch":
                        raise OSError("could not launch compiler CLI")
                    return subprocess.CompletedProcess(command, 2)
                with patch("arena_matrix_process_harness.subprocess.run", side_effect=fail) as run:
                    with self.assertRaises(AssertionError) as raised:
                        compile_matrix_families(compiler, output)
                run.assert_called_once()
                message = str(raised.exception)
                for detail in (FAMILIES[0], str(output / FAMILIES[0]),
                               "--families", "--hd", "stdout tail", "stderr tail"):
                    self.assertIn(detail, message)
                # The diagnostic records repr(command), so Windows backslashes
                # are escaped. Decode that exact record rather than searching
                # for a platform-dependent substring inside its representation.
                command_line, = [line.removeprefix("Command: ") for line in message.splitlines()
                                 if line.startswith("Command: ")]
                command = ast.literal_eval(command_line)
                self.assertEqual(command, run.call_args.args[0])
                self.assertEqual(command[command.index("--compiler") + 1], str(compiler))
                reason = {"exit": "exited 2", "timeout": "timed out after 60 seconds",
                          "launch": "could not launch compiler CLI"}[mode]
                self.assertIn(reason, message)
                logs = output / "compiler-logs"
                self.assertEqual((logs / (FAMILIES[0] + ".stdout.log")).read_text(encoding="utf-8"), stdout)
                self.assertEqual((logs / (FAMILIES[0] + ".stderr.log")).read_text(encoding="utf-8"), stderr)
                self.assertEqual((output / FAMILIES[0] / "partial.json").read_text(encoding="utf-8"),
                                 '{"cases": []}')
                self.assertFalse((output / FAMILIES[0] / "matrix.json").exists())

    def test_successful_exit_does_not_accept_missing_invalid_or_partial_inventory(self):
        for mode in ("missing", "invalid", "partial"):
            with self.subTest(mode=mode):
                output = self.output / mode
                def incomplete(command, **options):
                    result = self.successful_cli(command, **options)
                    matrix = Path(command[command.index("--output") + 1])
                    if mode == "missing":
                        (matrix / "matrix.json").unlink()
                    elif mode == "invalid":
                        (matrix / "matrix.json").write_text("invalid", encoding="utf-8")
                    else:
                        (matrix / "partial.json").write_text("{}", encoding="utf-8")
                    return result
                with patch("arena_matrix_process_harness.subprocess.run", side_effect=incomplete) as run:
                    with self.assertRaises(AssertionError) as raised:
                        compile_matrix_families(self.compiler, output)
                run.assert_called_once()
                self.assertIn(FAMILIES[0], str(raised.exception))
                self.assertIn("inventory", str(raised.exception))
                self.assertIn("Validated " + FAMILIES[0], str(raised.exception))


class ArenaMatrixAbilityCoverageTests(unittest.TestCase):
    def usage(self):
        return {name: {"resolutions": "1", "applications": "1"} for name in
                ("rattling_blow", "spark_lance_fire", "spark_lance_cold", "falter",
                 "brace_for_impact", "shell_impact")}

    def test_offensive_resolution_can_miss_or_be_resisted(self):
        usage = self.usage()
        for name in ("rattling_blow", "spark_lance_fire", "spark_lance_cold", "falter"):
            usage[name]["applications"] = "0"
        self.assertEqual(set(assert_matrix_ability_coverage([usage]).values()), {1})

    def test_missing_offensive_resolution_or_defensive_effect_still_fails(self):
        for name, field in (("rattling_blow", "resolutions"), ("spark_lance_fire", "resolutions"),
                            ("spark_lance_cold", "resolutions"), ("falter", "resolutions"),
                            ("brace_for_impact", "applications"), ("shell_impact", "applications")):
            with self.subTest(ability=name, field=field):
                usage = self.usage()
                usage[name][field] = "0"
                with self.assertRaisesRegex(AssertionError, name + " must record " + field):
                    assert_matrix_ability_coverage([usage])


class DebugMatrixSample(unittest.TestCase):
    def cases(self):
        return [(Path(family), {"family": family, "hit_dice": hd, "id": f"{family}-hd{hd}"})
                for family in FAMILIES for hd in HD_LEVELS if (family, hd) not in EXCLUSIONS]

    def test_the_sample_plays_every_family_and_every_band_once_per_family(self):
        sample = stratified_cases(self.cases())
        self.assertEqual([case["family"] for _, case in sample], list(FAMILIES))
        self.assertEqual({case["hit_dice"] for _, case in sample}, set(HD_LEVELS))
        self.assertTrue(all((case["family"], case["hit_dice"]) not in EXCLUSIONS for _, case in sample))

    def test_release_or_an_explicit_request_plays_the_complete_matrix(self):
        cases = [({}, False), ({"TOR_TEST_PROFILE": "debug"}, False), ({"TOR_TEST_PROFILE": "release"}, True),
                 ({"TOR_TEST_PROFILE": "debug", "TOR_ARENA_MATRIX": "complete"}, True)]
        for environment, complete in cases:
            with self.subTest(environment=environment):
                cleared = {name: "" for name in ("TOR_TEST_PROFILE", "TOR_ARENA_MATRIX")}
                with patch.dict(os.environ, cleared):
                    for name in cleared:
                        del os.environ[name]
                    os.environ.update(environment)
                    self.assertEqual(plays_complete_matrix(), complete)
