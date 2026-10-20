"""Reviewed inputs preserve the eighteen-species diagnostic output and source closure."""
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
    candidate_manifest, conditional_bands, create_candidate, population_shape)
from rogue_activation_oracle import (reviewed_twenty_four_activation, reviewed_twenty_one_activation,
                                     reviewed_twenty_six_activation)
from rogue_roster_study import DEFAULT_RULES, PLAYABLE_SPECIES, PRODUCTION, ROOT, encounter_sources


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


def twenty_one_source():
    """Synthetic historical activation, explicitly separate from live production."""
    source = encounter_sources()["manifest"]
    source["floor_activation"] = reviewed_twenty_one_activation()
    return source


def twenty_four_source():
    """Synthetic historical policy; never relabel current production as 24 species."""
    source = encounter_sources()["manifest"]
    source["floor_activation"] = reviewed_twenty_four_activation()
    return source


def synthetic_eighteen_source(destination):
    """Explicit helper fixture, not a frozen measured production baseline."""
    shutil.copytree(ROOT / PRODUCTION, destination, ignore=shutil.ignore_patterns(".compiled"))
    manifest = candidate_manifest(preactivation_source())
    (destination / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8")
    return destination


class RogueActivationCandidateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        directory = tempfile.TemporaryDirectory()
        cls.addClassCleanup(directory.cleanup)
        cls.source_package = synthetic_eighteen_source(Path(directory.name) / "eighteen-source")
        cls.source_arguments = {"source_package": cls.source_package,
            "source_provenance": {"reference": "synthetic:eighteen-species-unit-fixture"}}

    def test_reviewed_twenty_one_input_preserves_eighteen_species_diagnostic_output(self):
        source = twenty_one_source()
        original = copy.deepcopy(source)
        self.assertEqual(population_shape(source), "reviewed_twenty_one_species_input")
        candidate = candidate_manifest(source)
        self.assertEqual(candidate, candidate_manifest(preactivation_source()))
        self.assertEqual(tomllib.loads(toml_source(candidate)), candidate_manifest(preactivation_source()))
        self.assertEqual(source, original)
        activation = candidate["floor_activation"]
        stages = activation["profiles"]["rogue.low_depth"]["stages"]
        self.assertEqual(len(stages), 4)
        self.assertNotIn("rogue.room_gold", [stage["id"] for stage in stages])
        self.assertEqual(set(activation["carried_item_pools"]), {"rogue.vampire_supplies"})
        self.assertEqual({choice["archetype"] for band in stages[0]["bands"] for choice in band["choices"]},
                         {"rogue." + name for name in PLAYABLE_SPECIES})

    def test_reviewed_twenty_one_input_rejects_unknown_or_mismatched_activation_rules(self):
        self.assert_reviewed_input_rejects_changes(twenty_one_source)

    def test_reviewed_twenty_four_input_rejects_unknown_or_mismatched_activation_rules(self):
        self.assert_reviewed_input_rejects_changes(twenty_four_source)

    def test_reviewed_twenty_six_input_rejects_unknown_or_mismatched_activation_rules(self):
        self.assert_reviewed_input_rejects_changes(lambda: encounter_sources()["manifest"])

    def assert_reviewed_input_rejects_changes(self, source_factory):
        def stages(activation):
            return activation["profiles"]["rogue.low_depth"]["stages"]
        def room_gold(activation):
            return next(stage for stage in stages(activation) if stage["id"] == "rogue.room_gold")
        def leprechaun(activation):
            return next(choice for band in stages(activation)[0]["bands"] for choice in band["choices"]
                        if choice["archetype"] == "rogue.leprechaun")
        mutations = {
            "unknown_species": lambda a: stages(a)[0]["bands"][0]["choices"][0].update(archetype="rogue.dragon"),
            "missing_species_choice": lambda a: stages(a)[0]["bands"][5]["choices"].pop(),
            "choice_order": lambda a: stages(a)[0]["bands"][5]["choices"].reverse(),
            "weight": lambda a: stages(a)[0]["bands"][0]["choices"][0].update(weight=2),
            "boolean_weight": lambda a: stages(a)[0]["bands"][0]["choices"][0].update(weight=True),
            "boundary": lambda a: stages(a)[0]["bands"][14]["depth"].update(maximum=16),
            "growth": lambda a: stages(a)[0]["depth_growth"].update(depth=27),
            "count": lambda a: stages(a)[0].update(count=[3, 3]),
            "missing_stock": lambda a: leprechaun(a).pop("carried_loot"),
            "stock_chance": lambda a: leprechaun(a)["carried_loot"]["chance"].update(denominator=2),
            "stock_pool": lambda a: leprechaun(a)["carried_loot"].update(pool="rogue.vampire_supplies"),
            "gold_quantity": lambda a: a["carried_item_pools"]["rogue.leprechaun_gold"]["choices"][0]["quantity"].update(per_depth=11),
            "unknown_pool": lambda a: a["carried_item_pools"].update(unreviewed={"version": 1, "choices": []}),
            "missing_gold_pool": lambda a: a["carried_item_pools"].pop("rogue.leprechaun_gold"),
            "room_chance": lambda a: room_gold(a)["chance"].update(numerator=2),
            "room_quantity": lambda a: room_gold(a)["quantity"].update(maximum_base=52),
            "room_kind": lambda a: room_gold(a).update(kind="items"),
            "missing_room_stage": lambda a: stages(a).remove(room_gold(a)),
            "extra_stage": lambda a: stages(a).append(copy.deepcopy(stages(a)[-1])),
            "stage_order": lambda a: stages(a).reverse(),
            "unknown_field": lambda a: room_gold(a).update(unreviewed=True),
        }
        for name, mutate in mutations.items():
            with self.subTest(mutation=name):
                source = source_factory()
                mutate(source["floor_activation"])
                with self.assertRaisesRegex(ValueError, "Review changed population profile"):
                    candidate_manifest(source)

    def test_reviewed_twenty_one_materialization_records_input_output_and_fresh_source_closure(self):
        self.assert_reviewed_materialization(twenty_one_source(), "twenty_one", "twenty-one",
                                            ("leprechaun", "nymph", "orc"))

    def test_reviewed_twenty_four_materialization_records_input_output_and_fresh_source_closure(self):
        self.assert_reviewed_materialization(twenty_four_source(), "twenty_four", "twenty-four",
                                            ("leprechaun", "nymph", "orc", "medusa", "phantom", "xeroc"))

    def test_reviewed_twenty_six_materialization_records_input_output_and_fresh_source_closure(self):
        self.assert_reviewed_materialization(encounter_sources()["manifest"], "twenty_six", "twenty-six",
            ("leprechaun", "nymph", "orc", "medusa", "phantom", "xeroc", "aquator", "dragon"))

    def assert_reviewed_materialization(self, source, shape, label, omitted):
        imported = tomllib.loads((ROOT / PRODUCTION / "rules.toml").read_text())["import"]
        dependencies = {imported: hashlib.sha256(
            (ROOT / DEFAULT_RULES).read_bytes().replace(b"\r\n", b"\n")).hexdigest()}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            supplied = root / ("explicit-" + label + "-source")
            shutil.copytree(ROOT / PRODUCTION, supplied, ignore=shutil.ignore_patterns(".compiled"))
            (supplied / "scenario.toml").write_text(toml_source(source), encoding="utf-8")
            compiler = root / "compiler"
            compiler.write_bytes(b"test compiler")
            output = root / "candidate"
            original_open = Path.open
            def without_live_production(path, *args, **kwargs):
                self.assertFalse(path.is_relative_to(ROOT / PRODUCTION))
                return original_open(path, *args, **kwargs)
            provenance = {"kind": "retained_snapshot", "reference": "synthetic:reviewed-" + label}
            with patch.object(Path, "open", without_live_production), patch(
                    "rogue_activation_candidate.resolve_with_dependencies",
                    return_value=({"id": "test-rules"}, dependencies)), patch(
                    "rogue_activation_candidate.subprocess.run",
                    return_value=subprocess.CompletedProcess([], 0, "", "")):
                result = create_candidate(output, compiler, source_package=supplied,
                                          source_provenance=provenance)
            self.assertEqual(result["source_population_shape"], "reviewed_" + shape + "_species_input")
            self.assertEqual(result["source_provenance"], provenance)
            species = sorted("rogue." + name for name in PLAYABLE_SPECIES)
            stages = ["rogue.eight_species_foes", "rogue.bandages", "rogue.rations", "rogue.restore_strength"]
            input_stages = [*stages, "rogue.room_gold"]
            if shape == "twenty_six":
                input_stages.extend(["rogue.magic_potions", "rogue.magic_scrolls", "rogue.magic_devices",
                                     "rogue.equipment", "rogue.equipment_scrolls", "rogue.ammunition", "rogue.rings"])
            self.assertEqual(result["input_population_scope"], {
                "species": sorted([*species, *("rogue." + name for name in omitted)]),
                "stage_ids": input_stages,
                "carried_item_pools": ["rogue.leprechaun_gold", "rogue.vampire_supplies"]})
            self.assertEqual(result["output_population_scope"], {
                "species": species, "stage_ids": stages, "carried_item_pools": ["rogue.vampire_supplies"]})
            self.assertEqual(result["population_scope"],
                             "conditional_eighteen_species_with_open_vampire_griffin_jabberwock_tail")
            for exclusion in ("not current " + label + "-species production",
                              "Leprechaun birth-Gold pool", "room-Gold stage", *(name.title() for name in omitted)):
                self.assertIn(exclusion, " ".join(result["limitations"]))
            if shape == "twenty_six":
                self.assertIn("finite magic-potion, magic-scroll and magic-device stages are omitted",
                              result["limitations"][0])
                self.assertIn("finite equipment, equipment-scroll, ammunition and ring stages are omitted", result["limitations"][0])
            for relative, fingerprint in result["source_package_inputs"].items():
                raw = (supplied / relative).read_bytes()
                self.assertEqual(fingerprint, hashlib.sha256(raw).hexdigest())
                self.assertEqual((output / "sources/package" / relative).read_bytes(), raw)
            self.assertIn("scripts/rogue_activation_oracle.py", TOOL_INPUTS)
            self.assertIn("scripts/scalar_wire.py", TOOL_INPUTS)
            for relative in TOOL_INPUTS:
                raw = (ROOT / relative).read_bytes()
                self.assertEqual(result["tool_inputs"][relative], hashlib.sha256(raw).hexdigest())
                self.assertEqual((output / "sources/tools" / relative).read_bytes(), raw)
            manifest = tomllib.loads((output / "package/scenario.toml").read_text())
            self.assertEqual(manifest, candidate_manifest(preactivation_source()))

    def test_current_twenty_six_input_preserves_eighteen_species_diagnostic_output(self):
        source = encounter_sources()["manifest"]
        original = copy.deepcopy(source)
        self.assertEqual(population_shape(source), "reviewed_twenty_six_species_input")
        self.assertEqual(source["floor_activation"], reviewed_twenty_six_activation())
        candidate = candidate_manifest(source)
        self.assertEqual(candidate, candidate_manifest(preactivation_source()))
        self.assertEqual(source, original)
        stages = candidate["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
        self.assertEqual(len(stages), 4)
        self.assertEqual({choice["archetype"] for band in stages[0]["bands"] for choice in band["choices"]},
                         {"rogue." + name for name in PLAYABLE_SPECIES})

    def test_twenty_six_rejects_finite_stock_count_choice_boundary_and_order_drift(self):
        self.assertEqual(population_shape(encounter_sources()["manifest"]),
                         "reviewed_twenty_six_species_input")
        for identity in ("rogue.magic_potions", "rogue.magic_scrolls", "rogue.magic_devices",
                         "rogue.equipment", "rogue.equipment_scrolls", "rogue.ammunition", "rogue.rings"):
            def stages(activation):
                return activation["profiles"]["rogue.low_depth"]["stages"]
            def stage(activation):
                return next(row for row in stages(activation) if row["id"] == identity)
            def band(activation):
                return stage(activation)["bands"][0]
            def move_before_room_gold(activation):
                selected = stage(activation)
                stages(activation).remove(selected)
                stages(activation).insert(4, selected)
            mutations = {
                "missing_stage": lambda a: stages(a).remove(stage(a)),
                "duplicate_stage": lambda a: stages(a).append(copy.deepcopy(stage(a))),
                "legacy_stream_order": move_before_room_gold,
                "identity": lambda a: stage(a).update(id="rogue.unreviewed_magic"),
                "version": lambda a: stage(a).update(version=2),
                "boolean_version": lambda a: stage(a).update(version=True),
                "kind": lambda a: stage(a).update(kind="room_items"),
                "missing_count": lambda a: stage(a).pop("count"),
                "variable_count": lambda a: stage(a).update(count=[0, 1]),
                "extra_attempt": lambda a: stage(a).update(count=[2, 2]),
                "boolean_count": lambda a: stage(a).update(count=[True, True]),
                "quantity": lambda a: stage(a).update(quantity=2),
                "boolean_quantity": lambda a: stage(a).update(quantity=True),
                "chance": lambda a: stage(a).update(chance={"numerator": 1, "denominator": 2}),
                "unknown_field": lambda a: stage(a).update(unreviewed=True),
                "late_start": lambda a: band(a)["depth"].update(minimum=2),
                "finite_cutoff": lambda a: band(a).update(depth={"kind": "finite", "minimum": 1, "maximum": 26}),
                "extra_band": lambda a: stage(a)["bands"].append(copy.deepcopy(band(a))),
                "missing_choice": lambda a: band(a)["choices"].pop(),
                "extra_choice": lambda a: band(a)["choices"].append(copy.deepcopy(band(a)["choices"][0])),
                "choice_order": lambda a: band(a)["choices"].reverse(),
                "archetype": lambda a: band(a)["choices"][0].update(archetype="rogue.restore_strength"),
                "weight": lambda a: band(a)["choices"][0].update(weight=2),
                "boolean_weight": lambda a: band(a)["choices"][0].update(weight=True),
            }
            if identity == "rogue.ammunition":
                mutations.pop("choice_order")  # Reversing one choice is unchanged.
            for name, mutate in mutations.items():
                with self.subTest(stage=identity, mutation=name):
                    source = encounter_sources()["manifest"]
                    mutate(source["floor_activation"])
                    with self.assertRaisesRegex(ValueError, "Review changed population profile"):
                        population_shape(source)

    def test_magic_stock_does_not_expand_historical_twenty_one_or_twenty_four_inputs(self):
        stages = reviewed_twenty_six_activation()["profiles"]["rogue.low_depth"]["stages"]
        # Retain the independent three-magic-stage rejection witness, then also
        # reject equipment-only and combined extensions of historical inputs.
        for extra in (stages[5:8], stages[8:], stages[5:]):
            for source_factory in (twenty_one_source, twenty_four_source):
                with self.subTest(source=source_factory.__name__, stages=[row["id"] for row in extra]):
                    source = source_factory()
                    source["floor_activation"]["profiles"]["rogue.low_depth"]["stages"].extend(copy.deepcopy(extra))
                    with self.assertRaisesRegex(ValueError, "Review changed population profile"):
                        population_shape(source)

    def test_twenty_six_rejects_a_d_stock_order_schedule_and_tail_drift(self):
        def bands(activation):
            return activation["profiles"]["rogue.low_depth"]["stages"][0]["bands"]
        def band(activation, depth):
            return next(row for row in bands(activation) if row["depth"]["minimum"] == depth)
        def choice(activation, depth, name):
            return next(row for row in band(activation, depth)["choices"] if row["archetype"] == "rogue." + name)
        mutations = {
            "missing_aquator": lambda a: band(a, 9)["choices"].pop(),
            "aquator_stock": lambda a: choice(a, 9, "aquator").update(
                carried_loot=copy.deepcopy(choice(a, 22, "dragon")["carried_loot"])),
            "dragon_miss_chance": lambda a: choice(a, 22, "dragon")["carried_loot"]["chance"].update(denominator=2),
            "dragon_pool": lambda a: choice(a, 22, "dragon")["carried_loot"].update(pool="rogue.leprechaun_gold"),
            "dragon_order": lambda a: band(a, 22)["choices"].reverse(),
            "depth31_weight": lambda a: choice(a, 31, "dragon").update(weight=9),
            "early_uniform_tail": lambda a: band(a, 32)["depth"].update(minimum=31),
            "missing_schedule": lambda a: choice(a, 22, "dragon").pop("advancement"),
            "early_purchase": lambda a: choice(a, 22, "dragon")["advancement"][0].update(ordinal=14),
            "free_rank": lambda a: choice(a, 22, "dragon")["advancement"][0].update(purchases=[]),
            "wrong_skill": lambda a: choice(a, 22, "dragon")["advancement"][0]["purchases"][0]["selection"].update(skill="athletics"),
            "sixth_rank": lambda a: choice(a, 22, "dragon")["advancement"].append({
                "ordinal": 30, "purchases": [{"pool": "advancement", "selection": {
                    "kind": "skill_rank", "skill": "breath"}}]}),
        }
        for name, mutate in mutations.items():
            with self.subTest(mutation=name):
                source = encounter_sources()["manifest"]
                mutate(source["floor_activation"])
                with self.assertRaisesRegex(ValueError, "Review changed population profile"):
                    population_shape(source)

    def test_reviewed_oracles_are_independent_fresh_values_with_distinct_rosters(self):
        before = reviewed_twenty_one_activation()
        current = reviewed_twenty_four_activation()
        historical = copy.deepcopy(current)
        self.assertNotEqual(before, current)
        current["profiles"]["rogue.low_depth"]["stages"][0]["bands"][-1]["choices"][-1]["carried_loot"]["chance"]["numerator"] = 99
        current["carried_item_pools"]["rogue.vampire_supplies"]["choices"][0]["weight"] = 99
        self.assertEqual(reviewed_twenty_one_activation(), before)
        self.assertEqual(reviewed_twenty_four_activation(), twenty_four_source()["floor_activation"])
        self.assertEqual(reviewed_twenty_six_activation(), encounter_sources()["manifest"]["floor_activation"])
        expanded = reviewed_twenty_six_activation()
        expanded["profiles"]["rogue.low_depth"]["stages"][-1]["bands"][0]["choices"][0]["weight"] = 99
        expanded["profiles"]["rogue.low_depth"]["stages"][1]["quantity"] = 99
        self.assertEqual(reviewed_twenty_one_activation(), before)
        self.assertEqual(reviewed_twenty_four_activation(), historical)
        self.assertEqual(reviewed_twenty_six_activation(), encounter_sources()["manifest"]["floor_activation"])
        for activation in (before, historical):
            self.assertEqual([row["id"] for row in activation["profiles"]["rogue.low_depth"]["stages"]],
                             ["rogue.eight_species_foes", "rogue.bandages", "rogue.rations",
                              "rogue.restore_strength", "rogue.room_gold"])
        bands = before["profiles"]["rogue.low_depth"]["stages"][0]["bands"]
        self.assert_pinned_source_mass(bands, roster_size=21)

    def test_twenty_four_rejects_perception_stock_boundary_order_and_open_tail_drift(self):
        def bands(activation):
            return activation["profiles"]["rogue.low_depth"]["stages"][0]["bands"]
        def band(activation, depth):
            return next(row for row in bands(activation) if row["depth"]["minimum"] <= depth and
                        (row["depth"]["kind"] == "open" or depth <= row["depth"]["maximum"]))
        def choice(activation, depth, name):
            return next(row for row in band(activation, depth)["choices"] if row["archetype"] == "rogue." + name)
        def remove_deep_medusas(activation):
            for row in bands(activation):
                if row["depth"]["minimum"] >= 28:
                    row["choices"] = [entry for entry in row["choices"] if entry["archetype"] != "rogue.medusa"]
        mutations = {
            "medusa_carry": lambda a: choice(a, 18, "medusa")["carried_loot"]["chance"].update(numerator=1),
            "xeroc_carry": lambda a: choice(a, 16, "xeroc")["carried_loot"]["chance"].update(denominator=5),
            "phantom_stock": lambda a: choice(a, 15, "phantom").update(carried_loot=copy.deepcopy(choice(a, 18, "medusa")["carried_loot"])),
            "duplicate_depth16": lambda a: bands(a).insert(15, copy.deepcopy(band(a, 16))),
            "overlap_depth16": lambda a: band(a, 15)["depth"].update(maximum=16),
            "missing_phantom": lambda a: band(a, 15)["choices"].pop(),
            "xeroc_too_early": lambda a: band(a, 15)["choices"].append(copy.deepcopy(choice(a, 16, "xeroc"))),
            "medusa_finite_cutoff": remove_deep_medusas,
            "deep_medusa_weight": lambda a: choice(a, 28, "medusa").update(weight=1),
            "deep_choice_order": lambda a: band(a, 31)["choices"].reverse(),
            "partial_twenty_three": lambda a: [row.update(choices=[entry for entry in row["choices"]
                if entry["archetype"] != "rogue.phantom"]) for row in bands(a)],
        }
        for name, mutate in mutations.items():
            with self.subTest(mutation=name):
                source = twenty_four_source()
                mutate(source["floor_activation"])
                with self.assertRaisesRegex(ValueError, "Review changed population profile"):
                    population_shape(source)

    def assert_pinned_source_mass(self, bands, *, roster_size=18):
        # Independent exact enumeration of the pinned ten-way selector and its
        # five-way boundary redistribution, not the helper's embedded table.
        ladder = "KEBSHIROZLCQANYFTWPXUMVGJD"
        names = dict(zip("ESHIRZCQYTWUVJ", ("emu", "snake", "hobgoblin", "ice_monster",
                                       "rattlesnake", "zombie", "centaur", "quagga", "yeti", "troll",
                                       "wraith", "black_unicorn", "vampire", "jabberwock"), strict=True))
        names.update(K="kestrel", B="bat", F="venus_flytrap", G="griffin")
        self.assertEqual(set(names.values()), set(PLAYABLE_SPECIES))
        self.assertIn(roster_size, (18, 21, 24, 26))
        if roster_size >= 21:
            names.update(L="leprechaun", N="nymph", O="orc")
        if roster_size >= 24:
            names.update(M="medusa", P="phantom", X="xeroc")
        if roster_size == 26:
            names.update(A="aquator", D="dragon")
        self.assertTrue(all(math.gcd(*(choice["weight"] for choice in band["choices"])) == 1
                            for band in bands))
        for depth in (*range(1, 36), 267, 268, 274, 275, 1_000_000):
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
        self.assertEqual(bands[-1]["depth"], {"kind": "open", "minimum": 32 if roster_size == 26 else 31})
        self.assertEqual([row["archetype"] for row in bands[-1]["choices"]],
                         ["rogue.vampire", "rogue.griffin", "rogue.jabberwock"] + (["rogue.medusa"] if roster_size >= 24 else [])
                         + (["rogue.dragon"] if roster_size == 26 else []))
        self.assertTrue(all(row["depth"]["kind"] == "finite" for row in bands[:-1]))
        self.assertEqual([row["depth"]["maximum"] + 1 for row in bands[:-1]],
                         [row["depth"]["minimum"] for row in bands[1:]])

    def test_all_bands_and_open_tail_preserve_conditional_pinned_source_mass(self):
        self.assert_pinned_source_mass(conditional_bands())

    def test_production_bands_match_independent_pinned_selector(self):
        manifest = encounter_sources()["manifest"]
        stages = manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]
        self.assert_pinned_source_mass(stages[0]["bands"], roster_size=26)

    def test_historical_twenty_four_bands_keep_their_conditional_mass_and_tail(self):
        stages = reviewed_twenty_four_activation()["profiles"]["rogue.low_depth"]["stages"]
        self.assert_pinned_source_mass(stages[0]["bands"], roster_size=24)

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

    def test_wraith_has_exact_reference_depth_boundaries_and_zero_carry(self):
        production = encounter_sources()["manifest"]["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]["bands"]
        for origin, bands in (("helper", conditional_bands()), ("production", production)):
            for depth, eligible in ((13, False), (14, True), (23, True), (24, False)):
                with self.subTest(origin=origin, depth=depth):
                    band = next(row for row in bands if row["depth"]["minimum"] <= depth
                                and (row["depth"]["kind"] == "open" or depth <= row["depth"]["maximum"]))
                    wraith = [row for row in band["choices"] if row["archetype"] == "rogue.wraith"]
                    self.assertEqual(len(wraith), int(eligible))
                    if eligible:
                        self.assertEqual(wraith[0], {"archetype": "rogue.wraith", "weight": 5 if depth == 23 else 1})

    def test_jabberwock_starts_at_21_and_mixed_tail_retains_boundary_redistribution(self):
        production = encounter_sources()["manifest"]["floor_activation"]["profiles"]["rogue.low_depth"]["stages"][0]["bands"]
        for origin, bands in (("helper", conditional_bands()), ("production", production)):
            for depth in (20, 21, 26, 27, 28, 29, 30, 31, 32, 267, 268, 274, 275):
                with self.subTest(origin=origin, depth=depth):
                    band = next(row for row in bands if row["depth"]["minimum"] <= depth
                                and (row["depth"]["kind"] == "open" or depth <= row["depth"]["maximum"]))
                    choices = {row["archetype"]: row for row in band["choices"]}
                    self.assertEqual("rogue.jabberwock" in choices, depth >= 21)
                    if depth >= 21:
                        self.assertEqual(choices["rogue.jabberwock"]["carried_loot"], {
                            "version": 1, "pool": "rogue.vampire_supplies",
                            "chance": {"numerator": 7, "denominator": 10}})
                    if depth >= 27:
                        expected = (7, 12, 12) if depth == 29 else (8, 8, 13) if depth == 30 else (1, 1, 1)
                        if origin == "production" and depth in (28, 31):
                            expected = (11, 11, 11) if depth == 28 else (9, 9, 9)
                        weights = dict(zip(("rogue.vampire", "rogue.griffin", "rogue.jabberwock"), expected, strict=True))
                        if origin == "production":
                            weights["rogue.medusa"] = {28: 6, 29: 7, 30: 8, 31: 9}.get(depth, 1)
                            weights["rogue.dragon"] = {28: 11, 29: 12, 30: 13, 31: 14}.get(depth, 1)
                        self.assertEqual({key: value["weight"] for key, value in choices.items()}, weights)

    def test_explicit_source_is_independent_of_live_production(self):
        imported = tomllib.loads((ROOT / PRODUCTION / "rules.toml").read_text())["import"]
        dependencies = {imported: hashlib.sha256(
            (ROOT / DEFAULT_RULES).read_bytes().replace(b"\r\n", b"\n")).hexdigest()}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            supplied = root / "retained-source"
            synthetic_eighteen_source(supplied)
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
        for original in (preactivation_source(), candidate_manifest(preactivation_source()),
                         twenty_one_source(), twenty_four_source(), encounter_sources()["manifest"]):
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
        original = candidate_manifest(preactivation_source())
        candidate = candidate_manifest(original)
        activation = candidate["floor_activation"]
        chances = {"rogue.centaur": (3, 20), "rogue.yeti": (3, 10), "rogue.troll": (1, 2), "rogue.vampire": (1, 5), "rogue.griffin": (1, 5), "rogue.jabberwock": (7, 10)}
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
        for name, numerator in (("centaur", 3), ("yeti", 3), ("troll", 1), ("vampire", 1), ("jabberwock", 7)):
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
        source = candidate_manifest(preactivation_source())
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
        source = candidate_manifest(preactivation_source())
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
                result = create_candidate(output, compiler, start_depth=17, **self.source_arguments)
            launched.assert_called_once_with(
                [str(resolved_compiler), "validate", str((output / "package").resolve())],
                capture_output=True, text=True, encoding="utf-8", timeout=30)
            self.assertEqual(result["validated_packages"], 1)
            self.assertEqual(result["format"], "tor-rogue-activation-inputs-v2")
            self.assertEqual(result["artifact_purpose"], "diagnostic_population_package")
            self.assertFalse(result["production_mutated"])
            self.assertEqual(result["battle_evidence"], "not_run")
            self.assertEqual(result["calibration"], "not_assessed")
            self.assertEqual(result["population_scope"], "conditional_eighteen_species_with_open_vampire_griffin_jabberwock_tail")
            self.assertEqual(result["input_population_scope"], result["output_population_scope"])
            self.assertEqual(result["output_population_scope"]["species"],
                             sorted("rogue." + name for name in PLAYABLE_SPECIES))
            self.assertEqual(result["depth_growth"], {"kind": "racial_after", "depth": 26})
            self.assertIn("provisional", " ".join(result["limitations"]))
            self.assertIn("capacity", " ".join(result["limitations"]))
            for relative in result["source_package_inputs"]:
                self.assertEqual((output / "sources/package" / relative).read_bytes(),
                                 (self.source_package / relative).read_bytes())
            for group, names in (("tools", TOOL_INPUTS), ("compiler", COMPILER_DEPENDENCY_INPUTS)):
                for relative in names:
                    self.assertEqual((output / "sources" / group / relative).read_bytes(),
                                     (ROOT / relative).read_bytes())
            self.assertEqual(json.loads((output / "inputs.json").read_text()), result)
            self.assertFalse((output / "partial.json").exists())
            with self.assertRaises(FileExistsError):
                create_candidate(output, compiler, **self.source_arguments)

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
                        create_candidate(output, compiler, **self.source_arguments)
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
            synthetic_eighteen_source(supplied)
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
                    create_candidate(output, compiler, **self.source_arguments)
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
                    create_candidate(output, compiler, **self.source_arguments)
            self.assertFalse((output / "inputs.json").exists())
            partial = json.loads((output / "partial.json").read_text())
            self.assertEqual(partial["generation_status"], "failed")
            self.assertEqual(partial["validated_packages"], 1)


if __name__ == "__main__":
    unittest.main()
