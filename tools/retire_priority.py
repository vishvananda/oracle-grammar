"""Checkpoint-scoped, heuristic scheduling; never a semantic acceptance test."""
import argparse
from collections import defaultdict
import hashlib
import json
from pathlib import Path
import re
import sqlite3
import statistics
import time

import retire_rehabilitation

MAX_JOBS = 50_000
MAX_EVIDENCE_BYTES = 4096
VERSION = 'frontier-shape-v1'
VARIABLE = re.compile(r'"(?:[^"\\]|\\.)*"|\$[A-Za-z_]\w*')
QUOTED = re.compile(r'("(?:[^"\\]|\\.)*")')


def digest(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False)


def semantic_shape(text):
    """Alpha-rename placeholders, preserving their identity and all constants."""
    names = {}
    def replace(match):
        token = match.group()
        if not token.startswith('$'):
            return token
        if token not in names:
            names[token] = len(names)
        return '$'+str(names[token])
    parts = QUOTED.split(text)
    normalized = ''.join(part if index % 2 else re.sub(r'\s+', ' ', part)
                         for index, part in enumerate(parts)).strip()
    return VARIABLE.sub(replace, normalized)


def _snapshot(campaign):
    db = sqlite3.connect((campaign/'queue.sqlite').as_uri()+'?mode=ro', uri=True, isolation_level=None)
    try:
        db.execute('PRAGMA query_only=ON')
        db.execute('BEGIN')
        latest = db.execute('SELECT commit_id,evidence FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
        rows = db.execute("SELECT id,payload FROM jobs WHERE status='pending' ORDER BY rowid").fetchall()
        db.commit()
    finally:
        db.close()
    if not latest or len(rows) > MAX_JOBS:
        raise ValueError('missing checkpoint or pending inventory exceeds bound')
    state = hashlib.sha256(latest[0].encode()+b'\0')
    for job_id, payload in rows:
        state.update(job_id.encode()+b'\0'+payload.encode()+b'\n')
    return latest, rows, state.hexdigest()


def _file_spec(path):
    data = retire_rehabilitation._read(path)[0]
    return {'path': str(path), 'sha256': digest(data)}


def plan(campaign):
    """Compute one deterministic rank plan from the latest accepted audit snapshot."""
    started = time.monotonic()
    campaign = Path(campaign).resolve()
    (checkpoint, checkpoint_raw), rows, snapshot_sha = _snapshot(campaign)
    checkpoint_data = json.loads((campaign/'checkpoints'/f'{checkpoint}.json').read_text())
    inputs = Path(checkpoint_data['inputs']).resolve()
    evidence = {'kind': 'checkpoint-unused-rehabilitation',
                'scan': _file_spec(inputs/'scan/rules.json'),
                'scan_complete': _file_spec(inputs/'scan-complete.json'),
                'manifest': _file_spec(inputs/'manifest.json'),
                'commands': _file_spec(inputs/'scan/commands.jsonl')}
    _, unused, manifest_sha, scan_sha = retire_rehabilitation.verify_scan(
        campaign, checkpoint, json.loads(checkpoint_raw), evidence)
    scan_bytes = retire_rehabilitation._read(inputs/'scan/rules.json', scan_sha)[0]
    needed = json.loads(scan_bytes)['needed']
    by_face = defaultdict(set)
    for rule, faces in needed.items():
        for face in set(faces):
            by_face[face].add(rule)
    jobs = [(job_id, raw, json.loads(raw)) for job_id, raw in rows]
    author = [(job_id, raw, job) for job_id, raw, job in jobs if job.get('kind') == 'author']
    shapes = defaultdict(lambda: defaultdict(set))
    for job_id, _, job in author:
        for fragment in job.get('source_fragments', []):
            if isinstance(fragment, dict) and fragment.get('name') in job['rules'] and isinstance(fragment.get('sem'), str) and fragment['sem']:
                shapes[semantic_shape(fragment['sem'])][job_id].add(fragment['name'])
    ranked = []
    for ordinal, (job_id, raw, job) in enumerate(author):
        rules = set(job['rules'])
        names = {face for rule in rules for face in needed.get(rule, [])}
        extras = [len(by_face[name]-rules) for name in names]
        ready = sum(extra == 0 for extra in extras)
        near = sum(extra <= 1 for extra in extras)
        siblings = set()
        source_bytes = 0
        seen_fragments = set()
        for fragment in job.get('source_fragments', []):
            if not isinstance(fragment, dict) or fragment.get('name') not in rules:
                continue
            sem, body = fragment.get('sem', ''), fragment.get('body', '')
            if not isinstance(sem, str) or not isinstance(body, str):
                continue
            seen_fragments.add(fragment['name'])
            source_bytes += len(sem.encode()) + len(body.encode())
            if sem and len(shapes[semantic_shape(sem)]) <= 100:
                for other, other_rules in shapes[semantic_shape(sem)].items():
                    if other != job_id:
                        siblings.update(other_rules)
        unknown = bool(job.get('requires_diagnosis')) or seen_fragments != rules
        neighbors = len(set(job.get('syntax_neighbor_faces', [])))
        # Capped, explainable priorities; no number here proves semantics or speed.
        benefit = 12*min(ready, 8) + 3*min(near, 8) + 8*min(len(siblings), 8) + 2*min(len(rules), 6)
        penalty = min(80, len(names)*2 + neighbors//4) + min(50, source_bytes//200) + (20 if unknown else 0)
        components = {'ready_names': ready, 'one_extra_names': near, 'target_names': len(names),
                      'min_extra_rules': min(extras) if extras else None, 'syntax_neighbor_names': neighbors,
                      'same_shape_sibling_rules': len(siblings), 'rule_count': len(rules),
                      'semantic_rhs_bytes': source_bytes, 'requires_diagnosis': unknown,
                      'scan_unused_rules': len(rules & unused), 'benefit': benefit, 'penalty': penalty,
                      'score': benefit-penalty}
        ranked.append({'id': job_id, 'payload_sha256': digest(raw.encode()), 'ordinal': ordinal,
                       'score': benefit-penalty, 'evidence': components})
    ranked.sort(key=lambda item: (-item['score'], item['ordinal']))
    for rank, item in enumerate(ranked):
        item['rank'] = rank
        del item['ordinal']
    return {'schema': 1, 'checkpoint': checkpoint, 'algorithm': VERSION,
            'manifest_sha256': manifest_sha, 'scan_sha256': scan_sha,
            'pending_snapshot_sha256': snapshot_sha, 'snapshot_jobs': len(jobs),
            'mechanical_jobs': sum(job.get('kind') == 'mechanical' for _, _, job in jobs),
            'rows': ranked, 'elapsed_seconds': time.monotonic()-started,
            'notice': 'Selected tagged derivation and matching semantic shapes are scheduling hints, not semantic proof.'}


def verify_plan_inputs(campaign, checkpoint, plan_data):
    """Check the rank plan cites the current checkpoint's completed audit."""
    if not isinstance(checkpoint, str) or not checkpoint or Path(checkpoint).name != checkpoint:
        raise ValueError('invalid priority checkpoint')
    campaign = Path(campaign).resolve()
    db = sqlite3.connect((campaign/'queue.sqlite').as_uri()+'?mode=ro', uri=True)
    try:
        row = db.execute('SELECT commit_id,evidence FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
    finally:
        db.close()
    if not row or row[0] != checkpoint:
        raise ValueError('priority checkpoint is stale')
    record = json.loads((campaign/'checkpoints'/f'{checkpoint}.json').read_text())
    inputs = Path(record['inputs']).resolve()
    proof = {'kind': 'checkpoint-unused-rehabilitation',
             'scan': _file_spec(inputs/'scan/rules.json'), 'scan_complete': _file_spec(inputs/'scan-complete.json'),
             'manifest': _file_spec(inputs/'manifest.json'), 'commands': _file_spec(inputs/'scan/commands.jsonl')}
    _, _, manifest_sha, scan_sha = retire_rehabilitation.verify_scan(campaign, checkpoint, json.loads(row[1]), proof)
    if (not isinstance(plan_data, dict) or plan_data.get('manifest_sha256') != manifest_sha
            or plan_data.get('scan_sha256') != scan_sha):
        raise ValueError('priority plan audit hashes differ')
    return manifest_sha, scan_sha


def initialize(db):
    db.executescript('''
        CREATE TABLE IF NOT EXISTS priority_snapshots (
            checkpoint TEXT PRIMARY KEY REFERENCES checkpoints(commit_id), metadata TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS job_priorities (
            checkpoint TEXT NOT NULL REFERENCES priority_snapshots(checkpoint),
            job_id TEXT NOT NULL REFERENCES jobs(id), rank INTEGER NOT NULL,
            payload_sha256 TEXT NOT NULL, evidence TEXT NOT NULL,
            PRIMARY KEY(checkpoint, job_id)
        );
        CREATE INDEX IF NOT EXISTS job_priorities_rank ON job_priorities(checkpoint, rank, job_id);
        CREATE TABLE IF NOT EXISTS priority_rotation (
            checkpoint TEXT PRIMARY KEY REFERENCES priority_snapshots(checkpoint), author_claims INTEGER NOT NULL
        );
    ''')


def publish(db, checkpoint, plan_data):
    """Publish once under caller's immediate transaction; skip changed/nonpending jobs."""
    if not isinstance(plan_data, dict) or plan_data.get('schema') != 1 or plan_data.get('checkpoint') != checkpoint:
        raise ValueError('priority plan identity differs')
    if plan_data.get('algorithm') != VERSION:
        raise ValueError('unsupported priority algorithm')
    for key in ('manifest_sha256', 'scan_sha256', 'pending_snapshot_sha256'):
        if not isinstance(plan_data.get(key), str) or not re.fullmatch('[0-9a-f]{64}', plan_data[key]):
            raise ValueError('priority plan lacks immutable '+key)
    rows = plan_data.get('rows')
    if not isinstance(rows, list) or len(rows) > MAX_JOBS:
        raise ValueError('priority row limit exceeded')
    seen = set()
    for row in rows:
        if (not isinstance(row, dict) or not isinstance(row.get('id'), str) or not row['id']
                or row['id'] in seen or type(row.get('rank')) is not int or not 0 <= row['rank'] < MAX_JOBS
                or not isinstance(row.get('payload_sha256'), str) or not re.fullmatch('[0-9a-f]{64}', row['payload_sha256'])
                or not isinstance(row.get('evidence'), dict) or len(canonical(row['evidence']).encode()) > MAX_EVIDENCE_BYTES):
            raise ValueError('invalid priority row')
        seen.add(row['id'])
    current = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
    if not current or current[0] != checkpoint:
        raise ValueError('priority checkpoint is stale')
    metadata = {key: plan_data[key] for key in ('schema', 'checkpoint', 'algorithm', 'manifest_sha256',
                                                 'scan_sha256', 'pending_snapshot_sha256')}
    try:
        db.execute('INSERT INTO priority_snapshots(checkpoint,metadata) VALUES (?,?)', (checkpoint, canonical(metadata)))
    except sqlite3.IntegrityError as exc:
        raise ValueError('priority snapshot already exists') from exc
    db.execute('INSERT INTO priority_rotation(checkpoint,author_claims) VALUES (?,0)', (checkpoint,))
    inserted = 0
    for row in rows:
        existing = db.execute('SELECT status,payload FROM jobs WHERE id=?', (row['id'],)).fetchone()
        if not existing or existing[0] != 'pending' or digest(existing[1].encode()) != row['payload_sha256']:
            continue
        db.execute('INSERT INTO job_priorities(checkpoint,job_id,rank,payload_sha256,evidence) VALUES (?,?,?,?,?)',
                   (checkpoint, row['id'], row['rank'], row['payload_sha256'], canonical(row['evidence'])))
        inserted += 1
    return {'checkpoint': checkpoint, 'inserted': inserted, 'skipped': len(rows)-inserted, 'metadata': metadata}


def candidates(db, checkpoint):
    """Prioritize mechanical; every fourth ranked author claim uses FIFO."""
    rotation = db.execute('SELECT author_claims FROM priority_rotation WHERE checkpoint=?', (checkpoint,)).fetchone()
    fifo = rotation is not None and rotation[0] % 4 == 3
    return db.execute("SELECT j.id,j.payload FROM jobs j LEFT JOIN job_priorities p "
                      "ON p.job_id=j.id AND p.checkpoint=? WHERE j.status='pending' "
                      "ORDER BY CASE WHEN json_extract(j.payload,'$.kind')='mechanical' THEN 0 ELSE 1 END, "
                      "CASE WHEN ? THEN 0 ELSE p.rank IS NULL END, "
                      "CASE WHEN ? THEN j.rowid ELSE p.rank END, j.rowid", (checkpoint, fifo, fifo))


def claimed(db, checkpoint, job):
    if job.get('kind') == 'author':
        db.execute('UPDATE priority_rotation SET author_claims=(author_claims+1)%4 WHERE checkpoint=?', (checkpoint,))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('campaign', type=Path)
    parser.add_argument('--limit', type=int, default=10)
    parser.add_argument('--full', action='store_true', help='output complete plan for later explicit publication')
    args = parser.parse_args()
    if not 1 <= args.limit <= 100:
        parser.error('--limit must be between 1 and 100')
    result = plan(args.campaign)
    rows = result['rows']
    scores = [row['score'] for row in rows]
    result['ranked_jobs'] = len(rows)
    result['score_median'] = statistics.median(scores) if scores else None
    result['score_min'] = min(scores) if scores else None
    result['score_max'] = max(scores) if scores else None
    if not args.full:
        result.pop('rows')
        result['top'] = rows[:args.limit]
    print(json.dumps(result, sort_keys=True, allow_nan=False))


if __name__ == '__main__':
    main()
