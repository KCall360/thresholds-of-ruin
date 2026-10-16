"""Offline hostile transport and consumer fixtures; no live API or builds."""
from contextlib import redirect_stderr
import hashlib
import json
import os
import io
import stat
import subprocess
import sys
import tempfile
from pathlib import Path
import time
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch
import urllib.error
import warnings
import zipfile

import ci_adoption as adoption


class Response(io.BytesIO):
    status = 200

    def __init__(self, data, headers=None):
        super().__init__(data)
        self.headers = headers or {}


class ArtifactTransport(unittest.TestCase):
    def setUp(self):
        self.data = b'archive bytes'
        self.artifact = {'id': 12, 'size_in_bytes': len(self.data),
                         'digest': 'sha256:' + hashlib.sha256(self.data).hexdigest()}
        self.api = SimpleNamespace(token='private-api-token')
        self.location = 'https://storage.blob.core.windows.net/artifact?signature=private-storage-signature'
        self.api_opener, self.storage_opener = Mock(), Mock()
        self.factory = self.enterContext(patch.object(adoption.urllib.request, 'build_opener',
                                                     side_effect=[self.api_opener, self.storage_opener]))
        self.redirect(self.location)
        self.storage_opener.open.return_value = Response(self.data, {'Content-Length': str(len(self.data))})

    def redirect(self, location):
        self.api_opener.open.side_effect = urllib.error.HTTPError(
            'https://api.github.com/fixture', 302, 'redirect', {'Location': location}, io.BytesIO())

    def download(self):
        return adoption.download_artifact(self.api, self.artifact, deadline=time.monotonic() + 60)

    def test_exact_bytes_and_digest_with_credentials_only_on_api_origin(self):
        self.assertEqual(self.download(), self.data)
        api_request = self.api_opener.open.call_args.args[0]
        storage_request = self.storage_opener.open.call_args.args[0]
        self.assertEqual(api_request.full_url,
                         'https://api.github.com/repos/KCall360/thresholds-of-ruin/actions/artifacts/12/zip')
        self.assertEqual(api_request.get_header('Authorization'), 'Bearer private-api-token')
        self.assertEqual(dict(storage_request.header_items()), {'Accept-encoding': 'identity'})
        self.assertEqual(storage_request.full_url, self.location)
        self.assertTrue(all(isinstance(call.args[0], adoption.NoFollow) for call in self.factory.call_args_list))

    def test_invalid_metadata_does_not_start_a_request(self):
        for key, value in (('id', True), ('id', 0), ('size_in_bytes', 0),
                           ('size_in_bytes', adoption.MAX_ARCHIVE + 1), ('digest', 'missing')):
            with self.subTest(key=key, value=value):
                original = self.artifact[key]
                self.artifact[key] = value
                with self.assertRaises(adoption.EvidenceError): self.download()
                self.artifact[key] = original
        self.factory.assert_not_called()

    def test_redirects_reject_userinfo_downgrades_foreign_hosts_and_fragments(self):
        for location in ('http://storage.blob.core.windows.net/a', 'https://evil.example/a',
                         'https://storage.blob.core.windows.net.evil.example/a',
                         'https://@storage.blob.core.windows.net/a', 'https://user@storage.blob.core.windows.net/a',
                         'https://storage.blob.core.windows.net:8443/a', self.location + '#fragment'):
            with self.subTest(location=location):
                self.factory.side_effect = [self.api_opener, self.storage_opener]
                self.redirect(location)
                with self.assertRaises(adoption.EvidenceError): self.download()
        self.storage_opener.open.assert_not_called()

    def test_storage_redirect_is_not_followed_or_given_api_credentials(self):
        self.storage_opener.open.side_effect = urllib.error.HTTPError(
            self.location, 302, 'redirect', {'Location': 'https://evil.example'}, io.BytesIO())
        with self.assertRaises(urllib.error.HTTPError): self.download()
        self.assertEqual(self.factory.call_count, 2)
        self.assertNotIn('Authorization', dict(self.storage_opener.open.call_args.args[0].header_items()))
        self.assertIsNone(adoption.NoFollow().redirect_request(None, None, 302, '', {}, 'https://evil.example'))

    def test_bad_length_encoding_digest_and_expired_deadline_reject(self):
        for data, headers in ((self.data[:-1], {}), (self.data + b'x', {}),
                              (self.data, {'Content-Length': '999'}), (self.data, {'Content-Encoding': 'gzip'}),
                              (b'wrong content', {})):
            with self.subTest(data=data, headers=headers):
                self.factory.side_effect = [self.api_opener, self.storage_opener]
                self.redirect(self.location)
                self.storage_opener.open.return_value = Response(data, headers)
                with self.assertRaises(adoption.EvidenceError): self.download()
        with self.assertRaises(adoption.EvidenceError):
            adoption.download_artifact(self.api, self.artifact, deadline=time.monotonic() - 1)

    def test_late_eof_never_returns_accepted_bytes(self):
        clock = [0]
        def late_eof(_):
            clock[0] = 2
            return b''
        source = Mock()
        source.read1.side_effect = late_eof
        with patch.object(adoption.time, 'monotonic', side_effect=lambda: clock[0]), self.assertRaises(adoption.EvidenceError):
            adoption.read_bounded(source, 10, deadline=1)


def archive(entries, *, compression=zipfile.ZIP_STORED):
    output = io.BytesIO()
    with warnings.catch_warnings(), zipfile.ZipFile(output, 'w', compression=compression) as result:
        warnings.simplefilter('ignore', UserWarning)
        for name, value, mode in entries:
            info = zipfile.ZipInfo(name)
            info.compress_type = compression
            info.external_attr = mode << 16
            result.writestr(info, value)
    return output.getvalue()


class ArtifactZip(unittest.TestCase):
    def read(self, data):
        return adoption.artifact_records(data, deadline=time.monotonic() + 60)

    def test_reads_exact_regular_members_without_extracting(self):
        data = archive([('observations/before.json', b'{}', stat.S_IFREG),
                        ('run-full-ci-debug-rust/rust-debug.log', b'payload', stat.S_IFREG)], compression=zipfile.ZIP_DEFLATED)
        with patch.object(zipfile.ZipFile, 'extractall', side_effect=AssertionError('must never extract')):
            self.assertEqual(self.read(data), {'observations/before.json': b'{}',
                                             'run-full-ci-debug-rust/rust-debug.log': b'payload'})

    def test_ambiguous_paths_and_nonregular_entries_reject(self):
        for name in ('../escape', '/absolute', 'dir/../escape', 'C:/drive', 'a\\b', 'a//b', './x'):
            with self.subTest(name=name), self.assertRaises(adoption.EvidenceError):
                self.read(archive([(name, b'payload', stat.S_IFREG)]))
        for entries in ([('a', b'', stat.S_IFREG), ('a', b'', stat.S_IFREG)],
                        [('A', b'', stat.S_IFREG), ('a', b'', stat.S_IFREG)],
                        [('a/', b'', stat.S_IFDIR), ('a', b'', stat.S_IFREG)],
                        [('link', b'target', stat.S_IFLNK)], [('file', b'payload', stat.S_IFDIR)]):
            with self.subTest(entries=entries), self.assertRaises(adoption.EvidenceError):
                self.read(archive(entries))

    def test_directory_entries_and_raw_nul_names_are_unsupported(self):
        # Real retained verification ZIPs contain files only; no directory fast path.
        for name in ('directory/', 'directory//'):
            with self.subTest(name=name), self.assertRaises(adoption.EvidenceError):
                self.read(archive([(name, b'', stat.S_IFDIR)]))
        data = archive([('aXtail.log', b'payload', stat.S_IFREG)])
        with self.assertRaises(adoption.EvidenceError):
            self.read(data.replace(b'aXtail.log', b'a\x00tail.log'))

    def test_truncation_crc_and_unsupported_compression_reject(self):
        data = archive([('a.log', b'payload', stat.S_IFREG)])
        for broken in (data[:-1], data.replace(b'payload', b'payloae'),
                       archive([('a.log', b'payload', stat.S_IFREG)], compression=zipfile.ZIP_BZIP2)):
            with self.subTest(size=len(broken)), self.assertRaises((adoption.EvidenceError, zipfile.BadZipFile)):
                self.read(broken)
        encrypted = bytearray(data)
        for offset in (6, data.index(b'PK\x01\x02') + 8): encrypted[offset] |= 1
        with self.assertRaises(adoption.EvidenceError): self.read(bytes(encrypted))

    def test_byte_member_and_deadline_bounds_reject(self):
        data = archive([('a.log', b'payload', stat.S_IFREG)])
        for key, limit in (('MAX_ARCHIVE', len(data) - 1), ('MAX_TOTAL', 1), ('MAX_RECORD', 1)):
            with self.subTest(key=key), patch.object(adoption, key, limit), self.assertRaises(adoption.EvidenceError):
                self.read(data)
        for entries in ([], [(str(index), b'', stat.S_IFREG) for index in range(65)],
                        [('summary.json', b'x' * (2 * 1024 * 1024 + 1), stat.S_IFREG)]):
            with self.subTest(count=len(entries)), self.assertRaises(adoption.EvidenceError): self.read(archive(entries))
        with self.assertRaises(adoption.EvidenceError):
            adoption.artifact_records(data, deadline=time.monotonic() - 1)

    def test_expired_deadline_also_rejects_directory_only_archives(self):
        data = archive([('directory/', b'', stat.S_IFDIR)])
        with self.assertRaises(adoption.EvidenceError):
            adoption.artifact_records(data, deadline=time.monotonic() - 1)


class BoundedWorker(unittest.TestCase):
    def test_only_exact_success_token_and_zero_exit_are_accepted(self):
        emit = "import sys; sys.stdout.buffer.write(b'adopted\\n')"
        for code, expected in ((emit, True), (emit + "; raise SystemExit(1)", False),
                               ("import sys; sys.stdout.buffer.write(b'adopted\\r\\n')", False),
                               ("print('adopted extra')", False), ("print('')", False),
                               ("print('adopted'); print('details')", False)):
            with self.subTest(code=code):
                self.assertEqual(adoption.bounded_worker([sys.executable, '-c', code]), expected)
        self.assertFalse(adoption.bounded_worker(['/nonexistent/optional-worker']))

    def test_actual_timeout_kills_and_reaps_the_one_worker(self):
        # The adoption worker is Python-only and may not launch child processes.
        children = []
        actual = subprocess.Popen
        def remember(*args, **kwargs):
            child = actual(*args, **kwargs)
            children.append(child)
            return child
        with tempfile.TemporaryDirectory() as directory:
            started = Path(directory) / 'started'
            code = f"from pathlib import Path; import time; Path({str(started)!r}).touch(); time.sleep(60)"
            with patch.object(adoption.subprocess, 'Popen', side_effect=remember):
                self.assertFalse(adoption.bounded_worker([sys.executable, '-c', code], timeout=2))
            self.assertTrue(started.exists(), 'toy worker must actually start before timeout')
            self.assertEqual(len(children), 1)
            self.assertIsNotNone(children[0].returncode, 'timeout must reap, not abandon, the worker')
            self.assertIsNotNone(children[0].poll())

    def test_worker_interrupt_exit_codes_propagate_without_full_fallback(self):
        for code in (130, 143):
            with self.subTest(code=code):
                with self.assertRaises(SystemExit) as result:
                    adoption.bounded_worker([sys.executable, '-c', f'raise SystemExit({code})'])
                self.assertEqual(result.exception.code, code)

    @unittest.skipIf(os.name == 'nt', 'POSIX signal status; explicit exit codes are covered everywhere')
    def test_actual_signal_termination_is_preserved(self):
        with self.assertRaises(SystemExit) as result:
            adoption.bounded_worker([sys.executable, '-c', 'import os, signal; os.kill(os.getpid(), signal.SIGTERM)'])
        self.assertEqual(result.exception.code, 143)

    def test_cancellation_is_never_converted_to_adoption_or_fallback(self):
        with patch.object(adoption.subprocess, 'run', side_effect=KeyboardInterrupt), self.assertRaises(KeyboardInterrupt):
            adoption.bounded_worker([sys.executable, '-c', "print('adopted')"])


class ArtifactBinding(unittest.TestCase):
    def setUp(self):
        self.identity = {'candidate': True, 'source': {'run_id': 101, 'attempt': 1}}
        self.run = {'id': 101, 'run_attempt': 1, 'event': 'pull_request', 'workflow_id': 360984778,
                    'path': '.github/workflows/ci.yml', 'repository': {'id': 1375221902, 'full_name': 'KCall360/thresholds-of-ruin'},
                    'head_repository': {'id': 1375221902}, 'head_sha': 'a' * 40,
                    'status': 'completed', 'conclusion': 'success'}
        self.job = {'name': 'Rust (ubuntu-latest, debug, rust)', 'run_id': 101, 'run_attempt': 1, 'head_sha': 'a' * 40,
                    'status': 'completed', 'conclusion': 'success', 'started_at': '2026-10-16T01:00:00Z',
                    'completed_at': '2026-10-16T02:00:00Z'}
        self.artifact = {'id': 1001, 'name': 'verification-ubuntu-latest-debug-rust', 'expired': False,
                         'created_at': '2026-10-16T01:59:00Z', 'updated_at': '2026-10-16T01:59:01Z',
                         'expires_at': '2026-10-30T01:59:00Z', 'workflow_run': {'id': 101, 'repository_id': 1375221902,
                         'head_repository_id': 1375221902, 'head_sha': 'a' * 40}}
        self.api = Mock()
        self.api.get.return_value = self.run
        self.pages = self.enterContext(patch.object(adoption, 'pages', side_effect=lambda api, path, key: {
            'jobs': [self.job], 'artifacts': [self.artifact]}[key]))

    def select(self):
        return adoption.source_artifact(self.api, self.identity, 'ubuntu-latest', 'debug', 'rust',
                                        now=adoption.timestamp('2026-10-16T03:00:00Z'))

    def test_real_api_run_job_artifact_relationships_bind(self):
        self.assertEqual(self.select(), (self.artifact, self.job, self.run))
        self.assertEqual(self.api.get.call_args.args[0], '/repos/KCall360/thresholds-of-ruin/actions/runs/101')

    def test_wrong_attempt_commit_repo_or_time_is_ineligible(self):
        for obj, key, value in ((self.run, 'run_attempt', 2), (self.run, 'conclusion', 'cancelled'),
                                (self.job, 'head_sha', 'b' * 40), (self.job, 'run_id', 102),
                                (self.artifact, 'expired', True), (self.artifact['workflow_run'], 'id', 102),
                                (self.artifact['workflow_run'], 'repository_id', 999),
                                (self.artifact, 'created_at', '2026-10-16T02:01:00Z'),
                                (self.artifact, 'expires_at', '2026-10-16T02:30:00Z')):
            old = obj[key]; obj[key] = value
            with self.subTest(key=key), self.assertRaises(adoption.EvidenceError): self.select()
            obj[key] = old

    def test_missing_or_duplicate_artifact_or_job_is_ineligible(self):
        for key in ('artifacts', 'jobs'):
            for rows in ([], [self.artifact, self.artifact] if key == 'artifacts' else [self.job, self.job]):
                self.pages.side_effect = lambda api, path, requested: rows if requested == key else {
                    'jobs': [self.job], 'artifacts': [self.artifact]}[requested]
                with self.subTest(key=key, count=len(rows)), self.assertRaises(adoption.EvidenceError): self.select()


class OptionalCLI(unittest.TestCase):
    ARGS = ['--platform', 'ubuntu-latest', '--profile', 'debug', '--partition', 'rust']

    def setUp(self):
        self.directory = self.enterContext(tempfile.TemporaryDirectory())
        self.root = Path(self.directory)
        self.enterContext(patch.object(adoption.verify, 'ROOT', self.root))
        self.output = io.BytesIO()
        self.enterContext(patch.object(adoption.sys, 'stdout', SimpleNamespace(buffer=self.output)))

    def test_parent_accepts_only_successful_worker_and_writes_truthful_source_receipt(self):
        def worker(command):
            proof = Path(command[command.index('--worker-proof') + 1])
            proof.write_text(json.dumps({'source': {'run_id': 101}, 'artifact_id': 12, 'artifact_digest': 'sha256:fixture',
                                         'cell': ['ubuntu-latest', 'debug', 'rust']}))
            return True
        with patch.object(adoption, 'bounded_worker', side_effect=worker):
            self.assertEqual(adoption.main(self.ARGS), 0)
        self.assertEqual(self.output.getvalue(), b'adopted\n')
        receipt = json.loads((self.root / '.local/verify/adoption.json').read_text())
        self.assertIs(receipt['adopted'], True)
        self.assertIs(receipt['tests_executed_here'], False)
        self.assertEqual(receipt['source']['run_id'], 101)

    def test_failure_timeout_exception_or_missing_proof_never_emits_adoption(self):
        for outcome in (False, OSError('private signed URL must not leak'), True):
            with self.subTest(outcome=type(outcome).__name__), patch.object(adoption, 'bounded_worker') as worker:
                if isinstance(outcome, Exception): worker.side_effect = outcome
                else: worker.return_value = outcome
                error = io.StringIO()
                with redirect_stderr(error): self.assertEqual(adoption.main(self.ARGS), 1)
                self.assertEqual(self.output.getvalue(), b'')
                self.assertEqual(error.getvalue(), '')
                receipt = json.loads((self.root / '.local/verify/adoption.json').read_text())
                self.assertIs(receipt['adopted'], False)
                self.assertNotIn('private', json.dumps(receipt))

    def test_worker_cancellation_propagates_and_emits_no_adoption(self):
        with patch.object(adoption, 'bounded_worker', side_effect=KeyboardInterrupt), self.assertRaises(KeyboardInterrupt):
            adoption.main(self.ARGS)
        self.assertEqual(self.output.getvalue(), b'')

    def test_nonmain_or_missing_exact_cache_hit_stops_before_api_or_download(self):
        environment = {'GITHUB_EVENT_NAME': 'push', 'GITHUB_REF': 'refs/heads/main',
                       'GITHUB_REPOSITORY': adoption.REPOSITORY, 'CI_RUST_CACHE_OUTCOME': 'success',
                       'CI_RUST_CACHE_HIT': 'true', 'CI_BUILD_RECIPE': 'a' * 64}
        for key, value in (('GITHUB_EVENT_NAME', 'pull_request'), ('GITHUB_EVENT_NAME', 'schedule'),
                           ('GITHUB_EVENT_NAME', 'workflow_dispatch'), ('CI_RUST_CACHE_HIT', ''),
                           ('CI_RUST_CACHE_HIT', 'false'), ('CI_RUST_CACHE_OUTCOME', 'failure'), ('CI_BUILD_RECIPE', '')):
            with self.subTest(key=key, value=value), patch.dict(os.environ, {**environment, key: value}, clear=True), \
                    patch.object(adoption, 'GitHub') as api:
                self.assertEqual(adoption.main([*self.ARGS, '--worker-proof', str(self.root / 'proof.json')]), 1)
                api.assert_not_called()
        self.assertEqual(self.output.getvalue(), b'')


class FinalRechecks(unittest.TestCase):
    def setUp(self):
        self.directory = self.enterContext(tempfile.TemporaryDirectory())
        root = Path(self.directory)
        self.proof = root / 'proof.json'
        self.enterContext(patch.object(adoption.verify, 'ROOT', root))
        before = root / '.local/verify/observations/before.json'
        before.parent.mkdir(parents=True)
        before.write_text('{"execution_limits":{"jobs":4,"test_workers":2}}')
        identity = {'candidate': True, 'target_sha': 'c' * 40, 'target_run': 102,
                    'source': {'run_id': 101, 'attempt': 1, 'checkout_sha': 'b' * 40, 'tree': 'd' * 40,
                               'url': f'https://github.com/{adoption.REPOSITORY}/actions/runs/101/attempts/1'}}
        self.enterContext(patch.dict(os.environ, {'GITHUB_EVENT_NAME': 'push', 'GITHUB_REF': 'refs/heads/main',
            'GITHUB_REPOSITORY': adoption.REPOSITORY, 'GITHUB_SHA': 'c' * 40, 'GITHUB_RUN_ID': '102',
            'CI_RUST_CACHE_OUTCOME': 'success', 'CI_RUST_CACHE_HIT': 'true', 'CI_BUILD_RECIPE': 'a' * 64,
            'CI_QUALIFICATION': json.dumps(identity), 'GH_TOKEN': 'fixture-api-token'}, clear=True))
        self.run = {'id': 101, 'run_attempt': 1, 'event': 'pull_request', 'workflow_id': 360984778,
                    'path': '.github/workflows/ci.yml', 'repository': {'id': 1375221902, 'full_name': adoption.REPOSITORY},
                    'head_repository': {'id': 1375221902}, 'head_sha': 'a' * 40, 'pull_requests': [],
                    'run_started_at': '2026-10-16T01:00:00Z', 'updated_at': '2026-10-16T02:00:00Z',
                    'status': 'completed', 'conclusion': 'success'}
        self.latest = dict(self.run)
        self.main = {'object': {'sha': 'c' * 40}}
        self.target = {**self.run, 'id': 102, 'event': 'push', 'status': 'in_progress', 'conclusion': None, 'head_sha': 'c' * 40}
        api = Mock()
        api.get.side_effect = lambda path: self.latest if path.endswith('/101') else self.target if path.endswith('/102') else self.main
        self.enterContext(patch.object(adoption, 'GitHub', return_value=api))
        self.enterContext(patch.object(adoption, 'source_artifact', return_value=({'id': 12, 'digest': 'sha256:fixture'}, {}, self.run)))
        self.enterContext(patch.object(adoption, 'download_artifact', return_value=b'fixture'))
        self.enterContext(patch.object(adoption, 'artifact_records', return_value={}))
        self.validator = self.enterContext(patch.object(adoption, 'validate_cell'))
        self.args = SimpleNamespace(platform='ubuntu-latest', profile='debug', partition='rust')

    def test_completed_comparison_rechecks_source_and_active_target_before_proof(self):
        with patch.object(adoption.verify.subprocess, 'run', side_effect=AssertionError('no descendant')):
            adoption.attempt(self.args, self.proof)
        self.validator.assert_called_once()
        receipt = json.loads(self.proof.read_text())
        self.assertEqual(receipt['artifact_id'], 12)
        self.assertEqual(receipt['source']['url'], f'https://github.com/{adoption.REPOSITORY}/actions/runs/101/attempts/1')
        self.assertEqual(receipt['target_sha'], 'c' * 40)
        self.assertEqual(receipt['target_run'], 102)
        self.assertEqual(receipt['target_url'], f'https://github.com/{adoption.REPOSITORY}/actions/runs/102/attempts/1')

    def test_rerun_main_advance_or_target_cancellation_rejects_after_comparison(self):
        for obj, key, value in ((self.latest, 'run_attempt', 2), (self.main['object'], 'sha', 'e' * 40),
                                (self.target, 'status', 'completed'), (self.target, 'conclusion', 'cancelled'),
                                (self.target, 'head_sha', 'e' * 40)):
            old = obj[key]; obj[key] = value
            with self.subTest(key=key), self.assertRaises(adoption.EvidenceError): adoption.attempt(self.args, self.proof)
            self.assertFalse(self.proof.exists())
            obj[key] = old


if __name__ == '__main__':
    unittest.main()
