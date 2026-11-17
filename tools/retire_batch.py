"""Private provisional integration and validation of retirement proposals."""
from argparse import Namespace
import copy
from contextlib import contextmanager, nullcontext
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
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
CONTROL_INPUTS = ('allowed.json', 'gold.jsonl', 'oracle.json', 'index.json',
                  'bin/score', 'bin/validate-readings')


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


def _proposal(proposal):
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
    inputs = r.read(run/'inputs/manifest.json')
    if inputs['files'] != r.tree(run/'inputs/frozen'):
        raise ValueError('proposal frozen inputs changed')
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


def _family(directory, report, run, modes=False):
    family = r.read(directory/'family.json')
    a.validate_author_family(family)
    original = r.rows(run/'inputs/frozen/gold.jsonl')
    for number, edit in enumerate(report['edited']):
        folder = directory/f'edited-{number}'
        gold, rows = r.rows(folder/'gold.jsonl'), r.rows(folder/'rows.jsonl')
        faces = [face for card in gold for face in card['faces'] if face['name'] == edit['face']]
        matches = [row for row in rows if row['name'] == edit['face']]
        if len(faces) != 1 or len(matches) != 1 or matches[0]['status'] != 'equal':
            raise ValueError('edited operand artifacts are incomplete')
        face, row = faces[0], matches[0]
        originals = [card for card in original if any(f['name'] == edit['face'] for f in card['faces'])]
        if len(originals) != 1:
            raise ValueError('edited operand lacks original gold')
        expected = copy.deepcopy(originals[0])
        expected_face = next(f for f in expected['faces'] if f['name'] == edit['face'])
        for mutation in edit['mutations']:
            a.edit_leaf(expected_face['abilities'], mutation['path'], mutation['value'])
        if gold != [expected]:
            raise ValueError('edited gold differs from the declared operand mutations')
        case = {'name': face['name'], 'text': row['text'],
                'context': face['characteristics'], 'expected': face['abilities'],
                'readings': None, 'exact': bool(edit['flags'][1]), 'default': True}
        if modes is True:
            case['compositional'] = False
        family['cases'].append(case)
    return family


def _helper(checkpoint, source, fixture, log, grammar_dir=None):
    fixtures = [fixture] if isinstance(fixture, (str, Path)) else list(fixture)
    if not fixtures:
        raise ValueError('family check requires at least one fixture')
    helpers = Path(checkpoint['helpers'])
    build = r.read(helpers/'build.json')
    if build['source_sha256'] != dev.source(source) or r.digest(helpers/'family-check') != build['binaries']['family-check']:
        raise ValueError('family helper does not match compiled source')
    env = {k: v for k, v in os.environ.items() if not k.startswith('ORACLE_GRAMMAR_')}
    grammar = grammar_dir or source/'grammar'
    env.update(ORACLE_GRAMMAR_DIR=str(grammar), ORACLE_GRAMMAR_THREADS=str(FAMILY_CHECK_THREADS))
    declared = {entry.removeprefix('--module=').split('=', 1)[0] for entry in dev.modules(source)}
    current = {path.stem: path for path in (grammar/'syntax').glob('*.ebnf')}
    if (grammar/'tail/mined.ebnf').is_file():
        current['tail'] = grammar/'tail/mined.ebnf'
    overrides = [f'--module={name}={current.get(name, os.devnull)}' for name in sorted(declared | current.keys())]
    lease = retire_cpu.reserve(Path(checkpoint['cpu_pool']), FAMILY_CHECK_THREADS, checkpoint.get('cpu_capacity', 32)) if checkpoint.get('cpu_pool') else nullcontext(())
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
    try:
        reports = _family_reports(log, len(fixtures))
        for index, report in enumerate(reports):
            (log.parent/f'family-{index:03d}.log').write_text(json.dumps(report, sort_keys=True)+'\n')
        if any(report.get('failures') != [] or report.get('passed') != report.get('cases')
               for report in reports):
            code = code or 1
    except (IndexError, ValueError, KeyError, TypeError, AttributeError) as error:
        if not code:
            raise ValueError('invalid family check report') from error
    if code:
        raise ValueError(f'family check failed: {log}')


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


def _control_setup(checkpoint):
    spec = checkpoint.get('native_regression_controls')
    if spec is None:
        return None
    capacity = checkpoint.get('cpu_capacity', 32)
    if (not checkpoint.get('cpu_pool') or type(capacity) is not int
            or not FAMILY_CHECK_THREADS <= capacity <= 32):
        raise ValueError('native control requires six available CPU lease tokens')
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
               'manifest_sha256': r.digest(run/'manifest.json'),
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


def _control_check(checkpoint, control, grammar, directory):
    _control_verify(control)
    directory.mkdir()
    lease = retire_cpu.reserve(Path(checkpoint['cpu_pool']), FAMILY_CHECK_THREADS, checkpoint.get('cpu_capacity', 32)) if checkpoint.get('cpu_pool') else nullcontext(())
    with lease as fds:
        r._LEASES.extend(fds)
        try:
            try:
                report = w.checks(control['run'], directory, grammar, control['names'], control['data'], FAMILY_CHECK_THREADS)
            except (ValueError, OSError, subprocess.CalledProcessError, KeyError) as error:
                r.write(directory/'result.json', {'status': 'error', 'error': str(error)})
                raise
        finally:
            for fd in fds:
                r._LEASES.remove(fd)
    r.write(directory/'result.json', report)
    if report.get('faces') != control['occurrences'] or report.get('failures') != []:
        raise ValueError('native regression control failed: '+str(directory/'result.json'))
    _control_verify(control)
    return report


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
    try:
        with tempfile.TemporaryDirectory(dir=folder) as temporary:
            trial = Path(temporary)
            shutil.copytree(source/'grammar', trial/'grammar')
            additions, prepared = [], []
            for offset, ((_, proposal), item) in enumerate(zip(entries, items, strict=True)):
                run, task, directory, hooks = _proposal(proposal)
                mechanical = proposal.get('kind', 'author') == 'mechanical'
                if mechanical and hooks:
                    raise ValueError('mechanical proposal cannot request hooks')
                fixture = None if mechanical else _fixture_path(task['fixture'])
                if fixture and ((source/fixture).exists() or str(fixture) in manifest['fixtures']
                                or fixture in [destination for _, destination in additions]):
                    raise ValueError('fixture path already exists')
                if fixture and str(fixture) in context['baseline_names']:
                    raise ValueError('fixture already registered')
                if fixture and not (source/fixture).resolve().is_relative_to(source):
                    raise ValueError('fixture escapes candidate source')
                family = _family(directory, proposal['report'], run,
                                 modes=checkpoint.get('family_case_modes') is True) if fixture else None
                report = proposal['report']
                metadata = {'rules': task['rules'], 'task': report['task'], 'base_commit': report['source'],
                            'descriptor_sha256': report['descriptor_sha256'],
                            'support_sha256': report['support_sha256'], 'fixture_sha256': report.get('fixture_sha256'),
                            'kind': proposal.get('kind', 'author')}
                changes = merger.merge_grammar(run/'inputs/frozen', trial, directory, task['rules'], hooks)
                if mechanical and changes:
                    raise ValueError('mechanical proposal contains non-removal grammar edits')
                for name, text in changes.items():
                    target = trial/name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_text(text)
                removed, kept = mined.remove(trial, task['rules'])
                if set(removed) != set(task['rules']) or kept:
                    raise ValueError('assigned removals have retained users')
                a.apply_hooks(trial/'grammar', hooks)
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
            if (r.tree(source/'grammar') != prefix_tree or r.tree(trial/'grammar') != proof['trial_grammar']
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
            'names': control['names'], 'occurrences': control['occurrences'], 'threads': FAMILY_CHECK_THREADS}
    if library_control is not None:
        manifest['library_regression_controls'] = {key: str(library_control[key]) if key == 'profile' else library_control[key]
                                                  for key in ('profile', 'profile_sha256', 'binary_sha256', 'source_sha256', 'tests')}
    r.write(folder/'manifest.json', manifest)
    if state:
        r.write(state, {'folder': str(folder)})
    if control is not None:
        try:
            _control_check(checkpoint, control, source/'grammar', folder/'native-control-base')
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
               'control': control, 'library_control': library_control, 'baseline': baseline, 'baseline_names': baseline_names,
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
    lease = retire_cpu.reserve(Path(checkpoint['cpu_pool']), threads, capacity) if checkpoint.get('cpu_pool') else nullcontext(())
    try:
      with lease as fds:
        r._LEASES.extend(fds)
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
            try:
                screen = retire_parse_screen.run(output, folder/'parse-screen', source/'grammar', data,
                                                  min(6, threads))
            finally:
                _screen_identity(source, manifest, baseline, input_run, output, evidence)
            evidence['parse_screen'] = screen
            evidence['parse_screen_result_sha256'] = r.digest(folder/'parse-screen/result.json')
            if screen['failures']:
                raise ValueError('full allowed-population parse screen failed')
        checks = folder/'full-checks'
        checks.mkdir()
        try:
            report = w.checks(output, checks, source/'grammar', [e['name'] for e in data[0].values()], data, threads)
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
    finally:
        if 'fds' in locals():
            for fd in fds:
                r._LEASES.remove(fd)
    r.write(folder/'validation.json', evidence)
    return evidence
