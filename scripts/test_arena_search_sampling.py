"""Seeded TPE history can be reconstructed without private sampler state."""
import copy
import json
import unittest
from arena_search_sampling import initial_state, next_proposal, append_result, verify_state, seed_schedule, hash_json

SPACE = {"strength": {"type": "integer", "low": 0, "high": 5},
         "style": {"type": "categorical", "choices": ["heavy", "light"]}}


class SearchSamplingTests(unittest.TestCase):
    def test_json_continuation_after_tpe_startup_and_history_integrity(self):
        state = initial_state(SPACE, "42")
        saved = None
        for ordinal in range(25):
            params = next_proposal(state)
            state = append_result(state, params, -(params["strength"] - 3)**2,
                                  {"coverage": 1 if ordinal == 4 else 0})
            if ordinal == 11:
                saved = json.loads(json.dumps(state))
        for ordinal in range(12, 25):
            self.assertEqual(next_proposal(saved), state["records"][ordinal]["params"])
            saved = append_result(saved, state["records"][ordinal]["params"],
                                  state["records"][ordinal]["value"],
                                  state["records"][ordinal]["constraints"])
        self.assertEqual(saved, state)
        self.assertEqual(verify_state(state), next_proposal(state))
        corrupted = copy.deepcopy(state)
        corrupted["records"][-1]["value"] += 1
        with self.assertRaisesRegex(ValueError, "integrity"):
            verify_state(corrupted)
        rewritten = copy.deepcopy(state)
        rewritten["records"][0]["params"]["strength"] = (rewritten["records"][0]["params"]["strength"] + 1) % 6
        rewritten["records_sha256"] = hash_json(rewritten["records"])
        with self.assertRaisesRegex(ValueError, "next seeded proposal"):
            verify_state(rewritten)
        wrong_environment = copy.deepcopy(state)
        wrong_environment["environment"]["packages"]["optuna"] = "wrong"
        with self.assertRaisesRegex(ValueError, "environment"):
            next_proposal(wrong_environment)

    def test_seed_stages_are_disjoint_stable_and_full_defaults(self):
        values = seed_schedule("42")
        self.assertEqual({key: len(value) for key, value in values.items()},
                         {"training": 20, "screening": 20, "acceptance": 200})
        all_values = sum(values.values(), [])
        self.assertEqual(len(set(all_values)), 240)
        self.assertEqual(seed_schedule("42"), values)
        self.assertNotEqual(seed_schedule("43"), values)
        for value in all_values:
            self.assertEqual(str(int(value)), value)
            self.assertLess(int(value), 2**64)

    def test_invalid_spaces_results_and_unrequested_parameters_are_rejected(self):
        for space in [{}, {"x": {"type": "integer", "low": 5, "high": 1}},
                      {"x": {"type": "categorical", "choices": [True, 1]}},
                      {"x": {"type": "categorical", "choices": ["same", "same"]}}]:
            with self.subTest(space=space), self.assertRaises(ValueError):
                initial_state(space, "42")
        state = initial_state(SPACE, "42")
        params = next_proposal(state)
        original = copy.deepcopy(state)
        for value in [float("nan"), float("inf"), True]:
            with self.assertRaises(ValueError):
                append_result(state, params, value, {"coverage": 0})
        with self.assertRaises(ValueError):
            append_result(state, {**params, "unexpected": 1}, 0, {"coverage": 0})
        self.assertEqual(state, original)
