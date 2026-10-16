"""Expectation syntax is a closed class, never a dynamic dependency guess."""
import ast
import copy
import json
import re
from pathlib import Path
import subprocess
import unittest

import ci_reuse_policy as policy


class ExpectedExpressions(unittest.TestCase):
    def value(self, text, names=None):
        return policy.pure(ast.parse(text, mode='eval').body, names or {})

    def test_closed_builtin_values_and_real_roster_expression(self):
        names = {'PLAYABLE_SPECIES': ('emu', 'quagga')}
        self.assertEqual(self.value('[f"{species}-hd1" for species in PLAYABLE_SPECIES]', names),
                         ['emu-hd1', 'quagga-hd1'])
        self.assertEqual(self.value('len(PLAYABLE_SPECIES)', names), 2)
        self.assertEqual(self.value('{"a": [1, None, True]}["a"]'), [1, None, True])

    def test_comprehension_expansion_has_a_total_work_budget(self):
        with self.assertRaises(ValueError):
            self.value('[[y for y in ITEMS] for x in ITEMS]', {'ITEMS': tuple(range(100))})

    def test_shared_value_dag_has_an_expanded_size_budget(self):
        value = ['x']
        for _ in range(13): value = [value, value]
        with self.assertRaises(ValueError): self.value('VALUE', {'VALUE': value})

    def test_unreachable_comprehension_bodies_still_require_closed_syntax(self):
        for expression in ('[arbitrary_call() for x in ()]', '[x.bad for x in ()]',
                           '[unknown_name for x in ()]', '[arbitrary_call() for x in (1,) if False]'):
            with self.subTest(expression=expression), self.assertRaises(ValueError): self.value(expression)

    def test_unknown_syntax_and_custom_values_reject(self):
        class Evil(tuple):
            pass
        variants = [('obj.value', {'obj': (1,)}), ('obj[0]', {'obj': Evil((1,))}),
                    ('str(1)', {}), ('len(*ITEMS)', {'ITEMS': ()}), ('(x := 1)', {}),
                    ('(x for x in ITEMS)', {'ITEMS': (1,)}), ('f"{x!r}"', {'x': 1}),
                    ('f"{x:{width}}"', {'x': 1, 'width': 2}), ('len(ITEMS)', {'len': 1, 'ITEMS': ()})]
        for expression, names in variants:
            with self.subTest(expression=expression), self.assertRaises((ValueError, TypeError, KeyError)):
                self.value(expression, names)


class LeafPolicy(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root = Path(__file__).resolve().parent.parent
        cls.registry = json.loads((cls.root / 'scripts/ci_reuse_registry.json').read_text())
        cls.path = 'scripts/test_rogue_playable_study_process.py'
        cls.original = (cls.root / cls.path).read_bytes()
        # Keep a legacy literal fixture without depending on historical Git objects.
        cls.original = cls.original.replace(b'from rogue_roster_package import PLAYABLE_SPECIES\n', b'')
        cls.original = re.sub(rb'^        expected_cases = .*\n', b'', cls.original, flags=re.MULTILINE)
        cls.original = cls.original.replace(b'len(expected_cases)', b'12').replace(
            b'[case for case in expected_cases if case != "emu-hd1"]', b'["snake-hd1"]')
        cls.entry = cls.registry['leaves'][cls.path]
        cls.immutable = {'PLAYABLE_SPECIES': ('emu', 'quagga')}

    def normalized(self, source):
        return policy.normalize_leaf(source.encode() if isinstance(source, str) else source, self.entry, self.immutable)

    def corrected(self):
        text = self.original.decode()
        text = text.replace('from process_harness import ProcessTestCase, ROOT',
                            'from process_harness import ProcessTestCase, ROOT\nfrom rogue_roster_package import PLAYABLE_SPECIES')
        text = text.replace('self.assertEqual(inventory["required_cases"], 12)',
                            'expected_cases = [f"{species}-hd1" for species in PLAYABLE_SPECIES]\n'
                            '        self.assertEqual(inventory["required_cases"], len(expected_cases))')
        begin = text.index('self.assertEqual(inventory["missing_cases"],')
        end = text.index('self.assertEqual(inventory["battle_evidence"]', begin)
        return text[:begin] + 'self.assertEqual(inventory["missing_cases"], [case for case in expected_cases if case != "emu-hd1"])\n        ' + text[end:]

    def test_real_import_and_comprehension_repair_matches_reviewed_skeleton(self):
        self.assertEqual(self.normalized(self.corrected())['skeleton'], self.entry['skeleton'])
        self.assertEqual(self.normalized(self.original)['skeleton'], self.entry['skeleton'])
        self.assertEqual(self.normalized((self.root / self.path).read_bytes())['skeleton'], self.entry['skeleton'])

    def test_shared_acceptance_and_protected_guard_line_is_ineligible(self):
        source = self.original.decode().replace(
            'self.assertEqual(inventory["compiler_sha256"], digest(compiler))\n        self.assertEqual(inventory["validated_packages"], 4)',
            'self.assertEqual(inventory["compiler_sha256"], digest(compiler)); self.assertEqual(inventory["validated_packages"], 4)')
        self.assertNotEqual(source, self.original.decode())
        with self.assertRaises(ValueError): self.normalized(source)

    def test_duplicate_optional_import_and_missing_provider_binding_reject(self):
        source = self.corrected()
        imported = 'from rogue_roster_package import PLAYABLE_SPECIES'
        with self.assertRaises(ValueError): self.normalized(source.replace(imported, imported + '\n' + imported))
        try: result = self.normalized(source.replace(imported, ''))
        except ValueError: pass
        else: self.assertNotEqual(result['skeleton'], self.entry['skeleton'])

    def test_future_literal_expectation_change_does_not_need_per_fix_registry(self):
        changed = self.original.replace(b'inventory["required_cases"], 12', b'inventory["required_cases"], 19')
        self.assertEqual(self.normalized(changed)['skeleton'], self.entry['skeleton'])

    def test_setup_guard_action_actual_operand_and_unknown_import_stay_fixed(self):
        variants = [self.original.decode().replace('result.returncode, 0', 'result.returncode, 1'),
                    self.original.decode().replace('timeout=150', 'timeout=151'),
                    self.original.decode().replace('inventory["required_cases"]', 'inventory["validated_packages"]'),
                    self.original.decode().replace('import json', 'import json\nimport random')]
        for source in variants:
            with self.subTest(source=source[:40]):
                self.assertNotEqual(self.normalized(source)['skeleton'], self.entry['skeleton'])

    def test_expected_locals_cannot_escape_shadow_or_run_effects(self):
        good = self.corrected()
        variants = [good.replace('inventory["required_cases"], len(expected_cases)', 'expected_cases, len(expected_cases)'),
                    good.replace('expected_cases = [', 'expected_cases = side_effect()\n        hidden = ['),
                    good.replace('expected_cases = [', 'len = lambda x: 0\n        expected_cases = ['),
                    good.replace('expected_cases = [', 'expected_cases = []\n        expected_cases = [')]
        for source in variants:
            with self.subTest(source=source[:40]):
                try: result = self.normalized(source)
                except (ValueError, TypeError, KeyError): continue
                self.assertNotEqual(result['skeleton'], self.entry['skeleton'])

    def test_fixture_decorator_default_and_reflection_are_not_allowed_edits(self):
        original = self.original.decode()
        variants = [original.replace('def study_round_trip(self, reference):', 'def study_round_trip(self, reference="x"):'),
                    original.replace('    def study_round_trip', '    @decorator\n    def study_round_trip'),
                    original.replace('inputs = self.directory', 'locals()\n        inputs = self.directory'),
                    original.replace('class RoguePlayableStudyProcesses(ProcessTestCase):',
                                     'class RoguePlayableStudyProcesses(ProcessTestCase):\n    def setUp(self): pass')]
        for source in variants:
            with self.subTest(source=source[:40]):
                try: result = self.normalized(source)
                except ValueError: continue
                self.assertNotEqual(result['skeleton'], self.entry['skeleton'])

    def test_non_utf8_coding_cookie_cannot_hide_executable_source(self):
        escaped = b'# coding: utf-7\n# +AAo-side_effect()\n'
        for source in (self.original, (self.root / 'scripts/rogue_roster_package.py').read_bytes()):
            with self.assertRaises(ValueError): policy.syntax(escaped + source)

    def test_multiline_guard_cannot_share_an_eligible_statement_line(self):
        source = self.original.decode().replace(
            'self.assertEqual(inventory["compiler_sha256"], digest(compiler))\n        self.assertEqual(inventory["validated_packages"], 4)',
            'self.assertEqual(\n            inventory["compiler_sha256"], digest(compiler)); self.assertEqual(inventory["validated_packages"], 4)')
        self.assertNotEqual(source, self.original.decode())
        with self.assertRaises(ValueError): self.normalized(source)

    def test_other_registered_leaves_support_expectations_but_protect_actions(self):
        cases = [('scripts/test_rogue_roster_process.py', '[0, 1]', '[0, 1, 2]', 'seed=42', 'seed=43'),
                 ('scripts/test_rogue_chill_process.py', 'int(bandage["quantity"]), 2',
                  'int(bandage["quantity"]), 3', '\"--actor\", \"2\"', '\"--actor\", \"3\"')]
        for path, old, new, action, changed in cases:
            entry = self.registry['leaves'][path]
            source = (self.root / path).read_text()
            immutable = {'SPECIES': ('emu', 'quagga')}
            self.assertIn(old, source); self.assertIn(action, source)
            expected = policy.normalize_leaf(source.replace(old, new).encode(), entry, immutable)
            self.assertEqual(expected['skeleton'], entry['skeleton'])
            acted = policy.normalize_leaf(source.replace(action, changed).encode(), entry, immutable)
            self.assertNotEqual(acted['skeleton'], entry['skeleton'])

    def test_provider_allows_only_literal_tuple_data_drift(self):
        raw = (self.root / 'scripts/rogue_roster_package.py').read_bytes()
        values, shape = policy.immutable_provider(raw)
        changed = raw.replace(b'"emu", "snake"', b'"emu", "new_foe", "snake"')
        self.assertNotEqual(values, policy.immutable_provider(changed)[0])
        self.assertEqual(shape, policy.immutable_provider(changed)[1])
        self.assertNotEqual(shape, policy.immutable_provider(raw.replace(b'import copy', b'import copy\nimport random'))[1])

    def test_exact_tree_and_audit_pins_reject_drift_even_on_both_sides(self):
        output = subprocess.check_output(['git', 'ls-tree', '-rz', 'HEAD'], cwd=self.root)
        source = {}
        for row in output.split(b'\0'):
            if row:
                metadata, path = row.split(b'\t'); mode, kind, digest = metadata.decode().split()
                self.assertEqual(kind, 'blob')
                source[path.decode()] = [mode, digest]
        target = copy.deepcopy(source); target[self.path][1] = 'f' * 40
        read = lambda path: (self.root / path).read_bytes()
        corrected = lambda path: self.corrected().encode() if path == self.path else read(path)
        self.assertIn(self.path, policy.validate_trees(source, target, read, corrected))
        for change in ('owner', 'addition', 'nonleaf', 'mode', 'symlink'):
            before, after = copy.deepcopy(source), copy.deepcopy(target)
            if change == 'owner': before['scripts/rogue_earned_route_process.py'][1] = after['scripts/rogue_earned_route_process.py'][1] = 'e' * 40
            elif change == 'addition': after['scripts/unknown.py'] = ['100644', 'e' * 40]
            elif change == 'nonleaf': after['Cargo.toml'][1] = 'e' * 40
            elif change == 'symlink': before['Cargo.toml'][0] = after['Cargo.toml'][0] = '120000'
            else: after[self.path][0] = '100755'
            with self.subTest(change=change), self.assertRaises(ValueError):
                policy.validate_trees(before, after, read, corrected)


if __name__ == '__main__':
    unittest.main()
