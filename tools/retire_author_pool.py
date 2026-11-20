#!/usr/bin/env python3
"""Run a paired, three-slot authoring pilot from fixed private task descriptors."""
from concurrent.futures import FIRST_COMPLETED, ThreadPoolExecutor, wait
from contextlib import nullcontext
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
import retire_cpu
import retire_pool as pool


def prompt(run, task_id):
    task = r.read(run/'tasks'/f'{task_id}.json')
    packet = task.get('packet')
    packet_note = (f'Read the compact immutable task packet {packet}. It contains the rule declarations, '
                   'assigned reference operands and original record addresses. Join gold and Oracle records by face name '
                   'and their own record/face addresses independently; never zip the two arrays or assume matching order. '
                   'Use packet baseline text for original family wording; separately flag Oracle exact-text/reminder discrepancies. '
                   'Filter it with Python; '
                   'do not dump whole corpora. Start with declared modules and use rg --files before guessing paths.\n'
                   if packet else '')
    limit = task.get('max_diagnostic_attempts', 8)
    minutes = task.get('wall_budget_seconds', 1500) // 60
    diagnostic = [sys.executable, str(run/'support/runner/retire_author.py'), str(run), task_id]
    diagnostic_note = 'After grouped changes, run the bounded diagnostic and read its report.'
    if task.get('full_pass'):
        diagnostic.append('--preview')
        diagnostic_note = (
            'After grouped changes, run the target preview and read its report. '
            'It checks target readings, printing, and the authored family.\n'
            'Return your per-rule dispositions after the preview. '
            'The coordinator independently checks the full assigned cohort and edited operands after your response.'
        )
        if task.get('preview_edited_operands') is True:
            diagnostic_note = (
                'After grouped changes, run the target preview and read its report. '
                'It checks the authored family, target readings and printing, then your whole-card operand edits. '
                'Fix a bounded edited-operand refusal or print failure before returning your per-rule dispositions. '
                'The coordinator independently checks the full assigned cohort and edited operands after your response.'
            )
    budget_note = (f'There are at\nmost {limit} diagnostic attempts and {minutes} minutes for this task. Allow one repair round,\n'
                   'then park failures with evidence rather than extending the task. Fix useful bounded\n'
                   'failures; escalate when the next step needs a different contract.')
    if task.get('repair_until_budget') is True:
        budget_note = (
            f'There are at\nmost {limit} diagnostic attempts and {minutes} minutes for this task. The family-check\n'
            'wrapper does not count as a diagnostic attempt; rerun it freely on your fixture.\n'
            'Keep repairing bounded failures (fixture refusals, lost readings, print gates)\n'
            f'while attempts and time remain; do not start a diagnostic after minute {max(1, minutes-3)}.\n'
            'Park a rule with evidence only when the next step needs a different contract\n'
            'or the budget is spent. Never weaken a check to make it pass.')
    return f'''Author shared Oracle grammar for this bounded pilot task.
Read AGENTS.md, SPEC.md, docs/print-shapes.md and {run/'tasks'/f'{task_id}.json'}.
{packet_note}
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
For receipt-derived counts such as milled, exiled or discarded this way, both
parsing and inverse printing must match the actual producer and destination.
Transition(@prior, After) alone does not justify action-specific wording; scope
the whole producer-consumer contract or use a supported typed guard. If neither
is available, record the missing proof and escalate rather than equating phrases.
If an added reading appears meaningful but disagrees with baseline IR, mark it
needs-review as reference-review or equivalence-needed with evidence. Do not
suppress readings by first match or alter IR solely to match the reference.
Exact keyed preservation does not waive default-print or semantic reparse gates.
Apply the mechanical seeded removals with this descriptor-bound command instead
of editing huge seeded hook lines or semantic comments manually:
{json.dumps([sys.executable, str(run/'support/runner/retire_author.py'), str(run), task_id, '--remove-assigned'])}
It reconstructs seeded files from the fixed base, removes exactly the assigned
rules and saves private backups. Put new shared productions in your declared
handwritten modules, never in mined.ebnf.
If a new shared production needs registration in another module, write an array
in ignored .cache/author-hooks.json, for example
[{{"root":"ability","production":"your_new_shared_trigger"}}]. Allowed roots are
ability, line and event_clause. Do not place handwritten roots into mined hooks
or edit the root module outside your write set. The coordinator applies typed
ordinary hook requests to its private diagnostic snapshot and records them.
The registered production itself needs a declared @sem Ability:: result for
ability or line, or @sem EventSpec:: for event_clause. For a bare alternative
list, register a typed wrapper which captures the list's result.
This is syntax-only. If Rust, lexical, IR, binding or out-of-scope module changes
are necessary, record the exact reason and escalate. Do not edit those files.
No Cargo, git commits/staging, whole-corpus scans, additional agents or network.
Prebuilt probe, inspect, family-check wrappers are under {run/'support/bin'}.
These wrappers acquire CPU leases; invoke them directly and never invoke the
private native helpers. Set ORACLE_GRAMMAR_DIR=$PWD/grammar and ORACLE_GRAMMAR_THREADS=2.
Read their -- usage from src/bin if needed, instead of building anything.
Create {task['fixture']} with 4-64 focused family cases including an explicit
expected IR, a refusal and exact/default printing controls. It will run with
--compositional. Derive expected IR from the fixed allowed gold, not changed output.
Each case's name is its card name for tokenization. Use the actual face name
for self-references. For synthetic cases, choose a name whose full and shortened
forms do not occur in the rules text.
Keep every authored family case compositional; do not set compositional:false.
The coordinator separately checks whole-card operand edits with any unrelated,
unassigned seeded clauses retained; that check does not establish phrase coverage.
Create {task['edits']} containing 2-8 current-operand edit tests. Each is:
{{"face":"assigned target face", "text":"complete edited Oracle text",
"set":[{{"path":[0,"args",...],"value":"new primitive"}}]}}.
If a face name occurs in multiple records, add "record_faces", an array with
every face name in the intended record, including the target.
Paths index into that face's immutable gold abilities. The runner edits the IR
itself and then prints/reparses it against the edited text. Use semantic operand
changes (filter, amount, recipient etc.), not identifier renaming or unchanged
values. Include independent operands when the family supports them.
{diagnostic_note}
{json.dumps(diagnostic)}
Keep/wait on the same exec handle; do not restart a running helper. {budget_note} All four print
gates compare baseline successes: an already-false gate is not a new regression.
Do not forge or edit diagnostic reports or create files outside the write set
(scratch notes and outputs may go in ignored .cache/). Inspect returned artifact
paths rather than guessing. Return concise findings, changed shared capability,
test evidence, unresolved gates and suggested escalation. A passing cohort is
provisional; coordinator independently evaluates frozen candidate snapshots.
'''


def response_schema(rules):
    return {'type': 'object', 'additionalProperties': False,
            'required': ['summary', 'rules'], 'properties': {
                'summary': {'type': 'string'},
                'rules': {'type': 'array', 'items': {'type': 'object',
                    'additionalProperties': False, 'required': ['rule', 'status', 'reason', 'evidence'],
                    'properties': {'rule': {'type': 'string', 'enum': rules},
                        'status': {'type': 'string', 'enum': ['proposed', 'needs-review']},
                        'reason': {'type': 'string'}, 'evidence': {'type': 'string'}}}}}}


def model_slot(startup_lock):
    if startup_lock is None:
        return nullcontext(())
    campaign = Path(startup_lock).parent
    config = campaign/'config.json'
    if not config.is_file():
        return nullcontext(())
    capacity = r.read(config)['workers']
    return retire_cpu.reserve(campaign/'model-slots', 1, capacity)


def launch(run, task_id, on_started=None, extra_fds=(), startup_lock=None, *, previous_evaluation=None):
    descriptor = r.read(run/'tasks'/f'{task_id}.json')
    folder = run/'workers'/task_id
    folder.mkdir()
    text = prompt(run, task_id)
    if previous_evaluation:
        text += (f'\nThe restored draft failed independent evaluation at the current checkpoint. '
                 f'Read {previous_evaluation} and use its failures to guide the repair.\n')
    if descriptor.get('full_pass'):
        text += '''\nReturn the required structured JSON response with exactly one disposition for
EVERY assigned rule. Use proposed only when the target preview passes;
otherwise needs-review with that rule's concrete missing capability, unsupported
contract, dependency or failed gate. Cite a source fragment, reproduction or
diagnostic artifact in evidence. A generic refusal covering the entire bundle
does not establish review of its individual members. Do not mark review solely
because the coordinator's full batch audit has not run yet.\n'''
    (folder/'prompt.txt').write_text(text)
    cmd = pool.command(run, folder, descriptor['model'], descriptor['effort'])
    # Only the CPU lease pool is writable outside the private checkout.
    pos = cmd.index('--add-dir')
    if descriptor.get('cpu_pool'):
        cmd[pos+1] = str(Path(descriptor['cpu_pool']))
    else:
        del cmd[pos:pos+2]
    cmd[cmd.index('--cd')+1] = descriptor['checkout']
    if descriptor.get('full_pass'):
        schema = run/'response-schema.json'
        r.write(schema, response_schema(descriptor['rules']))
        cmd[-1:-1] = ['--output-schema', str(schema)]
    startup = Path(startup_lock) if startup_lock else run/'startup.lock'
    with model_slot(startup_lock) as model_fds, \
            (folder/'events.jsonl').open('w') as output, (folder/'stderr.log').open('w') as error, \
            (folder/'prompt.txt').open() as input_stream, startup.open('a') as lock:
        receipt = {'task': task_id, 'model': descriptor['model'], 'effort': descriptor['effort'],
                   'started_unix': time.time(), 'status': 'starting'}
        r.write(folder/'receipt.json', receipt)
        fcntl.flock(lock, fcntl.LOCK_EX)
        process = subprocess.Popen(cmd, stdin=input_stream, stdout=output, stderr=error,
                                   text=True, start_new_session=True, pass_fds=(*extra_fds, *model_fds))
        try:
            receipt.update(pid=process.pid, status='running')
            r.write(folder/'receipt.json', receipt)
            if on_started:
                on_started(process.pid, str(folder/'receipt.json'))
            deadline = time.monotonic()+60
            while process.poll() is None and time.monotonic() < deadline:
                if '"thread.started"' in (folder/'events.jsonl').read_text():
                    break
                time.sleep(.1)
        except BaseException:
            if process.poll() is None:
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    process.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    process.wait()
            raise
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)
        try:
            process.wait(timeout=descriptor.get('wall_budget_seconds', 1500))
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
        env = dict(os.environ)
        if extra_fds:
            env['RETIRE_JOB_FD'] = str(extra_fds[0])
        checked = subprocess.run([sys.executable, str(run/'support/runner/retire_author.py'),
                                  str(run), task_id, '--coordinator'],
                                 text=True, capture_output=True, env=env, pass_fds=tuple(extra_fds))
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
