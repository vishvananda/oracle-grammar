"""Private provisional integration and validation of retirement proposals."""
from argparse import Namespace
from collections import OrderedDict
import copy
from contextlib import contextmanager, nullcontext
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile
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

REPOSITORY_DRIVER = Path(__file__).with_name('retire_repository_checks.py')

FAMILY_CHECK_THREADS = 6
DEFAULT_NATIVE_CONTROL_THREADS = 6
MAX_ROUTED_EXCLUSIONS = 8
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
    """Verify each shared frozen checkpoint once within an ordered replay."""

    def __init__(self):
        self.snapshots = {}
        self.locations = {}

    @staticmethod
    def identity(root):
        files = []
        frozen = root/'frozen'
        if frozen.is_symlink() or not frozen.is_dir():
            raise ValueError('proposal frozen inputs changed')
        for path in sorted(frozen.rglob('*')):
            meta = path.stat(follow_symlinks=False)
            if stat.S_ISREG(meta.st_mode):
                files.append((path.relative_to(frozen), meta.st_dev, meta.st_ino, meta.st_size,
                              meta.st_ctime_ns, meta.st_mtime_ns))
            elif not stat.S_ISDIR(meta.st_mode):
                raise ValueError('proposal frozen inputs contain a nonregular path')
        return r.digest(root/'manifest.json'), files

    def read(self, directory):
        root = directory.resolve()
        if self.locations.setdefault(directory, root) != root:
            raise ValueError('proposal frozen input location changed')
        identity = self.identity(root)
        if root not in self.snapshots:
            manifest = r.read(root/'manifest.json')
            if manifest['files'] != r.tree(root/'frozen') or self.identity(root) != identity:
                raise ValueError('proposal frozen inputs changed')
            self.snapshots[root] = identity, manifest
        previous, manifest = self.snapshots[root]
        if identity != previous:
            raise ValueError('proposal frozen inputs changed')
        return manifest

    def unchanged(self):
        return (all(directory.resolve() == root for directory, root in self.locations.items())
                and all(self.identity(root) == original for root, (original, _) in self.snapshots.items()))


def _proposal(proposal, *, input_cache=None):
    run = Path(proposal['run']).resolve()
    report = proposal['report']
    if not report.get('passed') or report.get('failures'):
        raise ValueError('coordinator report did not pass')
    task = report['task']
    if Path(task).name != task:
        raise ValueError('invalid task id')
    descriptor = run/'tasks'/f'{task}.json'
    support = r.read(run/'support.json')
    if (r.digest(run/'support.json') != report['support_sha256']
            or r.tree(run/'support') != support['files']
            or r.digest(descriptor) != report['descriptor_sha256']
            or support['tasks'][task] != report['descriptor_sha256']):
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
        originals = (index.get(edit['face'], []) if index is not None else
                     [card for card in original if any(f['name'] == edit['face'] for f in card['faces'])])
        if len(originals) != 1:
            raise ValueError('edited operand lacks original gold')
        expected = copy.deepcopy(originals[0])
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


def _helper(checkpoint, source, fixture, log, grammar_dir=None, *, threads=FAMILY_CHECK_THREADS):
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
    for name, digest in control['input_sha256'].items():
        if r.digest(run/'frozen'/name) != digest:
            raise ValueError('native control frozen input changed: '+name)


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
                report = w.checks(control['run'], directory, grammar, control['names'], control['data'], threads)
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


def _control_baseline(campaign, checkpoint, control, grammar, directory):
    _control_verify(control)
    frozen = r.read(control['run']/'manifest.json')['files']
    grammar_hashes = r.tree(grammar)
    if (grammar_hashes != {name.removeprefix('grammar/'): sha for name, sha in frozen.items()
                           if name.startswith('grammar/')}
            or any(path.is_symlink() for path in grammar.rglob('*'))):
        return _control_check(checkpoint, control, grammar, directory)
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


def _mechanical_removal(run, directory, rules, scratch):
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
    before = r.tree(base)
    with tempfile.TemporaryDirectory(prefix='canonical-removal-', dir=scratch) as temporary:
        expected = Path(temporary)
        shutil.copytree(base, expected/'grammar')
        removed, kept = mined.remove(expected, rules)
        if set(removed) != set(rules) or kept:
            raise ValueError('mechanical canonical removal has retained users')
        if r.tree(candidate) != r.tree(expected/'grammar'):
            raise ValueError('mechanical proposal differs from complete canonical removal')
        if r.tree(base) != before:
            raise _UncertainPromotion('mechanical frozen base changed during verification')


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


def _merge_trial_proposal(trial, scratch, proposal, mechanical):
    run, task, directory, hooks = proposal
    if mechanical:
        if hooks:
            raise ValueError('mechanical proposal cannot request hooks')
        _mechanical_removal(run, directory, task['rules'], scratch)
    else:
        changes = merger.merge_grammar(run/'inputs/frozen', trial, directory, task['rules'], hooks)
        for name, text in changes.items():
            target = trial/name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(text)
    removed, kept = mined.remove(trial, task['rules'])
    if set(removed) != set(task['rules']) or kept:
        raise ValueError('assigned removals have retained users')
    a.apply_hooks(trial/'grammar', hooks)


def _retained_family_failure(failures, proof, source, logs):
    checks = proof.get('family_checks', [])
    if len(failures) != 1 or not 0 <= failures[0] < len(checks):
        return None
    check = checks[failures[0]]
    if check['kind'] not in ('baseline', 'prior-proposal'):
        return None
    fixture = check['fixture']
    try:
        failures = r.read(logs/check['log'])['failures']
        digest = r.digest(source/fixture)
    except (ValueError, OSError, KeyError, TypeError):
        return None
    if not isinstance(failures, list) or not failures:
        return None
    return {'fixture': fixture, 'sha256': digest, 'failures': failures}


def _stage_trial(context, entries, label):
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
    input_cache = _ProposalInputCache()
    try:
        with tempfile.TemporaryDirectory(dir=folder) as temporary:
            trial = Path(temporary)
            shutil.copytree(source/'grammar', trial/'grammar')
            additions, prepared = [], []
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
                    _merge_trial_proposal(trial, folder, (run, task, directory, hooks), mechanical)
                except merger.MergeConflict:
                    proof['merge_conflict_at'] = index
                    raise
                if fixture:
                    filename = 'family.json' if len(entries) == 1 else f'family-{offset:03d}.json'
                    temporary_family = trial/filename
                    r.write(temporary_family, family)
                    additions.append((temporary_family, fixture))
                prepared.append((task, fixture, item, metadata))
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
            for item in items:
                item.update(family_logs=str(logs), family_checks=family_checks)
            if fixtures:
                _helper(checkpoint, source, fixtures, logs/'family.log', grammar_dir=trial/'grammar')
            if control is not None:
                for item in items:
                    item['native_control'] = str(logs/'native-control')
                _control_check(checkpoint, control, trial/'grammar', logs/'native-control')
            if context['library_control'] is not None:
                for item in items:
                    item['library_control'] = str(logs/'library-control')
                library_controls.check(checkpoint, context['library_control'], trial/'grammar', logs/'library-control')
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
        proof.update(status='failed', reason=str(error))
        if isinstance(error, _FamilyCheckFailure) and error.failed_fixtures:
            checks = proof.get('family_checks', [])
            if all(0 <= index < len(checks) and checks[index]['kind'] == 'new-proposal'
                   for index in error.failed_fixtures):
                failed = {checks[index]['fixture'] for index in error.failed_fixtures}
                proof['new_families_failed_at'] = [
                    index for (index, _), (_, fixture, _, _) in zip(entries, prepared, strict=True)
                    if fixture and str(fixture) in failed]
            else:
                evidence = _retained_family_failure(error.failed_fixtures, proof, source, logs)
                if evidence is not None:
                    proof['retained_family_failure'] = evidence
        elif isinstance(error, _FamilyCheckFailure) and len(entries) > 1:
            conflict = _rule_reference_conflict(source, entries, logs/'family.log')
            if conflict is not None:
                key = 'stale_rule_conflict' if conflict.get('stale_reference') else 'removed_rule_conflict'
                proof[key] = conflict
        elif isinstance(error, _NativeControlFailure) and len(entries) > 1:
            failure = _retained_native_failure(control, error.report)
            if failure is not None:
                proof['retained_native_failure'] = failure
        for item in items:
            item.update(status='integration-rejected', reason=str(error))
    try:
        for name, path in (('family_log_sha256', logs/'family.log'),
                           ('native_result_sha256', logs/'native-control/result.json'),
                           ('library_result_sha256', logs/'library-control/result.json')):
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
                              item.get('kind') == 'mechanical')
    return grammar


def _probe_retained_family(context, entries, failure, logs, input_cache):
    """Check one witnessed family on a private ordered subset."""
    logs.mkdir()
    source, folder = context['source'], context['folder']
    known = dict(context['baseline']) | {
        name: r.digest(source/name) for name in context['manifest']['fixtures']}
    fixture = source/failure['fixture']
    if known.get(failure['fixture']) != failure['sha256'] or r.digest(fixture) != failure['sha256']:
        raise _UncertainPromotion('retained family changed before focused replay')
    proof = {'proposal_indices': [index for index, _ in entries], 'fixture': failure['fixture'],
             'fixture_sha256': failure['sha256']}
    prefix = r.tree(source/'grammar')
    try:
        with tempfile.TemporaryDirectory(dir=folder) as temporary:
            grammar = _replay_focused_grammar(context, entries, Path(temporary), input_cache)
            tree = r.tree(grammar)
            proof['trial_grammar'] = tree
            try:
                _helper(context['checkpoint'], source, fixture, logs/'family.log', grammar_dir=grammar, threads=2)
                outcome = 'passed'
            except _FamilyCheckFailure as error:
                observed = r.read(logs/'family-000.log')['failures'] if error.failed_fixtures == (0,) else None
                outcome = 'reproduced' if observed == failure['failures'] else 'inconclusive'
            if not input_cache.unchanged() or r.tree(grammar) != tree:
                raise _UncertainPromotion('retained family inputs changed during focused replay')
    except (ValueError, OSError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        outcome = 'inconclusive'
        proof['reason'] = str(error)
    try:
        current = (r.tree(source/'grammar'), r.digest(fixture),
                   r.digest(context['registry_path']))
        if current != (prefix, failure['sha256'], context['registry_hash']):
            raise _UncertainPromotion('retained family prefix changed during focused replay')
    except OSError as error:
        raise _UncertainPromotion('retained family prefix unavailable after focused replay') from error
    proof['outcome'] = outcome
    r.write(logs/'probe.json', proof)
    return outcome


def _isolate_retained(entries, test, folder, *, verify_full=False):
    """Find one independently reproducible regression for a fully gated retry."""
    folder.mkdir()
    counter = 0
    input_cache = _ProposalInputCache()

    def probe(group):
        nonlocal counter
        logs = folder/f'probe-{counter:02d}'
        counter += 1
        return test(group, logs, input_cache)

    if probe([]) != 'passed' or (verify_full and probe(entries) != 'reproduced'):
        return None
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


def _isolate_retained_family(context, entries, failure, folder):
    def test(group, logs, input_cache):
        return _probe_retained_family(context, group, failure, logs, input_cache)

    return _isolate_retained(entries, test, folder)


def _retained_native_failure(control, report):
    """Only route a complete, localized native rejection."""
    failures = report.get('failures')
    if (type(report.get('faces')) is not int or report['faces'] != control['occurrences']
            or not isinstance(failures, list) or not 1 <= len(failures) <= 3):
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
    if len(faces) == 1:
        return {'face': next(iter(faces)), 'failures': failures}
    return None


def _probe_retained_native(context, entries, control, failures, logs, input_cache):
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
                expected = {'faces': control['occurrences'], 'failures': failures}
                outcome = 'reproduced' if error.report == expected else 'inconclusive'
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


def _isolate_retained_native(context, entries, failure, folder):
    control = context['control']
    _control_verify(control)
    try:
        closure = tasks.close_cohort(control['run'], {'names': [failure['face']]}, max_occurrences=16)
    except tasks.CohortClosureLimit:
        return None
    if not set(closure['names']) <= set(control['names']):
        return None
    focused = {**control, 'names': closure['names'], 'occurrences': closure['closure_occurrence_count'],
               'threads': min(2, control['threads'])}

    def test(group, logs, input_cache):
        return _probe_retained_native(context, group, focused, failure['failures'], logs, input_cache)

    return _isolate_retained(entries, test, folder, verify_full=True)


def _retry_retained_control(context, entries, proof, whole_proof, size, withheld=()):
    family = proof.get('retained_family_failure')
    native = proof.get('retained_native_failure')
    if (family is None and native is None) or len(withheld) >= size:
        return False
    selection = [entry for entry in entries if entry[0] not in withheld]
    if len(selection) < 2:
        return False
    if family is not None:
        route, isolate, failure = 'retained_family', _isolate_retained_family, family
    else:
        route, isolate, failure = 'retained_native', _isolate_retained_native, native
    stem = f"{route.replace('_', '-')}-{entries[0][0]:03d}"
    folder = context['folder']/stem
    if folder.exists():
        folder = folder.with_name(f'{stem}-{len(withheld):03d}')
    culprit = isolate(context, selection, failure, folder)
    if culprit not in {index for index, _ in selection}:
        return False
    return _retry_without(context, entries, whole_proof, sorted([*withheld, culprit]), size,
                          route=route)


def _diagnosed_proposals(proof):
    if 'merge_conflict_at' in proof:
        return 'merge', [proof['merge_conflict_at']]
    for route, key in (('stale', 'stale_rule_conflict'), ('dependency', 'removed_rule_conflict')):
        if proof.get(key):
            return route, proof[key]['without_proposal_indices']
    return 'family', proof.get('new_families_failed_at', ())


def _retry_without(context, entries, whole_proof, failed_indices, size, *, route):
    """Use a bounded diagnosis to route fresh guarded trials."""
    manifest, folder = context['manifest'], context['folder']
    label = {'dependency': 'without-conflicting-removal', 'family': 'without-failed-families',
             'merge': 'without-merge-conflict', 'stale': 'without-stale-rule-reference',
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
    source = Path(checkpoint['integration_workspace']).resolve() if checkpoint.get('integration_workspace') else folder/'source'
    branch = 'retire-batch-'+ident
    state = _prepare_workspace(origin, checkpoint, source, branch) if checkpoint.get('integration_workspace') else None
    if state is None:
        _git(origin, 'worktree', 'add', '-b', branch, str(source), checkpoint['commit'])
    manifest = {'id': ident, 'folder': str(folder), 'source': str(source), 'branch': branch, 'base_commit': checkpoint['commit'],
                'status': 'staging', 'jobs': [], 'fixtures': [], 'rules': [],
                'family_check_threads': FAMILY_CHECK_THREADS, 'stage_group_size': group_size,
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
               'registry_path': registry_path, 'registry_hash': registry_hash}
    indexed = list(enumerate(proposals))
    try:
        for first in range(0, len(indexed), group_size):
            entries = indexed[first:first+group_size]
            label = f'proposal-{first:03d}' if len(entries) == 1 else f'group-{first:03d}'
            items, proof = _stage_trial(context, entries, label)
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
            if combine_selected and len(entries) > fallback_size:
                route, failed = _diagnosed_proposals(proof)
                retry_limit = max(fallback_size, min(MAX_ROUTED_EXCLUSIONS, len(entries)//8))
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
            _git(source, 'add', '--', 'grammar', 'tests/families.json', *manifest['fixtures'])
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


def validate(manifest, checkpoint, slots=12):
    with _workspace_lock(checkpoint):
        return _validate(manifest, checkpoint, slots)


@contextmanager
def _audit_cpu(checkpoint, threads, capacity):
    lease = (retire_cpu.reserve(Path(checkpoint['cpu_pool']), threads, capacity,
                                background=checkpoint.get('background_cpu', False))
             if checkpoint.get('cpu_pool') else nullcontext(()))
    with lease as fds:
        r._LEASES.extend(fds)
        try:
            yield
        finally:
            for fd in fds:
                r._LEASES.remove(fd)


def _validate(manifest, checkpoint, slots):
    """Freeze and run full allowed-population and repository gates for a candidate."""
    if manifest.get('status') != 'provisional':
        raise ValueError('only a provisional candidate can be validated')
    enabled = checkpoint.get('full_parse_screen', False)
    if type(enabled) is not bool:
        raise ValueError('full_parse_screen must be boolean')
    threads = checkpoint.get('audit_threads', 24)
    capacity = checkpoint.get('cpu_capacity', 32)
    if (type(threads) is not int or not 16 <= threads <= 24
            or type(capacity) is not int or not threads <= capacity <= 32):
        raise ValueError('audit threads must be 16–24 and within CPU capacity')
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
                'cpu_capacity': capacity}
    try:
        with _audit_cpu(checkpoint, threads, capacity):
            selected_helpers = Path(checkpoint['helpers']).resolve()
            candidate_spec = checkpoint.get('candidate_validation_helpers')
            if candidate_spec is not None:
                selected_helpers, candidate_build = _candidate_helper_snapshot(candidate_spec, checkpoint, source, folder)
                evidence['candidate_helper_build_sha256'] = candidate_spec['build_sha256']
                evidence['candidate_helper_profile'] = candidate_build['profile']
            r.freeze(Namespace(run=output, source=source, helpers=selected_helpers,
                               slots=1, threads=threads, allowed=inputs/'allowed.json', gold=inputs/'gold.jsonl',
                               oracle=inputs/'oracle.json', parse=baseline, prints=[baseline]))
            # dev.py runs families and library tests concurrently. Divide the lease
            # between those two gates, leaving headroom for the fast gates.
            gate_threads = str(max(2, threads//2-2))
            gate_env = {key: value for key, value in os.environ.items() if not key.startswith('ORACLE_GRAMMAR_')}
            gate_env.update(CARGO_BUILD_JOBS=str(max(4, threads-4)), RAYON_NUM_THREADS=gate_threads,
                            ORACLE_GRAMMAR_THREADS=gate_threads, RUST_TEST_THREADS=gate_threads)
            driver_sha256 = r.digest(REPOSITORY_DRIVER)
            evidence.update(repository_driver_sha256=driver_sha256, repository_grammar=str(source/'grammar'))
            with (folder/'repository-check.log').open('w') as log:
                code = subprocess.run([sys.executable, str(REPOSITORY_DRIVER), str(source),
                                       '--lease-fds', ','.join(map(str, r._LEASES))], cwd=source, env=gate_env,
                                      stdout=log, stderr=log, check=False, pass_fds=tuple(r._LEASES)).returncode
            evidence['repository_check_returncode'] = code
            if r.digest(REPOSITORY_DRIVER) != driver_sha256:
                raise ValueError('repository gate driver changed during validation')
            if code:
                raise ValueError('repository gates failed')
            if candidate_spec is not None:
                evidence['optimized_family_log'] = str(folder/'optimized-family-check.log')
                family_log = _optimized_families(source, selected_helpers, folder, int(gate_threads))
                evidence['optimized_family_log_sha256'] = r.digest(family_log)
        data = w.payload(output)
        if enabled:
            evidence['validation_manifest_sha256'] = r.digest(output/'manifest.json')
            _screen_identity(source, manifest, baseline, input_run, output, evidence)
            screen_threads = min(6, threads)
            try:
                with _audit_cpu(checkpoint, screen_threads, capacity):
                    screen = retire_parse_screen.run(output, folder/'parse-screen', source/'grammar',
                                                      data, screen_threads)
            finally:
                _screen_identity(source, manifest, baseline, input_run, output, evidence)
            evidence['parse_screen'] = screen
            evidence['parse_screen_result_sha256'] = r.digest(folder/'parse-screen/result.json')
            if screen['failures']:
                raise ValueError('full allowed-population parse screen failed')
        checks = folder/'full-checks'
        checks.mkdir()
        try:
            with _audit_cpu(checkpoint, threads, capacity):
                report = w.checks(output, checks, source/'grammar', [e['name'] for e in data[0].values()],
                                  data, threads)
        except subprocess.CalledProcessError as error:
            report = retire_native_outcome.preservation_failure(error, output, checks, data)
        evidence['full_checks'] = report
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
    except (ValueError, OSError, subprocess.CalledProcessError, KeyError) as error:
        evidence.update(status='failed', reason=str(error))
    r.write(folder/'validation.json', evidence)
    return evidence
