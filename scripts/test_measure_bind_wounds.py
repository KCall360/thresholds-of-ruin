"""Gameplay measurement validation and evidence handling, with no real builds."""
import contextlib
import copy
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

import measure_bind_wounds as measure


def records():
    """Independent small records covering every required control combination."""
    values = []
    for hd in (1, 6, 11):
        for stock in (0, 3):
            values.append({"case": "uncontested_rest", "hd": hd, "initial_bandages": stock,
                "initial_hp": 5, "maximum_hp": 8, "finite_uses": int(stock > 0),
                "explicit_heal": 2 if stock else 0, "natural_heal": 1 if stock else 3,
                "natural_period": 1200, "active_ticks_to_full": 1200,
                "rows": [{"tick": 1200, "hp": 8, "bandages": stock - int(stock > 0),
                          "natural_heal": 1 if stock else 3, "explicit_heal": 2 if stock else 0,
                          "spent": int(stock > 0), "cooldown": 0}]})
            values.append({"case": "hostile_pressure", "hd": hd, "initial_bandages": stock,
                "initial_hp": 5, "maximum_hp": 8, "finite_uses": 0, "explicit_heal": 0,
                "natural_heal": 0, "damage_received": 1, "active_ticks": 200, "dead": False,
                "final_hp": 4, "horizon_ticks": 200, "attempts": 0, "suspended_attempts": 0,
                "rows": [{"tick": 200, "hp": 4, "bandages": stock,
                          "preparation_active": False, "cooldown": 0, "recovery_elapsed_before": 100,
                          "active_tick_delta": 100, "recovery_period": 760,
                          "recovery_upper_bound": 200}]})
    return values


class BindWoundsValidation(unittest.TestCase):
    def test_accepts_all_controls_and_keeps_wasted_finite_use(self):
        values = records()
        measure.validate(values)
        wasted = next(row for row in values if row["case"] == "uncontested_rest"
                      and row["initial_bandages"] == 3)
        wasted.update(explicit_heal=0, natural_heal=3)
        measure.validate(values)
        self.assertEqual(wasted["finite_uses"], 1)

    def test_rejects_missing_duplicate_or_unknown_control(self):
        values = records()
        candidates = [values[:-1], values + [copy.deepcopy(values[0])]]
        unknown = copy.deepcopy(values)
        unknown[0]["hd"] = 12
        candidates.append(unknown)
        for candidate in candidates:
            with self.subTest(count=len(candidate)), self.assertRaises(ValueError):
                measure.validate(candidate)

    def test_rejects_boolean_hit_dice_instead_of_an_integer(self):
        values = records()
        values[0]["hd"] = True
        with self.assertRaises(ValueError):
            measure.validate(values)

    def test_rejects_boolean_finite_use_instead_of_an_integer(self):
        values = records()
        values[2]["finite_uses"] = True
        with self.assertRaises(ValueError):
            measure.validate(values)

    def test_rejects_unknown_summary_fields(self):
        values = records()
        values[0]["unverified_adjustment"] = 1
        with self.assertRaises(ValueError):
            measure.validate(values)

    def test_rejects_unknown_boundary_fields(self):
        values = records()
        values[0]["rows"][0]["hidden_heal"] = 1
        with self.assertRaises(ValueError):
            measure.validate(values)

    def test_rejects_inconsistent_heal_spend_and_empty_observations(self):
        for field, value in (("finite_uses", -1), ("finite_uses", 1),
                             ("explicit_heal", 1), ("natural_heal", 2), ("rows", [])):
            values = records()
            values[0][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                measure.validate(values)
        values = records()
        values[2]["finite_uses"] = 4
        with self.assertRaises(ValueError):
            measure.validate(values)

    def test_rejects_pressure_without_real_damage_or_with_unreconciled_healing(self):
        for field, value in (("damage_received", 0), ("natural_heal", 1)):
            values = records()
            values[1][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                measure.validate(values)

    def test_rejects_missing_crossed_or_fabricated_pressure_bound(self):
        for mutation in ("missing", "crossed", "mismatch", "negative", "boolean"):
            values = records()
            boundary = values[1]["rows"][0]
            if mutation == "missing":
                del boundary["recovery_upper_bound"]
            elif mutation == "crossed":
                boundary["recovery_period"] = 200
            elif mutation == "mismatch":
                boundary["recovery_upper_bound"] = 199
            elif mutation == "negative":
                boundary["active_tick_delta"] = -1
            else:
                boundary["active_tick_delta"] = True
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                measure.validate(values)


class BindWoundsRunner(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        patch = mock.patch.object(measure, "ROOT", self.root)
        patch.start()
        self.addCleanup(patch.stop)

    def test_output_path_only_allows_new_local_evidence_directory(self):
        alias_parent = self.root / "alias-parent"
        alias_parent.mkdir()
        # Exercise a noncanonical root on every platform, without symlink privileges.
        for index, root in enumerate((self.root, alias_parent / "..")):
            with self.subTest(root=root), mock.patch.object(measure, "ROOT", root):
                destination = root / ".local" / f"new-run-{index}"
                self.assertEqual(measure.output_path(str(destination)), destination.resolve())
                # Both operands must be canonical; Windows may expand temporary aliases.
                self.assertTrue(
                    measure.output_path(None).is_relative_to((root / ".local").resolve()))
                for bad in (root / "outside", root / ".local" / ".." / "outside"):
                    with self.subTest(path=bad), self.assertRaises(ValueError):
                        measure.output_path(str(bad))
                destination.mkdir(parents=True)
                (destination / "prior-evidence").write_text("keep", encoding="utf-8")
                with self.assertRaises(ValueError):
                    measure.output_path(str(destination))
                self.assertEqual(
                    (destination / "prior-evidence").read_text(encoding="utf-8"), "keep")

    def run_main(self, destination, fake_run, profile="release"):
        output = io.StringIO()
        args = ["measure_bind_wounds.py", "--profile", profile, "--output", str(destination)]
        with mock.patch.object(measure.sys, "argv", args), \
                mock.patch.object(measure, "capture", return_value="test-source-metadata"), \
                mock.patch.object(measure.platform, "platform", return_value="test-host"), \
                mock.patch.object(measure.subprocess, "run", side_effect=fake_run) as called, \
                contextlib.redirect_stdout(output):
            result = measure.main()
        return result, output.getvalue(), called

    def test_success_records_source_summary_hash_and_exact_fixture_command(self):
        destination = self.root / ".local" / "success"
        values = records()

        def run(command, **kwargs):
            kwargs["stdout"].write("mocked test passed\n")
            path = Path(kwargs["env"]["TOR_BIND_WOUNDS_MEASUREMENTS"])
            path.write_text("".join(json.dumps(row) + "\n" for row in values), encoding="utf-8")
            self.assertEqual(kwargs["cwd"], self.root)
            self.assertEqual(kwargs["stderr"], subprocess.STDOUT)
            return subprocess.CompletedProcess(command, 0)

        result, output, called = self.run_main(destination, run, profile="debug")
        self.assertEqual(result, 0)
        command = called.call_args.args[0]
        self.assertNotIn("--release", command)
        self.assertEqual(command[-4:], [measure.TEST, "--", "--exact", "--nocapture"])
        self.assertIn("PASS:", output)
        self.assertEqual(output.count("stock="), 12)
        metadata = json.loads((destination / "metadata.json").read_text(encoding="utf-8"))
        self.assertEqual(metadata["returncode"], 0)
        self.assertEqual(metadata["commit"], "test-source-metadata")
        expected = measure.hashlib.sha256((destination / "outcomes.jsonl").read_bytes()).hexdigest()
        self.assertEqual(metadata["outcomes_sha256"], expected)
        summary = json.loads((destination / "summary.json").read_text(encoding="utf-8"))
        self.assertEqual(len(summary), 12)
        self.assertTrue(all("rows" not in row for row in summary))
        self.assertEqual((destination / "test.log").read_text(encoding="utf-8"), "mocked test passed\n")

    def test_failed_subprocess_keeps_evidence_and_reports_failure_without_summary(self):
        destination = self.root / ".local" / "failed"

        def run(command, **kwargs):
            kwargs["stdout"].write("mocked assertion failure\n")
            self.assertIn("--release", command)
            return subprocess.CompletedProcess(command, 17)

        result, output, called = self.run_main(destination, run)
        self.assertEqual(result, 17)
        called.assert_called_once()
        self.assertIn("FAIL:", output)
        self.assertNotIn("PASS", output)
        self.assertNotIn("stock=", output)
        metadata = json.loads((destination / "metadata.json").read_text(encoding="utf-8"))
        self.assertEqual(metadata["returncode"], 17)
        self.assertNotIn("outcomes_sha256", metadata)
        self.assertFalse((destination / "summary.json").exists())
        self.assertEqual((destination / "test.log").read_text(encoding="utf-8"), "mocked assertion failure\n")


if __name__ == "__main__":
    unittest.main()
