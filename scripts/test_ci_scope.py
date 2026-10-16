"""A prose-only diff may skip expensive CI; ambiguity may never do so."""
from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import ci_scope

ROOT = Path(__file__).resolve().parents[1]


def job(name):
    workflow = (ROOT / '.github/workflows/ci.yml').read_text()
    return re.search(r'^  ' + name + r':\n(.*?)(?=^  \w|\Z)', workflow, re.M | re.S)[1]


def job_condition(name, scope_result, docs_only, *, cancelled=False):
    """Evaluate only the checked-in scope conditions' small expression vocabulary."""
    expression = re.search(r'^    if: (.+)$', job(name), re.M)[1].strip()
    if expression.startswith('${{') and expression.endswith('}}'):
        expression = expression[3:-2].strip()
    tokens = re.findall(r"'[^']*'|[A-Za-z_.]+(?:\(\))?|&&|==|[!()]|\S", expression)
    values = {
        'needs.scope.result': repr(scope_result), 'needs.scope.outputs.docs_only': repr(docs_only),
        'always()': 'True', 'cancelled()': repr(cancelled),
        "'success'": "'success'", "'true'": "'true'",
        '&&': 'and', '==': '==', '!': 'not', '(': '(', ')': ')',
    }
    # Unknown tokens fail rather than silently acquiring different semantics.
    translated = ' '.join(values[token] for token in tokens)
    # GitHub adds success() unless a status-check function overrides it.
    if not {'always()', 'cancelled()'}.intersection(tokens):
        translated = f'{scope_result == "success" and not cancelled!r} and ({translated})'
    return eval(translated, {'__builtins__': {}})


class WorkflowScopeContract(unittest.TestCase):
    def test_expensive_jobs_require_successful_docs_scope_before_skipping(self):
        for name in ('dependencies', 'profiles'):
            self.assertIn('needs: scope', job(name))
            for scope in ('success', 'failure', 'cancelled', 'skipped', '', 'unknown'):
                for mode in ('true', 'false', '', 'unknown'):
                    with self.subTest(job=name, scope=scope, docs_only=mode):
                        self.assertEqual(job_condition(name, scope, mode),
                                         not (scope == 'success' and mode == 'true'))

    def test_expensive_jobs_stop_when_the_workflow_is_cancelled(self):
        for name in ('dependencies', 'profiles'):
            for scope, mode in (('success', 'false'), ('failure', 'true')):
                with self.subTest(job=name, scope=scope, docs_only=mode):
                    self.assertTrue(job_condition(name, scope, mode), 'Live runs still need the full path')
                    self.assertFalse(job_condition(name, scope, mode, cancelled=True))

    def test_existing_aggregate_checks_intentional_skip_and_failures(self):
        body = job('check')
        self.assertIn('name: Rust (${{ matrix.os }})', body)
        self.assertIn('needs: [scope, tooling, profiles, dependencies]', body)
        self.assertIn('if: always()', body)
        match = re.search(r"python - <<'PY'\n(.*?)\n          PY", body, re.S)
        self.assertIsNotNone(match, 'The required aggregate needs an explicit scope-aware gate')
        program = '\n'.join(line[10:] for line in match[1].splitlines())
        for mode in ('true', 'false', '', 'unknown'):
            for scope in ('success', 'failure', 'cancelled', 'skipped'):
                for tooling in ('success', 'failure', 'cancelled', 'skipped'):
                    for profiles in ('success', 'failure', 'cancelled', 'skipped'):
                        for dependencies in ('success', 'failure', 'cancelled', 'skipped'):
                            expected = scope == tooling == 'success' and (
                                mode == 'true' and profiles == dependencies == 'skipped'
                                or mode == 'false' and profiles == dependencies == 'success')
                            environment = dict(DOCS_ONLY=mode, SCOPE_RESULT=scope, TOOLING_RESULT=tooling,
                                               PROFILES_RESULT=profiles, DEPENDENCIES_RESULT=dependencies)
                            with self.subTest(**environment), patch.dict(os.environ, environment), redirect_stdout(io.StringIO()):
                                try:
                                    exec(compile(program, '<workflow aggregate>', 'exec'), {})
                                except SystemExit as error:
                                    passed = error.code in (None, 0)
                                else:
                                    passed = True
                                self.assertEqual(passed, expected)


class GitScopeContract(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.git('init', '-q', '-b', 'main')
        for path in ('README.md', 'docs/guide.md', 'docs/reference/protocol.md', 'code.rs'):
            self.write(path, 'original\n')
        self.base = self.commit()

    def git(self, *arguments, data=None):
        return subprocess.run(['git', '-c', 'user.name=OpenAI', '-c', 'user.email=codex@openai.com',
                               *arguments], cwd=self.root, input=data, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, check=True).stdout.decode().strip()

    def write(self, path, content):
        destination = self.root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(content)

    def commit(self):
        self.git('add', '-A')
        self.git('commit', '-qm', 'fixture')
        return self.git('rev-parse', 'HEAD')

    def push(self, **changes):
        head = self.git('rev-parse', 'HEAD')
        event = dict(before=self.base, after=head, created=False, deleted=False, forced=False)
        event.update(changes)
        return ci_scope.classify(self.root, 'push', event, head)

    def test_complete_push_range_and_fixed_root_allowlist(self):
        self.write('README.md', 'updated\n')
        self.write('docs/nested/guide.md', 'new\n')
        self.commit()
        self.assertTrue(self.push()['docs_only'])
        self.write('code.rs', 'changed code\n')
        self.commit()
        self.write('docs/guide.md', 'latest docs commit\n')
        self.commit()
        self.assertFalse(self.push()['docs_only'], 'Earlier code in the push must not disappear')

    def test_renames_check_both_paths(self):
        for source, target, allowed in (('docs/guide.md', 'docs/renamed.md', True),
                                         ('code.rs', 'docs/code.md', False),
                                         ('docs/guide.md', 'renamed.rs', False)):
            with self.subTest(source=source, target=target):
                self.git('reset', '--hard', self.base)
                self.git('mv', source, target)
                self.commit()
                self.assertEqual(self.push()['docs_only'], allowed)

    def test_document_deletion_is_safe_but_the_documentation_gate_still_runs(self):
        (self.root / 'docs/guide.md').unlink()
        self.commit()
        self.assertTrue(self.push()['docs_only'])
        self.assertIn('python3 -m unittest discover -s scripts -p test_documentation.py -v', job('scope'))

    def test_protocol_workflow_code_config_and_unknown_paths_require_full(self):
        for path in ('docs/reference/protocol.md', '.github/workflows/ci.yml', 'scripts/ci_scope.py',
                     'Cargo.toml', 'Cargo.lock', '.cargo/config.toml', 'docs/image.png',
                     'crates/README.md', 'LICENSE', 'unknown.md'):
            with self.subTest(path=path):
                self.git('reset', '--hard', self.base)
                self.write(path, 'changed\n')
                self.commit()
                self.assertFalse(self.push()['docs_only'])

    def test_gitlink_changes_cannot_hide_behind_git_configuration(self):
        self.git('update-index', '--add', '--cacheinfo', '160000,' + self.base + ',nested')
        self.git('commit', '-qm', 'base gitlink')
        self.base = self.git('rev-parse', 'HEAD')
        self.write('docs/guide.md', 'new\n')
        self.git('add', 'docs/guide.md')
        self.git('update-index', '--cacheinfo', '160000,' + self.base + ',nested')
        self.git('commit', '-qm', 'changed gitlink and docs')
        self.git('config', 'diff.ignoreSubmodules', 'all')
        self.assertFalse(self.push()['docs_only'])

    def test_executable_mode_and_symlink_markdown_require_full(self):
        self.git('update-index', '--chmod=+x', 'docs/guide.md')
        self.git('commit', '-qm', 'executable mode')
        self.assertFalse(self.push()['docs_only'])
        self.git('reset', '--hard', self.base)
        blob = self.git('hash-object', '-w', '--stdin', data=b'../code.rs')
        self.git('update-index', '--add', '--cacheinfo', '120000,' + blob + ',docs/link.md')
        self.git('commit', '-qm', 'symbolic link')
        self.assertFalse(self.push()['docs_only'])

    def test_missing_shallow_forced_empty_and_unknown_events_require_full(self):
        self.assertFalse(self.push()['docs_only'])
        self.write('docs/guide.md', 'updated\n')
        head = self.commit()
        for changes in ({'before': '0' * 40}, {'before': '1' * 40}, {'before': '../main'},
                        {'after': self.base}, {'forced': True}, {'created': True}, {'deleted': True}):
            with self.subTest(changes=changes): self.assertFalse(self.push(**changes)['docs_only'])
        for event in ('workflow_dispatch', 'schedule', 'unknown', None):
            self.assertFalse(ci_scope.classify(self.root, event, {}, head)['docs_only'])
        (self.root / '.git/shallow').write_text(self.base + '\n')
        self.assertFalse(self.push()['docs_only'])

    def test_pr_checks_entire_branch_and_actual_merge_parents(self):
        self.git('checkout', '-qb', 'feature')
        self.write('docs/guide.md', 'feature docs\n')
        head = self.commit()
        self.git('checkout', '-q', 'main')
        self.write('code.rs', 'unrelated main change\n')
        base = self.commit()
        self.git('merge', '--no-ff', '-qm', 'synthetic merge', 'feature')
        checkout = self.git('rev-parse', 'HEAD')
        event = {'pull_request': {'base': {'sha': base}, 'head': {'sha': head}}}
        self.assertTrue(ci_scope.classify(self.root, 'pull_request', event, checkout)['docs_only'])
        event['pull_request']['base']['sha'] = self.base
        self.assertFalse(ci_scope.classify(self.root, 'pull_request', event, checkout)['docs_only'])
        event['pull_request']['base']['sha'] = base
        self.assertFalse(ci_scope.classify(self.root, 'pull_request', event, head)['docs_only'])

    def test_earlier_pr_code_and_unsafe_merge_resolution_cannot_hide(self):
        self.git('checkout', '-qb', 'feature')
        self.write('code.rs', 'earlier feature code\n')
        self.commit()
        self.write('docs/guide.md', 'last commit is docs\n')
        head = self.commit()
        self.git('checkout', '-q', 'main')
        self.git('merge', '--no-ff', '-qm', 'synthetic merge', 'feature')
        checkout = self.git('rev-parse', 'HEAD')
        event = {'pull_request': {'base': {'sha': self.base}, 'head': {'sha': head}}}
        self.assertFalse(ci_scope.classify(self.root, 'pull_request', event, checkout)['docs_only'])
        # Even if the merge discards the code change, the entire PR still includes it.
        self.git('checkout', self.base, '--', 'code.rs')
        self.git('commit', '--amend', '--no-edit', '-q')
        checkout = self.git('rev-parse', 'HEAD')
        self.assertFalse(ci_scope.classify(self.root, 'pull_request', event, checkout)['docs_only'])

    def test_docs_branch_cannot_hide_code_introduced_only_by_the_merge(self):
        self.git('checkout', '-qb', 'feature')
        self.write('docs/guide.md', 'feature docs\n')
        head = self.commit()
        self.git('checkout', '-q', 'main')
        self.git('merge', '--no-ff', '-qm', 'synthetic merge', 'feature')
        event = {'pull_request': {'base': {'sha': self.base}, 'head': {'sha': head}}}
        self.assertTrue(ci_scope.classify(self.root, 'pull_request', event, self.git('rev-parse', 'HEAD'))['docs_only'])
        self.write('code.rs', 'unsafe merge-only change\n')
        self.git('add', 'code.rs')
        self.git('commit', '--amend', '--no-edit', '-q')
        self.assertFalse(ci_scope.classify(self.root, 'pull_request', event, self.git('rev-parse', 'HEAD'))['docs_only'])

    def test_malformed_truncated_oversized_and_unsafe_raw_diffs_reject(self):
        record = b':100644 100644 ' + b'a' * 40 + b' ' + b'b' * 40 + b' M\0docs/guide.md\0'
        self.assertEqual(ci_scope.changed_paths(record), ['docs/guide.md'])
        for raw in (record[:-1], record + record, b'bad\0docs/x.md\0', b'x' * (ci_scope.MAX_DIFF_BYTES + 1),
                    record.replace(b'docs/guide.md', b'docs/../Cargo.toml'),
                    record.replace(b'docs/guide.md', b'docs/\xff.md')):
            with self.subTest(size=len(raw)), self.assertRaises(ValueError): ci_scope.changed_paths(raw)

    def test_windows_aliases_and_reserved_paths_require_full(self):
        for path in ('docs/reference/Protocol.md', 'docs/REFERENCE/protocol.md', 'docs/CON.md',
                     'docs/com1.md', 'docs/reference./protocol.md', 'docs/reference /protocol.md',
                     'docs/ reference/protocol.md', 'docs/readme.md/guide.md',
                     'docs/Guide.md', 'docs/readme.md', 'docs/.git/guide.md'):
            with self.subTest(path=path):
                self.assertFalse(ci_scope.documentation_path(path))
        self.assertTrue(ci_scope.documentation_path('docs/research/README.md'))
        self.write('docs/reference/Protocol.md', 'case-aliased wire input\n')
        self.commit()
        self.assertFalse(self.push()['docs_only'])

    def test_cli_writes_only_fixed_output_values(self):
        self.write('docs/guide.md', 'updated\n')
        head = self.commit()
        event, output = self.root / 'event.json', self.root / 'output'
        event.write_text(json.dumps(dict(before=self.base, after=head, created=False, deleted=False, forced=False)))
        environment = {**os.environ, 'GITHUB_EVENT_PATH': str(event), 'GITHUB_OUTPUT': str(output),
                       'GITHUB_SHA': head, 'GITHUB_EVENT_NAME': 'push'}
        result = subprocess.run([os.sys.executable, str(ROOT / 'scripts/ci_scope.py')], cwd=self.root,
                                env=environment, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(json.loads(result.stdout)['docs_only'])
        self.assertEqual(output.read_text(), 'docs_only=true\n')


if __name__ == '__main__':
    unittest.main()
