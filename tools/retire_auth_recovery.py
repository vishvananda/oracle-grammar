#!/usr/bin/env python3
"""Rehearse and explicitly retry terminal HTTP 401 attempts; never accept rules."""
from contextlib import closing
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import tempfile

import retire as r
from retire_queue import Queue
from retire_reuse import _read_archive
from retire_timeout_operator import create_json, identity, inventory, new_output, update_receipt

MAX_FILE = 32 * 1024 * 1024


def verifier_hashes():
    names = ('retire_auth_recovery.py', 'retire_queue.py', 'retire_reuse.py',
             'retire_timeout_operator.py', 'retire.py')
    return {name: r.digest(Path(__file__).with_name(name)) for name in names}


def _file(root, path, limit=MAX_FILE):
    if (not path.is_absolute() or path.is_symlink() or not path.is_file()
            or path.resolve() != path or not path.is_relative_to(root/'.cache')
            or path.stat().st_size > limit):
        raise ValueError('auth evidence path or size is invalid')
    data = path.read_bytes()
    if len(data) > limit:
        raise ValueError('auth evidence exceeds bound')
    return data, {'path': str(path), 'sha256': hashlib.sha256(data).hexdigest()}


def existing_queue(root):
    path = root/'queue.sqlite'
    if path.is_symlink() or not path.is_file():
        raise ValueError('existing campaign queue is unavailable')
    queue = Queue.__new__(Queue)
    queue.path = str(path)
    return queue


def _absent(pid):
    if type(pid) is not int or pid <= 0:
        raise ValueError('auth attempt PID is unknown')
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return
    raise ValueError('auth attempt PID is present or reused')


def inspect(root, db, job, attempt):
    """Verify this current terminal attempt, including archived candidate bytes."""
    if not identity(job) or not identity(attempt):
        raise ValueError('invalid job or attempt identity')
    row = db.execute('SELECT j.status,j.attempt_id,j.payload,a.job_id,a.base_commit,a.started,a.result '
                     'FROM jobs j JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?', (job,)).fetchone()
    if not row or row[:2] != ('needs-review', attempt) or row[3] != job or not row[6]:
        raise ValueError('auth recovery needs the current terminal review attempt')
    payload, start, result = json.loads(row[2]), json.loads(row[5] or 'null'), json.loads(row[6])
    rules = payload.get('rules')
    owned = [x[0] for x in db.execute('SELECT rule FROM rules WHERE job_id=? ORDER BY rule', (job,))]
    original = result.get('evidence', {})
    if (payload.get('kind') != 'author' or not isinstance(rules, list) or not rules
            or len(set(rules)) != len(rules) or sorted(rules) != owned
            or result.get('status') != 'needs-review' or original.get('kind') != 'author'
            or original.get('status') != 'operational-failure' or original.get('dispositions')
            or not isinstance(start, dict) or set(result.get('outcomes', {})) != set(rules)
            or any(x.get('status') != 'needs-review' for x in result['outcomes'].values())):
        raise ValueError('auth attempt has semantic disposition or differing rule ownership')
    if db.execute("SELECT 1 FROM events WHERE kind='retry-auth' AND json_extract(data,'$.attempt_id')=?",
                  (attempt,)).fetchone():
        raise ValueError('auth attempt already retried')
    run = root/'.cache/attempts'/attempt
    worker = run/'workers'/job
    paths = {'events': worker/'events.jsonl', 'final': worker/'final.txt',
             'model_receipt': worker/'receipt.json', 'descriptor': run/'tasks'/f'{job}.json',
             'support': run/'support.json', 'receipt': root/'.cache/receipts'/f'{attempt}.json',
             'archive_record': run/'archive.json', 'archive': root/'.cache/archives'/f'{attempt}.tar.gz'}
    decoded, specs = {}, {}
    for name, path in paths.items():
        decoded[name], specs[name] = _file(root, path, 64*1024*1024 if name == 'archive' else MAX_FILE)
    if decoded['final'].strip():
        raise ValueError('auth attempt has a usable or nonempty final response')
    events = [json.loads(line) for line in decoded['events'].splitlines() if line.strip()]
    if (not events or events[-1].get('type') != 'turn.failed'
            or not re.search(r'\b401\s+Unauthorized\b', events[-1].get('error', {}).get('message', ''))
            or any(e.get('type') == 'turn.completed' for e in events)
            or any(e.get('type') == 'item.completed' and e.get('item', {}).get('type') == 'agent_message'
                   for e in events)):
        raise ValueError('events do not prove a terminal HTTP 401 without a model disposition')
    receipt, model, task, support, archived = [json.loads(decoded[name]) for name in
                                            ('receipt', 'model_receipt', 'descriptor', 'support', 'archive_record')]
    pid = start.get('pid')
    _absent(pid)
    if (start.get('receipt') != str(paths['model_receipt']) or model.get('pid') != pid
            or model.get('task') != job or type(model.get('returncode')) is not int
            or model['returncode'] == 0 or model.get('timed_out', False) is not False
            or model.get('status') != 'needs-review' or type(model.get('finished_unix')) not in (int, float)
            or model['finished_unix'] <= model.get('started_unix', model['finished_unix'])
            or original != {**receipt, 'receipt': str(paths['receipt'])}
            or receipt.get('attempt') != attempt or receipt.get('job') != job
            or receipt.get('run') != str(run) or receipt.get('base_commit') != row[4]
            or receipt.get('error') != 'Expecting value: line 1 column 1 (char 0)'
            or task.get('id') != job or task.get('rules') != rules
            or support.get('tasks', {}).get(job) != specs['descriptor']['sha256']
            or archived.get('archive') != str(paths['archive'])
            or archived.get('sha256') != specs['archive']['sha256'] or archived.get('checkout_removed') is not True):
        raise ValueError('terminal auth receipt, descriptor, support or archive binding differs')
    members = _read_archive(paths['archive'])
    manifest = json.loads(members['manifest.json'][0])
    if (manifest.get('format') != 1 or manifest.get('attempt') != attempt or manifest.get('task') != job
            or manifest.get('base_commit') != row[4] or manifest.get('receipt') != receipt
            or manifest.get('receipt_sha256') != specs['receipt']['sha256']
            or manifest.get('descriptor_sha256') != specs['descriptor']['sha256']
            or manifest.get('patch_sha256') != hashlib.sha256(members['candidate.patch'][0]).hexdigest()
            or set(members) != {'manifest.json', 'candidate.patch'} | {'files/'+n for n in manifest['files']}):
        raise ValueError('auth candidate archive manifest differs')
    for name, info in manifest['files'].items():
        data, mode = members['files/'+name]
        if hashlib.sha256(data).hexdigest() != info['sha256'] or mode != info['mode']:
            raise ValueError('auth candidate archive content differs')
    if Path(task.get('checkout', '')).exists():
        raise ValueError('auth candidate checkout is still present')
    return {'job_id': job, 'attempt_id': attempt, 'rules': rules, 'base_commit': row[4], 'pid': pid,
            'result_sha256': hashlib.sha256(row[6].encode()).hexdigest(),
            'payload_sha256': hashlib.sha256(row[2].encode()).hexdigest(), 'artifacts': specs,
            'reuse': {'attempt': attempt, 'base_commit': row[4], 'path': str(paths['archive']),
                      'sha256': specs['archive']['sha256'], 'rules': rules},
            'archived_files': len(manifest['files']), 'archive_patch_bytes': len(members['candidate.patch'][0])}


def retry(queue, root, expected, plan_sha):
    """The kernel lease and row CAS protect one terminal attempt under a live controller."""
    with queue._retry_lease(expected['job_id']):
        with queue._transaction() as db:
            current = inspect(root, db, expected['job_id'], expected['attempt_id'])
            if current != expected:
                raise ValueError('auth attempt evidence changed since rehearsal')
            db.execute("UPDATE jobs SET status='pending',attempt_id=NULL WHERE id=?", (expected['job_id'],))
            queue._event(db, 'retry-auth', {'job_id': expected['job_id'], 'attempt_id': expected['attempt_id'],
                                          'pid': expected['pid'], 'process': 'absent', 'lease': 'acquired',
                                          'plan_sha256': plan_sha, 'evidence': expected})


def dryplan(root, path, attempts):
    root, path = Path(root).resolve(), new_output(path)
    if not isinstance(attempts, list) or not 1 <= len(attempts) <= 96 or len(set(attempts)) != len(attempts):
        raise ValueError('select one to 96 distinct attempt IDs')
    jobs = []
    with tempfile.TemporaryDirectory(prefix='auth-rehearsal-') as temporary:
        clone = Path(temporary)/'queue.sqlite'
        with closing(sqlite3.connect((root/'queue.sqlite').as_uri()+'?mode=ro', uri=True)) as source:
            with closing(sqlite3.connect(clone)) as target:
                source.backup(target)
        q = Queue(clone)
        with closing(q._connect()) as db:
            inv = inventory(db, root)
            for attempt in attempts:
                row = db.execute('SELECT job_id FROM attempts WHERE id=?', (attempt,)).fetchone()
                if not row:
                    raise ValueError('unknown auth attempt')
                jobs.append(inspect(root, db, row[0], attempt))
        for expected in jobs:
            with existing_queue(root)._retry_lease(expected['job_id']):
                leases = clone.parent/'leases'; leases.mkdir(exist_ok=True)
                (leases/expected['job_id']).touch()
                retry(q, root, expected, 'rehearsal')
        with closing(q._connect()) as db:
            if inventory(db, root) != inv:
                raise ValueError('rehearsal changed rule inventory')
    plan = {'kind': 'terminal-auth-retry-plan', 'run': str(root), 'verifiers': verifier_hashes(),
            'inventory': inv, 'jobs': jobs, 'acceptance': False,
            'scope': 'Fresh author attempt only; archived work preserved, all validation remains required.'}
    create_json(path, plan)
    return plan


def apply(root, plan_path, plan_sha, receipt_path):
    root, plan_path = Path(root).resolve(), Path(plan_path)
    if r.digest(plan_path) != plan_sha:
        raise ValueError('auth plan hash differs')
    plan = r.read(plan_path)
    if (plan.get('kind') != 'terminal-auth-retry-plan' or plan.get('run') != str(root)
            or plan.get('verifiers') != verifier_hashes() or plan.get('acceptance') is not False
            or not isinstance(plan.get('jobs'), list) or not 1 <= len(plan['jobs']) <= 96
            or len({x['attempt_id'] for x in plan['jobs']}) != len(plan['jobs'])):
        raise ValueError('auth plan identity differs')
    q = existing_queue(root)
    with closing(q._connect()) as db:
        if inventory(db, root) != plan['inventory']:
            raise ValueError('auth plan rule inventory differs')
    receipt_path = new_output(receipt_path)
    receipt = {'status': 'applying', 'plan_sha256': plan_sha, 'completed': [], 'inflight': None}
    create_json(receipt_path, receipt)
    try:
        for expected in plan['jobs']:
            receipt['inflight'] = expected['attempt_id']; update_receipt(receipt_path, receipt)
            retry(q, root, expected, plan_sha)
            receipt['completed'].append({'job_id': expected['job_id'], 'attempt_id': expected['attempt_id']})
            receipt['inflight'] = None; update_receipt(receipt_path, receipt)
        receipt['status'] = 'passed'; update_receipt(receipt_path, receipt)
        return receipt
    except BaseException:
        receipt['status'] = 'stopped'; update_receipt(receipt_path, receipt)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--attempt', action='append')
    parser.add_argument('--apply-plan-sha256')
    parser.add_argument('--receipt', type=Path)
    args = parser.parse_args()
    if args.apply_plan_sha256:
        if args.attempt or not args.receipt:
            parser.error('apply requires receipt and forbids attempt selection')
        apply(args.run, args.plan, args.apply_plan_sha256, args.receipt)
    else:
        if args.receipt or not args.attempt:
            parser.error('planning requires explicit attempts and forbids receipt')
        dryplan(args.run, args.plan, args.attempt)


if __name__ == '__main__':
    main()
