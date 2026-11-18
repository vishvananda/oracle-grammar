"""Isolate failed repository library tests using their frozen executable."""
import os
from pathlib import Path
import re
import subprocess

import retire as r
import retire_batch as batch
import retire_cpu as cpu
import retire_library_controls as controls
import retire_library_readmission as proof
import retire_parse_isolate as replay
from retire_native_readmission import _allowed
from retire_queue import Queue, _gates


def _archive(campaign, batch_id):
    r.require(re.fullmatch(r'[0-9a-f]{32}', batch_id), 'invalid batch ID')
    folder = campaign/'.cache/batches'/batch_id
    evidence = {key: replay._spec(folder/name) for key, name in
                (('manifest', 'manifest.json'), ('validation', 'validation.json'),
                 ('repository_log', 'repository-check.log'))}
    manifest = r.read(folder/'manifest.json')
    validation = r.read(folder/'validation.json')
    origin = manifest['base_commit']
    jobs = [item for item in manifest['jobs'] if item['status'] == 'included']
    ids = [item['job_id'] for item in jobs]
    log = (folder/'repository-check.log').read_bytes()
    r.require(manifest['id'] == batch_id and manifest['status'] == 'provisional'
              and 2 <= len(ids) <= 96 and len(set(ids)) == len(ids)
              and all(item['kind'] in ('author', 'mechanical') for item in jobs)
              and validation['candidate'] == manifest['commit'] and validation['source_checkpoint'] == origin
              and validation['included_jobs'] == ids and validation['status'] == 'failed'
              and validation['scope'] == 'combined-batch'
              and validation['reason'] == proof.LIBRARY_REJECTION
              and validation['repository_check_returncode'] == 1,
              'batch does not contain an isolated library gate failure')
    return folder, manifest, jobs, proof._repository_library_failure(log, _gates), evidence


def _check(checkpoint, binary, root, tests):
    results, failed = [], []
    grammar = {'ORACLE_GRAMMAR_DIR':str(root/'grammar')}
    env = {k:v for k,v in os.environ.items() if not k.startswith('ORACLE_GRAMMAR_')}
    env.update(grammar, ORACLE_GRAMMAR_THREADS='2', RUST_TEST_THREADS='2', RAYON_NUM_THREADS='2')
    with cpu.reserve(Path(checkpoint['cpu_pool']), 2, checkpoint['cpu_capacity'], background=True) as fds:
        for i, test in enumerate(tests):
            log, result = root/f'test-{i:03d}.log', root/f'test-{i:03d}-result.json'
            command = [binary['path'], '--exact', test, '--nocapture']
            with log.open('x') as stream:
                code = subprocess.run(command, stdout=stream, stderr=stream, env=env,
                                      pass_fds=fds, check=False, timeout=180).returncode
            r.require(code in (0, 101), 'focused library test did not reach a terminal assertion')
            r.write(result, {'test':test, 'source_sha256':checkpoint['source_sha256'],
                             'returncode':code, 'binary':binary, 'command':command, 'environment':grammar})
            results.append({'test':test, 'result':replay._spec(result), 'log':replay._spec(log)})
            if code:
                failed.append(test)
    return {'failures':failed}, results


def isolate(campaign, batch_id, scratch, max_trials):
    r.require(type(max_trials) is int and 3 <= max_trials <= 96, 'use 3–96 focused library trials')
    folder, manifest, jobs, tests, evidence = _archive(campaign, batch_id)
    origin = manifest['base_commit']
    cp = r.read(campaign/'checkpoints'/f'{origin}.json')
    command = controls.setup(cp, cp['source'])
    r.require(command is not None and _allowed(campaign, command['binary'])
              and cp['commit'] == origin and _allowed(campaign, scratch) and not scratch.exists(),
              'no matching protected library executable or fresh scratch directory')
    binary = replay._spec(command['binary'])
    r.require(binary['sha256'] == command['binary_sha256'], 'the protected test binary changed')
    base = command['manifest_path'].parent/'frozen/grammar'
    rejected = folder/'validation-inputs/frozen/grammar'
    before = r.tree(base)
    r.require(r.tree(rejected) == manifest['grammar'], 'the rejected grammar changed')
    scratch.mkdir(parents=True)
    checkpoint = dict(cp, source_sha256=command['source_sha256'])
    indices = list(range(len(jobs)))
    mechanical = [i for i in indices if jobs[i]['kind'] == 'mechanical']
    prefix = mechanical if mechanical == indices[:len(mechanical)] else []
    cached = batch._ProposalInputCache()
    with cpu.reserve(Path(cp['cpu_pool']), 1, cp['cpu_capacity'], background=True):
        proposals = list(zip(jobs, replay._proposals(campaign, jobs, scratch, cached), strict=True))
    seen, receipts = {}, {}

    def trial(selection, label=None):
        key = tuple(selection)
        if key not in seen:
            r.require(len(seen) < max_trials, 'focused library trial limit reached')
            root = scratch/(label or f'trial-{len(seen):03d}')
            ordered = selection
            grammar = rejected if label == 'full' else base
            if label == 'full':
                ordered = []
            elif prefix and tuple(prefix) in seen and selection[:len(prefix)] == prefix:
                saved = seen[tuple(prefix)]
                grammar, ordered = saved['root']/'grammar', selection[len(prefix):]
                r.require(r.tree(grammar) == saved['tree'], 'cached mechanical grammar changed')
            with cpu.reserve(Path(cp['cpu_pool']), 1, cp['cpu_capacity'], background=True):
                tree = replay._materialize(root, grammar, selection, ordered, proposals, cached)
            identity = root/'identity.json'
            outcome, results = _check(checkpoint, binary, root, tests)
            r.require(r.tree(root/'grammar') == tree, 'focused library grammar changed during validation')
            seen[key] = {'root':root, 'tree':tree, 'outcome':outcome,
                         'receipt':{'identity':replay._spec(identity), 'tests':results}}
            r.write(scratch/'progress.json', [{'indices':list(indices), 'folder':str(trial['root']),
                                              **trial['outcome']} for indices, trial in seen.items()])
        item = seen[key]
        if label:
            receipts[label] = item['receipt']
        return item['outcome']

    r.require(not trial([], 'base')['failures'], 'the accepted grammar fails the focused library tests')
    pinned = prefix if prefix and not trial(prefix, 'mechanical')['failures'] else []
    r.require(trial(indices, 'full')['failures'] == tests, 'the full library failure did not reproduce')
    remaining = replay._survivors(indices, trial, pinned)
    excluded = [jobs[i]['job_id'] for i in indices if i not in remaining]
    r.require(excluded and remaining and not trial(remaining, 'remaining')['failures'],
              'no nonempty library survivors passed together')
    controls.verify(command)
    r.require(r.tree(base) == before and r.tree(rejected) == manifest['grammar']
              and all(r.digest(spec['path']) == spec['sha256'] for spec in evidence.values()),
              'the archived library evidence changed')
    path = scratch/'isolation.json'
    r.write(path, {'kind':'isolated-library-test-regression', 'manifest':batch_id,
                   'candidate':manifest['commit'], 'parent':origin, 'failed_tests':tests,
                   'excluded_jobs':excluded, 'reason':'The remaining proposals pass the original failed library tests.',
                   'controls':{name:receipts[name] for name in ('base', 'full', 'remaining')}})
    selected = [jobs[i]['job_id'] for i in remaining]
    result = {'jobs':selected, 'rules':sum(len(jobs[i]['rules']) for i in remaining),
              'evidence':{**evidence, 'kind':'isolated-batch-library-revalidation',
                          'source_checkpoint':origin, 'isolation':replay._spec(path),
                          'reports':{jobs[i]['job_id']:proposals[i][1][-1] for i in remaining}}}
    r.write(scratch/'result.json', result)
    return result


def readmit_result(campaign, scratch, result):
    queue = Queue(campaign/'queue.sqlite')
    return replay._readmit(queue, scratch, result, queue.readmit_library_batch_proposals)
