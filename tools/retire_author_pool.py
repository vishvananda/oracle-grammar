#!/usr/bin/env python3
"""Run a paired, three-slot authoring pilot from fixed private task descriptors."""
from concurrent.futures import FIRST_COMPLETED, ThreadPoolExecutor, wait
import fcntl
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

sys.dont_write_bytecode = True
import retire as r
import retire_pool as pool


def prompt(run, task_id):
    task = r.read(run/'tasks'/f'{task_id}.json')
    return f'''Author shared Oracle grammar for this bounded pilot task.
Read AGENTS.md, SPEC.md, docs/print-shapes.md and {run/'tasks'/f'{task_id}.json'}.
Goal: {task['goal']}
Remove exactly these seeded rules: {json.dumps(task['rules'])}.
Write only: {json.dumps(task['write_files'])}.
Use current accepted source in your private checkout. Do not inspect other
checkouts, old draft campaigns, main's unaccepted work, other model reports,
session files, held-out data or external sources. Relevant allowed input is
{run/'inputs/frozen/gold.jsonl'} and {run/'inputs/frozen/oracle.json'}; read only
assigned target/cohort faces from the descriptor, using a filter script.
Use a reusable phrase production with one shared @sem, not a renamed/adopted
whole seeded rule. Preserve all readings, holder/actor/owner/controller/result
identities and original Oracle words. Do not rewrite text, widen equality,
change expected reference IR or use first-match semantics. Split inflections
and pronouns into separate productions per docs/print-shapes.md.
Apply the mechanical seeded removals with this descriptor-bound command instead
of editing huge seeded hook lines or semantic comments manually:
{json.dumps([sys.executable, str(run/'support/runner/retire_author.py'), str(run), task_id, '--remove-assigned'])}
It reconstructs seeded files from the fixed base, removes exactly the assigned
rules and saves private backups. Put new shared productions in your declared
handwritten modules, never in mined.ebnf.
This is syntax-only. If Rust, lexical, IR, binding or out-of-scope module changes
are necessary, record the exact reason and escalate. Do not edit those files.
No Cargo, git commits/staging, whole-corpus scans, additional agents or network.
Prebuilt probe, inspect, family-check helpers are under {run/'support/bin'}.
To use them set ORACLE_GRAMMAR_DIR=$PWD/grammar and ORACLE_GRAMMAR_THREADS=2.
Read their -- usage from src/bin if needed, instead of building anything.
Create {task['fixture']} with 4-64 focused family cases including an explicit
expected IR, a refusal and exact/default printing controls. It will run with
--compositional. Derive expected IR from the fixed allowed gold, not changed output.
Create {task['edits']} containing 2-8 current-operand edit tests. Each is:
{{"face":"assigned target face", "text":"complete edited Oracle text",
"set":[{{"path":[0,"args",...],"value":"new primitive"}}]}}.
Paths index into that face's immutable gold abilities. The runner edits the IR
itself and then prints/reparses it against the edited text. Use semantic operand
changes (filter, amount, recipient etc.), not identifier renaming or unchanged
values. Include independent operands when the family supports them.
After grouped changes run this exact bounded diagnostic command and read its report:
{json.dumps([sys.executable, str(run/'support/runner/retire_author.py'), str(run), task_id])}
Keep/wait on the same exec handle; do not restart a running helper. There are at
most eight diagnostic attempts and 25 minutes for this task. Fix useful bounded
failures; escalate when the next step needs a different contract. All four print
gates compare baseline successes: an already-false gate is not a new regression.
Do not forge or edit diagnostic reports or create files outside the write set
(scratch notes and outputs may go in ignored .cache/). Inspect returned artifact
paths rather than guessing. Return concise findings, changed shared capability,
test evidence, unresolved gates and suggested escalation. A passing cohort is
provisional; coordinator independently evaluates frozen candidate snapshots.
'''


def launch(run, task_id):
    descriptor = r.read(run/'tasks'/f'{task_id}.json')
    folder = run/'workers'/task_id
    folder.mkdir()
    text = prompt(run, task_id)
    (folder/'prompt.txt').write_text(text)
    cmd = pool.command(run, folder, descriptor['model'], descriptor['effort'])
    # The author may write only its private checkout. No shared epoch write root.
    pos = cmd.index('--add-dir')
    del cmd[pos:pos+2]
    cmd[cmd.index('--cd')+1] = descriptor['checkout']
    receipt = {'task': task_id, 'model': descriptor['model'], 'effort': descriptor['effort'],
               'started_unix': time.time(), 'status': 'starting'}
    r.write(folder/'receipt.json', receipt)
    with (folder/'events.jsonl').open('w') as output, (folder/'stderr.log').open('w') as error, \
            (folder/'prompt.txt').open() as input_stream, (run/'startup.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        process = subprocess.Popen(cmd, stdin=input_stream, stdout=output, stderr=error,
                                   text=True, start_new_session=True)
        receipt.update(pid=process.pid, status='running')
        r.write(folder/'receipt.json', receipt)
        deadline = time.monotonic()+60
        while process.poll() is None and time.monotonic() < deadline:
            if '"thread.started"' in (folder/'events.jsonl').read_text():
                break
            time.sleep(.1)
        fcntl.flock(lock, fcntl.LOCK_UN)
        try:
            process.wait(timeout=1500)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            receipt['timed_out'] = True
    receipt.update(returncode=process.returncode, finished_unix=time.time(),
                   status='completed' if process.returncode == 0 else 'needs-review')
    receipt['wall_seconds'] = receipt['finished_unix']-receipt['started_unix']
    r.write(folder/'receipt.json', receipt)
    # Agent prose and mutable local reports never establish quality.
    try:
        # Each evaluation owns its descriptor leases in one process. Sharing
        # retire._LEASES between executor threads could inherit another task's FD.
        checked = subprocess.run([sys.executable, str(run/'support/runner/retire_author.py'),
                                  str(run), task_id, '--coordinator'],
                                 text=True, capture_output=True)
        (folder/'evaluation.log').write_text(checked.stdout+checked.stderr)
        r.require(checked.returncode == 0, 'independent evaluation failed; inspect evaluation.log')
        evaluation = json.loads(checked.stdout.strip())
        receipt['evaluation'] = {'passed': evaluation['passed'], 'directory': evaluation['directory']}
    except Exception as error:
        receipt['evaluation'] = {'passed': False, 'error': str(error)}
    r.write(folder/'receipt.json', receipt)
    return receipt


def run_pairs(run, plan):
    """Keep slots occupied while preserving the first-model order of each pair."""
    first, second = plan['waves']
    successors = {task: next(other for other in second if other.rsplit('-', 1)[1]
                             == task.rsplit('-', 1)[1]) for task in first}
    r.require(len(first) <= 3 and len(set(first+second)) == len(first)+len(second)
              and len(set(successors.values())) == len(second), 'invalid paired task plan')
    receipts = []
    with ThreadPoolExecutor(max_workers=3) as executor:
        pending = {executor.submit(launch, run, task): task for task in first}
        while pending:
            completed, _ = wait(pending, return_when=FIRST_COMPLETED)
            for future in completed:
                task = pending.pop(future)
                receipts.append(future.result())
                if task in successors:
                    following = successors[task]
                    pending[executor.submit(launch, run, following)] = following
    return receipts


def main():
    run = Path(sys.argv[1]).resolve()
    with r.locked(run/'controller.lock', blocking=False) as acquired:
        r.require(acquired and not (run/'status.json').exists(), 'pilot already launched')
        r.write(run/'status.json', {'status': 'running', 'pid': os.getpid()})
        plan = r.read(run/'plan.json')
        receipts = run_pairs(run, plan)
        r.write(run/'status.json', {'status': 'evaluated', 'workers': receipts,
                                    'acceptance': 'bounded pilot only; integration not established'})


if __name__ == '__main__':
    main()
