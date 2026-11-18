"""Verify a narrowly isolated library-test batch failure for fresh revalidation."""
import hashlib
import json
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess
import tempfile

from retire_native_readmission import _allowed, _grammar_tree

LIBRARY_REJECTION = 'repository gates failed'

def _replay(campaign, checkpoint, included, indices, expected_tree, scratch, reports, *, canonical_mechanical=False):
    """Independently replay every retained protected proposal, in original order."""
    import mined
    import retire as r
    import retire_author as author
    import retire_batch as batch
    import retire_cpu
    import retire_merge as merger
    import retire_queue as queue

    cp = json.loads((campaign/'checkpoints'/f'{checkpoint}.json').read_text())
    base = Path(cp['inputs'])/'frozen'
    uri = (campaign/'queue.sqlite').as_uri()+'?mode=ro'
    db = sqlite3.connect(uri, uri=True, isolation_level=None)
    try:
        db.execute('BEGIN')
        originals = {}
        for index in indices:
            item = included[index]
            row = db.execute('SELECT a.job_id,a.base_commit,a.result,j.payload FROM attempts a '
                             'JOIN jobs j ON j.id=a.job_id WHERE a.id=?', (item['attempt_id'],)).fetchone()
            if not row or row[0] != item['job_id'] or not row[2]:
                raise ValueError('replay original attempt is missing')
            originals[index] = (row[1], json.loads(row[2]), json.loads(row[3]))
        db.commit()
    finally:
        db.close()
    # Ordered replay is single-threaded and gets a slot ahead of newly queued
    # worker checks while an audit and existing worker checks keep their leases.
    with retire_cpu.reserve(Path(cp['cpu_pool']), 1, cp.get('cpu_capacity', 32), priority_wait=True):
        with tempfile.TemporaryDirectory(prefix='.library-replay-', dir=scratch) as temporary:
            trial = Path(temporary)
            shutil.copytree(base/'grammar', trial/'grammar')
            inputs = batch._ProposalInputCache()
            for index in indices:
                item = included[index]
                base_commit, result, job = originals[index]
                original = result.get('evidence', {})
                rules, decisions, outcomes = job['rules'], original.get('dispositions', {}), result.get('outcomes', {})
                if (result.get('status') != 'provisional-pass' or set(decisions) != set(rules)
                        or set(outcomes) != set(rules)
                        or any(not isinstance(decisions[rule], dict) or not isinstance(outcomes[rule], dict)
                               or decisions[rule].get('rule', rule if job.get('kind') == 'mechanical' else None) != rule
                               or decisions[rule].get('status') != 'proposed' or not decisions[rule].get('reason')
                               or not decisions[rule].get('evidence') or outcomes[rule].get('status') != 'pass'
                               or not outcomes[rule].get('evidence') for rule in rules)):
                    raise ValueError('replay proposal lacks independent per-rule passes')
                report_path = Path(original.get('report', ''))
                report_spec = (reports[item['job_id']] if item['job_id'] in reports
                               else {'path': str(report_path), 'sha256': r.digest(report_path)})
                if original.get('kind') == 'author':
                    report, _ = queue._verify_original_report(campaign, item['job_id'], item['attempt_id'],
                                                              base_commit, original, report_spec)
                    if report.get('family_returncode') != 0:
                        raise ValueError('replay author family check did not pass')
                elif original.get('kind') == 'mechanical':
                    report = queue._verify_mechanical_readmission(campaign, item['job_id'], item['attempt_id'],
                                                                  base_commit, original, report_spec)
                else:
                    raise ValueError('replay proposal kind is unsupported')
                if (item.get('base_commit') != base_commit or item.get('rules') != rules
                        or item.get('kind') != original.get('kind')
                        or item.get('descriptor_sha256') != report.get('descriptor_sha256')
                        or item.get('support_sha256') != report.get('support_sha256')
                        or (original['kind'] == 'author' and item.get('fixture_sha256') != report.get('fixture_sha256'))
                        or type(report.get('faces')) is not int or report['faces'] <= 0):
                    raise ValueError('replay proposal identity differs')
                proposal = {'run': original['run'], 'report': report, 'kind': original['kind']}
                run, task, directory, hooks = batch._proposal(proposal, input_cache=inputs)
                if task.get('id') != item['job_id'] or task.get('rules') != rules:
                    raise ValueError('replay descriptor differs from owned rules')
                if original['kind'] == 'mechanical' and canonical_mechanical:
                    if hooks:
                        raise ValueError('replay mechanical proposal contains hooks')
                    batch._mechanical_removal(run, directory, rules, scratch)
                else:
                    changes = merger.merge_grammar(run/'inputs/frozen', trial, directory, rules, hooks)
                    if original['kind'] == 'mechanical' and (hooks or changes):
                        raise ValueError('replay mechanical proposal contains additional edits')
                    for name, text in changes.items():
                        path = trial/name
                        path.parent.mkdir(parents=True, exist_ok=True)
                        path.write_text(text)
                removed, kept = mined.remove(trial, rules)
                if set(removed) != set(rules) or kept:
                    raise ValueError('replay assigned removals have retained users')
                author.apply_hooks(trial/'grammar', hooks)
            if not inputs.unchanged():
                raise ValueError('proposal frozen inputs changed during ordered replay')
            if _grammar_tree(trial/'grammar') != expected_tree:
                raise ValueError('independent ordered replay differs from remaining control')


def _binary(campaign, spec):
    if not isinstance(spec, dict) or set(spec) != {'path', 'sha256'}:
        raise ValueError('focused test binary identity is missing')
    if not isinstance(spec['path'], str) or not isinstance(spec['sha256'], str):
        raise ValueError('focused test binary identity is invalid')
    path = Path(spec['path'])
    if (not path.is_absolute() or path.is_symlink() or not path.is_file() or not _allowed(campaign, path)
            or path.stat().st_size > 1024*1024*1024):
        raise ValueError('focused test binary is unavailable or outside scratch')
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024*1024), b''):
            digest.update(block)
    if digest.hexdigest() != spec['sha256']:
        raise ValueError('focused test binary hash differs')
    return path


def _control(campaign, spec, read_proof, expected_indices, included, should_fail, source_sha256, tests, binaries):
    if not isinstance(spec, dict) or not isinstance(spec.get('tests'), list):
        raise ValueError('library isolation control is missing')
    identity_path, identity_data = read_proof(spec.get('identity'))
    identity_path = identity_path.resolve()
    if not _allowed(campaign, identity_path) or identity_path.name != 'identity.json':
        raise ValueError('library isolation identity escapes scratch')
    root = identity_path.parent
    identity = json.loads(identity_data)
    expected_jobs = [{'job_id': included[i]['job_id'], 'attempt_id': included[i]['attempt_id']}
                     for i in expected_indices]
    if (not isinstance(identity, dict) or identity.get('method') != 'ordered-proposal-replay-v1'
            or identity.get('indices') != expected_indices or identity.get('proposals') != expected_jobs
            or not isinstance(identity.get('tree'), dict) or not identity['tree']
            or identity['tree'] != _grammar_tree(root/'grammar')):
        raise ValueError('library isolation grammar or ordered replay identity differs')
    if [item.get('test') if isinstance(item, dict) else None for item in spec['tests']] != tests:
        raise ValueError('library isolation must cover the exact failed tests')
    for index, item in enumerate(spec['tests']):
        values = {}
        for name in ('result', 'log'):
            path, data = read_proof(item.get(name))
            expected = root / (f'test-{index:03d}-result.json' if name == 'result' else f'test-{index:03d}.log')
            if path.resolve() != expected or not _allowed(campaign, path):
                raise ValueError('library isolation control artifacts do not share a run')
            values[name] = data if name == 'log' else json.loads(data)
        result = values['result']
        if not isinstance(result, dict):
            raise ValueError('library test result is invalid')
        binary_spec = result.get('binary')
        if not isinstance(binary_spec, dict):
            raise ValueError('focused test binary identity is missing')
        binary_key = (binary_spec.get('path'), binary_spec.get('sha256'))
        if binary_key not in binaries:
            binaries[binary_key] = _binary(campaign, binary_spec)
        binary = binaries[binary_key]
        test = tests[index]
        if (result.get('test') != test or result.get('source_sha256') != source_sha256
                or result.get('returncode') != (101 if should_fail else 0)
                or result.get('command') != [str(binary), '--exact', test, '--nocapture']
                or result.get('environment') != {'ORACLE_GRAMMAR_DIR': str(root/'grammar')}):
            raise ValueError('focused test command, grammar or exit differs')
        text = values['log'].decode('utf-8')
        if (len(re.findall(r'^running 1 test\s*$', text, re.M)) != 1
                or len(re.findall(r'^test '+re.escape(test)+r' \.\.\.', text, re.M)) != 1
                or not re.search(r'^test result: '+('FAILED' if should_fail else 'ok')
                                 +r'\. '+('0 passed; 1 failed;' if should_fail else '1 passed; 0 failed;'), text, re.M)):
            raise ValueError('focused library test log does not prove expected result')
    return identity


def _failed_tests(log):
    """Read terminal Rust failure summaries, excluding diagnostic and passing output."""
    summaries = re.findall(r'^test result: FAILED\. \d+ passed; (\d+) failed;', log, re.M)
    blocks = re.findall(r'^failures:\n((?:    [A-Za-z0-9_:]+\n)+)\ntest result: FAILED\.', log, re.M)
    names = [line.strip() for block in blocks for line in block.splitlines()]
    if (not summaries or len(blocks) != len(summaries) or sum(map(int, summaries)) != len(names)
            or not 1 <= len(names) <= 8 or len(set(names)) != len(names)
            or any('::' not in name for name in names)):
        raise ValueError('repository log does not identify an exact bounded failed test set')
    return names


def _repository_library_failure(log, gates):
    states = gates(log)
    if (states['test'] != 'FAILED'
            or any(state not in ('passed', 'reused') for name, state in states.items() if name != 'test')):
        raise ValueError('the repository failure is not limited to library tests')
    return _failed_tests(log.decode())


def _cached_schema_failure(campaign, selected, checkpoint, rejections, evidence, read_proof, gates):
    """Verify a stale checkout-bound test executable before retrying the same proposals."""
    entries = {name: read_proof(evidence.get(name)) for name in
               ('manifest', 'validation', 'repository_log', 'fixed_manifest', 'fixed_repository_log', 'fixed_tests')}
    manifest, validation, fixed = [json.loads(entries[key][1]) for key in
                                    ('manifest', 'validation', 'fixed_manifest')]
    batch_id, fixed_id = manifest['id'], fixed['id']
    for name in (batch_id, fixed_id):
        if not isinstance(name, str) or not re.fullmatch('[0-9a-f]{32}', name):
            raise ValueError('invalid cached test batch identity')
    original = campaign/'.cache/batches'/batch_id
    replacement = campaign/'.cache/batches'/fixed_id
    locations = {'manifest': original/'manifest.json', 'validation': original/'validation.json',
                 'repository_log': original/'repository-check.log',
                 'fixed_manifest': replacement/'manifest.json', 'fixed_repository_log': replacement/'repository-check.log'}
    if batch_id == fixed_id or any(entries[key][0].resolve() != path for key, path in locations.items()):
        raise ValueError('cached test failure or replacement is outside its batch')
    included = [item for item in manifest['jobs'] if item['status'] == 'included']
    ids = [item['job_id'] for item in included]
    if (len(ids) != len(selected) or {item['job_id']: item['attempt_id'] for item in included} != selected
            or manifest['status'] != 'provisional' or manifest['base_commit'] != checkpoint
            or validation.get('candidate') != manifest['commit'] or validation.get('source_checkpoint') != checkpoint
            or validation.get('included_jobs') != ids or validation.get('scope') != 'combined-batch'
            or validation.get('status') != 'failed' or validation.get('reason') != 'repository gates failed'
            or validation.get('repository_check_returncode') != 1 or 'full_checks' in validation
            or any(item.get('evidence') != {'manifest': batch_id, 'validation': validation}
                   for item in rejections.values())):
        raise ValueError('cached test failure does not cover the original cohort')
    failed_log = entries['repository_log'][1]
    tests = {'ir_schema::contract::frozen_descriptors_match_the_compiled_contract',
             'ir_schema::contract::schema_shape_is_frozen'}
    if ({name for name, status in gates(failed_log).items() if status == 'FAILED'} != {'test'}
            or set(_failed_tests(failed_log.decode())) != tests
            or failed_log.count(b'code: 2, kind: NotFound, message: "No such file or directory"') != len(tests)
            or b'error: test failed, to rerun pass `-p oracle-ir --lib`' not in failed_log
            or any(status == 'FAILED' for status in gates(entries['fixed_repository_log'][1]).values())
            or gates(entries['fixed_repository_log'][1])['test'] != 'passed'
            or any(f'test {name} ... ok\n'.encode() not in entries['fixed_tests'][1] for name in tests)
            or not _allowed(campaign, entries['fixed_tests'][0])):
        raise ValueError('cached test logs do not show exactly the missing fixtures and a successful retry')
    repo = json.loads((campaign/'config.json').read_text())['source_repo']
    trees = []
    for commit in (checkpoint, manifest['commit'], fixed['commit']):
        if not isinstance(commit, str) or not re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}', commit):
            raise ValueError('cached test proof needs immutable Git commits')
        trees.append(subprocess.check_output(['git', '-C', repo, 'rev-parse', commit+':crates/oracle-ir']))
    if len(set(trees)) != 1:
        raise ValueError('IR crate changed across the failed and passing test runs')
    candidate = original/'validation-inputs/frozen/grammar'
    if Path(validation['inputs']).resolve() != candidate.parent.parent or _grammar_tree(candidate) != manifest['grammar']:
        raise ValueError('cached test candidate grammar differs')
    _replay(campaign, checkpoint, included, list(range(len(included))), manifest['grammar'],
            original, evidence['reports'], canonical_mechanical=True)
    if _grammar_tree(candidate) != manifest['grammar'] or any(read_proof(evidence[key]) != value for key, value in entries.items()):
        raise ValueError('cached test evidence changed during replay')
    return {'manifest': batch_id, 'fixed_batch': fixed_id, 'failed_tests': sorted(tests),
            'requires_fresh_full_validation': True}


def verify(campaign, selected, checkpoint, rejections, evidence, read_proof, gates):
    if isinstance(evidence, dict) and evidence.get('kind') == 'stale-schema-test-cache-revalidation':
        return _cached_schema_failure(campaign, selected, checkpoint, rejections, evidence, read_proof, gates)
    if not isinstance(evidence, dict) or evidence.get('kind') != 'isolated-batch-library-revalidation':
        raise ValueError('isolated library batch revalidation evidence is required')
    proof_reads = []
    original_read_proof = read_proof

    def read_proof(spec):
        path, data = original_read_proof(spec)
        proof_reads.append((dict(spec), path.resolve(), hashlib.sha256(data).hexdigest()))
        return path, data

    paths, objects = {}, {}
    for name in ('manifest', 'validation', 'repository_log', 'isolation'):
        path, data = read_proof(evidence.get(name))
        paths[name] = path.resolve()
        objects[name] = data if name == 'repository_log' else json.loads(data)
    manifest, validation, gate_log, isolation = (objects[name] for name in
                                                 ('manifest', 'validation', 'repository_log', 'isolation'))
    batch_id = manifest.get('id')
    if (not isinstance(batch_id, str) or not batch_id or Path(batch_id).name != batch_id
            or paths['manifest'] != (campaign/'.cache/batches'/batch_id/'manifest.json').resolve()
            or paths['validation'] != paths['manifest'].parent/'validation.json'
            or paths['repository_log'] != paths['manifest'].parent/'repository-check.log'
            or not _allowed(campaign, paths['isolation'])):
        raise ValueError('library batch evidence paths differ')
    included = [item for item in manifest.get('jobs', []) if item.get('status') == 'included']
    ids = [item.get('job_id') for item in included]
    job_ids = list(selected)
    excluded = isolation.get('excluded_jobs')
    if (not included or any(not isinstance(name, str) or not name for name in ids)
            or len(ids) != len(set(ids)) or not set(job_ids) <= set(ids)
            or any(item.get('attempt_id') != selected[item['job_id']] for item in included
                   if item.get('job_id') in selected)
            or manifest.get('base_commit') != checkpoint
            or any(value.get('evidence', {}).get('manifest') != batch_id
                   or value.get('evidence', {}).get('validation') != validation for value in rejections.values())
            or validation.get('status') != 'failed' or validation.get('scope') != 'combined-batch'
            or validation.get('reason') != 'repository gates failed'
            or type(validation.get('repository_check_returncode')) is not int
            or validation['repository_check_returncode'] == 0
            or validation.get('candidate') != manifest.get('commit')
            or validation.get('source_checkpoint') != checkpoint
            or validation.get('included_jobs') != ids):
        raise ValueError('batch is not the matching rejected combined integration')
    if ('full_checks' in validation or 'rows' in validation or gates(gate_log).get('test') != 'FAILED'
            or any(status == 'FAILED' for name, status in gates(gate_log).items() if name != 'test')):
        raise ValueError('repository failure is not limited to library tests before the full audit')
    tests = _failed_tests(gate_log.decode('utf-8'))
    if (isolation.get('kind') != 'isolated-library-test-regression'
            or isolation.get('manifest') != manifest.get('id') or isolation.get('candidate') != manifest.get('commit')
            or isolation.get('parent') != checkpoint or isolation.get('failed_tests') != tests
            or not isinstance(excluded, list) or not excluded or len(excluded) > len(ids)
            or any(not isinstance(name, str) for name in excluded) or len(set(excluded)) != len(excluded)
            or not set(excluded) <= set(ids) or set(excluded) & set(job_ids)
            or not isinstance(isolation.get('reason'), str) or not isolation['reason'].strip()):
        raise ValueError('library isolation does not exclude the identified culprits or exact failed tests')
    cp_path = campaign/'checkpoints'/f'{checkpoint}.json'
    cp_bytes = cp_path.read_bytes()
    cp = json.loads(cp_bytes)
    inputs = Path(cp['inputs']).resolve()
    if cp.get('commit') != checkpoint or not inputs.is_relative_to(campaign.resolve()):
        raise ValueError('library isolation checkpoint inputs differ')
    frozen_manifest_path = inputs/'manifest.json'
    frozen_manifest_bytes = frozen_manifest_path.read_bytes()
    frozen = json.loads(frozen_manifest_bytes)
    base_tree = {name[len('grammar/'):]: digest for name, digest in frozen.get('files', {}).items()
                 if name.startswith('grammar/')}
    candidate = (campaign/'.cache/batches'/manifest['id']/'validation-inputs/frozen/grammar').resolve()
    candidate_manifest_path = candidate.parent.parent/'manifest.json'
    candidate_manifest_bytes = candidate_manifest_path.read_bytes()
    candidate_manifest = json.loads(candidate_manifest_bytes)
    candidate_tree = {name[len('grammar/'):]: digest for name, digest in candidate_manifest.get('files', {}).items()
                      if name.startswith('grammar/')}
    if (Path(validation.get('inputs', '')).resolve() != candidate.parent.parent
            or frozen.get('commit') != checkpoint or not base_tree or not manifest.get('grammar')
            or not isinstance(frozen.get('source'), str) or not re.fullmatch(r'[0-9a-f]{64}', frozen['source'])
            or candidate_manifest.get('commit') != manifest.get('commit')
            or candidate_manifest.get('source') != frozen['source'] or candidate_tree != manifest['grammar']
            or _grammar_tree(inputs/'frozen/grammar') != base_tree
            or _grammar_tree(candidate) != manifest['grammar']):
        raise ValueError('library isolation frozen grammar identity differs')
    controls = isolation.get('controls')
    if not isinstance(controls, dict):
        raise ValueError('library isolation controls are missing')
    expected = {'base': [], 'full': list(range(len(ids))),
                'remaining': [index for index, name in enumerate(ids) if name not in excluded]}
    results = {}
    binaries = {}
    for name in ('base', 'full', 'remaining'):
        results[name] = _control(campaign, controls.get(name), read_proof, expected[name], included,
                                 name == 'full', frozen['source'], tests, binaries)
    if results['base']['tree'] != base_tree or results['full']['tree'] != manifest['grammar']:
        raise ValueError('library isolation base or full grammar differs')
    if len(binaries) != 1:
        raise ValueError('library isolation must use the same test executable')
    for report in evidence['reports'].values():
        read_proof(report)
    _replay(campaign, checkpoint, included, expected['remaining'], results['remaining']['tree'],
            paths['isolation'].parent, evidence['reports'])
    # The replay can take minutes; reject any proof or grammar altered during it.
    for spec, old_path, old_digest in proof_reads:
        path, data = original_read_proof(spec)
        if path.resolve() != old_path or hashlib.sha256(data).hexdigest() != old_digest:
            raise ValueError('library isolation proof changed during replay')
    if (cp_path.read_bytes() != cp_bytes or frozen_manifest_path.read_bytes() != frozen_manifest_bytes
            or candidate_manifest_path.read_bytes() != candidate_manifest_bytes
            or _grammar_tree(inputs/'frozen/grammar') != base_tree
            or _grammar_tree(candidate) != manifest['grammar']):
        raise ValueError('library isolation frozen inputs changed during replay')
    for name in ('base', 'full', 'remaining'):
        root = Path(controls[name]['identity']['path']).resolve().parent
        if _grammar_tree(root/'grammar') != results[name]['tree']:
            raise ValueError('library isolation control grammar changed during replay')
    for path, digest in binaries:
        _binary(campaign, {'path': path, 'sha256': digest})
    return {'manifest': manifest['id'], 'excluded_jobs': excluded, 'failed_tests': tests,
            'requires_fresh_full_validation': True}
