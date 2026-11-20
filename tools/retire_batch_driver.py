#!/usr/bin/env python3
"""Run the existing batch pipeline with a durable lease per batch."""
import argparse
from collections import Counter, defaultdict
from contextlib import closing
import hashlib
import json
import os
from pathlib import Path
import sys
import time

sys.dont_write_bytecode = True

import retire as r
import mined
import retire_campaign as campaign
import retire_dispatch as dispatch
from retire_library_readmission import LIBRARY_REJECTION
from retire_parse_readmission import PARSE_SCREEN_REJECTION, PRINT_ONLY_REJECTION, awaiting_print_recovery
from retire_queue import Queue
from retire_rehabilitation import scan_evidence, verify_scan
from retire_tasks import validate_runner


def _runner(run, runner):
    selected = validate_runner(run, runner)
    r.require(r.digest(selected/'retire_batch_driver.py') == r.digest(Path(__file__)),
              'batch driver must execute from the selected runner version')
    for name in campaign.RUNNERS:
        module = sys.modules.get(Path(name).stem)
        source = getattr(module, '__file__', None)
        if source:
            r.require((selected/name).is_file() and r.digest(selected/name) == r.digest(Path(source)),
                      'batch driver imports differ from the selected runner version: '+name)
    return selected


def _recorded_recovery(events, job, checkpoint, expected):
    reason = expected.get('validation', {}).get('reason')
    recovered_kinds = {
        PRINT_ONLY_REJECTION: ('readmitted-native-batch', 'isolated-full-check-revalidation'),
        PARSE_SCREEN_REJECTION: ('readmitted-parse-screen-batch', 'isolated-parse-screen-revalidation'),
        LIBRARY_REJECTION: ('readmitted-library-batch', 'isolated-batch-library-revalidation'),
    }
    recovery = None
    if job.get('status') == 'integration-rejected':
        kinds = ('retry-reviewed-repair', 'retry-staging-reviewed-repair')
    elif job.get('status') == 'included' and reason in recovered_kinds:
        recovery = recovered_kinds[reason]
        kinds = (recovery[0],)
    else:
        return False
    original = {'status': 'integration-rejected', 'checkpoint': None, 'evidence': expected}
    matches = [event['data'] for event in events
               if event['kind'] in kinds
               and event['data'].get('job_id') == job['job_id']
               and event['data'].get('attempt_id') == job['attempt_id']
               and event['data'].get('previous_adjudication') == original]
    if len(matches) != 1 or matches[0].get('checkpoint') != checkpoint:
        return False
    event = matches[0]
    if recovery:
        return (event.get('manifest') == expected['manifest']
                and event.get('evidence', {}).get('kind') == recovery[1])
    return event.get('previous_status') == 'integration-rejected'


def _recorded_rehabilitation(events, parent, job, checkpoint, expected):
    if (not parent or parent['attempt_id'] != job['attempt_id']
            or parent.get('adjudication') != {'status': 'integration-rejected',
                                              'checkpoint': None, 'evidence': expected}):
        return False
    identities = [(event['data'].get('parent_job'), event['data'].get('origin_attempt'),
                   event['data'].get('checkpoint')) for event in events
                  if event['kind'] == 'rehabilitated-unused']
    return identities.count((job['job_id'], job['attempt_id'], checkpoint)) == 1


def _batch_observation(run, batch_id, queue, checkpoint):
    if not isinstance(batch_id, str) or not batch_id or Path(batch_id).name != batch_id or batch_id in ('.', '..'):
        raise ValueError('dispatch returned an invalid batch manifest identity')
    folder = run/'.cache/batches'/batch_id
    manifest_path = folder/'manifest.json'
    if not manifest_path.is_file() or manifest_path.resolve() != manifest_path:
        raise ValueError('dispatched batch manifest is missing or unsafe')
    manifest = r.read(manifest_path)
    result = {'manifest': str(manifest_path), 'manifest_sha256': r.digest(manifest_path),
              'stage_status': manifest.get('status'), 'candidate': manifest.get('commit')}
    if manifest.get('base_commit') != checkpoint or manifest.get('id') != folder.name:
        raise ValueError('batch manifest identity differs')
    jobs = manifest.get('jobs')
    if (not isinstance(jobs, list) or not jobs or any(not isinstance(j, dict) for j in jobs)
            or any(not j.get('job_id') or not j.get('attempt_id') for j in jobs)
            or len({j['job_id'] for j in jobs}) != len(jobs)):
        raise ValueError('batch manifest job evidence is incomplete')
    stage = manifest.get('status')
    if stage == 'no-proposals':
        if any(j.get('status') == 'included' for j in jobs):
            raise ValueError('empty batch contains included jobs')
        result.update(outcome='rejected', reason=manifest.get('reason', 'all proposals rejected during staging'))
    elif stage == 'provisional':
        validation_path = folder/'validation.json'
        validation = r.read(validation_path)
        result.update(validation=str(validation_path), validation_sha256=r.digest(validation_path),
                      validation_status=validation.get('status'))
        if (validation.get('candidate') != manifest.get('commit')
                or validation.get('source_checkpoint') != checkpoint):
            raise ValueError('batch validation identity differs')
        if validation.get('status') == 'passed':
            result['outcome'] = 'passed'
            return result
        if validation.get('status') != 'failed' or validation.get('reason') not in (
                LIBRARY_REJECTION, 'explicit candidate grammar tests failed',
                PRINT_ONLY_REJECTION, PARSE_SCREEN_REJECTION,
                'optimized family report contains failures'):
            raise ValueError('batch validation failed operationally: '+str(validation_path))
        result.update(outcome='rejected', reason=validation['reason'])
    else:
        raise ValueError('batch staging failed operationally: '+str(manifest_path))
    rejected = {row['id']: row for row in queue.get_jobs('integration-rejected')}
    events = None
    superseded = None
    for job in jobs:
        row = rejected.get(job['job_id'])
        evidence = (row.get('adjudication') or {}).get('evidence', {}) if row else {}
        expected = ({'manifest': folder.name, 'validation': validation}
                    if job.get('status') == 'included' and stage == 'provisional'
                    else {'manifest': folder.name, 'reason': 'merge or staging rejected'})
        if not row or row['attempt_id'] != job['attempt_id'] or evidence != expected:
            # A verified recovery may finish between batch adjudication and observation.
            if events is None:
                events = queue.audit()
            if _recorded_recovery(events, job, checkpoint, expected):
                continue
            if superseded is None:
                superseded = {row['id']: row for row in queue.get_jobs('superseded')}
            if not _recorded_rehabilitation(events, superseded.get(job['job_id']), job, checkpoint, expected):
                raise ValueError('rejected batch job disposition is missing or differs')
    return result


def recovered_cohort(queue, checkpoint):
    """Select the oldest current-attempt readmission group as a routing hint."""
    kinds = ('readmitted-batch', 'readmitted-native-batch', 'readmitted-library-batch',
             'readmitted-native-exit-one-batch', 'readmitted-parse-screen-batch', 'readmitted-shape-baseline')
    with closing(queue._connect()) as db:
        db.execute('BEGIN')
        latest = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
        if not latest or latest[0] != checkpoint:
            raise ValueError('checkpoint changed during recovered cohort selection')
        rows = db.execute("""SELECT e.seq, j.id, j.attempt_id,
                json_extract(e.data,'$.attempt_id'), json_extract(e.data,'$.manifest'),
                json_extract(e.data,'$.checkpoint'), json_extract(e.data,'$.job_id')
            FROM events e JOIN jobs j ON j.id=json_extract(e.data,'$.job_id')
            WHERE j.status='provisional-pass' AND e.kind IN (?,?,?,?,?,?)
            ORDER BY e.seq DESC""", kinds)
        seen, groups = set(), {}
        for seq, job_id, current_attempt, attempt, manifest, origin_checkpoint, event_job in rows:
            if job_id in seen:
                continue
            seen.add(job_id)
            if not isinstance(event_job, str) or event_job != job_id:
                raise ValueError('recovered event job identity is malformed')
            if not isinstance(attempt, str) or not attempt:
                raise ValueError('recovered event attempt identity is malformed')
            if attempt != current_attempt:
                continue
            if any(not isinstance(value, str) or not value for value in
                   (job_id, current_attempt, manifest, origin_checkpoint)):
                raise ValueError('recovered event cohort identity is malformed')
            groups.setdefault((manifest, origin_checkpoint), []).append((seq, job_id))
        if not groups:
            return []
        key, group = min(groups.items(), key=lambda item: min(seq for seq, _ in item[1]))
        return {'manifest': key[0], 'checkpoint': key[1],
                'jobs': [job for _, job in sorted(group)[:96]]}


def mechanical_lane(queue, limit, minimum=None):
    """Return FIFO mechanical and author selections without altering queue state."""
    if minimum is None:
        minimum = limit
    mechanical, authors = [], []
    for row in queue.get_jobs('provisional-pass'):
        job_id, payload = row.get('id'), row.get('job')
        if not isinstance(job_id, str) or not job_id or not isinstance(payload, dict):
            raise ValueError('provisional job identity is malformed')
        kind = payload.get('kind')
        if kind == 'mechanical':
            mechanical.append(job_id)
        elif kind == 'author':
            authors.append(job_id)
        else:
            raise ValueError('provisional job kind is unsupported')
    if len(mechanical) >= minimum:
        return {'lane': 'mechanical', 'jobs': mechanical[:96], 'combine': True}
    if mechanical:
        return {'lane': 'mechanical-first', 'jobs': (mechanical+authors)[:limit],
                'mechanical': mechanical, 'combine': False}
    return None


def reviewed_replay_jobs(queue, checkpoint, limit):
    """Prioritize ready attempts that independently replayed a reviewed draft."""
    with closing(queue._connect()) as db:
        db.execute('BEGIN')
        latest = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
        if not latest or latest[0] != checkpoint:
            raise ValueError('checkpoint changed during reviewed replay selection')
        return [row[0] for row in db.execute("""SELECT j.id FROM jobs j
            JOIN attempts a ON a.id=j.attempt_id AND a.job_id=j.id
            JOIN checkpoints c ON c.commit_id=a.base_commit
            WHERE j.status='provisional-pass'
                AND json_extract(a.result,'$.status')='provisional-pass'
                AND json_extract(a.result,'$.evidence.reviewed_replay.kind')='independently-reviewed-draft'
                AND json_extract(a.result,'$.evidence.reviewed_replay.rebased_onto')=a.base_commit
            ORDER BY c.seq DESC,j.rowid LIMIT ?""", (limit,))]


def verified_ready_lane(queue, checkpoint, limit, minimum, mechanical=(), recovered=()):
    """Fill a batch with mechanical work and independently replayed drafts."""
    if not minimum:
        return None
    excluded = set(recovered)
    ready = [job for job in mechanical if job not in excluded]
    reviewed = reviewed_replay_jobs(queue, checkpoint, limit+len(excluded))
    ready.extend(job for job in reviewed if job not in excluded and job not in ready)
    if len(ready) >= minimum:
        return {'lane': 'verified-ready', 'jobs': ready[:limit], 'combine': True}
    return None


def stale_provisional_jobs(queue, checkpoint, hours, limit):
    """Return the oldest ready jobs whose passing attempt finished before the age bound."""
    if limit <= 0:
        return []
    with closing(queue._connect()) as db:
        db.execute('BEGIN')
        latest = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
        if not latest or latest[0] != checkpoint:
            raise ValueError('checkpoint changed during stale provisional selection')
        return [row[0] for row in db.execute("""SELECT j.id FROM jobs j
            JOIN events e ON e.kind='finished' AND json_extract(e.data,'$.attempt_id')=j.attempt_id
            WHERE j.status='provisional-pass' AND unixepoch(e.at) <= unixepoch('now')-?
            GROUP BY j.id ORDER BY MIN(j.rowid) LIMIT ?""", (int(hours*3600), limit))]


def stale_lane(queue, checkpoint, config, since_stale, limit):
    """Route aged ready work periodically so verified lanes cannot starve it."""
    hours = config.get('stale_provisional_hours')
    if hours is None:
        return None
    every = config.get('stale_provisional_every', 4)
    size = config.get('stale_provisional_batch_size', limit)
    minimum = config.get('stale_provisional_minimum', 8)
    if (type(hours) not in (int, float) or not 1 <= hours <= 240
            or type(every) is not int or not 2 <= every <= 20
            or type(size) is not int or not 1 <= size <= 96
            or type(minimum) is not int or not 1 <= minimum <= size):
        raise ValueError('stale provisional settings are invalid')
    if since_stale + 1 < every:
        return None
    jobs = stale_provisional_jobs(queue, checkpoint, hours, size)
    if len(jobs) < minimum:
        return None
    return {'lane': 'stale-provisional', 'jobs': jobs, 'combine': True}


def _preflight_document(path, limit, *, sha256=None):
    try:
        if path.resolve() != path or path.is_symlink() or not path.is_file():
            return None
        with path.open('rb') as stream:
            data = stream.read(limit+1)
        if len(data) > limit or (sha256 is not None and hashlib.sha256(data).hexdigest() != sha256):
            return None
        return json.loads(data)
    except (OSError, ValueError, RecursionError):
        return None


def _preflight_receipt(run, config):
    explicit = config.get('batch_preflight_selection')
    if explicit is not None:
        return explicit
    # The planning service publishes this envelope atomically. Publishing it
    # leaves concurrent campaign and retained-control configuration alone.
    return _preflight_document(run/'batch-preflight-ready.json', 4096)


def preflight_ready_lane(queue, checkpoint, limit, spec):
    """Route an exact projected selection through ordinary staging only at its parent."""
    if (not isinstance(spec, dict) or set(spec) != {'path', 'sha256'} or not isinstance(spec['path'], str)
            or not isinstance(spec['sha256'], str) or len(spec['sha256']) != 64):
        return None
    plan = _preflight_document(Path(spec['path']), 128*1024, sha256=spec['sha256'])
    if (not isinstance(plan, dict) or plan.get('kind') != 'prospective-retirement-batch'
            or plan.get('checkpoint') != checkpoint):
        return None
    entries = plan.get('jobs')
    if not isinstance(entries, list) or not 1 <= len(entries) <= limit:
        return None
    if any(not isinstance(entry, dict) or set(entry) != {'job', 'attempt'}
           or not all(isinstance(value, str) and value for value in entry.values()) for entry in entries):
        return None
    jobs = [entry['job'] for entry in entries]
    current = {row['id']: row['attempt_id'] for row in queue.get_jobs('provisional-pass')}
    if len(set(jobs)) != len(jobs) or any(current.get(entry['job']) != entry['attempt'] for entry in entries):
        return None
    return {'lane': 'preflight-verified', 'jobs': jobs, 'combine': True}


def unused_author_jobs(queue, checkpoint, limit, covered):
    """Prioritize provisional author jobs whose entire assignment is now unused."""
    if limit <= 0:
        return []
    root = Path(queue.path).parent
    covered = set(covered)
    with closing(queue._connect()) as db:
        db.execute('BEGIN')
        latest = db.execute('SELECT commit_id,evidence FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
        if not latest or latest[0] != checkpoint:
            raise ValueError('checkpoint changed during unused author selection')
        inputs = Path(campaign.checkpoint(root, checkpoint)['inputs'])
        _, unused, _, _ = verify_scan(root, checkpoint, json.loads(latest[1]), scan_evidence(inputs))
        if not unused:
            return []
        selected = []
        candidates = db.execute("""SELECT j.id,j.payload FROM jobs j
            JOIN attempts a ON a.id=j.attempt_id AND a.job_id=j.id
            WHERE j.status='provisional-pass' AND json_extract(j.payload,'$.kind')='author'
                AND json_extract(a.result,'$.status')='provisional-pass'
            ORDER BY j.rowid""")
        for job, payload in candidates:
            rules = json.loads(payload)['rules']
            if job not in covered and rules and unused.issuperset(rules):
                selected.append(job)
                if len(selected) == limit:
                    break
        return selected


def dependency_caller_jobs(queue, checkpoint, covered, limit):
    """Prioritize ready jobs whose seeded callers keep parked rules reachable."""
    if limit <= 0:
        return []
    with closing(queue._connect()) as db:
        db.execute('BEGIN')
        latest = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
        if not latest or latest[0] != checkpoint:
            raise ValueError('checkpoint changed during caller dependency selection')
        owners = {rule:(job,status) for rule,job,status in db.execute(
            "SELECT r.rule,j.id,j.status FROM rules r JOIN jobs j ON j.id=r.job_id "
            "WHERE j.status<>'accepted'")}
    parked = {rule for rule,(_,status) in owners.items() if status in ('needs-review','integration-rejected')}
    if not parked:
        return []
    source = Path(campaign.checkpoint(Path(queue.path).parent,checkpoint)['source'])
    blockers = defaultdict(set)
    for module in mined.grammar_files(source):
        for caller,entries in mined.definitions(module.read_text().splitlines()).items():
            branches = mined.alternatives(entries)
            if branches is not None and (mined.fallback(caller) or len(set().union(*branches)) != 1):
                continue
            for _,line in entries:
                for callee in parked.intersection(mined.uses(line.partition('::=')[2] if '::=' in line else line)):
                    caller_job,status = owners.get(caller,(None,None))
                    if caller_job != owners[callee][0]:
                        blockers[callee].add(caller_job if status=='provisional-pass' else None)
    ready = Counter()
    for callers in blockers.values():
        if None not in callers:
            remaining = callers.difference(covered)
            if len(remaining) <= limit:
                ready.update(remaining)
    return sorted(ready,key=lambda job:(-ready[job],job))[:limit]


def with_reviewed_replays(queue, checkpoint, slots, limit, fifo, *, preferred=(), callers=()):
    slots = min(slots, max(0, limit-len(preferred)))
    if not slots and not callers and not preferred:
        return fifo
    reviewed = reviewed_replay_jobs(queue, checkpoint, slots) if slots else []
    return list(dict.fromkeys([*preferred, *reviewed, *callers, *fifo]))[:limit]


def recovered_replay_fill(queue, checkpoint, selected, slots, limit):
    """Append independently verified drafts to an underfilled recovery batch."""
    room = min(limit-len(selected), slots)
    if room <= 0:
        return []
    seen = set(selected)
    ranked = reviewed_replay_jobs(queue, checkpoint, room+len(seen))
    return [job for job in ranked if job not in seen][:room]


def run_driver(run, runner, *, flush_once=False, poll_seconds=2):
    run = Path(run).resolve()
    runner = _runner(run, runner)
    started = time.time()
    queue = Queue(run/'queue.sqlite')
    previous = r.read(run/'batch-driver.json') if (run/'batch-driver.json').is_file() else {}
    previous_lane = (previous.get('last_batch') or {}).get('lane')
    since_stale = previous.get('batches_since_stale', 0)
    if type(since_stale) is not int or since_stale < 0:
        since_stale = 0
    state = {'pid': os.getpid(), 'runner': str(runner), 'started_unix': started,
             'last_batch': {'lane': previous_lane} if previous_lane else None,
             'batches_since_stale': since_stale}
    def status(phase, progress=None, **extra):
        state.update(phase=phase, updated_unix=time.time(), **extra)
        if progress is not None:
            state.update(checkpoint=progress['checkpoint'], counts=progress['counts'],
                         job_counts=progress['job_counts'])
        r.write(run/'batch-driver.json', state)

    # This separate singleton protects the status file while the batch lease is awaited.
    with dispatch.lease(run/'batch-driver.lock'):
        status('waiting-lock', queue.progress())
        try:
            last_batch = None
            flush = flush_once
            delay = 0
            while True:
                # Keep the driver singleton while giving repair and publication
                # services a chance to acquire the integration lease.
                if delay:
                    time.sleep(delay)
                delay = poll_seconds
                with dispatch.lease(run/'batch.lock', blocking=True) as fd:
                    r._LEASES.append(fd)
                    try:
                        if last_batch is None:
                            last_batch = time.monotonic()
                        progress = queue.progress()
                        jobs = progress['job_counts']
                        if (run/'drain.json').exists() or (run/'batch-driver-drain.json').exists():
                            status('drained', progress, reason='explicit drain requested')
                            return
                        publication = run/'publication.json'
                        if publication.is_file() and (record := r.read(publication)).get('status') == 'pending':
                            if not dispatch.retry_dirty_publication(run, queue, progress['checkpoint'], record):
                                status('waiting-publication', progress, reason=dispatch.DIRTY_PUBLICATION)
                                continue
                            status('idle', progress, reason=None)
                        config = r.read(run/'config.json')
                        limit = config['first_batch_size'] if progress['checkpoint'] == config['initial_commit'] else config['batch_size']
                        if type(limit) is not int or not 1 <= limit <= 96:
                            raise ValueError('batch size must be between 1 and 96')
                        wait = config['batch_wait_seconds']
                        if type(wait) not in (int, float) or wait < 0:
                            raise ValueError('invalid batch wait')
                        routing = config.get('recovered_cohorts_first', False)
                        if type(routing) is not bool:
                            raise ValueError('recovered_cohorts_first must be boolean')
                        union_trial = config.get('recovered_union_trial', False)
                        if type(union_trial) is not bool:
                            raise ValueError('recovered_union_trial must be boolean')
                        fresh_union = config.get('fresh_union_trial', False)
                        if type(fresh_union) is not bool:
                            raise ValueError('fresh_union_trial must be boolean')
                        mechanical_priority = config.get('mechanical_cohorts_first', False)
                        if type(mechanical_priority) is not bool:
                            raise ValueError('mechanical_cohorts_first must be boolean')
                        mechanical_minimum = config.get('mechanical_batch_minimum', limit)
                        if type(mechanical_minimum) is not int or not 1 <= mechanical_minimum <= 96:
                            raise ValueError('mechanical_batch_minimum must be between 1 and 96')
                        replay_slots = config.get('reviewed_replay_slots', 0)
                        if type(replay_slots) is not int or not 0 <= replay_slots <= 96:
                            raise ValueError('reviewed_replay_slots must be between 0 and 96')
                        recovered_replay_slots = config.get('recovered_reviewed_replay_slots', 0)
                        if type(recovered_replay_slots) is not int or not 0 <= recovered_replay_slots <= 96:
                            raise ValueError('recovered_reviewed_replay_slots must be between 0 and 96')
                        dependency_slots = config.get('dependency_caller_slots', 0)
                        if type(dependency_slots) is not int or not 0 <= dependency_slots <= 96:
                            raise ValueError('dependency_caller_slots must be between 0 and 96')
                        unused_slots = config.get('unused_author_slots', 0)
                        if type(unused_slots) is not int or not 0 <= unused_slots <= 96:
                            raise ValueError('unused_author_slots must be between 0 and 96')
                        ready = jobs.get('provisional-pass', 0)
                        lane = mechanical_lane(queue, limit, mechanical_minimum) if mechanical_priority and ready else None
                        large_mechanical = lane is not None and lane['combine']
                        cohort = recovered_cohort(queue, progress['checkpoint']) if routing and ready and not large_mechanical else []
                        verified_minimum = config.get('verified_ready_minimum', 0)
                        if type(verified_minimum) is not int or not 0 <= verified_minimum <= 96:
                            raise ValueError('verified_ready_minimum must be between 0 and 96')
                        verified_limit = config.get('verified_ready_batch_size', limit)
                        if type(verified_limit) is not int or not 1 <= verified_limit <= 96:
                            raise ValueError('verified_ready_batch_size must be between 1 and 96')
                        verified = None
                        if ready and not large_mechanical:
                            verified = stale_lane(queue, progress['checkpoint'], config, since_stale, limit)
                        if verified is None and ready and not large_mechanical:
                            verified = preflight_ready_lane(queue, progress['checkpoint'], verified_limit,
                                                            _preflight_receipt(run, config))
                            if verified is None and previous_lane != 'verified-ready':
                                verified = verified_ready_lane(queue, progress['checkpoint'], verified_limit,
                                                               min(verified_minimum, verified_limit),
                                                               lane['mechanical'] if lane else (),
                                                               cohort['jobs'] if cohort else ())
                        if verified:
                            lane, cohort = verified, []
                        standalone_lane = large_mechanical or verified is not None
                        pending = jobs.get('pending', 0)
                        running = jobs.get('running', 0)
                        if ready and (standalone_lane or cohort or flush or ready >= limit or not pending
                                      or time.monotonic()-last_batch >= wait):
                            _runner(run, runner)
                            chosen_lane = lane if standalone_lane or (lane and not cohort) else None
                            recovered = cohort['jobs'] if cohort else []
                            also_mechanical = []
                            if cohort and lane and len(recovered) < limit:
                                existing = set(recovered)
                                unselected = [job for job in lane['mechanical'] if job not in existing]
                                also_mechanical = unselected[:limit-len(recovered)]
                            prefix = recovered+also_mechanical if cohort else (
                                chosen_lane['mechanical'] if chosen_lane and not standalone_lane else [])
                            unused = [] if standalone_lane else unused_author_jobs(
                                queue, progress['checkpoint'], min(unused_slots, limit-len(prefix)), prefix)
                            also_reviewed = (recovered_replay_fill(queue, progress['checkpoint'],
                                             prefix+unused, recovered_replay_slots, limit)
                                             if cohort else [])
                            protected = prefix+unused+also_reviewed
                            caller_room = 0 if standalone_lane else min(dependency_slots, limit-len(protected))
                            callers = dependency_caller_jobs(queue, progress['checkpoint'], protected, caller_room)
                            status('running', progress, lane=chosen_lane['lane'] if chosen_lane else
                                   'recovered-with-reviewed' if also_reviewed else
                                   'recovered-with-mechanical' if also_mechanical else
                                   'recovered' if cohort else 'fifo', dependency_callers=callers,
                                   unused_author_jobs=unused)
                            if chosen_lane:
                                selected = chosen_lane['jobs']
                                if not chosen_lane['combine']:
                                    selected = with_reviewed_replays(queue, progress['checkpoint'], replay_slots,
                                                                   limit, selected, preferred=protected,
                                                                   callers=callers)
                                batch_id = dispatch._batch(run, selected,
                                                combine_selected=chosen_lane['combine'] or fresh_union)
                            elif cohort:
                                batch_id = dispatch._batch(run, protected+callers,
                                                combine_selected=cohort['checkpoint'] == progress['checkpoint'] or union_trial)
                            elif fresh_union or replay_slots or callers or unused:
                                fifo = [row['id'] for row in queue.get_jobs('provisional-pass', limit=limit)]
                                selected = with_reviewed_replays(queue, progress['checkpoint'], replay_slots,
                                                               limit, fifo, preferred=unused, callers=callers)
                                batch_id = dispatch._batch(run, selected, combine_selected=fresh_union)
                            else:
                                batch_id = dispatch._batch(run)
                            observation = _batch_observation(run, batch_id, queue, progress['checkpoint'])
                            previous_lane = state['lane']
                            observation['lane'] = previous_lane
                            since_stale = 0 if previous_lane == 'stale-provisional' else since_stale+1
                            state['batches_since_stale'] = since_stale
                            new_progress = queue.progress()
                            if observation['outcome'] == 'passed':
                                if new_progress['checkpoint'] != observation['candidate']:
                                    raise ValueError('passed batch did not advance to its candidate checkpoint')
                                if not publication.is_file() or r.read(publication).get('status') not in ('integrated', 'pending'):
                                    raise ValueError('batch publication requires operator review')
                            elif new_progress['checkpoint'] != progress['checkpoint']:
                                raise ValueError('rejected batch unexpectedly advanced checkpoint')
                            last_batch = time.monotonic()
                            flush = False
                            status('idle', new_progress, last_batch=observation, reason=None)
                            continue
                        if not pending and not running and not ready:
                            if awaiting_print_recovery(run):
                                status('idle', progress, reason='automatic batch recovery is active')
                                continue
                            final = queue.progress()
                            if any(final['job_counts'].get(name) for name in ('pending', 'running', 'provisional-pass')):
                                continue
                            publication = run/'publication.json'
                            complete = (not final['counts'].get('failed', 0) and publication.exists()
                                        and r.read(publication).get('status') == 'integrated')
                            status('first-pass-complete' if complete else 'needs-operator', final)
                            return
                        status('idle', progress)
                    finally:
                        r._LEASES.remove(fd)
        except Exception as error:
            status('needs-operator', queue.progress(), reason=str(error))
            raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run', type=Path)
    parser.add_argument('--runner', type=Path, required=True)
    parser.add_argument('--flush-once', action='store_true')
    args = parser.parse_args()
    run_driver(args.run, args.runner, flush_once=args.flush_once)


if __name__ == '__main__':
    main()
