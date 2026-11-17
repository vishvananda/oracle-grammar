"""Prepare one isolated, descriptor-bound retirement authoring attempt."""
import hashlib
from collections import deque
from functools import lru_cache
from threading import RLock
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import uuid

import dev
import retire as r
import retire_reuse

RUNNERS = tuple(dict.fromkeys((*r.TOOLING, 'retire_author.py', 'retire_author_pool.py',
                               'retire_pool.py', 'retire_cpu.py', 'retire_helper.py')))
HELPERS = ('family-check', 'probe', 'inspect')


def git(root, *args):
    return subprocess.check_output(['git', *args], cwd=root, text=True).strip()


def validate_runner(campaign, runner=None):
    """Resolve and verify a fixed canonical or versioned campaign runner."""
    campaign = Path(campaign).resolve()
    selected = Path(os.path.abspath(runner)) if runner is not None else campaign/'runner'
    canonical = campaign/'runner'
    versions = campaign/'runners'
    if selected == canonical:
        manifest = campaign/'runner.json'
    elif selected.parent == versions and selected.name not in ('', '.', '..'):
        manifest = selected.with_suffix('.json')
        if versions.is_symlink():
            raise ValueError('versioned runner parent must not be a symlink')
    else:
        raise ValueError('runner must be a canonical or direct versioned campaign runner')
    if (selected.is_symlink() or not selected.is_dir() or manifest.is_symlink()
            or not manifest.is_file() or any(path.is_symlink() for path in selected.rglob('*'))):
        raise ValueError('runner or manifest is missing or contains a symlink')
    r.require(r.tree(selected) == r.read(manifest), 'campaign runner changed')
    return selected


def _identity(checkpoint, campaign, runner):
    source, inputs, helpers = (Path(checkpoint[k]).resolve() for k in ('source', 'inputs', 'helpers'))
    manifest = r.read(inputs/'manifest.json')
    r.require(manifest['format'] == 3 and manifest['commit'] == checkpoint['commit'],
              'checkpoint and frozen manifest commits differ')
    r.require(git(source, 'rev-parse', 'HEAD') == checkpoint['commit'], 'source commit differs')
    r.require(not git(source, 'status', '--porcelain', '--untracked-files=no'), 'source tracked files changed')
    r.require(dev.source(source) == manifest['source'], 'compiled source differs from frozen manifest')
    build = r.read(helpers/'build.json')
    r.require(build['source_sha256'] == manifest['source'], 'helper build source differs')
    frozen_build = r.read(inputs/'frozen/build.json')
    r.require(frozen_build['source_sha256'] == build['source_sha256'], 'frozen build source differs')
    for name in HELPERS:
        r.require(r.digest(helpers/name) == build['binaries'][name], 'helper checksum mismatch: '+name)
    for name in r.TOOLING:
        r.require(r.digest(runner/name) == manifest['tooling'][name],
                  'frozen runner tooling mismatch: '+name)
    return source, inputs, helpers, manifest


def _checkout(source, destination, commit):
    destination.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(['git', 'worktree', 'add', '--detach', str(destination), commit],
                   cwd=source, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    # Git worktree has private index metadata. Replace every tracked regular file
    # with a private copy; never copy ignored caches or overwrite .git metadata.
    entries = subprocess.check_output(['git', 'ls-files', '-z'], cwd=source).split(b'\0')
    for entry in entries:
        if not entry:
            continue
        relative = os.fsdecode(entry)
        src, target = source/relative, destination/relative
        temporary = target.with_name(target.name+'.'+uuid.uuid4().hex+'.tmp')
        if src.is_symlink():
            os.symlink(os.readlink(src), temporary)
        elif src.is_file():
            shutil.copy2(src, temporary)
        else:
            raise ValueError('unsupported tracked file: '+relative)
        os.replace(temporary, target)


def _paths(source, job):
    fixture = 'families/retirement-'+hashlib.sha256(job['id'].encode()).hexdigest()[:16]+'.json'
    edits = 'tests/fixtures/retirement-'+job['id']+'-edits.json'
    r.require(re.fullmatch(r'[A-Za-z0-9_-]+', job['id']) is not None, 'unsafe job id')
    hinted = set(job.get('allowedwritepaths', []))
    modules = {p for p in hinted if re.fullmatch(r'grammar/syntax/[A-Za-z0-9_]+\.ebnf', p)
               and not p.endswith('/mined.ebnf') and (source/p).is_file()}
    discovery = not modules
    if discovery:
        modules = {f'grammar/syntax/{name}.ebnf' for name in ('actions', 'nouns', 'conditions', 'events')
                   if (source/f'grammar/syntax/{name}.ebnf').is_file()}
    hints = sorted(modules)
    # Any existing runtime syntax module may be needed for a compositional fix.
    modules = {p.relative_to(source).as_posix() for p in (source/'grammar/syntax').glob('*.ebnf')
               if p.is_file() and p.name != 'mined.ebnf'}
    # Both seeded files may contain hooks referring to the assigned rule.
    seeded = {f'grammar/{kind}/mined.ebnf' for kind in ('syntax', 'tail')
              if (source/f'grammar/{kind}/mined.ebnf').is_file()}
    return fixture, edits, sorted(modules | seeded | {fixture, edits}), discovery, hints


_PACKET_LOCK = RLock()


@lru_cache(maxsize=1)
def _packet_data(inputs, manifest_hash):
    """Read immutable population once per coordinator process, retaining addresses."""
    del manifest_hash
    frozen = Path(inputs)/'frozen'
    by_name = {}
    for key, row in r.read(frozen/'index.json').items():
        by_name.setdefault(row['name'], []).append((int(key), row))
    gold = r.rows(frozen/'gold.jsonl')
    oracle = {}
    for number, value in enumerate(r.read(frozen/'oracle.json')):
        key = tuple(sorted(face['name'] for face in value['cards']))
        oracle.setdefault(key, []).append((number, value))
    return by_name, gold, oracle


MAX_BASE_COHORT_NAMES = 500
MAX_COHORT_OCCURRENCES = 1024


class CohortClosureLimit(ValueError):
    """The complete shared-face closure is too large for a bounded diagnostic."""
    def __init__(self, count, limit):
        self.count, self.limit = count, limit
        super().__init__(f'cohort closure exceeds {limit} face occurrences (at least {count}); coordinator review required')


def close_cohort(inputs, detail, *, max_occurrences=MAX_COHORT_OCCURRENCES):
    """Close names over shared gold records without dropping duplicate occurrences."""
    if type(max_occurrences) is not int or not 1 <= max_occurrences <= MAX_COHORT_OCCURRENCES:
        raise ValueError('invalid cohort occurrence limit')
    names = detail.get('names')
    r.require(isinstance(names, list) and all(isinstance(name, str) and name for name in names),
              'cohort names must be a list of nonempty strings')
    initial = set(names)
    r.require(len(initial) <= MAX_BASE_COHORT_NAMES, 'base cohort exceeds 500 distinct face names')
    inputs = Path(inputs)
    with _PACKET_LOCK:
        by_name, records, _ = _packet_data(str(inputs), r.digest(inputs/'manifest.json'))
    r.require(initial <= by_name.keys(), 'cohort outside frozen population')
    pending = deque(sorted(initial))
    selected, record_ids, observed = set(initial), set(), set()
    occurrences = 0
    while pending:
        name = pending.popleft()
        for _, row in by_name[name]:
            record, face = row.get('record'), row.get('face')
            r.require(type(record) is int and 0 <= record < len(records)
                      and type(face) is int and 0 <= face < len(records[record]['faces'])
                      and records[record]['faces'][face]['name'] == name,
                      'frozen index address does not match gold face')
            address = (record, face)
            r.require(address not in observed, 'frozen index repeats a gold face address')
            observed.add(address)
            if record in record_ids:
                continue
            record_ids.add(record)
            faces = records[record]['faces']
            occurrences += len(faces)
            if occurrences > max_occurrences:
                raise CohortClosureLimit(occurrences, max_occurrences)
            for item in faces:
                companion = item['name']
                r.require(companion in by_name, 'gold companion outside frozen population')
                if companion not in selected:
                    selected.add(companion)
                    pending.append(companion)
    expected = {(record, face) for record in record_ids for face in range(len(records[record]['faces']))}
    r.require(observed == expected, 'frozen index omits gold face occurrences in the cohort closure')
    result = dict(detail)
    result.update(names=sorted(selected), closure_added_names=sorted(selected-initial),
                  closure_base_name_count=len(initial), closure_record_count=len(record_ids),
                  closure_occurrence_count=occurrences, closure_occurrence_limit=max_occurrences,
                  closure_scope='transitive shared-face-name closure over frozen gold records; no truncation')
    return result


def _packet(inputs, job, cohort):
    with _PACKET_LOCK:
        by_name, records, oracle_index = _packet_data(str(inputs), r.digest(inputs/'manifest.json'))
    targets = set(job.get('target_faces', []))
    r.require(targets <= by_name.keys(), 'target outside frozen population')
    r.require(set(cohort['names']) <= by_name.keys(), 'cohort outside frozen population')
    r.require(targets <= set(cohort['names']), 'cohort omitted targets')
    matches = sorted((pair for name in targets for pair in by_name[name]), key=lambda pair: pair[0])
    record_ids = sorted({entry['record'] for _, entry in matches})
    gold = [{'record': number, 'value': records[number]} for number in record_ids]
    # Reference and Oracle preserve their own face order; match the full name
    # multiset while retaining both original record and face addresses.
    names = {tuple(sorted(face['name'] for face in record['value']['faces'])) for record in gold}
    oracle = [{'record': number, 'value': value} for key in sorted(names)
              for number, value in oracle_index.get(key, [])]
    r.require(all(key in oracle_index for key in names), 'target oracle record missing')
    baseline = []
    for ordinal, row in matches:
        text = row.get('text')
        occurrences = [{'record': record['record'], 'face': face_number,
                        'exact_text': text is not None and face.get('text') == text}
                       for record in oracle for face_number, face in enumerate(record['value']['cards'])
                       if face['name'] == row['name']]
        baseline.append({'ordinal': ordinal, 'record': row['record'], 'face': row['face'],
                         'name': row['name'], 'text': text, 'flags': row.get('flags'),
                         'oracle_occurrences': occurrences})
    return {'rules': job['rules'], 'source_fragments': job.get('source_fragments', []),
            'target_faces': sorted(targets), 'gold_records': gold, 'oracle_records': oracle,
            'baseline': baseline,
            'cohort': cohort, 'notice': 'Records match by complete face-name multiset; original orders and addresses are retained. Text identity is not inferred. Cohort is diagnostic, not acceptance proof.'}


def prepare_task(campaign: Path, claim: dict, checkpoint: dict, job: dict, cohort_detail: dict, runner=None) -> dict:
    campaign = Path(campaign).resolve()
    attempt = claim['attempt_id']
    try:
        parsed_attempt = uuid.UUID(attempt)
    except (ValueError, TypeError, AttributeError) as exc:
        raise ValueError('invalid attempt id') from exc
    r.require(parsed_attempt.version == 4 and attempt in (str(parsed_attempt), parsed_attempt.hex),
              'attempt id must be a canonical UUID4 or UUID4 hex')
    r.require(claim['base_commit'] == checkpoint['commit'], 'claim and checkpoint commits differ')
    if 'job' in claim:
        r.require(claim['job']['id'] == job['id'] and claim['job']['rules'] == job['rules'],
                  'claim and job differ')
    r.require(job['rules'] and len(set(job['rules'])) == len(job['rules']), 'invalid assigned rules')
    r.require(isinstance(job.get('goal'), str) and job['goal'].strip(), 'task goal is required')
    selected_runner = validate_runner(campaign, runner)
    source, inputs, helpers, manifest = _identity(checkpoint, campaign, selected_runner)
    run = campaign/'.cache/attempts'/attempt
    checkout = campaign/'workspaces'/attempt
    r.require(not checkout.exists(), 'attempt checkout already exists')
    fixture, edits, write_files, discovery, hints = _paths(source, job)
    cohort_detail = close_cohort(inputs, cohort_detail)
    packet = _packet(inputs, job, cohort_detail)
    run.mkdir(parents=True, exist_ok=False)
    created_checkout = False
    try:
        _checkout(source, checkout, claim['base_commit'])
        created_checkout = True
        r.require(dev.source(checkout) == manifest['source'], 'private checkout compiled source differs')
        (checkout/'.cache').mkdir(exist_ok=True)
        reuse = None
        if claim.get('reuse_archive'):
            reuse = retire_reuse.restore_candidate(campaign, checkout, job['id'], claim['base_commit'],
                                                     write_files, claim['reuse_archive'], rules=job['rules'])
            r.require(dev.source(checkout) == manifest['source'], 'restored candidate changed compiled inputs')
            packet['reuse'] = reuse
        (run/'inputs').symlink_to(inputs, target_is_directory=True)
        runner, binaries = run/'support/runner', run/'support/bin'
        runner.mkdir(parents=True)
        binaries.mkdir()
        native = run/'support/native'
        native.mkdir()
        for name in RUNNERS:
            shutil.copy2(selected_runner/name, runner/name)
        for name in HELPERS:
            os.link(helpers/name, native/name)
            shutil.copy2(selected_runner/'retire_helper.py', binaries/name)
            (binaries/name).chmod(0o755)
        validate_runner(campaign, selected_runner)
        for name in RUNNERS:
            r.require(r.digest(runner/name) == r.digest(selected_runner/name), 'runner changed while copying')
        for name in HELPERS:
            r.require(r.digest(binaries/name) == r.digest(selected_runner/'retire_helper.py'),
                      'helper wrapper changed while copying')
        tasks = run/'tasks'
        tasks.mkdir()
        (run/'workers').mkdir()
        task = dict(job, model=job.get('model', 'gpt-6.1-terra'), effort=job.get('effort', 'high'),
                    checkout=str(checkout), fixture=fixture, edits=edits,
                    hooks='.cache/author-hooks.json', write_files=write_files,
                    allowedwritepaths=write_files, module_hints=hints,
                    module_discovery_required=discovery, cohort=cohort_detail['names'],
                    cohort_detail=cohort_detail, packet=str(run/'packet.json'),
                    rule_dispositions_required=job['rules'], max_diagnostic_attempts=3,
                    wall_budget_seconds=900, helper_threads=2,
                    cpu_pool=str(campaign/'cpu'), cpu_capacity=32)
        task['goal'] = (task['goal'] + ' Start with module_hints; related runtime syntax modules may be edited. '
                        'Request root dispatch hooks through .cache/author-hooks.json.')
        if reuse:
            task['reuse'] = reuse
            task['goal'] = ('Private checkout starts with a prior infrastructure-tainted candidate; '
                            'recheck current files and retain or revise them. ' + task['goal'])
        r.write(tasks/(job['id']+'.json'), task)
        r.write(run/'packet.json', packet)
        support = {'files': r.tree(run/'support'), 'tasks': {job['id']: r.digest(tasks/(job['id']+'.json'))},
                   'packet_sha256': r.digest(run/'packet.json')}
        r.write(run/'support.json', support)
        return {'run': str(run), 'task_id': job['id'], 'checkout': str(checkout)}
    except BaseException:
        if created_checkout or checkout.exists():
            subprocess.run(['git', 'worktree', 'remove', '--force', str(checkout)], cwd=source,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        shutil.rmtree(run, ignore_errors=True)
        raise
