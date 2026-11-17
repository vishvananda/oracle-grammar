#!/usr/bin/env python3
"""Run the existing batch pipeline behind one durable campaign batch lease."""
import argparse
import os
from pathlib import Path
import sys
import time

sys.dont_write_bytecode = True

import retire as r
import retire_campaign as campaign
import retire_dispatch as dispatch
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
                'repository gates failed', 'explicit candidate grammar tests failed',
                'full allowed-population checks failed', 'optimized family report contains failures'):
            raise ValueError('batch validation failed operationally: '+str(validation_path))
        result.update(outcome='rejected', reason=validation['reason'])
    else:
        raise ValueError('batch staging failed operationally: '+str(manifest_path))
    rejected = {row['id']: row for row in queue.get_jobs('integration-rejected')}
    for job in jobs:
        row = rejected.get(job['job_id'])
        evidence = (row.get('adjudication') or {}).get('evidence', {}) if row else {}
        expected = ({'manifest': folder.name, 'validation': validation}
                    if job.get('status') == 'included' and stage == 'provisional'
                    else {'manifest': folder.name, 'reason': 'merge or staging rejected'})
        if not row or row['attempt_id'] != job['attempt_id'] or evidence != expected:
            raise ValueError('rejected batch job disposition is missing or differs')
    return result


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
                        counts = progress['counts']
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
                        ready = jobs.get('provisional-pass', 0)
                        pending = jobs.get('pending', 0)
                        running = jobs.get('running', 0)
                        if ready and (flush or ready >= limit or not pending or time.monotonic()-last_batch >= wait):
                            _runner(run, runner)
                            folders = run/'.cache/batches'
                            before = {p.name for p in folders.iterdir() if p.is_dir()} if folders.exists() else set()
                            status('running', progress)
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
                            publication = run/'publication.json'
                            complete = (not counts.get('failed', 0) and publication.exists()
                                        and r.read(publication).get('status') == 'integrated')
                            status('first-pass-complete' if complete else 'needs-operator', progress)
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
