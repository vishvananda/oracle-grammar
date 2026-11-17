#!/usr/bin/env python3
"""Rehearse or explicitly apply proved four-to-six-rule timeout splits."""
import argparse
from contextlib import closing
import hashlib
import json
import os
import re
from pathlib import Path
import sqlite3
import sys
import tempfile
import time

sys.dont_write_bytecode = True
import retire as r
from retire_queue import Queue


def verifier_hashes():
    names = ('retire.py', 'retire_queue.py', 'retire_timeout_recovery.py',
             'retire_rehabilitation.py', 'retire_priority.py')
    return {name: r.digest(Path(__file__).with_name(name)) for name in names}


def spec(path):
    path = Path(path)
    if path.stat().st_size > 64*1024*1024:
        raise ValueError('timeout evidence exceeds bound')
    return {'path': str(path), 'sha256': r.digest(path)}


def inventory(db, run):
    config = r.read(run/'config.json')
    expected = config.get('expected_rules')
    if (not isinstance(expected, list) or not 1 <= len(expected) <= 200_000
            or any(not isinstance(name, str) or not name for name in expected)
            or len(set(expected)) != len(expected)):
        raise ValueError('campaign expected rule inventory is invalid')
    names = [row[0] for row in db.execute('SELECT rule FROM rules ORDER BY rule')]
    if names != sorted(expected):
        raise ValueError('queue rule inventory differs from campaign config')
    return {'count': len(names), 'sha256': hashlib.sha256(json.dumps(names).encode()).hexdigest()}


def new_output(path):
    path = Path(path)
    if (not path.is_absolute() or not path.parent.is_dir() or path.is_symlink()
            or any(part.is_symlink() for part in path.parents)):
        raise ValueError('output path must be absolute in an existing directory')
    return path


def create_json(path, value):
    text = json.dumps(value, sort_keys=True, indent=2) + '\n'
    if len(text.encode()) > 64*1024*1024:
        raise ValueError('output exceeds bound')
    with path.open('x', encoding='utf-8') as stream:
        stream.write(text)
        stream.flush()
        os.fsync(stream.fileno())


def update_receipt(path, value):
    """Atomically persist a progress receipt, including its directory entry."""
    text = json.dumps(value, sort_keys=True, indent=2) + '\n'
    if len(text.encode()) > 64*1024*1024:
        raise ValueError('receipt exceeds bound')
    fd, temporary = tempfile.mkstemp(prefix='.'+path.name+'.', suffix='.tmp', dir=path.parent)
    try:
        with os.fdopen(fd, 'w', encoding='utf-8') as stream:
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def identity(value):
    return (isinstance(value, str) and 0 < len(value) <= 128 and '\0' not in value
            and Path(value).name == value and value not in ('.', '..'))


def evidence(run, checkpoint, job, attempt):
    source = run/'.cache/attempts'/attempt
    worker = source/'workers'/job
    inputs = Path(r.read(run/'checkpoints'/f'{checkpoint}.json')['inputs'])
    scan = {'kind': 'checkpoint-unused-rehabilitation', **{key: spec(inputs/name) for key, name in {
        'manifest': 'manifest.json', 'scan': 'scan/rules.json', 'scan_complete': 'scan-complete.json',
        'commands': 'scan/commands.jsonl'}.items()}}
    return {'kind': 'timeout-split-recovery', 'scan_proof': scan, **{key: spec(path) for key, path in {
        'final': worker/'final.txt', 'model_receipt': worker/'receipt.json', 'events': worker/'events.jsonl',
        'descriptor': source/'tasks'/f'{job}.json', 'support': source/'support.json',
        'base_manifest': source/'inputs/manifest.json', 'archive_metadata': source/'archive.json',
        'archive': run/'.cache/archives'/f'{attempt}.tar.gz',
        'terminal_receipt': run/'.cache/receipts'/f'{attempt}.json'}.items()}}


def _clone_queue(run, clone):
    # All database writes go to the backup; queue.path continues to bind the
    # existing campaign's immutable artifacts and job leases.
    queue = object.__new__(Queue)
    queue.path = str(run/'queue.sqlite')
    def connect():
        db = sqlite3.connect(clone, timeout=60, isolation_level=None)
        db.row_factory = sqlite3.Row
        db.execute('PRAGMA foreign_keys=ON')
        return db
    queue._connect = connect
    return queue


def _current(db, job):
    return db.execute('SELECT j.status,j.attempt_id,j.payload,a.result FROM jobs j LEFT JOIN attempts a '
                      'ON a.id=j.attempt_id WHERE j.id=?', (job,)).fetchone()


def _candidate(run, db, checkpoint, job):
    if not identity(job): raise ValueError('invalid job identity')
    row = _current(db, job)
    if not row or row[0] != 'needs-review' or not row[1] or not row[3]:
        raise ValueError('not a current finished needs-review attempt')
    if not identity(row[1]): raise ValueError('invalid attempt identity')
    payload = json.loads(row[2]); rules = payload.get('rules')
    if (payload.get('kind') != 'author' or payload.get('timeout_origin') is not None
            or not isinstance(rules, list) or not 4 <= len(rules) <= 6 or len(set(rules)) != len(rules)):
        raise ValueError('not an original four-to-six-rule author job')
    worker = run/'.cache/attempts'/row[1]/'workers'/job
    if (worker/'receipt.json').stat().st_size > 1024*1024:
        raise ValueError('model receipt exceeds bound')
    receipt = r.read(worker/'receipt.json')
    if ((worker/'final.txt').stat().st_size != 0 or receipt.get('timed_out') is not True
            or type(receipt.get('returncode')) is not int or receipt['returncode'] == 0):
        raise ValueError('not a proved empty-final timed-out model receipt')
    return {'job_id': job, 'attempt_id': row[1], 'checkpoint': checkpoint, 'rules': rules,
            'result_sha256': hashlib.sha256(row[3].encode()).hexdigest(),
            'evidence': evidence(run, checkpoint, job, row[1])}


def verify_split(db, item, result):
    row = _current(db, item['job_id'])
    if (not row or row[0] != 'superseded' or row[1] != item['attempt_id'] or not row[3]
            or hashlib.sha256(row[3].encode()).hexdigest() != item['result_sha256']):
        raise ValueError('original attempt changed after split')
    children = result['children']
    if not isinstance(children, list) or len(children) != 2 or len(set(children)) != 2:
        raise ValueError('timeout split child identities differ')
    events = [json.loads(row[0]) for row in db.execute(
        "SELECT data FROM events WHERE kind='rehabilitated-timeout' AND json_extract(data,'$.job_id')=?",
        (item['job_id'],))]
    if (len(events) != 1 or events[0] != result or result.get('attempt_id') != item['attempt_id']
            or result.get('checkpoint') != item['checkpoint'] or result.get('evidence') != item['evidence']):
        raise ValueError('immutable timeout creation event differs')
    midpoint = (len(item['rules'])+1)//2
    hashes = {}
    for child_id, expected in zip(children, (item['rules'][:midpoint], item['rules'][midpoint:]), strict=True):
        child = db.execute('SELECT payload FROM jobs WHERE id=?', (child_id,)).fetchone()
        payload = json.loads(child[0]) if child else {}
        if (payload.get('rules') != expected or payload.get('rule_dispositions_required') != expected
                or payload.get('timeout_origin') != {'job_id': item['job_id'], 'attempt_id': item['attempt_id'],
                                                     'checkpoint': item['checkpoint']}):
            raise ValueError('immutable timeout child payload differs')
        hashes[child_id] = hashlib.sha256(child[0].encode()).hexdigest()
    if 'child_payload_sha256' in item and item['child_payload_sha256'] != hashes:
        raise ValueError('immutable timeout child payload hash differs')
    return hashes


def dryplan(run, output, selected=None):
    run, output = Path(run).resolve(), new_output(output)
    if selected is not None and not isinstance(selected, list):
        raise ValueError('job selection must be a list')
    if output.exists(): raise ValueError('plan output already exists')
    with tempfile.TemporaryDirectory(prefix='timeout-plan-') as temporary:
        clone = Path(temporary)/'queue.sqlite'
        with closing(sqlite3.connect((run/'queue.sqlite').as_uri()+'?mode=ro', uri=True)) as source:
            with closing(sqlite3.connect(clone)) as target:
                source.backup(target)
        queue = _clone_queue(run, clone)
        with closing(queue._connect()) as db:
            checkpoint = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()[0]
            before = inventory(db, run)
            jobs = selected if selected is not None else [row[0] for row in db.execute(
                "SELECT id FROM jobs WHERE status='needs-review' ORDER BY rowid")]
            if len(jobs) > 200_000 or len(jobs) != len(set(jobs)) or any(not identity(job) for job in jobs):
                raise ValueError('invalid or duplicate job selection')
            rows = []
            for job in jobs:
                try:
                    item = _candidate(run, db, checkpoint, job)
                    result = queue.rehabilitate_timeout(job, item['attempt_id'], checkpoint, item['evidence'])
                except (ValueError, OSError, KeyError, TypeError) as error:
                    rows.append({'job_id': job, 'status': 'parked', 'reason': str(error)})
                    continue
                # A failed post-commit proof invalidates the whole rehearsal.
                item['child_payload_sha256'] = verify_split(db, item, result)
                item['children'] = result['children']
                if inventory(db, run) != before: raise RuntimeError('rule inventory changed in rehearsal')
                rows.append({'status': 'rehearsed', **item})
        document = {'format': 1, 'run': str(run), 'checkpoint': checkpoint, 'inventory': before,
                    'script_sha256': r.digest(Path(__file__)), 'verifier_sha256': verifier_hashes(),
                    'created_unix': time.time(), 'jobs': rows}
        create_json(output, document)
        return document


def apply(plan_path, digest, receipt_path, expected_run=None):
    plan_path, receipt_path = Path(plan_path), new_output(receipt_path)
    if (not plan_path.is_absolute() or plan_path.is_symlink() or not plan_path.is_file()
            or any(part.is_symlink() for part in plan_path.parents)
            or plan_path.stat().st_size > 64*1024*1024):
        raise ValueError('plan must be an absolute bounded regular file')
    if not re.fullmatch(r'[0-9a-f]{64}', digest) or r.digest(plan_path) != digest:
        raise ValueError('explicit plan hash differs')
    plan = r.read(plan_path)
    if (plan.get('format') != 1 or plan.get('script_sha256') != r.digest(Path(__file__))
            or plan.get('verifier_sha256') != verifier_hashes()):
        raise ValueError('plan, script or verifier identity differs')
    run = Path(plan['run'])
    if expected_run is not None and run.resolve() != Path(expected_run).resolve():
        raise ValueError('campaign differs from plan')
    if not run.is_absolute() or not run.is_dir():
        raise ValueError('plan campaign path is invalid')
    if not isinstance(plan.get('jobs'), list) or len(plan['jobs']) > 200_000:
        raise ValueError('plan jobs exceed bound')
    ids = []
    for item in plan['jobs']:
        if (not isinstance(item, dict) or item.get('status') not in ('parked', 'rehearsed')
                or not identity(item.get('job_id'))):
            raise ValueError('plan contains an invalid job')
        ids.append(item['job_id'])
        if item['status'] == 'rehearsed' and (item.get('checkpoint') != plan.get('checkpoint')
                or not identity(item.get('attempt_id')) or not isinstance(item.get('child_payload_sha256'), dict)):
            raise ValueError('plan contains invalid rehearsal identity')
    if len(ids) != len(set(ids)):
        raise ValueError('plan contains duplicate jobs')
    queue = Queue(run/'queue.sqlite')
    with closing(queue._connect()) as db:
        latest = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
        if not latest or latest[0] != plan['checkpoint'] or inventory(db, run) != plan['inventory']:
            raise ValueError('planned checkpoint or inventory changed')
    if not any(item.get('status') == 'rehearsed' for item in plan['jobs']):
        raise ValueError('plan contains no rehearsed timeout jobs')
    receipt = {'status': 'applying', 'plan_sha256': digest, 'completed': [], 'inflight': None}
    create_json(receipt_path, receipt)
    try:
        for item in plan['jobs']:
            if item['status'] != 'rehearsed': continue
            if (plan['script_sha256'] != r.digest(Path(__file__))
                    or plan['verifier_sha256'] != verifier_hashes()):
                raise ValueError('script or verifier changed during apply')
            with closing(queue._connect()) as db:
                row = _current(db, item['job_id'])
                if (not row or row[0] != 'needs-review' or row[1] != item['attempt_id'] or not row[3]
                        or hashlib.sha256(row[3].encode()).hexdigest() != item['result_sha256']
                        or inventory(db, run) != plan['inventory']):
                    raise ValueError('planned attempt or inventory changed')
            receipt['inflight'] = item['job_id']
            update_receipt(receipt_path, receipt)
            result = queue.rehabilitate_timeout(item['job_id'], item['attempt_id'], plan['checkpoint'], item['evidence'])
            receipt['completed'].append(result)
            receipt['inflight'] = None
            update_receipt(receipt_path, receipt)
            if result['children'] != item['children']: raise ValueError('child identities differ from rehearsal')
            with closing(queue._connect()) as db:
                verify_split(db, item, result)
        with closing(queue._connect()) as db:
            if inventory(db, run) != plan['inventory']: raise ValueError('rule inventory changed')
        receipt['status'] = 'passed'
    except BaseException as error:
        receipt.update(status='failed', reason=str(error))
        raise
    finally:
        update_receipt(receipt_path, receipt)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--job', action='append')
    parser.add_argument('--apply-plan-sha256')
    parser.add_argument('--receipt', type=Path)
    args = parser.parse_args()
    if args.apply_plan_sha256:
        if args.job or not args.receipt: parser.error('apply requires a receipt and no job filter')
        result = apply(args.plan, args.apply_plan_sha256, args.receipt, expected_run=args.run)
    else:
        if args.receipt: parser.error('--receipt is only valid with --apply-plan-sha256')
        result = dryplan(args.run, args.plan, args.job)
    print(json.dumps({'plan': str(args.plan), 'sha256': r.digest(args.plan),
                      'status': result.get('status', 'planned'),
                      'rehearsed': sum(x['status'] == 'rehearsed' for x in result.get('jobs', []))}))


if __name__ == '__main__': main()
