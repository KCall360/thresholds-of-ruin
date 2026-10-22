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


def workflow_job(name):
    text = (ROOT / '.github/workflows/ci.yml').read_text()
    return re.search(r'^  ' + re.escape(name) + r':\n(.*?)(?=^  \w|\Z)', text, re.M | re.S)[1]


def consumer_permitted(name, *, producer='success', apps='true', mode='full', docs='false', cancelled=False):
    expression = re.search(r'^    if: \$\{\{ (.+) \}\}$', workflow_job(name), re.M)[1]
    values = {'cancelled()': repr(cancelled), 'needs.scope.result': repr('success'),
              'needs.scope.outputs.mode': repr(mode), 'needs.scope.outputs.docs_only': repr(docs),
              'needs.scope.outputs.requires_apps': repr(apps),
              'needs.windows_binaries.result': repr(producer), 'needs.selected_rust.result': repr(producer),
              '&&': 'and', '||': 'or', '!': 'not', '==': '==', '!=': '!=', '(': '(', ')': ')'}
    tokens = re.findall(r"'[^']*'|[A-Za-z_.]+(?:\(\))?|&&|\|\||!=|==|[!()]|\S", expression)
    translated = ' '.join(token if token.startswith("'") else values[token] for token in tokens)
    return eval(translated, {'__builtins__': {}})


class WorkflowAdoption(unittest.TestCase):
    def test_active_workflow_has_no_prior_head_or_postmerge_test_pass_adoption(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        for retired in ('python scripts/ci_adoption.py', 'python scripts/ci_head_reuse.py',
                        'python scripts/ci_qualification.py', '--cross-head-proof', 'steps.adoption'):
            self.assertNotIn(retired, workflow)
        self.assertIn('python scripts/verify.py full --ci-profile', workflow)
        self.assertIn('python scripts/ci_selected.py aggregate', workflow)

    def test_cache_writes_stay_successful_and_same_repository_ref_scoped(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        expressions = re.findall(r'^          save-if: \$\{\{ (.+) \}\}$', workflow, re.M)
        caches = re.findall(r'uses: Swatinem/rust-cache@', workflow)
        self.assertEqual(len(expressions), len(caches))
        self.assertEqual(workflow.count('          cache-on-failure: false'), len(caches))
        self.assertGreater(len(caches), 0)
        for expression in expressions:
            for passed in (False, True):
                for event in ('push', 'pull_request', 'pull_request_target', 'workflow_dispatch', 'schedule'):
                    for ref in ('refs/heads/main', 'refs/heads/topic', 'refs/pull/188/merge'):
                        for head in ('owner/repo', 'fork/repo', ''):
                            values = {'job.status': repr('success' if passed else 'failure'), 'github.ref': repr(ref),
                                      'github.event_name': repr(event), 'github.repository': repr('owner/repo'),
                                      'github.event.pull_request.head.repo.full_name': repr(head),
                                      '&&': 'and', '||': 'or', '==': '==', '!=': '!=', '(': '(', ')': ')'}
                            tokens = re.findall(r"'[^']*'|[A-Za-z_.]+(?:\(\))?|&&|\|\||!=|==|[()]|\S", expression)
                            program = ' '.join(token if token.startswith("'") else values[token] for token in tokens)
                            actual = eval(program, {'__builtins__': {}})
                            expected = passed and event != 'pull_request_target' and (ref == 'refs/heads/main' or event == 'pull_request' and head == 'owner/repo')
                            with self.subTest(event=event, ref=ref, head=head, passed=passed):
                                self.assertEqual(actual, expected)
        # No cross-PR cache namespace override or untrusted-trigger expansion.
        self.assertNotIn('pull_request_target:', workflow)
        self.assertNotIn('cache-on-failure: true', workflow)

    def test_full_windows_consumers_never_start_without_successful_producer(self):
        for producer in ('success', 'failure', 'cancelled', 'skipped', '', 'unknown'):
            for cancelled in (False, True):
                for docs in ('false', 'true'):
                    with self.subTest(producer=producer, cancelled=cancelled, docs=docs):
                        self.assertEqual(consumer_permitted('windows_python', producer=producer,
                                                            cancelled=cancelled, docs=docs),
                                         producer == 'success' and not cancelled and docs == 'false')
        self.assertFalse(consumer_permitted('windows_python', mode='selected'))
        # Independent Rust/Linux partitions must not acquire a producer barrier.
        self.assertNotIn('windows_binaries', workflow_job('profiles'))
        # Required full qualification still rejects failed producer/skipped shards.
        gate = workflow_job('check')
        self.assertIn("windows == ['success', 'success']", gate)
        self.assertIn("WINDOWS_BINARIES_RESULT: ${{ needs.windows_binaries.result }}", gate)
        self.assertIn("WINDOWS_PYTHON_RESULT: ${{ needs.windows_python.result }}", gate)

    def test_native_artifact_contracts_run_before_any_producer_compile(self):
        producer = workflow_job('windows_binaries')
        self.assertIn('run: python -m unittest discover -s scripts -p test_ci_binary_artifact.py', producer)
        self.assertLess(producer.index('Check artifact transport contracts before compiling'),
                        producer.index('Build exact-run ordinary applications'))
        self.assertNotIn('continue-on-error', producer)

    def test_producer_smokes_fresh_transport_before_uploading_pristine_bundle(self):
        producer = workflow_job('windows_binaries')
        stage = workflow_step('Validate fresh producer transport and application runtime')
        self.assertLess(producer.index('Build exact-run ordinary applications'),
                        producer.index('Validate fresh producer transport and application runtime'))
        self.assertLess(producer.index('Validate fresh producer transport and application runtime'),
                        producer.index('Publish binaries, not test evidence'))
        self.assertIn('shell: bash', stage)
        self.assertIn('python scripts/ci_binary_artifact.py validate .local/producer-validation '
                      '--profile ${{ matrix.profile }} --source-sha "$GITHUB_SHA" --smoke', stage)
        self.assertNotIn('cargo ', stage)
        self.assertNotIn('continue-on-error', stage)
        upload = workflow_step('Publish binaries, not test evidence')
        self.assertIn('.local/application-artifact/manifest.json', upload)
        self.assertIn('.local/application-artifact/executables.tar', upload)
        self.assertNotIn('producer-validation', upload)
        # Execute the actual copy program: only immutable transport files move,
        # archive modes simulate artifact download, and an old destination fails.
        match = re.search(r"python - <<'PY'\n(.*?)\n          PY", stage, re.S)
        self.assertIsNotNone(match)
        program = '\n'.join(line[10:] for line in match[1].splitlines())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original = root / '.local/application-artifact'
            original.mkdir(parents=True)
            for name in ('manifest.json', 'executables.tar'):
                path = original / name
                path.write_bytes(('immutable:' + name).encode())
                path.chmod(0o755)
            (original / 'not-transport.txt').write_text('must not be copied')
            before = {path.name: (path.read_bytes(), path.stat().st_mode) for path in original.iterdir()}
            subprocess.run([sys.executable, '-c', program], cwd=root, check=True, timeout=10)
            copied = root / '.local/producer-validation'
            self.assertEqual({path.name for path in copied.iterdir()}, {'manifest.json', 'executables.tar'})
            for path in copied.iterdir():
                self.assertEqual(path.read_bytes(), before[path.name][0])
                if os.name != 'nt':
                    self.assertEqual(path.stat().st_mode & 0o777, 0o644)
            self.assertEqual({path.name: (path.read_bytes(), path.stat().st_mode) for path in original.iterdir()}, before)
            rejected = subprocess.run([sys.executable, '-c', program], cwd=root,
                                      capture_output=True, timeout=10)
            self.assertNotEqual(rejected.returncode, 0)

    def test_selected_consumer_keeps_pure_tooling_independent_of_app_producer(self):
        for producer in ('success', 'failure', 'cancelled', 'skipped', '', 'unknown'):
            with self.subTest(producer=producer):
                self.assertEqual(consumer_permitted('selected_linux', producer=producer, mode='selected'),
                                 producer == 'success')
                self.assertTrue(consumer_permitted('selected_linux', producer=producer, mode='selected', apps='false'))
                self.assertFalse(consumer_permitted('selected_linux', producer=producer, mode='selected',
                                                    apps='false', cancelled=True))
        self.assertFalse(consumer_permitted('selected_linux', mode='full', apps='false'))

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
