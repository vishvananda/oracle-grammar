"""Prepare one isolated, descriptor-bound retirement authoring attempt."""
import hashlib
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


def _identity(checkpoint, campaign):
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
        r.require(r.digest(campaign/'runner'/name) == manifest['tooling'][name],
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
    # Both seeded files may contain hooks referring to the assigned rule.
    seeded = {f'grammar/{kind}/mined.ebnf' for kind in ('syntax', 'tail')
              if (source/f'grammar/{kind}/mined.ebnf').is_file()}
    return fixture, edits, sorted(modules | seeded | {fixture, edits}), discovery


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


def prepare_task(campaign: Path, claim: dict, checkpoint: dict, job: dict, cohort_detail: dict) -> dict:
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
    source, inputs, helpers, manifest = _identity(checkpoint, campaign)
    run = campaign/'.cache/attempts'/attempt
    checkout = campaign/'workspaces'/attempt
    r.require(not checkout.exists(), 'attempt checkout already exists')
    fixture, edits, write_files, discovery = _paths(source, job)
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
            shutil.copy2(campaign/'runner'/name, runner/name)
        for name in HELPERS:
            os.link(helpers/name, native/name)
            shutil.copy2(campaign/'runner/retire_helper.py', binaries/name)
            (binaries/name).chmod(0o755)
        tasks = run/'tasks'
        tasks.mkdir()
        (run/'workers').mkdir()
        task = dict(job, model=job.get('model', 'gpt-6.1-terra'), effort=job.get('effort', 'high'),
                    checkout=str(checkout), fixture=fixture, edits=edits,
                    hooks='.cache/author-hooks.json', write_files=write_files,
                    module_discovery_required=discovery, cohort=cohort_detail['names'],
                    cohort_detail=cohort_detail, packet=str(run/'packet.json'),
                    rule_dispositions_required=job['rules'], max_diagnostic_attempts=3,
                    wall_budget_seconds=900, helper_threads=2,
                    cpu_pool=str(campaign/'cpu'), cpu_capacity=32)
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
