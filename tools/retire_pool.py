#!/usr/bin/env python3
"""Run a durable model pool over one frozen mechanical retirement epoch."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

sys.dont_write_bytecode = True
import retire as r


def inventory(run):
    scan = r.read(run/'scan/rules.json')
    return [{'rule': rule, 'lane': 'mechanical', 'faces': []}
            for rule in scan['unused']] + [
        {'rule': rule, 'lane': 'shared-grammar', 'faces': faces}
        for rule, faces in sorted(scan['needed'].items())]


def prior_findings(prior, run):
    """Reuse diagnostics only when all immutable inputs match; never import a pass."""
    if prior is None:
        return []
    before, current = r.verify(prior), r.verify(run)
    r.require(before['files'] == current['files'], 'prior findings have different inputs')
    findings = []
    for job in sorted((prior/'jobs').iterdir()):
        path = job/'result.json'
        if path.exists():
            result = r.read(path)
            findings.append({'rule': result['rule'], 'status': result['status'],
                             'reason': result.get('reason'), 'failures': result.get('failures', []),
                             'evidence': str(path), 'sha256': r.digest(path)})
    return findings


def prompt(run, rules, name, knowledge):
    command = [sys.executable, str(run/'frozen/runner/retire.py'), 'worker', str(run),
               '--name', name, '--rules', *rules, '--limit', str(len(rules))]
    return f'''You are {name}, a mechanical grammar-retirement worker.
Use gpt-6-luna at max effort. Your fixed assignments are {json.dumps(rules)}.
Read {knowledge} for earlier diagnostics. They are findings, not acceptance.
Run this exact command directly with an exec tool (JSON array shown):
{json.dumps(command)}
Keep the returned process handle and wait on that same process until it exits.
Do not restart an ongoing command on observation timeout. The runner records timing.
Then inspect the job reports with:
{json.dumps([sys.executable, str(run/'frozen/runner/retire.py'), 'report', str(run), '--rules', *rules])}
Use artifact paths from the reports; do not guess directory names.
Check every assigned rule is terminal and distinguish provisional-pass, needs-review,
and failed. A zero helper exit does not establish semantic or printing acceptance.
Summarize each failed gate and evidence path. Reuse earlier explanations where relevant.
Do not edit source, grammar, frozen files, results, fixtures or knowledge. Do not run
Cargo, audits, Git commits, network tools, or additional agents. Only the deterministic
worker/report commands may write the shared epoch. Your own directory is for notes.
Do not repair semantics or weaken gates. Whole input, all native readings and all four
print gates must be checked for regressions against the baseline. A pre-existing
false print gate is not a new failure: preserve every baseline success, rather than
requiring all inputs to have exact default printing already. Use the report's lost
gate list instead of interpreting optional row fields yourself. A short-circuited
large/uncertain cohort has no score/native files; their absence is expected.
Coordinator handles combined validation and escalations.
Return a concise final report; no approval request is needed.
'''


def command(run, folder, model, effort):
    return [shutil.which('codex') or 'codex', 'exec', '--ignore-user-config',
            '--skip-git-repo-check', '--sandbox', 'workspace-write', '--json',
            '--color', 'never', '--model', model, '--cd', str(folder),
            '--add-dir', str(run), '--config', 'approval_policy="never"',
            '--config', 'model_reasoning_effort='+json.dumps(effort),
            '--config', 'sandbox_workspace_write.network_access=false',
            '--config', 'shell_environment_policy.inherit="none"',
            '--config', 'shell_environment_policy.set={PATH="/usr/local/bin:/usr/bin:/bin"}',
            '--output-last-message', str(folder/'final.txt'), '-']


def launch(run, assignment, model, effort):
    folder = run/'pool'/assignment['name']
    folder.mkdir()
    text = prompt(run, assignment['rules'], assignment['name'], run/'pool/knowledge.json')
    (folder/'prompt.txt').write_text(text)
    receipt = dict(assignment, started_unix=time.time(), status='starting', model=model,
                   effort=effort)
    r.write(folder/'receipt.json', receipt)
    with (folder/'events.jsonl').open('w') as output, (folder/'stderr.log').open('w') as error, \
            (folder/'prompt.txt').open() as input_stream, (run/'pool/startup.lock').open('a') as lock:
        # Codex installs system skills during startup in a shared directory.
        # Serialize initialization, not model turns, to avoid installer races.
        fcntl.flock(lock, fcntl.LOCK_EX)
        process = subprocess.Popen(command(run, folder, model, effort), stdin=input_stream,
                                   stdout=output, stderr=error, text=True, start_new_session=True)
        receipt.update(pid=process.pid, status='running')
        r.write(folder/'receipt.json', receipt)
        deadline = time.monotonic() + 60
        while process.poll() is None and time.monotonic() < deadline:
            if '"thread.started"' in (folder/'events.jsonl').read_text():
                break
            time.sleep(.1)
        fcntl.flock(lock, fcntl.LOCK_UN)
        process.wait()
    receipt.update(status='completed' if process.returncode == 0 else 'failed',
                   returncode=process.returncode, finished_unix=time.time())
    receipt['wall_seconds'] = receipt['finished_unix'] - receipt['started_unix']
    r.write(folder/'receipt.json', receipt)
    return receipt


def summarize(run):
    jobs, escalations = [], []
    for job in sorted((run/'jobs').iterdir()):
        description = r.read(job/'job.json')
        result = r.read(job/'result.json') if (job/'result.json').exists() else {}
        entry = {'rule': description['rule'], 'status': result.get('status', 'pending'),
                 'reason': result.get('reason'), 'failures': result.get('failures', []),
                 'evidence': str(job/'result.json')}
        jobs.append(entry)
        if entry['status'] in ('needs-review', 'failed'):
            lane = ('operational' if entry['status'] == 'failed' else
                    'dependency-or-broader-validation' if entry['reason'] else 'larger-model')
            escalations.append(dict(entry, lane=lane,
                                    required='isolated repair, regression tests, full batch gates'))
    summary = {'jobs': jobs, 'escalations': escalations,
               'provisional_passes': sum(j['status'] == 'provisional-pass' for j in jobs),
               'remaining': sum(j['status'] in ('pending', 'running') for j in jobs)}
    r.write(run/'pool/results.json', summary)
    return summary


def run_pool(args):
    run = args.run.resolve()
    manifest = r.verify(run)
    r.require(1 <= args.workers <= manifest['slots'], 'pool exceeds epoch capacity')
    pool = run/'pool'
    pool.mkdir()  # Fail closed on replay: unknown children must be inspected first.
    rules = sorted(r.read(run/'scan/rules.json')['unused'])
    assignments = [{'name': f'luna-{i+1:02}', 'rules': rules[i::args.workers]}
                   for i in range(min(args.workers, len(rules)))]
    r.write(pool/'inventory.json', inventory(run))
    r.write(pool/'knowledge.json', {'source': manifest['source'],
                                  'prior_findings': prior_findings(args.prior, run),
                                  'reuse': 'diagnostics only; every assignment runs its own checks'})
    r.write(pool/'plan.json', {'model': args.model, 'effort': args.effort,
                             'assignments': assignments, 'snapshot': r.digest(run/'manifest.json')})
    started = time.time()
    r.write(pool/'status.json', {'status': 'running', 'started_unix': started, 'pid': os.getpid()})
    with ThreadPoolExecutor(max_workers=args.workers) as executor:
        receipts = list(executor.map(lambda a: launch(run, a, args.model, args.effort), assignments))
    r.verify(run)
    results = summarize(run)
    failed = any(row['returncode'] for row in receipts) or results['remaining']
    status = 'needs-operator' if failed else 'workers-complete'
    r.write(pool/'status.json', {'status': status, 'started_unix': started,
                               'finished_unix': time.time(), 'workers': len(receipts),
                               'provisional_passes': results['provisional_passes'],
                               'escalations': len(results['escalations'])})
    if not failed and results['provisional_passes'] and args.validate:
        r.write(pool/'status.json', {'status': 'validating', 'started_unix': started,
                                    'workers': len(receipts)})
        runner = run/'frozen/runner/retire.py'
        for action in ('assemble', 'validate'):
            with (pool/(action+'.log')).open('w') as log:
                subprocess.run([sys.executable, str(runner), action, str(run)], check=True,
                               stdout=log, stderr=log)
        r.write(pool/'validation-reference.json', {'path': str(run/'combined/validation.json'),
                                                  'sha256': r.digest(run/'combined/validation.json'),
                                                  'source_applied': False})
        validation = r.read(run/'combined/validation.json')
        r.write(pool/'status.json', {'status': validation['status'], 'started_unix': started,
                                    'finished_unix': time.time(), 'workers': len(receipts),
                                    'provisional_passes': results['provisional_passes'],
                                    'escalations': len(results['escalations']),
                                    'source_applied': False})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run', type=Path)
    parser.add_argument('--workers', type=int, default=12)
    parser.add_argument('--model', default='gpt-6-luna')
    parser.add_argument('--effort', default='max')
    parser.add_argument('--prior', type=Path)
    parser.add_argument('--validate', action='store_true')
    args = parser.parse_args()
    try:
        run_pool(args)
    except FileExistsError:
        # A second controller must never rewrite the first controller's state.
        raise
    except Exception as error:
        pool = args.run.resolve()/'pool'
        if (pool/'status.json').exists():
            status = r.read(pool/'status.json')
            status.update(status='needs-operator', error=str(error))
            r.write(pool/'status.json', status)
        raise


if __name__ == '__main__':
    main()
