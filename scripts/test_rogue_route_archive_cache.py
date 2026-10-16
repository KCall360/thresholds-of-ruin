"""Cheap hostile-input contracts for attempt-local lossless archive verification."""
import gzip
import hashlib
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import rogue_earned_route_process as route


class ArchiveVerificationTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def archive(self, name='one', raw=b'ordinary frame\n' * 100):
        path = self.root / (name + '.gz')
        path.write_bytes(gzip.compress(raw))
        return {'retained': path.name, 'compressed_sha256': route.digest(path),
                'uncompressed_sha256': hashlib.sha256(raw).hexdigest(), 'uncompressed_bytes': len(raw)}

    def test_repeated_verification_rehashes_compressed_bytes_but_decodes_once(self):
        row = self.archive()
        cache = route.ArchiveVerificationCache()
        with patch.object(route, '_hash_compressed', wraps=route._hash_compressed) as hashed, \
                patch.object(route.gzip, 'open', wraps=gzip.open) as decoded:
            for _ in range(3):
                route._verify_compressed(self.root, row, cache=cache)
        self.assertEqual(hashed.call_count, 3)
        self.assertEqual(decoded.call_count, 1)

    def test_disabled_timing_adds_no_archive_stat_probe(self):
        row = self.archive()
        cache = route.ArchiveVerificationCache(measurements=route.RouteMeasurements())
        stat = Path.stat
        def continuity_only(path, *, follow_symlinks=True):
            self.assertFalse(follow_symlinks, 'metrics-only stat')
            return stat(path, follow_symlinks=False)
        with patch.object(Path, 'stat', autospec=True, side_effect=continuity_only):
            route._verify_compressed(self.root, row, cache=cache)

    def test_new_verifier_has_no_trust_from_manifest_or_previous_instance(self):
        row = self.archive()
        with patch.object(route.gzip, 'open', wraps=gzip.open) as decoded:
            for _ in range(2):
                route._verify_compressed(self.root, row, cache=route.ArchiveVerificationCache())
        self.assertEqual(decoded.call_count, 2)

    def test_old_archive_mutation_is_rejected_after_later_archive_is_cached(self):
        first, second = self.archive(), self.archive('two', b'next')
        cache = route.ArchiveVerificationCache()
        for row in (first, second):
            route._verify_compressed(self.root, row, cache=cache)
        path = self.root / first['retained']
        raw = bytearray(path.read_bytes()); raw[-1] ^= 1; path.write_bytes(raw)
        with self.assertRaisesRegex(route.EvidenceError, 'compressed log changed'):
            route._verify_compressed(self.root, first, cache=cache)

    def test_manifest_raw_hash_or_length_mismatch_cannot_hit_verified_content(self):
        row = self.archive(); cache = route.ArchiveVerificationCache()
        route._verify_compressed(self.root, row, cache=cache)
        for field, value in [('uncompressed_sha256', '0' * 64), ('uncompressed_bytes', row['uncompressed_bytes'] + 1)]:
            with self.subTest(field=field), self.assertRaisesRegex(route.EvidenceError, 'exact bytes'):
                route._verify_compressed(self.root, {**row, field: value}, cache=cache)

    def test_initial_trust_binds_the_compressed_bytes_actually_decoded(self):
        row = self.archive(raw=b'wrong'); path = self.root / row['retained']
        row.update(uncompressed_sha256=hashlib.sha256(b'right').hexdigest(), uncompressed_bytes=5)
        hash_stream = route._hash_compressed
        def change_after_hash(stream):
            result = hash_stream(stream)
            path.write_bytes(gzip.compress(b'right'))
            return result
        cache = route.ArchiveVerificationCache()
        with patch.object(route, '_hash_compressed', side_effect=change_after_hash), \
                self.assertRaisesRegex(route.EvidenceError, 'compressed bytes changed during verification'):
            route._verify_compressed(self.root, row, cache=cache)
        self.assertEqual(len(cache._verified), 0)

    def test_hash_and_decode_use_one_owned_handle_until_cache_acceptance(self):
        row = self.archive(); cache = route.ArchiveVerificationCache()
        opened = []
        original = Path.open
        def tracked(path, *args, **kwargs):
            stream = original(path, *args, **kwargs)
            opened.append(stream)
            return stream
        gzip_open = gzip.open
        def decoded(reader, mode):
            self.assertIs(reader.stream, opened[0])
            self.assertFalse(reader.stream.closed)
            return gzip_open(reader, mode)
        with patch.object(Path, 'open', autospec=True, side_effect=tracked), \
                patch.object(route.gzip, 'open', side_effect=decoded):
            route._verify_compressed(self.root, row, cache=cache)
        self.assertEqual(len(opened), 1)
        self.assertTrue(opened[0].closed)
        self.assertEqual(len(cache._verified), 1)

    def test_lru_eviction_is_bounded_and_requires_full_reverification(self):
        rows = [self.archive(str(n), bytes([n])) for n in range(3)]
        cache = route.ArchiveVerificationCache(max_entries=2)
        with patch.object(route.gzip, 'open', wraps=gzip.open) as decoded:
            for index in (0, 1, 0, 2, 1):
                route._verify_compressed(self.root, rows[index], cache=cache)
                self.assertLessEqual(len(cache._verified), 2)
        self.assertEqual(decoded.call_count, 4)

    def test_capacity_cannot_disable_bound_or_grow_without_limit(self):
        for capacity in (0, -1, 257, True, 1.5):
            with self.subTest(capacity=capacity), self.assertRaises(ValueError):
                route.ArchiveVerificationCache(max_entries=capacity)

    def test_truncated_archive_with_matching_compressed_hash_never_establishes_trust(self):
        row = self.archive(); path = self.root / row['retained']
        path.write_bytes(path.read_bytes()[:-5]); row['compressed_sha256'] = route.digest(path)
        cache = route.ArchiveVerificationCache()
        with self.assertRaises((EOFError, OSError)):
            route._verify_compressed(self.root, row, cache=cache)
        self.assertEqual(len(cache._verified), 0)

    def test_compression_and_verification_share_cache_without_reclaiming_failed_raw(self):
        process = self.root / 'process-00-headless'; process.mkdir()
        path = process / 'stdout.log'; path.write_bytes(b'raw evidence')
        cache = route.ArchiveVerificationCache()
        with patch.object(route.gzip, 'open', wraps=gzip.open) as opened:
            rows = route.compress_completed_logs(self.root, cache=cache)
            self.assertEqual(route.compress_completed_logs(self.root, cache=cache), rows)
        self.assertEqual(sum(call.args[1] == 'rb' for call in opened.call_args_list), 1)
        self.assertFalse(path.exists())

    def test_compression_failure_preserves_raw_and_does_not_publish_trust(self):
        process = self.root / 'process-00-headless'; process.mkdir()
        path = process / 'stdout.log'; path.write_bytes(b'raw evidence')
        cache = route.ArchiveVerificationCache()
        with patch.object(route.shutil, 'copyfileobj', side_effect=OSError('write interrupted')), \
                self.assertRaisesRegex(OSError, 'write interrupted'):
            route.compress_completed_logs(self.root, cache=cache)
        self.assertEqual(path.read_bytes(), b'raw evidence')
        self.assertFalse((self.root / 'retained-raw-logs.json').exists())
        self.assertEqual(len(cache._verified), 0)

    def test_corrupt_compression_result_preserves_raw_and_manifest(self):
        process = self.root / 'process-00-headless'; process.mkdir()
        path = process / 'stdout.log'; path.write_bytes(b'raw evidence')
        cache = route.ArchiveVerificationCache()
        with patch.object(route.shutil, 'copyfileobj', side_effect=lambda source, target, **kw: target.write(b'wrong')), \
                self.assertRaisesRegex(route.EvidenceError, 'exact bytes'):
            route.compress_completed_logs(self.root, cache=cache)
        self.assertEqual(path.read_bytes(), b'raw evidence')
        self.assertFalse((self.root / 'retained-raw-logs.json').exists())
        self.assertEqual(len(cache._verified), 0)

    def test_saved_manifest_contains_no_cache_trust(self):
        process = self.root / 'process-00-headless'; process.mkdir()
        (process / 'stdout.log').write_bytes(b'raw evidence')
        route.compress_completed_logs(self.root, cache=route.ArchiveVerificationCache())
        rows = json.loads((self.root / 'retained-raw-logs.json').read_text())
        self.assertEqual(set(rows[0]), {'original', 'retained', 'uncompressed_sha256',
                                       'uncompressed_bytes', 'compressed_sha256'})

    def test_route_compression_and_resume_share_only_their_own_cache(self):
        process = self.root / 'process-00-headless'; process.mkdir()
        (process / 'stdout.log').write_bytes(b'raw evidence')
        with patch.object(route, 'frozen_inputs', return_value={}):
            first = route.RouteExecution(Mock(), self.root, actor=1, seed=42, package=self.root / 'missing')
            second = route.RouteExecution(Mock(), self.root, actor=1, seed=42, package=self.root / 'missing')
        report = {'outcome': 'acceptance_prefix_saved', 'controller': {'commands': 128}}
        report['retained_segment'] = route.retain_segment(self.root, self.root / 'unused.db', report)
        (self.root / 'attempt.json').write_text(json.dumps(report))
        with patch.object(route.gzip, 'open', wraps=gzip.open) as opened:
            first.compress_logs()
            with patch.object(route, 'validate_resume_binding', side_effect=route.EvidenceError('stop before launch')), \
                    self.assertRaisesRegex(route.EvidenceError, 'stop before launch'):
                first.resume()
        self.assertEqual(sum(call.args[1] == 'rb' for call in opened.call_args_list), 1)
        self.assertEqual(len(second.archive_cache._verified), 0)
        first.harness.server.assert_not_called()


class RouteMeasurementTests(unittest.TestCase):
    def test_disabled_measurement_reads_no_clock_and_writes_no_sidecar(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(route.time, 'perf_counter_ns') as clock:
            metrics = route.RouteMeasurements()
            with metrics.span('persist'):
                metrics.add('archive_hash', size=100)
            metrics.segment(None)
            metrics.write(directory)
            clock.assert_not_called()
            self.assertEqual(list(Path(directory).iterdir()), [])

    def test_nested_timings_are_inclusive_explicit_and_do_not_change_report(self):
        with tempfile.TemporaryDirectory() as directory, \
                patch.object(route.time, 'perf_counter_ns', side_effect=[0, 10, 20, 30, 50, 60]):
            metrics = route.RouteMeasurements(enabled=True)
            with metrics.span('execute'), metrics.span('persist'):
                pass
            metrics.write(directory)
            report = json.loads((Path(directory) / 'timing.json').read_text())
        self.assertEqual(report['stats']['execute'], {'calls': 1, 'elapsed_ns': 40, 'bytes': 0})
        self.assertEqual(report['stats']['persist']['elapsed_ns'], 10)
        self.assertEqual(report['observer_elapsed_ns'], 60)
        self.assertIn('do not sum', report['timing_semantics'])
        self.assertEqual(set(report['stats']), set(route.RouteMeasurements.NAMES))

    def test_archive_counters_distinguish_full_decode_from_hash_only_hit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); process = root / 'process-00-headless'; process.mkdir()
            raw = b'raw evidence'; (process / 'stdout.log').write_bytes(raw)
            metrics = route.RouteMeasurements(enabled=True)
            cache = route.ArchiveVerificationCache(measurements=metrics)
            rows = route.compress_completed_logs(root, cache=cache)
            route.compress_completed_logs(root, cache=cache)
            self.assertEqual(metrics.stats['archive_cache_miss']['calls'], 1)
            self.assertEqual(metrics.stats['archive_cache_hit']['calls'], 1)
            self.assertEqual(metrics.stats['archive_decode']['calls'], 1)
            self.assertEqual(metrics.stats['archive_decode']['bytes'], len(raw))
            self.assertEqual(metrics.stats['archive_hash']['calls'], 2)
            self.assertEqual(metrics.stats['archive_hash']['bytes'], 2 * (root / rows[0]['retained']).stat().st_size)

    def test_segment_detail_is_bounded_with_explicit_omission_count(self):
        metrics = route.RouteMeasurements(enabled=True)
        report = {'outcome': 'acceptance_prefix_saved', 'controller': {'commands': 12},
                  'transport_boundary': {'output_bytes': route.CHUNK_SOFT_BYTES}}
        execution = SimpleNamespace(report=report, output_budget=SimpleNamespace(total_bytes=route.CHUNK_SOFT_BYTES))
        original = json.dumps(report, sort_keys=True)
        for _ in range(metrics.SEGMENT_LIMIT + 3):
            metrics.segment(execution)
        self.assertEqual(len(metrics.segments), metrics.SEGMENT_LIMIT)
        self.assertEqual(metrics.omitted_segments, 3)
        self.assertEqual(metrics.segments[0]['boundary_reason'], 'output_soft_limit')
        self.assertEqual(json.dumps(report, sort_keys=True), original)
        with tempfile.TemporaryDirectory() as directory:
            metrics.write(directory)
            self.assertLess((Path(directory) / 'timing.json').stat().st_size, 32768)

    def test_timing_write_failure_is_visible_without_masking_original_failure(self):
        class Attempt:
            measurements = route.RouteMeasurements(enabled=True)
            output = Path('unused')

            @route._timed_route
            def execute(self):
                self._measurement_admitted = True
                raise route.EvidenceError('original route failure')
        with patch.object(route, 'write_json', side_effect=OSError('disk full')), \
                patch('sys.stderr') as stderr, self.assertRaisesRegex(route.EvidenceError, 'original route failure'):
            Attempt().execute()
        self.assertIn('timing sidecar unavailable', ''.join(call.args[0] for call in stderr.write.call_args_list))

    def test_serializer_or_closed_stderr_cannot_replace_original_route_failure(self):
        class Attempt:
            measurements = route.RouteMeasurements(enabled=True)
            output = Path('unused')

            @route._timed_route
            def execute(self):
                self._measurement_admitted = True
                raise route.EvidenceError('original route failure')
        for error in (ValueError('artifact cap'), TypeError('serialization'), OSError('disk full')):
            with self.subTest(error=type(error).__name__), \
                    patch.object(route, 'write_json', side_effect=error), \
                    patch('sys.stderr') as stderr, self.assertRaisesRegex(route.EvidenceError, 'original route failure'):
                stderr.write.side_effect = OSError('closed stderr')
                Attempt().execute()

    def test_refused_execute_does_not_overwrite_existing_attempt_timing(self):
        with tempfile.TemporaryDirectory() as directory, \
                patch.dict('os.environ', {'TOR_EARNED_ROUTE_TIMING': '1'}), \
                patch.object(route, 'frozen_inputs', return_value={}):
            root = Path(directory); sidecar = root / 'timing.json'; sidecar.write_bytes(b'original timing')
            execution = route.RouteExecution(Mock(), root, actor=1, seed=42, package=root / 'missing')
            with self.assertRaises(FileExistsError):
                execution.execute()
            self.assertEqual(sidecar.read_bytes(), b'original timing')

    def test_refused_resume_does_not_overwrite_existing_attempt_timing(self):
        with tempfile.TemporaryDirectory() as directory, \
                patch.dict('os.environ', {'TOR_EARNED_ROUTE_TIMING': '1'}), \
                patch.object(route, 'frozen_inputs', return_value={}):
            root = Path(directory); sidecar = root / 'timing.json'; sidecar.write_bytes(b'original timing')
            report = {'outcome': 'acceptance_prefix_saved', 'controller': {'commands': 128}}
            report['retained_segment'] = route.retain_segment(root, root / 'unused.db', report)
            (root / 'attempt.json').write_text(json.dumps(report))
            execution = route.RouteExecution(Mock(), root, actor=1, seed=42, package=root / 'missing')
            with patch.object(route, 'validate_resume_binding', side_effect=route.EvidenceError('identity changed')), \
                    self.assertRaisesRegex(route.EvidenceError, 'identity changed'):
                execution.resume()
            self.assertEqual(sidecar.read_bytes(), b'original timing')

    def test_failed_spans_are_counted_and_unknown_counter_names_are_rejected(self):
        metrics = route.RouteMeasurements(enabled=True)
        with self.assertRaisesRegex(ValueError, 'failure'), metrics.span('persist'):
            raise ValueError('failure')
        self.assertEqual(metrics.stats['persist']['calls'], 1)
        with self.assertRaises(KeyError):
            metrics.add('unbounded-dynamic-name', calls=1)


if __name__ == '__main__':
    unittest.main()
