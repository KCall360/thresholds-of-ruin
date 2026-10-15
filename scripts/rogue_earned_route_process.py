"""Bounded ordinary earned-route execution and retained evidence.

The controller receives public observations and owner advancement only. Private
save/flow audit runs after decisions, never as navigation or combat input. This
revision does not certify a complete measurement until all replay/flow gates pass.
"""
import argparse
from collections import Counter
import copy
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
from process_harness import ProcessTestCase, ROOT, SUFFIX, binaries, hit_die_counts, settle_rogue_action
from rogue_activation_candidate import create_candidate

FORMAT = 'tor-rogue-earned-route-v1'
MINIMUM_FREE_BYTES = 512 * 1024 * 1024
SOURCE_PACKAGE = ROOT / 'scenarios/rogue-exploration'
CHECKPOINTS = (1, 5, 8, 11, 17, 26)
REPLAY_LIMITATION = ('Exact source-free replay and separately labeled normalized execution from a '
                    'saved initial boundary are required. Independent fresh seed runs are separate acceptance attempts.')


class EvidenceError(ValueError):
    """An attempted run cannot support a complete evidence claim."""


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
    if isinstance(error, (TimeoutError, subprocess.TimeoutExpired, OSError)) or any(
            text in str(error) for text in ('Process output deadline', 'Unexpected process exit')):
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
    with sqlite3.connect(f'file:{Path(save).resolve()}?mode=ro', uri=True) as db:
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
    if (len(dice) != 2 or any(die['source'] != {'kind': 'class', 'id': source} for die in dice)
            or int(progress['earned_xp']) <= 0 or int(progress['spent_xp']) <= 0
            or progress['highest_earned_tier'] < 2 or progress['highest_applied_tier'] < 2):
        raise EvidenceError('Saved two-HD history is not an actual earned second HD')
    for field in ('earned_xp', 'spent_xp', 'highest_earned_tier', 'highest_applied_tier'):
        if int(progress[field]) != int(overview['growth'][field]):
            raise EvidenceError('Owner advancement and saved earned progression disagree')
    return copy.deepcopy(dice)


def frozen_inputs(bin_directory):
    tracked = subprocess.check_output(['git', 'ls-files', '-z'], cwd=ROOT).decode().split('\0')
    paths = [ROOT / relative for relative in tracked if relative]
    paths.extend(ROOT / 'scripts' / name for name in (
        'rogue_earned_route.py', 'rogue_earned_route_process.py'))
    paths.extend(Path(bin_directory) / (name + SUFFIX) for name in ('tor-server', 'tor-client-headless', 'tor-scenario'))
    return fingerprint(paths)


def prepare_campaign(output, bin_directory):
    """Write all eight unrun slots before generation; retain every failed stage."""
    from rogue_earned_route import fixed_cohort
    output.mkdir(parents=True, exist_ok=False)
    inventory = {'format': FORMAT, 'policy_revision': 3, 'slots': fixed_cohort(),
                 'preparation_status': 'running', 'measurement_status': 'unrun',
                 'limitations': [REPLAY_LIMITATION, 'Later earned points remain unspent.',
                                'Candidate U-only depths 18..26 remain an unmeasured hazard.']}
    write_json(output / 'inventory.json', inventory)
    try:
        inventory['frozen_inputs'] = frozen_inputs(bin_directory)
        compiler = Path(bin_directory) / ('tor-scenario' + SUFFIX)
        production = output / 'inputs/production/package'
        shutil.copytree(SOURCE_PACKAGE, production, ignore=shutil.ignore_patterns('.compiled'))
        compiled = subprocess.run([str(compiler), 'validate', str(production)], capture_output=True,
                                  text=True, encoding='utf-8', timeout=60)
        (output / 'production-validation.log').write_text(compiled.stdout + compiled.stderr, encoding='utf-8')
        if compiled.returncode:
            raise EvidenceError('Unchanged production snapshot failed ordinary validation')
        create_candidate(output / 'inputs/candidate', compiler, start_depth=1)
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


def compress_completed_logs(output):
    """Lossless retention with verified uncompressed bytes, after processes stop."""
    output = Path(output)
    manifest = []
    for path in sorted(output.glob('process-*/*.log')):
        destination = path.with_suffix(path.suffix + '.gz')
        before, size = digest(path), path.stat().st_size
        with path.open('rb') as source, gzip.open(destination, 'wb', compresslevel=1) as target:
            shutil.copyfileobj(source, target, length=1024 * 1024)
        verified = hashlib.sha256()
        length = 0
        with gzip.open(destination, 'rb') as source:
            while block := source.read(1024 * 1024):
                verified.update(block); length += len(block)
        if verified.hexdigest() != before or length != size:
            raise EvidenceError('Compressed raw log did not preserve exact bytes')
        manifest.append({'original': str(path.relative_to(output)),
                         'retained': str(destination.relative_to(output)),
                         'uncompressed_sha256': before, 'uncompressed_bytes': size,
                         'compressed_sha256': digest(destination)})
        path.unlink()
    if manifest:
        write_json(output / 'retained-raw-logs.json', manifest)
    return manifest


COMPARISON_SURFACES = {
    'commands': 'All mutating owner confirmations, actions and resumes, in issue order; extra read-only restart checks are separately audited.',
    'state': 'Complete final ordinary state, with saved privacy handles unchanged.',
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
        self.phase = 'first_purchase'
        self.inputs = frozen_inputs(harness.bin)
        self.package_inputs = files(self.package)
        self.report = {'format': FORMAT, 'arm': arm, 'seed': seed, 'actor': actor,
            'sample_role': 'acceptance_fixture' if seed == 42 else 'original_cohort',
            'evidence_status': 'unrun', 'outcome': None, 'frozen_inputs': self.inputs,
            'package_inputs': self.package_inputs, 'wall_limit_seconds': wall_seconds,
            'first_command_limit': first_limit, 'resume_limit': 16,
            'minimum_free_bytes': MINIMUM_FREE_BYTES, 'policy_revision': 3,
            'checkpoints': {str(depth): None for depth in CHECKPOINTS},
            'replay': {'status': 'unqualified', 'reason': REPLAY_LIMITATION},
            'flows': None, 'stock_transfer_flows': {'status': 'unavailable',
                'reason': 'Adapter does not expose arbitrary acquisition/drop/transfer flows.'}}

    def persist(self):
        self.report.update(controller=self.controller.dump(), commands=self.commands,
                           acquisitions=self.acquisitions, phase_commands=dict(self.counts),
                           phase_elapsed_ticks=dict(self.phase_ticks))
        if self.current is not None:
            self.report.update(state=self.current['state'], history=self.current['history'], overview=self.overview)
        if self.started is not None:
            self.report['wall_seconds'] = time.monotonic() - self.started
        write_json(self.output / 'attempt.json', self.report)

    def check_time(self):
        if time.monotonic() - self.started >= self.wall_seconds:
            raise TimeoutError('Route segment wall deadline')
        if shutil.disk_usage(self.output).free < MINIMUM_FREE_BYTES:
            raise OSError('Route infrastructure storage floor reached; partial trace retained')

    def issue(self, value, *, kind='read'):
        self.check_time()
        if kind != 'read':
            self.controller.consume_command(kind=kind, action=value.get('action'))
            self.counts[self.phase] += 1
        self.commands.append({'kind': kind, 'phase': self.phase, 'command': copy.deepcopy(value)})
        with (self.output / 'commands.jsonl').open('a', encoding='utf-8') as log:
            log.write(json.dumps(self.commands[-1], sort_keys=True) + '\n')
        response = self.harness.command(self.player, value)
        self.current = response
        if response.get('error'):
            raise ValueError(f'Ordinary command refused: {value}: {response["error"]}')
        return response

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
                execution.check_time()
                remaining = execution.wall_seconds - (time.monotonic() - execution.started)
                return execution.harness.frame(player, predicate, seconds=min(seconds, remaining))
        self.current = settle_rogue_action(Settlement(), self.player, current, survive=False)
        return self.current

    def assert_start(self):
        view = self.current['state']['observation']
        stats = view['combat']['own_stats']
        source = 'warrior' if self.actor == 1 else 'mage'
        if (self.current['state']['wizard_game'] or view['combat']['terminal']
                or hit_die_counts(stats['hit_dice']) != {('class', source): 1}
                or self.overview['points'] != {'earned': 1, 'spent': 0, 'unspent': 1}
                or self.overview['growth']['future'] != {'kind': 'selected', 'source': {'kind': 'class', 'class': source}}
                or int(self.overview['growth']['earned_xp']) != 0):
            raise EvidenceError('Unexpected authored HD-one ordinary starting state')
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

    def purchase(self):
        from rogue_earned_route import first_purchase
        self.owner_overview(); self.assert_start()
        self.controller.observe(self.current['state'], self.overview)
        before = copy.deepcopy(self.current['state'])
        self.issue({'type': 'advancement', 'command': first_purchase(self.actor)})
        if self.current['state'] != before:
            raise EvidenceError('First-purchase preview mutated ordinary state')
        if self.controller.first_commands >= self.controller.first_limit:
            return
        confirmed = self.issue({'type': 'advancement', 'command': 'confirm'}, kind='confirmation')
        self.settle(confirmed)
        self.owner_overview()
        if self.overview['points'] != {'earned': 1, 'spent': 1, 'unspent': 0}:
            raise EvidenceError('First purchase did not spend exactly its existing point')
        identity = 'rattling_blow' if self.actor == 1 else 'spark_lance_cold'
        abilities = {row['id'] for row in self.current['state']['observation']['combat']['own_stats']['abilities']}
        if identity not in abilities or (self.actor == 2 and 'spark_lance_fire' in abilities):
            raise EvidenceError('Requested ordinary first purchase was not applied')

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
        self.report['ordinary_evidence_status'] = 'complete'
        # The separately recorded source-free/new-execution gates remain open.
        self.report['evidence_status'] = 'incomplete'

    def run_loop(self, *, prefix_commands=None):
        while True:
            self.check_time()
            self.controller.observe(self.current['state'], self.overview)
            depth = str(self.controller.depth)
            if depth in self.report['checkpoints'] and self.report['checkpoints'][depth] is None:
                self.report['checkpoints'][depth] = {'tick': self.current['state']['observation']['tick'],
                    'commands': self.controller.commands, 'state': copy.deepcopy(self.current['state']),
                    'overview': copy.deepcopy(self.overview)}
            decision = self.controller.choose(self.current['state'])
            if decision.outcome is not None:
                self.report['outcome'] = decision.outcome
                break
            if prefix_commands is not None and self.controller.commands >= prefix_commands:
                self.report['outcome'] = 'acceptance_prefix_saved'
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
        self.finish_evidence()

    def stop_processes(self):
        for process in (self.player, self.server):
            if process is not None:
                process.stop(save=False)
        self.player = self.server = None

    def retain_initial_boundary(self):
        self.issue({'type': 'request', 'request': {'type': 'save'}})
        initial = self.output / 'initial.db'
        with sqlite3.connect(self.harness.save) as source, sqlite3.connect(initial) as destination:
            source.backup(destination)
        independent = read_authoritative_evidence(initial, self.actor)
        anchor = independent['anchor']
        if (anchor['history_sequence'] != 0 or anchor['entry_id'] is not None
                or int(self.current['state']['observation']['tick']) != anchor['tick']
                or self.current['branch'] != anchor['branch'] or independent['selected'] != self.actor):
            raise EvidenceError('Initial ordinary save does not establish the exact independent start anchor')
        self.report['initial_boundary'] = {'file': initial.name, 'sha256': digest(initial), 'anchor': anchor,
            'state': copy.deepcopy(self.current['state']), 'history': copy.deepcopy(self.current['history'])}

    def execute(self, *, prefix_commands=None, initial_save=None):
        self.output.mkdir(parents=True, exist_ok=False)
        self.started = time.monotonic()
        self.report['evidence_status'] = 'running'
        self.persist()
        self.harness.directory, self.harness.save = self.output, self.output / 'game.db'
        try:
            verify_fingerprint(self.inputs)
            environment = {'TOR_SURVIVAL_DIAGNOSTICS_LOG': str(self.output / 'survival.jsonl')}
            if initial_save is None:
                self.server = self.harness.server('--checkpoint-interval', 1, scenario=self.package,
                    character=self.actor, seed=self.seed, save_actor=self.actor, extra_env=environment)
            else:
                with sqlite3.connect(initial_save) as source, sqlite3.connect(self.harness.save) as destination:
                    source.backup(destination)
                self.report['sample_role'] = 'saved_initial_execution_comparison'
                self.report['replayed_initial_save_sha256'] = digest(Path(initial_save))
                self.server = self.harness.server('--checkpoint-interval', 1, '--regions', 2,
                    seed=None, save_actor=self.actor, extra_env=environment)
            self.player, self.current = self.harness.client(actor=self.actor, omit_memory=True)
            self.retain_initial_boundary()
            self.purchase()
            self.run_loop(prefix_commands=prefix_commands)
        except Exception as error:
            self.report.update(evidence_status='failed', failure={'type': type(error).__name__, 'message': str(error)[-4000:]})
            if self.report['outcome'] is None:
                self.report['outcome'] = classify_failure(error)
        finally:
            self.persist()
            self.stop_processes()
        return self.report

    def resume(self, *, continuation=False, prefix_commands=None, delete_source=False):
        """Resume the same saved actor/memory; ordinary state/history stay exact."""
        from rogue_earned_route import Controller
        retained = json.loads((self.output / 'attempt.json').read_text(encoding='utf-8'))
        if retained['evidence_status'] == 'failed':
            raise EvidenceError('A failed attempt is retained, not silently resumed')
        if continuation and retained['outcome'] != 'earned_hd2':
            raise EvidenceError('Continuation requires a saved actual earned-HD2 milestone')
        self.controller = Controller.load(retained['controller'])
        self.commands = retained['commands']
        self.acquisitions = retained['acquisitions']
        self.counts = Counter(retained['phase_commands'])
        self.phase_ticks = Counter(retained['phase_elapsed_ticks'])
        self.report = retained
        self.report.setdefault('segments', []).append({
            'outcome': retained['outcome'], 'commands': self.controller.commands,
            'wall_seconds': retained.get('wall_seconds'),
            'anchor': copy.deepcopy(retained.get('authoritative_anchor'))})
        expected_state, expected_history = retained['state'], retained['history']
        expected_receipts = retained['receipts']
        self.report.update(evidence_status='running', outcome=None)
        self.started = time.monotonic()
        if delete_source:
            if not self.package.is_relative_to(self.output.parent):
                raise EvidenceError('Source deletion must target this acceptance output only')
            shutil.rmtree(self.package)
            self.report['generated_source_deleted'] = True
        try:
            verify_fingerprint(self.inputs)
            self.harness.directory, self.harness.save = self.output, self.output / 'game.db'
            self.server = self.harness.server('--checkpoint-interval', 1, '--regions', 2,
                seed=None, save_actor=self.actor,
                extra_env={'TOR_SURVIVAL_DIAGNOSTICS_LOG': str(self.output / 'survival.jsonl')})
            self.player, self.current = self.harness.client(actor=self.actor, omit_memory=True)
            if self.current['state'] != expected_state or self.current['history'] != expected_history:
                raise EvidenceError('Source-free restore changed exact state/history')
            private = self.barrier()
            if private['receipts'] != expected_receipts or private['selected'] != self.actor:
                raise EvidenceError('Source-free restore changed chronological receipts/selected actor')
            self.report['source_free_restart'] = {'status': 'passed', 'exact_state_history_receipts': True}
            self.owner_overview()
            if continuation:
                self.controller.begin_continuation()
            self.run_loop(prefix_commands=prefix_commands)
        except Exception as error:
            self.report.update(evidence_status='failed', failure={'type': type(error).__name__, 'message': str(error)[-4000:]})
            if self.report['outcome'] is None:
                self.report['outcome'] = classify_failure(error)
        finally:
            self.persist()
            self.stop_processes()
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
    bin_directory = binaries()
    prepare_campaign(args.output.resolve(), bin_directory)
    if args.prepare_only:
        return 0
    harness = ProcessTestCase()
    harness.bin, harness.suffix, harness.processes = bin_directory, SUFFIX, []
    run = RouteExecution(harness, args.output.resolve() / 'acceptance-seed42', actor=1, seed=42,
                         package=args.output.resolve() / 'inputs/production/package',
                         require_diagnostics=not args.without_diagnostics)
    result = run.execute()
    print(json.dumps({key: result.get(key) for key in ('outcome', 'evidence_status', 'ordinary_evidence_status', 'failure')}))
    harness.doCleanups()
    return 0 if result.get('ordinary_evidence_status') == 'complete' else 2


if __name__ == '__main__':
    raise SystemExit(main())
