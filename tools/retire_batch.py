"""Private provisional integration and validation of retirement proposals."""
from argparse import Namespace
import copy
from contextlib import contextmanager, nullcontext
import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import uuid

import dev
import mined
import retire as r
import retire_author as a
import retire_cpu
import retire_merge as merger
import retire_worker as w


def _git(source, *args):
    return subprocess.check_output(['git', '-c', 'core.hooksPath=/dev/null', *args], cwd=source, text=True).strip()


def _fixture_path(value):
    path = Path(value)
    if path.is_absolute() or '..' in path.parts or len(path.parts) < 2 or path.parts[0] != 'families' or path.suffix != '.json':
        raise ValueError('fixture must be a path under families/')
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


def _family(directory, report, run):
    family = r.read(directory/'family.json')
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
        family['cases'].append({'name': face['name'], 'text': row['text'],
                                'context': face['characteristics'], 'expected': face['abilities'],
                                'readings': None, 'exact': bool(edit['flags'][1]), 'default': True})
    return family


def _helper(checkpoint, source, fixture, log):
    helpers = Path(checkpoint['helpers'])
    build = r.read(helpers/'build.json')
    if build['source_sha256'] != dev.source(source) or r.digest(helpers/'family-check') != build['binaries']['family-check']:
        raise ValueError('family helper does not match compiled source')
    env = {k: v for k, v in os.environ.items() if not k.startswith('ORACLE_GRAMMAR_')}
    env.update(ORACLE_GRAMMAR_DIR=str(source/'grammar'), ORACLE_GRAMMAR_THREADS='2')
    lease = retire_cpu.reserve(Path(checkpoint['cpu_pool']), 2, checkpoint.get('cpu_capacity', 32)) if checkpoint.get('cpu_pool') else nullcontext(())
    with lease as fds:
        r._LEASES.extend(fds)
        try:
            with log.open('w') as stream:
                code = subprocess.run([str((helpers/'family-check').resolve()), str(fixture), '--compositional'],
                                      cwd=source, env=env, stdout=stream, stderr=stream, check=False,
                                      pass_fds=tuple(r._LEASES)).returncode
        finally:
            for fd in fds:
                r._LEASES.remove(fd)
    if code:
        raise ValueError(f'family check failed: {fixture}')


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


def stage(campaign, checkpoint, proposals):
    with _workspace_lock(checkpoint):
        return _stage(campaign, checkpoint, proposals)


def _stage(campaign, checkpoint, proposals):
    """Create and commit one private candidate; return a reviewable manifest."""
    proposals = list(proposals)
    if not 1 <= len(proposals) <= 96:
        raise ValueError('a batch must contain one to ninety-six proposals')
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
                'status': 'staging', 'jobs': [], 'fixtures': [], 'rules': []}
    r.write(folder/'manifest.json', manifest)
    if state:
        r.write(state, {'folder': str(folder)})
    for proposal in proposals:
        item = {'job_id': proposal['job_id'], 'attempt_id': proposal['attempt_id']}
        try:
            run, task, directory, hooks = _proposal(proposal)
            mechanical = proposal.get('kind', 'author') == 'mechanical'
            if mechanical and hooks:
                raise ValueError('mechanical proposal cannot request hooks')
            fixture = None if mechanical else _fixture_path(task['fixture'])
            if fixture and ((source/fixture).exists() or str(fixture) in manifest['fixtures']):
                raise ValueError('fixture path already exists')
            family = _family(directory, proposal['report'], run) if fixture else None
            if fixture and not (source/fixture).resolve().is_relative_to(source):
                raise ValueError('fixture escapes candidate source')
            with tempfile.TemporaryDirectory(dir=folder) as temporary:
                trial = Path(temporary)
                shutil.copytree(source/'grammar', trial/'grammar')
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
                    r.write(trial/'family.json', family)
                    (source/fixture).parent.mkdir(parents=True, exist_ok=True)
                backup = trial/'previous-grammar'
                os.replace(source/'grammar', backup)
                try:
                    os.replace(trial/'grammar', source/'grammar')
                    if fixture:
                        os.replace(trial/'family.json', source/fixture)
                except OSError:
                    if (source/'grammar').exists():
                        shutil.rmtree(source/'grammar')
                    os.replace(backup, source/'grammar')
                    raise
            if fixture:
                manifest['fixtures'].append(str(fixture))
            manifest['rules'].extend(task['rules'])
            item.update(status='included', rules=task['rules'], task=proposal['report']['task'],
                        base_commit=proposal['report']['source'], descriptor_sha256=proposal['report']['descriptor_sha256'],
                        support_sha256=proposal['report']['support_sha256'], fixture_sha256=proposal['report'].get('fixture_sha256'), kind=proposal.get('kind', 'author'))
        except (ValueError, OSError, KeyError, TypeError) as error:
            item.update(status='integration-rejected', reason=str(error))
        manifest['jobs'].append(item)
    if not any(job['status'] == 'included' for job in manifest['jobs']):
        manifest['status'] = 'no-proposals'
    else:
        try:
            registry = source/'tests/families.json'
            families = r.read(registry)
            if any(f in families for f in manifest['fixtures']):
                raise ValueError('fixture already registered')
            if manifest['fixtures']:
                r.write(registry, families + manifest['fixtures'])
            for index, fixture in enumerate(manifest['fixtures']):
                _helper(checkpoint, source, source/fixture, folder/f'family-{index}.log')
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


def validate(manifest, checkpoint, slots=12):
    with _workspace_lock(checkpoint):
        return _validate(manifest, checkpoint, slots)


def _validate(manifest, checkpoint, slots):
    """Freeze and run full allowed-population and repository gates for a candidate."""
    if manifest.get('status') != 'provisional':
        raise ValueError('only a provisional candidate can be validated')
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
                'baseline_sha256': r.digest(baseline), 'inputs': str(output),
                'source_checkpoint': checkpoint['commit']}
    lease = retire_cpu.reserve(Path(checkpoint['cpu_pool']), 24, checkpoint.get('cpu_capacity', 32)) if checkpoint.get('cpu_pool') else nullcontext(())
    try:
      with lease as fds:
        r._LEASES.extend(fds)
        r.freeze(Namespace(run=output, source=source, helpers=Path(checkpoint['helpers']).resolve(),
                           slots=1, threads=24, allowed=inputs/'allowed.json', gold=inputs/'gold.jsonl',
                           oracle=inputs/'oracle.json', parse=baseline, prints=[baseline]))
        data = w.payload(output)
        checks = folder/'full-checks'
        checks.mkdir()
        report = w.checks(output, checks, source/'grammar', [e['name'] for e in data[0].values()], data, 24)
        evidence['full_checks'] = report
        if report['failures']:
            raise ValueError('full allowed-population checks failed')
        # dev.py runs families and library tests concurrently. Divide the lease
        # between those two gates, leaving headroom for the fast gates.
        gate_env = {**os.environ, 'CARGO_BUILD_JOBS': '20', 'RAYON_NUM_THREADS': '10',
                    'ORACLE_GRAMMAR_THREADS': '10', 'RUST_TEST_THREADS': '10'}
        with (folder/'repository-check.log').open('w') as log:
            code = subprocess.run(['python3', 'tools/dev.py', 'check'], cwd=source, env=gate_env,
                                  stdout=log, stderr=log, check=False, pass_fds=tuple(r._LEASES)).returncode
        evidence['repository_check_returncode'] = code
        if code:
            raise ValueError('repository gates failed')
        if (r.digest(baseline) != evidence['baseline_sha256']
                or r.read(input_run/'manifest.json')['files'] != r.tree(inputs)):
            raise ValueError('checkpoint inputs changed during validation')
        if _git(source, 'rev-parse', 'HEAD') != manifest['commit'] or _git(source, 'status', '--porcelain'):
            raise ValueError('candidate changed during validation')
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
