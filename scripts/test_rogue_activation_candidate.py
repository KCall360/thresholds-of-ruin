"""Explicit-source diagnostics preserve exact canonical or frozen E/S population contracts."""
import copy
from fractions import Fraction
import hashlib
import json
import math
import shutil
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch

from arena_matrix import toml_source
from rogue_activation_candidate import (COMPILER_DEPENDENCY_INPUTS, TOOL_INPUTS,
    candidate_manifest, conditional_bands, create_candidate)
from rogue_roster_study import DEFAULT_RULES, PLAYABLE_SPECIES, PRODUCTION, ROOT, encounter_sources


SOURCE_ARGUMENTS = {"source_package": ROOT / PRODUCTION,
                    "source_provenance": {"reference": "repository:scenarios/rogue-exploration"}}


def preactivation_source():
    """Synthetic old shape, not a retained measurement baseline or fixture copy."""
    source = encounter_sources()["manifest"]
    activation = source["floor_activation"]
    activation.pop("carried_item_pools", None)
    stages = []
    for name, archetype, kind, count in (
            ("emus", "emu", "actors", 2), ("snakes", "snake", "actors", 2),
            ("bandages", "clean_bandage", "items", 1), ("rations", "ration", "items", 1)):
        stage = {"id": "rogue." + name, "version": 1, "kind": kind, "count": [count, count],
                 "bands": [{"depth": {"kind": "open", "minimum": 1},
                            "choices": [{"archetype": "rogue." + archetype, "weight": 1}]}]}
        if kind == "actors":
            stage.update(ai="rogue_hostile", avoid_protagonist_sight=True)
        else:
            stage["quantity"] = 2 if name == "bandages" else 1
        stages.append(stage)
    activation["profiles"] = {"rogue.low_depth": {"version": 1, "stages": stages}}
    return source


class RogueActivationCandidateTests(unittest.TestCase):
    def assert_pinned_source_mass(self, bands):
        # Independent exact enumeration of the pinned ten-way selector and its
        # five-way boundary redistribution, not the helper's embedded table.
        ladder = "KEBSHIROZLCQANYFTWPXUMVGJD"
        names = dict(zip("ESHIRZCQYTUV", ("emu", "snake", "hobgoblin", "ice_monster",
                                       "rattlesnake", "zombie", "centaur", "quagga", "yeti", "troll",
                                       "black_unicorn", "vampire"), strict=True))
        self.assertEqual(set(names.values()), set(PLAYABLE_SPECIES))
        self.assertTrue(all(math.gcd(*(choice["weight"] for choice in band["choices"])) == 1
                            for band in bands))
        for depth in (*range(1, 36), 274, 275, 1_000_000):
            mass = {"rogue." + name: Fraction() for name in names.values()}
            for offset in range(10):
                index = depth + offset - 6
                indices = range(5) if index < 0 else range(21, 26) if index > 25 else [index]
                for selected in indices:
                    if ladder[selected] in names:
                        mass["rogue." + names[ladder[selected]]] += Fraction(1, 10 * len(indices))
            eligible = {name: value for name, value in mass.items() if value}
            matching = [row for row in bands if row["depth"]["minimum"] <= depth
                        and (row["depth"]["kind"] == "open" or depth <= row["depth"]["maximum"])]
            self.assertEqual(len(matching), 1, depth)
            band = matching[0]
            authored = {row["archetype"]: row["weight"] for row in band["choices"]}
            self.assertEqual(set(authored), set(eligible), depth)
            total_mass, total_weight = sum(eligible.values()), sum(authored.values())
            self.assertEqual({key: value / total_mass for key, value in eligible.items()},
                             {key: Fraction(value, total_weight) for key, value in authored.items()}, depth)
        self.assertEqual(bands[-1]["depth"], {"kind": "open", "minimum": 27})
        self.assertEqual([row["archetype"] for row in bands[-1]["choices"]], ["rogue.vampire"])
        self.assertTrue(all(row["depth"]["kind"] == "finite" for row in bands[:-1]))
        self.assertEqual([row["depth"]["maximum"] + 1 for row in bands[:-1]],
                         [row["depth"]["minimum"] for row in bands[1:]])

    def test_all_bands_and_open_tail_preserve_conditional_pinned_source_mass(self):
        self.assert_pinned_source_mass(conditional_bands())

    def test_production_bands_match_independent_pinned_selector(self):
        manifest = encounter_sources()["manifest"]
        stages = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
        self.assert_pinned_source_mass(stages[0]["bands"])

    def test_centaur_yeti_have_exact_pinned_selector_boundaries(self):
        production = encounter_sources()["manifest"]["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]["bands"]
        for bands in (conditional_bands(), production):
            for name, depths in (("centaur", ((6, False), (7, True), (16, True), (17, False))),
                                 ("yeti", ((10, False), (11, True), (20, True), (21, False)))):
                for depth, eligible in depths:
                    with self.subTest(species=name, depth=depth):
                        band = next(row for row in bands if row["depth"]["minimum"] <= depth
                                    and (row["depth"]["kind"] == "open" or depth <= row["depth"]["maximum"]))
                        self.assertEqual("rogue." + name in {row["archetype"] for row in band["choices"]}, eligible)

    def test_troll_has_exact_reference_depth_boundaries_and_half_carry_chance(self):
        production = encounter_sources()["manifest"]["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]["bands"]
        for origin, bands in (("helper", conditional_bands()), ("production", production)):
            for depth, eligible in ((12, False), (13, True), (22, True), (23, False)):
                with self.subTest(origin=origin, depth=depth):
                    band = next(row for row in bands if row["depth"]["minimum"] <= depth
                                and (row["depth"]["kind"] == "open" or depth <= row["depth"]["maximum"]))
                    troll = [row for row in band["choices"] if row["archetype"] == "rogue.troll"]
                    self.assertEqual(len(troll), int(eligible))
                    if eligible:
                        self.assertEqual(troll[0]["carried_loot"], {"version": 1,
                            "pool": "rogue.vampire_supplies", "chance": {"numerator": 1, "denominator": 2}})

    def test_explicit_source_is_independent_of_live_production(self):
        imported = tomllib.loads((ROOT / PRODUCTION / "rules.toml").read_text())["import"]
        dependencies = {imported: hashlib.sha256(
            (ROOT / DEFAULT_RULES).read_bytes().replace(b"\r\n", b"\n")).hexdigest()}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            supplied = root / "retained-source"
            shutil.copytree(ROOT / PRODUCTION, supplied, ignore=shutil.ignore_patterns(".compiled"))
            source = tomllib.loads((supplied / "scenario.toml").read_text())
            source["id"] = "retained-explicit-source"
            (supplied / "scenario.toml").write_text(toml_source(source))
            compiler = root / "compiler"
            compiler.write_bytes(b"test compiler")
            original_open = Path.open
            def without_live_production(path, *args, **kwargs):
                self.assertFalse(path.is_relative_to(ROOT / PRODUCTION),
                                 "Explicit source generation must not read live production")
                return original_open(path, *args, **kwargs)
            with patch.object(Path, "open", without_live_production), patch("rogue_activation_candidate.resolve_with_dependencies",
                       return_value=({"id": "test-rules"}, dependencies)), patch(
                    "rogue_activation_candidate.subprocess.run",
                    return_value=subprocess.CompletedProcess([], 0, "", "")):
                result = create_candidate(root / "candidate", compiler,
                    source_package=supplied,
                    source_provenance={"kind": "retained_snapshot", "reference": "test-source"})
            self.assertEqual(result["source_provenance"],
                             {"kind": "retained_snapshot", "reference": "test-source"})
            retained = root / "candidate/sources/package/scenario.toml"
            self.assertEqual(retained.read_bytes(), (supplied / "scenario.toml").read_bytes())
            self.assertEqual(result["source_package_inputs"]["scenario.toml"],
                             hashlib.sha256(retained.read_bytes()).hexdigest())
            self.assertNotIn("production_activation", result)

    def test_exact_preactivation_and_activated_sources_materialize_same_policy(self):
        old = preactivation_source()
        activated = candidate_manifest(old)
        activated["id"] = old["id"]
        before = copy.deepcopy(activated)
        self.assertEqual(candidate_manifest(activated), candidate_manifest(old))
        self.assertEqual(activated, before)

    def test_population_policy_rejects_boolean_numeric_aliases(self):
        for original in (preactivation_source(), candidate_manifest(preactivation_source())):
            source = copy.deepcopy(original)
            source["floor_activation"]["version"] = True
            with self.assertRaises(ValueError):
                candidate_manifest(source)
            source = copy.deepcopy(original)
            source["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]["version"] = True
            with self.assertRaises(ValueError):
                candidate_manifest(source)

    def test_partial_or_modified_activated_policy_is_rejected(self):
        old = preactivation_source()
        activated = candidate_manifest(old)
        mutations = [
            lambda a: a["profiles"]["rogue.low_depth"]["stages"][0].update(count=[3, 3]),
            lambda a: a["profiles"]["rogue.low_depth"]["stages"][0].update(id="changed_stream"),
            lambda a: a["profiles"]["rogue.low_depth"].update(version=2),
            lambda a: a["profiles"]["rogue.low_depth"]["stages"][0]["bands"][0]["choices"][0].update(weight=2),
            lambda a: a["profiles"]["rogue.low_depth"]["stages"][0].update(depth_growth={"kind": "racial_after", "depth": 27}),
            lambda a: a["profiles"]["rogue.low_depth"]["stages"][1].update(quantity=3),
            lambda a: a["profiles"]["rogue.low_depth"]["stages"].reverse(),
            lambda a: a["carried_item_pools"]["rogue.vampire_supplies"]["choices"][0].update(weight=2),
            lambda a: a["carried_item_pools"].pop("rogue.vampire_supplies"),
            lambda a: a["profiles"]["rogue.low_depth"]["stages"][0].update(unexpected="not reviewed"),
            lambda a: a["profiles"]["rogue.low_depth"]["stages"][0]["bands"][0]["choices"][0].update(unexpected=True),
            lambda a: a["carried_item_pools"]["rogue.vampire_supplies"].update(unexpected=True),
            lambda a: a["carried_item_pools"]["rogue.vampire_supplies"]["choices"][0].update(unexpected=True),
            lambda a: a["profiles"]["rogue.low_depth"]["stages"][0].update(ai="different_ai"),
            lambda a: a["profiles"]["rogue.low_depth"]["stages"][0].update(avoid_protagonist_sight=False),
            lambda a: a["profiles"]["rogue.low_depth"]["stages"][0]["bands"][-1]["choices"][0]["carried_loot"]["chance"].update(numerator=2),
            lambda a: a["profiles"]["rogue.low_depth"]["stages"].append(copy.deepcopy(a["profiles"]["rogue.low_depth"]["stages"][-1])),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(mutation=index):
                source = copy.deepcopy(activated)
                mutate(source["floor_activation"])
                with self.assertRaises(ValueError):
                    candidate_manifest(source)
        # The old source must also be exact: counts alone cannot bless changes.
        old["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]["bands"][0]["choices"][0]["weight"] = 2
        with self.assertRaises(ValueError):
            candidate_manifest(old)

    def test_candidate_has_four_attempts_unchanged_supplies_and_selected_entry_authority(self):
        original = preactivation_source()
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
        self.assertEqual(stages[0]["id"], "rogue.eight_species_foes")
        self.assertEqual(stages[0]["depth_growth"], {"kind": "racial_after", "depth": 26})
        self.assertEqual(stages[0]["ai"], "rogue_hostile")
        self.assertTrue(stages[0]["avoid_protagonist_sight"])
        self.assertEqual(stages[1:3], original["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][2:])
        self.assertEqual(sum(stage["count"][1] for stage in stages if stage["kind"] == "actors"), 4)
        self.assertEqual(original, before)

    def test_only_reference_carriers_receive_exact_finite_birth_stock(self):
        original = encounter_sources()["manifest"]
        candidate = candidate_manifest(original)
        activation = candidate["floor_activation"]
        chances = {"rogue.centaur": (3, 20), "rogue.yeti": (3, 10), "rogue.troll": (1, 2), "rogue.vampire": (1, 5)}
        self.assertEqual(activation["carried_item_pools"]["rogue.vampire_supplies"], {
            "version": 1, "choices": [
                {"archetype": "rogue.ration", "weight": 1, "quantity": 1},
                {"archetype": "rogue.clean_bandage", "weight": 1, "quantity": 1},
            ]})
        seen = set()
        for band in activation["profiles"]["rogue.low_depth"]["stages"][0]["bands"]:
            for choice in band["choices"]:
                seen.add(choice["archetype"])
                if choice["archetype"] in chances:
                    numerator, denominator = chances[choice["archetype"]]
                    self.assertEqual(choice["carried_loot"], {"version": 1,
                        "pool": "rogue.vampire_supplies",
                        "chance": {"numerator": numerator, "denominator": denominator}})
                else:
                    self.assertNotIn("carried_loot", choice)
        self.assertEqual(seen, {"rogue." + name for name in PLAYABLE_SPECIES})
        self.assertEqual(candidate["ai_profiles"], original["ai_profiles"])
        self.assertEqual(tomllib.loads(toml_source(candidate)), candidate)

    def test_candidate_rejects_unreviewed_stock_pool_ownership(self):
        for source in (preactivation_source(), candidate_manifest(preactivation_source())):
            source["floor_activation"].setdefault("carried_item_pools", {})["existing"] = {
                "version": 1, "choices": [{"archetype": "rogue.ration", "weight": 1, "quantity": 2}]}
            with self.assertRaisesRegex(ValueError, "pool"):
                candidate_manifest(source)

    def test_each_carrier_choice_and_generation_owns_its_stock_policy_copy(self):
        for name, numerator in (("centaur", 3), ("yeti", 3), ("troll", 1), ("vampire", 1)):
            with self.subTest(species=name):
                bands = conditional_bands()
                choices = [choice for band in bands for choice in band["choices"]
                           if choice["archetype"] == "rogue." + name]
                self.assertGreater(len(choices), 1)
                choices[0]["carried_loot"]["chance"]["numerator"] = 5
                self.assertTrue(all(choice["carried_loot"]["chance"]["numerator"] == numerator
                                    for choice in choices[1:]))
                self.assertTrue(all(choice["carried_loot"]["chance"]["numerator"] == numerator
                                    for band in conditional_bands() for choice in band["choices"]
                                    if choice["archetype"] == "rogue." + name))

    def test_later_start_is_explicit_geometry_diagnostic_not_earned_growth(self):
        source = encounter_sources()["manifest"]
        for depth in (2, 5, 8, 17, 26, 27, 274, 275, 1_000_000):
            candidate = candidate_manifest(source, start_depth=depth)
            self.assertEqual({row["anchor"] for row in candidate["characters"]}, {f"dungeon/{depth}/up"})
            for original, selected in zip(source["characters"], candidate["characters"], strict=True):
                self.assertEqual(selected["creature"], original["creature"])
                self.assertEqual(selected["loadout"], original["loadout"])
        for depth in (0, 1_000_001, True, "2"):
            with self.assertRaises(ValueError):
                candidate_manifest(source, start_depth=depth)

    def test_candidate_diagnostic_start_cannot_exceed_authored_world_domain(self):
        source = encounter_sources()["manifest"]
        source["procedural_worlds"]["dungeon"]["domain"]["max_depth"] = 300
        self.assertEqual(candidate_manifest(source, start_depth=300)["characters"][0]["anchor"],
                         "dungeon/300/up")
        with self.assertRaises(ValueError):
            candidate_manifest(source, start_depth=301)

    def test_source_inputs_are_explicit_and_provenance_is_required(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            compiler = root / "compiler"
            compiler.write_bytes(b"test compiler")
            with self.assertRaises(TypeError):
                create_candidate(root / "candidate", compiler)
            for provenance in (None, {}, {"reference": ""}, {"reference": 3}):
                with self.assertRaises(ValueError):
                    create_candidate(root / "candidate", compiler,
                        source_package=ROOT / PRODUCTION, source_provenance=provenance)
            self.assertFalse((root / "candidate").exists())

    def test_materialization_snapshots_inputs_and_calls_only_the_bounded_ordinary_compiler(self):
        imported = tomllib.loads((ROOT / PRODUCTION / "rules.toml").read_text())["import"]
        dependencies = {imported: hashlib.sha256(
            (ROOT / DEFAULT_RULES).read_bytes().replace(b"\r\n", b"\n")).hexdigest()}
        with tempfile.TemporaryDirectory() as directory:
            # Exercise a non-canonical spelling on every platform, without
            # needing symlink privileges (Windows temp paths may use 8.3 names).
            compiler_parent = Path(directory) / "compiler-parent"
            compiler_parent.mkdir()
            compiler = compiler_parent / ".." / "compiler"
            compiler.write_bytes(b"explicit test compiler")
            resolved_compiler = compiler.resolve(strict=True)
            self.assertNotEqual(compiler, resolved_compiler)
            self.assertTrue(compiler.samefile(resolved_compiler))
            output = compiler_parent / ".." / "candidate"
            self.assertNotEqual(output, output.resolve())
            with patch("rogue_activation_candidate.resolve_with_dependencies",
                       return_value=({"id": "test-rules"}, dependencies)), patch(
                    "rogue_activation_candidate.subprocess.run",
                    return_value=subprocess.CompletedProcess([], 0, "", "")) as launched:
                result = create_candidate(output, compiler, start_depth=17, **SOURCE_ARGUMENTS)
            launched.assert_called_once_with(
                [str(resolved_compiler), "validate", str((output / "package").resolve())],
                capture_output=True, text=True, encoding="utf-8", timeout=30)
            self.assertEqual(result["validated_packages"], 1)
            self.assertEqual(result["format"], "tor-rogue-activation-inputs-v2")
            self.assertEqual(result["artifact_purpose"], "diagnostic_population_package")
            self.assertFalse(result["production_mutated"])
            self.assertEqual(result["battle_evidence"], "not_run")
            self.assertEqual(result["calibration"], "not_assessed")
            self.assertEqual(result["population_scope"], "conditional_twelve_species_with_open_vampire_tail")
            self.assertEqual(result["depth_growth"], {"kind": "racial_after", "depth": 26})
            self.assertIn("provisional", " ".join(result["limitations"]))
            self.assertIn("capacity", " ".join(result["limitations"]))
            for relative in result["source_package_inputs"]:
                self.assertEqual((output / "sources/package" / relative).read_bytes(),
                                 (ROOT / PRODUCTION / relative).read_bytes())
            for group, names in (("tools", TOOL_INPUTS), ("compiler", COMPILER_DEPENDENCY_INPUTS)):
                for relative in names:
                    self.assertEqual((output / "sources" / group / relative).read_bytes(),
                                     (ROOT / relative).read_bytes())
            self.assertEqual(json.loads((output / "inputs.json").read_text()), result)
            self.assertFalse((output / "partial.json").exists())
            with self.assertRaises(FileExistsError):
                create_candidate(output, compiler, **SOURCE_ARGUMENTS)

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
                        create_candidate(output, compiler, **SOURCE_ARGUMENTS)
                self.assertEqual(launched.call_count, 0 if stale else 1)
                self.assertFalse((output / "inputs.json").exists())
                partial = json.loads((output / "partial.json").read_text())
                self.assertEqual(partial["validated_packages"], 0)
                self.assertEqual(partial["generation_status"], "failed")
                self.assertTrue(partial["failure"]["message"])

    def test_supplied_package_drift_cannot_publish_validated_inventory(self):
        imported = tomllib.loads((ROOT / PRODUCTION / "rules.toml").read_text())["import"]
        dependencies = {imported: hashlib.sha256(
            (ROOT / DEFAULT_RULES).read_bytes().replace(b"\r\n", b"\n")).hexdigest()}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            supplied = root / "source"
            shutil.copytree(ROOT / PRODUCTION, supplied, ignore=shutil.ignore_patterns(".compiled"))
            compiler = root / "compiler"
            compiler.write_bytes(b"test compiler")
            output = root / "candidate"
            def change_input(*args, **kwargs):
                with (supplied / "catalog/actors.toml").open("a") as stream:
                    stream.write("\n# changed after snapshot\n")
                return subprocess.CompletedProcess([], 0, "", "")
            with patch("rogue_activation_candidate.resolve_with_dependencies",
                       return_value=({"id": "test-rules"}, dependencies)), patch(
                    "rogue_activation_candidate.subprocess.run", side_effect=change_input):
                with self.assertRaisesRegex(ValueError, "source or compiler changed"):
                    create_candidate(output, compiler, source_package=supplied,
                                     source_provenance={"reference": "retained-test"})
            self.assertFalse((output / "inputs.json").exists())
            partial = json.loads((output / "partial.json").read_text())
            self.assertEqual(partial["generation_status"], "failed")
            self.assertEqual(partial["validated_packages"], 1)
            self.assertNotEqual((output / "sources/package/catalog/actors.toml").read_bytes(),
                                (supplied / "catalog/actors.toml").read_bytes())

    def test_compiler_drift_cannot_publish_validated_inventory(self):
        imported = tomllib.loads((ROOT / PRODUCTION / "rules.toml").read_text())["import"]
        dependencies = {imported: hashlib.sha256(
            (ROOT / DEFAULT_RULES).read_bytes().replace(b"\r\n", b"\n")).hexdigest()}
        with tempfile.TemporaryDirectory() as directory:
            compiler = Path(directory) / "compiler"
            compiler.write_bytes(b"test compiler")
            output = Path(directory) / "candidate"
            def change_compiler(*args, **kwargs):
                compiler.write_bytes(b"changed compiler")
                return subprocess.CompletedProcess([], 0, "", "")
            with patch("rogue_activation_candidate.resolve_with_dependencies",
                       return_value=({"id": "test-rules"}, dependencies)), patch(
                    "rogue_activation_candidate.subprocess.run", side_effect=change_compiler):
                with self.assertRaisesRegex(ValueError, "source or compiler changed"):
                    create_candidate(output, compiler, **SOURCE_ARGUMENTS)
            self.assertFalse((output / "inputs.json").exists())
            partial = json.loads((output / "partial.json").read_text())
            self.assertEqual(partial["generation_status"], "failed")
            self.assertEqual(partial["validated_packages"], 1)

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
                    create_candidate(output, compiler, **SOURCE_ARGUMENTS)
            self.assertFalse((output / "inputs.json").exists())
            partial = json.loads((output / "partial.json").read_text())
            self.assertEqual(partial["generation_status"], "failed")
            self.assertEqual(partial["validated_packages"], 1)


if __name__ == "__main__":
    unittest.main()
