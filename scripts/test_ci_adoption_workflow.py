"""Execute the checked-in cell gate and optional-helper shell, without CI/network."""
import itertools
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from test_verify import workflow_bash

ROOT = Path(__file__).resolve().parents[1]


def workflow_step(name):
    workflow = (ROOT / '.github/workflows/ci.yml').read_text()
    match = re.search(r'^      - name: ' + re.escape(name) + r'\n(.*?)(?=^      - |^  \w|\Z)', workflow, re.M | re.S)
    if match is None:
        raise AssertionError(f'Missing workflow step: {name}')
    return match[1]


class WorkflowAdoption(unittest.TestCase):
    def test_observers_helper_and_verifier_share_explicit_shell_environment(self):
        # The validator compares inherited PATH, not just explicit overrides.
        # Windows' default PowerShell differs from the Bash observation/helper.
        for name in ('Record checkout and resolved input observations before verification',
                     'Adopt independently matched premerge cell when available',
                     'Verify complete profile partition',
                     'Record checkout and resolved input observations after verification'):
            with self.subTest(step=name):
                self.assertEqual(re.findall(r'^        shell: (.+)$', workflow_step(name), re.M), ['bash'])

    def test_raw_cell_outcomes_only_accept_fresh_success_or_explicit_adoption(self):
        block = workflow_step('Require fresh verification or explicit matched adoption')
        self.assertIn('if: always()', block)
        self.assertIn('shell: bash', block)
        program = re.search(r"python - <<'PY_GATE'\n(.*?)\n          PY_GATE", block, re.S)[1]
        program = '\n'.join(line[10:] for line in program.splitlines())
        statuses = ('success', 'failure', 'cancelled', 'skipped', '')
        for verified, adopted, selected in itertools.product(statuses, statuses, ('true', 'false', '', 'unknown')):
            expected = (verified == 'success' and adopted in ('success', 'skipped') and selected in ('false', '')) or (
                verified == 'skipped' and adopted == 'success' and selected == 'true')
            with self.subTest(verified=verified, adopted=adopted, selected=selected), patch.dict(os.environ, {
                    'VERIFICATION_OUTCOME': verified, 'ADOPTION_OUTCOME': adopted, 'ADOPTED': selected}):
                with self.assertRaises(SystemExit) as result:
                    exec(compile(program, '<workflow cell gate>', 'exec'), {})
                self.assertEqual(result.exception.code == 0, expected)

    def test_shell_fixture_requires_native_shell_instead_of_skipping(self):
        namespace = {'__name__': 'shell_probe', '__file__': __file__}
        with patch.object(shutil, 'which', return_value=None):
            exec(compile(Path(__file__).read_text(encoding='utf-8'), __file__, 'exec'), namespace)
        method = namespace['WorkflowAdoption'].test_optional_helper_failures_fall_through_but_interrupts_stop
        self.assertFalse(getattr(method, '__unittest_skip__', False))

    def test_optional_helper_failures_fall_through_but_interrupts_stop(self):
        block = workflow_step('Adopt independently matched premerge cell when available')
        self.assertIn('shell: bash', block)
        program = '\n'.join(line[10:] for line in block.split('        run: |\n', 1)[1].splitlines())
        command = 'python scripts/ci_adoption.py --platform ${{ matrix.os }} --profile ${{ matrix.profile }} --partition ${{ matrix.partition }}'
        self.assertIn(command, program)
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'output'
            for status, token in ((0, 'adopted'), (0, 'unexpected'), (1, ''), (2, ''), (130, ''), (143, '')):
                script = program.replace(command, f"printf '%s\\n' '{token}'; exit {status}")
                output.write_text('')
                with self.subTest(status=status, token=token):
                    result = subprocess.run([workflow_bash(), '--noprofile', '--norc', '-eo', 'pipefail', '-c', script],
                                            env={**os.environ, 'GITHUB_OUTPUT': str(output)}, capture_output=True, timeout=5)
                    self.assertEqual(result.returncode, status if status in (130, 143) else 0, result.stderr)
                    expected = '' if status in (130, 143) else f'adopted={str(status == 0 and token == "adopted").lower()}\n'
                    self.assertEqual(output.read_text(), expected)

    def test_full_command_remains_default_and_adoption_is_main_push_only(self):
        optional = workflow_step('Adopt independently matched premerge cell when available')
        self.assertIn("if: github.event_name == 'push' && github.ref == 'refs/heads/main'", optional)
        self.assertIn("steps.rust_cache.outputs.cache-hit == 'true'", optional)
        self.assertNotIn('continue-on-error', optional)
        full = workflow_step('Verify complete profile partition')
        self.assertIn("if: success() && !cancelled() && steps.adoption.outputs.adopted != 'true'", full)
        self.assertIn('python scripts/verify.py full --ci-profile ${{ matrix.profile }} --ci-partition ${{ matrix.partition }}', full)
        self.assertNotIn('continue-on-error', full)


if __name__ == '__main__':
    unittest.main()
