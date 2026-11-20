#!/usr/bin/env python3
"""Fill idle Terra slots with independent author assignments."""

import argparse
from collections import defaultdict
from contextlib import closing
from functools import lru_cache
import json
from pathlib import Path
import re
import sqlite3

import retire as r
from retire_queue import RETRY_UNBLOCKED_AUTHOR, Queue, RehabilitationIneligible
from retire_rehabilitation import (deferred_author_targets, resolve_origin, scan_evidence,
                                   verified_dependency_scan, verify_scan)

MAX_WAVE = 12
MAX_PARKED = 5000
SMALL_RULES = 3
SMALL_FACES = 8
MAX_RULES = 5
MAX_FACES = 16
MAX_RETRY_RULES = 6


def _snapshot(campaign, limit, protected):
    uri = (campaign/'queue.sqlite').as_uri()+'?mode=ro'
    with closing(sqlite3.connect(uri, uri=True)) as db:
        db.execute('BEGIN')
        checkpoint = db.execute('SELECT commit_id,evidence FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
        active = [json.loads(row[0]) for row in db.execute(
            "SELECT payload FROM jobs WHERE status IN ('pending','running') LIMIT ?", (limit,))]
        if not checkpoint or len(active) >= limit:
            return checkpoint, active, []
        rows = db.execute("SELECT id,status,attempt_id,payload,adjudication FROM jobs "
                          "WHERE status='needs-review' AND attempt_id IS NULL ORDER BY id LIMIT ?",
                          (MAX_PARKED+1,)).fetchall()
        r.require(len(rows) <= MAX_PARKED, 'deferred author inventory exceeds the planning bound')
        candidates = []
        for job_id, *row in rows:
            if job_id in protected:
                continue
            job = json.loads(row[2])
            if job.get('kind') != 'author' or not 1 <= len(job.get('rules', [])) <= MAX_RULES:
                continue
            origin = resolve_origin(db, job_id, row)
            if origin is not None:
                candidates.append((job, origin[1]['job_id']))
        return checkpoint, active, candidates


def _by_face(needed):
    by_face = defaultdict(set)
    for rule, faces in needed.items():
        for face in faces:
            by_face[face].add(rule)
    return by_face


def _rank(needed, unused, active, candidates, limit):
    by_face = _by_face(needed)
    busy = {face for job in active for face in job.get('target_faces', [])}
    origins = {(job.get('review_origin') or {}).get('job_id', job['id']) for job in active}
    ranked = []
    for job, origin in candidates:
        if origin in origins:
            continue
        try:
            faces = deferred_author_targets(needed, unused, job)
        except ValueError:
            continue
        if not 1 <= len(faces) <= MAX_FACES or busy.intersection(faces):
            continue
        rules = set(job['rules'])
        other = [len(by_face[face]-rules) for face in faces]
        larger = len(rules) > SMALL_RULES or len(faces) > SMALL_FACES
        rank = (larger, -sum(count == 0 for count in other), -sum(count <= 1 for count in other),
                max(other), len(rules), len(faces), job['id'])
        ranked.append((rank, origin, faces, job))
    selected = []
    for _, origin, faces, job in sorted(ranked):
        if len(selected) >= limit:
            break
        if origin not in origins and not busy.intersection(faces):
            selected.append({'job_id':job['id'], 'origin':origin, 'rules':job['rules'], 'target_faces':faces})
            busy.update(faces)
            origins.add(origin)
    return selected


def refill(campaign, *, apply=False, protected=(), max_wave=MAX_WAVE, expected_checkpoint=None):
    """Plan or admit one bounded wave through the verified queue interface."""
    campaign = Path(campaign).resolve()
    workers = r.read(campaign/'config.json')['workers']
    r.require(type(workers) is int and 1 <= workers <= MAX_WAVE
              and type(max_wave) is int and 1 <= max_wave <= MAX_WAVE, 'invalid worker or wave limit')
    with r.locked(campaign/'deferred-frontier.lock', blocking=False) as owned:
        if not owned or any((campaign/name).exists() for name in ('drain.json','controller-drain.json')):
            return {'status':'idle','jobs':[]}
        head, active, candidates = _snapshot(campaign, workers, frozenset(protected))
        result = {'status':'planned','checkpoint':head[0] if head else None,
                  'active_jobs':len(active),'jobs':[]}
        if expected_checkpoint is not None and result['checkpoint'] != expected_checkpoint:
            return dict(result, status='stale-checkpoint')
        if not candidates:
            return result
        checkpoint, evidence = head
        record = r.read(campaign/'checkpoints'/f'{checkpoint}.json')
        inputs = Path(record['inputs']).resolve()
        proof = scan_evidence(inputs)
        _, unused, _, digest = verify_scan(campaign, checkpoint, json.loads(evidence), proof)
        data = r.read(inputs/'scan/rules.json')
        r.require(r.digest(inputs/'scan/rules.json') == digest, 'deferred scan changed after verification')
        wave = _rank(data['needed'], unused, active, candidates, min(max_wave, workers-len(active)))
        if not apply:
            result['jobs'] = wave
            return result
        queue = Queue(campaign/'queue.sqlite')
        result['status'] = 'applied'
        for row in wave:
            try:
                receipt = queue.queue_deferred_author(row['job_id'], checkpoint, proof)
            except RehabilitationIneligible as error:
                result['jobs'].append(dict(row, status='stale', reason=str(error)))
            else:
                result['jobs'].append(dict(row, status='queued', receipt=receipt))
        return result


def plan_unblocked(campaign, *, protected=()):
    """Rank parked assignments using changed seed dependencies on their approved faces."""
    campaign = Path(campaign).resolve()
    uri = (campaign/'queue.sqlite').as_uri()+'?mode=ro'
    with closing(sqlite3.connect(uri, uri=True)) as db:
        db.execute('BEGIN')
        history = {commit: json.loads(evidence) for commit, evidence in
                   db.execute('SELECT commit_id,evidence FROM checkpoints ORDER BY seq DESC')}
        jobs = db.execute("""SELECT j.id,j.attempt_id,j.payload,a.base_commit FROM jobs j
                             JOIN attempts a ON a.id=j.attempt_id AND a.job_id=j.id
                             WHERE j.status='needs-review' AND a.result IS NOT NULL
                                 AND json_extract(j.payload,'$.kind')='author'
                             ORDER BY j.rowid LIMIT ?""", (MAX_PARKED+1,)).fetchall()
        retried = {job for job, in db.execute("SELECT json_extract(data,'$.job_id') FROM events WHERE kind=?",
                                             (RETRY_UNBLOCKED_AUTHOR,))}
        accepted = {name for name, in db.execute("SELECT r.rule FROM rules r JOIN jobs j ON j.id=r.job_id "
                                               "WHERE j.status='accepted'")}
    r.require(history and len(jobs) <= MAX_PARKED, 'dependency retry inventory exceeds the planning bound')
    checkpoint = next(iter(history))
    excluded = retried | set(protected)

    def scan(commit):
        record = r.read(campaign/'checkpoints'/f'{commit}.json')
        return verified_dependency_scan(campaign, commit, history[commit], scan_evidence(Path(record['inputs'])))

    population, needed, unused = scan(checkpoint)
    absent = accepted.difference(needed, unused)
    now = _by_face(needed)

    @lru_cache(maxsize=80)
    def previous(commit):
        basis, earlier, _ = scan(commit)
        if basis != population:
            raise ValueError('the dependency scan uses a different approved population')
        return _by_face({name: faces for name, faces in earlier.items() if name in absent})

    candidates, unavailable = [], set()
    for job_id, attempt, payload, base in jobs:
        if job_id in excluded or base == checkpoint or base not in history or base in unavailable:
            continue
        job = json.loads(payload)
        if not 1 <= len(job['rules']) <= MAX_RETRY_RULES:
            continue
        try:
            faces = deferred_author_targets(needed, unused, job)
        except ValueError:
            continue
        if len(faces) > MAX_FACES:
            continue
        try:
            earlier = previous(base)
        except (OSError, ValueError, KeyError, TypeError):
            unavailable.add(base)
            continue
        rules = set(job['rules'])
        witnesses = {face: sorted(earlier.get(face, set()) - rules) for face in faces if earlier.get(face)}
        if witnesses:
            candidates.append({'job':job_id,'attempt':attempt,'origin':base,'rules':job['rules'],
                               'faces':faces,'unblocked':witnesses,
                               'remaining_neighbors':sum(len(now[face]-rules) for face in faces)})
    candidates.sort(key=lambda row: (-len(row['unblocked']) / len(row['faces']), row['remaining_neighbors'],
                                     len(row['faces']), -sum(map(len, row['unblocked'].values())), row['job']))
    return {'checkpoint':checkpoint, 'verified_candidates':len(candidates),
            'unavailable_origin_scans':len(unavailable), 'candidates':candidates}


def refill_unblocked(campaign, plan, *, apply=False, protected=(), max_wave=MAX_WAVE):
    """Consume a ranked plan while the queue rechecks each selected retirement witness."""
    campaign = Path(campaign).resolve()
    workers = r.read(campaign/'config.json')['workers']
    r.require(type(workers) is int and 1 <= workers <= MAX_WAVE
              and type(max_wave) is int and 1 <= max_wave <= MAX_WAVE, 'invalid worker or wave limit')
    checkpoint = plan['checkpoint']
    r.require(re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}', checkpoint) is not None
              and isinstance(plan['candidates'], list) and len(plan['candidates']) <= MAX_PARKED,
              'invalid dependency retry plan')
    with r.locked(campaign/'deferred-frontier.lock', blocking=False) as owned:
        if not owned or any((campaign/name).exists() for name in ('drain.json','controller-drain.json')):
            return {'status':'idle','jobs':[]}
        head, active, _ = _snapshot(campaign, workers, frozenset(protected))
        if not head or head[0] != checkpoint:
            return {'status':'stale-checkpoint','jobs':[]}
        limit = min(max_wave, workers-len(active))
        result = {'status':'planned','checkpoint':checkpoint,'active_jobs':len(active),'jobs':[]}
        busy = {face for job in active for face in job.get('target_faces', [])}
        excluded = set(protected)
        queue = Queue(campaign/'queue.sqlite') if apply and limit else None
        current = r.read(campaign/'checkpoints'/f'{checkpoint}.json')
        proof = scan_evidence(Path(current['inputs']))
        queued = 0
        for row in plan['candidates']:
            if queued >= limit:
                break
            job_id, base, faces = row['job'], row['origin'], row['faces']
            if job_id in excluded or busy.intersection(faces):
                continue
            r.require(re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}', base) is not None,
                      'invalid dependency retry origin')
            if queue is not None:
                origin = r.read(campaign/'checkpoints'/f'{base}.json')
                earlier = {'attempt_id':row['attempt'], 'scan':scan_evidence(Path(origin['inputs']))}
                try:
                    receipt = queue.queue_deferred_author(job_id, checkpoint, proof, unblocked=earlier)
                except RehabilitationIneligible as error:
                    result['jobs'].append(dict(row, status='stale', reason=str(error)))
                    continue
                result['jobs'].append(dict(row, status='queued', receipt=receipt))
                faces = receipt['target_faces']
            else:
                result['jobs'].append(row)
            busy.update(faces)
            excluded.add(job_id)
            queued += 1
        return dict(result, status='applied') if apply else result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('campaign', type=Path)
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--checkpoint', metavar='COMMIT')
    parser.add_argument('--protect', action='append', default=[], metavar='JOB')
    parser.add_argument('--max-wave', type=int, choices=range(1,MAX_WAVE+1), default=MAX_WAVE)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--plan-unblocked', action='store_true')
    mode.add_argument('--unblocked-plan', type=Path)
    args = parser.parse_args()
    options = dict(apply=args.apply, protected=args.protect, max_wave=args.max_wave)
    if args.plan_unblocked:
        if args.apply:
            parser.error('--plan-unblocked only reads accepted inputs and the queue')
        result = plan_unblocked(args.campaign, protected=args.protect)
        if args.checkpoint and args.checkpoint != result['checkpoint']:
            parser.error('--checkpoint differs from the accepted dependency scan')
    elif args.unblocked_plan:
        plan = r.read(args.unblocked_plan)
        if args.checkpoint and args.checkpoint != plan['checkpoint']:
            parser.error('--checkpoint differs from the dependency retry plan')
        result = refill_unblocked(args.campaign, plan, **options)
    else:
        result = refill(args.campaign, expected_checkpoint=args.checkpoint, **options)
    print(json.dumps(result))


if __name__ == '__main__':
    main()
