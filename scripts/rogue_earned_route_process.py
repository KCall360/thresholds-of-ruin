"""Bounded ordinary earned-route execution and retained evidence.

The controller receives public observations and owner advancement only. Private
save/flow audit runs after decisions, never as navigation or combat input. This
revision does not certify a complete measurement until all replay/flow gates pass.
"""
import argparse
from collections import Counter, OrderedDict
from contextlib import closing, contextmanager, nullcontext
import copy
from functools import wraps
import gzip
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import time

from arena_evaluation import digest, files, write_json
from checkpoint_payload import decode_checkpoint_payload
from scalar_wire import u64
from process_harness import ProcessTestCase, ROOT, SUFFIX, binaries, settle_rogue_action
from rogue_earned_route import POLICY_REVISION, CHECKPOINT_DEPTHS, starting_body_hd, ControllerError
from ruleset_sources import resolve_with_dependencies
from process_work_budget import ProcessBudgetExceeded, ProcessOutputBudget

def _disclosed_state_payload(state):
    return {key: value for key, value in state.items()
            if key not in ('snapshot', 'revision')}


def validate_restored_picture(current, expected):
    """A new attachment issues fresh picture capabilities, not new world facts."""
    if (_disclosed_state_payload(current['state']) != _disclosed_state_payload(expected['state'])
            or current['history'] != expected['history']):
        raise EvidenceError('Source-free restore changed exact state/history')


FORMAT = 'tor-rogue-earned-route-v1'
MINIMUM_FREE_BYTES = 512 * 1024 * 1024
CHUNK_SOFT_BYTES = 16 * 1024 * 1024
CHUNK_HARD_BYTES = 32 * 1024 * 1024
CHUNK_COMMANDS = 128
CHECKPOINTS = CHECKPOINT_DEPTHS
REPLAY_LIMITATION = ('Exact source-free replay and separately labeled normalized execution from a '
                    'saved initial boundary are required. Independent fresh seed runs are separate acceptance attempts.')


class EvidenceError(ValueError):
    """An attempted run cannot support a complete evidence claim."""


class CommandBudgetCensor(Exception):
    """The next mutating command was not issued; preserve the accepted tail."""


def fingerprint(paths):
    return {str(Path(path).resolve(strict=True)): digest(Path(path)) for path in paths}


def verify_fingerprint(frozen):
    for path, expected in frozen.items():
        try:
            actual = digest(Path(path))
        except OSError as error:
            raise EvidenceError(f'Frozen input missing: {path}') from error
        if actual != expected:
            raise EvidenceError(f'Frozen input changed: {path}')


def validate_receipts(rows, selected):
    scopes = set()
    for row in rows:
        if row['entrant'] != selected:
            raise EvidenceError('Activation receipt has the wrong entrant')
        if row['scope'] in scopes:
            raise EvidenceError('Repeated floor activation receipt: duplicate scope')
        scopes.add(row['scope'])
    return rows


def compare_replay(pristine, replay):
    return [field for field in ('commands', 'state', 'history', 'receipts', 'controller', 'flows')
            if pristine.get(field) != replay.get(field)]


def classify_failure(error):
    if isinstance(error, (TimeoutError, subprocess.TimeoutExpired, OSError, ProcessBudgetExceeded)) or any(
            text in str(error) for text in ('Process output deadline', 'Unexpected process exit',
                                           'JSON artifact exceeds bounded size')):
        return 'failed_infrastructure'
    return 'failed_evidence' if isinstance(error, EvidenceError) else 'failed_controller'


def stock(view, supply):
    return sum(int(row['quantity']) for row in view['inventory']
               if (row.get('known_supply') or {}).get('id') == supply)


def record_acquisition(before, action, after):
    if action.get('type') != 'take':
        return None
    item = next((row['item'] for row in before['ground_items']
                 if row['item']['id'] == action['item']), None)
    if item is None or (item.get('known_supply') or {}).get('id') not in ('ration', 'bandage'):
        raise EvidenceError('Take has no disclosed finite supply stack')
    supply, quantity = item['known_supply']['id'], int(item['quantity'])
    if int(action['quantity']) != quantity or stock(after, supply) != stock(before, supply) + quantity:
        raise EvidenceError('Accepted take and owner stock do not prove full stack acquisition')
    if any(row['item']['id'] == action['item'] for row in after['ground_items']):
        raise EvidenceError('Accepted full-stack take left its ground stack visible')
    return {'supply': supply, 'quantity': quantity, 'item': item['id']}


def validate_initial_anchor(begin, independent, actor):
    """Bind the first diagnostic row to the retained ordinary initial save."""
    if (set(independent) != {'lineage', 'branch', 'history_sequence', 'entry_id', 'tick'}
            or begin.get('event') != 'survival.begin' or begin.get('selected_actor') != actor
            or any(begin.get(key) != value for key, value in independent.items())):
        raise EvidenceError('Whole-run diagnostics lost or changed the independent initial boundary')


def read_authoritative_evidence(save, selected):
    """Read one durable audit, not an action-selection oracle. No saved edits."""
    with closing(sqlite3.connect(Path(save).resolve().as_uri() + '?mode=ro', uri=True)) as db, db:
        db.execute('BEGIN')
        base = json.loads(db.execute('SELECT frame FROM journal WHERE sequence=0').fetchone()[0][24:])['archive']
        rows = db.execute('SELECT sequence,frame FROM journal WHERE sequence>0 UNION ALL '
                          'SELECT sequence,frame FROM history WHERE sequence>0 ORDER BY sequence').fetchall()
        checkpoint = db.execute('SELECT sequence,payload FROM checkpoint').fetchone()
    expected = list(range(1, len(rows) + 1))
    if [row[0] for row in rows] != expected:
        raise EvidenceError('Duplicate, missing or unordered durable history sequence')
    records = [json.loads(frame[24:])['record'] for _, frame in rows]
    receipts = list(base['initial_activations'])
    for record in records:
        receipts.extend(record['activations'])
    validate_receipts(receipts, selected)
    if checkpoint is None:
        if rows:
            raise EvidenceError('Accepted work has no current checkpoint')
        return {'anchor': {'lineage': base['branch'], 'branch': base['branch'],
                          'history_sequence': 0, 'entry_id': None, 'tick': 0},
                'receipts': receipts, 'records': records, 'creature': None, 'selected': base['scenario']['package']['selected']}
    sequence, payload = checkpoint
    saved = json.loads(decode_checkpoint_payload(payload))
    if sequence != len(rows) or saved['sequence'] != sequence or saved['record_count'] != sequence:
        raise EvidenceError('Checkpoint is not the complete durable history endpoint')
    branch = saved['current_branch']
    if records and records[-1]['entry']['branch'] != branch:
        raise EvidenceError('Final checkpoint and history branches disagree')
    actors = saved['shared']['actors'][saved['game']['actors']]
    creature = copy.deepcopy(actors[str(selected)]['combat']['creature'])
    return {'anchor': {'lineage': base['branch'], 'branch': branch,
                      'history_sequence': sequence, 'entry_id': records[-1]['entry']['id'] if records else None,
                      'tick': saved['game']['tick']}, 'receipts': receipts, 'records': records,
            'creature': creature, 'selected': base['scenario']['package']['selected']}


def verify_earned_history(evidence, selected, overview):
    creature = evidence['creature']
    source = 'warrior' if selected == 1 else 'mage'
    if evidence['selected'] != selected or creature is None:
        raise EvidenceError('Milestone lacks saved selected-actor provenance')
    dice = creature['build']['hd']
    progress = creature['earned_progress']
    build = creature['build']
    body = build.get('body_build')
    class_source = {'kind': 'class', 'id': source}
    if body is None:
        supported = len(dice) >= 2 and all(die['source'] == class_source and die.get('owner') is None for die in dice)
    else:
        # A current PC earns its second PERSONAL class HD while retaining the
        # exact one-row birth package. Other forms/compositions are unsupported
        # evidence, not inferred advancement or a compatibility conversion.
        personal = dice[1:]
        owners = [die.get('owner') for die in personal]
        supported = (len(dice) >= 3 and body['active'] == body['base']
                     and body['birth_personal_count'] == 1
                     and len(body['base']['advancement']) == 1
                     and dice[0]['source'] == {'kind': 'racial'}
                     and dice[0].get('owner') == {'kind': 'body', 'form': body['base']['id'], 'ordinal': 0}
                     and all(die['source'] == class_source for die in personal)
                     and all(isinstance(owner, dict) and owner.get('kind') == 'personal'
                             and type(owner.get('id')) is int and owner['id'] > 0 for owner in owners)
                     and len({owner['id'] for owner in owners}) == len(personal))
    if (not supported or progress['authored_baseline'] != 1 or int(progress['earned_xp']) <= 0 or int(progress['spent_xp']) <= 0
            or progress['highest_earned_tier'] < 2 or progress['highest_applied_tier'] < 2):
        raise EvidenceError('Saved history is not an actual earned second personal class HD')
    for field in ('earned_xp', 'spent_xp', 'highest_earned_tier', 'highest_applied_tier'):
        if int(progress[field]) != u64(overview['growth'][field]):
            raise EvidenceError('Owner advancement and saved earned progression disagree')
    return copy.deepcopy(dice)


def frozen_inputs(bin_directory):
    tracked = subprocess.check_output(['git', 'ls-files', '-z'], cwd=ROOT).decode().split('\0')
    paths = [ROOT / relative for relative in tracked if relative]
    # Runtime dependencies must be pinned even before a new file is staged.
    paths.extend(ROOT / 'scripts' / name for name in (
        'rogue_earned_route.py', 'rogue_earned_route_process.py', 'process_work_budget.py', 'scalar_wire.py'))
    paths.extend(Path(bin_directory) / (name + SUFFIX) for name in ('tor-server', 'tor-client-headless', 'tor-scenario'))
    return fingerprint(paths)


def source_snapshot(package, output, compiler, *, reference):
    """Retain explicit source bytes and their dependency identity, without Git policy."""
    if not isinstance(reference, str) or not reference:
        raise EvidenceError('Source snapshot requires explicit provenance')
    package, output = Path(package), Path(output)
    original = files(package)
    output.mkdir(parents=True, exist_ok=False)
    shutil.copytree(package, output / 'package', ignore=shutil.ignore_patterns('.compiled'))
    _, dependencies = resolve_with_dependencies(output / 'package/rules.toml', compiler)
    if files(package) != original:
        raise EvidenceError('Source changed while materializing snapshot')
    manifest = {'reference': reference, 'package_inputs': files(output / 'package'),
                'rules_dependencies': dependencies}
    write_json(output / 'source.json', manifest)
    return output


def configured_arm_sources():
    """No live-production fallback: both independently materialized inputs are explicit."""
    values = {arm: os.environ.get('TOR_EARNED_ROUTE_' + arm.upper() + '_SOURCE')
              for arm in ('production', 'candidate')}
    if not all(values.values()):
        raise EvidenceError('Supply explicit production and candidate source snapshots')
    return {arm: Path(path) for arm, path in values.items()}


def prepare_campaign(output, bin_directory, *, arm_sources=None):
    """Freeze eight new slots; old campaigns and source snapshots are never rewritten."""
    from rogue_earned_route import fixed_cohort
    arm_sources = configured_arm_sources() if arm_sources is None else arm_sources
    if set(arm_sources) != {'production', 'candidate'}:
        raise EvidenceError('Both explicit arm snapshots are required')
    output.mkdir(parents=True, exist_ok=False)
    inventory = {'format': FORMAT, 'policy_revision': POLICY_REVISION, 'slots': fixed_cohort(),
                 'preparation_status': 'running', 'measurement_status': 'unrun',
                 'limitations': [REPLAY_LIMITATION, 'Later earned points remain unspent.',
                     'Depth27 is an earned arrival, not Vampire combat survival.']}
    write_json(output / 'inventory.json', inventory)
    try:
        inventory['frozen_inputs'] = frozen_inputs(bin_directory)
        compiler = Path(bin_directory) / ('tor-scenario' + SUFFIX)
        inventory['arm_sources'] = {}
        for arm in ('production', 'candidate'):
            source = Path(arm_sources[arm]).resolve(strict=True)
            manifest_hash = digest(source / 'source.json')
            manifest = json.loads((source / 'source.json').read_text(encoding='utf-8'))
            if (set(manifest) != {'reference', 'package_inputs', 'rules_dependencies'}
                    or not isinstance(manifest['reference'], str) or not manifest['reference']):
                raise EvidenceError('Invalid explicit source snapshot provenance')
            expected = manifest['package_inputs']
            if files(source / 'package') != expected:
                raise EvidenceError('Explicit source snapshot changed')
            package = output / 'inputs' / arm / 'package'
            package.parent.mkdir(parents=True)
            shutil.copytree(source / 'package', package, ignore=shutil.ignore_patterns('.compiled'))
            _, dependencies = resolve_with_dependencies(package / 'rules.toml', compiler)
            if dependencies != manifest['rules_dependencies']:
                raise EvidenceError('Current compiler differs from pinned source rules dependencies')
            result = subprocess.run([str(compiler), 'validate', str(package)], capture_output=True,
                                    text=True, encoding='utf-8', timeout=60)
            (output / (arm + '-validation.log')).write_text(result.stdout + result.stderr, encoding='utf-8')
            if result.returncode:
                raise EvidenceError('Explicit ' + arm + ' snapshot failed ordinary validation')
            if (files(source / 'package') != expected
                    or digest(source / 'source.json') != manifest_hash):
                raise EvidenceError('Explicit source snapshot changed during compilation')
            inventory['arm_sources'][arm] = {**manifest, 'source': str(source),
                                             'manifest_sha256': manifest_hash}
        inventory['packages'] = {arm: files(output / 'inputs' / arm / 'package')
                                 for arm in ('production', 'candidate')}
        inventory['environment'] = {'python': sys.version, 'platform': sys.platform,
            'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
            'cargo_target_dir': os.environ.get('CARGO_TARGET_DIR'),
            'source_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()}
        verify_fingerprint(inventory['frozen_inputs'])
        inventory['preparation_status'] = 'complete'
    except Exception as error:
        inventory.update(preparation_status='failed', failure={'type': type(error).__name__, 'message': str(error)})
        write_json(output / 'inventory.json', inventory)
        raise
    write_json(output / 'inventory.json', inventory)
    return inventory


def _retained_path(output, relative):
    path = Path(relative)
    if path.is_absolute() or '..' in path.parts or not path.parts:
        raise EvidenceError('Invalid retained artifact path')
    candidate = Path(output) / path
    if any(parent.is_symlink() for parent in (candidate, *candidate.parents)
           if parent != Path(output).parent):
        raise EvidenceError('Retained artifacts cannot use symlinks')
    return candidate


class RouteMeasurements:
    """Opt-in fixed-size counters; nested inclusive times must not be summed."""
    NAMES = ('archive_hash', 'archive_decode', 'archive_cache_hit', 'archive_cache_miss',
             'archive_cache_evict', 'compress_logs', 'execute', 'resume', 'persist',
             'start_server', 'connect_player', 'owner_overview', 'barrier',
             'stop_processes', 'finalize_segment', 'input_fingerprint')
    SEGMENT_LIMIT = 128

    def __init__(self, *, enabled=False):
        self.enabled = enabled
        self.started = time.perf_counter_ns() if enabled else None
        self.stats = {name: {'calls': 0, 'elapsed_ns': 0, 'bytes': 0} for name in self.NAMES}
        self.segments, self.omitted_segments = [], 0
        self.write_failures, self.last_write_error = 0, None

    @contextmanager
    def span(self, name):
        if not self.enabled:
            yield
            return
        row = self.stats[name]
        began = time.perf_counter_ns()
        try:
            yield
        finally:
            row['calls'] += 1
            row['elapsed_ns'] += time.perf_counter_ns() - began

    def add(self, name, *, calls=0, size=0):
        if self.enabled:
            self.stats[name]['calls'] += calls
            self.stats[name]['bytes'] += size

    def segment(self, execution):
        if not self.enabled:
            return
        if len(self.segments) == self.SEGMENT_LIMIT:
            self.omitted_segments += 1
            return
        report = execution.report
        boundary = report.get('transport_boundary', {}) if report.get('outcome') == 'acceptance_prefix_saved' else {}
        self.segments.append({'commands': report.get('controller', {}).get('commands'),
            'outcome': report.get('outcome'),
            'raw_output_bytes': execution.output_budget.total_bytes if execution.output_budget else None,
            'boundary_reason': ('output_soft_limit' if boundary.get('output_bytes', 0) >= CHUNK_SOFT_BYTES
                                else 'command_prefix') if boundary else None})

    def write(self, output):
        if not self.enabled:
            return
        try:
            write_json(Path(output) / 'timing.json', {'format': 'tor-earned-route-timing-v1',
                'timing_semantics': 'Inclusive nested spans overlap; do not sum. Observational only; not qualification evidence.',
                'observer_elapsed_ns': time.perf_counter_ns() - self.started,
                'stats': self.stats, 'segments': self.segments, 'omitted_segments': self.omitted_segments,
                'write_failures': self.write_failures, 'last_write_error': self.last_write_error})
        except Exception as error:
            # Optional diagnostics must not replace the original route exception.
            self.write_failures += 1
            self.last_write_error = f'{type(error).__name__}: {error}'[:500]
            try:
                print(f'Earned-route timing sidecar unavailable: {self.last_write_error}', file=sys.stderr)
            except (OSError, ValueError):
                pass  # A later successful sidecar still reports the failure count.


def _timed_route(method):
    @wraps(method)
    def measured(self, *args, **kwargs):
        metrics = getattr(self, 'measurements', None)
        if metrics is None or not metrics.enabled:
            return method(self, *args, **kwargs)
        if method.__name__ in ('execute', 'resume'):
            self._measurement_admitted = False
        try:
            with metrics.span(method.__name__):
                return method(self, *args, **kwargs)
        finally:
            if method.__name__ == 'finalize_segment':
                metrics.segment(self)
            if method.__name__ in ('execute', 'resume', 'compress_logs') and self._measurement_admitted:
                metrics.write(self.output)
    return measured


class ArchiveVerificationCache:
    """Attempt-local, bounded verified content relationships; never persisted."""
    def __init__(self, *, max_entries=256, measurements=None):
        if type(max_entries) is not int or not 1 <= max_entries <= 256:
            raise ValueError('Archive cache capacity must be between 1 and 256')
        self.max_entries = max_entries
        self._verified = OrderedDict()
        self.measurements = measurements


class _HashingReader:
    """Hash exactly the compressed stream decoded on a cache miss."""
    def __init__(self, stream):
        self.stream, self.hash = stream, hashlib.sha256()
        self.size = 0

    def read(self, size=-1):
        block = self.stream.read(size)
        self.hash.update(block)
        self.size += len(block)
        return block


def _hash_compressed(stream):
    reader = _HashingReader(stream)
    while reader.read(1024 * 1024):
        pass
    return reader.hash.hexdigest(), reader.size


def _verify_compressed(output, row, *, cache=None):
    path = _retained_path(output, row['retained'])
    metrics = cache.measurements if cache is not None else None
    if metrics is not None and not metrics.enabled:
        metrics = None
    # Keep one owned handle through hashing, decoding and accepting the binding.
    with path.open('rb') as compressed:
        with metrics.span('archive_hash') if metrics else nullcontext():
            compressed_hash, size = _hash_compressed(compressed)
            if compressed_hash != row['compressed_sha256']:
                raise EvidenceError('Retained compressed log changed')
            if metrics:
                metrics.add('archive_hash', size=size)
        key = (compressed_hash, row['uncompressed_sha256'], row['uncompressed_bytes'])
        if cache is not None and key in cache._verified:
            cache._verified.move_to_end(key)
            if metrics:
                metrics.add('archive_cache_hit', calls=1)
            return
        if metrics:
            metrics.add('archive_cache_miss', calls=1)
        verified, length = hashlib.sha256(), 0
        with metrics.span('archive_decode') if metrics else nullcontext():
            compressed.seek(0)
            reader = _HashingReader(compressed)
            with gzip.open(reader, 'rb') as source:
                while block := source.read(1024 * 1024):
                    verified.update(block); length += len(block)
                    if metrics:
                        metrics.add('archive_decode', size=len(block))
            if reader.hash.hexdigest() != compressed_hash:
                raise EvidenceError('Retained compressed bytes changed during verification')
        if verified.hexdigest() != row['uncompressed_sha256'] or length != row['uncompressed_bytes']:
            raise EvidenceError('Compressed raw log did not preserve exact bytes')
        if cache is not None:
            cache._verified[key] = None
            if len(cache._verified) > cache.max_entries:
                cache._verified.popitem(last=False)
                if metrics:
                    metrics.add('archive_cache_evict', calls=1)


def compress_completed_logs(output, *, cache=None):
    """Merge verified lossless retention only after the owning processes stop."""
    output = Path(output)
    index = output / 'retained-raw-logs.json'
    manifest = json.loads(index.read_text(encoding='utf-8')) if index.exists() else []
    existing = {}
    for row in manifest:
        if row['original'] in existing:
            raise EvidenceError('Duplicate retained log identity')
        _verify_compressed(output, row, cache=cache)
        existing[row['original']] = row
    for path in sorted(output.glob('process-*/*.log')):
        relative = str(path.relative_to(output))
        before, size = digest(path), path.stat().st_size
        if relative in existing:
            row = existing[relative]
            if before != row['uncompressed_sha256'] or size != row['uncompressed_bytes']:
                raise EvidenceError('Previously retained raw log changed')
            path.unlink()
            continue
        destination = path.with_suffix(path.suffix + '.gz')
        if destination.exists():
            raise EvidenceError('Unindexed compressed log already exists; raw log retained')
        with path.open('rb') as source, gzip.open(destination, 'wb', compresslevel=1) as target:
            shutil.copyfileobj(source, target, length=1024 * 1024)
        row = {'original': relative, 'retained': str(destination.relative_to(output)),
               'uncompressed_sha256': before, 'uncompressed_bytes': size,
               'compressed_sha256': digest(destination)}
        _verify_compressed(output, row, cache=cache)
        manifest.append(row)
        # Publish retention before reclaiming raw bytes. A retry can verify both.
        write_json(index, manifest)
        path.unlink()
    return manifest


def validate_resume_binding(retained, expected, package):
    """Bind a new executor to retained identity and bytes before any server launch."""
    from rogue_earned_route import Controller
    for field in ('actor', 'seed', 'arm', 'policy_revision', 'first_command_limit',
                  'wall_limit_seconds', 'resume_limit', 'minimum_free_bytes',
                  'require_diagnostics', 'package_path', 'chunk_soft_bytes', 'chunk_hard_bytes'):
        if (field not in retained or field not in expected
                or type(retained[field]) is not type(expected[field])
                or retained[field] != expected[field]):
            raise EvidenceError('Resume binding changed: ' + field)
    controller = Controller.load(retained['controller'])
    if (controller.character != retained['actor']
            or controller.first_limit != retained['first_command_limit']
            or controller.floor_limit != 2500 or controller.run_limit != 65000):
        raise EvidenceError('Retained controller identity or command bounds changed')
    if retained['frozen_inputs'] != expected['frozen_inputs']:
        raise EvidenceError('Retained source/executable fingerprints changed')
    verify_fingerprint(retained['frozen_inputs'])
    package = Path(package)
    if package.exists():
        if files(package) != retained['package_inputs']:
            raise EvidenceError('Retained package fingerprint changed')
    elif not retained.get('generated_source_deleted'):
        raise EvidenceError('Retained package unexpectedly disappeared')
    return controller


def validate_saved_authority(retained, private):
    """Accepted history is exact; normalization is only for newly issued execution."""
    for saved, actual in (('actor', 'selected'), ('authoritative_anchor', 'anchor'),
            ('authoritative_records', 'records'), ('receipts', 'receipts'),
            ('final_creature', 'creature')):
        if (saved not in retained or actual not in private
                or _json_digest(retained[saved]) != _json_digest(private[actual])):
            raise EvidenceError('Source-free restore changed exact saved authority: ' + saved)


def _json_digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, allow_nan=False,
                                    separators=(',', ':')).encode('utf-8')).hexdigest()


def _report_digest(report):
    return _json_digest({key: value for key, value in report.items() if key != 'retained_segment'})


def _backup_save(source_path, destination_path):
    # Connection.__exit__ handles transactions, but does not release file handles.
    with closing(sqlite3.connect(Path(source_path).resolve().as_uri() + '?mode=ro', uri=True)) as source, \
            closing(sqlite3.connect(destination_path)) as destination, source, destination:
        source.backup(destination)


def retain_segment(output, save, report, *, milestone=None):
    """Compact transport binding; full SQLite/report copies only at logical boundaries."""
    output, save = Path(output), Path(save)
    segments = output / 'segments'
    segments.mkdir(exist_ok=True)
    number = 0
    while (segments / f'{number:04d}').exists():
        number += 1
    destination = segments / f'{number:04d}'
    destination.mkdir()
    full = milestone is not None or report.get('outcome') != 'acceptance_prefix_saved'
    boundary = {'kind': 'milestone' if full else 'transport', 'milestone': milestone,
        'report_sha256': _report_digest(report), 'anchor': report.get('authoritative_anchor'),
        'records_sha256': _json_digest(report.get('authoritative_records')),
        'creature_sha256': _json_digest(report.get('final_creature')),
        'controller': report.get('controller'), 'frozen_inputs': report.get('frozen_inputs'),
        'package_inputs': report.get('package_inputs'),
        'source_free_restart': report.get('source_free_restart'), 'traces': {}, 'process_logs': {}}
    for name in ('commands.jsonl', 'survival.jsonl'):
        path = output / name
        if path.exists():
            boundary['traces'][name] = {'bytes': path.stat().st_size, 'sha256': digest(path)}
    for path in sorted(output.glob('process-*/*.log')):
        boundary['process_logs'][str(path.relative_to(output))] = {
            'bytes': path.stat().st_size, 'sha256': digest(path)}
    retained = {'index': number, 'kind': boundary['kind']}
    if full:
        snapshot = destination / 'game.db'
        _backup_save(save, snapshot)
        record = destination / 'attempt.json'
        write_json(record, report)
        retained.update(report=str(record.relative_to(output)), report_sha256=digest(record),
                        save=str(snapshot.relative_to(output)), save_sha256=digest(snapshot))
    record = destination / 'boundary.json'
    write_json(record, boundary)
    retained.update(boundary=str(record.relative_to(output)), boundary_sha256=digest(record))
    return retained


def verify_retained_segment(output, report, *, cache=None):
    retained = report['retained_segment']
    for field in ('boundary', *(('report', 'save') if retained['kind'] == 'milestone' else ())):
        if digest(_retained_path(output, retained[field])) != retained[field + '_sha256']:
            raise EvidenceError('Immutable segment artifact changed: ' + field)
    boundary = json.loads(_retained_path(output, retained['boundary']).read_text(encoding='utf-8'))
    if boundary['report_sha256'] != _report_digest(report):
        raise EvidenceError('Attempt differs from immutable retained segment')
    for name, expected in boundary['traces'].items():
        path = _retained_path(output, name)
        if path.stat().st_size != expected['bytes'] or digest(path) != expected['sha256']:
            raise EvidenceError('Retained append-only trace boundary changed: ' + name)
    index = Path(output) / 'retained-raw-logs.json'
    compressed = ({row['original']: row for row in json.loads(index.read_text(encoding='utf-8'))}
                  if index.exists() else {})
    for name, expected in boundary['process_logs'].items():
        path = _retained_path(output, name)
        if path.exists():
            if path.stat().st_size != expected['bytes'] or digest(path) != expected['sha256']:
                raise EvidenceError('Retained process trace changed: ' + name)
        else:
            row = compressed.get(name)
            if (row is None or row['uncompressed_bytes'] != expected['bytes']
                    or row['uncompressed_sha256'] != expected['sha256']):
                raise EvidenceError('Missing lossless retained process trace: ' + name)
            _verify_compressed(output, row, cache=cache)


COMPARISON_SURFACES = {
    'commands': 'All mutating owner confirmations, actions and resumes, in issue order; extra read-only restart checks are separately audited.',
    'state': 'Complete final ordinary payload, with saved privacy handles unchanged; only attachment-issued snapshot and display generation are excluded.',
    'history': 'Complete final public history page as actually returned.',
    'receipts': 'Full chronological activation receipt audit, without deduplication.',
    'controller': 'Full saved controller state, graph, growth, rest and spent budgets.',
    'flows': 'Exact per-actor gross totals only; complete raw sidecar and anchored reader envelope retained separately.',
    'authoritative_records': 'Every durable journal/history record, ordered by exact sequence.',
    'authoritative_anchor': 'Independent final save anchor; only its typed entry UUID may map.',
    'final_creature': 'Complete selected-actor authoritative creature, including nutrition/recovery fractions, build and earned progression.',
}


def comparison_surface(report):
    if report.get('ordinary_evidence_status') != 'complete':
        raise EvidenceError('Cannot compare incomplete ordinary route evidence')
    flows = report.get('flows')
    if not isinstance(flows, dict) or 'actors' not in flows:
        raise EvidenceError('Cannot compare gross flows without the commit-only validated collector')
    result = {field: copy.deepcopy(report[field]) for field in COMPARISON_SURFACES
              if field not in ('commands', 'flows')}
    result['state'] = _disclosed_state_payload(result['state'])
    result['commands'] = [copy.deepcopy(row) for row in report['commands'] if row['kind'] != 'read']
    result['flows'] = copy.deepcopy(flows['actors'])
    result['initial_history_sequence'] = 0
    return result


def compare_saved_initial_execution(original, repeated):
    """Compare reviewed surfaces, with complete raw evidence retained alongside."""
    from rogue_route_replay import compare_execution
    if repeated.get('replayed_initial_save_sha256') != original['initial_boundary']['sha256']:
        raise EvidenceError('New execution did not use the exact retained initial save')
    result = compare_execution(comparison_surface(original), comparison_surface(repeated))
    result['surface_selection'] = copy.deepcopy(COMPARISON_SURFACES)
    result['source_free_restart'] = copy.deepcopy(repeated.get('source_free_restart'))
    result['same_initial_save_sha256'] = original['initial_boundary']['sha256']
    return result


class RouteExecution:
    """One retained attempt through the shared actual-process harness."""
    def __init__(self, harness, output, *, actor, seed, package, arm='production',
                 first_limit=2500, wall_seconds=900, require_diagnostics=True):
        from rogue_earned_route import Controller
        self.harness, self.output = harness, Path(output)
        self.actor, self.seed, self.package, self.arm = actor, seed, Path(package), arm
        self.controller = Controller(actor, first_limit=first_limit)
        self.wall_seconds, self.require_diagnostics = wall_seconds, require_diagnostics
        self.current = self.overview = self.player = self.server = None
        self.commands, self.acquisitions = [], []
        self.counts, self.phase_ticks = Counter(), Counter()
        self.started = None
        self.logical_elapsed = 0.0
        self.output_budget = None
        self.chunk_process_index = 0
        self.phase = 'first_purchase'
        self.measurements = RouteMeasurements(enabled=os.environ.get('TOR_EARNED_ROUTE_TIMING') == '1')
        self._measurement_admitted = False
        self.archive_cache = ArchiveVerificationCache(measurements=self.measurements)
        with self.measurements.span('input_fingerprint'):
            self.inputs = frozen_inputs(harness.bin)
        self.package_inputs = files(self.package) if self.package.exists() else None
        self.report = {'format': FORMAT, 'arm': arm, 'seed': seed, 'actor': actor,
            'sample_role': 'acceptance_fixture' if seed == 42 else 'original_cohort',
            'evidence_status': 'unrun', 'outcome': None, 'frozen_inputs': self.inputs,
            'package_inputs': self.package_inputs, 'wall_limit_seconds': wall_seconds,
            'first_command_limit': first_limit, 'resume_limit': 16,
            'minimum_free_bytes': MINIMUM_FREE_BYTES, 'policy_revision': POLICY_REVISION,
            'require_diagnostics': require_diagnostics, 'package_path': str(self.package.resolve()),
            'logical_elapsed_seconds': 0.0,
            'chunk_soft_bytes': CHUNK_SOFT_BYTES, 'chunk_hard_bytes': CHUNK_HARD_BYTES,
            'checkpoints': {str(depth): None for depth in CHECKPOINTS},
            'replay': {'status': 'unqualified', 'reason': REPLAY_LIMITATION},
            'flows': None, 'stock_transfer_flows': {'status': 'unavailable',
                'reason': 'Adapter does not expose arbitrary acquisition/drop/transfer flows.'}}

    @_timed_route
    def compress_logs(self):
        return compress_completed_logs(self.output, cache=self.archive_cache)

    @_timed_route
    def persist(self):
        self.report.update(controller=self.controller.dump(), commands=self.commands,
                           acquisitions=self.acquisitions, phase_commands=dict(self.counts),
                           phase_elapsed_ticks=dict(self.phase_ticks))
        if self.current is not None:
            self.report.update(state=self.current['state'], history=self.current['history'], overview=self.overview)
        if self.started is not None:
            self.report['wall_seconds'] = time.monotonic() - self.started
            self.report['logical_elapsed_seconds'] = self.logical_elapsed + self.report['wall_seconds']
        write_json(self.output / 'attempt.json', self.report, compact=True)

    def start_transport(self):
        self.chunk_process_index = len(self.harness.processes)
        paths = [self.output / f'process-{self.chunk_process_index + offset:02d}-{name}' / log
                 for offset, name in enumerate(('tor-server', 'tor-client-headless'))
                 for log in ('stdout.log', 'stderr.log')]
        paths.append(self.output / f'startup-{self.chunk_process_index:02d}.jsonl')
        self.output_budget = ProcessOutputBudget(paths, limit_bytes=CHUNK_HARD_BYTES)

    def check_time(self):
        if self.logical_elapsed + time.monotonic() - self.started >= self.wall_seconds:
            raise TimeoutError('Route segment wall deadline')
        if self.output_budget is not None:
            self.output_budget.check()
        if shutil.disk_usage(self.output).free < MINIMUM_FREE_BYTES:
            raise OSError('Route infrastructure storage floor reached; partial trace retained')

    def frame(self, player, predicate, seconds=15):
        self.check_time()
        remaining = self.wall_seconds - self.logical_elapsed - (time.monotonic() - self.started)
        def checked(frame):
            self.check_time()
            return predicate(frame)
        frame = self.harness.frame(player, checked, seconds=min(seconds, remaining))
        self.current = frame
        self.check_time()
        return frame

    @_timed_route
    def start_server(self, *args, **kwargs):
        return self.harness.server(*args, **kwargs)

    @_timed_route
    def connect_player(self):
        # Same ordinary arguments as ProcessTestCase.client, with per-frame
        # output/deadline checks already active during the initial disclosure.
        self.player = self.harness.launch('tor-client-headless', [
            '--connect', self.harness.address, '--actor', str(self.actor),
            '--omit-memory', '--compact-background'])
        self.current = self.frame(self.player, lambda frame: frame['type'] == 'ready')

    def issue(self, value, *, kind='read'):
        self.check_time()
        if kind != 'read':
            if self.controller._at_budget():
                self.report['unsubmitted_budget_command'] = copy.deepcopy(value)
                raise CommandBudgetCensor('Deterministic command budget reached before submission')
            self.controller.consume_command(kind=kind, action=value.get('action'))
            self.counts[self.phase] += 1
        self.commands.append({'kind': kind, 'phase': self.phase, 'command': copy.deepcopy(value)})
        with (self.output / 'commands.jsonl').open('a', encoding='utf-8') as log:
            log.write(json.dumps(self.commands[-1], sort_keys=True) + '\n')
        self.player.write(json.dumps(value))
        response = self.frame(self.player, lambda frame: frame['type'] == 'ready')
        self.current = response
        self.check_time()
        if response.get('error'):
            raise ValueError(f'Ordinary command refused: {value}: {response["error"]}')
        return response

    @_timed_route
    def owner_overview(self):
        frame = self.issue({'type': 'advancement', 'command': 'overview'})
        self.overview = frame['advancement_page']['entries'][0]['value']
        return self.overview

    def settle(self, current):
        execution = self
        class Settlement:
            def __getattr__(self, name):
                return getattr(execution.harness, name)
            def command(self, player, value):
                if value != {'type': 'resume_intention'}:
                    raise EvidenceError('Unexpected settlement command')
                return execution.issue(value, kind='resume')
            def frame(self, player, predicate, seconds=15):
                return execution.frame(player, predicate, seconds=seconds)
        self.current = settle_rogue_action(Settlement(), self.player, current, survive=False)
        # Readiness acknowledges causal completion, not a refreshed picture.
        # This controller immediately accounts for acquisitions and asks for a
        # picture-bound advancement page. Demand that picture explicitly; the
        # client coalesces this with any already pending Observe. Do not change
        # general headless admission timing or silently retry a refused query.
        return self.issue({'type': 'request', 'request': {'type': 'observe'}})

    def assert_start(self):
        view = self.current['state']['observation']
        stats = view['combat']['own_stats']
        source = 'warrior' if self.actor == 1 else 'mage'
        try:
            starting_body_hd(stats['hit_dice'], self.actor)
        except ControllerError as error:
            raise EvidenceError('Unsupported ordinary PC starting composition') from error
        if (self.current['state']['wizard_game'] or view['combat']['terminal']
                or self.overview['points'] != {'earned': "1", 'spent': "0", 'unspent': "1"}
                or self.overview['growth']['future'] != {'kind': 'selected', 'source': {'kind': 'class', 'class': source}}
                or int(self.overview['growth']['earned_xp']) != 0):
            raise EvidenceError('Unexpected authored personal-class-HD-one ordinary starting state')
        if {row['name']: int(row['quantity']) for row in view['inventory']} != {
                'mace': 1, 'ring mail': 1, 'clean bandage': 3, 'food ration': 1}:
            raise EvidenceError('Actual finite starting kit changed')
        if stats['nutrition']['kind'] != 'required' or int(stats['nutrition']['reserve']) != 1300:
            raise EvidenceError('Actual starting nutrition changed')
        names = {row['id']: row['name'] for row in view['inventory']}
        equipped = {names[row['item']]: row['slot']['id'] for row in view['interactions']['inventory']
                    if row['equipped_slot'] is not None}
        if equipped != {'mace': 'weapon', 'ring mail': 'body_armor'}:
            raise EvidenceError('Authored equipped starting kit changed')
        self.report['initial'] = {'state': copy.deepcopy(self.current['state']),
                                  'overview': copy.deepcopy(self.overview)}
        self.report['checkpoints']['1'] = {'tick': view['tick'], 'commands': self.controller.commands,
            'state': copy.deepcopy(self.current['state']), 'overview': copy.deepcopy(self.overview)}

    def purchase(self):
        from rogue_earned_route import first_purchase
        self.owner_overview(); self.assert_start()
        self.controller.observe(self.current['state'], self.overview)
        before = copy.deepcopy(self.current['state'])
        body_hd = starting_body_hd(before['observation']['combat']['own_stats']['hit_dice'], self.actor)
        self.issue({'type': 'advancement', 'command': first_purchase(self.actor, body_hd=body_hd)})
        if self.current['state'] != before:
            raise EvidenceError('First-purchase preview mutated ordinary state')
        if self.controller.first_commands >= self.controller.first_limit:
            return
        confirmed = self.issue({'type': 'advancement', 'command': 'confirm'}, kind='confirmation')
        self.settle(confirmed)
        self.owner_overview()
        if self.overview['points'] != {'earned': "1", 'spent': "1", 'unspent': "0"}:
            raise EvidenceError('First purchase did not spend exactly its existing point')
        identity = 'rattling_blow' if self.actor == 1 else 'spark_lance_cold'
        abilities = {row['id'] for row in self.current['state']['observation']['combat']['own_stats']['abilities']}
        if identity not in abilities or (self.actor == 2 and 'spark_lance_fire' in abilities):
            raise EvidenceError('Requested ordinary first purchase was not applied')

    @_timed_route
    def barrier(self):
        self.issue({'type': 'request', 'request': {'type': 'save'}})
        private = read_authoritative_evidence(self.harness.save, self.actor)
        self.report.update(authoritative_anchor=private['anchor'], receipts=private['receipts'],
                           authoritative_records=private['records'],
                           final_creature=private['creature'])
        return private

    def finish_evidence(self):
        private = self.barrier()
        if self.report['outcome'] == 'earned_hd2':
            self.report['earned_hit_dice_history'] = verify_earned_history(private, self.actor, self.overview)
        with self.measurements.span('input_fingerprint'):
            verify_fingerprint(self.inputs)
        if self.package.exists():
            if files(self.package) != self.package_inputs:
                raise EvidenceError('Frozen package changed during execution')
        elif not self.report.get('generated_source_deleted'):
            raise EvidenceError('Frozen package unexpectedly disappeared')
        # The final anchor is from durable save data, never the diagnostic itself.
        log = self.output / 'survival.jsonl'
        if self.require_diagnostics:
            try:
                from survival_diagnostics import read_log
            except ImportError as error:
                raise EvidenceError('Commit-only diagnostic reader is unavailable') from error
            deadline = time.monotonic() + 5
            while True:
                try:
                    self.report['flows'] = read_log(log, expected_final_anchor=private['anchor'])
                    with log.open(encoding='utf-8') as stream:
                        begin = json.loads(stream.readline())
                    validate_initial_anchor(begin, self.report['initial_boundary']['anchor'], self.actor)
                    break
                except (ValueError, OSError) as error:
                    if time.monotonic() >= deadline:
                        raise EvidenceError(f'Final committed-flow audit failed: {error}') from error
                    time.sleep(0.01)
        else:
            self.report['flows'] = {'status': 'unavailable', 'reason': 'Commit-only diagnostics not integrated for this acceptance attempt.'}
        self.check_time()
        self.report['ordinary_evidence_status'] = 'complete'
        # The separately recorded source-free/new-execution gates remain open.
        self.report['evidence_status'] = 'incomplete'

    def run_steps(self, *, prefix_commands=None):
        while True:
            self.check_time()
            self.controller.observe(self.current['state'], self.overview)
            depth = str(self.controller.depth)
            if depth in self.report['checkpoints'] and self.report['checkpoints'][depth] is None:
                self.report['checkpoints'][depth] = {'tick': self.current['state']['observation']['tick'],
                    'commands': self.controller.commands, 'state': copy.deepcopy(self.current['state']),
                    'overview': copy.deepcopy(self.overview)}
                self.barrier()
                self.persist()
                self.report.setdefault('milestone_snapshots', {})[depth] = retain_segment(
                    self.output, self.harness.save, self.report, milestone='depth_' + depth)
            decision = self.controller.choose(self.current['state'])
            if decision.outcome is not None:
                self.report['outcome'] = decision.outcome
                break
            if ((prefix_commands is not None and self.controller.commands >= prefix_commands)
                    or (self.output_budget is not None and self.output_budget.used_bytes() >= CHUNK_SOFT_BYTES)):
                self.report['outcome'] = 'acceptance_prefix_saved'
                self.report['transport_boundary'] = {
                    'commands': self.controller.commands,
                    'output_bytes': self.output_budget.used_bytes() if self.output_budget is not None else 0}
                break
            self.phase = decision.phase
            before = copy.deepcopy(self.current['state']['observation'])
            began = int(before['tick'])
            action = decision.action
            if action.get('type') == 'use_ability' and 'supplies' not in action:
                action = {**action, 'supplies': {'kind': 'carried'}}
            current = self.issue({'type': 'act', 'action': action}, kind='action')
            self.settle(current)
            self.phase_ticks[self.phase] += int(self.current['state']['observation']['tick']) - began
            acquired = record_acquisition(before, decision.action, self.current['state']['observation'])
            if acquired is not None:
                self.acquisitions.append(acquired)
            self.owner_overview()
            self.persist()

    def run_loop(self, *, prefix_commands=None):
        try:
            self.run_steps(prefix_commands=prefix_commands)
        except CommandBudgetCensor:
            # A paid intention can pause at the last permitted command. No
            # further resume is submitted, but its real held/paid tail is saved.
            self.report['outcome'] = 'censored_action_budget'
        self.finish_evidence()

    @_timed_route
    def stop_processes(self):
        # Freeze authority before disconnecting the controller can suspend work.
        for process in self.harness.processes[self.chunk_process_index:]:
            process.stop(save=False)
        self.player = self.server = None

    def retain_initial_boundary(self):
        self.issue({'type': 'request', 'request': {'type': 'save'}})
        initial = self.output / 'initial.db'
        _backup_save(self.harness.save, initial)
        independent = read_authoritative_evidence(initial, self.actor)
        anchor = independent['anchor']
        if (anchor['history_sequence'] != 0 or anchor['entry_id'] is not None
                or int(self.current['state']['observation']['tick']) != anchor['tick']
                or self.current['branch'] != anchor['branch'] or independent['selected'] != self.actor):
            raise EvidenceError('Initial ordinary save does not establish the exact independent start anchor')
        self.report['initial_boundary'] = {'file': initial.name, 'sha256': digest(initial), 'anchor': anchor,
            'state': copy.deepcopy(self.current['state']), 'history': copy.deepcopy(self.current['history'])}

    @_timed_route
    def execute(self, *, prefix_commands=None, initial_save=None):
        self.output.mkdir(parents=True, exist_ok=False)
        self._measurement_admitted = True
        self.started = time.monotonic()
        self.logical_elapsed = 0.0
        self.report['evidence_status'] = 'running'
        self.persist()
        self.harness.directory, self.harness.save = self.output, self.output / 'game.db'
        self.start_transport()
        try:
            with self.measurements.span('input_fingerprint'):
                verify_fingerprint(self.inputs)
            environment = {'TOR_SURVIVAL_DIAGNOSTICS_LOG': str(self.output / 'survival.jsonl')}
            if initial_save is None:
                self.server = self.start_server('--checkpoint-interval', 1, scenario=self.package,
                    character=self.actor, seed=self.seed, save_actor=self.actor, extra_env=environment)
            else:
                _backup_save(initial_save, self.harness.save)
                self.report['sample_role'] = 'saved_initial_execution_comparison'
                self.report['replayed_initial_save_sha256'] = digest(Path(initial_save))
                self.server = self.start_server('--checkpoint-interval', 1, '--regions', 2,
                    seed=None, save_actor=self.actor, extra_env=environment)
            self.connect_player()
            self.retain_initial_boundary()
            self.purchase()
            self.run_loop(prefix_commands=prefix_commands)
        except Exception as error:
            self.report.update(evidence_status='failed', ordinary_evidence_status='incomplete',
                failure_classification=classify_failure(error), failure={'type': type(error).__name__, 'message': str(error)[-4000:]})
            if self.report['outcome'] is None:
                self.report['outcome'] = classify_failure(error)
        finally:
            self.finalize_segment()
        return self.report

    @_timed_route
    def finalize_segment(self):
        self.stop_processes()
        try:
            if self.report.get('ordinary_evidence_status') == 'complete':
                self.check_time()
                validate_saved_authority(self.report, read_authoritative_evidence(self.harness.save, self.actor))
            self.persist()
            self.report['last_stopped_unix'] = time.time()
            if self.harness.save.exists():
                self.report['retained_segment'] = retain_segment(self.output, self.harness.save, self.report)
                write_json(self.output / 'attempt.json', self.report, compact=True)
        except Exception as error:
            self.report.update(evidence_status='failed', ordinary_evidence_status='incomplete',
                failure_classification=classify_failure(error),
                failure={'type': type(error).__name__, 'message': str(error)[-4000:]})
            if self.report.get('outcome') is None:
                self.report['outcome'] = classify_failure(error)
            # Preserve the existing report/raw tail even if the complete artifact
            # exceeds the fixed cap. This small independent failure is not success.
            write_json(self.output / 'retention-failure.json', {
                'outcome': self.report['outcome'], 'evidence_status': 'failed',
                'ordinary_evidence_status': 'incomplete',
                'failure_classification': classify_failure(error), 'failure': self.report['failure']})

    @_timed_route
    def resume(self, *, continuation=False, prefix_commands=None, delete_source=False):
        """Resume one immutable prefix, retaining the logical segment's wall budget."""
        retained = json.loads((self.output / 'attempt.json').read_text(encoding='utf-8'))
        if (self.output / 'retention-failure.json').exists():
            raise EvidenceError('Failed retention cannot be resumed')
        verify_retained_segment(self.output, retained, cache=self.archive_cache)
        with self.measurements.span('input_fingerprint'):
            controller = validate_resume_binding(retained, self.report, self.package)
        if retained['evidence_status'] == 'failed' or retained.get('ordinary_evidence_status') != 'complete':
            raise EvidenceError('A failed attempt is retained, not silently resumed')
        if continuation and retained['outcome'] != 'earned_hd2':
            raise EvidenceError('Continuation requires a saved actual earned-HD2 milestone')
        if not continuation and retained['outcome'] not in ('acceptance_prefix_saved', 'earned_hd2',
                'death', 'checkpoint_depth_27', 'censored_action_budget',
                'censored_route_exhausted', 'censored_no_disclosed_combat_route'):
            raise EvidenceError('Unexpected retained route outcome')
        gap = time.time() - retained['last_stopped_unix']
        if gap < 0:
            raise EvidenceError('Wall clock moved backwards across saved-prefix transport')
        self._measurement_admitted = True
        self.logical_elapsed = 0.0 if continuation else retained['logical_elapsed_seconds'] + gap
        self.controller, self.report = controller, retained
        self.inputs, self.package_inputs = retained['frozen_inputs'], retained['package_inputs']
        self.commands, self.acquisitions = retained['commands'], retained['acquisitions']
        self.counts = Counter(retained['phase_commands'])
        self.phase_ticks = Counter(retained['phase_elapsed_ticks'])
        expected = copy.deepcopy(retained)
        self.report.update(evidence_status='running', outcome=None)
        self.started = time.monotonic()
        self.start_transport()
        try:
            self.check_time()
            self.harness.directory, self.harness.save = self.output, self.output / 'game.db'
            validate_saved_authority(expected, read_authoritative_evidence(self.harness.save, self.actor))
            if delete_source:
                if not self.package.is_relative_to(self.output.parent):
                    raise EvidenceError('Source deletion must target this acceptance output only')
                shutil.rmtree(self.package)
                self.report['generated_source_deleted'] = True
            self.server = self.start_server('--checkpoint-interval', 1, '--regions', 2,
                seed=None, save_actor=self.actor,
                extra_env={'TOR_SURVIVAL_DIAGNOSTICS_LOG': str(self.output / 'survival.jsonl')})
            self.connect_player()
            validate_restored_picture(self.current, expected)
            private = self.barrier()
            validate_saved_authority(expected, private)
            self.report['source_free_restart'] = {'status': 'passed',
                'exact_state_history_receipts': True, 'exact_anchor_records_creature': True}
            self.owner_overview()
            if continuation:
                self.controller.begin_continuation()
            self.run_loop(prefix_commands=prefix_commands)
        except Exception as error:
            self.report.update(evidence_status='failed', ordinary_evidence_status='incomplete',
                failure_classification=classify_failure(error), failure={'type': type(error).__name__, 'message': str(error)[-4000:]})
            if self.report['outcome'] is None:
                self.report['outcome'] = classify_failure(error)
        finally:
            self.finalize_segment()
        return self.report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('--prepare-only', action='store_true', help='Freeze all eight unrun slots; launch no samples')
    parser.add_argument('--acceptance-seed42', action='store_true', help='Separate known-path fixture, never a cohort substitute')
    parser.add_argument('--without-diagnostics', action='store_true', help='Ordinary controller acceptance only; evidence stays incomplete')
    args = parser.parse_args(argv)
    if not args.prepare_only and not args.acceptance_seed42:
        parser.error('Choose --prepare-only or --acceptance-seed42; full pilot qualification is not established')
    try:
        arm_sources = configured_arm_sources()
    except EvidenceError as error:
        parser.error(str(error) + '; set TOR_EARNED_ROUTE_PRODUCTION_SOURCE and TOR_EARNED_ROUTE_CANDIDATE_SOURCE')
    bin_directory = binaries()
    prepare_campaign(args.output.resolve(), bin_directory, arm_sources=arm_sources)
    if args.prepare_only:
        return 0
    harness = ProcessTestCase()
    harness.bin, harness.suffix, harness.processes = bin_directory, SUFFIX, []
    run = RouteExecution(harness, args.output.resolve() / 'acceptance-seed42', actor=1, seed=42,
                         package=args.output.resolve() / 'inputs/production/package',
                         require_diagnostics=not args.without_diagnostics)
    result = run.execute(prefix_commands=CHUNK_COMMANDS)
    while result.get('outcome') == 'acceptance_prefix_saved' and result.get('ordinary_evidence_status') == 'complete' and result.get('evidence_status') != 'failed':
        run.compress_logs()
        result = run.resume(prefix_commands=result['controller']['commands'] + CHUNK_COMMANDS)
    run.compress_logs()
    print(json.dumps({key: result.get(key) for key in ('outcome', 'evidence_status', 'ordinary_evidence_status', 'failure')}))
    harness.doCleanups()
    return 0 if (result.get('ordinary_evidence_status') == 'complete'
                 and result.get('evidence_status') != 'failed'
                 and not (run.output / 'retention-failure.json').exists()) else 2


if __name__ == '__main__':
    raise SystemExit(main())
