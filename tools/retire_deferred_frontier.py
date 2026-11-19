#!/usr/bin/env python3
"""Fill idle Terra slots with independent inherited author assignments."""

import argparse
from collections import defaultdict
from contextlib import closing
import json
from pathlib import Path
import sqlite3

import retire as r
from retire_queue import Queue, RehabilitationIneligible
from retire_rehabilitation import deferred_author_targets, resolve_origin, scan_evidence, verify_scan

MAX_WAVE = 12
MAX_PARKED = 5000


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
            if job.get('kind') != 'author' or not 1 <= len(job.get('rules', [])) <= 3:
                continue
            origin = resolve_origin(db, job_id, row)
            if origin is not None:
                candidates.append((job, origin[1]['job_id']))
        return checkpoint, active, candidates


def _rank(needed, unused, active, candidates, limit):
    by_face = defaultdict(set)
    for rule, faces in needed.items():
        for face in faces:
            by_face[face].add(rule)
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
        if not 1 <= len(faces) <= 8 or busy.intersection(faces):
            continue
        rules = set(job['rules'])
        other = [len(by_face[face]-rules) for face in faces]
        rank = (-sum(count == 0 for count in other), -sum(count <= 1 for count in other),
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('campaign', type=Path)
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--checkpoint', metavar='COMMIT')
    parser.add_argument('--protect', action='append', default=[], metavar='JOB')
    parser.add_argument('--max-wave', type=int, choices=range(1,MAX_WAVE+1), default=MAX_WAVE)
    args = parser.parse_args()
    print(json.dumps(refill(args.campaign, apply=args.apply, protected=args.protect, max_wave=args.max_wave,
                            expected_checkpoint=args.checkpoint)))


if __name__ == '__main__':
    main()
