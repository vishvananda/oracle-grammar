#!/usr/bin/env python3
"""Rolling Terra workers, deterministic removals, and one retained batch lane."""
from concurrent.futures import FIRST_COMPLETED, ThreadPoolExecutor, wait
from collections import OrderedDict
from contextlib import contextmanager
import argparse
import copy
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading
import time
import uuid

sys.dont_write_bytecode = True
import dev
import mined
import retire as r
import retire_author as author
import retire_author_pool as models
import retire_batch
import retire_campaign as campaign_api
import retire_cpu
import retire_plan as planner
import retire_priority
import retire_metrics
import retire_retention
from retire_queue import Queue, RehabilitationIneligible
from retire_tasks import (CohortClosureLimit, close_cohort, mechanical_chunks,
                          prepare_task, validate_runner)
import retire_worker as w


@contextmanager
def lease(path, *, blocking=False):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('a') as stream:
        fcntl.flock(stream, fcntl.LOCK_EX | (0 if blocking else fcntl.LOCK_NB))
        yield stream.fileno()


def batch_available(run):
    """Probe the existing batch lease; the child must still acquire it itself."""
    try:
        fd = os.open(run/'batch.lock', os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW)
    except FileNotFoundError:
        return True
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return False
        fcntl.flock(fd, fcntl.LOCK_UN)
        return True
    finally:
        os.close(fd)


def decisions(path, rules):
    value = r.read(path)
    rows = value['rules']
    r.require(len(rows) == len(rules) and {row['rule'] for row in rows} == set(rules),
              'model did not individually review every assigned rule')
    for row in rows:
        r.require(row['status'] in ('proposed','needs-review') and len(row['reason'].strip()) >= 16
                  and row['evidence'].strip(), 'model disposition lacks concrete evidence')
    return {row['rule']: row for row in rows}


def mechanical(run, task_id):
    """No model and no shared-source edit; the batch still needs whole-population proof."""
    task, root, manifest = author.verify(run, task_id)
    directory = run/'evaluations'/task_id/('attempt-'+uuid.uuid4().hex)
    directory.mkdir(parents=True)
    inputs = run/'inputs'
    report = {'task':task_id,'source':manifest['commit'],'directory':str(directory),
              'passed':False,'failures':[],'hooks':[],'edited':[], 'kind':'mechanical',
              'descriptor_sha256':r.digest(run/'tasks'/f'{task_id}.json'),
              'support_sha256':r.digest(run/'support.json')}
    with retire_cpu.reserve(Path(task['cpu_pool']),2,task.get('cpu_capacity',32)) as fds:
        r._LEASES.extend(fds)
        inherited = os.environ.get('RETIRE_JOB_FD')
        if inherited:
            r._LEASES.append(int(inherited))
        try:
            shutil.copytree(inputs/'frozen/grammar',directory/'grammar')
            unused = set(r.read(inputs/'scan/rules.json')['unused'])
            r.require(set(task['rules']) <= unused, 'mechanical candidate is no longer audited unused')
            removed, kept = mined.remove(directory,task['rules'])
            r.require(set(removed)==set(task['rules']) and not kept,'removal has retained users')
            names = set()
            data = w.payload(inputs)
            chunks = task.get('mechanical_chunks')
            if chunks is not None:
                report.update(chunks_expected=len(chunks) if isinstance(chunks, list) else 0,
                              chunks_started=0, chunks_completed=0, coverage_complete=False, chunk_results=[])
            for rule in task['rules']:
                cohort = w.cohort(inputs,rule,data[0])
                r.require(cohort and (chunks is not None or len(cohort)<=500),
                          'mechanical cohort uncertain or too large')
                names.update(cohort)
            if chunks is None:
                r.require(len(names)<=500,'combined mechanical cohort too large')
                r.require(names <= set(task['cohort']), 'mechanical targets outside prepared cohort')
                report.update(w.checks(inputs,directory,directory/'grammar',set(task['cohort']),data,2))
                report['passed'] = not report['failures']
            else:
                expected = mechanical_chunks(inputs, sorted(names))
                r.require(len(expected) > 1 and chunks == expected, 'mechanical chunk plan differs')
                r.require(task['target_faces'] == chunks[0]['base_names'] and task['cohort'] == chunks[0]['names'],
                          'mechanical first chunk differs from task packet')
                report['faces'] = 0
                for index, chunk in enumerate(chunks):
                    folder = directory/f'chunk-{index:03d}'
                    folder.mkdir()
                    report['chunks_started'] += 1
                    result = w.checks(inputs,folder,directory/'grammar',chunk['names'],data,2)
                    r.require(result['faces'] == chunk['occurrences'], 'mechanical chunk population differs')
                    report['chunk_results'].append({'index': index, 'directory': str(folder), **result})
                    report['chunks_completed'] += 1
                    report['faces'] += result['faces']
                    report['failures'].extend(result['failures'])
                report['coverage_complete'] = report['chunks_completed'] == report['chunks_expected']
                report['passed'] = report['coverage_complete'] and not report['failures']
        except Exception as error:
            report['failures'].append(str(error))
        finally:
            if 'chunks_expected' in report:
                report['chunks_unrun'] = report['chunks_expected'] - report['chunks_started']
                report['chunks_incomplete'] = report['chunks_expected'] - report['chunks_completed']
            if inherited:
                r._LEASES.remove(int(inherited))
            for fd in fds:
                r._LEASES.remove(fd)
    r.verify(inputs)
    report['grammar'] = r.tree(directory/'grammar')
    r.write(directory/'hooks.json',[])
    r.write(directory/'report.json',report)
    print(json.dumps(report),flush=True)


class Contexts:
    def __init__(self):
        self.lock = threading.Lock()
        self.values = OrderedDict()

    def get(self, checkpoint):
        with self.lock:
            commit = checkpoint['commit']
            if commit not in self.values:
                inputs = Path(checkpoint['inputs'])
                data = w.payload(inputs)
                scan = r.read(inputs/'scan/rules.json')
                inverted = planner.prepare_cohort_index(data[0],data[1])
                self.values[commit] = (data,scan,inverted)
                while len(self.values)>2:
                    self.values.popitem(last=False)
            self.values.move_to_end(commit)
            return self.values[commit]


def attempt_usage(run, job, working, receipt):
    """Collect this attempt's paid usage, or preserve why its cost is unknown."""
    if job['kind'] != 'author':
        return None
    if working is None or not isinstance(receipt, dict) or not receipt.get('finished_unix'):
        return {'status': 'unavailable', 'error': 'terminal model receipt unavailable', 'estimated_usd': None}
    try:
        config = r.read(run/'config.json')
        metrics = config.get('metrics')
        r.require(metrics is not None, 'usage estimator is not configured')
        r.require(r.digest(Path(metrics['usage_estimator'])) == metrics['estimator_sha256'],
                  'usage estimator changed')
        return retire_metrics.collect(working, job['id'], receipt, metrics)
    except Exception as error:
        return {'status': 'unavailable', 'error': str(error), 'estimated_usd': None}


def mechanical_detail(inputs, names):
    """Keep ordinary tasks unchanged; partition only oversized diagnostics."""
    chunked = len(names) > 500
    if not chunked:
        try:
            close_cohort(inputs, {'names': names})
        except CohortClosureLimit:
            chunked = True
    if chunked:
        chunks = mechanical_chunks(inputs, names)
        detail = {'names': chunks[0]['base_names'], 'sampled': False,
                  'scope': 'first of complete bounded mechanical chunks',
                  'full_batch_audit_required': True}
        return detail, chunks[0]['base_names'], chunks
    return ({'names': names, 'sampled': False, 'scope': 'audited-unused literal triage',
             'full_batch_audit_required': True}, names, None)


def perform(run, claim, contexts, runner=None):
    queue = Queue(run/'queue.sqlite')
    cp = campaign_api.checkpoint(run,claim['base_commit'])
    job = dict(claim['job'], model='gpt-6.1-terra',effort='high',full_pass=True)
    attempt = claim['attempt_id']
    evidence = run/'.cache/receipts'/(attempt+'.json')
    evidence.parent.mkdir(parents=True,exist_ok=True)
    working=None
    receipt=None
    usage=None
    usage_collected=False
    with lease(run/'leases'/job['id']) as fd:
        try:
            data,scan,inverted = contexts.get(cp)
            if job['kind']=='mechanical':
                names = sorted({name for rule in job['rules'] for name in w.cohort(Path(cp['inputs']),rule,data[0])})
                detail, job['target_faces'], chunks = mechanical_detail(Path(cp['inputs']), names)
                if chunks is not None:
                    job['mechanical_chunks'] = chunks
            else:
                detail = planner.select_cohort_detail(job,inverted,data[1],scan)
            reused=run/'reuse'/(job['id']+'.json')
            if reused.exists():
                prior=r.read(reused)
                if prior.get('review_notes'):
                    job['goal']+=' Coordinator review: '+' '.join(prior['review_notes'])
                if prior.get('base_commit')==cp['commit']:
                    claim=dict(claim,reuse_archive=prior)
            prepared = prepare_task(run,claim,cp,job,detail,runner=runner)
            working = Path(prepared['run'])
            task_id = prepared['task_id']
            if job['kind']=='mechanical':
                if runner is not None: validate_runner(run,runner)
                env = dict(os.environ,RETIRE_JOB_FD=str(fd))
                command=[sys.executable,str(Path(runner or run/'runner')/'retire_dispatch.py'),'mechanical',str(working),task_id]
                with (working/'mechanical.log').open('w') as log:
                    process=subprocess.Popen(command,env=env,stdout=log,stderr=log,pass_fds=(fd,))
                    queue.mark_started(attempt,process.pid,str(working/'mechanical.log'))
                    code=process.wait()
                paths=list((working/'evaluations'/task_id).glob('*/report.json'))
                r.require(code==0 and len(paths)==1,'mechanical execution did not publish a report')
                report=r.read(paths[0])
                reviewed={rule:{'status':'proposed' if report['passed'] else 'needs-review',
                    'reason':'Audited unused removal checked against native readings and print gates'
                        if report['passed'] else str(report['failures']),
                    'evidence':str(paths[0])} for rule in job['rules']}
            else:
                receipt=models.launch(working,task_id,
                    on_started=lambda pid,path: queue.mark_started(attempt,pid,path),extra_fds=(fd,),
                    startup_lock=run/'startup.lock')
                reviewed=decisions(working/'workers'/task_id/'final.txt',job['rules'])
                paths=list((working/'evaluations'/task_id).glob('*/report.json'))
                report=r.read(paths[0]) if len(paths)==1 else {
                    'passed':False,'failures':[receipt.get('evaluation',{}).get('error','evaluation absent')]}
            passed=bool(report.get('passed')) and all(x['status']=='proposed' for x in reviewed.values())
            outcome={}
            for rule,row in reviewed.items():
                ok=passed and row['status']=='proposed'
                outcome[rule]={'status':'pass' if ok else 'needs-review',
                    'reason':row['reason'] if report.get('passed') else str(report.get('failures')),
                    'evidence':{'model':row['evidence'],'report':str(paths[0]) if paths else None}}
            result={'run':str(working),'task':task_id,'report':str(paths[0]) if paths else None,
                    'kind':job['kind'],'dispositions':reviewed,'base_commit':claim['base_commit']}
            usage=attempt_usage(run,job,working,receipt)
            usage_collected=True
            if usage is not None:
                result['usage']=usage
            r.write(evidence,result)
            queue.finish(attempt,'provisional-pass' if passed else 'needs-review',
                         {'receipt':str(evidence),**result},outcome)
        except Exception as error:
            result={'attempt':attempt,'job':job['id'],'kind':job['kind'],
                    'status':'operational-failure','error':str(error),
                    'run':str(working) if working else None,'task':job['id'],'base_commit':claim['base_commit']}
            if not usage_collected:
                usage=attempt_usage(run,job,working,receipt)
            if usage is not None:
                result['usage']=usage
            r.write(evidence,result)
            status='needs-review' if isinstance(receipt, dict) and receipt.get('finished_unix') else 'failed'
            queue.finish(attempt,status,{'receipt':str(evidence),**result},
                {rule:{'status':'needs-review','reason':'Operational failure before complete review: '+str(error),
                       'evidence':str(evidence)} for rule in job['rules']})
        if working:
            try:
                archive=retire_retention.archive_checkout(working,job['id'],evidence)
                r.write(working/'archive.json',archive)
            except Exception as error:
                r.write(working/'archive.json',{'status':'retained','reason':str(error)})
    return job['id']


def reclassify(run, cp):
    queue=Queue(run/'queue.sqlite')
    scan=r.read(Path(cp['inputs'])/'scan/rules.json')
    unused=set(scan['unused'])
    for row in queue.get_jobs('pending'):
        job=row['job']
        fresh=set(job['rules']) & unused
        if not fresh or job['kind']=='mechanical':
            continue
        groups=[]
        remaining=[rule for rule in job['rules'] if rule not in fresh]
        if remaining:
            groups.append(dict(job,id=job['id']+'-rest-'+cp['commit'][:8],rules=remaining,
                target_faces=sorted({name for rule in remaining for name in scan['needed'].get(rule,[])}),
                source_fragments=[f for f in job['source_fragments'] if f['name'] in remaining]))
        for rule in sorted(fresh):
            groups.append(dict(job,id='mech-'+hashlib.sha256((rule+cp['commit']).encode()).hexdigest()[:16],
                               rules=[rule],kind='mechanical',target_faces=[],
                               source_fragments=[f for f in job['source_fragments'] if f['name']==rule]))
        try:
            queue.split_pending(job['id'],groups,{'checkpoint':cp['commit'],'reason':'newly audited-unused'})
        except ValueError:
            latest=next(row for row in queue.get_jobs() if row['id']==job['id'])
            if latest['status']=='pending':
                raise
    proof=None
    for status in ('needs-review','integration-rejected','failed'):
        for row in queue.get_jobs(status):
            job=row['job']
            if not set(job['rules']) & unused or job.get('review_checkpoint')==cp['commit']:
                continue
            if proof is None:
                inputs=Path(cp['inputs'])
                paths={'scan':inputs/'scan/rules.json', 'scan_complete':inputs/'scan-complete.json',
                       'manifest':inputs/'manifest.json', 'commands':inputs/'scan/commands.jsonl'}
                proof={'kind':'checkpoint-unused-rehabilitation',
                       **{name:{'path':str(path.resolve()),'sha256':r.digest(path)} for name,path in paths.items()}}
            try:
                queue.rehabilitate_unused(row['id'],cp['commit'],proof)
            except RehabilitationIneligible:
                continue


def proposals(run, count, selected_job_ids=None):
    queue=Queue(run/'queue.sqlite')
    if selected_job_ids is None:
        rows=queue.get_jobs('provisional-pass',limit=count)
    else:
        if (not isinstance(selected_job_ids,list) or not 1 <= len(selected_job_ids) <= 96
                or any(not isinstance(job_id,str) or not job_id for job_id in selected_job_ids)
                or len(set(selected_job_ids)) != len(selected_job_ids)):
            raise ValueError('selected jobs must be 1–96 distinct nonempty IDs')
        available={row['id']:row for row in queue.get_jobs('provisional-pass')}
        if any(job_id not in available for job_id in selected_job_ids):
            raise ValueError('every selected job must be provisional')
        rows=[available[job_id] for job_id in selected_job_ids]
    output=[]
    for row in rows:
        attempt=queue.get_attempt(row['attempt_id'])
        evidence=attempt['result']['evidence']
        report_path=evidence.get('report')
        if not report_path:
            if selected_job_ids is not None:
                raise ValueError('selected provisional job lacks a report')
            continue
        output.append({'job_id':row['id'],'attempt_id':row['attempt_id'],
                       'run':evidence['run'],'report':r.read(Path(report_path)),
                       'kind':evidence['kind']})
    return output


def priority_policy(config):
    policy = config.get('priority_policy')
    if policy is not None and policy != retire_priority.VERSION:
        raise ValueError('unsupported priority policy')
    return policy


def refresh_priorities(run, checkpoint, policy):
    """Publish one checkpoint-bound scheduling plan when explicitly enabled."""
    if policy is None:
        return None
    if policy != retire_priority.VERSION:
        raise ValueError('unsupported priority policy')
    plan = retire_priority.plan(run)
    if plan.get('checkpoint') != checkpoint:
        raise ValueError('priority plan checkpoint changed')
    return Queue(run/'queue.sqlite').publish_priorities(checkpoint, plan)


def batch(run, selected_job_ids=None, *, wait=False, combine_selected=False):
    if type(combine_selected) is not bool or (combine_selected and not selected_job_ids):
        raise ValueError('combine_selected requires an explicit nonempty selection')
    try:
        with lease(run/'batch.lock', blocking=wait) as fd:
            r._LEASES.append(fd)
            try:
                if combine_selected:
                    return _batch(run, selected_job_ids, combine_selected=True)
                return _batch(run) if selected_job_ids is None else _batch(run, selected_job_ids)
            finally:
                r._LEASES.remove(fd)
    except BlockingIOError:
        return


def _batch(run, selected_job_ids=None, *, combine_selected=False):
    if type(combine_selected) is not bool or (combine_selected and not selected_job_ids):
        raise ValueError('combine_selected requires an explicit nonempty selection')
    queue=Queue(run/'queue.sqlite')
    cp=campaign_api.checkpoint(run,queue.coverage()['checkpoint'])
    config=r.read(run/'config.json')
    policy=priority_policy(config)
    cp=dict(cp,audit_threads=config.get('audit_threads',24))
    screen=config.get('full_parse_screen',False)
    r.require(type(screen) is bool,'full_parse_screen must be boolean')
    cp['full_parse_screen']=screen
    cp['stage_group_size']=config.get('stage_group_size',1)
    cp['native_control_threads']=config.get('native_control_threads',retire_batch.DEFAULT_NATIVE_CONTROL_THREADS)
    retire_batch._native_control_threads(cp)
    cp.pop('native_regression_controls',None)
    if config.get('native_regression_controls') is not None:
        cp['native_regression_controls']=config['native_regression_controls']
    cp.pop('library_regression_controls',None)
    if config.get('library_regression_controls') is not None:
        cp['library_regression_controls']=config['library_regression_controls']
    cp.pop('candidate_validation_helpers',None)
    if config.get('candidate_validation_helpers') is not None:
        cp['candidate_validation_helpers']=config['candidate_validation_helpers']
        retire_batch._candidate_helper_preflight(cp['candidate_validation_helpers'],cp,Path(cp['source']),run)
    limit=config['first_batch_size'] if cp['commit']==config['initial_commit'] else config['batch_size']
    items=proposals(run,limit,selected_job_ids)
    if not items:
        return
    manifest=(retire_batch.stage(run,cp,items,combine_selected=True) if combine_selected
              else retire_batch.stage(run,cp,items))
    included=[row['job_id'] for row in manifest['jobs'] if row['status']=='included']
    rejected=[row['job_id'] for row in manifest['jobs'] if row['status']!='included']
    if rejected:
        queue.reject_jobs(rejected,{'manifest':manifest['id'],'reason':'merge or staging rejected'})
    if manifest['status']!='provisional':
        if included:
            queue.reject_jobs(included,{'manifest':manifest['id'],'reason':manifest.get('reason','staging failed')})
        return
    result=retire_batch.validate(manifest,cp,slots=12)
    if result['status']!='passed':
        queue.reject_jobs(included,{'manifest':manifest['id'],'validation':result})
        return
    next_helpers=cp['helpers']
    override=cp.get('candidate_validation_helpers')
    if override is not None:
        snapshot=Path(result.get('helpers','')).resolve()
        expected=(Path(manifest['folder'])/'candidate-validation-helpers').resolve()
        r.require(snapshot==expected,'validated helper snapshot path differs')
        build=retire_batch._verified_helper_build(snapshot,override['build_sha256'])
        frozen=Path(result['inputs'])/'frozen'
        r.require(r.digest(frozen/'build.json')==override['build_sha256']
                  and all(r.digest(frozen/'bin'/name)==build['binaries'][name] for name in r.HELPERS),
                  'validated frozen helper identity differs')
        next_helpers=str(snapshot)
    commit=manifest['commit']
    source=run/'checkpoints'/commit/'source'
    source.parent.mkdir(parents=True)
    subprocess.run(['git','worktree','add','--detach',str(source),commit],cwd=cp['source'],check=True)
    inputs=Path(result['inputs'])
    scan_checkpoint(inputs,cp)
    next_cp=dict(cp,commit=commit,source=str(source),inputs=str(inputs),baseline=result['rows'],helpers=next_helpers)
    next_cp.pop('candidate_validation_helpers',None)
    campaign_api.save_checkpoint(run,next_cp)
    queue.advance_and_accept(commit,cp['commit'],included,{'manifest':manifest['id'],'validation':result})
    reclassify(run,next_cp)
    refresh_priorities(run,commit,policy)
    r.write(run/'latest-checkpoint.json',next_cp)
    publish_main(run, cp, next_cp, manifest)


def scan_checkpoint(inputs, cp):
    with retire_cpu.reserve(Path(cp['cpu_pool']),cp.get('audit_threads',24),cp.get('cpu_capacity',32)) as fds:
        r._LEASES.extend(fds)
        try:
            r.scan(argparse.Namespace(run=inputs,rules=None))
        finally:
            for fd in fds:
                r._LEASES.remove(fd)


def publish_main(run, previous, current, manifest):
    """Publish an audited batch only while main still matches its semantic parent."""
    root=Path(r.read(run/'config.json')['main_repo'])
    proof=Path(manifest['folder'])/'main-integration.json'
    record={'candidate':current['commit'],'status':'pending'}
    def git(*args):
        return subprocess.check_output(['git',*args],cwd=root,text=True).strip()
    try:
        r.require(not git('status','--porcelain'),'main has uncommitted work')
        r.require(dev.source(root)==dev.source(Path(previous['source'])),'main compiled source differs')
        for path in ('grammar','families'):
            r.require(r.tree(root/path)==r.tree(Path(previous['source'])/path),
                      'main semantic source differs: '+path)
        r.require(r.digest(root/'tests/families.json')==r.digest(Path(previous['source'])/'tests/families.json'),
                  'main family registry differs')
        changed=git('diff-tree','--no-commit-id','--name-only','-r',current['commit']).splitlines()
        r.require(changed and all(p.startswith(('grammar/','families/')) or p=='tests/families.json'
                                 for p in changed),'batch changed unapproved publication paths')
        subprocess.run(['git','cherry-pick',current['commit']],cwd=root,check=True,
                       stdout=subprocess.DEVNULL)
        record.update(status='integrated',commit=git('rev-parse','HEAD'))
    except (ValueError,OSError,subprocess.CalledProcessError) as error:
        record['reason']=str(error)
    r.write(proof,record)
    r.write(run/'publication.json',record)


def run_campaign(run, runner=None):
    config=r.read(run/'config.json')
    priority_policy(config)
    external_driver = config.get('external_batch_driver', False)
    r.require(type(external_driver) is bool, 'external_batch_driver must be boolean')
    selected=validate_runner(run,runner)
    if runner is not None:
        # The controller and its imported orchestration modules must all match
        # the selected snapshot, not just the child scripts it later launches.
        r.require(r.digest(selected/'retire_dispatch.py')==r.digest(Path(__file__)),
                  'controller must execute from the selected runner version')
        for name in campaign_api.RUNNERS:
            module = sys.modules.get(Path(name).stem)
            source = getattr(module, '__file__', None)
            if source:
                r.require((selected/name).is_file() and r.digest(selected/name)==r.digest(Path(source)),
                          'controller imports differ from the selected runner version: '+name)
    queue=Queue(run/'queue.sqlite')
    contexts=Contexts()
    with lease(run/'controller.lock'):
        # Unknown old children are never silently reassigned.
        r.require(not queue.get_jobs('running'),'running attempts need verified recovery before controller restart')
        r.write(run/'controller.json',{'pid':os.getpid(),'started_unix':time.time()})
        futures={}
        batching=None
        retried=set()
        stop_reason=None
        last_batch=time.monotonic()
        with ThreadPoolExecutor(max_workers=config['workers']) as executor:
            while True:
                if batching and batching.poll() is not None:
                    r.require(batching.returncode==0,'batch process failed; inspect batch-controller.log')
                    batching=None
                    last_batch=time.monotonic()
                for row in queue.get_jobs('failed'):
                    if row['attempt_id'] in retried:
                        continue
                    retried.add(row['attempt_id'])
                    try:
                        queue.retry_failed(row['id'],row['attempt_id'],
                            {'reason':'terminal operational failure; kernel ownership rechecked'},
                            config['max_operational_retries'])
                    except ValueError:
                        pass
                if len(queue.get_jobs('failed'))>=3:
                    stop_reason='Repeated operational failures require tooling review'
                drain=run/'drain.json'
                if drain.exists():
                    stop_reason=r.read(drain).get('reason','Explicit controller drain requested')
                if shutil.disk_usage(run).free < 15*1024**3:
                    stop_reason='Campaign filesystem has less than 15 GiB free'
                if shutil.disk_usage(Path(config['main_repo'])).free < 2*1024**3:
                    stop_reason='Main filesystem has less than 2 GiB free'
                free=[f'terra-{n:02}' for n in range(config['workers']) if f'terra-{n:02}' not in futures.values()]
                for worker in ([] if stop_reason else free):
                    claim=queue.claim(worker,kinds=['mechanical']) or queue.claim(worker,kinds=['author'])
                    if claim:
                        futures[executor.submit(perform,run,claim,contexts,selected)]=worker
                if futures:
                    finished,_=wait(futures,timeout=2,return_when=FIRST_COMPLETED)
                    for future in finished:
                        future.result()
                        del futures[future]
                coverage=queue.progress()
                pending=coverage['counts'].get('pending',0)
                limit=config['first_batch_size'] if coverage['checkpoint']==config['initial_commit'] else config['batch_size']
                ready=queue.get_jobs('provisional-pass',limit=limit)
                if not external_driver and not stop_reason and not batching and ready and (len(ready)>=limit or not pending
                        or time.monotonic()-last_batch>=config['batch_wait_seconds']) and batch_available(run):
                    validate_runner(run,selected)
                    log=(run/'batch-controller.log').open('a')
                    batching=subprocess.Popen([sys.executable,str(selected/'retire_dispatch.py'),'batch',str(run)],
                                              stdout=log,stderr=log)
                    log.close()
                r.write(run/'status.json',{'status':'running','pid':os.getpid(),
                    'rules':coverage['total_rules'],'counts':coverage['counts'],
                    'checkpoint':coverage['checkpoint'],'batch_pid':batching.pid if batching else None,
                    'stop_reason':stop_reason,
                    'updated_unix':time.time()})
                if stop_reason and not futures and not batching:
                    r.write(run/'status.json',{'status':'needs-operator','reason':stop_reason,
                        'rules':coverage['total_rules'],'counts':coverage['counts'],
                        'checkpoint':coverage['checkpoint'],'updated_unix':time.time()})
                    return
                if not pending and not futures and not batching and not ready:
                    complete=not coverage['counts'].get('failed',0)
                    publication=run/'publication.json'
                    complete=complete and (not publication.exists() or r.read(publication)['status']=='integrated')
                    r.write(run/'status.json',{'status':'first-pass-complete' if complete else 'needs-operator',
                            'rules':coverage['total_rules'],'counts':coverage['counts'],
                            'checkpoint':coverage['checkpoint'],'updated_unix':time.time()})
                    return
                if not futures:
                    time.sleep(2)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command',choices=['run','mechanical','batch'])
    parser.add_argument('run',type=Path)
    parser.add_argument('task',nargs='?')
    parser.add_argument('--runner',type=Path)
    parser.add_argument('--jobs-file',type=Path)
    parser.add_argument('--wait',action='store_true')
    parser.add_argument('--combine-selected',action='store_true')
    args=parser.parse_args()
    run=args.run.resolve()
    if args.command!='run' and args.runner is not None: parser.error('--runner is only valid for run')
    if args.combine_selected and (args.command != 'batch' or args.jobs_file is None):
        parser.error('--combine-selected requires batch and --jobs-file')
    if args.command!='batch' and (args.jobs_file is not None or args.wait):
        parser.error('--jobs-file and --wait are only valid for batch')
    if args.command=='mechanical':mechanical(run,args.task)
    elif args.command=='batch':
        selected = r.read(args.jobs_file) if args.jobs_file else None
        if args.combine_selected and (not isinstance(selected, list) or not selected):
            parser.error('--combine-selected requires a nonempty jobs file')
        batch(run, selected, wait=args.wait, combine_selected=args.combine_selected)
    else:run_campaign(run,args.runner)


if __name__=='__main__':
    main()
