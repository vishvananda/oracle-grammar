#!/usr/bin/env python3
"""Prepare and inspect an exhaustive, checkpointed retirement first pass."""
from argparse import Namespace
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

sys.dont_write_bytecode = True
import retire as r
import retire_plan as planner
from retire_queue import Queue

RUNNERS = (*r.TOOLING, 'retire_author.py', 'retire_author_pool.py', 'retire_pool.py',
           'retire_cpu.py', 'retire_cohort.py', 'retire_screen_packet.py', 'retire_plan.py', 'retire_queue.py', 'retire_cohort_proof.py', 'retire_rehabilitation.py', 'retire_timeout_recovery.py', 'retire_native_readmission.py', 'retire_native_exit_readmission.py', 'retire_native_outcome.py', 'retire_library_readmission.py', 'retire_library_controls.py', 'retire_repository_checks.py', 'retire_priority.py', 'retire_tasks.py',
           'retire_merge.py', 'retire_batch.py', 'retire_dispatch.py', 'retire_batch_driver.py', 'retire_campaign.py',
           'retire_metrics.py', 'retire_retention.py', 'retire_helper.py', 'retire_reuse.py')


def save_checkpoint(run, checkpoint):
    folder = run/'checkpoints'
    folder.mkdir(exist_ok=True)
    path = folder/(checkpoint['commit']+'.json')
    r.require(not path.exists(), 'checkpoint record already exists')
    r.write(path, checkpoint)


def checkpoint(run, commit):
    return r.read(run/'checkpoints'/(commit+'.json'))


def singleton_mechanical(jobs):
    output = []
    for job in jobs:
        if job['kind'] != 'mechanical' or len(job['rules']) == 1:
            output.append(job)
            continue
        for rule in job['rules']:
            copy = dict(job, id='mech-'+hashlib.sha256(rule.encode()).hexdigest()[:16], rules=[rule])
            copy['source_fragments'] = [f for f in job['source_fragments'] if f['name'] == rule]
            output.append(copy)
    return output


def prepare(args):
    run = args.run.resolve()
    r.require(not run.exists(), 'campaign exists; resume it instead of recreating inputs')
    run.mkdir(parents=True)
    source, helpers, baseline = (p.resolve() for p in (args.source, args.helpers, args.baseline))
    commit = subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip()
    inputs = run/'epochs'/commit/'inputs'
    inputs.parent.mkdir(parents=True)
    r.freeze(Namespace(run=inputs, source=source, helpers=helpers, gold=args.gold.resolve(),
                       oracle=args.oracle.resolve(), allowed=args.allowed.resolve(),
                       parse=baseline, prints=[baseline], slots=12, threads=2))
    r.scan(Namespace(run=inputs, rules=None))
    scan = r.read(inputs/'scan/rules.json')
    plan = planner.build_plan(source, scan, coarse=True)
    jobs = singleton_mechanical(plan['jobs'])
    # Prefer coherent high-yield, smaller declarations while preserving every job.
    jobs.sort(key=lambda j: (j['kind'] != 'mechanical', j.get('requires_diagnosis', False),
                            -len(j['rules']), sum(len(f['sem']) for f in j['source_fragments']), j['id']))
    r.write(run/'plan.json', dict(plan, jobs=jobs))
    runner = run/'runner'
    runner.mkdir()
    for name in RUNNERS:
        shutil.copy2(Path(__file__).parent/name, runner/name)
    r.write(run/'runner.json', r.tree(runner))
    config = {'format': 1, 'workers': args.workers, 'model': 'gpt-6.1-terra', 'effort': 'high',
              'max_diagnostic_attempts': 3, 'wall_budget_seconds': 900,
              'cpu_capacity': 32, 'batch_size': 48, 'first_batch_size': 12, 'batch_wait_seconds': 900,
              'audit_threads': getattr(args, 'audit_threads', 16),
              'initial_commit': commit, 'expected_rules': sorted(plan['manifest']['expected_rules']),
              'source_repo': str(source), 'main_repo': str(args.main.resolve()),
              'target_dir': str(args.target.resolve()), 'scope': 'complete first pass; failures remain for deeper review'}
    if getattr(args, 'usage_estimator', None) and getattr(args, 'pricing', None):
        config['metrics'] = {'usage_estimator': str(args.usage_estimator.resolve()),
                             'estimator_sha256': r.digest(args.usage_estimator),
                             'pricing': r.read(args.pricing)}
    config['max_operational_retries'] = 1
    r.write(run/'config.json', config)
    cp = {'commit': commit, 'source': str(source), 'inputs': str(inputs), 'helpers': str(helpers),
          'baseline': str(baseline), 'integration_workspace': str(run/'workspaces/coordinator'),
          'cpu_pool': str(run/'cpu'), 'cpu_capacity': 32}
    save_checkpoint(run, cp)
    queue = Queue(run/'queue.sqlite', {'max_workers': args.workers, 'expected_rules': config['expected_rules']})
    queue.add_checkpoint(commit, {'kind': 'accepted-input', 'manifest': r.digest(inputs/'manifest.json')})
    queue.add_jobs(jobs)
    coverage = queue.coverage()
    r.require(set(coverage['rules']) == set(config['expected_rules']), 'campaign inventory differs')
    r.write(run/'status.json', {'status': 'prepared', 'rules': coverage['total_rules'], 'jobs': len(jobs)})
    print(json.dumps({'campaign': str(run), 'jobs': len(jobs), 'rules': coverage['total_rules']}))


def status(run):
    coverage = Queue(run/'queue.sqlite').coverage()
    print(json.dumps({'status': r.read(run/'status.json'), 'rules': coverage['total_rules'],
                      'counts': coverage['counts'], 'checkpoint': coverage['checkpoint']}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    p = commands.add_parser('prepare')
    p.add_argument('run', type=Path)
    for name in ('source','helpers','baseline','gold','oracle','allowed','main','target'):
        p.add_argument('--'+name, type=Path, required=True)
    p.add_argument('--workers', type=int, default=12, choices=range(1,13))
    p.add_argument('--audit-threads', type=int, default=16, choices=(16,24))
    p.add_argument('--usage-estimator', type=Path)
    p.add_argument('--pricing', type=Path)
    for name in ('run','status'):
        p = commands.add_parser(name)
        p.add_argument('run', type=Path)
        if name == 'run':
            p.add_argument('--runner', type=Path)
    args = parser.parse_args()
    if args.command == 'prepare':
        prepare(args)
    elif args.command == 'status':
        status(args.run.resolve())
    else:
        import retire_dispatch
        retire_dispatch.run_campaign(args.run.resolve(), args.runner)


if __name__ == '__main__':
    main()
