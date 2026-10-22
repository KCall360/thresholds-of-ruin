"""Fresh workflow execution replaces retired test-pass adoption plumbing.

Retired pass-adoption code has been removed. These integration contracts prove
the active workflow requires fresh results and preserves build-only transport.
"""
import os
from pathlib import Path
import re
import subprocess
import shlex
import sys
import tempfile
import unittest

from test_verify import workflow_bash

ROOT = Path(__file__).resolve().parents[1]


def workflow_step(name):
    workflow = (ROOT / '.github/workflows/ci.yml').read_text()
    match = re.search(r'^      - name: ' + re.escape(name) + r'\n(.*?)(?=^      - |^  \w|\Z)', workflow, re.M | re.S)
    if match is None:
        raise AssertionError(f'Missing workflow step: {name}')
    return match[1]


class WorkflowAdoption(unittest.TestCase):
    def test_active_workflow_has_no_prior_head_or_postmerge_test_pass_adoption(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        for retired in ('python scripts/ci_adoption.py', 'python scripts/ci_head_reuse.py',
                        'python scripts/ci_qualification.py', '--cross-head-proof', 'steps.adoption'):
            self.assertNotIn(retired, workflow)
        self.assertIn('python scripts/verify.py full --ci-profile', workflow)
        self.assertIn('python scripts/ci_selected.py aggregate', workflow)

    def test_complete_partition_is_fresh_and_uses_explicit_shell(self):
        block = workflow_step('Verify complete profile partition')
        self.assertIn('shell: bash', block)
        self.assertIn('if: success() && !cancelled()', block)
        self.assertNotIn('cache-hit', block)
        self.assertNotIn('continue-on-error', block)

    def test_partition_and_shard_gates_reject_failure_cancellation_and_skip(self):
        for name in ('Require fresh complete partition execution', 'Require fresh complete shard execution'):
            block = workflow_step(name)
            self.assertIn('if: always()', block)
            self.assertIn('shell: bash', block)
            program = re.search(r'^        run: (.+)$', block, re.M)[1]
            for outcome in ('success', 'failure', 'cancelled', 'skipped', '', 'unknown'):
                result = subprocess.run([workflow_bash(), '--noprofile', '--norc', '-c', program],
                    env={**os.environ, 'VERIFICATION_OUTCOME': outcome}, capture_output=True, timeout=5)
                self.assertEqual(result.returncode == 0, outcome == 'success', (name, outcome))

    def test_full_producer_creates_parent_on_clean_checkout_without_replacing_bundle(self):
        import ci_binary_artifact as artifact
        block = workflow_step('Prepare ordinary application artifact parent')
        command = shlex.split(re.search(r'^        run: (.+)$', block, re.M)[1])
        self.assertEqual(command[:2], ['python', '-c'])
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertLess(workflow.index('Prepare ordinary application artifact parent'),
                        workflow.index('Build exact-run ordinary applications'))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            executables = {}
            for name in artifact.BINARIES:
                path = root / (name + '.exe')
                path.write_bytes(b'fixture executable')
                executables[name] = path
            output = root / '.local/application-artifact'
            with self.assertRaises(OSError):
                artifact.package(executables, {'platform': 'Windows'}, output)
            subprocess.run([sys.executable, *command[1:]], cwd=root, check=True, timeout=10)
            self.assertTrue(output.parent.is_dir())
            self.assertFalse(output.exists())
            artifact.package(executables, {'platform': 'Windows'}, output)
            self.assertTrue((output / 'manifest.json').is_file())
            with self.assertRaises(FileExistsError):
                artifact.package(executables, {'platform': 'Windows'}, output)

    def test_exhaustive_schedule_retains_weekly_budget(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        schedules = re.findall(r'^    - cron: "([^"\n]+)"$', workflow, re.M)
        self.assertEqual(schedules, ['17 6 * * 1'])
        self.assertIn('  release:\n    types: [published]', workflow)
        self.assertIn('  workflow_dispatch:', workflow)

    def test_artifact_validation_is_not_test_result_reuse(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertIn('--ci-binaries .local/application-artifact', workflow)
        self.assertIn('--ci-python-shard ${{ matrix.shard }}', workflow)
        self.assertIn('Verify complete profile partition', workflow)
        self.assertNotIn('adopted=true', workflow)


if __name__ == '__main__':
    unittest.main()
