import unittest
from model_performance_report import CASES, PHASES, HAIR_PHASES, validate


def fixture():
    return [{"version": 4, "case": case, "sample": sample, "seed": 7,
             **{phase: [0.2] * 30 for phase in PHASES},
             **{phase: [0.02] * 30 for phase in HAIR_PHASES},
             "pose_update_ms": [1.0] * 30, "motion_frames": 30, "hz": 50, "changed_pose_frames": 20,
             "cold_spawn_ms": 10.0, "warm_spawn_ms": 2.0,
             "cold_meshes": 20, "warm_added_meshes": 2,
             "cold_mesh_bytes": 2000, "warm_added_mesh_bytes": 200,
             "cold_entities": 50, "warm_entities": 50}
            for case in CASES for sample in range(3)]


class ModelReport(unittest.TestCase):
    def test_complete_repeatable_workload(self):
        self.assertEqual(set(validate(fixture())), set(CASES))

    def test_incomplete_duplicate_and_reordered_reports(self):
        rows = fixture()
        for invalid in (rows[:-1], rows + [rows[0]], list(reversed(rows))):
            with self.subTest(rows=len(invalid)), self.assertRaises(ValueError):
                validate(invalid)

    def test_malformed_samples_and_missing_reuse(self):
        for field, value in [("version", 1), ("hz", 0), ("motion_frames", 29),
                             ("pose_update_ms", [1.0] * 29), ("pose_update_ms", [float("nan")] * 30),
                             ("changed_pose_frames", 0), ("changed_pose_frames", "20"), ("seed", 8), ("sample", False),
                             ("cold_spawn_ms", float("nan")), ("warm_spawn_ms", -1),
                             ("cold_meshes", 0), ("cold_entities", True),
                             ("warm_entities", 51), ("warm_added_meshes", 20),
                             ("warm_added_mesh_bytes", 2000)]:
            rows = fixture()
            rows[0][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate(rows)
        rows = fixture()
        rows[1]["cold_meshes"] += 1
        with self.assertRaisesRegex(ValueError, "counts changed"):
            validate(rows)
        rows = fixture()
        del rows[0]["warm_entities"]
        with self.assertRaises(ValueError):
            validate(rows)

    def test_phase_timings_are_complete_finite_and_within_total(self):
        for phase in (*PHASES, *HAIR_PHASES):
            for invalid in ([0.2] * 29, [float("inf")] * 30, [-1] * 30, [True] * 30, [2.0] * 30):
                rows = fixture()
                rows[0][phase] = invalid
                with self.subTest(phase=phase, invalid=invalid[0]), self.assertRaises(ValueError):
                    validate(rows)
        rows = fixture()
        rows[0]["version"] = 3
        with self.assertRaises(ValueError):
            validate(rows)

    def test_hair_subphases_fit_inside_hair_time(self):
        rows = fixture()
        hair = ("hair_controls_ms", "hair_deform_ms", "hair_constraints_ms", "hair_mesh_ms")
        for row in rows:
            row["version"] = 4
            row.update({phase: [0.02] * 30 for phase in hair})
        self.assertEqual(set(validate(rows)), set(CASES))
        for phase in hair:
            broken = [dict(row) for row in rows]
            broken[0][phase] = [0.3] * 30
            with self.subTest(phase=phase), self.assertRaisesRegex(ValueError, "Hair subphases"):
                validate(broken)

        for phase in hair:
            broken = [dict(row) for row in rows]
            active = next(row for row in broken if row["case"] == "nymph-princess-cloak")
            active[phase] = [0.0] * 30
            with self.subTest(phase=phase), self.assertRaisesRegex(ValueError, "active hair"):
                validate(broken)

    def test_harness_routes_and_extracts_the_model_workload(self):
        import perf_compare
        unit = perf_compare.parse_cases(["models:human-sylvan-plate"], 1)[0]
        self.assertEqual(unit.spec.manifest, "crates/client-3d/Cargo.toml")
        timings, counts, version = perf_compare.extract(unit, fixture())
        self.assertEqual(version, 4)
        self.assertEqual(timings[CASES[0]]["warm_spawn_ms"], [2.0] * 3)
        self.assertEqual(counts[CASES[0]]["cold_entities"], 150)
