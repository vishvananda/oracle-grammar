#!/usr/bin/env python3
"""Promote an isolated mechanical candidate after its retained checks complete."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
import time

sys.dont_write_bytecode = True
import dev
import retire as r

GATES = {'fmt', 'pin', 'hygiene', 'tools', 'clippy', 'build', 'families', 'test'}


def gate_results(text):
    matches = re.findall(
        r'^(fmt|pin|hygiene|tools|clippy|build|families|test)\s+(passed|reused|FAILED)\s+',
        text, re.MULTILINE)
    found = dict(matches)
    complete = len(matches) == len(GATES) and set(found) == GATES
    return {'complete': complete,
            'passed': complete and all(v != 'FAILED' for v in found.values()),
            'gates': found}


def git(root, *arguments):
    return subprocess.check_output(['git', *arguments], cwd=root, text=True).strip()


def check_candidate(run, root):
    manifest = r.verify(run)
    selection = r.read(run/'combined/selection.json')
    r.require(root != Path(__file__).resolve().parent.parent, 'never promote the main checkout')
    r.require(git(root, 'rev-parse', 'HEAD') == manifest['commit'], 'checkout base changed')
    r.require(dev.source(root) == manifest['source'], 'compiled source changed')
    r.require(r.tree(root/'grammar') == selection['grammar'] == r.tree(run/'combined/grammar'),
              'candidate differs from the staged and audited grammar')
    r.require(not git(root, 'diff', '--cached', '--name-only'), 'index must be empty')
    changed = git(root, 'diff', '--name-only').splitlines()
    r.require(changed and all(p.startswith('grammar/') for p in changed),
              'checkout contains changes outside the mechanical patch')
    r.require(not git(root, 'ls-files', '--others', '--exclude-standard'), 'untracked source files')
    return manifest, selection


def accepted(validation, run, selection):
    return (validation['status'] == 'validated' and not validation['failures']
            and validation['snapshot'] == r.digest(run/'manifest.json')
            and validation['grammar'] == selection['grammar']
            and validation['patch'] == r.digest(run/'combined/retirement.patch'))


def finish(args):
    run, root = args.run.resolve(), args.checkout.resolve()
    output = run/'checkpoint'
    output.mkdir()  # No implicit replay after a commit, scan or unknown launch.
    with r.locked(output/'lease'):
        manifest, selection = check_candidate(run, root)
        r.require(re.fullmatch('[0-9a-f]{32}', args.gates_invocation), 'invalid gate invocation')
        r.require(subprocess.run(['git', 'check-ref-format', '--branch', args.branch], cwd=root,
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0,
                  'invalid checkpoint branch')
        r.require(subprocess.run(['git', 'show-ref', '--verify', '--quiet', 'refs/heads/'+args.branch],
                                 cwd=root).returncode == 1, 'checkpoint branch already exists')
        deadline = time.monotonic() + args.wait_seconds
        while True:
            journal = subprocess.check_output(['journalctl', '--user', '--no-pager', '-o', 'cat',
                                               '_SYSTEMD_INVOCATION_ID='+args.gates_invocation],
                                              text=True)
            gates = gate_results(journal)
            (output/'repository-gates.log').write_text(journal)
            r.write(output/'gates.json', dict(gates, invocation=args.gates_invocation))
            audit = run/'combined/validation.json'
            r.write(output/'status.json', {'status': 'waiting', 'gates_complete': gates['complete'],
                                           'audit_complete': audit.exists()})
            if gates['complete'] and audit.exists():
                break
            if time.monotonic() >= deadline:
                r.write(output/'status.json', {'status': 'needs-operator',
                                               'reason': 'wait expired; checks were not terminated'})
                return
            time.sleep(15)
        validation = r.read(audit)
        if not gates['passed'] or not accepted(validation, run, selection):
            r.write(output/'status.json', {'status': 'needs-review', 'gates_passed': gates['passed'],
                                           'validation': validation['status'], 'source_applied': False})
            return
        check_candidate(run, root)
        git(root, 'switch', '-c', args.branch)
        git(root, 'add', '--', 'grammar')
        git(root, 'commit', '-m', 'Retire validated mechanical grammar rules')
        commit = git(root, 'rev-parse', 'HEAD')
        r.require(not git(root, 'status', '--porcelain'), 'checkpoint is dirty after commit')
        r.write(output/'status.json', {'status': 'committed-awaiting-scan', 'commit': commit,
                                       'branch': args.branch, 'main_changed': False})
        scan = output/'scan'
        scan.mkdir()
        frozen = run/'frozen'
        r.execute(run, scan, root/'grammar', 'rule-audit',
                  [frozen/'gold.jsonl', frozen/'oracle.json', frozen/'allowed.json', scan/'rules.json'],
                  manifest['slots'] * manifest['threads'])
        report = r.read(scan/'rules.json')
        r.require(report['summary']['faces'] == report['summary']['equal'] == manifest['population']
                  and report['summary']['at_risk'] == 0, 'follow-up scan is incomplete or at risk')
        r.require(r.tree(root/'grammar') == selection['grammar'], 'checkpoint changed during scan')
        r.verify(run)
        previous = set(r.read(run/'scan/rules.json')['unused'])
        r.write(output/'status.json', {'status': 'accepted-isolated-checkpoint', 'commit': commit,
                                       'branch': args.branch, 'checkout': str(root),
                                       'rules': selection['rules'], 'grammar': selection['grammar'],
                                       'compiled_source': manifest['source'],
                                       'validation_sha256': r.digest(audit),
                                       'gates_sha256': r.digest(output/'gates.json'),
                                       'scan_sha256': r.digest(scan/'rules.json'),
                                       'remaining_unused': report['unused'],
                                       'new_mechanical_candidates': sorted(set(report['unused'])-previous),
                                       'main_changed': False, 'pushed': False})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run', type=Path)
    parser.add_argument('--checkout', type=Path, required=True)
    parser.add_argument('--gates-invocation', required=True)
    parser.add_argument('--branch', required=True)
    parser.add_argument('--wait-seconds', type=int, default=21600)
    args = parser.parse_args()
    try:
        finish(args)
    except FileExistsError:
        raise
    except Exception as error:
        output = args.run.resolve()/'checkpoint'
        if output.exists():
            status = r.read(output/'status.json') if (output/'status.json').exists() else {}
            r.write(output/'status.json', dict(status, status='needs-operator', error=str(error)))
        raise


if __name__ == '__main__':
    main()
