"""Source-derived restricted weights are inputs, never an accepted deep policy."""
import copy
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch

from rogue_activation_candidate import INPUTS, candidate_manifest, conditional_bands, create_candidate
from rogue_roster_study import DEFAULT_RULES, PRODUCTION, ROOT, encounter_sources


class RogueActivationCandidateTests(unittest.TestCase):
    def test_all_twenty_six_depths_preserve_conditional_pinned_source_mass(self):
        # Independent exact enumeration of the pinned ten-way selector and its
        # five-way boundary redistribution, not the helper's embedded table.
        ladder = "KEBSHIROZLCQANYFTWPXUMVGJD"
        names = dict(zip("ESHIZQU", ("emu", "snake", "hobgoblin", "ice_monster",
                                    "zombie", "quagga", "black_unicorn"), strict=True))
        bands = conditional_bands()
        for depth in range(1, 27):
            mass = {"rogue." + name: Fraction() for name in names.values()}
            for offset in range(10):
                index = depth + offset - 6
                indices = range(5) if index < 0 else range(21, 26) if index > 25 else [index]
                for selected in indices:
                    if ladder[selected] in names:
                        mass["rogue." + names[ladder[selected]]] += Fraction(1, 10 * len(indices))
            eligible = {name: value for name, value in mass.items() if value}
            band = next(row for row in bands
                        if row["depth"]["minimum"] <= depth <= row["depth"]["maximum"])
            authored = {row["archetype"]: row["weight"] for row in band["choices"]}
            self.assertEqual(set(authored), set(eligible), depth)
            total_mass, total_weight = sum(eligible.values()), sum(authored.values())
            self.assertEqual({key: value / total_mass for key, value in eligible.items()},
                             {key: Fraction(value, total_weight) for key, value in authored.items()}, depth)
        self.assertEqual(max(row["depth"]["maximum"] for row in bands), 26)
        self.assertTrue(all(row["depth"]["kind"] == "finite" for row in bands))

    def test_candidate_has_four_attempts_unchanged_supplies_and_selected_entry_authority(self):
        original = encounter_sources()["manifest"]
        before = copy.deepcopy(original)
        candidate = candidate_manifest(original)
        activation = candidate["floor_activation"]
        self.assertEqual(activation["protagonist"], "selected_character")
        self.assertEqual(candidate["characters"], original["characters"])
        self.assertEqual(candidate["procedural_worlds"], original["procedural_worlds"])
        self.assertEqual(candidate["capabilities"], {"npc_stair_traversal": False})
        self.assertNotIn("objective", candidate)
        stages = activation["profiles"]["rogue.low_depth"]["stages"]
        self.assertEqual(stages[0]["count"], [4, 4])
        self.assertEqual(stages[0]["ai"], "rogue_hostile")
        self.assertTrue(stages[0]["avoid_protagonist_sight"])
        self.assertEqual(stages[1:], original["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][2:])
        self.assertEqual(sum(stage["count"][1] for stage in stages if stage["kind"] == "actors"), 4)
        self.assertEqual(original, before)

    def test_later_start_is_explicit_geometry_diagnostic_not_earned_growth(self):
        source = encounter_sources()["manifest"]
        for depth in (2, 5, 8, 17, 26):
            candidate = candidate_manifest(source, start_depth=depth)
            self.assertEqual({row["anchor"] for row in candidate["characters"]}, {f"dungeon/{depth}/up"})
            for original, selected in zip(source["characters"], candidate["characters"], strict=True):
                self.assertEqual(selected["creature"], original["creature"])
                self.assertEqual(selected["loadout"], original["loadout"])
        for depth in (0, 27, 1000000, True, "2"):
            with self.assertRaises(ValueError):
                candidate_manifest(source, start_depth=depth)

    def test_materialization_snapshots_inputs_and_calls_only_the_bounded_ordinary_compiler(self):
        imported = tomllib.loads((ROOT / PRODUCTION / "rules.toml").read_text())["import"]
        dependencies = {imported: hashlib.sha256(
            (ROOT / DEFAULT_RULES).read_bytes().replace(b"\r\n", b"\n")).hexdigest()}
        with tempfile.TemporaryDirectory() as directory:
            compiler = Path(directory) / "compiler"
            compiler.write_bytes(b"explicit test compiler")
            output = Path(directory) / "candidate"
            with patch("rogue_activation_candidate.resolve_with_dependencies",
                       return_value=({"id": "test-rules"}, dependencies)), patch(
                    "rogue_activation_candidate.subprocess.run",
                    return_value=subprocess.CompletedProcess([], 0, "", "")) as launched:
                result = create_candidate(output, compiler, start_depth=17)
            launched.assert_called_once()
            self.assertEqual(launched.call_args.args[0], [str(compiler), "validate", str(output / "package")])
            self.assertEqual(launched.call_args.kwargs["timeout"], 30)
            self.assertEqual(result["validated_packages"], 1)
            self.assertEqual(result["production_activation"], "not_selected")
            self.assertEqual(result["battle_evidence"], "not_run")
            self.assertEqual(result["calibration"], "not_assessed")
            for relative in INPUTS:
                self.assertEqual((output / "sources" / relative).read_bytes(), (ROOT / relative).read_bytes())
            self.assertEqual(json.loads((output / "inputs.json").read_text()), result)
            self.assertFalse((output / "partial.json").exists())
            with self.assertRaises(FileExistsError):
                create_candidate(output, compiler)

    def test_failed_compiler_or_stale_bundle_keeps_partial_evidence_and_never_claims_validation(self):
        imported = tomllib.loads((ROOT / PRODUCTION / "rules.toml").read_text())["import"]
        dependencies = {imported: hashlib.sha256(
            (ROOT / DEFAULT_RULES).read_bytes().replace(b"\r\n", b"\n")).hexdigest()}
        for stale in (False, True):
            with self.subTest(stale=stale), tempfile.TemporaryDirectory() as directory:
                compiler = Path(directory) / "compiler"
                compiler.write_bytes(b"explicit test compiler")
                output = Path(directory) / "candidate"
                with patch("rogue_activation_candidate.resolve_with_dependencies",
                           return_value=({"id": "test-rules"}, {} if stale else dependencies)), patch(
                        "rogue_activation_candidate.subprocess.run",
                        return_value=subprocess.CompletedProcess([], 2, "", "invalid candidate")) as launched:
                    with self.assertRaises(ValueError):
                        create_candidate(output, compiler)
                self.assertEqual(launched.call_count, 0 if stale else 1)
                self.assertFalse((output / "inputs.json").exists())
                partial = json.loads((output / "partial.json").read_text())
                self.assertEqual(partial["validated_packages"], 0)
                self.assertEqual(partial["generation_status"], "failed")
                self.assertTrue(partial["failure"]["message"])

    def test_changed_source_cannot_publish_a_validated_candidate_inventory(self):
        from arena_evaluation import digest
        imported = tomllib.loads((ROOT / PRODUCTION / "rules.toml").read_text())["import"]
        dependencies = {imported: hashlib.sha256(
            (ROOT / DEFAULT_RULES).read_bytes().replace(b"\r\n", b"\n")).hexdigest()}
        with tempfile.TemporaryDirectory() as directory:
            compiler = Path(directory) / "compiler"
            compiler.write_bytes(b"explicit test compiler")
            output = Path(directory) / "candidate"
            def fingerprint(path):
                return "changed source" if path == ROOT / DEFAULT_RULES else digest(path)
            with patch("rogue_activation_candidate.resolve_with_dependencies",
                       return_value=({"id": "test-rules"}, dependencies)), patch(
                    "rogue_activation_candidate.subprocess.run",
                    return_value=subprocess.CompletedProcess([], 0, "", "")), patch(
                    "rogue_activation_candidate.digest", side_effect=fingerprint):
                with self.assertRaisesRegex(ValueError, "source or compiler changed"):
                    create_candidate(output, compiler)
            self.assertFalse((output / "inputs.json").exists())
            partial = json.loads((output / "partial.json").read_text())
            self.assertEqual(partial["generation_status"], "failed")
            self.assertEqual(partial["validated_packages"], 1)


if __name__ == "__main__":
    unittest.main()
