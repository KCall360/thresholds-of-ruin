"""No-build assurance uses real inherited shims, not fake successful builds."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import ci_no_build as guard


class ToolingNoBuildTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name).resolve() / 'guard'
        self.env = guard.prepare(self.directory, executables={'cargo': sys.executable, 'rustc': sys.executable})

    def rows(self):
        return [json.loads(line) for line in (self.directory / 'audit.jsonl').read_text().splitlines()]

    def write(self, rows):
        (self.directory / 'audit.jsonl').write_text(''.join(json.dumps(row) + '\n' for row in rows))

    def invoke(self, tool, arguments):
        extension = '.cmd' if os.name == 'nt' else ''
        return subprocess.run([str(self.directory / 'bin' / (tool + extension)), *arguments],
                              env=dict(os.environ, **self.env), capture_output=True, text=True, check=False)

    def test_real_version_shim_and_empty_application_directory(self):
        result = self.invoke('rustc', ['--version'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('Python', result.stdout)
        self.assertEqual(guard.validate_audit(self.directory), {'metadata_calls': 1, 'build_calls': 0})
        self.assertEqual(list(Path(self.env['TOR_TEST_BINARIES']).iterdir()), [])
        self.assertFalse(Path(self.env['TOR_SCENARIO_COMPILER']).exists())

    def test_child_python_inherits_the_guard(self):
        code = "import subprocess; raise SystemExit(subprocess.run(['cargo','build']).returncode)"
        result = subprocess.run([sys.executable, '-c', code], env=dict(os.environ, **self.env), cwd=self.temporary.name,
                                capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 126, result.stderr)
        with self.assertRaisesRegex(ValueError, 'Unexpected compiler'):
            guard.validate_audit(self.directory)

    def test_python_startup_guards_extensionless_compilers_and_children(self):
        directory = Path(self.temporary.name) / 'startup'
        env = guard.prepare(directory, executables={'cargo': sys.executable, 'rustc': sys.executable},
                            python_guard=True)
        # Remove the POSIX shim too: this test must exercise Popen interception.
        env['PATH'] = os.defpath
        guard.probe(directory, env)
        code = "import subprocess; raise SystemExit(subprocess.run(['cargo', '--version']).returncode)"
        result = subprocess.run([sys.executable, '-c', code], env=dict(os.environ, **env),
                                cwd=self.temporary.name, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('Python', result.stdout)
        self.assertEqual(guard.validate_audit(directory)['metadata_calls'], 2)
        code = "import subprocess; raise SystemExit(subprocess.run(['rustc.exe', 'forbidden.rs']).returncode)"
        result = subprocess.run([sys.executable, '-c', code], env=dict(os.environ, **env),
                                cwd=self.temporary.name, capture_output=True, text=True)
        self.assertEqual(result.returncode, 126, result.stderr)
        with self.assertRaisesRegex(ValueError, 'Unexpected compiler'):
            guard.validate_audit(directory)
        self.assertFalse((Path(self.temporary.name) / 'target').exists())

    def test_python_startup_rejects_compiler_executable_override(self):
        directory = Path(self.temporary.name) / 'override-startup'
        env = guard.prepare(directory, executables={'cargo': sys.executable, 'rustc': sys.executable},
                            python_guard=True)
        code = ("import subprocess; raise SystemExit(subprocess.run(['alias', 'build'], "
                "executable='does-not-exist/cargo.exe').returncode)")
        result = subprocess.run([sys.executable, '-c', code], env=dict(os.environ, **env),
                                cwd=self.temporary.name, capture_output=True, text=True)
        self.assertEqual(result.returncode, 126, result.stderr)
        rows = [json.loads(line) for line in (directory / 'audit.jsonl').read_text().splitlines()]
        self.assertEqual(len(rows), 1)
        self.assertFalse(rows[0]['permitted'])
        self.assertEqual(rows[0]['arguments'], ['unsupported-launch-form'])

    def test_missing_startup_hook_cannot_pass_probe(self):
        directory = Path(self.temporary.name) / 'missing-startup'
        env = guard.prepare(directory, executables={'cargo': sys.executable, 'rustc': sys.executable},
                            python_guard=True)
        (directory / 'startup/sitecustomize.py').write_text('')
        with self.assertRaisesRegex(ValueError, 'startup'):
            guard.probe(directory, env)

    def test_exact_metadata_is_offline_and_forwarded_once(self):
        args = ['metadata', '--format-version', '1', '--no-deps', '--locked']
        with patch.object(guard.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)) as run:
            self.assertEqual(guard.dispatch(self.directory, 'cargo', args), 0)
        self.assertEqual(run.call_args.args[0], [os.path.abspath(sys.executable), *args])
        self.assertEqual(run.call_args.kwargs['env']['CARGO_NET_OFFLINE'], 'true')
        self.assertEqual(run.call_count, 1)
        self.assertEqual(guard.validate_audit(self.directory)['metadata_calls'], 1)

    def test_builds_and_extra_arguments_are_never_forwarded(self):
        forbidden = [('cargo', ['test']), ('cargo', ['check']), ('cargo', ['clippy']),
                     ('cargo', ['doc']), ('cargo', ['-V', '--help']),
                     ('cargo', ['metadata', '--format-version', '1', '--no-deps', '--locked', '--features', 'x']),
                     ('rustc', ['a.rs']), ('rustc', ['--version', 'a.rs'])]
        for tool, args in forbidden:
            with self.subTest(tool=tool, args=args), patch.object(
                    guard.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)) as run:
                self.assertEqual(guard.dispatch(self.directory, tool, args), 126)
                run.assert_not_called()

    def test_failed_or_unfinished_metadata_does_not_qualify(self):
        with patch.object(guard.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1)):
            guard.dispatch(self.directory, 'cargo', ['-V'])
        with self.assertRaisesRegex(ValueError, 'invocation failed'):
            guard.validate_audit(self.directory)
        self.write(self.rows()[:1])
        with self.assertRaisesRegex(ValueError, 'Unfinished'):
            guard.validate_audit(self.directory)

    def test_duplicate_or_mismatched_completion_is_rejected(self):
        self.assertEqual(self.invoke('cargo', ['-V']).returncode, 0)
        rows = self.rows()
        self.write([*rows, rows[-1]])
        with self.assertRaisesRegex(ValueError, 'Duplicate'):
            guard.validate_audit(self.directory)
        rows[-1]['arguments'] = ['--version']
        self.write(rows)
        with self.assertRaisesRegex(ValueError, 'Unfinished'):
            guard.validate_audit(self.directory)

    def test_changed_tool_and_guard_identity_reject_before_execution(self):
        path = self.directory / 'manifest.json'
        original = json.loads(path.read_text())
        for key in ('sha256', 'resolved'):
            data = json.loads(json.dumps(original))
            data['tools']['cargo'][key] = 'changed'
            path.write_text(json.dumps(data))
            with patch.object(guard.subprocess, 'run') as run:
                self.assertEqual(guard.dispatch(self.directory, 'cargo', ['-V']), 126)
                run.assert_not_called()
        original['shim_source_sha256'] = 'changed'
        path.write_text(json.dumps(original))
        with self.assertRaisesRegex(ValueError, 'guard changed'):
            guard.validate_audit(self.directory)

    def test_empty_audit_valid_but_truncated_or_unknown_events_fail(self):
        self.assertEqual(guard.validate_audit(self.directory)['metadata_calls'], 0)
        (self.directory / 'audit.jsonl').write_text('{')
        with self.assertRaisesRegex(ValueError, 'Incomplete'):
            guard.validate_audit(self.directory)
        self.write([{'event': 'invented', 'invocation': 'a' * 32, 'tool': 'cargo', 'arguments': ['-V']}])
        with self.assertRaisesRegex(ValueError, 'Unknown'):
            guard.validate_audit(self.directory)

    def test_foreign_evidence_does_not_probe_tools_but_still_rejects_missing_completion(self):
        self.assertEqual(self.invoke('cargo', ['-V']).returncode, 0)
        path = self.directory / 'manifest.json'
        manifest = json.loads(path.read_text())
        manifest['tools']['cargo']['path'] = '/unavailable/foreign/cargo.exe'
        manifest['tools']['cargo']['resolved'] = '/unavailable/foreign/cargo.exe'
        path.write_text(json.dumps(manifest))
        self.assertEqual(guard.validate_audit(self.directory, check_tools=False)['metadata_calls'], 1)
        with self.assertRaises((OSError, ValueError)):
            guard.validate_audit(self.directory)
        self.write(self.rows()[:1])
        with self.assertRaisesRegex(ValueError, 'Unfinished'):
            guard.validate_audit(self.directory, check_tools=False)

    def test_duplicate_keys_and_nonfinite_evidence_fail_closed(self):
        for text in ('{"event":"request","event":"completion"}', '{"returncode":NaN}'):
            with self.subTest(text=text), self.assertRaises(ValueError):
                guard.strict_json(text)
        path = self.directory / 'manifest.json'
        path.write_text('{"version":1,"version":1}')
        with self.assertRaisesRegex(ValueError, 'Duplicate'):
            guard.validate_audit(self.directory, check_tools=False)

    def test_python_source_identity_uses_universal_newlines_not_content_rewriting(self):
        source = self.directory / 'source.py'
        source.write_bytes(b'x = 1\ny = 2\n')
        with patch.object(guard, '__file__', str(source)):
            expected = guard.source_digest()
            source.write_bytes(b'x = 1\r\ny = 2\r\n')
            self.assertEqual(guard.source_digest(), expected)
            source.write_bytes(b'x = 2\r\ny = 2\r\n')
            self.assertNotEqual(guard.source_digest(), expected)


if __name__ == '__main__':
    unittest.main()
