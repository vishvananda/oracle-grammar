"""Durable, explicitly adjudicated work queue for rule retirement campaigns.

This module never launches workers or decides whether their evidence is sound.
SQLite transactions serialize claims, checkpoint advancement, and adjudication.
"""
import errno
import fcntl
import hashlib
import io
import json
import os
import re
import subprocess
from pathlib import Path
import stat
import sqlite3
import tarfile
import tempfile
import uuid
from contextlib import ExitStack, closing, contextmanager

import retire_cohort_proof
import retire_rehabilitation
import retire_native_readmission
import retire_native_exit_readmission
import retire_parse_readmission
import retire_library_readmission
import retire_priority
import retire_timeout_recovery
import shapes

REVIEWED_RETRY_EVENTS = {
    None: 'retry-reviewed-repair',
    'interrupted_diagnostic': 'retry-interrupted-reviewed-repair',
    'legacy_diagnostic_prompt': 'retry-preview-reviewed-repair',
    'staging_failure': 'retry-staging-reviewed-repair',
    'full_check_failure': 'retry-full-check-reviewed-repair',
}
SPLIT_REVIEWED_AUTHOR = 'split-reviewed-author'
_REVIEWED_EVENTS = (*REVIEWED_RETRY_EVENTS.values(), SPLIT_REVIEWED_AUTHOR)
_REVIEWED_EVENT_SLOTS = ','.join('?' for _ in _REVIEWED_EVENTS)


def _pid_started_after(pid, finished):
    """Distinguish a recycled Linux PID from a completed attempt's process."""
    if type(finished) is not int or finished <= 0:
        return False
    try:
        # The parenthesized process name may include whitespace or parentheses.
        stat_fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
        ticks = int(stat_fields[19])
        with Path('/proc/stat').open() as stream:
            boot = next(int(line.split()[1]) for line in stream if line.startswith('btime '))
        started = boot + ticks / os.sysconf('SC_CLK_TCK')
    except (IndexError, OSError, StopIteration, ValueError, ZeroDivisionError):
        return False
    # The kernel and SQLite wall-clock timestamps have whole-second precision.
    # Leave headroom for clock adjustments when comparing their observations.
    return started > finished + 60


def _json(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)


def _evidence(value):
    if not value:
        raise ValueError("evidence is required")
    return _json(value)


def _proof_file(spec):
    if not isinstance(spec, dict) or not isinstance(spec.get('path'), str) or not isinstance(spec.get('sha256'), str):
        raise ValueError('artifact path and SHA-256 are required')
    path = Path(spec['path'])
    if not path.is_absolute() or not path.is_file() or path.is_symlink() or path.stat().st_size > 32 * 1024 * 1024:
        raise ValueError('evidence artifact is unavailable or too large')
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != spec['sha256']:
        raise ValueError('evidence artifact hash differs')
    return path, data


def _verify_readmission_artifacts(campaign, job_id, attempt_id, base_commit, original, rejection, evidence):
    manifest_path, manifest_bytes = _proof_file(evidence.get('previous_manifest'))
    previous = rejection.get('evidence', {}).get('manifest')
    if not isinstance(previous, str) or not previous or Path(previous).name != previous:
        raise ValueError('previous rejection manifest identity is missing')
    expected_manifest = campaign/'.cache/batches'/previous/'manifest.json'
    if manifest_path.resolve() != expected_manifest.resolve():
        raise ValueError('previous manifest does not match rejection')
    manifest = json.loads(manifest_bytes)
    matches = [x for x in manifest.get('jobs', []) if x.get('job_id') == job_id and x.get('attempt_id') == attempt_id]
    if (manifest.get('id') != previous or len(matches) != 1 or matches[0].get('status') != 'integration-rejected'
            or not str(matches[0].get('reason', '')).startswith('family check failed:')):
        raise ValueError('manifest does not show the original family-gate rejection')
    gate_path, gate_bytes = _proof_file(evidence.get('gate_proof'))
    logs = Path(matches[0].get('family_logs', '')).resolve()
    if not logs.is_relative_to(expected_manifest.parent.resolve()) or gate_path.resolve().parent != logs:
        raise ValueError('gate proof is outside the rejected proposal logs')
    report, authored = _verify_original_report(campaign, job_id, attempt_id, base_commit, original, evidence.get('report'))
    edited = report.get('edited', [])
    try:
        gate = json.loads(gate_bytes.splitlines()[0])
        failure_names = {item['name'] for item in gate['failures']}
        edited_names = {item['face'] for item in edited}
    except (ValueError, IndexError, KeyError, TypeError) as exc:
        raise ValueError('invalid family gate proof') from exc
    if (not edited or not failure_names or not failure_names <= edited_names
            or gate.get('family') != authored.get('family') or gate.get('compositional') is not True
            or gate.get('cases') != len(authored.get('cases', [])) + len(edited)):
        raise ValueError('family gate proof does not match generated operand cases')


def _verify_original_report(campaign, job_id, attempt_id, base_commit, original, report_spec, *,
                            passed=True, unremoved=False):
    report_path, report_bytes = _proof_file(report_spec)
    if report_path != Path(original.get('report', '')):
        raise ValueError('report is not the original independently evaluated report')
    expected_run = campaign/'.cache/attempts'/attempt_id
    expected_eval = expected_run/'evaluations'/job_id
    if not report_path.resolve().is_relative_to(expected_eval.resolve()) or report_path.name != 'report.json':
        raise ValueError('report is outside the original evaluation')
    report = json.loads(report_bytes)
    failures = report.get('failures')
    if (report.get('passed') is not passed or not isinstance(failures, list) or bool(failures) == passed
            or report.get('task') != job_id or report.get('source') != base_commit
            or Path(report.get('directory', '')).resolve() != report_path.parent.resolve()):
        raise ValueError('original independent report did not pass' if passed
                         else 'original independent report did not record a failure')
    descriptor = expected_run/'tasks'/f'{job_id}.json'
    support = expected_run/'support.json'
    family = report_path.parent/'family.json'
    artifacts = [(descriptor, 'descriptor_sha256'), (support, 'support_sha256')]
    if unremoved:
        if (passed or failures != ['remove exactly assigned rules; never rename or adopt seeded rules']
                or report.get('scope') == 'author-target-preview'
                or 'family_returncode' in report or family.exists()
                or report.get('source_grammar') != report.get('grammar')):
            raise ValueError('the archived draft did not stop at canonical seeded removal')
    else:
        artifacts.append((family, 'fixture_sha256'))
    for path, key in artifacts:
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != report.get(key):
            raise ValueError('original evaluation artifact hash differs: '+key)
    support_data = json.loads(support.read_bytes())
    if support_data.get('tasks', {}).get(job_id) != report.get('descriptor_sha256'):
        raise ValueError('original support manifest does not bind the task')
    grammar = report_path.parent/'grammar'
    if not grammar.is_dir():
        raise ValueError('original evaluated grammar is missing')
    grammar_files = {}
    for path in sorted(grammar.rglob('*')):
        if path.is_symlink():
            raise ValueError('original evaluated grammar contains a symlink')
        if path.is_file():
            grammar_files[str(path.relative_to(grammar))] = hashlib.sha256(path.read_bytes()).hexdigest()
    if grammar_files != report.get('grammar'):
        raise ValueError('original evaluated grammar changed')
    return report, None if unremoved else json.loads(family.read_bytes())


def _verify_reviewed_repair(campaign, checkpoint, job, evidence, *, timed_out_attempt=None):
    path, _ = _proof_file(evidence.get('report'))
    if path.resolve() != path or len(path.parents) < 7:
        raise ValueError('repair report must have a canonical evaluation path')
    job_id = job['id']
    work = path.parents[3]
    scratch = work.parents[2]
    if ((scratch == campaign.resolve() and work.name != timed_out_attempt)
            or (timed_out_attempt is not None and scratch != campaign.resolve())
            or not re.fullmatch(r'[0-9a-f-]{36}', work.name)
            or work.parent != scratch/'.cache/attempts'
            or path.parent.parent != work/'evaluations'/job_id):
        raise ValueError('repair report is outside its private attempt')
    report, _ = _verify_original_report(scratch, job_id, work.name, checkpoint,
                                       {'report': str(path)}, evidence['report'])
    task = json.loads((work/'tasks'/f'{job_id}.json').read_bytes())
    cp = json.loads((campaign/'checkpoints'/f'{checkpoint}.json').read_bytes())
    inputs = Path(cp['inputs']).resolve()
    baseline = json.loads((inputs/'manifest.json').read_bytes())
    closure = task.get('cohort_detail', {}).get('closure_occurrence_count')
    edits = report.get('edited')
    if (cp.get('commit') != checkpoint or baseline.get('commit') != checkpoint
            or (work/'inputs').resolve() != inputs
            or task.get('id') != job_id or task.get('rules') != job['rules']
            or task.get('target_faces') != job.get('target_faces')
            or type(closure) is not int or not 1 <= closure <= 1024 or report.get('faces') != closure
            or report.get('scope') == 'author-target-preview'
            or report.get('family_returncode') != 0 or not isinstance(edits, list) or not 2 <= len(edits) <= 8):
        raise ValueError('repair report differs from the assigned checkpoint, cohort or required checks')
    return report, task


def _verify_staging_repair(campaign, job, attempt, source, rejection, evidence):
    """Bind a repair to the exact solo trial rejected by the integration driver."""
    if not isinstance(evidence, dict):
        raise ValueError('staging repair requires retained trial evidence')
    manifest_path, manifest_bytes = _proof_file(evidence.get('manifest'))
    trial_path, trial_bytes = _proof_file(evidence.get('trial'))
    manifest, trial = json.loads(manifest_bytes), json.loads(trial_bytes)
    previous = rejection.get('evidence', {})
    batch = previous.get('manifest')
    if (not isinstance(batch, str) or Path(batch).name != batch or not batch
            or previous != {'manifest': batch, 'reason': 'merge or staging rejected'}
            or manifest_path != campaign/'.cache/batches'/batch/'manifest.json'
            or manifest_path.resolve() != manifest_path or manifest.get('id') != batch
            or manifest.get('base_commit') != source
            or trial_path.name != 'trial.json' or trial_path.parent.parent != manifest_path.parent
            or trial_path.resolve() != trial_path or not re.fullmatch(r'proposal-\d{3}', trial_path.parent.name)):
        raise ValueError('staging repair is outside the rejected solo trial')
    matches = [item for item in manifest.get('jobs', []) if item.get('job_id') == job]
    if (len(matches) != 1 or matches[0].get('attempt_id') != attempt
            or matches[0].get('status') != 'integration-rejected'
            or matches[0].get('trial_proof') != evidence['trial']
            or matches[0].get('reason') != trial.get('reason')
            or trial.get('jobs') != [job] or trial.get('attempts') != [attempt]
            or trial.get('status') != 'failed'):
        raise ValueError('staging repair does not identify the rejected attempt')
    for reason, relative, digest in (
            ('family check failed', 'family.log', 'family_log_sha256'),
            ('native regression control failed', 'native-control/result.json', 'native_result_sha256'),
            ('native regression control failed', 'native-focused/result.json', 'native_focused_result_sha256'),
            ('library regression control failed', 'library-control/result.json', 'library_result_sha256'),
            ('print shape check failed', 'print-shape.json', 'print_shape_sha256')):
        failure = trial_path.parent/relative
        if trial.get('reason') == f'{reason}: {failure}':
            path, _ = _proof_file(evidence.get('failure'))
            if path == failure and path.resolve() == path and evidence['failure']['sha256'] == trial.get(digest):
                return
    raise ValueError('staging repair does not bind a recorded diagnostic failure')


def _verify_rewritten_review(db, campaign, assignment, start, result, evidence, reviewed):
    """Recover the same checked draft after an author rewrote it and staging vetoed it."""
    job, attempt, source = assignment
    prior = db.execute(
        f"SELECT data FROM events WHERE kind IN ({_REVIEWED_EVENT_SLOTS}) "
        "AND json_extract(data,'$.job_id')=? ORDER BY seq DESC LIMIT 1",
        (*_REVIEWED_EVENTS, job)).fetchone()
    prior = json.loads(prior[0]) if prior else {}
    original = prior.get('evidence', {})
    if (prior.get('checkpoint') != source or original.get('source_checkpoint') != source
            or original.get('report') != evidence['report'] or 'timeout' in original
            or reviewed.get('hooks') != []):
        raise ValueError('rewritten draft differs from the exact recorded review')
    replacement = evidence['staging_failure']['rewritten_draft']
    if not isinstance(replacement, dict) or set(replacement) != {'report', 'model_receipt'}:
        raise ValueError('rewritten draft requires the evaluated report and model receipt')
    reported, _ = _verify_original_report(campaign, job, attempt, source, result['evidence'], replacement['report'])
    work = campaign/'.cache/attempts'/attempt
    task = json.loads((work/'tasks'/f'{job}.json').read_bytes())
    expected = {'kind': 'independently-reviewed-draft', 'report_sha256': evidence['report']['sha256'],
                'rebased_from': source, 'rebased_onto': source}
    path, body = _proof_file(replacement['model_receipt'])
    model = json.loads(body)
    if (path != work/'workers'/job/'receipt.json' or str(path) != start.get('receipt')
            or model.get('pid') != start['pid'] or model.get('status') != 'completed'
            or model.get('evaluation') != {'passed': True, 'directory': reported['directory']}
            or task.get('reuse') != expected or reported['grammar'] == reviewed['grammar']):
        raise ValueError('model did not replace the recorded reviewed grammar')



def _verify_shape_repair(db, campaign, checkpoint, assignment, rejection, evidence):
    """Bind a shape-only repair to a survivor readmission that excluded this attempt."""
    job, attempt = assignment
    seq = evidence.get('readmission_event_seq')
    if set(evidence) != {'kind', 'readmission_event_seq'} or type(seq) is not int or seq <= 0:
        raise ValueError('shape repair must identify a completed survivor readmission')
    row = db.execute("SELECT data FROM events WHERE seq=? AND kind='readmitted-shape-baseline'", (seq,)).fetchone()
    if row is None:
        raise ValueError('shape repair survivor readmission is missing')
    readmitted = json.loads(row[0])
    original, proof = readmitted['evidence'], readmitted['proof']
    batch = readmitted.get('manifest')
    if (original.get('kind') != 'isolated-shape-revalidation'
            or not isinstance(batch, str) or not re.fullmatch(r'[0-9a-f]{32}', batch)
            or proof.get('manifest') != batch or proof.get('excluded_proposal') != job
            or readmitted.get('job_id') == job or not proof.get('introduced_shapes')
            or proof.get('requires_fresh_full_validation') is not True
            or readmitted.get('previous_adjudication') != rejection):
        raise ValueError('shape repair does not match the verified excluded proposal')
    folder = campaign/'.cache/batches'/batch
    manifest_path, manifest_bytes = _proof_file(original.get('manifest'))
    validation_path, validation_bytes = _proof_file(original.get('validation'))
    log_path, log = _proof_file(original.get('repository_log'))
    manifest, validation = json.loads(manifest_bytes), json.loads(validation_bytes)
    origin = manifest.get('base_commit')
    matches = [item for item in manifest.get('jobs', []) if item.get('job_id') == job]
    if (manifest_path != folder/'manifest.json' or validation_path != folder/'validation.json'
            or log_path != folder/'repository-check.log' or manifest.get('id') != batch
            or rejection.get('evidence') != {'manifest': batch, 'validation': validation}
            or validation.get('status') != 'failed' or validation.get('reason') != 'repository gates failed'
            or validation.get('source_checkpoint') != origin or original.get('source_checkpoint') != origin
            or validation.get('candidate') != manifest.get('commit') or proof.get('candidate') != manifest.get('commit')
            or {name for name, state in _gates(log).items() if state == 'FAILED'} != {'hygiene', 'tools'}
            or len(matches) != 1 or matches[0].get('attempt_id') != attempt
            or matches[0].get('status') != 'included'):
        raise ValueError('shape repair does not identify the rejected attempt')
    _verify_readmission_origin(db, checkpoint, readmitted['checkpoint'])
    _verify_readmission_origin(db, checkpoint, origin)


def _verify_isolated_repair(db, campaign, checkpoint, assignment, rejection, evidence, *, full_check=False):
    """Bind a repair to an excluded proposal in a verified survivor readmission."""
    if full_check and evidence.get('kind') == 'isolated-shape-repair':
        _verify_shape_repair(db, campaign, checkpoint, assignment, rejection, evidence)
        return
    job, attempt = assignment
    seq = evidence.get('readmission_event_seq')
    if set(evidence) != {'readmission_event_seq', 'isolation'} or type(seq) is not int or seq <= 0:
        raise ValueError('parse repair must identify a completed survivor readmission')
    kind = 'readmitted-native-batch' if full_check else 'readmitted-parse-screen-batch'
    row = db.execute('SELECT data FROM events WHERE seq=? AND kind=?', (seq, kind)).fetchone()
    if row is None:
        raise ValueError('parse repair survivor readmission is missing')
    readmitted = json.loads(row[0])
    isolation_path, isolation_bytes = _proof_file(evidence['isolation'])
    isolation = json.loads(isolation_bytes)
    original = readmitted.get('evidence', {})
    proof = readmitted.get('proof', {})
    manifest_path, manifest_bytes = _proof_file(original.get('manifest'))
    manifest = json.loads(manifest_bytes)
    validation = rejection.get('evidence', {}).get('validation', {})
    batch, origin = manifest.get('id'), manifest.get('base_commit')
    excluded = isolation.get('excluded_jobs')
    reason = 'full allowed-population checks failed' if full_check else 'full allowed-population parse screen failed'
    if full_check and (original.get('kind') != 'isolated-full-check-revalidation'
                       or proof.get('proof_scope') != 'archived full-check isolation'):
        raise ValueError('print repair requires a verified full-check isolation')
    if (not isinstance(batch, str) or not batch or Path(batch).name != batch
            or manifest_path != campaign/'.cache/batches'/batch/'manifest.json'
            or manifest_path.resolve() != manifest_path
            or isolation_path.resolve() != isolation_path
            or readmitted.get('manifest') != batch
            or readmitted.get('previous_adjudication') != rejection
            or original.get('isolation') != evidence['isolation']
            or proof.get('verified_artifacts', {}).get(str(isolation_path)) != evidence['isolation']['sha256']
            or proof.get('manifest') != batch
            or not isinstance(excluded, list) or not 1 <= len(excluded) <= 96 or job not in excluded
            or proof.get('excluded_jobs') != excluded
            or proof.get('requires_fresh_full_validation') is not True
            or isolation.get('parent') != origin
            or validation.get('source_checkpoint') != origin
            or validation.get('reason') != reason):
        raise ValueError('parse repair does not match the verified excluded proposals')
    matches = [item for item in manifest.get('jobs', []) if item.get('job_id') == job]
    if (len(matches) != 1 or matches[0].get('attempt_id') != attempt
            or matches[0].get('status') != 'included'):
        raise ValueError('parse repair does not identify the rejected attempt')
    _verify_readmission_origin(db, checkpoint, origin)


_GATE_NAMES = {'fmt', 'pin', 'hygiene', 'tools', 'clippy', 'build', 'families', 'test'}
_DIAGNOSTIC_EXCLUSIONS = {'src/bin/family-check.rs', 'src/tests/family_check_modes.rs'}


def _gates(data):
    text = data.decode('utf-8')
    rows = re.findall(r'^(fmt|pin|hygiene|tools|clippy|build|families|test)\s+(passed|reused|FAILED)\s+', text, re.M)
    if len(rows) != len(_GATE_NAMES) or {name for name, _ in rows} != _GATE_NAMES:
        raise ValueError('repository gate report is incomplete or duplicated')
    return dict(rows)


def _git_source_identity(repo, old, new):
    for commit in (old, new):
        if not isinstance(commit, str) or not re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}', commit):
            raise ValueError('source commit must be an immutable full Git ID')
    def read(*args):
        try:
            return subprocess.check_output(['git', '-C', str(repo), *args], stderr=subprocess.PIPE)
        except (OSError, subprocess.CalledProcessError) as exc:
            raise ValueError('unable to verify recovery source commits') from exc
    grammar_before = read('rev-parse', old+':grammar').strip()
    grammar_after = read('rev-parse', new+':grammar').strip()
    if grammar_before != grammar_after:
        raise ValueError('audited grammar changed')
    selected = {'Cargo.toml', 'Cargo.lock', 'pins/model.json', '.cargo/config.toml',
                'rust-toolchain.toml', 'clippy.toml', 'grammar/lexical.ebnf',
                'grammar/print-model.txt', 'grammar/print-weights.txt', 'tools/dev.py', 'tools/pin.py'}
    def native(commit):
        records = {}
        for item in read('ls-tree', '-r', '-z', '--full-tree', commit).split(b'\0'):
            if not item:
                continue
            head, raw = item.split(b'\t', 1)
            name = os.fsdecode(raw)
            if ((name.startswith(('src/', 'crates/')) or name in selected)
                    and name not in _DIAGNOSTIC_EXCLUSIONS):
                records[name] = head
        return records
    native_before, native_after = native(old), native(new)
    if native_before != native_after:
        raise ValueError('native implementation changed beyond diagnostic exclusions')
    def digest_native(records):
        digest = hashlib.sha256()
        for name in sorted(records):
            digest.update(os.fsencode(name)+b'\0'+records[name]+b'\n')
        return digest.hexdigest()
    return {'old_commit': old, 'new_commit': new,
            'grammar_before_sha256': hashlib.sha256(grammar_before).hexdigest(),
            'grammar_after_sha256': hashlib.sha256(grammar_after).hexdigest(),
            'native_before_sha256': digest_native(native_before),
            'native_after_sha256': digest_native(native_after),
            'excluded_native_paths': sorted(_DIAGNOSTIC_EXCLUSIONS)}



def _verify_diagnostic_recovery(campaign, commit, parent, jobids, evidence):
    if not isinstance(evidence, dict) or evidence.get('kind') != 'diagnostic-only-family-limit':
        raise ValueError('diagnostic-only-family-limit evidence is required')
    manifest_path, manifest_data = _proof_file(evidence.get('manifest'))
    validation_path, validation_data = _proof_file(evidence.get('validation'))
    old_log_path, old_log = _proof_file(evidence.get('previous_repo_log'))
    _, new_log = _proof_file(evidence.get('new_repo_log'))
    _, identity_bytes = _proof_file(evidence.get('identity_proof'))
    manifest, validation, identity = map(json.loads, (manifest_data, validation_data, identity_bytes))
    batch_id = manifest.get('id')
    if (not isinstance(batch_id, str) or not batch_id or Path(batch_id).name != batch_id
            or manifest_path.resolve() != (campaign/'.cache/batches'/batch_id/'manifest.json').resolve()
            or validation_path.resolve() != (manifest_path.parent/'validation.json').resolve()
            or old_log_path.resolve() != (manifest_path.parent/'repository-check.log').resolve()):
        raise ValueError('original batch artifact paths differ')
    included = [item for item in manifest.get('jobs', []) if item.get('status') == 'included']
    ids = [item.get('job_id') for item in included]
    full = validation.get('full_checks', {})
    if (not included or len(set(ids)) != len(ids) or set(ids) != set(jobids)
            or len(validation.get('included_jobs', [])) != len(ids)
            or set(validation.get('included_jobs', [])) != set(ids)
            or validation.get('status') != 'failed' or validation.get('reason') != 'repository gates failed'
            or type(validation.get('repository_check_returncode')) is not int
            or validation['repository_check_returncode'] == 0
            or type(full.get('faces')) is not int or full['faces'] <= 0 or full.get('failures') != []
            or validation.get('candidate') != manifest.get('commit')
            or validation.get('source_checkpoint') != parent or manifest.get('base_commit') != parent):
        raise ValueError('original combined audit is not the specified native-pass repository-gate failure')
    if (_gates(old_log).get('families') != 'FAILED'
            or any(status == 'FAILED' for name, status in _gates(old_log).items() if name != 'families')
            or b'at most 64' not in old_log or b'family-check' not in old_log
            or any(status == 'FAILED' for status in _gates(new_log).values())):
        raise ValueError('repository gates do not prove the bounded-family-only failure and recovery')
    if (identity.get('old_commit') != validation['candidate'] or identity.get('new_commit') != commit
            or identity.get('parent_checkpoint') != parent
            or set(identity.get('excluded_native_paths', [])) != _DIAGNOSTIC_EXCLUSIONS):
        raise ValueError('source identity proof does not match the recovery commits or exclusions')
    config = json.loads((campaign/'config.json').read_bytes())
    actual = _git_source_identity(config['source_repo'], validation['candidate'], commit)
    for field in ('grammar_before_sha256', 'grammar_after_sha256',
                  'native_before_sha256', 'native_after_sha256'):
        if identity.get(field) != actual[field]:
            raise ValueError('identity proof hash differs from verified Git source: '+field)
    return manifest, validation, included


def _verify_shape_survivors(campaign, origin, included, selected, baseline, evidence):
    from retire_batch import _ProposalInputCache

    indices = [index for index, job in enumerate(included) if job['job_id'] in selected]
    if len(indices) != len(included)-1:
        raise ValueError('one shape-producing proposal must be excluded')
    survivor = evidence['survivor']
    grammar = Path(survivor['grammar'])
    if (grammar.name != 'grammar' or grammar != grammar.resolve()
            or not grammar.is_relative_to((campaign/'.cache').resolve())
            or not grammar.is_dir()):
        raise ValueError('shape survivor grammar is outside campaign evidence')
    expected = survivor['tree']
    if (retire_library_readmission._grammar_tree(grammar) != expected
            or shapes.introduced(shapes.found(grammar.parent), baseline)):
        raise ValueError('shape survivor grammar changed or introduces a print shape')
    retire_library_readmission._replay(campaign, origin, included, indices, expected, grammar.parent,
                                       evidence['reports'], canonical_mechanical=True,
                                       input_cache=_ProposalInputCache())
    if retire_library_readmission._grammar_tree(grammar) != expected:
        raise ValueError('shape survivor grammar changed during independent replay')
    return next(item['job_id'] for item in included if item['job_id'] not in selected)


def _verify_shape_readmission(campaign, selected, origin, rejections, evidence):
    values = {key: _proof_file(evidence.get(key)) for key in ('manifest', 'validation', 'repository_log')}
    manifest = json.loads(values['manifest'][1])
    validation = json.loads(values['validation'][1])
    ident = manifest.get('id')
    candidate = manifest.get('commit')
    if not re.fullmatch(r'[0-9a-f]{32}', ident or '') or not re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}', candidate or ''):
        raise ValueError('shape readmission requires immutable batch identities')
    folder = campaign/'.cache/batches'/ident
    for key, name in (('manifest','manifest.json'), ('validation','validation.json'),
                      ('repository_log','repository-check.log')):
        if values[key][0] != folder/name:
            raise ValueError('shape readmission batch artifacts differ')
    included = [item for item in manifest['jobs'] if item['status']=='included']
    attempts = {item['job_id']:item['attempt_id'] for item in included}
    shape_removal = evidence.get('kind') == 'isolated-shape-revalidation'
    chosen = {job: attempt for job, attempt in attempts.items() if not shape_removal or job in selected}
    expected = {'status':'integration-rejected','checkpoint':None,
                'evidence':{'manifest':ident,'validation': validation}}
    if (manifest['status']!='provisional' or manifest.get('base_commit')!=origin
            or validation.get('source_checkpoint')!=origin or validation.get('candidate')!=candidate
            or validation.get('status')!='failed' or validation.get('reason')!='repository gates failed'
            or validation.get('repository_check_returncode')!=1
            or len(attempts)!=len(included) or chosen!=selected
            or validation.get('included_jobs')!=list(attempts)
            or any(rejection!=expected for rejection in rejections.values())):
        raise ValueError('shape readmission does not cover the exact rejected batch')
    log = values['repository_log'][1]
    failures = {gate for gate, state in _gates(log).items() if state=='FAILED'}
    if failures!={'hygiene','tools'}:
        raise ValueError('repository rejection was not limited to shape diagnostics')
    config = json.loads((campaign/'config.json').read_bytes())
    git = ['git','-C',config['source_repo'],'archive',candidate,'grammar',shapes.BASELINE_PATH]
    archive = subprocess.check_output(git,timeout=30)
    with tempfile.TemporaryDirectory() as temporary:
        with tarfile.open(fileobj=io.BytesIO(archive)) as contents:
            contents.extractall(temporary,filter='data')
        stale = shapes.check(Path(temporary))
    text = log.decode()
    header = re.search(r'^== hygiene \([^\n]+\)\n(.*?)\n\n== tools \(',text,re.M | re.S)
    if (not stale or header is None or header[1].splitlines()!=[*stale,f'{len(stale)} hygiene checks failed']
            or re.findall(r'^FAIL: (\S+)',text,re.M)!=['test_repository_matches_its_baseline']
            or 'FAILED (failures=1)' not in text):
        raise ValueError('the archived batch did not fail solely on print shapes')
    proof = {'manifest':ident,'candidate':candidate,'requires_fresh_full_validation':True}
    if shape_removal:
        if not all(': new print-blowup shape ' in message for message in stale):
            raise ValueError('shape survivor recovery requires newly introduced shapes')
        baseline = json.loads(subprocess.check_output(
            ['git', '-C', config['source_repo'], 'show', f'{origin}:{shapes.BASELINE_PATH}'], timeout=30))
        proof['excluded_proposal'] = _verify_shape_survivors(campaign, origin, included, selected, baseline, evidence)
        proof['introduced_shapes'] = stale
    else:
        if not all(message.endswith('is gone; remove it from '+shapes.BASELINE_PATH) for message in stale):
            raise ValueError('the archived batch did not fail solely on resolved shape exemptions')
        proof['removed_exemptions'] = stale
    return proof


def _verify_disk_retry(campaign, job_id, attempt_id, base, job, started, result, evidence):
    run = campaign/'.cache/attempts'/attempt_id
    worker = run/'workers'/job_id
    expected = {'evaluation_log': worker/'evaluation.log', 'model_receipt': worker/'receipt.json',
                'final': worker/'final.txt', 'descriptor': run/'tasks'/f'{job_id}.json',
                'support': run/'support.json'}
    data = {}
    for key, wanted in expected.items():
        path, contents = _proof_file(evidence.get(key))
        if path.resolve() != wanted.resolve() or any(part.is_symlink() for part in
                (run, run/'workers', worker, run/'tasks', wanted)):
            raise ValueError('disk retry artifact is outside the assigned attempt: '+key)
        data[key] = contents
    receipt, final, descriptor, support = map(json.loads, (data['model_receipt'], data['final'],
                                                            data['descriptor'], data['support']))
    original = result.get('evidence')
    if not isinstance(original, dict):
        raise ValueError('original attempt evidence is missing')
    if (job.get('kind') != 'author' or original.get('kind') != 'author'
            or original.get('task') != job_id or original.get('base_commit') != base
            or original.get('run') != str(run) or original.get('report') is not None
            or descriptor.get('id') != job_id or descriptor.get('rules') != job.get('rules')
            or support.get('tasks', {}).get(job_id) != hashlib.sha256(data['descriptor']).hexdigest()):
        raise ValueError('attempt or descriptor identity differs')
    evaluation = receipt.get('evaluation')
    if (not isinstance(evaluation, dict)
            or started.get('receipt') != str(expected['model_receipt']) or receipt.get('task') != job_id
            or receipt.get('pid') != started.get('pid') or receipt.get('status') != 'completed'
            or type(receipt.get('returncode')) is not int or receipt['returncode'] != 0
            or type(receipt.get('started_unix')) not in (int, float) or receipt['started_unix'] <= 0
            or type(receipt.get('finished_unix')) not in (int, float)
            or not receipt['finished_unix'] >= receipt['started_unix'] or receipt.get('timed_out') is True
            or evaluation.get('passed') is not False
            or evaluation.get('error') != 'independent evaluation failed; inspect evaluation.log'):
        raise ValueError('model receipt is not completed with the independent evaluation failure')
    rules = job.get('rules', [])
    rows = final.get('rules') if isinstance(final, dict) else None
    decisions = original.get('dispositions')
    outcomes = result.get('outcomes')
    if (not rules or len(set(rules)) != len(rules) or not isinstance(rows, list) or len(rows) != len(rules)
            or any(not isinstance(value, dict) for value in rows)
            or {value.get('rule') for value in rows} != set(rules)
            or not isinstance(decisions, dict) or {value['rule']: value for value in rows} != decisions
            or not isinstance(outcomes, dict) or set(outcomes) != set(rules)
            or any(value.get('status') != 'proposed' or not isinstance(value.get('reason'), str)
                   or not value['reason'].strip() or not isinstance(value.get('evidence'), str)
                   or not value['evidence'].strip() for value in rows)
            or any(not isinstance(value, dict) or value.get('status') != 'needs-review'
                   or 'independent evaluation failed' not in str(value.get('reason')) for value in outcomes.values())):
        raise ValueError('model dispositions are incomplete or contain a separate review')
    log = data['evaluation_log']
    index_path = (b'/worktrees/' + attempt_id.encode() + b'/index.lock')
    index_error = any(b'fatal:' in line and b'/.git/worktrees/' in line and index_path in line
                      and b'write error. Out of diskspace' in line for line in log.splitlines())
    if (not index_error or b'Traceback (most recent call last):' not in log
            or b'changed_files' not in log or b"['git', 'diff', '--name-only']" not in log
            or b'CalledProcessError' not in log or b'non-zero exit status 128' not in log):
        raise ValueError('assigned evaluator log does not prove Git index ENOSPC')
    evaluations = run/'evaluations'/job_id
    if evaluations.is_symlink():
        raise ValueError('evaluation directory must not be a symlink')
    if evaluations.exists():
        children = list(evaluations.iterdir())
        if len(children) > 128 or any(child.is_symlink() or (child/'report.json').is_symlink()
                                      or (child/'report.json').exists() for child in children):
            raise ValueError('an independent evaluation report exists or evaluation state is ambiguous')


def _verify_batch_readmission(campaign, job_id, attempt_id, checkpoint, base_commit, job,
                             original, rejection, evidence):
    manifest_path, manifest_data = _proof_file(evidence.get('manifest'))
    validation_path, validation_data = _proof_file(evidence.get('validation'))
    log_path, log = _proof_file(evidence.get('repository_log'))
    isolation_path, isolation_data = _proof_file(evidence.get('isolation'))
    manifest, validation, isolation = map(json.loads, (manifest_data, validation_data, isolation_data))
    batch = manifest.get('id')
    if (not isinstance(batch, str) or not batch or Path(batch).name != batch
            or manifest_path.resolve() != (campaign/'.cache/batches'/batch/'manifest.json').resolve()
            or validation_path.resolve() != (manifest_path.parent/'validation.json').resolve()
            or log_path.resolve() != (manifest_path.parent/'repository-check.log').resolve()
            or not isolation_path.resolve().is_relative_to((campaign/'.cache').resolve())):
        raise ValueError('original batch evidence paths differ')
    included = [item for item in manifest.get('jobs', []) if item.get('status') == 'included']
    ids = [item.get('job_id') for item in included]
    matches = [item for item in included if item.get('job_id') == job_id and item.get('attempt_id') == attempt_id]
    if (not included or len(ids) != len(set(ids)) or len(matches) != 1
            or manifest.get('base_commit') != checkpoint
            or rejection.get('evidence', {}).get('manifest') != batch
            or rejection.get('evidence', {}).get('validation') != validation
            or validation.get('scope') != 'combined-batch' or validation.get('status') != 'failed'
            or validation.get('reason') != 'repository gates failed'
            or type(validation.get('repository_check_returncode')) is not int
            or validation['repository_check_returncode'] == 0
            or validation.get('candidate') != manifest.get('commit')
            or validation.get('source_checkpoint') != checkpoint
            or len(validation.get('included_jobs', [])) != len(ids)
            or set(validation['included_jobs']) != set(ids)):
        raise ValueError('batch is not the matching rejected combined integration')
    gates = _gates(log)
    if gates.get('families') != 'FAILED' or any(status == 'FAILED' for name, status in gates.items() if name != 'families'):
        raise ValueError('repository failure is not limited to families')
    culprit = isolation.get('culprit_job')
    if (isolation.get('kind') != 'isolated-ambiguity-regression' or isolation.get('manifest') != batch
            or isolation.get('candidate') != manifest.get('commit') or isolation.get('parent') != checkpoint
            or culprit not in ids or culprit == job_id or not isinstance(isolation.get('family'), str)
            or not isolation['family'] or not isinstance(isolation.get('case'), str) or not isolation['case']
            or not isinstance(isolation.get('reason'), str) or not isolation['reason'].strip()):
        raise ValueError('isolation evidence does not exclude the identified culprit')
    artifacts = isolation.get('evidence')
    if not isinstance(artifacts, dict):
        raise ValueError('isolation diagnostic artifacts are missing')
    config_path = campaign/'config.json'
    config = json.loads(config_path.read_bytes()) if config_path.exists() else {}
    allowed_caches = [(campaign/'.cache').resolve()]
    if isinstance(config.get('main_repo'), str):
        allowed_caches.append((Path(config['main_repo'])/'.cache').resolve())
    for key in ('base_log', 'trial_log', 'without_modifier_log', 'probe_log'):
        path, data = _proof_file(artifacts.get(key))
        if not data or not any(path.resolve().is_relative_to(cache) for cache in allowed_caches):
            raise ValueError('isolation diagnostic artifact is unavailable')
    item = matches[0]
    if (item.get('base_commit') != base_commit or item.get('rules') != job.get('rules')
            or item.get('kind') != original.get('kind')):
        raise ValueError('original proposal identity differs')
    if original.get('kind') == 'author':
        report, _ = _verify_original_report(campaign, job_id, attempt_id, base_commit, original, evidence.get('report'))
        if report.get('family_returncode') != 0:
            raise ValueError('author family check did not pass')
    elif original.get('kind') == 'mechanical':
        report = _verify_mechanical_readmission(campaign, job_id, attempt_id, base_commit, original, evidence.get('report'))
    else:
        raise ValueError('original proposal kind is unsupported')
    descriptor_path = campaign/'.cache/attempts'/attempt_id/'tasks'/f'{job_id}.json'
    descriptor = json.loads(descriptor_path.read_bytes())
    if descriptor.get('id') != job_id or descriptor.get('rules') != job.get('rules'):
        raise ValueError('individual descriptor does not match assigned rules')
    if (type(report.get('faces')) is not int or report['faces'] <= 0
            or item.get('descriptor_sha256') != report.get('descriptor_sha256')
            or item.get('support_sha256') != report.get('support_sha256')):
        raise ValueError('individual native/print report does not match the proposal')
    return {'manifest': batch, 'culprit_job': culprit, 'family': isolation['family'],
            'case': isolation['case'], 'requires_fresh_full_validation': True}


def _verify_mechanical_readmission(campaign, job_id, attempt_id, base_commit, original, report_spec, *, passed=True):
    report_path, data = _proof_file(report_spec)
    expected = campaign/'.cache/attempts'/attempt_id/'evaluations'/job_id
    if (report_path != Path(original.get('report', '')) or report_path.name != 'report.json'
            or not report_path.resolve().is_relative_to(expected.resolve())):
        raise ValueError('mechanical report is outside the assigned evaluation')
    report = json.loads(data)
    failures = report.get('failures')
    if (report.get('passed') is not passed or not isinstance(failures, list) or bool(failures) == passed
            or report.get('kind') != 'mechanical'
            or report.get('task') != job_id or report.get('source') != base_commit
            or report.get('hooks') != [] or Path(report.get('directory', '')).resolve() != report_path.parent.resolve()):
        raise ValueError('mechanical individual report did not pass')
    run = campaign/'.cache/attempts'/attempt_id
    descriptor, support = run/'tasks'/f'{job_id}.json', run/'support.json'
    for path, key in ((descriptor, 'descriptor_sha256'), (support, 'support_sha256')):
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != report.get(key):
            raise ValueError('mechanical evaluation artifact hash differs: '+key)
    if json.loads(support.read_bytes()).get('tasks', {}).get(job_id) != report['descriptor_sha256']:
        raise ValueError('mechanical support does not bind descriptor')
    grammar = report_path.parent/'grammar'
    if not grammar.is_dir():
        raise ValueError('mechanical evaluated grammar is missing')
    actual = {}
    for path in sorted(grammar.rglob('*')):
        if path.is_symlink():
            raise ValueError('mechanical evaluated grammar contains a symlink')
        if path.is_file():
            actual[str(path.relative_to(grammar))] = hashlib.sha256(path.read_bytes()).hexdigest()
    if actual != report.get('grammar'):
        raise ValueError('mechanical evaluated grammar changed')
    return report


def _verify_family_limit_revalidation(campaign, job_id, attempt_id, base_commit, original,
                                      rejection, evidence, checkpoint_evidence):
    manifest_path, manifest_bytes = _proof_file(evidence.get('previous_manifest'))
    validation_path, validation_bytes = _proof_file(evidence.get('validation'))
    log_path, log = _proof_file(evidence.get('previous_repo_log'))
    manifest, validation = json.loads(manifest_bytes), json.loads(validation_bytes)
    previous = rejection.get('evidence', {}).get('manifest')
    if (not isinstance(previous, str) or not previous or Path(previous).name != previous
            or manifest_path.resolve() != (campaign/'.cache/batches'/previous/'manifest.json').resolve()
            or validation_path.resolve() != (manifest_path.parent/'validation.json').resolve()
            or log_path.resolve() != (manifest_path.parent/'repository-check.log').resolve()):
        raise ValueError('original batch artifact paths differ')
    included = [item for item in manifest.get('jobs', []) if item.get('status') == 'included']
    ids = [item.get('job_id') for item in included]
    matches = [item for item in included if item.get('job_id') == job_id and item.get('attempt_id') == attempt_id]
    full = validation.get('full_checks', {})
    if (manifest.get('id') != previous or len(matches) != 1 or len(set(ids)) != len(ids)
            or len(validation.get('included_jobs', [])) != len(ids)
            or set(validation.get('included_jobs', [])) != set(ids)
            or validation != rejection.get('evidence', {}).get('validation')
            or validation.get('candidate') != manifest.get('commit')
            or validation.get('source_checkpoint') != manifest.get('base_commit')
            or validation.get('status') != 'failed' or validation.get('reason') != 'repository gates failed'
            or type(validation.get('repository_check_returncode')) is not int
            or validation['repository_check_returncode'] == 0
            or type(full.get('faces')) is not int or full['faces'] <= 0 or full.get('failures') != []):
        raise ValueError('original batch is not an included native-pass family-limit rejection')
    gates = _gates(log)
    if (gates.get('families') != 'FAILED' or any(value == 'FAILED' for key, value in gates.items() if key != 'families')
            or b'family-check' not in log or b'at most 64' not in log):
        raise ValueError('original repository failure is not limited to the family CLI')
    candidate = validation['candidate']
    if not isinstance(candidate, str) or not re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}', candidate):
        raise ValueError('audited candidate commit is invalid')
    config = json.loads((campaign/'config.json').read_bytes())
    try:
        registry_data = subprocess.check_output(['git', '-C', str(config['source_repo']), 'show',
                                                 candidate+':tests/families.json'], stderr=subprocess.PIPE)
        registry = json.loads(registry_data)
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError) as exc:
        raise ValueError('cannot verify original family registry') from exc
    if (not isinstance(registry, list) or len(registry) <= 64
            or any(not isinstance(x, str) or not x for x in registry) or len(set(registry)) != len(registry)):
        raise ValueError('original family registry does not prove a limit overflow')
    limit = checkpoint_evidence.get('family_registry_limit')
    if (checkpoint_evidence.get('family_case_modes') is not True or type(limit) is not int
            or limit < len(registry)):
        raise ValueError('latest checkpoint lacks sufficient family registry limit and case modes')
    report, _ = _verify_original_report(campaign, job_id, attempt_id, base_commit, original, evidence.get('report'))
    item = matches[0]
    if (item.get('base_commit') != base_commit or item.get('descriptor_sha256') != report.get('descriptor_sha256')
            or item.get('support_sha256') != report.get('support_sha256')):
        raise ValueError('manifest and original independent report differ')
    return item, len(registry)


def _verify_readmission_origin(db, checkpoint, origin):
    if origin == checkpoint:
        return
    expected = checkpoint
    for commit, parent in db.execute('SELECT commit_id,parent FROM checkpoints ORDER BY seq DESC'):
        if commit != expected:
            break
        if commit == origin:
            return
        expected = parent
    raise ValueError('proof origin is outside the accepted checkpoint chain')


def _reviewed_split_jobs(parent, checkpoint, origin, evidence):
    rules, selected, targets = parent['rules'], evidence.get('rules'), evidence.get('target_faces')
    if (not isinstance(selected, list) or not selected or any(not isinstance(x, str) for x in selected)
            or len(set(selected)) != len(selected) or not set(selected) < set(rules)
            or not isinstance(targets, list) or not targets or any(not isinstance(x, str) for x in targets)
            or len(set(targets)) != len(targets) or not set(targets) <= set(parent.get('target_faces', []))):
        raise ValueError('split needs a proper rule subset and assigned target faces')
    kept = [rule for rule in rules if rule in selected]
    deferred = [rule for rule in rules if rule not in selected]
    prefix = parent['id']+'\0'+checkpoint+'\0'
    selected_id = 'rehab-author-'+hashlib.sha256((prefix+'\0'.join(sorted(kept))).encode()).hexdigest()[:20]
    deferred_id = 'rehab-review-'+hashlib.sha256((prefix+'deferred').encode()).hexdigest()[:20]

    def child(job_id, part):
        fragments = [x for x in parent.get('source_fragments', []) if x.get('name') in part]
        return dict(parent, id=job_id, rules=part, source_fragments=fragments, review_origin=origin,
                    review_checkpoint=checkpoint)

    return dict(child(selected_id, kept), target_faces=targets), child(deferred_id, deferred)


def _verify_split_dispositions(original, owned, selected, repaired):
    dispositions = original.get('dispositions')
    if original.get('kind') != 'author' or not isinstance(dispositions, dict) or set(dispositions) != owned:
        raise ValueError('the original author dispositions do not cover its assignment')
    if any(not isinstance(row, dict) or row.get('rule') != name
           or row.get('status') not in ('proposed', 'needs-review') for name, row in dispositions.items()):
        raise ValueError('the original author dispositions are malformed')
    if not repaired and any(dispositions[name]['status'] != 'proposed' for name in selected):
        raise ValueError('the selected subset was not proposed in the original assignment')


def _family_split_cases(evidence, archived, family, selected, deferred):
    """Preserve the passing compositional cases and every archived refusal."""
    if (archived.get('family_returncode') != 1 or archived.get('scope') == 'author-target-preview'
            or archived.get('failures') != ['family regression tests failed']):
        raise ValueError('the original report lacks a single compositional family veto')
    gate, data = _proof_file(evidence.get('family_gate'))
    if gate.resolve() != gate or gate != Path(archived['directory'])/'family-check.log':
        raise ValueError('family veto proof is outside the original evaluation')
    cases = family['cases']
    try:
        result = json.loads(data.splitlines()[0])
        failures = result['failures']
        failed = {failure['name'] for failure in failures}
        pure_refusals = all(failure['why'].startswith('unexpected refusal: NoParse {') for failure in failures)
    except (ValueError, IndexError, KeyError, TypeError, AttributeError) as exc:
        raise ValueError('invalid archived family veto') from exc
    targets = set(selected['target_faces'])
    if (result.get('compositional') is not True or result.get('family') != family.get('family')
            or result.get('cases') != len(cases) or type(result.get('passed')) is not int
            or result['passed'] + len(failures) != len(cases) or len(failed) != len(failures)
            or not failed or not pure_refusals or failed & targets or not failed <= set(deferred['target_faces'])
            or any(sum(case['name'] == name for case in cases) != 1 for name in failed)):
        raise ValueError('archived family veto does not isolate the deferred side')
    positive = [case for case in cases if case['name'] in targets and case.get('expected') is not None
                and case.get('compositional', True) is True and not case.get('reject')]
    required = [*positive, *(case for case in cases if case.get('reject') is True)]
    return {case['name'] for case in positive}, required


def _verify_family_split_coverage(campaign, evidence, checkpoint_evidence, selected, deferred, positive=None):
    checkpoint = evidence['source_checkpoint']
    inputs, unused, _, digest = retire_rehabilitation.verify_scan(campaign, checkpoint, checkpoint_evidence,
                                                                  evidence.get('scan'))
    dependencies = json.loads(retire_rehabilitation._read(inputs/'scan/rules.json', digest)[0])['needed']
    targets = set(selected['target_faces'])
    if any(rule in unused or not dependencies.get(rule) for rule in selected['rules']):
        raise ValueError('each selected rule needs an accepted dependent face')
    if positive is not None and any(not positive.intersection(dependencies[rule]) for rule in selected['rules']):
        raise ValueError('each selected rule needs an archived passing family case')
    selected_faces = {name for rule in selected['rules'] for name in dependencies[rule]}
    deferred_faces = {name for rule in deferred['rules'] for name in dependencies.get(rule, ())}
    if selected_faces != targets or selected_faces & deferred_faces:
        raise ValueError('the accepted scan does not separate the selected and deferred faces')


def _verify_family_split_fixture(report, preserved):
    path = Path(report['directory'])/'family.json'
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != report['fixture_sha256']:
        raise ValueError('the repaired family fixture changed')
    current = json.loads(data)['cases']
    if any(case not in current for case in preserved):
        raise ValueError('the repaired family omitted an archived passing case or refusal')


class RehabilitationIneligible(ValueError):
    """A parked job is safely ineligible or changed while being checked."""


class Queue:
    def __init__(self, path, config=None):
        self.path = str(path)
        config = dict(config or {})
        maximum = config.get("max_workers", 12)
        if not isinstance(maximum, int) or isinstance(maximum, bool) or not 1 <= maximum <= 12:
            raise ValueError("max_workers must be between 1 and 12")
        with closing(self._connect()) as db:
            db.execute("PRAGMA journal_mode=WAL")
            db.executescript("""
                CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS jobs (
                    id TEXT PRIMARY KEY, payload TEXT NOT NULL, status TEXT NOT NULL,
                    attempt_id TEXT, adjudication TEXT
                );
                CREATE TABLE IF NOT EXISTS rules (
                    rule TEXT PRIMARY KEY, job_id TEXT NOT NULL REFERENCES jobs(id)
                );
                CREATE TABLE IF NOT EXISTS checkpoints (
                    seq INTEGER PRIMARY KEY AUTOINCREMENT, commit_id TEXT UNIQUE NOT NULL,
                    parent TEXT, evidence TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS attempts (
                    id TEXT PRIMARY KEY, job_id TEXT NOT NULL REFERENCES jobs(id),
                    worker TEXT NOT NULL, base_commit TEXT NOT NULL,
                    started TEXT, result TEXT
                );
                CREATE TABLE IF NOT EXISTS events (
                    seq INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL,
                    data TEXT NOT NULL, at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
            """)
            retire_priority.initialize(db)
            existing = db.execute("SELECT value FROM metadata WHERE key='config'").fetchone()
            if existing and config and existing[0] != _json(config):
                raise ValueError("queue configuration differs from existing configuration")
            if not existing:
                db.execute("INSERT INTO metadata VALUES ('config', ?)", (_json(config),))

    def _connect(self):
        db = sqlite3.connect(self.path, timeout=30, isolation_level=None)
        db.execute("PRAGMA foreign_keys=ON")
        db.row_factory = sqlite3.Row
        return db

    @contextmanager
    def _transaction(self):
        db = self._connect()
        try:
            db.execute("BEGIN IMMEDIATE")
            yield db
            db.commit()
        except BaseException:
            db.rollback()
            raise
        finally:
            db.close()

    @staticmethod
    def _event(db, kind, data):
        db.execute("INSERT INTO events(kind,data) VALUES (?,?)", (kind, _json(data)))

    def add_jobs(self, jobs):
        with self._transaction() as db:
            for job in jobs:
                job = dict(job)
                job_id, rules = job.get("id"), job.get("rules")
                if not isinstance(job_id, str) or not job_id or not isinstance(rules, list) or not rules:
                    raise ValueError("each job needs a nonempty id and rules list")
                if any(not isinstance(rule, str) or not rule for rule in rules) or len(set(rules)) != len(rules):
                    raise ValueError("rules must be distinct nonempty strings")
                try:
                    db.execute("INSERT INTO jobs(id,payload,status) VALUES (?,?,'pending')", (job_id, _json(job)))
                    db.executemany("INSERT INTO rules(rule,job_id) VALUES (?,?)", [(rule, job_id) for rule in rules])
                except sqlite3.IntegrityError as exc:
                    raise ValueError("duplicate job or rule ownership") from exc
                self._event(db, "job-added", job)

    def split_pending(self, jobid, groups, evidence):
        _evidence(evidence)
        groups = [dict(group) for group in groups]
        if not groups:
            raise ValueError("split requires groups")
        with self._transaction() as db:
            row = db.execute("SELECT status FROM jobs WHERE id=?", (jobid,)).fetchone()
            if not row or row[0] != "pending":
                raise ValueError("only pending jobs can be split")
            original = {r[0] for r in db.execute("SELECT rule FROM rules WHERE job_id=?", (jobid,))}
            assigned = []
            ids = []
            for group in groups:
                group_id, rules = group.get("id"), group.get("rules")
                if not isinstance(group_id, str) or not group_id or not isinstance(rules, list) or not rules:
                    raise ValueError("each group needs a nonempty id and rules list")
                if any(not isinstance(rule, str) or not rule for rule in rules):
                    raise ValueError("rules must be nonempty strings")
                ids.append(group_id)
                assigned.extend(rules)
            if len(set(ids)) != len(ids) or len(set(assigned)) != len(assigned) or set(assigned) != original:
                raise ValueError("groups must exactly partition the original rules with distinct ids")
            if db.execute("SELECT 1 FROM jobs WHERE id IN (%s) LIMIT 1" % ",".join("?" for _ in ids), ids).fetchone():
                raise ValueError("group job id already exists")
            for group in groups:
                db.execute("INSERT INTO jobs(id,payload,status) VALUES (?,?,'pending')", (group["id"], _json(group)))
                db.executemany("UPDATE rules SET job_id=? WHERE rule=?", [(group["id"], rule) for rule in group["rules"]])
            db.execute("UPDATE jobs SET status='superseded' WHERE id=?", (jobid,))
            self._event(db, "split", {"job_id": jobid, "groups": groups, "evidence": evidence})

    def split_reviewed_author(self, job_id, attempt_id, checkpoint, evidence):
        """Queue an independently evaluated subset and preserve its deferred siblings."""
        kind = evidence.get('kind') if isinstance(evidence, dict) else None
        integration = kind == 'reviewed-integration-repair-split'
        family_split = kind == 'reviewed-family-repair-split'
        unremoved = kind == 'reviewed-unremoved-draft-split'
        repaired = family_split or unremoved or kind == 'reviewed-author-repair-split'
        if (not (integration or repaired or kind == 'reviewed-author-split')
                or not isinstance(evidence.get('source_checkpoint'), str) or not evidence['source_checkpoint']
                or not isinstance(attempt_id, str) or not attempt_id):
            raise ValueError('reviewed author split evidence must bind the attempt and proof checkpoint')
        source = evidence['source_checkpoint']
        notes = evidence.get('review_notes')
        if not isinstance(notes, str) or not notes.strip() or len(notes) > 2048:
            raise ValueError('reviewed author split requires concise review notes')
        campaign = Path(self.path).parent
        status = 'integration-rejected' if integration else 'needs-review'

        def current(db):
            row = db.execute('SELECT status,attempt_id,payload,adjudication FROM jobs WHERE id=?',
                             (job_id,)).fetchone()
            if not row or row[0] != status or (row[1] != attempt_id and not (repaired and row[1] is None)):
                raise ValueError('parked author attempt or checkpoint changed')
            resolved = retire_rehabilitation.resolve_origin(db, job_id, row)
            if resolved is None or (resolved[1]['attempt_id'], resolved[1]['status']) != (attempt_id, status):
                raise ValueError('parked author attempt or checkpoint changed')
            return resolved

        with closing(self._connect()) as db:
            expected = current(db)
        with self._retry_lease(expected[1]['job_id']):
            with self._transaction() as db:
                head = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
                if not head or head[0] != checkpoint or current(db) != expected:
                    raise ValueError('parked author attempt or checkpoint changed')
                parent, origin, base, started, raw = expected
                owned = {item[0] for item in db.execute('SELECT rule FROM rules WHERE job_id=?', (job_id,))}
                if (parent.get('kind') != 'author' or parent.get('id') != job_id
                        or len(parent['rules']) != len(owned) or set(parent['rules']) != owned):
                    raise ValueError('the parked author no longer owns its original assignment')
                pid = json.loads(started or '{}').get('pid')
                if type(pid) is not int or pid <= 0:
                    raise ValueError('the parked author process identity is unknown')
                self._require_absent_pid(pid)
                _verify_readmission_origin(db, checkpoint, base)
                # Keep the child ID that its independently evaluated report binds.
                # The pending child still rebases and rechecks on the claim checkpoint.
                _verify_readmission_origin(db, checkpoint, source)
                selected, deferred = _reviewed_split_jobs(parent, source, origin, evidence)
                result = json.loads(raw)
                if integration and result['status'] != 'provisional-pass':
                    raise ValueError('integration split requires an originally passing proposal')
                original = result['evidence']
                _verify_split_dispositions(original, set(result['outcomes']), selected['rules'], repaired)
                origin_job = origin['job_id']
                options = {'passed': not repaired}
                if unremoved:
                    options['unremoved'] = True
                archived, family = _verify_original_report(campaign, origin_job, attempt_id, base, original,
                                                           evidence.get('original_report'), **options)
                if family_split or unremoved:
                    positive, preserved = (_family_split_cases(evidence, archived, family, selected, deferred)
                                           if family_split else (None, []))
                    (accepted,), = db.execute('SELECT evidence FROM checkpoints WHERE commit_id=?', (source,))
                    _verify_family_split_coverage(campaign, evidence, json.loads(accepted), selected, deferred, positive)
                elif archived.get('family_returncode') != 0 or archived.get('scope') == 'author-target-preview':
                    raise ValueError('the original report lacks a full author evaluation')
                repaired_report, task = _verify_reviewed_repair(campaign, source, selected, evidence)
                if family_split:
                    _verify_family_split_fixture(repaired_report, preserved)
                descriptor = campaign/'.cache/attempts'/attempt_id/'tasks'/f'{origin_job}.json'
                old = descriptor.read_bytes()
                if hashlib.sha256(old).hexdigest() != archived['descriptor_sha256']:
                    raise ValueError('the original author descriptor changed during review')
                if not set(json.loads(old)['cohort']) <= set(task['cohort']):
                    raise ValueError('the narrowed evaluation does not cover the original cohort')
                selected['goal'] += ' Coordinator replay passed the narrowed assignment. '+notes.strip()
                for child, status in ((selected, 'pending'), (deferred, 'needs-review')):
                    db.execute('INSERT INTO jobs(id,payload,status) VALUES (?,?,?)',
                               (child['id'], _json(child), status))
                    db.executemany('UPDATE rules SET job_id=? WHERE rule=? AND job_id=?',
                                   [(child['id'], rule, job_id) for rule in child['rules']])
                db.execute("UPDATE jobs SET status='superseded' WHERE id=?", (job_id,))
                receipt = {'parent_job':job_id, 'job_id':selected['id'], 'review_job':deferred['id'],
                           'origin_attempt':attempt_id, 'checkpoint':checkpoint, 'rules':selected['rules']}
                self._event(db, SPLIT_REVIEWED_AUTHOR, {**receipt, 'evidence':evidence})
                return receipt

    def rehabilitate_unused(self, job_id, checkpoint, evidence):
        """Move newly unused parked rules to fresh mechanical jobs atomically."""
        def current(db):
            return db.execute('SELECT status,attempt_id,payload,adjudication FROM jobs WHERE id=?', (job_id,)).fetchone()
        with closing(self._connect()) as db:
            row = current(db)
            latest = db.execute('SELECT commit_id,evidence FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
            if not row or row[0] not in ('needs-review', 'integration-rejected', 'failed') or not latest or latest[0] != checkpoint:
                raise RehabilitationIneligible('job is not parked or checkpoint is stale')
            resolved = retire_rehabilitation.resolve_origin(db, job_id, row)
            if resolved is None:
                raise RehabilitationIneligible('original attempt identity is unknown')
            payload, origin, base, started_raw, result_raw = resolved
            if payload.get('mechanical_failure_origin'):
                raise RehabilitationIneligible('diagnosed mechanical regression requires authoring')
            if base == checkpoint or payload.get('review_checkpoint') == checkpoint:
                raise RehabilitationIneligible('job was already evaluated or split at this checkpoint')
            snapshot = tuple(row)
            checkpoint_snapshot = tuple(latest)
        pid = (json.loads(started_raw) if started_raw else {}).get('pid')
        if type(pid) is not int or pid <= 0:
            raise RehabilitationIneligible('original worker PID is unknown')
        with self._retry_lease(origin['job_id'], unavailable_exc=RehabilitationIneligible):
            try:
                self._require_absent_pid(pid)
            except ValueError as exc:
                raise RehabilitationIneligible(str(exc)) from exc
            inputs, unused, manifest_hash, scan_hash = retire_rehabilitation.verify_scan(
                Path(self.path).parent, checkpoint, json.loads(checkpoint_snapshot[1]), evidence)
            rules = payload.get('rules')
            if not isinstance(rules, list) or not rules or len(set(rules)) != len(rules):
                raise ValueError('parked job has invalid rules')
            selected = [rule for rule in rules if rule in unused]
            if not selected:
                raise RehabilitationIneligible('no parked rules are unused at this checkpoint')
            retire_rehabilitation.verify_rule_receipts(inputs, selected, manifest_hash, scan_hash)
            with self._transaction() as db:
                new_latest = db.execute('SELECT commit_id,evidence FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
                new_row = current(db)
                if not new_latest or tuple(new_latest) != checkpoint_snapshot or not new_row or tuple(new_row) != snapshot:
                    raise RehabilitationIneligible('checkpoint or parked job changed')
                if retire_rehabilitation.resolve_origin(db, job_id, new_row) != resolved:
                    raise ValueError('original attempt changed during verification')
                verified = retire_rehabilitation.verify_scan(
                    Path(self.path).parent, checkpoint, json.loads(checkpoint_snapshot[1]), evidence)
                if verified != (inputs, unused, manifest_hash, scan_hash):
                    raise ValueError('checkpoint scan changed during verification')
                retire_rehabilitation.verify_rule_receipts(inputs, selected, manifest_hash, scan_hash)
                try:
                    self._require_absent_pid(pid)
                except ValueError as exc:
                    raise RehabilitationIneligible(str(exc)) from exc
                owned = {r[0] for r in db.execute('SELECT rule FROM rules WHERE job_id=?', (job_id,))}
                if owned != set(rules):
                    raise ValueError('parked rule ownership differs from payload')
                mechanical, remaining = [], [rule for rule in rules if rule not in unused]
                for rule in selected:
                    suffix = hashlib.sha256((job_id+'\0'+checkpoint+'\0'+rule).encode()).hexdigest()[:20]
                    child_id = 'rehab-mech-'+suffix
                    child = dict(payload, id=child_id, kind='mechanical', rules=[rule], target_faces=[],
                                 source_fragments=[x for x in payload.get('source_fragments', []) if x.get('name') == rule],
                                 rehabilitation_origin={'job_id': job_id, 'attempt_id': origin['attempt_id'],
                                                        'checkpoint': checkpoint})
                    child.pop('review_origin', None)
                    child.pop('review_checkpoint', None)
                    db.execute("INSERT INTO jobs(id,payload,status) VALUES (?,?,'pending')", (child_id, _json(child)))
                    db.execute('UPDATE rules SET job_id=? WHERE rule=? AND job_id=?', (child_id, rule, job_id))
                    mechanical.append(child_id)
                review_job = None
                if remaining:
                    suffix = hashlib.sha256((job_id+'\0'+checkpoint+'\0review').encode()).hexdigest()[:20]
                    review_job = 'rehab-review-'+suffix
                    review = dict(payload, id=review_job, rules=remaining, review_origin=origin,
                                  review_checkpoint=checkpoint,
                                  source_fragments=[x for x in payload.get('source_fragments', []) if x.get('name') in remaining])
                    db.execute("INSERT INTO jobs(id,payload,status) VALUES (?,?,'needs-review')", (review_job, _json(review)))
                    db.executemany('UPDATE rules SET job_id=? WHERE rule=? AND job_id=?',
                                   [(review_job, rule, job_id) for rule in remaining])
                db.execute("UPDATE jobs SET status='superseded' WHERE id=?", (job_id,))
                result = {'parent_job': job_id, 'origin_attempt': origin['attempt_id'], 'checkpoint': checkpoint,
                          'mechanical_jobs': mechanical, 'review_job': review_job, 'rules': selected,
                          'manifest_sha256': manifest_hash, 'scan_sha256': scan_hash}
                self._event(db, 'rehabilitated-unused', {**result, 'evidence': evidence})
                return result

    def queue_deferred_author(self, job_id, checkpoint, evidence):
        """Schedule the first standalone author attempt for an inherited review."""
        def current(db):
            row = db.execute('SELECT status,attempt_id,payload,adjudication FROM jobs WHERE id=?',
                             (job_id,)).fetchone()
            if not row or tuple(row[:2]) != ('needs-review', None):
                raise RehabilitationIneligible('the deferred job already has a standalone disposition')
            resolved = retire_rehabilitation.resolve_origin(db, job_id, row)
            if resolved is None or resolved[0].get('id') != job_id or resolved[0].get('kind') != 'author':
                raise RehabilitationIneligible('the deferred job has no verified author origin')
            return resolved

        latest = 'SELECT commit_id,evidence FROM checkpoints ORDER BY seq DESC LIMIT 1'
        with closing(self._connect()) as db:
            initial = current(db)
            head = db.execute(latest).fetchone()
            if not head or head[0] != checkpoint:
                raise RehabilitationIneligible('the accepted checkpoint changed')
        job, origin, base, started, _ = initial
        rules = job['rules']
        pid = json.loads(started or '{}').get('pid')
        if type(pid) is not int or pid <= 0:
            raise RehabilitationIneligible('the original worker PID is unknown')
        campaign = Path(self.path).parent
        inputs, unused, manifest_hash, scan_hash = retire_rehabilitation.verify_scan(
            campaign, checkpoint, json.loads(head[1]), evidence)
        targets = retire_rehabilitation.deferred_author_faces(inputs, scan_hash, unused, job)
        with self._retry_lease(origin['job_id'], unavailable_exc=RehabilitationIneligible):
            with self._transaction() as db:
                owned = {row[0] for row in db.execute('SELECT rule FROM rules WHERE job_id=?', (job_id,))}
                if db.execute(latest).fetchone() != head or current(db) != initial or owned != set(rules):
                    raise RehabilitationIneligible('the accepted checkpoint or deferred ownership changed')
                _verify_readmission_origin(db, checkpoint, base)
                finished = db.execute("SELECT unixepoch(at) FROM events WHERE kind='finished' "
                                      "AND json_extract(data,'$.attempt_id')=?", (origin['attempt_id'],)).fetchall()
                try:
                    self._require_absent_pid(pid, finished=finished[0][0] if len(finished) == 1 else None)
                except ValueError as exc:
                    raise RehabilitationIneligible(str(exc)) from exc
                archive = campaign/'.cache/attempts'/origin['attempt_id']
                goal = (job.get('goal', '')+' Author the remaining rules in this assignment. Preserve the scopes and '
                        f'meanings checked in earlier attempts. The original attempt evidence is under {archive}.')
                payload = dict(job, target_faces=targets, goal=goal)
                db.execute("UPDATE jobs SET payload=?,status='pending' WHERE id=?", (_json(payload), job_id))
                receipt = {'job_id': job_id, 'origin_attempt': origin['attempt_id'], 'checkpoint': checkpoint,
                           'rules': rules, 'target_faces': targets, 'manifest_sha256': manifest_hash, 'scan_sha256': scan_hash}
                self._event(db, 'queued-deferred-author', {**receipt, 'evidence': evidence})
                return receipt

    def escalate_mechanical_failure(self, job_id, attempt_id, checkpoint, evidence):
        """Schedule a fresh author attempt for a failure on the accepted chain."""
        if not isinstance(evidence, dict) or evidence.get('kind') != 'mechanical-author-escalation':
            raise ValueError('mechanical-author-escalation evidence is required')
        query = ('SELECT j.status,j.attempt_id,j.payload,a.base_commit,a.started,a.result,a.job_id FROM jobs j '
                 'JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?')
        campaign = Path(self.path).parent
        with self._retry_lease(job_id):
            with closing(self._connect()) as db:
                db.execute('BEGIN')
                row = db.execute(query, (job_id,)).fetchone()
                head = db.execute('SELECT commit_id,evidence FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
                if (not row or tuple(row[:2]) != ('needs-review', attempt_id)
                        or row[6] != job_id or not head or head[0] != checkpoint):
                    raise ValueError('mechanical attempt or checkpoint is stale')
                base = row[3]
                _verify_readmission_origin(db, checkpoint, base)
                source_evidence = db.execute('SELECT evidence FROM checkpoints WHERE commit_id=?', (base,)).fetchone()[0]
                snapshot, tip = tuple(row), tuple(head)
            payload, start, original = map(json.loads, (row[2], row[4], row[5]))
            rules = payload['rules']
            outcomes = original.get('outcomes', {})
            receipt = original.get('evidence', {})
            pid = start.get('pid')
            if (payload.get('kind') != 'mechanical' or len(rules) != 1
                    or original.get('status') != 'needs-review' or set(outcomes) != set(rules)
                    or outcomes[rules[0]].get('status') != 'needs-review' or receipt.get('kind') != 'mechanical'
                    or type(pid) is not int or pid <= 0):
                raise ValueError('one finished mechanical rule with diagnosed failure is required')
            self._require_absent_pid(pid)
            inputs, unused, manifest_hash, scan_hash = retire_rehabilitation.verify_scan(
                campaign, base, json.loads(source_evidence), evidence.get('scan'))
            if not set(rules) <= unused:
                raise ValueError('mechanical rule is absent from the unused audit')
            retire_rehabilitation.verify_rule_receipts(inputs, rules, manifest_hash, scan_hash)
            run = campaign/'.cache/attempts'/attempt_id
            report = _verify_mechanical_readmission(campaign, job_id, attempt_id, base, receipt,
                                                       evidence.get('report'), passed=False)
            faces = retire_rehabilitation.mechanical_failure(inputs, run, rules, report, evidence)
            suffix = hashlib.sha256((job_id+'\0'+attempt_id+'\0'+checkpoint).encode()).hexdigest()[:20]
            origin = {'job_id': job_id, 'attempt_id': attempt_id, 'checkpoint': base}
            goal = payload['goal']
            if not payload.get('mechanical_failure_origin'):
                goal += ' Preserve the readings and print gates of the diagnosed target faces.'
            child = dict(payload, id='repair-mech-'+suffix, kind='author', target_faces=faces,
                         requires_diagnosis=True, mechanical_failure_origin=origin, goal=goal)
            for key in ('rehabilitation_origin', 'review_origin', 'review_checkpoint'):
                child.pop(key, None)
            with self._transaction() as db:
                latest = db.execute('SELECT commit_id,evidence FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
                current = db.execute(query, (job_id,)).fetchone()
                owned = [rule for rule, in db.execute('SELECT rule FROM rules WHERE job_id=?', (job_id,))]
                if not latest or tuple(latest) != tip or not current or tuple(current) != snapshot or owned != rules:
                    raise ValueError('mechanical checkpoint, attempt or ownership changed')
                _verify_readmission_origin(db, checkpoint, base)
                proof = db.execute('SELECT evidence FROM checkpoints WHERE commit_id=?', (base,)).fetchone()
                if not proof or proof[0] != source_evidence:
                    raise ValueError('mechanical proof checkpoint changed')
                self._require_absent_pid(pid)
                _verify_mechanical_readmission(campaign, job_id, attempt_id, base, receipt,
                                                evidence.get('report'), passed=False)
                retire_rehabilitation.recheck_mechanical_failure(inputs, evidence)
                db.execute("INSERT INTO jobs(id,payload,status) VALUES (?,?,'pending')", (child['id'], _json(child)))
                db.execute("UPDATE rules SET job_id=? WHERE job_id=? AND rule=?", (child['id'], job_id, rules[0]))
                db.execute("UPDATE jobs SET status='superseded' WHERE id=?", (job_id,))
                result = {**origin, 'scheduled_checkpoint': checkpoint, 'child_job': child['id'],
                          'rules': rules, 'target_faces': faces, 'evidence': evidence}
                self._event(db, 'escalated-mechanical-failure', result)
                return result

    def publish_priorities(self, checkpoint, plan):
        """Publish one audited heuristic snapshot; do not change job ownership."""
        inputs = retire_priority.verify_plan_inputs(Path(self.path).parent, checkpoint, plan)
        with self._transaction() as db:
            result = retire_priority.publish(db, checkpoint, plan)
            if inputs != (result['metadata']['manifest_sha256'], result['metadata']['scan_sha256']):
                raise ValueError('priority audit input hashes differ')
            self._event(db, 'priority-published', result)
            return result

    def rehabilitate_timeout(self, job_id, attempt, checkpoint, evidence):
        """Recover a proved author timeout as bounded pending work."""
        return retire_timeout_recovery.rehabilitate(self, job_id, attempt, checkpoint, evidence)

    def add_checkpoint(self, commit, evidence, expected_parent=None):
        if not isinstance(commit, str) or not commit:
            raise ValueError("commit is required")
        encoded = _evidence(evidence)
        with self._transaction() as db:
            row = db.execute("SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1").fetchone()
            parent = row[0] if row else None
            if expected_parent != parent:
                raise ValueError("checkpoint parent conflict")
            try:
                db.execute("INSERT INTO checkpoints(commit_id,parent,evidence) VALUES (?,?,?)", (commit, parent, encoded))
            except sqlite3.IntegrityError as exc:
                raise ValueError("checkpoint already exists") from exc
            self._event(db, "checkpoint", {"commit": commit, "parent": parent, "evidence": evidence})

    def claim(self, worker, checkpoint=None, kinds=None):
        if not isinstance(worker, str) or not worker:
            raise ValueError("worker is required")
        with self._transaction() as db:
            latest = db.execute("SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1").fetchone()
            if not latest:
                raise ValueError("a checkpoint is required before claiming")
            base = latest[0]
            if checkpoint is not None and checkpoint != base:
                raise ValueError("checkpoint is stale")
            config = json.loads(db.execute("SELECT value FROM metadata WHERE key='config'").fetchone()[0])
            active = db.execute("SELECT count(*) FROM jobs WHERE status='running'").fetchone()[0]
            if active >= config.get("max_workers", 12):
                return None
            if db.execute("SELECT 1 FROM attempts a JOIN jobs j ON j.attempt_id=a.id WHERE j.status='running' AND a.worker=?", (worker,)).fetchone():
                raise ValueError("worker already owns a running job")
            if kinds is not None:
                if isinstance(kinds, str):
                    raise ValueError("kinds must be a collection")
                kinds = list(kinds)
                if not kinds or any(not isinstance(kind, str) or not kind for kind in kinds):
                    raise ValueError("kinds must contain nonempty strings")
            candidates = retire_priority.candidates(db, base)
            row = next((candidate for candidate in candidates
                        if kinds is None or json.loads(candidate[1]).get("kind") in kinds), None)
            if not row:
                return None
            job = json.loads(row[1])
            retire_priority.claimed(db, base, job)
            attempt = str(uuid.uuid4())
            db.execute("INSERT INTO attempts(id,job_id,worker,base_commit) VALUES (?,?,?,?)", (attempt, row[0], worker, base))
            db.execute("UPDATE jobs SET status='running',attempt_id=? WHERE id=?", (attempt, row[0]))
            result = {"attempt_id": attempt, "job": job, "base_commit": base}
            self._event(db, "claimed", {**result, "worker": worker})
            return result

    def mark_started(self, attempt, pid=None, receipt=None):
        if pid is None and receipt is None:
            raise ValueError("pid or receipt is required")
        details = {"pid": pid, "receipt": receipt}
        encoded = _json(details)
        with self._transaction() as db:
            row = db.execute("SELECT a.started,j.status FROM attempts a JOIN jobs j ON j.id=a.job_id WHERE a.id=? AND j.attempt_id=a.id", (attempt,)).fetchone()
            if not row or row[1] != "running":
                raise ValueError("attempt is not running")
            if row[0] is not None:
                if row[0] != encoded:
                    raise ValueError("start receipt is immutable")
                return
            db.execute("UPDATE attempts SET started=? WHERE id=?", (encoded, attempt))
            self._event(db, "started", {"attempt_id": attempt, **details})

    def finish(self, attempt, status, evidence, outcomes):
        if status not in ("provisional-pass", "needs-review", "failed"):
            raise ValueError("invalid finish status")
        _evidence(evidence)
        if not isinstance(outcomes, dict):
            raise ValueError("outcomes must map every rule to its disposition")
        result = {"status": status, "evidence": evidence, "outcomes": outcomes}
        encoded = _json(result)
        with self._transaction() as db:
            row = db.execute("SELECT a.job_id,a.result,j.status FROM attempts a JOIN jobs j ON j.id=a.job_id WHERE a.id=?", (attempt,)).fetchone()
            if not row:
                raise ValueError("unknown attempt")
            if row[1] is not None:
                if row[1] != encoded:
                    raise ValueError("attempt result is immutable")
                return
            if row[2] != "running":
                raise ValueError("attempt is not running")
            rules = {r[0] for r in db.execute("SELECT rule FROM rules WHERE job_id=?", (row[0],))}
            if set(outcomes) != rules:
                raise ValueError("outcomes must cover exactly every assigned rule")
            for outcome in outcomes.values():
                if not isinstance(outcome, dict) or outcome.get("status") not in ("pass", "needs-review") or not outcome.get("evidence"):
                    raise ValueError("each outcome needs a status and evidence")
                if outcome["status"] == "needs-review" and not outcome.get("reason"):
                    raise ValueError("needs-review outcome requires a reason")
            if status == "provisional-pass" and any(o["status"] != "pass" for o in outcomes.values()):
                raise ValueError("provisional pass requires every rule to pass")
            db.execute("UPDATE attempts SET result=? WHERE id=?", (encoded, attempt))
            db.execute("UPDATE jobs SET status=? WHERE id=?", (status, row[0]))
            self._event(db, "finished", {"attempt_id": attempt, **result})

    @contextmanager
    def _retry_lease(self, job_id, unavailable_exc=ValueError):
        if (not isinstance(job_id, str) or not job_id or job_id in (".", "..")
                or Path(job_id).name != job_id or "\x00" in job_id):
            raise ValueError("job ID must identify a single lease file")
        lease_path = Path(self.path).parent / "leases" / job_id
        try:
            fd = os.open(lease_path, os.O_RDWR | os.O_NOFOLLOW | os.O_CLOEXEC)
        except OSError as exc:
            raise unavailable_exc("existing attempt lease is unavailable") from exc
        try:
            if not stat.S_ISREG(os.fstat(fd).st_mode):
                raise ValueError("attempt lease is not a regular file")
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as exc:
                raise unavailable_exc("attempt lease is still held") from exc
            yield
        finally:
            os.close(fd)

    def readmit_proposal(self, job_id, attempt_id, checkpoint, evidence):
        """Return one independently passed, rejected proposal to batch review.

        The caller must independently establish that the cited gate failure is
        limited to coordinator-generated whole-card cases. This does not accept
        a proposal or waive any gate; the new batch reruns validation.
        """
        if not isinstance(evidence, dict) or evidence.get('kind') != 'family-case-mode-revalidation':
            raise ValueError('family-case-mode-revalidation evidence is required')
        # Files are checked again within the transaction, with the job lease held.
        with self._retry_lease(job_id):
            with self._transaction() as db:
                latest = db.execute('SELECT commit_id,evidence FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
                if not latest or latest[0] != checkpoint:
                    raise ValueError('checkpoint is stale')
                checkpoint_evidence = json.loads(latest[1])
                if not isinstance(checkpoint_evidence, dict) or checkpoint_evidence.get('family_case_modes') is not True:
                    raise ValueError('latest checkpoint does not prove family case modes')
                query = ('SELECT j.status,j.attempt_id,j.adjudication,j.payload,a.base_commit,a.started,a.result,a.job_id,a.worker '
                         'FROM jobs j JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?')
                row = db.execute(query, (job_id,)).fetchone()
                if not row or row[0] != 'integration-rejected' or row[1] != attempt_id or not row[6] or row[7] != job_id:
                    raise ValueError('readmission requires the current rejected attempt')
                start = json.loads(row[5]) if row[5] else {}
                pid = start.get('pid')
                if type(pid) is not int or pid <= 0:
                    raise ValueError('attempt process identity is unknown')
                try:
                    os.kill(pid, 0)
                except ProcessLookupError:
                    pass
                except OSError as exc:
                    if exc.errno != errno.ESRCH:
                        raise ValueError('attempt process state is unknown') from exc
                else:
                    raise ValueError('attempt process is still present')
                result = json.loads(row[6])
                rules = {r[0] for r in db.execute('SELECT rule FROM rules WHERE job_id=?', (job_id,))}
                payload_rules = json.loads(row[3])['rules']
                original = result.get('evidence', {})
                dispositions = original.get('dispositions', {}) if isinstance(original, dict) else {}
                outcomes = result.get('outcomes', {})
                if (not rules or len(payload_rules) != len(rules) or set(payload_rules) != rules
                        or result.get('status') != 'provisional-pass' or set(outcomes) != rules
                        or any(not isinstance(v, dict) or v.get('status') != 'pass' for v in outcomes.values())
                        or not isinstance(dispositions, dict) or set(dispositions) != rules
                        or any(not isinstance(v, dict) or v.get('status') != 'proposed'
                               or v.get('rule') != name for name, v in dispositions.items())
                        or original.get('kind') != 'author' or original.get('task') != job_id
                        or original.get('base_commit') != row[4]):
                    raise ValueError('original proposal lacks complete independently passed rule dispositions')
                rejection = json.loads(row[2]) if row[2] else {}
                _verify_readmission_artifacts(Path(self.path).parent, job_id, attempt_id, row[4],
                                             original, rejection, evidence)
                current = db.execute(query, (job_id,)).fetchone()
                if not current or tuple(current) != tuple(row):
                    raise ValueError('attempt changed during readmission verification')
                db.execute("UPDATE jobs SET status='provisional-pass',adjudication=NULL WHERE id=?", (job_id,))
                self._event(db, 'readmitted', {'job_id': job_id, 'attempt_id': attempt_id,
                            'checkpoint': checkpoint, 'pid': pid, 'process': 'absent',
                            'lease': 'acquired', 'previous_adjudication': rejection, 'evidence': evidence})

    def readmit_family_limit_proposal(self, job_id, attempt_id, checkpoint, evidence):
        """Readmit one native-passed proposal for complete restaging at the latest checkpoint.

        No audit is treated as proof of the new union; all trial, native and
        repository gates must run again. The rejected attempt stays immutable.
        """
        if not isinstance(evidence, dict) or evidence.get('kind') != 'family-limit-revalidation':
            raise ValueError('family-limit-revalidation evidence is required')
        with self._retry_lease(job_id):
            with self._transaction() as db:
                latest = db.execute('SELECT commit_id,evidence FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
                if not latest or latest[0] != checkpoint:
                    raise ValueError('checkpoint is stale')
                checkpoint_evidence = json.loads(latest[1])
                if not isinstance(checkpoint_evidence, dict):
                    raise ValueError('latest checkpoint has no family capability evidence')
                query = ('SELECT j.status,j.attempt_id,j.adjudication,j.payload,a.base_commit,a.started,a.result,a.job_id,a.worker '
                         'FROM jobs j JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?')
                row = db.execute(query, (job_id,)).fetchone()
                if not row or row[0] != 'integration-rejected' or row[1] != attempt_id or not row[6] or row[7] != job_id:
                    raise ValueError('readmission requires the current rejected attempt')
                pid = (json.loads(row[5]) if row[5] else {}).get('pid')
                if type(pid) is not int or pid <= 0:
                    raise ValueError('attempt process identity is unknown')
                try:
                    os.kill(pid, 0)
                except ProcessLookupError:
                    pass
                except OSError as exc:
                    if exc.errno != errno.ESRCH:
                        raise ValueError('attempt process state is unknown') from exc
                else:
                    raise ValueError('attempt process is still present')
                result = json.loads(row[6])
                rules = {record[0] for record in db.execute('SELECT rule FROM rules WHERE job_id=?', (job_id,))}
                payload_rules = json.loads(row[3])['rules']
                original = result.get('evidence', {})
                dispositions = original.get('dispositions', {}) if isinstance(original, dict) else {}
                outcomes = result.get('outcomes', {})
                if (not rules or len(payload_rules) != len(rules) or set(payload_rules) != rules
                        or result.get('status') != 'provisional-pass' or set(outcomes) != rules
                        or any(not isinstance(value, dict) or value.get('status') != 'pass' for value in outcomes.values())
                        or not isinstance(dispositions, dict) or set(dispositions) != rules
                        or any(not isinstance(value, dict) or value.get('status') != 'proposed'
                               or value.get('rule') != name for name, value in dispositions.items())
                        or original.get('kind') != 'author' or original.get('task') != job_id
                        or original.get('base_commit') != row[4]):
                    raise ValueError('original proposal lacks complete independently passed rule dispositions')
                rejection = json.loads(row[2]) if row[2] else {}
                item, registry_count = _verify_family_limit_revalidation(
                    Path(self.path).parent, job_id, attempt_id, row[4], original, rejection, evidence,
                    checkpoint_evidence)
                if len(item.get('rules', [])) != len(rules) or set(item.get('rules', [])) != rules:
                    raise ValueError('original manifest rule ownership differs')
                current = db.execute(query, (job_id,)).fetchone()
                if not current or tuple(current) != tuple(row):
                    raise ValueError('attempt changed during readmission verification')
                db.execute("UPDATE jobs SET status='provisional-pass',adjudication=NULL WHERE id=?", (job_id,))
                self._event(db, 'readmitted-family-limit', {'job_id': job_id, 'attempt_id': attempt_id,
                            'checkpoint': checkpoint, 'old_registry_count': registry_count,
                            'pid': pid, 'process': 'absent', 'lease': 'acquired',
                            'previous_adjudication': rejection, 'evidence': evidence})

    def retry_reviewed_repair(self, job_id, attempt_id, checkpoint, evidence):
        """Retry a verified repair, with bounded recovery for later diagnostic failures."""
        if not isinstance(evidence, dict) or evidence.get('kind') != 'reviewed-author-repair':
            raise ValueError('reviewed-author-repair evidence is required')
        notes = evidence.get('review_notes')
        if not isinstance(notes, str) or not notes.strip() or len(notes) > 2048:
            raise ValueError('reviewed author repair requires a concise reproduction note')
        routes = [key for key in REVIEWED_RETRY_EVENTS if key is not None and key in evidence]
        if len(routes) > 1:
            raise ValueError('one diagnostic recovery route is allowed per attempt')
        route = routes[0] if routes else None
        interrupted = route == 'interrupted_diagnostic'
        legacy = route == 'legacy_diagnostic_prompt'
        staging = route == 'staging_failure'
        staging_proof = evidence.get('staging_failure')
        isolated_parse = staging and isinstance(staging_proof, dict) and 'readmission_event_seq' in staging_proof
        rewritten = staging and isinstance(staging_proof, dict) and 'rewritten_draft' in staging_proof
        if isolated_parse and rewritten:
            raise ValueError('staging repair proof mixes distinct retry cases')
        recovery = 'rewritten' if rewritten else 'parse' if isolated_parse else 'ordinary'
        full_check = route == 'full_check_failure'
        if interrupted and 'timeout' in evidence:
            raise ValueError('interrupted diagnostic recovery requires a fresh independent report')
        if legacy and 'timeout' not in evidence:
            raise ValueError('legacy full diagnostic recovery requires a protected passing timeout')
        if (staging or full_check) and 'timeout' in evidence:
            raise ValueError('an integration rejection requires a fresh independent repair')
        event = REVIEWED_RETRY_EVENTS[route]
        with self._retry_lease(job_id):
            with self._transaction() as db:
                row = db.execute(
                    'SELECT j.status,j.attempt_id,j.payload,j.adjudication,a.base_commit,a.started,a.result '
                    'FROM jobs j JOIN attempts a ON a.id=j.attempt_id AND a.job_id=j.id WHERE j.id=?',
                    (job_id,)).fetchone()
                latest = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
                if (not row or row[0] not in ('needs-review', 'integration-rejected') or row[1] != attempt_id
                        or not row[6] or not latest or latest[0] != checkpoint):
                    raise ValueError('reviewed attempt or checkpoint changed')
                job, start, result = map(json.loads, (row[2], row[5] or '{}', row[6]))
                rules = {item[0] for item in db.execute('SELECT rule FROM rules WHERE job_id=?', (job_id,))}
                expected_status = 'needs-review' if row[0] == 'needs-review' else 'provisional-pass'
                if (job.get('id') != job_id or job.get('kind') != 'author' or not rules
                        or len(job['rules']) != len(rules) or set(job['rules']) != rules
                        or result.get('status') != expected_status or set(result.get('outcomes', {})) != rules):
                    raise ValueError('completed author assignment or rule coverage differs')
                # A solo staging veto and a full parse audit have separate retry budgets.
                retries = {(kind, slot) for kind, slot in db.execute(
                    "SELECT kind, CASE WHEN json_type(data,'$.evidence.staging_failure.rewritten_draft') IS NOT NULL "
                    "THEN 'rewritten' WHEN json_type(data,'$.evidence.staging_failure.readmission_event_seq') "
                    "IS NOT NULL THEN 'parse' ELSE 'ordinary' END "
                    f"FROM events WHERE kind IN ({_REVIEWED_EVENT_SLOTS}) AND json_extract(data,'$.job_id')=?",
                    (*_REVIEWED_EVENTS, job_id))}
                split = (SPLIT_REVIEWED_AUTHOR, 'ordinary') in retries
                if (event, recovery) in retries or (route is None and split):
                    raise ValueError('reviewed repair retry already used for this job')
                reviewed = (REVIEWED_RETRY_EVENTS[None], 'ordinary') in retries or split
                if (interrupted or legacy) and (not reviewed or row[0] != 'needs-review'):
                    raise ValueError('diagnostic recovery requires a previously retried author repair')
                if staging and (not reviewed or row[0] != 'integration-rejected'):
                    raise ValueError('staging recovery requires a previously retried integration rejection')
                if full_check and row[0] != 'integration-rejected':
                    raise ValueError('full-check recovery requires an integration rejection')
                pid = start.get('pid')
                if type(pid) is not int or pid <= 0:
                    raise ValueError('attempt process identity is unknown')
                completion = db.execute("SELECT unixepoch(at) FROM events WHERE kind='finished' "
                                        "AND json_extract(data,'$.attempt_id')=?", (attempt_id,)).fetchall()
                self._require_absent_pid(pid, finished=completion[0][0] if len(completion)==1 else None)
                _verify_readmission_origin(db, checkpoint, row[4])
                source = evidence.get('source_checkpoint', checkpoint)
                _verify_readmission_origin(db, checkpoint, source)
                if rewritten and (source != checkpoint or row[4] != source or 'source_checkpoint' not in evidence):
                    raise ValueError('rewritten draft recovery requires its exact reviewed checkpoint')
                campaign = Path(self.path).parent
                if staging:
                    rejection = json.loads(row[3])
                    if isolated_parse:
                        _verify_isolated_repair(db, campaign, source, (job_id, attempt_id), rejection, staging_proof)
                    else:
                        _verify_staging_repair(campaign, job_id, attempt_id, source, rejection, staging_proof)
                if full_check:
                    _verify_isolated_repair(db, campaign, source, (job_id, attempt_id), json.loads(row[3]),
                                            evidence['full_check_failure'], full_check=True)
                timed_out_attempt = None
                if interrupted:
                    state = retire_timeout_recovery.TimeoutState(row[4], job, row[5], row[6])
                    retire_timeout_recovery.verify_interrupted_diagnostic(
                        campaign, job_id, attempt_id, state, evidence['interrupted_diagnostic'])
                if 'timeout' in evidence:
                    if row[0] != 'needs-review' or source != row[4]:
                        raise ValueError('timeout evaluation must match the current rejected author attempt and its base')
                    timeout = evidence['timeout']
                    state = retire_timeout_recovery.TimeoutState(source, job, row[5], row[6])
                    retire_timeout_recovery.verify(campaign, job_id, attempt_id, state, timeout)
                    receipt = campaign/'.cache/attempts'/attempt_id/'workers'/job_id/'receipt.json'
                    model = json.loads(retire_timeout_recovery.artifact(timeout, 'model_receipt', receipt))
                    report_path, _ = _proof_file(evidence.get('report'))
                    if model.get('evaluation') != {'passed': True, 'directory': str(report_path.parent)}:
                        raise ValueError('passed evaluation differs from the protected timeout receipt')
                    if legacy:
                        retire_timeout_recovery.verify_legacy_full_diagnostic(
                            campaign, job_id, attempt_id, timeout, evidence['legacy_diagnostic_prompt'])
                    timed_out_attempt = attempt_id
                approved, _ = _verify_reviewed_repair(campaign, source, job, evidence, timed_out_attempt=timed_out_attempt)
                if rewritten:
                    _verify_rewritten_review(db, campaign, (job_id, attempt_id, source),
                                             start, result, evidence, approved)
                job['goal'] += ' Coordinator replay passed this repair on '+source+'. '+notes.strip()
                db.execute("UPDATE jobs SET payload=?,status='pending',attempt_id=NULL,adjudication=NULL WHERE id=?",
                           (_json(job), job_id))
                self._event(db, event, {'job_id': job_id, 'attempt_id': attempt_id,
                    'previous_status': row[0], 'previous_adjudication': json.loads(row[3]) if row[3] else None,
                    'checkpoint': checkpoint, 'pid': pid, 'evidence': evidence})

    def retry_failed(self, job_id, attempt_id, evidence, max_retries):
        """Requeue a finished failure only after proving its process and lease are gone.

        A live or reused PID, unknown start, missing lease or held lease refuses
        recovery. The kernel lock stays held through the transactional update.
        """
        _evidence(evidence)
        if type(max_retries) is not int or max_retries < 0:
            raise ValueError("max_retries must be a nonnegative integer")
        with self._retry_lease(job_id):
            with self._transaction() as db:
                row = db.execute(
                    "SELECT j.status,j.attempt_id,a.started,a.result FROM jobs j "
                    "JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?", (job_id,)).fetchone()
                if not row or row[0] != "failed" or row[1] != attempt_id or not row[3]:
                    raise ValueError("retry requires the current finished failed attempt")
                start = json.loads(row[2]) if row[2] else {}
                pid = start.get("pid")
                if type(pid) is not int or pid <= 0:
                    raise ValueError("attempt process identity is unknown")
                try:
                    os.kill(pid, 0)
                except ProcessLookupError:
                    pass
                except OSError as exc:
                    if exc.errno != errno.ESRCH:
                        raise ValueError("attempt process state is unknown") from exc
                else:
                    raise ValueError("attempt process is still present")
                attempts = db.execute("SELECT count(*) FROM attempts WHERE job_id=?", (job_id,)).fetchone()[0]
                if attempts - 1 >= max_retries:
                    raise ValueError("retry limit reached")
                db.execute("UPDATE jobs SET status='pending',attempt_id=NULL WHERE id=?", (job_id,))
                self._event(db, "retry", {"job_id": job_id, "attempt_id": attempt_id,
                                          "pid": pid, "process": "absent", "lease": "acquired",
                                          "retry_number": attempts, "evidence": evidence})

    def retry_tainted(self, job_id, attempt_id, evidence):
        """Retry only an explicitly verified read-only CPU-pool infrastructure failure.

        The controller must independently verify its terminal invocation and the
        provenance of the cited artifact. An absent recorded PID and an unlocked
        job lease are checked again here while the queue transaction is held.
        """
        _evidence(evidence)
        terminal = evidence.get("controller_terminal") if isinstance(evidence, dict) else None
        failure = evidence.get("infrastructure_failure") if isinstance(evidence, dict) else None
        if (not isinstance(terminal, dict)
                or not isinstance(terminal.get("unit"), str) or not terminal["unit"]
                or not isinstance(terminal.get("invocation"), str) or not terminal["invocation"]
                or not isinstance(terminal.get("verification"), dict) or not terminal["verification"]
                or not isinstance(failure, dict)
                or failure.get("kind") != "sandbox-cpu-pool-read-only"
                or not isinstance(failure.get("artifact"), str) or not failure["artifact"]
                or not isinstance(failure.get("sha256"), str) or len(failure["sha256"]) != 64):
            raise ValueError("terminal controller and infrastructure failure evidence required")
        try:
            path = Path(failure["artifact"])
            if not path.is_file() or path.stat().st_size > 32 * 1024 * 1024:
                raise ValueError("infrastructure artifact is unavailable or too large")
            artifact = path.read_bytes()
        except OSError as exc:
            raise ValueError("infrastructure artifact is unavailable") from exc
        if (hashlib.sha256(artifact).hexdigest() != failure["sha256"]
                or b"Errno 30" not in artifact or b"allocator.lock" not in artifact):
            raise ValueError("infrastructure artifact does not prove the stated failure")
        with self._retry_lease(job_id):
            with self._transaction() as db:
                row = db.execute(
                    "SELECT j.status,j.attempt_id,a.started,a.result FROM jobs j "
                    "JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?", (job_id,)).fetchone()
                if not row or row[0] not in ("needs-review", "failed") or row[1] != attempt_id or not row[3]:
                    raise ValueError("retry requires the current terminal review or failed attempt")
                start = json.loads(row[2]) if row[2] else {}
                pid = start.get("pid")
                if type(pid) is not int or pid <= 0:
                    raise ValueError("attempt process identity is unknown")
                try:
                    os.kill(pid, 0)
                except ProcessLookupError:
                    pass
                except OSError as exc:
                    if exc.errno != errno.ESRCH:
                        raise ValueError("attempt process state is unknown") from exc
                else:
                    raise ValueError("attempt process is still present")
                db.execute("UPDATE jobs SET status='pending',attempt_id=NULL WHERE id=?", (job_id,))
                self._event(db, "retry-tainted", {"job_id": job_id, "attempt_id": attempt_id,
                                                  "pid": pid, "process": "absent", "lease": "acquired",
                                                  "evidence": evidence})

    def readmit_batch_proposal(self, job_id, attempt_id, checkpoint, evidence):
        """Return one individually passed proposal to fresh integration review."""
        if not isinstance(evidence, dict) or evidence.get('kind') != 'isolated-batch-family-revalidation':
            raise ValueError('isolated batch revalidation evidence is required')
        with self._retry_lease(job_id):
            with self._transaction() as db:
                latest = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
                if not latest or latest[0] != checkpoint:
                    raise ValueError('checkpoint is stale')
                query = ('SELECT j.status,j.attempt_id,j.payload,j.adjudication,a.base_commit,a.started,a.result,a.job_id,a.worker '
                         'FROM jobs j JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?')
                row = db.execute(query, (job_id,)).fetchone()
                if not row or row[0] != 'integration-rejected' or row[1] != attempt_id or not row[6] or row[7] != job_id:
                    raise ValueError('readmission requires the current rejected attempt')
                job, rejection, original = map(json.loads, (row[2], row[3], row[6]))
                started = json.loads(row[5]) if row[5] else {}
                pid = started.get('pid')
                if type(pid) is not int or pid <= 0:
                    raise ValueError('attempt process identity is unknown')
                self._require_absent_pid(pid)
                rules = job['rules']
                decisions = original.get('evidence', {}).get('dispositions', {})
                outcomes = original.get('outcomes', {})
                if (original.get('status') != 'provisional-pass' or set(decisions) != set(rules)
                        or set(outcomes) != set(rules)
                        or any(not isinstance(decisions[rule], dict)
                               or decisions[rule].get('rule', rule if job.get('kind') == 'mechanical' else None) != rule
                               or decisions[rule].get('status') != 'proposed' or not decisions[rule].get('reason')
                               or not decisions[rule].get('evidence') or not isinstance(outcomes[rule], dict)
                               or outcomes[rule].get('status') != 'pass' or not outcomes[rule].get('evidence')
                               for rule in rules)):
                    raise ValueError('original per-rule dispositions do not all pass')
                audit = _verify_batch_readmission(Path(self.path).parent, job_id, attempt_id,
                                                    checkpoint, row[4], job, original['evidence'],
                                                    rejection, evidence)
                for event in db.execute("SELECT data FROM events WHERE kind IN ('readmitted-batch','readmitted-native-batch','readmitted-library-batch','readmitted-native-exit-one-batch','readmitted-parse-screen-batch')"):
                    previous = json.loads(event[0])
                    if previous.get('job_id') == job_id and previous.get('manifest') == audit['manifest']:
                        raise ValueError('batch readmission already used for this job')
                current = db.execute(query, (job_id,)).fetchone()
                if not current or tuple(current) != tuple(row):
                    raise ValueError('attempt changed during readmission verification')
                db.execute("UPDATE jobs SET status='provisional-pass',adjudication=NULL WHERE id=?", (job_id,))
                self._event(db, 'readmitted-batch', {'job_id': job_id, 'attempt_id': attempt_id,
                            'checkpoint': checkpoint, 'manifest': audit['manifest'], 'proof': audit,
                            'previous_adjudication': rejection, 'evidence': evidence})

    def readmit_library_batch_proposals(self, job_ids, checkpoint, evidence):
        """Return library-isolated jobs on the current checkpoint from a verified accepted ancestor."""
        return self._readmit_shared_batch(job_ids, checkpoint, evidence, mode='library')

    def readmit_native_exit_one_batch_proposals(self, job_ids, checkpoint, evidence):
        """Atomically return native exit-one isolated jobs for fresh validation."""
        return self._readmit_shared_batch(job_ids, checkpoint, evidence, mode='native-exit-one')

    def readmit_parse_screen_batch_proposals(self, job_ids, checkpoint, evidence):
        """Return parse-isolated proposals for a fresh trial on the current checkpoint.

        An optional source_checkpoint identifies the accepted ancestor for the
        original rejection and its isolation proof.
        """
        return self._readmit_shared_batch(job_ids, checkpoint, evidence, mode='parse-screen')

    def readmit_full_check_batch_proposals(self, job_ids, checkpoint, evidence):
        """Return an isolated archived full-check cohort to fresh validation."""
        return self._readmit_shared_batch(job_ids, checkpoint, evidence, mode='full-check')

    def readmit_shape_baseline_proposals(self, job_ids, checkpoint, evidence):
        """Requeue the exact batch after verifying a stale print-shape baseline."""
        return self._readmit_shared_batch(job_ids, checkpoint, evidence, mode='shape')

    def _readmit_shared_batch(self, job_ids, checkpoint, evidence, *, mode):
        """Keep leases and compare-and-swap all protected attempts together."""
        if (not isinstance(job_ids, list) or not 1 <= len(job_ids) <= 96
                or any(not isinstance(name, str) or not name for name in job_ids)
                or len(set(job_ids)) != len(job_ids)):
            raise ValueError('readmission requires 1–96 distinct job IDs')
        kind = {'library': 'isolated-batch-library-revalidation',
                'native-exit-one': 'isolated-native-exit-one-revalidation',
                'parse-screen': 'isolated-parse-screen-revalidation',
                'full-check': 'isolated-full-check-revalidation',
                'shape': 'stale-shape-baseline-revalidation'}[mode]
        cached = mode == 'library' and isinstance(evidence, dict) and evidence.get('kind') == 'stale-schema-test-cache-revalidation'
        allowed_kinds = (kind, 'isolated-shape-revalidation') if mode == 'shape' else (kind,)
        if not cached and (not isinstance(evidence, dict) or evidence.get('kind') not in allowed_kinds):
            raise ValueError('matching isolated batch revalidation evidence is required')
        verify_archived_replay = mode in ('parse-screen', 'full-check')
        origin = evidence.get('source_checkpoint', checkpoint) if verify_archived_replay or mode in ('library','shape') else checkpoint
        if not isinstance(origin, str) or not origin or Path(origin).name != origin:
            raise ValueError('invalid batch readmission origin')
        reports = evidence.get('reports')
        if not isinstance(reports, dict) or set(reports) != set(job_ids):
            raise ValueError('individual report evidence must cover every selected job')
        query = ('SELECT j.status,j.attempt_id,j.payload,j.adjudication,a.base_commit,a.started,a.result,a.job_id,a.worker '
                 'FROM jobs j JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?')
        with ExitStack() as leases:
            for name in sorted(job_ids):
                leases.enter_context(self._retry_lease(name))
            with closing(self._connect()) as db:
                db.execute('BEGIN')
                latest = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
                if not latest or latest[0] != checkpoint:
                    raise ValueError('checkpoint is stale')
                _verify_readmission_origin(db, checkpoint, origin)
                rows = {name: db.execute(query, (name,)).fetchone() for name in job_ids}
                owners = {name: [x[0] for x in db.execute('SELECT rule FROM rules WHERE job_id=? ORDER BY rule', (name,))]
                          for name in job_ids}
                attempts = [row[1] for row in rows.values() if row]
                slots = ','.join('?' for _ in attempts)
                finished = dict(db.execute("SELECT json_extract(data,'$.attempt_id'), min(unixepoch(at)) "
                    "FROM events WHERE kind='finished' "
                    f"AND json_extract(data,'$.attempt_id') IN ({slots}) GROUP BY 1 HAVING count(*)=1", attempts))
            snapshots, rejections, pids = {}, {}, {}
            for name, row in rows.items():
                if not row or row[0] != 'integration-rejected' or not row[1] or not row[6] or row[7] != name:
                    raise ValueError('readmission requires current rejected attempts')
                snapshots[name] = tuple(row)
                job, rejection, original = map(json.loads, (row[2], row[3], row[6]))
                pid = (json.loads(row[5]) if row[5] else {}).get('pid')
                if type(pid) is not int or pid <= 0:
                    raise ValueError('attempt process identity is unknown')
                self._require_absent_pid(pid, finished=finished.get(row[1]))
                pids[name] = pid
                rules = job['rules']
                if not rules or len(set(rules)) != len(rules) or sorted(rules) != owners[name]:
                    raise ValueError('current readmission rule ownership differs')
                dispositions, outcomes = original.get('evidence', {}).get('dispositions', {}), original.get('outcomes', {})
                if (original.get('status') != 'provisional-pass' or set(dispositions) != set(rules)
                        or set(outcomes) != set(rules)
                        or any(not isinstance(dispositions[rule], dict)
                               or dispositions[rule].get('rule', rule if job.get('kind') == 'mechanical' else None) != rule
                               or dispositions[rule].get('status') != 'proposed' or not dispositions[rule].get('reason')
                               or not dispositions[rule].get('evidence') or not isinstance(outcomes[rule], dict)
                               or outcomes[rule].get('status') != 'pass' or not outcomes[rule].get('evidence')
                               for rule in rules)):
                    raise ValueError('original per-rule dispositions do not all pass')
                rejections[name] = rejection
                if mode=='shape':
                    address, data = _proof_file(reports[name])
                    if (str(address)!=original['evidence'].get('report')
                            or json.loads(data).get('passed') is not True):
                        raise ValueError('shape readmission original passing report differs')
            campaign = Path(self.path).parent
            selected = {name: row[1] for name, row in rows.items()}
            verifier = {'library': retire_library_readmission, 'native-exit-one': retire_native_exit_readmission,
                        'parse-screen': retire_parse_readmission, 'full-check': retire_parse_readmission}.get(mode)
            proof = (_verify_shape_readmission(campaign, selected, origin, rejections, evidence) if mode=='shape'
                     else verifier.verify(campaign, selected, origin, rejections, evidence, _proof_file, _gates))
            if origin != checkpoint:
                proof['source_checkpoint'] = origin
            with self._transaction() as db:
                latest = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
                if not latest or latest[0] != checkpoint:
                    raise ValueError('checkpoint changed during batch proof verification')
                _verify_readmission_origin(db, checkpoint, origin)
                previous_readmissions = set()
                for event in db.execute("SELECT data FROM events WHERE kind IN "
                                        "('readmitted-batch','readmitted-native-batch','readmitted-library-batch','readmitted-native-exit-one-batch','readmitted-parse-screen-batch','readmitted-shape-baseline')"):
                    previous = json.loads(event[0])
                    previous_readmissions.add((previous.get('job_id'), previous.get('manifest')))
                for name in job_ids:
                    current = db.execute(query, (name,)).fetchone()
                    owned = [x[0] for x in db.execute('SELECT rule FROM rules WHERE job_id=? ORDER BY rule', (name,))]
                    if not current or tuple(current) != snapshots[name]:
                        raise ValueError('attempt changed during batch proof verification')
                    if owned != owners[name]:
                        raise ValueError('ownership changed during batch proof verification')
                    if (name, proof['manifest']) in previous_readmissions:
                        raise ValueError('batch readmission already used for this job')
                    self._require_absent_pid(pids[name], finished=finished.get(current[1]))
                if verify_archived_replay:
                    verifier.recheck(proof)
                for name in job_ids:
                    db.execute("UPDATE jobs SET status='provisional-pass',adjudication=NULL WHERE id=?", (name,))
                    event = {'library': 'readmitted-library-batch', 'native-exit-one': 'readmitted-native-exit-one-batch',
                             'parse-screen': 'readmitted-parse-screen-batch','shape':'readmitted-shape-baseline',
                             'full-check': 'readmitted-native-batch'}[mode]
                    self._event(db, event, {'job_id': name, 'attempt_id': snapshots[name][1],
                                'checkpoint': checkpoint, 'manifest': proof['manifest'], 'proof': proof,
                                'previous_adjudication': rejections[name], 'evidence': evidence})
            return {'jobs': list(job_ids), 'checkpoint': checkpoint, 'proof': proof}

    def readmit_native_batch_proposal(self, job_id, attempt_id, checkpoint, evidence):
        """Return one independently passed proposal for a new full batch audit."""
        query = ('SELECT j.status,j.attempt_id,j.payload,j.adjudication,a.base_commit,a.started,a.result,a.job_id,a.worker '
                 'FROM jobs j JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?')
        with self._retry_lease(job_id):
            with closing(self._connect()) as db:
                db.execute('BEGIN')
                latest = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
                if not latest or latest[0] != checkpoint:
                    raise ValueError('checkpoint is stale')
                row = db.execute(query, (job_id,)).fetchone()
            if not row or row[0] != 'integration-rejected' or row[1] != attempt_id or not row[6] or row[7] != job_id:
                raise ValueError('readmission requires the current rejected attempt')
            snapshot = tuple(row)
            job, rejection, result = map(json.loads, (row[2], row[3], row[6]))
            pid = (json.loads(row[5]) if row[5] else {}).get('pid')
            if type(pid) is not int or pid <= 0:
                raise ValueError('attempt process identity is unknown')
            self._require_absent_pid(pid)
            original = result.get('evidence', {})
            decisions, outcomes, rules = original.get('dispositions', {}), result.get('outcomes', {}), job['rules']
            if (result.get('status') != 'provisional-pass' or set(decisions) != set(rules)
                    or set(outcomes) != set(rules)
                    or any(not isinstance(decisions[rule], dict)
                           or decisions[rule].get('rule', rule if job.get('kind') == 'mechanical' else None) != rule
                           or decisions[rule].get('status') != 'proposed' or not decisions[rule].get('reason')
                           or not decisions[rule].get('evidence') or not isinstance(outcomes[rule], dict)
                           or outcomes[rule].get('status') != 'pass' or not outcomes[rule].get('evidence')
                           for rule in rules)):
                raise ValueError('original per-rule dispositions do not all pass')
            campaign = Path(self.path).parent
            item, proof = retire_native_readmission.verify(campaign, job_id, attempt_id, checkpoint,
                                                             rejection, evidence, _proof_file, _gates)
            if (item.get('base_commit') != row[4] or item.get('rules') != rules
                    or item.get('kind') != original.get('kind')):
                raise ValueError('original proposal identity differs')
            if original.get('kind') == 'author':
                report, _ = _verify_original_report(campaign, job_id, attempt_id, row[4], original, evidence.get('report'))
                if report.get('family_returncode') != 0:
                    raise ValueError('author family check did not pass')
            elif original.get('kind') == 'mechanical':
                report = _verify_mechanical_readmission(campaign, job_id, attempt_id, row[4], original, evidence.get('report'))
            else:
                raise ValueError('original proposal kind is unsupported')
            descriptor = json.loads((campaign/'.cache/attempts'/attempt_id/'tasks'/f'{job_id}.json').read_bytes())
            if (descriptor.get('id') != job_id or descriptor.get('rules') != rules
                    or type(report.get('faces')) is not int or report['faces'] <= 0
                    or item.get('descriptor_sha256') != report.get('descriptor_sha256')
                    or item.get('support_sha256') != report.get('support_sha256')):
                raise ValueError('individual report does not match the assigned proposal')
            with self._transaction() as db:
                latest = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
                current = db.execute(query, (job_id,)).fetchone()
                if not latest or latest[0] != checkpoint or not current or tuple(current) != snapshot:
                    raise ValueError('checkpoint or attempt changed during native proof verification')
                for event in db.execute("SELECT data FROM events WHERE kind IN ('readmitted-batch','readmitted-native-batch','readmitted-library-batch','readmitted-native-exit-one-batch','readmitted-parse-screen-batch')"):
                    previous = json.loads(event[0])
                    if previous.get('job_id') == job_id and previous.get('manifest') == proof['manifest']:
                        raise ValueError('native batch readmission already used for this job')
                self._require_absent_pid(pid)
                db.execute("UPDATE jobs SET status='provisional-pass',adjudication=NULL WHERE id=?", (job_id,))
                self._event(db, 'readmitted-native-batch', {'job_id': job_id, 'attempt_id': attempt_id,
                            'checkpoint': checkpoint, 'manifest': proof['manifest'], 'proof': proof,
                            'previous_adjudication': rejection, 'evidence': evidence})

    def retry_disk_tainted(self, job_id, attempt_id, evidence):
        """Retry one proved independent-evaluator Git index ENOSPC failure.

        The caller independently verifies the named old controller's unit and
        invocation are terminal. Neither the retry nor model output is acceptance.
        """
        if not isinstance(evidence, dict) or evidence.get('kind') != 'git-index-enospc':
            raise ValueError('git-index-enospc evidence is required')
        terminal = evidence.get('controller_terminal')
        verification = terminal.get('verification') if isinstance(terminal, dict) else None
        if (not isinstance(terminal, dict) or not isinstance(terminal.get('unit'), str) or not terminal['unit']
                or not isinstance(terminal.get('invocation'), str) or not terminal['invocation']
                or not isinstance(verification, dict) or verification.get('inactive') is not True
                or type(verification.get('main_pid')) is not int or verification['main_pid'] != 0
                or type(verification.get('previous_pid')) is not int
                or verification['previous_pid'] <= 0):
            raise ValueError('verified terminal controller evidence is required')
        with self._retry_lease(job_id):
            with self._transaction() as db:
                row = db.execute(
                    'SELECT j.status,j.attempt_id,j.payload,a.base_commit,a.started,a.result '
                    'FROM jobs j JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?', (job_id,)).fetchone()
                if not row or row[0] not in ('needs-review', 'failed') or row[1] != attempt_id or not row[5]:
                    raise ValueError('retry requires the current terminal review or failed attempt')
                job, result = json.loads(row[2]), json.loads(row[5])
                started = json.loads(row[4]) if row[4] else {}
                pid = started.get('pid')
                if type(pid) is not int or pid <= 0:
                    raise ValueError('attempt process identity is unknown')
                self._require_absent_pid(pid)
                self._require_absent_pid(verification['previous_pid'])
                _verify_disk_retry(Path(self.path).parent, job_id, attempt_id, row[3], job,
                                   started, result, evidence)
                for event in db.execute("SELECT data FROM events WHERE kind='retry-disk-tainted'"):
                    if json.loads(event[0]).get('job_id') == job_id:
                        raise ValueError('disk retry already used for this job')
                db.execute("UPDATE jobs SET status='pending',attempt_id=NULL WHERE id=?", (job_id,))
                self._event(db, 'retry-disk-tainted', {'job_id': job_id, 'attempt_id': attempt_id,
                            'pid': pid, 'process': 'absent', 'lease': 'acquired', 'evidence': evidence})

    def retry_cohort_tainted(self, job_id, attempt_id, evidence):
        """Retry one proved old shared-face cohort defect; never accept its result."""
        with self._retry_lease(job_id):
            with closing(self._connect()) as db:
                row = db.execute(
                    "SELECT j.status,j.attempt_id,j.payload,a.base_commit,a.started,a.result "
                    "FROM jobs j JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?", (job_id,)).fetchone()
            if not row or row[0] not in ("needs-review", "failed") or row[1] != attempt_id or not row[5]:
                raise ValueError("retry requires the current terminal review or failed attempt")
            snapshot = tuple(row)
            started = json.loads(row[4]) if row[4] else {}
            pid = started.get("pid")
            if type(pid) is not int or pid <= 0:
                raise ValueError("attempt process identity is unknown")
            self._require_absent_pid(pid)
            result = json.loads(row[5])
            original = result.get("evidence")
            if not isinstance(original, dict):
                raise ValueError("original evaluation evidence is missing")
            proof = retire_cohort_proof.verify(Path(self.path).parent, job_id, attempt_id, row[3],
                                                original, evidence, json.loads(row[2])["rules"])
            with self._transaction() as db:
                current = db.execute(
                    "SELECT j.status,j.attempt_id,j.payload,a.base_commit,a.started,a.result "
                    "FROM jobs j JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?", (job_id,)).fetchone()
                if not current or tuple(current) != snapshot:
                    raise ValueError("attempt changed during proof verification")
                self._require_absent_pid(pid)
                existing = db.execute("SELECT data FROM events WHERE kind='retry-cohort-tainted'")
                if any(json.loads(event[0]).get("job_id") == job_id for event in existing):
                    raise ValueError("cohort retry already used for this job")
                db.execute("UPDATE jobs SET status='pending',attempt_id=NULL WHERE id=?", (job_id,))
                self._event(db, "retry-cohort-tainted", {"job_id": job_id, "attempt_id": attempt_id,
                            "pid": pid, "process": "absent", "lease": "acquired",
                            "proof": proof, "evidence": evidence})

    @staticmethod
    def _require_absent_pid(pid, *, finished=None):
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return
        except OSError as exc:
            if exc.errno == errno.ESRCH or _pid_started_after(pid, finished):
                return
            raise ValueError("attempt process state is unknown") from exc
        if not _pid_started_after(pid, finished):
            raise ValueError("attempt process is still present")

    def retry_unstarted(self, job_id, attempt_id, evidence):
        """Administrative recovery of an explicitly proved pre-launch failure.

        The caller verifies the named controller is terminal and launch never
        occurred; a missing or unknown PID is not sufficient evidence.
        """
        _evidence(evidence)
        terminal = evidence.get("controller_terminal") if isinstance(evidence, dict) else None
        if (not isinstance(evidence, dict) or evidence.get("launch_not_attempted") is not True
                or not isinstance(terminal, dict)
                or not isinstance(terminal.get("unit"), str) or not terminal["unit"]
                or not isinstance(terminal.get("invocation"), str) or not terminal["invocation"]
                or not isinstance(terminal.get("verification"), dict) or not terminal["verification"]):
            raise ValueError("explicit terminal controller and pre-launch evidence required")
        with self._retry_lease(job_id):
            with self._transaction() as db:
                row = db.execute(
                    "SELECT j.status,j.attempt_id,a.started,a.result FROM jobs j "
                    "JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?", (job_id,)).fetchone()
                if not row or row[0] != "failed" or row[1] != attempt_id or not row[3]:
                    raise ValueError("retry requires the current finished failed attempt")
                if row[2] is not None:
                    raise ValueError("attempt has a recorded start")
                db.execute("UPDATE jobs SET status='pending',attempt_id=NULL WHERE id=?", (job_id,))
                self._event(db, "retry-unstarted", {"job_id": job_id, "attempt_id": attempt_id,
                                                    "lease": "acquired", "evidence": evidence})

    def _adjudicate(self, jobids, status, evidence, checkpoint=None):
        _evidence(evidence)
        jobids = list(jobids)
        if not jobids or len(set(jobids)) != len(jobids):
            raise ValueError("distinct jobs are required")
        with self._transaction() as db:
            if status == "accepted":
                latest = db.execute("SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1").fetchone()
                if not latest or checkpoint != latest[0]:
                    raise ValueError("acceptance requires the latest checkpoint")
            detail = {"status": status, "checkpoint": checkpoint, "evidence": evidence}
            for job_id in jobids:
                row = db.execute("SELECT status FROM jobs WHERE id=?", (job_id,)).fetchone()
                if not row or row[0] != "provisional-pass":
                    raise ValueError("only provisional passes can be adjudicated")
                db.execute("UPDATE jobs SET status=?,adjudication=? WHERE id=?", (status, _json(detail), job_id))
            self._event(db, "adjudicated", {"jobs": jobids, **detail})

    def advance_and_accept(self, commit, parent, jobids, evidence):
        """Atomically advance the checkpoint and accept all provisional jobs."""
        if not isinstance(commit, str) or not commit:
            raise ValueError("commit is required")
        encoded = _evidence(evidence)
        jobids = list(jobids)
        if not jobids or len(set(jobids)) != len(jobids):
            raise ValueError("distinct jobs are required")
        with self._transaction() as db:
            latest = db.execute("SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1").fetchone()
            actual_parent = latest[0] if latest else None
            if parent != actual_parent:
                raise ValueError("checkpoint parent conflict")
            for job_id in jobids:
                row = db.execute("SELECT status FROM jobs WHERE id=?", (job_id,)).fetchone()
                if not row or row[0] != "provisional-pass":
                    raise ValueError("only provisional passes can be adjudicated")
            try:
                db.execute("INSERT INTO checkpoints(commit_id,parent,evidence) VALUES (?,?,?)",
                           (commit, parent, encoded))
            except sqlite3.IntegrityError as exc:
                raise ValueError("checkpoint already exists") from exc
            detail = {"status": "accepted", "checkpoint": commit, "evidence": evidence}
            for job_id in jobids:
                db.execute("UPDATE jobs SET status='accepted',adjudication=? WHERE id=?",
                           (_json(detail), job_id))
            self._event(db, "checkpoint", {"commit": commit, "parent": parent, "evidence": evidence})
            self._event(db, "adjudicated", {"jobs": jobids, **detail})

    def advance_recovered_diagnostics(self, commit, parent, jobids, evidence):
        """Atomically accept a fully audited batch after a diagnostic-only repair.

        This requires the complete included batch and preserves its original
        rejection events; no native, printing, reference, or authored-case gate
        is relaxed. Evidence and source identity are rechecked under the leases.
        """
        if not isinstance(commit, str) or not commit:
            raise ValueError('commit is required')
        jobids = list(jobids)
        if not jobids or len(set(jobids)) != len(jobids):
            raise ValueError('distinct jobs are required')
        encoded = _evidence(evidence)
        campaign = Path(self.path).parent
        with ExitStack() as leases:
            for job_id in sorted(jobids):
                leases.enter_context(self._retry_lease(job_id))
            with self._transaction() as db:
                latest = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
                if not latest or latest[0] != parent:
                    raise ValueError('checkpoint parent conflict')
                manifest, validation, included = _verify_diagnostic_recovery(campaign, commit, parent, jobids, evidence)
                by_job = {item['job_id']: item for item in included}
                previous = {}
                for job_id in jobids:
                    row = db.execute(
                        'SELECT j.status,j.attempt_id,j.adjudication,j.payload,a.base_commit,a.started,a.result '
                        'FROM jobs j JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?', (job_id,)).fetchone()
                    item = by_job[job_id]
                    if not row or row[0] != 'integration-rejected' or row[1] != item.get('attempt_id') or not row[6]:
                        raise ValueError('recovery requires the original rejected attempts')
                    rejection = json.loads(row[2]) if row[2] else {}
                    if (rejection.get('status') != 'integration-rejected'
                            or rejection.get('evidence', {}).get('manifest') != manifest['id']
                            or rejection.get('evidence', {}).get('validation') != validation):
                        raise ValueError('original rejection does not match the full audit')
                    pid = (json.loads(row[5]) if row[5] else {}).get('pid')
                    if type(pid) is not int or pid <= 0:
                        raise ValueError('attempt process identity is unknown')
                    try:
                        os.kill(pid, 0)
                    except ProcessLookupError:
                        pass
                    except OSError as exc:
                        if exc.errno != errno.ESRCH:
                            raise ValueError('attempt process state is unknown') from exc
                    else:
                        raise ValueError('attempt process is still present')
                    result = json.loads(row[6])
                    rules = {r[0] for r in db.execute('SELECT rule FROM rules WHERE job_id=?', (job_id,))}
                    original_rules = json.loads(row[3])['rules']
                    data = result.get('evidence', {})
                    outcomes = result.get('outcomes', {})
                    dispositions = data.get('dispositions', {}) if isinstance(data, dict) else {}
                    if (not rules or len(original_rules) != len(rules) or set(original_rules) != rules
                            or len(item.get('rules', [])) != len(rules)
                            or set(item.get('rules', [])) != rules or item.get('base_commit') != row[4]
                            or result.get('status') != 'provisional-pass' or set(outcomes) != rules
                            or any(not isinstance(v, dict) or v.get('status') != 'pass' for v in outcomes.values())
                            or not isinstance(dispositions, dict) or set(dispositions) != rules
                            or any(not isinstance(v, dict) or v.get('status') != 'proposed'
                                   or v.get('rule') != name for name, v in dispositions.items())
                            or data.get('task') != job_id or data.get('base_commit') != row[4]):
                        raise ValueError('original batch proposal is not a complete per-rule pass')
                    report_path = Path(data.get('report', ''))
                    expected = campaign/'.cache/attempts'/row[1]/'evaluations'/job_id
                    if (not report_path.is_absolute() or not report_path.resolve().is_relative_to(expected.resolve())
                            or not report_path.is_file() or report_path.is_symlink()):
                        raise ValueError('original independent report is unavailable')
                    report = json.loads(report_path.read_bytes())
                    if (report.get('passed') is not True or report.get('failures') != []
                            or report.get('task') != job_id or report.get('source') != row[4]
                            or report.get('descriptor_sha256') != item.get('descriptor_sha256')
                            or report.get('support_sha256') != item.get('support_sha256')):
                        raise ValueError('original independent report differs from the audited manifest')
                    previous[job_id] = rejection
                try:
                    db.execute('INSERT INTO checkpoints(commit_id,parent,evidence) VALUES (?,?,?)',
                               (commit, parent, encoded))
                except sqlite3.IntegrityError as exc:
                    raise ValueError('checkpoint already exists') from exc
                detail = {'status': 'accepted', 'checkpoint': commit, 'kind': 'diagnostic-only',
                          'evidence': evidence}
                for job_id in jobids:
                    db.execute("UPDATE jobs SET status='accepted',adjudication=? WHERE id=?",
                               (_json(detail), job_id))
                self._event(db, 'checkpoint', {'commit': commit, 'parent': parent, 'evidence': evidence})
                self._event(db, 'diagnostic-recovery', {'jobs': jobids, 'checkpoint': commit,
                            'previous_adjudications': previous, 'evidence': evidence})

    def accept_jobs(self, jobids, checkpoint, evidence):
        self._adjudicate(jobids, "accepted", evidence, checkpoint)

    def reject_jobs(self, jobids, evidence):
        self._adjudicate(jobids, "integration-rejected", evidence)

    def progress(self):
        """Return a consistent aggregate without loading job or rule payloads."""
        with closing(self._connect()) as db:
            db.execute("BEGIN")
            rows = db.execute(
                "SELECT j.status, count(r.rule), count(DISTINCT j.id) "
                "FROM jobs j JOIN rules r ON r.job_id=j.id "
                "WHERE j.status != 'superseded' GROUP BY j.status").fetchall()
            latest = db.execute("SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1").fetchone()
            counts = {row[0]: row[1] for row in rows}
            job_counts = {row[0]: row[2] for row in rows}
            return {"total_rules": sum(counts.values()), "counts": counts,
                    "job_counts": job_counts, "accepted_rules": counts.get("accepted", 0),
                    "checkpoint": latest[0] if latest else None}

    def coverage(self):
        with closing(self._connect()) as db:
            db.execute("BEGIN")
            jobs = []
            counts = {}
            rule_counts = {}
            rules = {}
            for row in db.execute("SELECT id,payload,status,attempt_id,adjudication FROM jobs WHERE status != 'superseded' ORDER BY rowid"):
                payload = json.loads(row[1])
                job = {"id": row[0], "rules": payload["rules"], "status": row[2], "attempt_id": row[3], "adjudication": json.loads(row[4]) if row[4] else None}
                counts[row[2]] = counts.get(row[2], 0) + len(job["rules"])
                attempt = db.execute("SELECT result FROM attempts WHERE id=?", (row[3],)).fetchone() if row[3] else None
                result = json.loads(attempt[0]) if attempt and attempt[0] else None
                inherited = payload.get("review_origin") is not None and row[2] == "needs-review" and row[3] is None
                if payload.get("review_origin") is not None and not inherited:
                    job["prior_review_origin"] = payload["review_origin"]
                if inherited:
                    resolved = retire_rehabilitation.resolve_origin(db, row[0], (row[2], row[3], row[1], row[4]))
                    result = json.loads(resolved[4])
                    job["review_origin"] = resolved[1]
                job["outcomes"] = {rule: result["outcomes"][rule] for rule in job["rules"]} if result else None
                for rule in job["rules"]:
                    outcome = result["outcomes"][rule] if result else None
                    disposition = outcome["status"] if outcome else row[2]
                    if inherited:
                        disposition = "needs-review"
                    elif row[2] in ("accepted", "integration-rejected", "failed"):
                        disposition = row[2]
                    rule_counts[disposition] = rule_counts.get(disposition, 0) + 1
                    rules[rule] = {"job_id": row[0], "disposition": disposition, "outcome": outcome}
                jobs.append(job)
            total = db.execute("SELECT count(*) FROM rules").fetchone()[0]
            latest = db.execute("SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1").fetchone()
            return {"total_rules": total, "counts": counts, "accepted_rules": counts.get("accepted", 0), "rule_counts": rule_counts, "rules": rules, "jobs": jobs, "checkpoint": latest[0] if latest else None}

    def get_jobs(self, status=None, limit=None):
        if limit is not None and (not isinstance(limit, int) or isinstance(limit, bool) or limit < 1):
            raise ValueError("limit must be a positive integer")
        query = "SELECT id,payload,status,attempt_id,adjudication FROM jobs"
        args = []
        if status is not None:
            query += " WHERE status=?"
            args.append(status)
        query += " ORDER BY rowid"
        if limit is not None:
            query += " LIMIT ?"
            args.append(limit)
        with closing(self._connect()) as db:
            return [{"id": row[0], "job": json.loads(row[1]), "status": row[2],
                     "attempt_id": row[3], "adjudication": json.loads(row[4]) if row[4] else None}
                    for row in db.execute(query, args)]

    def get_attempt(self, attemptid):
        with closing(self._connect()) as db:
            row = db.execute("SELECT id,job_id,worker,base_commit,started,result FROM attempts WHERE id=?", (attemptid,)).fetchone()
            if not row:
                return None
            return {"attempt_id": row[0], "job_id": row[1], "worker": row[2],
                    "base_commit": row[3], "started": json.loads(row[4]) if row[4] else None,
                    "result": json.loads(row[5]) if row[5] else None}

    def reviewed_draft(self, job_id):
        """Return the most recent independent replay recorded for this exact assignment."""
        with closing(self._connect()) as db:
            row = db.execute(
                f"SELECT data FROM events WHERE kind IN ({_REVIEWED_EVENT_SLOTS}) "
                "AND json_extract(data,'$.job_id')=? ORDER BY seq DESC LIMIT 1",
                (*_REVIEWED_EVENTS, job_id)).fetchone()
        if row:
            evidence = json.loads(row[0])['evidence']
            if 'timeout' not in evidence and 'source_checkpoint' in evidence:
                return {name: evidence[name] for name in ('source_checkpoint', 'report')}
        return None

    def inventory(self):
        return self.coverage()

    def audit(self):
        with closing(self._connect()) as db:
            return [{"seq": r[0], "kind": r[1], "data": json.loads(r[2]), "at": r[3]} for r in db.execute("SELECT seq,kind,data,at FROM events ORDER BY seq")]
