"""Private provisional integration and validation of retirement proposals."""
from argparse import Namespace
from collections import OrderedDict
from concurrent.futures import ThreadPoolExecutor
import copy
from contextlib import ExitStack, closing, contextmanager, nullcontext
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import stat
import subprocess
import sys
import tempfile
import time
import uuid

import dev
import mined
import retire as r
import retire_author as a
import retire_cpu
import retire_library_controls as library_controls
import retire_native_outcome
import retire_parse_screen
import retire_merge as merger
import retire_tasks as tasks
import retire_worker as w
import shapes

REPOSITORY_DRIVER = Path(__file__).with_name('retire_repository_checks.py')

FAMILY_CHECK_THREADS = 6
DEFAULT_NATIVE_CONTROL_THREADS = 6
DEFAULT_PARSE_SCREEN_THREADS = 6
MAX_ROUTED_EXCLUSIONS = 12
RETAINED_NATIVE_PROBE_LIMIT = 16
RETAINED_FAMILY_PROBE_LIMIT = 4
RETAINED_PRINT_TRACE_LIMIT = 5
MIN_EARLY_PRINT_GROUP = 16
CONTROL_INPUTS = ('allowed.json', 'gold.jsonl', 'oracle.json', 'index.json',
                  'bin/score', 'bin/validate-readings')
BASE_CONTROL_ARTIFACTS = {'before.jsonl', 'commands.jsonl', 'gold.jsonl', 'native-failures.jsonl',
                          'native.json', 'oracle.json', 'result.json', 'rows.jsonl', 'score.log',
                          'slice.json', 'validate-readings.log'}


def _git(source, *args):
    return subprocess.check_output(['git', '-c', 'core.hooksPath=/dev/null', *args], cwd=source, text=True).strip()


def _fixture_path(value):
    path = Path(value)
    if path.is_absolute() or '..' in path.parts or len(path.parts) < 2 or path.parts[0] != 'families' or path.suffix != '.json':
        raise ValueError('fixture must be a path under families/')
    return path


def _baseline_fixture_path(value):
    path = Path(value)
    allowed = path.parts[:1] == ('families',) or path.parts[:2] == ('tests', 'fixtures')
    if (path.is_absolute() or '..' in path.parts or str(path) != value
            or not allowed or path.suffix != '.json'):
        raise ValueError('baseline fixture must be under families/ or tests/fixtures/')
    return path


class _ProposalInputCache:
    """Reuse verified frozen inputs and proposal support across ordered trials."""

    def __init__(self, snapshots=None, removals=None, supports=None):
        self.snapshots = {} if snapshots is None else snapshots
        self.removals = {} if removals is None else removals
        self.supports = {} if supports is None else supports
        self.locations = {}
        self.used_supports = set()

    def trial(self):
        return _ProposalInputCache(self.snapshots, self.removals, self.supports)

    @staticmethod
    def identity(root, directory='frozen', manifest='manifest.json'):
        files = []
        tree = root/directory
        if tree.is_symlink() or not tree.is_dir():
            raise ValueError(f'proposal {directory} tree changed')
        for path in sorted(tree.rglob('*')):
            meta = path.stat(follow_symlinks=False)
            if stat.S_ISREG(meta.st_mode):
                files.append((path.relative_to(tree), meta.st_dev, meta.st_ino, meta.st_size,
                              meta.st_ctime_ns, meta.st_mtime_ns))
            elif not stat.S_ISDIR(meta.st_mode):
                raise ValueError(f'proposal {directory} tree contains a nonregular path')
        return r.digest(root/manifest), files

    def _verified(self, root, directory, manifest, snapshots, error, expected=None):
        identity = self.identity(root, directory, manifest)
        if expected is not None and identity[0] != expected:
            raise ValueError(error)
        if root not in snapshots:
            contents = r.read(root/manifest)
            if contents['files'] != r.tree(root/directory) or self.identity(root, directory, manifest) != identity:
                raise ValueError(error)
            snapshots[root] = identity, contents
        previous, contents = snapshots[root]
        if identity != previous:
            raise ValueError(error)
        return contents

    def read(self, directory):
        root = directory.resolve()
        if self.locations.setdefault(directory, root) != root:
            raise ValueError('proposal frozen input location changed')
        return self._verified(root, 'frozen', 'manifest.json', self.snapshots, 'proposal frozen inputs changed')

    def read_support(self, run, expected):
        manifest = self._verified(run, 'support', 'support.json', self.supports, 'proposal support changed', expected)
        self.used_supports.add(run)
        return manifest

    def unchanged(self):
        return (all(directory.resolve() == root for directory, root in self.locations.items())
                and all(self.identity(root) == self.snapshots[root][0] for root in set(self.locations.values()))
                and all(self.identity(run, 'support', 'support.json') == self.supports[run][0]
                        for run in self.used_supports))

    def _removal_key(self, base, rules):
        directory = base.parent
        root = self.locations.get(directory)
        if base.name != 'frozen' or root is None or directory.resolve() != root:
            raise ValueError('canonical removal requires the verified proposal base')
        return root, tuple(sorted(rules))

    def canonical_seeded_removal(self, base, rules):
        root, assignment = self._removal_key(base, rules)
        key = 'seeded', root, assignment
        if key not in self.removals:
            self.removals[key] = merger.canonical_seeded_removal(root/'frozen', rules)
        return self.removals[key]

    def canonical_mechanical_removal(self, base, rules, scratch):
        root, assignment = self._removal_key(base, rules)
        key = 'grammar', root, assignment
        if key not in self.removals:
            self.removals[key] = _canonical_mechanical_tree(root/'frozen/grammar', rules, scratch)
        return self.removals[key]


def _proposal(proposal, *, input_cache=None):
    run = Path(proposal['run']).resolve()
    report = proposal['report']
    if not report.get('passed') or report.get('failures'):
        raise ValueError('coordinator report did not pass')
    task = report['task']
    if Path(task).name != task:
        raise ValueError('invalid task id')
    descriptor = run/'tasks'/f'{task}.json'
    if input_cache is None:
        support = r.read(run/'support.json')
        if r.digest(run/'support.json') != report['support_sha256'] or r.tree(run/'support') != support['files']:
            raise ValueError('proposal support or descriptor changed')
    else:
        support = input_cache.read_support(run, report['support_sha256'])
    if r.digest(descriptor) != report['descriptor_sha256'] or support['tasks'][task] != report['descriptor_sha256']:
        raise ValueError('proposal support or descriptor changed')
    if input_cache is None:
        inputs = r.read(run/'inputs/manifest.json')
        if inputs['files'] != r.tree(run/'inputs/frozen'):
            raise ValueError('proposal frozen inputs changed')
    else:
        inputs = input_cache.read(run/'inputs')
    if report['source'] != inputs['commit']:
        raise ValueError('proposal report base mismatch')
    directory = Path(report['directory']).resolve()
    if r.tree(directory/'grammar') != report['grammar']:
        raise ValueError('evaluated grammar changed')
    if proposal.get('kind', 'author') != 'mechanical' and r.digest(directory/'family.json') != report['fixture_sha256']:
        raise ValueError('evaluated family changed')
    hooks = r.read(directory/'hooks.json')
    if hooks != report['hooks']:
        raise ValueError('evaluated hook requests changed')
    return run, r.read(descriptor), directory, hooks


class _GoldIndexCache:
    """Bounded stage-local index of frozen gold records, keyed by content hash."""
    def __init__(self):
        self._indices = OrderedDict()

    def get(self, run):
        manifest = r.read(run/'inputs/manifest.json')
        expected = manifest.get('files', {}).get('gold.jsonl')
        if not isinstance(expected, str) or not re.fullmatch(r'[0-9a-f]{64}', expected):
            raise ValueError('frozen gold hash is missing or invalid')
        path = run/'inputs/frozen/gold.jsonl'
        if r.digest(path) != expected:
            raise ValueError('frozen gold hash differs')
        if expected in self._indices:
            self._indices.move_to_end(expected)
            return self._indices[expected]
        if len(self._indices) == 2:
            self._indices.popitem(last=False)
        records = r.rows(path)
        index = {}
        for card in records:
            for name in dict.fromkeys(face['name'] for face in card['faces']):
                index.setdefault(name, []).append(card)
        if r.digest(path) != expected:
            raise ValueError('frozen gold changed while indexing')
        self._indices[expected] = index
        return index


def _family(directory, report, run, modes=False, *, gold_cache=None):
    family = r.read(directory/'family.json')
    a.validate_author_family(family)
    original = r.rows(run/'inputs/frozen/gold.jsonl') if gold_cache is None else None
    index = gold_cache.get(run) if gold_cache is not None else None
    for number, edit in enumerate(report['edited']):
        folder = directory/f'edited-{number}'
        gold, rows = r.rows(folder/'gold.jsonl'), r.rows(folder/'rows.jsonl')
        faces = [face for card in gold for face in card['faces'] if face['name'] == edit['face']]
        matches = [row for row in rows if row['name'] == edit['face']]
        if len(faces) != 1 or len(matches) != 1 or matches[0]['status'] != 'equal':
            raise ValueError('edited operand artifacts are incomplete')
        face, row = faces[0], matches[0]
        if row.get('source_context') != edit.get('source_context'):
            raise ValueError('edited source context differs from the checked report')
        originals = (index.get(edit['face'], []) if index is not None else
                     [card for card in original if any(f['name'] == edit['face'] for f in card['faces'])])
        original_record, _ = a.edited_record(originals, 'faces', edit['face'], edit.get('record_faces'))
        expected = copy.deepcopy(original_record)
        expected_face = next(f for f in expected['faces'] if f['name'] == edit['face'])
        for mutation in edit['mutations']:
            a.edit_leaf(expected_face['abilities'], mutation['path'], mutation['value'])
        if gold != [expected]:
            raise ValueError('edited gold differs from the declared operand mutations')
        family['cases'].append(a.edited_family_case(face, row, edit['flags'], allow_seeded=modes is True))
    return family


class _FamilyCheckFailure(ValueError):
    def __init__(self, log, failed_fixtures=()):
        super().__init__(f'family check failed: {log}')
        self.failed_fixtures = failed_fixtures


def _failed_fixtures(fixtures, reports, returncode):
    """Locate failing families only when the helper accounted for every fixture."""
    if returncode != 1 or len(fixtures) != len(reports):
        return ()
    for path, report in zip(fixtures, reports, strict=True):
        family = r.read(path)
        if (report['family'] != family['family'] or report['cases'] != len(family['cases'])
                or not 0 <= report['passed'] <= report['cases']
                or report['cases'] - report['passed'] != len(report['failures'])):
            return ()
    return tuple(index for index, report in enumerate(reports) if report['failures'])


def _family_check_threads(checkpoint):
    threads = checkpoint.get('family_check_threads', FAMILY_CHECK_THREADS)
    capacity = checkpoint.get('cpu_capacity', 32)
    if (type(capacity) is not int or not 1 <= capacity <= 32
            or type(threads) is not int or not 1 <= threads <= min(16, capacity)):
        raise ValueError('family_check_threads must be an integer from 1 to min(16, cpu_capacity)')
    return threads


def _helper(checkpoint, source, fixture, log, grammar_dir=None, *, threads=None):
    if threads is None:
        threads = _family_check_threads(checkpoint)
    fixtures = [fixture] if isinstance(fixture, (str, Path)) else list(fixture)
    if not fixtures:
        raise ValueError('family check requires at least one fixture')
    helpers = Path(checkpoint['helpers'])
    build = r.read(helpers/'build.json')
    if build['source_sha256'] != dev.source(source) or r.digest(helpers/'family-check') != build['binaries']['family-check']:
        raise ValueError('family helper does not match compiled source')
    env = {k: v for k, v in os.environ.items() if not k.startswith('ORACLE_GRAMMAR_')}
    grammar = grammar_dir or source/'grammar'
    env.update(ORACLE_GRAMMAR_DIR=str(grammar), ORACLE_GRAMMAR_THREADS=str(threads))
    declared = {entry.removeprefix('--module=').split('=', 1)[0] for entry in dev.modules(source)}
    current = {path.stem: path for path in (grammar/'syntax').glob('*.ebnf')}
    if (grammar/'tail/mined.ebnf').is_file():
        current['tail'] = grammar/'tail/mined.ebnf'
    overrides = [f'--module={name}={current.get(name, os.devnull)}' for name in sorted(declared | current.keys())]
    lease = (retire_cpu.reserve(Path(checkpoint['cpu_pool']), threads,
                                checkpoint.get('cpu_capacity', 32), background=checkpoint.get('background_cpu', False))
             if checkpoint.get('cpu_pool') else nullcontext(()))
    with lease as fds:
        r._LEASES.extend(fds)
        try:
            with log.open('w') as stream:
                code = subprocess.run([str((helpers/'family-check').resolve()),
                                       *(str(path) for path in fixtures), '--compositional', *overrides],
                                      cwd=source, env=env, stdout=stream, stderr=stream, check=False,
                                      pass_fds=tuple(r._LEASES)).returncode
        finally:
            for fd in fds:
                r._LEASES.remove(fd)
    failed_fixtures = ()
    try:
        reports = _family_reports(log, len(fixtures))
        for index, report in enumerate(reports):
            (log.parent/f'family-{index:03d}.log').write_text(json.dumps(report, sort_keys=True)+'\n')
        failed_fixtures = _failed_fixtures(fixtures, reports, code)
        if any(report.get('failures') != [] or report.get('passed') != report.get('cases')
               for report in reports):
            code = code or 1
    except (IndexError, ValueError, KeyError, TypeError, AttributeError, OSError) as error:
        if not code:
            raise ValueError('invalid family check report') from error
    if code:
        raise _FamilyCheckFailure(log, failed_fixtures)


def _family_reports(log, count):
    data = json.loads(log.read_text().splitlines()[0])
    reports = data['families'] if count > 1 and data.get('schema') == 1 else [data]
    if not isinstance(reports, list) or len(reports) != count:
        raise ValueError('family report count differs')
    for report in reports:
        if (not isinstance(report, dict) or not isinstance(report.get('family'), str)
                or type(report.get('cases')) is not int or type(report.get('passed')) is not int
                or not isinstance(report.get('failures'), list)):
            raise ValueError('invalid family report')
    return reports


def _verified_helper_build(path, build_sha256):
    manifest = path/'build.json'
    if manifest.is_symlink() or not manifest.is_file() or r.digest(manifest) != build_sha256:
        raise ValueError('candidate helper build manifest changed')
    build = r.read(manifest)
    if (build.get('profile') != 'optimized' or not isinstance(build.get('binaries'), dict)
            or set(build['binaries']) != set(dev.BINS)):
        raise ValueError('candidate helper build must be complete and optimized')
    for name in dev.BINS:
        binary = path/name
        if binary.is_symlink() or not binary.is_file() or r.digest(binary) != build['binaries'][name]:
            raise ValueError('candidate helper binary changed: '+name)
    return build


def _candidate_helper_preflight(spec, checkpoint, source, campaign):
    if not isinstance(spec, dict) or set(spec) != {'path', 'build_sha256'}:
        raise ValueError('candidate helper path and build SHA-256 are required')
    if not isinstance(spec['path'], str) or not isinstance(spec['build_sha256'], str) \
            or not re.fullmatch(r'[0-9a-f]{64}', spec['build_sha256']):
        raise ValueError('invalid candidate helper specification')
    root = Path(spec['path'])
    if not root.is_absolute() or '..' in root.parts or not root.is_dir():
        raise ValueError('candidate helper path must be an absolute directory')
    if any(path.is_symlink() for path in (root, *root.parents)):
        raise ValueError('candidate helper path contains a symlink')
    if root.resolve().is_relative_to(campaign.resolve()) or root.resolve().is_relative_to(source.resolve()):
        raise ValueError('candidate helper build must be outside campaign and private workspace')
    build = _verified_helper_build(root, spec['build_sha256'])
    parent = r.read(Path(checkpoint['inputs'])/'frozen/build.json')
    if build.get('source_sha256') != dev.source(source) or build.get('source_sha256') != parent.get('source_sha256'):
        raise ValueError('candidate helper compiled source differs')
    if not isinstance(build.get('compiler'), dict) or build['compiler'] != parent.get('compiler'):
        raise ValueError('candidate helper compiler identity differs')
    return root, build


def _candidate_helper_snapshot(spec, checkpoint, source, folder):
    root, build = _candidate_helper_preflight(spec, checkpoint, source, folder.parents[2])
    snapshot = folder/'candidate-validation-helpers'
    snapshot.mkdir()
    for name in ('build.json', *dev.BINS):
        shutil.copy2(root/name, snapshot/name)
        (snapshot/name).chmod(0o444 if name == 'build.json' else 0o555)
    if _verified_helper_build(snapshot, spec['build_sha256']) != build:
        raise ValueError('candidate helper snapshot differs')
    return snapshot, build


def _optimized_families(source, helpers, folder, threads):
    names = r.read(source/'tests/families.json')
    if not isinstance(names, list) or not 1 <= len(names) <= 4096:
        raise ValueError('optimized family registry is invalid')
    paths = [source/_baseline_fixture_path(name) for name in names]
    env = {k: v for k, v in os.environ.items() if not k.startswith('ORACLE_GRAMMAR_')}
    env.update(ORACLE_GRAMMAR_DIR=str(source/'grammar'), ORACLE_GRAMMAR_THREADS=str(threads))
    log = folder/'optimized-family-check.log'
    with log.open('w') as stream:
        code = subprocess.run([str(helpers/'family-check'), *map(str, paths), '--compositional',
                               *dev.modules(source)], cwd=source, env=env, stdout=stream, stderr=stream,
                              check=False, pass_fds=tuple(r._LEASES)).returncode
    if code:
        raise ValueError('optimized family check failed: '+str(log))
    try:
        reports = _family_reports(log, len(paths))
        if any(item['failures'] or item['passed'] != item['cases'] for item in reports):
            raise ValueError('optimized family report contains failures')
    except (IndexError, KeyError, TypeError, AttributeError) as error:
        raise ValueError('optimized family report is invalid') from error
    return log


def _control_verify(control):
    profile = control['profile']
    if (profile.is_symlink() or not profile.is_file() or profile.stat().st_size > 1024*1024
            or r.digest(profile) != control['profile_sha256']):
        raise ValueError('native control profile changed')
    run = control['run']
    if r.digest(run/'manifest.json') != control['manifest_sha256']:
        raise ValueError('native control checkpoint manifest changed')
    verified = control.setdefault('_verified_inputs', {})
    for name, digest in control['input_sha256'].items():
        path = run/'frozen'/name
        error = 'native control frozen input changed: '+name
        identity = _control_input_identity(path, error)
        receipt = (path, digest, identity)
        if name not in verified:
            if r.digest(path) != digest or _control_input_identity(path, error) != identity:
                raise ValueError(error)
            verified[name] = receipt
        elif verified[name] != receipt:
            raise ValueError(error)


def _control_input_identity(path, error):
    """Detect replacement and writes even when a file's mtime was restored."""
    entry = path.stat(follow_symlinks=False)
    if not stat.S_ISREG(entry.st_mode):
        raise ValueError(error)
    return (entry.st_dev, entry.st_ino, entry.st_size, entry.st_mtime_ns, entry.st_ctime_ns)


def _native_control_threads(checkpoint):
    threads = checkpoint.get('native_control_threads', DEFAULT_NATIVE_CONTROL_THREADS)
    capacity = checkpoint.get('cpu_capacity', 32)
    if (type(capacity) is not int or not 1 <= capacity <= 32
            or type(threads) is not int or not 1 <= threads <= min(16, capacity)):
        raise ValueError('native_control_threads must be an integer from 1 to min(16, cpu_capacity)')
    return threads


def _control_setup(checkpoint):
    spec = checkpoint.get('native_regression_controls')
    if spec is None:
        return None
    threads = _native_control_threads(checkpoint)
    if not checkpoint.get('cpu_pool'):
        raise ValueError('native control requires the shared CPU lease pool')
    if not isinstance(spec, dict) or set(spec) != {'path', 'sha256'}:
        raise ValueError('native control requires profile path and SHA-256')
    if not isinstance(spec['path'], str) or not isinstance(spec['sha256'], str):
        raise ValueError('invalid native control profile specification')
    profile = Path(spec['path'])
    if (not profile.is_absolute() or profile.is_symlink() or not profile.is_file()
            or profile.stat().st_size > 1024*1024):
        raise ValueError('native control profile is missing, unsafe or too large')
    data = profile.read_bytes()
    if hashlib.sha256(data).hexdigest() != spec['sha256']:
        raise ValueError('native control profile SHA-256 differs')
    document = json.loads(data)
    names = document.get('cards') if isinstance(document, dict) and set(document) == {'cards'} else None
    if (not isinstance(names, list) or not 1 <= len(names) <= 500
            or any(not isinstance(name, str) or not name for name in names) or len(set(names)) != len(names)):
        raise ValueError('native control requires 1–500 unique face names')
    run = Path(checkpoint['inputs']).resolve()
    manifest = r.read(run/'manifest.json')
    if manifest.get('format') != 3 or manifest.get('commit') != checkpoint['commit']:
        raise ValueError('native control checkpoint identity differs')
    files = manifest.get('files', {})
    if any(not isinstance(files.get(name), str) for name in CONTROL_INPUTS):
        raise ValueError('native control checkpoint lacks frozen input hashes')
    control = {'profile': profile, 'profile_sha256': spec['sha256'], 'run': run,
               'manifest_sha256': r.digest(run/'manifest.json'), 'threads': threads,
               'input_sha256': {name: files[name] for name in CONTROL_INPUTS}}
    _control_verify(control)
    closure = tasks.close_cohort(run, {'names': names}, max_occurrences=1024)
    allowed = r.read(run/'frozen/allowed.json').get('cards')
    if (not isinstance(allowed, list) or any(not isinstance(name, str) for name in allowed)
            or len(allowed) != len(set(allowed)) or not set(closure['names']) <= set(allowed)):
        raise ValueError('native control cohort escapes allowed population')
    control['names'] = closure['names']
    control['occurrences'] = closure['closure_occurrence_count']
    control['data'] = w.payload(run)
    _control_verify(control)
    return control


class _NativeControlFailure(ValueError):
    def __init__(self, path, report):
        super().__init__('native regression control failed: '+str(path))
        self.report = report


def _control_check(checkpoint, control, grammar, directory):
    threads = _native_control_threads(checkpoint)
    if control['threads'] != threads:
        raise ValueError('native control thread identity changed after setup')
    _control_verify(control)
    directory.mkdir()
    lease = (retire_cpu.reserve(Path(checkpoint['cpu_pool']), threads, checkpoint.get('cpu_capacity', 32),
                                background=checkpoint.get('background_cpu', False))
             if checkpoint.get('cpu_pool') else nullcontext(()))
    with lease as fds:
        r._LEASES.extend(fds)
        try:
            try:
                try:
                    report = w.checks(control['run'], directory, grammar, control['names'], control['data'], threads)
                except subprocess.CalledProcessError as native_error:
                    index, gold, oracle = control['data']
                    selected = set(control['names'])
                    subset = ({key: row for key, row in index.items() if row['name'] in selected}, gold, oracle)
                    report = retire_native_outcome.preservation_failure(native_error, control['run'], directory, subset)
            except (ValueError, OSError, subprocess.CalledProcessError, KeyError) as error:
                r.write(directory/'result.json', {'status': 'error', 'error': str(error)})
                raise
        finally:
            for fd in fds:
                r._LEASES.remove(fd)
    r.write(directory/'result.json', report)
    _control_verify(control)
    if report.get('faces') != control['occurrences'] or report.get('failures') != []:
        raise _NativeControlFailure(directory/'result.json', report)
    return report


def _restore_control_baseline(campaign, index, identity, directory, occurrences):
    try:
        if index.is_symlink() or not index.is_file():
            return False
        proof = r.read(index)
        origin = Path(proof['path'])
        if (proof['identity'] != identity or origin.name != 'native-control-base'
                or origin.parent.parent != campaign/'.cache/batches' or origin.resolve() != origin
                or any(path.is_symlink() for path in origin.rglob('*'))
                or r.read(origin/'result.json') != {'faces': occurrences, 'failures': []}
                or set(proof['files']) != BASE_CONTROL_ARTIFACTS
                or r.tree(origin) != proof['files']):
            return False
        with tempfile.TemporaryDirectory(dir=directory.parent) as temporary:
            restored = Path(temporary)/directory.name
            shutil.copytree(origin, restored)
            if r.tree(restored) != proof['files']:
                return False
            r.write(restored/'reuse.json', {'identity': identity, 'origin': str(origin),
                                          'index_sha256': r.digest(index)})
            os.rename(restored, directory)
        return True
    except (OSError, ValueError, KeyError, TypeError):
        return False


def _accepted_control_rows(campaign, checkpoint, control):
    """Read exact score rows from the immutable, accepted full audit."""
    uri = (campaign/'queue.sqlite').as_uri()+'?mode=ro'
    with closing(sqlite3.connect(uri, uri=True)) as db:
        parent, raw = db.execute('SELECT parent,evidence FROM checkpoints WHERE commit_id=?',
                                 (checkpoint['commit'],)).fetchone()
    ledger = json.loads(raw)
    ident, validation = ledger['manifest'], ledger['validation']
    r.require(re.fullmatch(r'[0-9a-f]{32}', ident) is not None, 'invalid accepted batch identity')
    folder = campaign/'.cache/batches'/ident
    path = folder/'full-checks/rows.jsonl'
    checks = validation['full_checks']
    population = len(control['data'][0])
    r.require(parent and path.resolve() == path and path.is_file() and path.stat().st_size <= 1024**3
              and r.read(folder/'validation.json') == validation
              and validation['status'] == 'passed' and validation['scope'] == 'combined-batch'
              and validation['candidate'] == checkpoint['commit'] and validation['source_checkpoint'] == parent
              and validation['repository_check_returncode'] == 0
              and checks == {'faces': population, 'failures': []}
              and control['run'] == folder/'validation-inputs' == Path(validation['inputs'])
              and validation['validation_manifest_sha256'] == control['manifest_sha256']
              and Path(checkpoint['baseline']) == path == Path(validation['rows']),
              'accepted score rows do not match this checkpoint')
    wanted = set(control['names'])
    digest = hashlib.sha256()
    selected = []
    total = 0
    with path.open('rb') as stream:
        for line in stream:
            digest.update(line)
            row = json.loads(line)
            if row['name'] in wanted:
                selected.append(row)
            total += 1
    sha = digest.hexdigest()
    r.require(total == population and sha == validation['rows_sha256'], 'accepted score rows changed')
    return selected, {'batch': ident, 'rows': str(path), 'sha256': sha, 'population': population}


def _reuse_accepted_control(campaign, checkpoint, control, grammar, directory, grammar_hashes):
    try:
        rows, proof = _accepted_control_rows(campaign, checkpoint, control)
        with tempfile.TemporaryDirectory(dir=directory.parent) as temporary:
            trial = Path(temporary)/directory.name
            trial.mkdir()
            names = w.subset(trial, control['names'], control['data'])
            count = control['occurrences']
            if len(rows) != count or w.compare(rows, control['data'][0], names):
                return None
            r.jsonl(trial/'rows.jsonl', rows)
            threads = control['threads']
            with _audit_cpu(checkpoint, threads, checkpoint.get('cpu_capacity', 32)):
                r.execute(control['run'], trial, grammar, 'validate-readings',
                          [trial/'gold.jsonl', trial/'rows.jsonl', trial/'native.json',
                           trial/'native-failures.jsonl', '--preserve-from', trial/'before.jsonl'], threads)
            if w.native_failures(r.read(trial/'native.json'), count):
                return None
            _control_verify(control)
            if r.tree(grammar) != grammar_hashes:
                return None
            result = {'faces': count, 'failures': []}
            r.write(trial/'result.json', result)
            r.write(trial/'accepted-audit.json', proof)
            os.rename(trial, directory)
            return result
    except (OSError, ValueError, KeyError, TypeError, sqlite3.Error, subprocess.CalledProcessError):
        return None


def _control_baseline(campaign, checkpoint, control, grammar, directory):
    _control_verify(control)
    frozen = r.read(control['run']/'manifest.json')['files']
    grammar_hashes = r.tree(grammar)
    if (grammar_hashes != {name.removeprefix('grammar/'): sha for name, sha in frozen.items()
                           if name.startswith('grammar/')}
            or any(path.is_symlink() for path in grammar.rglob('*'))):
        return _control_check(checkpoint, control, grammar, directory)
    reused = _reuse_accepted_control(campaign, checkpoint, control, grammar, directory, grammar_hashes)
    if reused is not None:
        return reused
    identity = {'grammar': grammar_hashes, 'checkpoint': control['manifest_sha256'],
                'profile': control['profile_sha256'], 'threads': control['threads'],
                'tooling': {name: r.digest(Path(__file__).with_name(name))
                            for name in ('retire.py', 'retire_batch.py', 'retire_worker.py')}}
    key = hashlib.sha256(json.dumps(identity, sort_keys=True).encode()).hexdigest()
    lookup = campaign/'.cache/native-baselines'
    if lookup.is_symlink() or lookup.parent.is_symlink():
        return _control_check(checkpoint, control, grammar, directory)
    index = lookup/(key+'.json')
    if _restore_control_baseline(campaign, index, identity, directory, control['occurrences']):
        _control_verify(control)
        return r.read(directory/'result.json')
    result = _control_check(checkpoint, control, grammar, directory)
    files = r.tree(directory)
    if set(files) == BASE_CONTROL_ARTIFACTS and r.tree(grammar) == grammar_hashes:
        try:
            lookup.mkdir(exist_ok=True)
            r.write(index, {'identity': identity, 'path': str(directory), 'files': files})
        except OSError:
            pass  # The baseline already passed and its complete evidence is in the batch.
    return result


@contextmanager
def _workspace_lock(checkpoint):
    workspace = checkpoint.get('integration_workspace')
    if not workspace:
        yield
        return
    path = Path(workspace).resolve()
    path.parent.mkdir(parents=True, exist_ok=True)
    with (path.parent/(path.name+'.lock')).open('a') as stream:
        try:
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as exc:
            raise ValueError('integration workspace has an active batch or audit') from exc
        r._LEASES.append(stream.fileno())
        try:
            yield
        finally:
            # Closing preserves the lease for any surviving inherited helper.
            r._LEASES.remove(stream.fileno())


def _prepare_workspace(origin, checkpoint, source, branch):
    state = source.parent/(source.name+'.state.json')
    if source.exists():
        if not state.exists():
            raise ValueError('integration workspace is not owned by this campaign')
        previous = r.read(state)
        previous_folder = Path(previous['folder'])
        previous_manifest = r.read(previous_folder/'manifest.json')
        if previous_manifest['status'] == 'provisional':
            validation = previous_folder/'validation.json'
            if not validation.exists() or r.read(validation).get('status') not in ('passed', 'failed'):
                raise ValueError('previous batch has not reached terminal validation')
        elif previous_manifest['status'] not in ('failed', 'no-proposals'):
            raise ValueError('previous batch is not terminal')
        if _git(source, 'status', '--porcelain'):
            raise ValueError('integration workspace is not clean')
        common = lambda root: (root/_git(root, 'rev-parse', '--git-common-dir')).resolve()
        if common(source) != common(origin):
            raise ValueError('integration workspace belongs to a different repository')
        _git(source, 'switch', '-c', branch, checkpoint['commit'])
    else:
        source.parent.mkdir(parents=True, exist_ok=True)
        _git(origin, 'worktree', 'add', '-b', branch, str(source), checkpoint['commit'])
    return state


class _UncertainPromotion(RuntimeError):
    """A failed publication could not restore the original workspace."""


def _promote_trial(source, trial, fixtures):
    """Install a verified union or restore its exact prior grammar and fixtures."""
    installed = []
    backup = trial/'previous-grammar'
    for _, destination in fixtures:
        target = source/destination
        if target.exists() or target.is_symlink() or not target.resolve().is_relative_to(source):
            raise ValueError('fixture appeared or escaped before publication')
        target.parent.mkdir(parents=True, exist_ok=True)
    os.replace(source/'grammar', backup)
    try:
        os.replace(trial/'grammar', source/'grammar')
        for temporary, destination in fixtures:
            os.replace(temporary, source/destination)
            installed.append(source/destination)
    except OSError as error:
        try:
            for path in installed:
                path.unlink()
            if (source/'grammar').exists():
                shutil.rmtree(source/'grammar')
            os.replace(backup, source/'grammar')
        except OSError as rollback:
            raise _UncertainPromotion(f'publication failed: {error}; rollback failed: {rollback}') from rollback
        raise


def _canonical_mechanical_tree(base, rules, scratch):
    before = r.tree(base)
    with tempfile.TemporaryDirectory(prefix='canonical-removal-', dir=scratch) as temporary:
        expected = Path(temporary)
        shutil.copytree(base, expected/'grammar')
        removed, kept = mined.remove(expected, rules)
        if set(removed) != set(rules) or kept:
            raise ValueError('mechanical canonical removal has retained users')
        tree = r.tree(expected/'grammar')
    if r.tree(base) != before:
        raise _UncertainPromotion('mechanical frozen base changed during verification')
    return tree


def _mechanical_removal(run, directory, rules, scratch, *, input_cache=None):
    """Require the complete candidate to equal the canonical assigned removals."""
    if (not isinstance(rules, list) or not rules or len(set(rules)) != len(rules)
            or any(not isinstance(name, str) or not mined.SEEDED.fullmatch(name) for name in rules)):
        raise ValueError('mechanical proposal needs distinct assigned seeded rules')
    base, candidate = run/'inputs/frozen/grammar', directory/'grammar'
    for grammar in (base, candidate):
        if grammar.is_symlink() or not grammar.is_dir():
            raise ValueError('mechanical grammar is unavailable or a symlink')
        if any(path.is_symlink() or not (path.is_file() or path.is_dir()) for path in grammar.rglob('*')):
            raise ValueError('mechanical grammar contains a symlink or nonregular entry')
    expected = (_canonical_mechanical_tree(base, rules, scratch) if input_cache is None
                else input_cache.canonical_mechanical_removal(base.parent, rules, scratch))
    if r.tree(candidate) != expected:
        raise ValueError('mechanical proposal differs from complete canonical removal')


def _declares(root, rule):
    declaration = re.compile(re.escape(rule) + r'\s*::=')
    return any(declaration.match(line) for path in mined.grammar_files(root)
               for _, line, _, _ in mined.code(path.read_text().splitlines()))


def _uses_rule(root, owner, rule):
    """Recognize a grammar reference outside quoted terminals and comments."""
    literals = re.compile(r"""\x22(?:[^\x22\\]|\\.)*\x22|'(?:[^'\\]|\\.)*'|/\*.*?\*/""")
    for path in mined.grammar_files(root):
        for _, line, production, _ in mined.code(path.read_text().splitlines()):
            if production == owner:
                body = line.partition('::=')[2] if mined.DEFINITION.match(line) else line
                if rule in mined.BARE.findall(literals.sub(' ', body)):
                    return True
    return False


def _unknown_rule_diagnostics(log):
    """Accept only a bounded compiler report entirely about unresolved names."""
    try:
        with log.open() as stream:
            error = stream.read(4097)
    except (OSError, UnicodeError):
        return []
    message = re.fullmatch(r'Error: "([^"\r\n]+)"\s*', error) if len(error) <= 4096 else None
    diagnostics = message[1].split(r'\n') if message else []
    if not 1 <= len(diagnostics) <= MAX_ROUTED_EXCLUSIONS:
        return []
    clauses = [re.fullmatch(r'([A-Za-z_]\w*): unknown rule ([A-Za-z_]\w*)', line) for line in diagnostics]
    return [clause.groups() for clause in clauses] if all(clauses) else []


def _reference_owner(source, entries, reference, missing, declares):
    previous = declares(source, missing)
    if not previous and declares(source, reference):
        return None
    owners = []
    for index, proposal in entries:
        if proposal.get('kind') == 'mechanical':
            continue
        base = Path(proposal['run'])/'inputs/frozen'
        current = Path(proposal['report']['directory'])
        if previous:
            responsible = not declares(current, missing) and declares(base, missing)
        else:
            responsible = (declares(base, missing) and declares(current, missing)
                           and not declares(base, reference) and _uses_rule(current, reference, missing))
        if responsible:
            owners.append(index)
    if len(owners) != 1:
        return None
    result = {'reference': reference, 'missing_rule': missing, 'without_proposal_indices': owners}
    if not previous:
        result['stale_reference'] = True
    return result


def _rule_reference_conflict(source, entries, log):
    """Resolve each compiler-reported missing rule to a unique proposal."""
    diagnostics = _unknown_rule_diagnostics(log)
    declared = {}

    def declares(root, rule):
        key = (root.resolve(), rule)
        if key not in declared:
            declared[key] = _declares(root, rule)
        return declared[key]

    conflicts = []
    try:
        for reference, missing in dict.fromkeys(diagnostics):
            conflict = _reference_owner(source, entries, reference, missing, declares)
            if conflict is None:
                return None
            conflicts.append(conflict)
    except (OSError, UnicodeError):
        return None
    if not conflicts:
        return None
    if len(conflicts) == 1:
        return conflicts[0]
    result = {'conflicts': conflicts, 'without_proposal_indices': sorted({
        index for item in conflicts for index in item['without_proposal_indices']})}
    if all(item.get('stale_reference') for item in conflicts):
        result['stale_reference'] = True
    return result


class _PrintShapeFailure(ValueError):
    pass


def _merge_trial_proposal(trial, scratch, proposal, mechanical, input_cache, shape_baseline=None):
    run, task, directory, hooks = proposal
    if mechanical:
        if hooks:
            raise ValueError('mechanical proposal cannot request hooks')
        _mechanical_removal(run, directory, task['rules'], scratch, input_cache=input_cache)
    else:
        changes = merger.merge_grammar(run/'inputs/frozen', trial, directory, task['rules'], hooks,
                                      canonical_removal=input_cache.canonical_seeded_removal)
        if shape_baseline is not None:
            diagnostics = [error for text in changes.values()
                           for error in shapes.introduced(shapes.production_shapes(text), shape_baseline)]
            if diagnostics:
                raise _PrintShapeFailure(diagnostics)
        for name, text in changes.items():
            target = trial/name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(text)
    removed, kept = mined.remove(trial, task['rules'])
    if set(removed) != set(task['rules']) or kept:
        raise ValueError('assigned removals have retained users')
    a.apply_hooks(trial/'grammar', hooks)


def _probe_lines(binary, grammar, name, text, fds, *, compositional=False, types=()):
    env = {k:v for k,v in os.environ.items() if not k.startswith(('ORACLE_GRAMMAR_', 'PROBE_'))}
    env['ORACLE_GRAMMAR_DIR'] = str(grammar)
    env['PROBE_TYPES'] = ' '.join(types)
    command = [str(binary), name, text]
    if compositional:
        command.append('--without=m_,t_')
    with tempfile.TemporaryFile() as output:
        subprocess.run(command, env=env, stdout=output, stderr=subprocess.DEVNULL,
                       check=True, timeout=24, pass_fds=fds)
        output.seek(0)
        result = output.read(1024*1024+1)
    r.require(len(result) <= 1024*1024, 'parse trace exceeds the hint budget')
    return result.splitlines()


def _route_keys(line):
    return set(json.loads(line.removeprefix(b'  rules ')))


def _route_names(keys):
    return {key.split('.',1)[0].split('#',1)[0] for key in keys}


def _probe_rules(binary, grammar, name, text, fds):
    traces = [_route_names(_route_keys(line)) for line in _probe_lines(binary,grammar,name,text,fds)
              if line.startswith(b'  rules ')]
    return set().union(*traces)


def _print_trace_delta(row, trace):
    original, printed = row.get('text'), row.get('first')
    if (row.get('readings') != 1 or 'matching_readings' in row
            or not all(isinstance(text, str) and 0 < len(text) <= 4096 for text in (original, printed))
            or original == printed):
        return None
    spelled = trace(row['name'], printed)
    retained = trace(row['name'], original)
    return retained, spelled-retained


def _new_productions(run, directory, wanted):
    found = set()
    for path in (directory/'grammar/syntax').glob('*.ebnf'):
        if path.name == 'mined.ebnf':
            continue
        source = path.read_text()
        if not any(name+' ::=' in source for name in wanted):
            continue
        base = run/'inputs/frozen/grammar/syntax'/path.name
        previous = mined.definitions(base.read_text().splitlines()) if base.exists() else {}
        found.update((wanted & mined.definitions(source.splitlines()).keys()) - previous.keys())
    return found


def _trace_binary(checkpoint, run):
    binary = Path(checkpoint['helpers'])/'probe'
    expected = r.read(run/'frozen/build.json')['binaries']['probe']
    r.require(binary.is_absolute() and binary.is_file() and not binary.is_symlink()
              and binary.stat().st_size <= 1024*1024*1024 and r.digest(binary) == expected,
              'probe differs from the frozen helper')
    return binary, expected


def _traced_meanings(lines):
    meanings = []
    for line in lines:
        if line.startswith(b'[{'):
            meanings.append((json.loads(line),set()))
        elif line.startswith(b'  rules ') and meanings:
            meanings[-1][1].update(_route_keys(line))
    return meanings


def _trace_delta(lines, row, delta):
    meanings = _traced_meanings(lines)

    def rules(indices, *, require_route):
        readings = row['all']
        wanted = set()
        for index in indices:
            r.require(type(index) is int and 0 <= index < len(readings), 'invalid trace reading index')
            trace, = [route for meaning, route in meanings if meaning == readings[index]]
            r.require(trace or not require_route, 'empty trace for added native reading')
            wanted.update(trace)
        return wanted

    return (rules(delta['added_current_indices'], require_route=True),
            rules(delta['retained_current_indices'], require_route=False))


def _added_family_routes(binary, grammars, fixture, failure, fds):
    counts = re.fullmatch(r'expected Some\((\d{1,2})\) readings, got (\d{1,2})', failure['why'])
    if counts is None:
        return set()
    expected, observed = map(int, counts.groups())
    if not 1 <= expected < observed <= 16:
        return set()
    cases = [case for case in r.read(fixture)['cases'] if case.get('name') == failure['name']
             and 'context' not in case and case.get('compositional', True)
             and isinstance(case.get('text'), str) and 0 < len(case['text']) <= 4096]
    if len(cases) != 1:
        return set()
    case, = cases
    before, after = [_traced_meanings(_probe_lines(binary, grammar, case['name'], case['text'], fds,
                                                  compositional=True, types=case.get('types', [])))
                     for grammar in grammars]
    old, new = ([meaning for meaning, _ in traces] for traces in (before, after))
    if len(old) != expected or len(new) != observed or any(meaning not in new for meaning in old):
        return set()
    additions = set().union(*(routes for meaning, routes in after if meaning not in old))
    return _route_names(additions - set().union(*(routes for _, routes in before)))


def _retained_family_hints(checkpoint, source, grammar, drafts, evidence):
    """Rank hints from an actual failed trial. Exact singleton checks establish causality."""
    witnesses = evidence.get('fixtures', [evidence])
    if not 1 <= sum(len(item['failures']) for item in witnesses) <= RETAINED_FAMILY_PROBE_LIMIT:
        return []
    try:
        binary, expected = _trace_binary(checkpoint, Path(checkpoint['inputs']))
        hints = set()
        with retire_cpu.reserve(Path(checkpoint['cpu_pool']), 1, checkpoint['cpu_capacity'],
                                wait=False, background=True) as fds:
            for item in witnesses:
                fixture = source/item['fixture']
                if r.digest(fixture) != item['sha256']:
                    return []
                for failure in item['failures']:
                    try:
                        wanted = _added_family_routes(binary, (source/'grammar', grammar), fixture, failure, fds)
                    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
                        continue
                    matches = [index for index, run, directory in drafts
                               if wanted and _new_productions(run, directory, wanted)]
                    if len(matches) == 1:
                        hints.update(matches)
        return sorted(hints) if r.digest(binary) == expected else []
    except (OSError, ValueError, KeyError, TypeError):
        return []


def _retained_added_readings(checks, faces):
    if checks is None or not faces:
        return []
    native = r.read(checks/'native.json')['reading_preservation']
    deltas = [delta for delta in native['additions'] if delta['name'] in faces]
    names = {delta['name'] for delta in deltas}
    if native.get('merged_faces') or not deltas or len(names) != len(deltas):
        return []
    rows = [row for row in r.rows(checks/'rows.jsonl') if row['name'] in names]
    if len(rows) != len(names) or {row['name'] for row in rows} != names:
        return []
    by_name = {row['name']: row for row in rows}
    return [(by_name[delta['name']], delta) for delta in deltas[:3]
            if 0 < len(by_name[delta['name']]['text']) <= 4096]


def _retained_print_hints(checkpoint, grammar, rows_path, drafts, evidence, *, reading_checks=None):
    """Use bounded print and added-reading traces to rank independently checked proposals."""
    failures = [item for item in evidence['failures'] if isinstance(item, dict)]
    names = {item['face'] for item in failures if 'exact_default' in item.get('lost', [])}
    parse_faces = {item['face'] for item in failures if item.get('gate') == 'parse/reading-count/text'}
    try:
        rows = [row for row in r.rows(rows_path) if row['name'] in names] if names else []
        if len(rows) != len(names) or {row['name'] for row in rows} != names:
            rows = []
        try:
            readings = _retained_added_readings(reading_checks, parse_faces)
        except (OSError, ValueError, KeyError, TypeError):
            readings = []
        if not rows and not readings:
            return []
        binary, expected = _trace_binary(checkpoint, Path(checkpoint['inputs']))
        routes = []
        # Focused isolation also needs CPU tokens. Keep the bounded route
        # hint available while a foreground bundle is draining the pool.
        with retire_cpu.reserve(Path(checkpoint['cpu_pool']), 1, checkpoint['cpu_capacity'],
                                background=True) as fds:
            ordered = sorted(rows, key=lambda item: (len(item['text']), item['name']))
            for row in ordered[:RETAINED_PRINT_TRACE_LIMIT]:
                try:
                    delta = _print_trace_delta(row, lambda name, text: _probe_rules(binary, grammar, name, text, fds))
                except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
                    continue
                if delta is not None and delta[1]:
                    routes.append(delta[1])
            for row, delta in readings:
                try:
                    lines = _probe_lines(binary, grammar, row['name'], row['text'], fds)
                    added, retained = _trace_delta(lines, row, delta)
                    routes.append(_route_names(added) - _route_names(retained))
                except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
                    continue
        hints = set()
        for wanted in routes:
            matches = [index for index, run, directory in drafts
                       if wanted and _new_productions(run, directory, wanted)]
            if len(matches) == 1:
                hints.update(matches)
        return sorted(hints) if r.digest(binary) == expected else []
    except (OSError, ValueError, KeyError, TypeError):
        return []


def _retained_family_failure(failed_indices, proof, source, logs):
    checks = proof.get('family_checks', [])
    if not 1 <= len(failed_indices) <= RETAINED_FAMILY_PROBE_LIMIT:
        return None
    witnesses = []
    for index in failed_indices:
        if not 0 <= index < len(checks) or checks[index]['kind'] not in ('baseline', 'prior-proposal'):
            return None
        check = checks[index]
        fixture = check['fixture']
        try:
            failures = r.read(logs/check['log'])['failures']
            digest = r.digest(source/fixture)
        except (ValueError, OSError, KeyError, TypeError):
            return None
        if not isinstance(failures, list) or not failures:
            return None
        witnesses.append({'fixture': fixture, 'sha256': digest, 'failures': failures})
    return witnesses[0] if len(witnesses) == 1 else {'fixtures': witnesses}


def _check_trial_native(context, items, proof, input_cache, grammar, logs, *, focused=None):
    control = context['control'] if focused is None else focused
    directory = logs/('native-control' if focused is None else 'native-focused')
    checkpoint = context['checkpoint']
    if focused is not None:
        checkpoint = {**checkpoint, 'native_control_threads': control['threads']}
    for item in items:
        item['native_control'] = str(directory)
    try:
        _control_check(checkpoint, control, grammar, directory)
    except _NativeControlFailure:
        if not input_cache.unchanged() or r.tree(grammar) != proof['trial_grammar']:
            raise _UncertainPromotion('failed native trial inputs changed during verification')
        proof['verified_native_snapshot'] = {
            key: control[key] for key in ('profile_sha256', 'manifest_sha256', 'threads')}
        raise


def _retained_parse_candidates(checkpoint, control, grammar, directory):
    """Find cheap reading-count or recognition witnesses in the retained inputs."""
    _control_verify(control)
    directory.mkdir()
    selected = w.subset(directory, control['names'], control['data'])
    expected = [entry for entry in control['data'][0].values() if entry['name'] in selected]
    r.require(len(expected) == control['occurrences'], 'retained parse cohort differs')
    output = directory/'rows.jsonl'
    threads = min(4, control['threads'])
    args = [directory/'gold.jsonl', directory/'oracle.json', directory/'slice.json', output]
    with _audit_cpu(checkpoint, threads, checkpoint.get('cpu_capacity', 32)):
        r.execute(control['run'], directory, grammar, 'score', args, threads)
    rows = r.rows(output)
    names = set()
    for row, old in zip(rows, expected, strict=True):
        r.require(row['name'] == old['name'] and row['text'] == old['text'],
                  'retained parse screen occurrence differs')
        if row['status'] != 'equal' or row.get('readings') != old['parse']['readings']:
            names.add(row['name'])
    _control_verify(control)
    return sorted(names)


def _check_retained_parse(context, items, proof, input_cache, grammar, logs, drafts):
    control, checkpoint = context['control'], context['checkpoint']
    directory = logs/'retained-parse-screen'
    try:
        names = _retained_parse_candidates(checkpoint, control, grammar, directory)
        focus = _focused_native_control(control, names[:RETAINED_NATIVE_PROBE_LIMIT//2]) if names else None
        metadata = directory/'result.json'
        r.write(metadata, {'faces': control['occurrences'], 'suspect_faces': names,
                           'rows_sha256': r.digest(directory/'rows.jsonl')})
        proof['retained_parse_screen_sha256'] = r.digest(metadata)
    except (OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError) as error:
        proof['retained_parse_screen_error'] = str(error)
        return
    if focus is None:
        return
    try:
        _check_trial_native(context, items, proof, input_cache, grammar, directory, focused=focus)
    except _NativeControlFailure as error:
        witness = _retained_native_failure(focus, error.report)
        if witness is None or not any(isinstance(item, dict) and item.get('gate') == 'parse/reading-count/text'
                                      and item['face'] in names for item in witness['failures']):
            return
        proof['retained_native_failure'] = witness
        native = directory/'native-focused'
        proof['retained_parse_native_sha256'] = r.digest(native/'result.json')
        proof['retained_parse_focus'] = {key: focus[key] for key in ('names', 'occurrences')}
        hints = _retained_print_hints(checkpoint, grammar, native/'rows.jsonl', drafts, witness,
                                      reading_checks=native)
        if hints:
            proof['retained_print_hints'] = hints
        raise


def _early_render_losses(rows, expected):
    losses = []
    for row, before in zip(rows, expected, strict=True):
        if row['status'] != 'equal' or row['name'] != before['name'] or row['text'] != before['text']:
            return None
        if row['readings'] != before['parse']['readings']:
            losses.append({'face': row['name'], 'gate': 'parse/reading-count/text'})
        if before['flags'][3] and not r.flags(row)[3]:
            losses.append({'face': row['name'], 'lost': ['exact_default']})
    return losses


def _early_score(checkpoint, grammar, data, folder, names, *, render):
    folder.mkdir()
    run = Path(checkpoint['inputs'])
    # Keep complete source records so a face name shared by multiple printings
    # selects every occurrence with its original companion-face identity.
    selected = set(names)
    expected = [entry for entry in data[0].values() if entry['name'] in selected]
    slice_path = folder/'slice.json'
    r.write(slice_path, {'cards': names})
    rows = folder/'rows.jsonl'
    threads = checkpoint['audit_threads'] if render else _parse_screen_threads(checkpoint)
    capacity = checkpoint['cpu_capacity']
    # This screen can use fewer workers without changing its ordered results.
    # Start when half of the requested bundle is free so a large screen does
    # not block smaller diagnostics while other processes finish their leases.
    minimum = (threads+1)//2
    args = [run/'frozen/gold.jsonl', run/'frozen/oracle.json', slice_path, rows]
    if render:
        args.append('--render')
    with _audit_cpu(checkpoint, threads, capacity, minimum_threads=minimum) as granted:
        r.execute(run, folder, grammar, 'score', args, granted)
    return r.rows(rows), expected, granted


def _early_parse_losses(rows, expected):
    failures = []
    for row, before in zip(rows, expected, strict=True):
        if row['name'] != before['name'] or row['text'] != before['text']:
            raise ValueError('The early parse screen occurrence differs')
        if row['status'] != 'equal' or row.get('readings') != before['parse']['readings']:
            failures.append({'face': row['name'], 'gate': 'parse/reading-count/text'})
    return failures


def _early_default_print(checkpoint, grammar, data, logs):
    run = Path(checkpoint['inputs'])
    manifest = r.verify(run)
    protected = {entry['name'] for entry in data[0].values() if entry['flags'][3]}
    remaining = {entry['name'] for entry in data[0].values()} - protected
    parse_losses = []
    if remaining:
        folder = logs/'unprotected-parse-screen'
        rows, expected, granted = _early_score(checkpoint, grammar, data, folder, sorted(remaining), render=False)
        parse_losses = _early_parse_losses(rows, expected)
        r.write(folder/'result.json', {'occurrences': len(expected),
                'rows_sha256': r.digest(folder/'rows.jsonl'), 'failures': parse_losses, 'threads': granted})
    losses = []
    if protected:
        folder = logs/'default-print-screen'
        rows, expected, granted = _early_score(checkpoint, grammar, data, folder, sorted(protected), render=True)
        losses = _early_render_losses(rows, expected)
        r.write(folder/'result.json', {'status': 'inconclusive' if losses is None else 'checked',
                'occurrences': len(expected), 'protected_occurrences': sum(entry['flags'][3] for entry in expected),
                'rows_sha256': r.digest(folder/'rows.jsonl'), 'failures': losses, 'threads': granted})
    if r.verify(run) != manifest:
        raise ValueError('The early screening inputs changed')
    return losses + parse_losses if losses is not None else parse_losses or None


def _witness_control(checkpoint, losses, logs):
    if not losses or not 1 <= len(losses) <= RETAINED_NATIVE_PROBE_LIMIT:
        return None
    names = sorted({item['face'] for item in losses})
    if len({(item['face'], 'gate' in item) for item in losses}) != len(losses):
        return None
    closure = tasks.close_cohort(Path(checkpoint['inputs']), {'names': names},
                                 max_occurrences=RETAINED_NATIVE_PROBE_LIMIT)
    profile = logs/'default-print-profile.json'
    r.write(profile, {'cards': closure['names']})
    profile.chmod(0o444)
    spec = {'path': str(profile), 'sha256': r.digest(profile)}
    cp = dict(checkpoint, native_regression_controls=spec, native_control_threads=2)
    return cp, _control_setup(cp), spec


def _confirms_early_loss(loss, observed):
    if 'lost' not in loss:
        return loss in observed
    # The focused render also checks verified default printing. Losing that
    # flag as well still confirms the early exact-default loss on the face.
    return any(isinstance(item, dict) and item['face'] == loss['face']
               and set(loss['lost']) <= set(item.get('lost', ())) for item in observed)


def _check_early_default(context, proof, input_cache, grammar, logs, drafts):
    checkpoint = context['checkpoint']
    control = context['control']
    previous = context.get('early_print_recheck')
    losses = None
    try:
        if previous is None:
            data = control['data'] if control is not None else w.payload(Path(checkpoint['inputs']))
            losses = _early_default_print(checkpoint, grammar, data, logs)
            for screen in ('default-print-screen', 'unprotected-parse-screen'):
                result = logs/screen/'result.json'
                if result.is_file():
                    proof[screen.replace('-', '_')+'_sha256'] = r.digest(result)
            # The independent closure can veto a trial. Every provisional
            # batch still runs full allowed-population validation.
            sampled = losses is not None and len(losses) > RETAINED_NATIVE_PROBE_LIMIT
            witnesses = losses[:RETAINED_NATIVE_PROBE_LIMIT//2] if sampled else losses
            focus = _witness_control(checkpoint, witnesses, logs)
            if focus is None:
                return
            cp, focused, spec = focus
            if sampled:
                names = set(focused['names'])
                losses = [item for item in losses if item['face'] in names]
        else:
            cp = dict(checkpoint, native_regression_controls=previous, native_control_threads=2)
            focused, spec = _control_setup(cp), previous
            proof['early_print_rechecked_profile'] = previous
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        proof['default_print_screen_error'] = str(error)
        return
    try:
        _control_check(cp, focused, grammar, logs/'native-focused')
        result = logs/'native-focused/result.json'
        if previous is not None and result.is_file():
            proof['early_print_recheck_sha256'] = r.digest(result)
    except _NativeControlFailure as error:
        failures = _retained_native_failure(focused, error.report)
        if failures is None:
            return
        if losses is not None and any(not _confirms_early_loss(item, failures['failures']) for item in losses):
            return
        if not input_cache.unchanged() or r.tree(grammar) != proof['trial_grammar']:
            raise _UncertainPromotion('Early print witness inputs changed during staging')
        proof.update(retained_native_failure=failures, early_print_control=spec,
                     verified_native_snapshot={key: focused[key] for key in ('profile_sha256','manifest_sha256','threads')})
        rows = logs/('default-print-screen' if previous is None else 'native-focused')/'rows.jsonl'
        hints = _retained_print_hints(cp, grammar, rows, drafts, failures,
                                     reading_checks=logs/'native-focused')
        if hints:
            proof['retained_print_hints'] = hints
        raise
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        proof['default_print_screen_error'] = str(error)


def _recheck_retained_family(context, index, proof, grammar, logs):
    """Repeat a known retained failure after the rest of its group has moved."""
    failure = context.get('family_suspects', {}).get(index)
    if failure is None:
        return
    source = context['source']
    witnesses = failure.get('fixtures', [failure])
    checks = {row['fixture']: row for row in proof['family_checks'] if row['kind'] != 'new-proposal'}
    fixtures = [source/row['fixture'] for row in witnesses]
    if any(row['fixture'] not in checks or r.digest(path) != row['sha256']
           for row, path in zip(witnesses, fixtures, strict=True)):
        return
    focused = logs/'family-focused'
    focused.mkdir()
    checkpoint = context['checkpoint']
    try:
        _helper(checkpoint, source, fixtures, focused/'family.log', grammar_dir=grammar,
                threads=min(2, _family_check_threads(checkpoint)))
    except _FamilyCheckFailure as error:
        observed = error.failed_fixtures
        if not observed or any(not 0 <= pos < len(witnesses)
                               or r.read(focused/f'family-{pos:03d}.log')['failures'] != witnesses[pos]['failures']
                               for pos in observed):
            return
        if any(r.digest(path) != row['sha256'] for row, path in zip(witnesses, fixtures, strict=True)):
            raise _UncertainPromotion('retained family changed during focused staging check')
        shutil.copyfile(focused/'family.log', logs/'family.log')
        selected = []
        for pos, row in enumerate(witnesses):
            name = f'family-{pos:03d}.log'
            shutil.copyfile(focused/name, logs/name)
            selected.append(dict(checks[row['fixture']], log=name))
        proof.update(family_checks=selected, family_check_scope='retained-witness')
        raise _FamilyCheckFailure(logs/'family.log', observed) from error


def _stage_trial(context, entries, label, *, merge_conflict_limit=1):
    """Merge and gate one ordered trial; publish nothing until every gate passes."""
    source, folder, manifest = context['source'], context['folder'], context['manifest']
    checkpoint, control = context['checkpoint'], context['control']
    logs = folder/label
    logs.mkdir()
    items = [{'job_id': proposal['job_id'], 'attempt_id': proposal['attempt_id']} for _, proposal in entries]
    prefix_tree = r.tree(source/'grammar')
    prefix_fixtures = {name: r.digest(source/name) for name in manifest['fixtures']}
    proof = {'proposal_indices': [index for index, _ in entries],
             'jobs': [item['job_id'] for item in items], 'status': 'running',
             'attempts': [item['attempt_id'] for item in items],
             'prefix_grammar': prefix_tree, 'prefix_fixtures': prefix_fixtures}
    input_cache = context.setdefault('proposal_inputs', _ProposalInputCache()).trial()
    merge_conflicts = []
    try:
        with tempfile.TemporaryDirectory(dir=folder) as temporary:
            trial = Path(temporary)
            shutil.copytree(source/'grammar', trial/'grammar')
            additions, prepared, trace_drafts = [], [], []
            for offset, ((index, proposal), item) in enumerate(zip(entries, items, strict=True)):
                run, task, directory, hooks = _proposal(proposal, input_cache=input_cache)
                mechanical = proposal.get('kind', 'author') == 'mechanical'
                fixture = None if mechanical else _fixture_path(task['fixture'])
                if fixture and ((source/fixture).exists() or str(fixture) in manifest['fixtures']
                                or fixture in [destination for _, destination in additions]):
                    raise ValueError('fixture path already exists')
                if fixture and str(fixture) in context['baseline_names']:
                    raise ValueError('fixture already registered')
                if fixture and not (source/fixture).resolve().is_relative_to(source):
                    raise ValueError('fixture escapes candidate source')
                family = _family(directory, proposal['report'], run,
                                 modes=checkpoint.get('family_case_modes') is True,
                                 gold_cache=context['gold_cache']) if fixture else None
                report = proposal['report']
                metadata = {'rules': task['rules'], 'task': report['task'], 'base_commit': report['source'],
                            'descriptor_sha256': report['descriptor_sha256'],
                            'support_sha256': report['support_sha256'], 'fixture_sha256': report.get('fixture_sha256'),
                            'kind': proposal.get('kind', 'author')}
                try:
                    _merge_trial_proposal(trial, folder, (run, task, directory, hooks), mechanical, input_cache,
                                          context.get('shape_baseline'))
                except _PrintShapeFailure as error:
                    if not input_cache.unchanged():
                        raise _UncertainPromotion('proposal inputs changed during print-shape diagnosis')
                    path = logs/'print-shape.json'
                    r.write(path, {'proposal_index': index, 'diagnostics': error.args[0]})
                    proof['print_shape_failed_at'] = [index]
                    raise ValueError(f'print shape check failed: {path}') from error
                except merger.MergeConflict as error:
                    # The merge preview raises before it writes to the trial.
                    # Continue over the same prefix a retry would use.
                    merge_conflicts.append((index, error))
                    if len(merge_conflicts) == merge_conflict_limit:
                        break
                    continue
                if fixture:
                    filename = 'family.json' if len(entries) == 1 else f'family-{offset:03d}.json'
                    temporary_family = trial/filename
                    r.write(temporary_family, family)
                    additions.append((temporary_family, fixture))
                prepared.append((task, fixture, item, metadata))
                if not mechanical:
                    trace_drafts.append((index, run, directory))
            if merge_conflicts:
                if not input_cache.unchanged():
                    raise _UncertainPromotion('proposal inputs changed during merge diagnosis')
                raise merge_conflicts[0][1]
            proof['proposals'] = [{'job_id': item['job_id'], 'attempt_id': item['attempt_id'],
                                   'base_commit': metadata['base_commit'],
                                   'descriptor_sha256': metadata['descriptor_sha256'],
                                   'support_sha256': metadata['support_sha256'],
                                   'fixture_sha256': metadata['fixture_sha256']}
                                  for _, _, item, metadata in prepared]
            if (r.digest(context['registry_path']) != context['registry_hash']
                    or any(r.digest(source/name) != digest for name, digest in context['baseline'])):
                raise ValueError('baseline family changed during staging')
            fixtures = ([source/name for name, _ in context['baseline']]
                        + [source/path for path in manifest['fixtures']] + [path for path, _ in additions])
            if len(fixtures) > 4096:
                raise ValueError('trial exceeds family-check fixture limit')
            descriptions = ([{'fixture': name, 'kind': 'baseline'} for name, _ in context['baseline']]
                            + [{'fixture': name, 'kind': 'prior-proposal'} for name in manifest['fixtures']]
                            + [{'fixture': str(destination), 'kind': 'new-proposal'} for _, destination in additions])
            family_checks = [dict(entry, log=f'family-{index:03d}.log')
                             for index, entry in enumerate(descriptions)]
            proof['family_checks'] = family_checks
            proof['trial_grammar'] = r.tree(trial/'grammar')
            proof['new_fixtures'] = [{'fixture': str(destination), 'sha256': r.digest(path)}
                                     for path, destination in additions]
            witnesses = context.get('native_suspects', {}).get(entries[0][0]) if len(entries) == 1 else None
            if witnesses:
                focused = _focused_native_control(witnesses['control'], witnesses['names'])
                if focused is not None:
                    proof['native_focused_names'] = focused['names']
                    _check_trial_native(context, items, proof, input_cache, trial/'grammar', logs, focused=focused)
            if (control is not None and checkpoint.get('early_retained_parse') is True
                    and len(entries) >= MIN_EARLY_PRINT_GROUP):
                _check_retained_parse(context, items, proof, input_cache, trial/'grammar', logs, trace_drafts)
            for item in items:
                item.update(family_logs=str(logs), family_checks=family_checks)
            if fixtures:
                try:
                    if len(entries) == 1:
                        _recheck_retained_family(context, entries[0][0], proof, trial/'grammar', logs)
                    _helper(checkpoint, source, fixtures, logs/'family.log', grammar_dir=trial/'grammar')
                except _FamilyCheckFailure as error:
                    for item in items:
                        item['family_checks'] = proof['family_checks']
                    if (not input_cache.unchanged() or r.tree(trial/'grammar') != proof['trial_grammar']
                            or any(r.digest(path) != record['sha256']
                                   for (path, _), record in zip(additions, proof['new_fixtures'], strict=True))):
                        raise _UncertainPromotion('failed family trial inputs changed during verification')
                    proof['verified_family_snapshot'] = True
                    if error.failed_fixtures:
                        witness = _retained_family_failure(error.failed_fixtures, proof, source, logs)
                        if witness is not None:
                            proof['retained_family_failure'] = witness
                            if len(entries) > 1:
                                hints = _retained_family_hints(checkpoint, source, trial/'grammar', trace_drafts, witness)
                                if hints:
                                    proof['retained_family_hints'] = hints
                    raise
            if control is not None:
                try:
                    _check_trial_native(context, items, proof, input_cache, trial/'grammar', logs)
                except _NativeControlFailure as error:
                    if len(entries) > 1:
                        witness = _retained_native_failure(control, error.report)
                        if witness is not None:
                            proof['retained_native_failure'] = witness
                            native = logs/'native-control'
                            hints = _retained_print_hints(checkpoint, trial/'grammar', native/'rows.jsonl',
                                                          trace_drafts, witness, reading_checks=native)
                            if hints:
                                proof['retained_print_hints'] = hints
                    raise
            if context['library_control'] is not None:
                for item in items:
                    item['library_control'] = str(logs/'library-control')
                library_controls.check(checkpoint, context['library_control'], trial/'grammar', logs/'library-control')
            if ((context.get('early_default_print') and len(entries) >= MIN_EARLY_PRINT_GROUP)
                    or (context.get('early_print_recheck') is not None and len(entries) > 1)):
                _check_early_default(context, proof, input_cache, trial/'grammar', logs, trace_drafts)
            if (not input_cache.unchanged() or r.tree(source/'grammar') != prefix_tree
                    or r.tree(trial/'grammar') != proof['trial_grammar']
                    or r.digest(context['registry_path']) != context['registry_hash']
                    or any(r.digest(source/name) != digest for name, digest in context['baseline'])
                    or any(r.digest(source/name) != digest for name, digest in prefix_fixtures.items())
                    or any(r.digest(path) != record['sha256']
                           for (path, _), record in zip(additions, proof['new_fixtures'], strict=True))):
                raise _UncertainPromotion('trial or accepted prefix changed during verification')
            _promote_trial(source, trial, additions)
        for task, fixture, item, metadata in prepared:
            if fixture:
                manifest['fixtures'].append(str(fixture))
            manifest['rules'].extend(task['rules'])
            item.update(status='included', **metadata)
        proof['status'] = 'passed'
    except (ValueError, OSError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        try:
            if (r.tree(source/'grammar') != prefix_tree
                    or r.digest(context['registry_path']) != context['registry_hash']
                    or any(r.digest(source/name) != digest for name, digest in context['baseline'])
                    or any(r.digest(source/name) != digest for name, digest in prefix_fixtures.items())):
                raise _UncertainPromotion('accepted prefix changed during failed trial')
        except OSError as mismatch:
            raise _UncertainPromotion('accepted prefix unavailable after failed trial') from mismatch
        if merge_conflicts:
            first, error = merge_conflicts[0]
            proof['merge_conflict_at'] = first
            if len(merge_conflicts) > 1:
                proof['merge_conflicts'] = [{'proposal_index': index, 'reason': str(conflict)}
                                            for index, conflict in merge_conflicts]
        proof.update(status='failed', reason=str(error))
        if isinstance(error, _FamilyCheckFailure) and error.failed_fixtures:
            checks = proof.get('family_checks', [])
            if all(0 <= index < len(checks) and checks[index]['kind'] == 'new-proposal'
                   for index in error.failed_fixtures):
                failed = {checks[index]['fixture'] for index in error.failed_fixtures}
                proof['new_families_failed_at'] = [
                    index for (index, _), (_, fixture, _, _) in zip(entries, prepared, strict=True)
                    if fixture and str(fixture) in failed]
        elif isinstance(error, _FamilyCheckFailure) and len(entries) > 1:
            conflict = _rule_reference_conflict(source, entries, logs/'family.log')
            if conflict is not None:
                key = 'stale_rule_conflict' if conflict.get('stale_reference') else 'removed_rule_conflict'
                proof[key] = conflict
        for item in items:
            item.update(status='integration-rejected', reason=str(error))
    try:
        for name, path in (('family_log_sha256', logs/'family.log'),
                           ('native_result_sha256', logs/'native-control/result.json'),
                           ('native_focused_result_sha256', logs/'native-focused/result.json'),
                           ('library_result_sha256', logs/'library-control/result.json'),
                           ('print_shape_sha256', logs/'print-shape.json')):
            if path.is_file():
                proof[name] = r.digest(path)
        r.write(logs/'trial.json', proof)
        proof_hash = r.digest(logs/'trial.json')
    except (ValueError, OSError) as error:
        raise _UncertainPromotion('trial evidence could not be retained: '+str(error)) from error
    for item in items:
        item['trial_proof'] = {'path': str(logs/'trial.json'), 'sha256': proof_hash}
    return items, proof


def _replay_focused_grammar(context, entries, trial, input_cache):
    grammar = trial/'grammar'
    shutil.copytree(context['source']/'grammar', grammar)
    for _, item in entries:
        _merge_trial_proposal(trial, context['folder'], _proposal(item, input_cache=input_cache),
                              item.get('kind') == 'mechanical', input_cache)
    return grammar


def _probe_retained_family(context, entries, witnesses, logs, input_cache, *, partial=False):
    """Check witnessed retained families on a private ordered subset."""
    logs.mkdir()
    source, folder = context['source'], context['folder']
    known = dict(context['baseline']) | {
        name: r.digest(source/name) for name in context['manifest']['fixtures']}
    fixture_hashes = {source/item['fixture']: item['sha256'] for item in witnesses}
    if (len(fixture_hashes) != len(witnesses)
            or any(known.get(str(path.relative_to(source))) != digest or r.digest(path) != digest
                   for path, digest in fixture_hashes.items())):
        raise _UncertainPromotion('retained family changed before focused replay')
    proof = {'proposal_indices': [index for index, _ in entries]}
    if len(witnesses) == 1:
        proof.update(fixture=witnesses[0]['fixture'], fixture_sha256=witnesses[0]['sha256'])
    else:
        proof['fixtures'] = [{'fixture': item['fixture'], 'sha256': item['sha256']} for item in witnesses]
    prefix = r.tree(source/'grammar')
    try:
        with tempfile.TemporaryDirectory(dir=folder) as temporary:
            grammar = _replay_focused_grammar(context, entries, Path(temporary), input_cache)
            tree = r.tree(grammar)
            proof['trial_grammar'] = tree
            try:
                _helper(context['checkpoint'], source, list(fixture_hashes), logs/'family.log',
                        grammar_dir=grammar, threads=2)
                outcome = 'passed'
            except _FamilyCheckFailure as error:
                observed = error.failed_fixtures
                matched = (bool(observed) and (partial or len(observed) == len(witnesses))
                           and all(0 <= i < len(witnesses)
                                   and r.read(logs/f'family-{i:03d}.log')['failures'] == witnesses[i]['failures']
                                   for i in observed))
                outcome = 'reproduced' if matched else 'inconclusive'
            if not input_cache.unchanged() or r.tree(grammar) != tree:
                raise _UncertainPromotion('retained family inputs changed during focused replay')
    except (ValueError, OSError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        outcome = 'inconclusive'
        proof['reason'] = str(error)
    try:
        if (r.tree(source/'grammar') != prefix or r.digest(context['registry_path']) != context['registry_hash']
                or any(r.digest(path) != digest for path, digest in fixture_hashes.items())):
            raise _UncertainPromotion('retained family prefix changed during focused replay')
    except OSError as error:
        raise _UncertainPromotion('retained family prefix unavailable after focused replay') from error
    proof['outcome'] = outcome
    r.write(logs/'probe.json', proof)
    return outcome


def _locate_retained_culprit(entries, probe):
    while len(entries) > 1:
        middle = len(entries)//2
        left, right = entries[:middle], entries[middle:]
        outcome = probe(left)
        if outcome == 'reproduced':
            entries = left
        elif outcome == 'passed' and probe(right) == 'reproduced':
            entries = right
        else:
            return None
    return entries[0][0]


def _isolate_retained(entries, test, folder, input_cache, *, verify_full=False, limit=1, stop_after=None, hints=()):
    """Find independent regressions before the fully gated retry."""
    folder.mkdir()
    results = {}

    def probe(group):
        indices = tuple(index for index, _ in group)
        if indices not in results:
            logs = folder/f'probe-{len(results):02d}'
            results[indices] = test(group, logs, input_cache)
        return results[indices]

    if probe([]) != 'passed' or (verify_full and probe(entries) != 'reproduced'):
        return []
    culprits = []
    if hints:
        for entry in entries:
            index, _ = entry
            if index in hints and probe([entry]) == 'reproduced':
                culprits.append(index)
                if len(culprits) >= limit or (stop_after is not None and stop_after(index)):
                    return culprits
    if culprits:
        entries = [entry for entry in entries if entry[0] not in culprits]
        if not entries or probe(entries) != 'reproduced':
            return culprits
    while entries:
        culprit = _locate_retained_culprit(entries, probe)
        if culprit is None:
            break
        culprits.append(culprit)
        if len(culprits) >= limit or (stop_after is not None and stop_after(culprit)):
            break
        entries = [entry for entry in entries if entry[0] != culprit]
        if not entries or probe(entries) != 'reproduced':
            break
    return culprits


def _matches_verified_family_trial(context, entries, failure, trial):
    return (trial is not None and trial.get('verified_family_snapshot') is True
            and trial.get('status') == 'failed' and trial.get('retained_family_failure') == failure
            and trial.get('proposal_indices') == [index for index, _ in entries]
            and trial.get('jobs') == [item['job_id'] for _, item in entries]
            and trial.get('attempts') == [item['attempt_id'] for _, item in entries]
            and trial.get('prefix_grammar') == r.tree(context['source']/'grammar'))


def _isolate_retained_family(context, entries, failure, folder, *, limit, initial_trial=None):
    witnesses = failure.get('fixtures', [failure])
    multiple = len(witnesses) > 1

    def test(group, logs, input_cache):
        return _probe_retained_family(context, group, witnesses, logs, input_cache,
                                      partial=multiple and len(group) < len(entries))

    inputs = context.setdefault('proposal_inputs', _ProposalInputCache()).trial()
    # A hash-checked staging failure already ran this exact union and these
    # retained fixtures. Still require the accepted base, singleton witnesses,
    # and a fresh full staging trial for the remaining union.
    verified = _matches_verified_family_trial(context, entries, failure, initial_trial)
    hints = initial_trial.get('retained_family_hints', ()) if verified else ()
    verify_full = multiple and not verified
    return _isolate_retained(entries, test, folder, inputs, verify_full=verify_full,
                             limit=limit if multiple else 1, hints=hints)


def _retained_native_failure(control, report):
    """Only route a complete, localized native rejection."""
    failures = report.get('failures')
    if (type(report.get('faces')) is not int or report['faces'] != control['occurrences']
            or not isinstance(failures, list) or not 1 <= len(failures) <= RETAINED_NATIVE_PROBE_LIMIT):
        return None
    faces = set()
    for item in failures:
        if item == 'native readings changed or failed validation':
            continue
        if not isinstance(item, dict) or item.get('face') not in control['names']:
            return None
        if set(item) == {'face', 'gate'} and item['gate'] == 'parse/reading-count/text':
            faces.add(item['face'])
        elif (set(item) == {'face', 'lost'} and isinstance(item['lost'], list)
              and 1 <= len(item['lost']) <= len(r.PRINT_GATES) and all(gate in r.PRINT_GATES for gate in item['lost'])):
            faces.add(item['face'])
        else:
            return None
    if len(faces) == 1 and len(failures) <= 3:
        return {'face': next(iter(faces)), 'failures': failures}
    if len(faces) > 1:
        return {'faces': sorted(faces), 'failures': failures}
    return None


def _reproduces_native_failure(report, occurrences, expected, *, partial):
    if report.get('faces') != occurrences or set(report) != {'faces', 'failures'}:
        return False
    observed = report['failures']
    if not isinstance(observed, list):
        return False
    if observed == expected:
        return True
    if not partial or not any(isinstance(item, dict) for item in observed):
        return False
    # Smaller proposal groups can reproduce just one of the named failures.
    # The ordered comparison also rejects repeats absent from the original.
    remaining = iter(expected)
    return all(item in remaining for item in observed)


def _probe_retained_native(context, entries, control, failures, logs, input_cache, *, partial=False):
    logs.mkdir()
    source = context['source']
    prefix = r.tree(source/'grammar')
    proof = {'proposal_indices': [index for index, _ in entries], 'names': control['names']}
    checkpoint = {**context['checkpoint'], 'native_control_threads': control['threads']}
    try:
        with tempfile.TemporaryDirectory(dir=context['folder']) as temporary:
            grammar = _replay_focused_grammar(context, entries, Path(temporary), input_cache)
            tree = r.tree(grammar)
            proof['trial_grammar'] = tree
            try:
                _control_check(checkpoint, control, grammar, logs/'native-control')
                outcome = 'passed'
            except _NativeControlFailure as error:
                matched = _reproduces_native_failure(error.report, control['occurrences'], failures, partial=partial)
                outcome = 'reproduced' if matched else 'inconclusive'
                proof['reproduced_full_failure'] = matched and error.report['failures'] == failures
            if not input_cache.unchanged() or r.tree(grammar) != tree:
                raise _UncertainPromotion('native control inputs changed during focused replay')
            proof['result_sha256'] = r.digest(logs/'native-control/result.json')
    except (ValueError, OSError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        outcome = 'inconclusive'
        proof['reason'] = str(error)
    if r.tree(source/'grammar') != prefix or r.digest(context['registry_path']) != context['registry_hash']:
        raise _UncertainPromotion('native control prefix changed during focused replay')
    proof['outcome'] = outcome
    r.write(logs/'probe.json', proof)
    return outcome


def _matches_verified_native_trial(context, entries, failure, trial):
    control = context['control']
    return (trial is not None and trial.get('verified_native_snapshot') ==
            {key: control[key] for key in ('profile_sha256', 'manifest_sha256', 'threads')}
            and trial.get('status') == 'failed' and trial.get('retained_native_failure') == failure
            and trial.get('proposal_indices') == [index for index, _ in entries]
            and trial.get('jobs') == [item['job_id'] for _, item in entries]
            and trial.get('attempts') == [item['attempt_id'] for _, item in entries]
            and trial.get('prefix_grammar') == r.tree(context['source']/'grammar'))


def _focused_native_control(control, names):
    try:
        closure = tasks.close_cohort(control['run'], {'names': list(names)},
                                     max_occurrences=RETAINED_NATIVE_PROBE_LIMIT)
    except tasks.CohortClosureLimit:
        return None
    if not set(closure['names']) <= set(control['names']):
        return None
    return {**control, 'names': closure['names'], 'occurrences': closure['closure_occurrence_count'],
            'threads': min(2, control['threads'])}


def _isolate_retained_native(context, entries, failure, folder, *, limit, initial_trial=None):
    control = context['control']
    _control_verify(control)
    names = failure['faces'] if 'faces' in failure else [failure['face']]
    multiple = len(names) > 1
    focused = _focused_native_control(control, names)
    if focused is None:
        return []
    full_witnesses = set()

    def test(group, logs, input_cache):
        outcome = _probe_retained_native(context, group, focused, failure['failures'], logs, input_cache,
                                         partial=multiple and len(group) < len(entries))
        if outcome == 'reproduced' and len(group) == 1 and r.read(logs/'probe.json').get('reproduced_full_failure'):
            full_witnesses.add(group[0][0])
        return outcome

    inputs = context.setdefault('proposal_inputs', _ProposalInputCache()).trial()
    verified = _matches_verified_native_trial(context, entries, failure, initial_trial)
    if not verified and initial_trial is not None:
        scope = {key: focused[key] for key in ('names', 'occurrences')}
        if initial_trial.get('retained_parse_focus') == scope and initial_trial.get('retained_parse_native_sha256'):
            verified = _matches_verified_native_trial(dict(context, control=focused), entries, failure, initial_trial)
    hints = initial_trial.get('retained_print_hints', ()) if verified else ()
    verify_full = not multiple or not verified
    return _isolate_retained(entries, test, folder, inputs, verify_full=verify_full,
                             limit=limit if multiple else 1, stop_after=full_witnesses.__contains__, hints=hints)


def _retry_retained_control(context, entries, proof, whole_proof, size, withheld=()):
    family = proof.get('retained_family_failure')
    native = proof.get('retained_native_failure')
    if (family is None and native is None) or len(withheld) >= size:
        return False
    selection = [entry for entry in entries if entry[0] not in withheld]
    if len(selection) < 2:
        return False
    route = 'retained_family' if family is not None else 'retained_native'
    stem = f"{route.replace('_', '-')}-{entries[0][0]:03d}"
    folder = context['folder']/stem
    if folder.exists():
        folder = folder.with_name(f'{stem}-{len(withheld):03d}')
    if family is not None:
        culprits = _isolate_retained_family(context, selection, family, folder, limit=size-len(withheld),
                                            initial_trial=proof)
    else:
        diagnostic = context
        if 'early_print_control' in proof:
            cp = dict(context['checkpoint'], native_regression_controls=proof['early_print_control'],
                      native_control_threads=2)
            diagnostic = dict(context, checkpoint=cp, control=_control_setup(cp))
        culprits = _isolate_retained_native(diagnostic, selection, native, folder, limit=size-len(withheld),
                                            initial_trial=proof)
    if not culprits or not set(culprits) <= {index for index, _ in selection}:
        return False
    if family is not None:
        context.setdefault('family_suspects', {}).update((index, family) for index in culprits)
    else:
        names = native['faces'] if 'faces' in native else [native['face']]
        witness = {'names': names, 'control': diagnostic['control']}
        context.setdefault('native_suspects', {}).update((index, witness) for index in culprits)
        if 'early_print_control' in proof:
            context = dict(context, early_print_recheck=proof['early_print_control'])
    return _retry_without(context, entries, whole_proof, sorted([*withheld, *culprits]), size,
                          route=route)


def _diagnosed_proposals(proof):
    if 'merge_conflict_at' in proof:
        conflicts = proof.get('merge_conflicts')
        return 'merge', ([item['proposal_index'] for item in conflicts]
                         if conflicts else [proof['merge_conflict_at']])
    for route, key in (('stale', 'stale_rule_conflict'), ('dependency', 'removed_rule_conflict')):
        if proof.get(key):
            return route, proof[key]['without_proposal_indices']
    if proof.get('print_shape_failed_at'):
        return 'print_shape', proof['print_shape_failed_at']
    return 'family', proof.get('new_families_failed_at', ())


def _retry_without(context, entries, whole_proof, failed_indices, size, *, route):
    """Use a bounded diagnosis to route fresh guarded trials."""
    manifest, folder = context['manifest'], context['folder']
    label = {'dependency': 'without-conflicting-removal', 'family': 'without-failed-families',
             'merge': 'without-merge-conflict', 'stale': 'without-stale-rule-reference',
             'print_shape': 'without-print-shape-regression',
             'retained_family': 'without-retained-family-regression',
             'retained_native': 'without-retained-native-regression'}[route]
    subgroup = context.get('subgroup_attempt')
    completed = {'group_attempt': whole_proof}
    if subgroup is not None:
        label = f'subgroup-{entries[0][0]:03d}-{label}'
        completed['subgroup_attempt'] = subgroup
    receipt = route+'_retry'
    first_attempt = True
    while 1 <= len(failed_indices) <= size:
        withheld = [entry for entry in entries if entry[0] in failed_indices]
        survivors = [entry for entry in entries if entry[0] not in failed_indices]
        if len(withheld) != len(failed_indices) or not survivors:
            return False
        suffix = (not first_attempt or (route == 'merge' and len(failed_indices) > 1)
                  or (folder/label).exists())
        attempt_label = f'{label}-{len(failed_indices):03d}' if suffix else label
        first_attempt = False
        items, proof = _stage_trial(context, survivors, attempt_label)
        survivor_proof = items[0]['trial_proof']
        record = {'without_proposal_indices': failed_indices, 'jobs': proof['jobs'],
                  'status': proof['status'], 'proof': survivor_proof, 'group_attempt': whole_proof}
        manifest[receipt] = record
        manifest.setdefault(receipt+'_attempts', []).append(record)
        if proof['status'] == 'passed':
            break
        r.write(folder/'manifest.json', manifest)
        _, next_failed = _diagnosed_proposals(proof)
        if next_failed and set(next_failed) <= {index for index, _ in survivors}:
            failed_indices = sorted({*failed_indices, *next_failed})
            continue
        return _retry_retained_control(context, entries, proof, whole_proof, size, failed_indices)
    else:
        return False
    completed[receipt] = survivor_proof
    for item in items:
        item.update(completed)
    manifest['jobs'].extend(items)
    r.write(folder/'manifest.json', manifest)
    for index, proposal in withheld:
        single, _ = _stage_trial(context, [(index, proposal)], f'proposal-{index:03d}')
        single[0].update(completed)
        manifest['jobs'].extend(single)
        r.write(folder/'manifest.json', manifest)
    return True


def _fallback_trial(context, entries, whole_proof, label):
    manifest, folder = context['manifest'], context['folder']
    items, proof = _stage_trial(context, entries, label)
    trial_proof = items[0]['trial_proof']
    manifest.setdefault('fallback_trials', []).append(
        {'jobs': proof['jobs'], 'status': proof['status'], 'proof': trial_proof,
         'group_attempt': whole_proof})
    passed = proof['status'] == 'passed'
    if passed:
        for item in items:
            item['group_attempt'] = whole_proof
        manifest['jobs'].extend(items)
    r.write(folder/'manifest.json', manifest)
    return passed, trial_proof, proof


def _subgroup_fallback(context, entries, whole_proof, size):
    """Check a mixed batch's mechanical prefix together before smaller trials."""
    manifest, folder = context['manifest'], context['folder']
    manifest['fallback_group_size'] = size
    r.write(folder/'manifest.json', manifest)
    prefix = next((offset for offset, (_, item) in enumerate(entries) if item.get('kind') != 'mechanical'),
                  len(entries))
    if size < prefix < len(entries):
        passed, _, _ = _fallback_trial(context, entries[:prefix], whole_proof, 'mechanical-prefix')
        if passed:
            entries = entries[prefix:]
    for offset in range(0, len(entries), size):
        group = entries[offset:offset+size]
        subgroup_proof = None
        if len(group) > 1:
            passed, subgroup_proof, proof = _fallback_trial(context, group, whole_proof, f'subgroup-{group[0][0]:03d}')
            if passed:
                continue
            if len(group) >= 3:
                scoped = dict(context, subgroup_attempt=subgroup_proof)
                route, failed = _diagnosed_proposals(proof)
                if failed:
                    passed = _retry_without(scoped, group, whole_proof, failed, 1, route=route)
                else:
                    passed = _retry_retained_control(scoped, group, proof, whole_proof, 1)
                if passed:
                    continue
        for index, proposal in group:
            single, _ = _stage_trial(context, [(index, proposal)], f'proposal-{index:03d}')
            single[0]['group_attempt'] = whole_proof
            if subgroup_proof is not None:
                single[0]['subgroup_attempt'] = subgroup_proof
            manifest['jobs'].extend(single)
            r.write(folder/'manifest.json', manifest)


def stage(campaign, checkpoint, proposals, *, combine_selected=False):
    if type(combine_selected) is not bool:
        raise ValueError('combine_selected must be boolean')
    with _workspace_lock(checkpoint):
        return _stage(campaign, checkpoint, proposals, combine_selected=combine_selected)


def _stage(campaign, checkpoint, proposals, *, combine_selected=False):
    """Create and commit one private candidate; return a reviewable manifest."""
    proposals = list(proposals)
    if not 1 <= len(proposals) <= 96:
        raise ValueError('a batch must contain one to ninety-six proposals')
    group_size = checkpoint.get('stage_group_size', 1)
    if type(group_size) is not int or not 1 <= group_size <= 4:
        raise ValueError('stage_group_size must be an integer from 1 to 4')
    family_threads = _family_check_threads(checkpoint)
    if type(combine_selected) is not bool:
        raise ValueError('combine_selected must be boolean')
    fallback_size = group_size
    if combine_selected:
        group_size = len(proposals)
    control = _control_setup(checkpoint)
    library_control = library_controls.setup(checkpoint, checkpoint['source'])
    campaign = Path(campaign).resolve()
    origin = Path(checkpoint['source']).resolve()
    if checkpoint.get('integration_workspace') and Path(checkpoint['integration_workspace']).resolve() == origin:
        raise ValueError('integration workspace must differ from checkpoint source')
    ident = uuid.uuid4().hex
    folder = campaign/'.cache/batches'/ident
    folder.mkdir(parents=True)
    started = time.time()
    source = Path(checkpoint['integration_workspace']).resolve() if checkpoint.get('integration_workspace') else folder/'source'
    branch = 'retire-batch-'+ident
    state = _prepare_workspace(origin, checkpoint, source, branch) if checkpoint.get('integration_workspace') else None
    if state is None:
        _git(origin, 'worktree', 'add', '-b', branch, str(source), checkpoint['commit'])
    manifest = {'id': ident, 'folder': str(folder), 'source': str(source), 'branch': branch, 'base_commit': checkpoint['commit'],
                'status': 'staging', 'started_unix': started, 'jobs': [], 'fixtures': [], 'rules': [],
                'family_check_threads': family_threads, 'stage_group_size': group_size,
                'combine_selected': combine_selected}
    registry_path = source/'tests/families.json'
    registry_hash = r.digest(registry_path)
    baseline_names = r.read(registry_path)
    if (not isinstance(baseline_names, list) or any(not isinstance(name, str) for name in baseline_names)
            or len(set(baseline_names)) != len(baseline_names)):
        raise ValueError('baseline family registry must contain unique paths')
    baseline = []
    for name in baseline_names:
        relative = _baseline_fixture_path(name)
        path = source/relative
        symlink = any((source/Path(*relative.parts[:index])).is_symlink()
                      for index in range(1, len(relative.parts)+1))
        if symlink or not path.is_file() or not path.resolve().is_relative_to(source):
            raise ValueError('baseline family is missing or escapes source')
        baseline.append((name, r.digest(path)))
    if _git(source, 'status', '--porcelain', '--', 'tests/families.json', *baseline_names):
        raise ValueError('baseline family registry or fixtures differ from checkpoint')
    tracked = set(_git(source, 'ls-files', '--', 'tests/families.json', *baseline_names).splitlines())
    if tracked != {'tests/families.json', *baseline_names}:
        raise ValueError('baseline family registry or fixtures differ from checkpoint')
    manifest['baseline_families'] = [{'path': name, 'sha256': digest} for name, digest in baseline]
    manifest['baseline_registry_sha256'] = registry_hash
    if control is not None:
        manifest['native_regression_controls'] = {'profile': str(control['profile']),
            'profile_sha256': control['profile_sha256'], 'checkpoint_manifest_sha256': control['manifest_sha256'],
            'names': control['names'], 'occurrences': control['occurrences'], 'threads': control['threads']}
    if library_control is not None:
        manifest['library_regression_controls'] = {key: str(library_control[key]) if key == 'profile' else library_control[key]
                                                  for key in ('profile', 'profile_sha256', 'binary_sha256', 'source_sha256', 'tests')}
    r.write(folder/'manifest.json', manifest)
    if state:
        r.write(state, {'folder': str(folder)})
    if control is not None:
        try:
            _control_baseline(campaign, checkpoint, control, source/'grammar', folder/'native-control-base')
        except (ValueError, OSError, subprocess.CalledProcessError, KeyError) as error:
            manifest.update(status='failed', reason=str(error), failure_scope='baseline-native-control')
            r.write(folder/'manifest.json', manifest)
            return manifest
    if library_control is not None:
        try:
            library_controls.check(checkpoint, library_control, source/'grammar', folder/'library-control-base')
            manifest['library_regression_controls']['base_result_sha256'] = r.digest(folder/'library-control-base/result.json')
            r.write(folder/'manifest.json', manifest)
        except (ValueError, OSError, subprocess.CalledProcessError, KeyError) as error:
            manifest.update(status='failed', reason=str(error), failure_scope='baseline-library-control')
            r.write(folder/'manifest.json', manifest)
            return manifest
    context = {'source': source, 'folder': folder, 'manifest': manifest, 'checkpoint': checkpoint,
               'control': control, 'library_control': library_control, 'gold_cache': _GoldIndexCache(),
               'baseline': baseline, 'baseline_names': baseline_names,
               'registry_path': registry_path, 'registry_hash': registry_hash,
               'early_default_print': combine_selected and checkpoint.get('early_default_print', False),
               'shape_baseline': r.read(source/shapes.BASELINE_PATH) if (source/shapes.BASELINE_PATH).is_file() else None}
    indexed = list(enumerate(proposals))
    try:
        for first in range(0, len(indexed), group_size):
            entries = indexed[first:first+group_size]
            label = f'proposal-{first:03d}' if len(entries) == 1 else f'group-{first:03d}'
            route_union = combine_selected and len(entries) > fallback_size
            retry_limit = max(fallback_size, min(MAX_ROUTED_EXCLUSIONS, len(entries)//6)) if route_union else 1
            items, proof = _stage_trial(context, entries, label, merge_conflict_limit=retry_limit)
            if len(entries) > 1:
                manifest.setdefault('group_trials', []).append({'jobs': proof['jobs'], 'status': proof['status'],
                                                                 'proof': items[0]['trial_proof']})
                if proof['status'] != 'passed':
                    r.write(folder/'manifest.json', manifest)
            if proof['status'] == 'passed' or len(entries) == 1:
                manifest['jobs'].extend(items)
                r.write(folder/'manifest.json', manifest)
                continue
            group_proof = items[0]['trial_proof']
            if route_union:
                route, failed = _diagnosed_proposals(proof)
                if failed:
                    passed = _retry_without(context, entries, group_proof, failed, retry_limit, route=route)
                else:
                    passed = _retry_retained_control(context, entries, proof, group_proof, retry_limit)
                if not passed:
                    _subgroup_fallback(context, entries, group_proof, fallback_size)
                continue
            for index, proposal in entries:
                single, _ = _stage_trial(context, [(index, proposal)], f'proposal-{index:03d}')
                single[0]['group_attempt'] = group_proof
                manifest['jobs'].extend(single)
                r.write(folder/'manifest.json', manifest)
    except (_UncertainPromotion, OSError) as error:
        manifest.update(status='failed', reason=str(error), failure_scope='combined-batch')
        try:
            if _git(source, 'status', '--porcelain'):
                _git(source, 'add', '-A', '--', '.')
                _git(source, '-c', 'user.name=Retirement Coordinator', '-c', 'user.email=retirement@localhost',
                     'commit', '-m', 'Preserve failed retirement batch')
                manifest['failed_candidate_commit'] = _git(source, 'rev-parse', 'HEAD')
        except (OSError, subprocess.CalledProcessError) as snapshot_error:
            manifest['snapshot_error'] = str(snapshot_error)
        r.write(folder/'manifest.json', manifest)
        return manifest
    if not any(job['status'] == 'included' for job in manifest['jobs']):
        try:
            if control is not None:
                _control_verify(control)
            if library_control is not None:
                library_controls.verify(library_control)
            manifest['status'] = 'no-proposals'
        except (ValueError, OSError) as error:
            manifest.update(status='failed', reason=str(error), failure_scope='native-control-input')
    else:
        try:
            if control is not None:
                _control_verify(control)
            if library_control is not None:
                library_controls.verify(library_control)
            if (r.digest(registry_path) != registry_hash
                    or any(r.digest(source/name) != digest for name, digest in baseline)):
                raise ValueError('baseline family changed during staging')
            registry = source/'tests/families.json'
            families = r.read(registry)
            if any(f in families for f in manifest['fixtures']):
                raise ValueError('fixture already registered')
            if manifest['fixtures']:
                r.write(registry, families + manifest['fixtures'])
            manifest['grammar'] = r.tree(source/'grammar')
            shape_baseline = [shapes.BASELINE_PATH] if (source/shapes.BASELINE_PATH).is_file() and shapes.prune_fixed(source) else []
            _git(source, 'add', '--', 'grammar', 'tests/families.json', *manifest['fixtures'], *shape_baseline)
            _git(source, '-c', 'user.name=Retirement Coordinator', '-c', 'user.email=retirement@localhost',
                 'commit', '-m', 'Stage provisional retirement batch')
            manifest['commit'] = _git(source, 'rev-parse', 'HEAD')
            manifest['status'] = 'provisional'
        except (ValueError, OSError, subprocess.CalledProcessError) as error:
            manifest.update(status='failed', reason=str(error), failure_scope='combined-batch')
            # Preserve a failed partial candidate before reusing the owned workspace.
            # The branch and commit are evidence, never a provisional acceptance.
            try:
                if _git(source, 'status', '--porcelain'):
                    _git(source, 'add', '-A', '--', '.')
                    _git(source, '-c', 'user.name=Retirement Coordinator', '-c', 'user.email=retirement@localhost',
                         'commit', '-m', 'Preserve failed retirement batch')
                    manifest['failed_candidate_commit'] = _git(source, 'rev-parse', 'HEAD')
            except (OSError, subprocess.CalledProcessError) as snapshot_error:
                manifest['snapshot_error'] = str(snapshot_error)
    r.write(folder/'manifest.json', manifest)
    return manifest


def _screen_identity(source, manifest, baseline, input_run, output, evidence):
    if (r.digest(baseline) != evidence['baseline_sha256']
            or r.digest(input_run/'manifest.json') != evidence['checkpoint_manifest_sha256']
            or r.read(input_run/'manifest.json')['files'] != r.tree(input_run/'frozen')
            or r.digest(output/'manifest.json') != evidence['validation_manifest_sha256']
            or r.read(output/'manifest.json')['files'] != r.tree(output/'frozen')
            or _git(source, 'rev-parse', 'HEAD') != manifest['commit']
            or _git(source, 'status', '--porcelain')
            or r.tree(source/'grammar') != manifest['grammar']):
        raise ValueError('candidate or frozen inputs changed during parse screen')


def validate(manifest, checkpoint, slots=12, *, scan=False):
    with _workspace_lock(checkpoint):
        return _validate(manifest, checkpoint, slots, scan=scan)


def _parse_screen_threads(checkpoint):
    threads = checkpoint.get('parse_screen_threads', DEFAULT_PARSE_SCREEN_THREADS)
    capacity = checkpoint.get('cpu_capacity', 32)
    if (type(capacity) is not int or not 1 <= capacity <= 32
            or type(threads) is not int or not 1 <= threads <= min(16, capacity)):
        raise ValueError('parse_screen_threads must be an integer from 1 to min(16, cpu_capacity)')
    return threads


@contextmanager
def _audit_cpu(checkpoint, threads, capacity, *, minimum_threads=None):
    pool = checkpoint.get('cpu_pool')
    lease = (retire_cpu.reserve(Path(pool), threads, capacity, minimum_threads=minimum_threads,
                                background=checkpoint.get('background_cpu', False)) if pool else nullcontext(()))
    with lease as fds:
        r._LEASES.extend(fds)
        try:
            yield len(fds) if pool else threads
        finally:
            for fd in fds:
                r._LEASES.remove(fd)


def _audit_parse_screen(checkpoint, manifest, output, data, evidence):
    source = Path(manifest['source']).resolve()
    baseline = Path(checkpoint['baseline']).resolve()
    input_run = Path(checkpoint['inputs']).resolve()
    folder = Path(manifest['folder']).resolve()/'parse-screen'
    threads = _parse_screen_threads(checkpoint)
    result = {'parse_screen_threads': threads, 'validation_manifest_sha256': r.digest(output/'manifest.json')}
    identity = {**evidence, **result}
    _screen_identity(source, manifest, baseline, input_run, output, identity)
    try:
        with _audit_cpu(checkpoint, threads, checkpoint.get('cpu_capacity', 32)):
            result['parse_screen'] = retire_parse_screen.run(output, folder, source/'grammar', data, threads)
    finally:
        _screen_identity(source, manifest, baseline, input_run, output, identity)
    result['parse_screen_result_sha256'] = r.digest(folder/'result.json')
    return result


def _run_repository_driver(source, folder, threads, descriptors, evidence):
    # Families and library tests run concurrently inside the repository driver.
    gate_threads = max(2, threads//2-2)
    environment = {key: value for key, value in os.environ.items() if not key.startswith('ORACLE_GRAMMAR_')}
    environment.update(CARGO_BUILD_JOBS=str(max(4, threads-4)), RAYON_NUM_THREADS=str(gate_threads),
                       ORACLE_GRAMMAR_THREADS=str(gate_threads), RUST_TEST_THREADS=str(gate_threads))
    driver_sha256 = r.digest(REPOSITORY_DRIVER)
    evidence.update(repository_driver_sha256=driver_sha256, repository_grammar=str(source/'grammar'))
    argv = [sys.executable, str(REPOSITORY_DRIVER), str(source), '--lease-fds', ','.join(map(str, descriptors))]
    with (folder/'repository-check.log').open('w') as log:
        code = subprocess.run(argv, cwd=source, env=environment, stdout=log, stderr=log, check=False,
                              pass_fds=descriptors).returncode
    evidence['repository_check_returncode'] = code
    if r.digest(REPOSITORY_DRIVER) != driver_sha256:
        raise ValueError('repository gate driver changed during validation')
    if code:
        raise ValueError('repository gates failed')
    return gate_threads


def _parallel_repository_driver(checkpoint, source, folder, threads, inherited, evidence):
    # The background thread owns separate descriptors and never changes r._LEASES.
    # Only the foreground native audit publishes its lease in that process-local list.
    with retire_cpu.reserve(Path(checkpoint['cpu_pool']), threads, checkpoint.get('cpu_capacity', 32),
                            background=checkpoint.get('background_cpu', False)) as descriptors:
        _run_repository_driver(source, folder, threads, inherited+descriptors, evidence)


def _validate(manifest, checkpoint, slots, *, scan=False):
    """Freeze and run full allowed-population and repository gates for a candidate."""
    if manifest.get('status') != 'provisional':
        raise ValueError('only a provisional candidate can be validated')
    enabled = checkpoint.get('full_parse_screen', False)
    if type(enabled) is not bool:
        raise ValueError('full_parse_screen must be boolean')
    parse_first = checkpoint.get('parse_screen_before_repository', False)
    if type(parse_first) is not bool or (parse_first and not enabled):
        raise ValueError('parse_screen_before_repository requires an enabled full parse screen')
    _parse_screen_threads(checkpoint)
    threads = checkpoint.get('audit_threads', 24)
    capacity = checkpoint.get('cpu_capacity', 32)
    if (type(threads) is not int or not 16 <= threads <= 24
            or type(capacity) is not int or not threads <= capacity <= 32):
        raise ValueError('audit threads must be 16–24 and within CPU capacity')
    repository_threads = checkpoint.get('repository_check_threads', threads)
    if type(repository_threads) is not int or not 8 <= repository_threads <= threads:
        raise ValueError('repository_check_threads must be between 8 and audit_threads')
    overlap = checkpoint.get('parallel_repository_checks', False)
    if type(overlap) is not bool:
        raise ValueError('parallel_repository_checks must be boolean')
    candidate_spec = checkpoint.get('candidate_validation_helpers')
    overlap = (overlap and parse_first and checkpoint.get('cpu_pool') and candidate_spec is None
               and threads+repository_threads <= capacity)
    source = Path(manifest['source']).resolve()
    if (_git(source, 'rev-parse', 'HEAD') != manifest['commit']
            or _git(source, 'status', '--porcelain')
            or r.tree(source/'grammar') != manifest['grammar']):
        raise ValueError('candidate source or grammar changed')
    folder = Path(manifest['folder']).resolve()
    baseline = Path(checkpoint['baseline']).resolve()
    input_run = Path(checkpoint['inputs']).resolve()
    inputs = input_run/'frozen'
    if r.read(input_run/'manifest.json')['files'] != r.tree(inputs):
        raise ValueError('checkpoint frozen inputs changed')
    output = folder/'validation-inputs'
    evidence = {'status': 'running', 'candidate': manifest['commit'],
                'scope': 'combined-batch',
                'included_jobs': [job['job_id'] for job in manifest['jobs'] if job['status'] == 'included'],
                'baseline_sha256': r.digest(baseline),
                'checkpoint_manifest_sha256': r.digest(input_run/'manifest.json'), 'inputs': str(output),
                'source_checkpoint': checkpoint['commit'], 'audit_threads': threads,
                'cpu_capacity': capacity, 'repository_check_threads': repository_threads}
    executor = pending = None
    repository_evidence = {}
    try:
        with _audit_cpu(checkpoint, repository_threads, capacity):
            selected_helpers = Path(checkpoint['helpers']).resolve()
            if candidate_spec is not None:
                selected_helpers, candidate_build = _candidate_helper_snapshot(candidate_spec, checkpoint, source, folder)
                evidence['candidate_helper_build_sha256'] = candidate_spec['build_sha256']
                evidence['candidate_helper_profile'] = candidate_build['profile']
            r.freeze(Namespace(run=output, source=source, helpers=selected_helpers,
                               slots=1, threads=threads, allowed=inputs/'allowed.json', gold=inputs/'gold.jsonl',
                               oracle=inputs/'oracle.json', parse=baseline, prints=[baseline]))
        data = w.payload(output) if parse_first else None
        screened = None
        if parse_first:
            screened = _audit_parse_screen(checkpoint, manifest, output, data, evidence)
            if screened['parse_screen']['failures']:
                evidence.update(screened, repository_check_skipped=retire_parse_screen.REPOSITORY_SKIPPED)
                raise ValueError('full allowed-population parse screen failed')
        if overlap:
            evidence['parallel_repository_checks'] = True
            executor = ThreadPoolExecutor(max_workers=1)
            pending = executor.submit(_parallel_repository_driver, checkpoint, source, folder, repository_threads,
                                      tuple(r._LEASES), repository_evidence)
        else:
            with _audit_cpu(checkpoint, repository_threads, capacity):
                gate_threads = _run_repository_driver(source, folder, repository_threads, tuple(r._LEASES), evidence)
                if candidate_spec is not None:
                    evidence['optimized_family_log'] = str(folder/'optimized-family-check.log')
                    family_log = _optimized_families(source, selected_helpers, folder, gate_threads)
                    evidence['optimized_family_log_sha256'] = r.digest(family_log)
        if data is None:
            data = w.payload(output)
        if enabled:
            evidence.update(screened if screened is not None else
                            _audit_parse_screen(checkpoint, manifest, output, data, evidence))
            if evidence['parse_screen']['failures']:
                raise ValueError('full allowed-population parse screen failed')
            if screened is not None:
                _screen_identity(source, manifest, baseline, input_run, output, evidence)
                if r.digest(folder/'parse-screen/result.json') != evidence['parse_screen_result_sha256']:
                    raise ValueError('early parse screen changed during repository checks')
        checks = folder/'full-checks'
        checks.mkdir()
        with ExitStack() as audit_lease:
            audit_lease.enter_context(_audit_cpu(checkpoint, threads, capacity))
            try:
                report = w.checks(output, checks, source/'grammar', [e['name'] for e in data[0].values()],
                                  data, threads)
            except subprocess.CalledProcessError as error:
                report = retire_native_outcome.preservation_failure(error, output, checks, data)
            evidence['full_checks'] = report
            released = False
            if pending is not None:
                if not pending.done():
                    # A different foreground waiter could need more than the
                    # remaining tokens. Release the audit before joining.
                    audit_lease.close()
                    released = True
                pending.result()
                evidence.update(repository_evidence)
                _screen_identity(source, manifest, baseline, input_run, output, evidence)
                if r.digest(folder/'parse-screen/result.json') != evidence['parse_screen_result_sha256']:
                    raise ValueError('early parse screen changed during concurrent validation')
            if report['failures']:
                raise ValueError('full allowed-population checks failed')
            if (r.digest(baseline) != evidence['baseline_sha256']
                    or r.read(input_run/'manifest.json')['files'] != r.tree(inputs)):
                raise ValueError('checkpoint inputs changed during validation')
            if _git(source, 'rev-parse', 'HEAD') != manifest['commit'] or _git(source, 'status', '--porcelain'):
                raise ValueError('candidate changed during validation')
            if candidate_spec is not None:
                build = _verified_helper_build(selected_helpers, candidate_spec['build_sha256'])
                if (dev.source(source) != build['source_sha256']
                        or r.digest(output/'frozen/build.json') != candidate_spec['build_sha256']
                        or any(r.digest(output/'frozen/bin'/name) != build['binaries'][name] for name in r.HELPERS)
                        or r.read(output/'manifest.json')['files'] != r.tree(output/'frozen')):
                    raise ValueError('candidate frozen helper identity changed')
                evidence['helpers'] = str(selected_helpers)
            evidence['rows'] = str(checks/'rows.jsonl')
            evidence['rows_sha256'] = r.digest(checks/'rows.jsonl')
            evidence['status'] = 'passed'
            if scan:
                # The scan uses the frozen audit thread count. Keep its CPU
                # reservation across publication, or reacquire it after a join.
                if released:
                    audit_lease.enter_context(_audit_cpu(checkpoint, threads, capacity))
                r.write(folder/'validation.json', evidence)
                r.scan(Namespace(run=output, rules=None))
                return evidence
    except (ValueError, OSError, subprocess.CalledProcessError, KeyError) as error:
        if evidence['status'] == 'passed':
            raise
        evidence.update(status='failed', reason=str(error))
    finally:
        if executor is not None:
            executor.shutdown(wait=True)
            evidence.update(repository_evidence)
    r.write(folder/'validation.json', evidence)
    return evidence
