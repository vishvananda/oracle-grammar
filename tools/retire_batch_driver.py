#!/usr/bin/env python3
"""Run the existing batch pipeline behind one durable campaign batch lease."""
import argparse
from contextlib import closing
import os
from pathlib import Path
import sys
import time

sys.dont_write_bytecode = True

import retire as r
import retire_campaign as campaign
import retire_dispatch as dispatch
from retire_library_readmission import LIBRARY_REJECTION
from retire_parse_readmission import PARSE_SCREEN_REJECTION, PRINT_ONLY_REJECTION, awaiting_print_recovery
from retire_queue import Queue
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


def _batch_observation(run, before, queue, checkpoint):
    folders = run/'.cache/batches'
    after = {p.name for p in folders.iterdir() if p.is_dir()} if folders.exists() else set()
    created = after-before
    if len(created) != 1:
        raise ValueError('batch did not produce exactly one new manifest')
    folder = folders/created.pop()
    manifest_path = folder/'manifest.json'
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
    kinds = ('readmitted-batch', 'readmitted-native-batch',
             'readmitted-library-batch', 'readmitted-native-exit-one-batch', 'readmitted-parse-screen-batch')
    with closing(queue._connect()) as db:
        db.execute('BEGIN')
        latest = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
        if not latest or latest[0] != checkpoint:
            raise ValueError('checkpoint changed during recovered cohort selection')
        rows = db.execute("""SELECT e.seq, j.id, j.attempt_id,
                json_extract(e.data,'$.attempt_id'), json_extract(e.data,'$.manifest'),
                json_extract(e.data,'$.checkpoint'), json_extract(e.data,'$.job_id')
            FROM events e JOIN jobs j ON j.id=json_extract(e.data,'$.job_id')
            WHERE j.status='provisional-pass' AND e.kind IN (?,?,?,?,?)
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
        return {'lane': 'mechanical-first', 'jobs': (mechanical+authors)[:limit], 'combine': False}
    return None


def run_driver(run, runner, *, flush_once=False, poll_seconds=2):
    run = Path(run).resolve()
    runner = _runner(run, runner)
    started = time.time()
    queue = Queue(run/'queue.sqlite')
    state = {'pid': os.getpid(), 'runner': str(runner), 'started_unix': started}
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
            with dispatch.lease(run/'batch.lock', blocking=True) as fd:
                r._LEASES.append(fd)
                try:
                    last_batch = time.monotonic()
                    flush = flush_once
                    while True:
                        progress = queue.progress()
                        jobs = progress['job_counts']
                        if (run/'drain.json').exists() or (run/'batch-driver-drain.json').exists():
                            status('drained', progress, reason='explicit drain requested')
                            return
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
                        ready = jobs.get('provisional-pass', 0)
                        lane = mechanical_lane(queue, limit, mechanical_minimum) if mechanical_priority and ready else None
                        large_mechanical = lane is not None and lane['combine']
                        cohort = recovered_cohort(queue, progress['checkpoint']) if routing and ready and not large_mechanical else []
                        pending = jobs.get('pending', 0)
                        running = jobs.get('running', 0)
                        if ready and (large_mechanical or cohort or flush or ready >= limit or not pending
                                      or time.monotonic()-last_batch >= wait):
                            _runner(run, runner)
                            folders = run/'.cache/batches'
                            before = {p.name for p in folders.iterdir() if p.is_dir()} if folders.exists() else set()
                            chosen_lane = lane if large_mechanical or (lane and not cohort) else None
                            status('running', progress, lane=chosen_lane['lane'] if chosen_lane else
                                   'recovered' if cohort else 'fifo')
                            if chosen_lane:
                                dispatch._batch(run, chosen_lane['jobs'],
                                                combine_selected=chosen_lane['combine'] or fresh_union)
                            elif cohort:
                                dispatch._batch(run, cohort['jobs'],
                                                combine_selected=cohort['checkpoint'] == progress['checkpoint'] or union_trial)
                            elif fresh_union:
                                selected = queue.get_jobs('provisional-pass', limit=limit)
                                dispatch._batch(run, [row['id'] for row in selected], combine_selected=True)
                            else:
                                dispatch._batch(run)
                            observation = _batch_observation(run, before, queue, progress['checkpoint'])
                            new_progress = queue.progress()
                            if observation['outcome'] == 'passed':
                                if new_progress['checkpoint'] != observation['candidate']:
                                    raise ValueError('passed batch did not advance to its candidate checkpoint')
                                publication = run/'publication.json'
                                if not publication.exists() or r.read(publication).get('status') != 'integrated':
                                    raise ValueError('batch publication requires operator review')
                            elif new_progress['checkpoint'] != progress['checkpoint']:
                                raise ValueError('rejected batch unexpectedly advanced checkpoint')
                            last_batch = time.monotonic()
                            flush = False
                            status('idle', new_progress, last_batch=observation)
                            continue
                        if not pending and not running and not ready:
                            if awaiting_print_recovery(run):
                                status('idle', progress, reason='automatic batch recovery is active')
                                time.sleep(poll_seconds)
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
                        time.sleep(poll_seconds)
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
