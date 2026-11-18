import fcntl
import hashlib
import json
from contextlib import closing, contextmanager
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import retire_library_readmission as library
from retire_queue import Queue
import retire_queue as queue_module


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value)+'\n')


def spec(path):
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


class CachedSchemaTestReadmissionTests(unittest.TestCase):
    def test_recovery_requires_every_original_proposal_and_unchanged_ir(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bad, good = (root/'.cache/batches'/(name*32) for name in ('a', 'b'))
            parent, candidate, replacement = (name*40 for name in ('c', 'd', 'e'))
            selected = {'job': 'attempt'}
            grammar = bad/'validation-inputs/frozen/grammar'
            grammar.mkdir(parents=True)
            (grammar/'base.ebnf').write_text('candidate')
            tree = {'base.ebnf': spec(grammar/'base.ebnf')['sha256']}
            write(root/'config.json', {'source_repo': str(root)})
            write(bad/'manifest.json', {'id': bad.name, 'commit': candidate, 'status': 'provisional',
                 'base_commit': parent, 'grammar': tree,
                 'jobs': [{'status': 'included', 'job_id': 'job', 'attempt_id': 'attempt'}]})
            validation = {'status': 'failed', 'scope': 'combined-batch', 'reason': 'repository gates failed',
                          'repository_check_returncode': 1, 'candidate': candidate, 'source_checkpoint': parent,
                          'included_jobs': ['job'], 'inputs': str(grammar.parent.parent)}
            write(bad/'validation.json', validation)
            write(good/'manifest.json', {'id': good.name, 'commit': replacement})
            tests = ('ir_schema::contract::frozen_descriptors_match_the_compiled_contract',
                     'ir_schema::contract::schema_shape_is_frozen')
            pass_log = ''.join(f'{name} passed 1.0 s\n' for name in sorted(queue_module._GATE_NAMES))
            failed_log = (pass_log.replace('test passed', 'test FAILED')
                          + 'code: 2, kind: NotFound, message: "No such file or directory"\n'*2
                          + 'failures:\n' + ''.join(f'    {test}\n' for test in tests)
                          + '\ntest result: FAILED. 3 passed; 2 failed; 2 ignored;\n'
                          + 'error: test failed, to rerun pass `-p oracle-ir --lib`\n')
            (bad/'repository-check.log').write_text(failed_log)
            (good/'repository-check.log').write_text(pass_log)
            fixed_tests = root/'.cache/test.log'
            fixed_tests.write_text(''.join(f'test {test} ... ok\n' for test in tests))
            proof = {'kind': 'stale-schema-test-cache-revalidation', 'reports': {},
                     'manifest': spec(bad/'manifest.json'), 'validation': spec(bad/'validation.json'),
                     'repository_log': spec(bad/'repository-check.log'),
                     'fixed_manifest': spec(good/'manifest.json'),
                     'fixed_repository_log': spec(good/'repository-check.log'), 'fixed_tests': spec(fixed_tests)}
            rejections = {'job': {'evidence': {'manifest': bad.name, 'validation': validation}}}
            with patch.object(library, '_replay') as replay, \
                 patch.object(library.subprocess, 'check_output', return_value=b'same-tree') as git:
                def verify(selection):
                    return library.verify(root, selection, parent, rejections, proof,
                                          queue_module._proof_file, queue_module._gates)
                self.assertTrue(verify(selected)['requires_fresh_full_validation'])
                replay.assert_called_once()
                with self.assertRaisesRegex(ValueError, 'original cohort'):
                    verify({})
                git.side_effect = [b'base-tree', b'base-tree', b'changed-tree']
                with self.assertRaisesRegex(ValueError, 'IR crate changed'):
                    verify(selected)
                replay.assert_called_once()


@contextmanager
def mutation_during_readmission_verification(queue, verifier_name, attempt, field, value):
    """Run the real verifier, then edit its held transaction and require rollback."""
    verify, transaction, held = getattr(queue_module, verifier_name), queue._transaction, []
    query = {'job_id': 'UPDATE attempts SET job_id=? WHERE id=?',
             'worker': 'UPDATE attempts SET worker=? WHERE id=?'}[field]
    @contextmanager
    def capture():
        with transaction() as db:
            held.append(db)
            try: yield db
            finally: held.pop()
    def mutate(*args, **kwargs):
        result = verify(*args, **kwargs)
        held[-1].execute(query, (value, attempt))
        return result
    with patch.object(queue, '_transaction', side_effect=capture), \
            patch.object(queue_module, verifier_name, side_effect=mutate):
        yield


class BatchReadmissionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.queue = Queue(self.root/'queue.sqlite')
        self.queue.add_checkpoint('parent', 'proof')
        self.queue.add_jobs([{'id': 'author', 'kind': 'author', 'rules': ['a']},
                             {'id': 'mechanical', 'kind': 'mechanical', 'rules': ['b']},
                             {'id': 'culprit', 'kind': 'author', 'rules': ['c']}])
        self.items, self.reports = [], {}
        for job in ('author', 'mechanical', 'culprit'):
            kind = 'mechanical' if job == 'mechanical' else 'author'
            claim = self.queue.claim('worker-'+job, kinds=[kind])
            self.assertEqual(claim['job']['id'], job)
            attempt = claim['attempt_id']
            child = subprocess.Popen(['true'])
            child.wait()
            self.queue.mark_started(attempt, pid=child.pid)
            lease = self.root/'leases'/job
            lease.parent.mkdir(exist_ok=True)
            lease.touch()
            run = self.root/'.cache/attempts'/attempt
            directory = run/'evaluations'/job/'attempt-1'
            directory.mkdir(parents=True)
            descriptor = run/'tasks'/f'{job}.json'
            write(descriptor, {'id': job, 'rules': claim['job']['rules']})
            support = run/'support.json'
            write(support, {'tasks': {job: spec(descriptor)['sha256']}})
            grammar = directory/'grammar'
            grammar.mkdir()
            (grammar/'test.ebnf').write_text('test ::= "a"')
            family = directory/'family.json'
            write(family, {'family': 'test', 'cases': []})
            report = directory/'report.json'
            data = {'task': job, 'source': 'parent', 'directory': str(directory), 'passed': True,
                    'failures': [], 'faces': 1, 'descriptor_sha256': spec(descriptor)['sha256'],
                    'support_sha256': spec(support)['sha256'], 'grammar': {'test.ebnf': spec(grammar/'test.ebnf')['sha256']}}
            if job == 'mechanical':
                data.update(kind='mechanical', hooks=[])
            else:
                data.update(fixture_sha256=spec(family)['sha256'], family_returncode=0)
            write(report, data)
            self.reports[job] = report
            rules = claim['job']['rules']
            decision = {rule: {'status': 'proposed', 'reason': 'reviewed', 'evidence': 'proof'} for rule in rules}
            if job != 'mechanical':
                for rule in rules:
                    decision[rule]['rule'] = rule
            self.queue.finish(attempt, 'provisional-pass', {'kind': claim['job']['kind'], 'report': str(report),
                              'dispositions': decision}, {rule: {'status': 'pass', 'evidence': 'proof'} for rule in rules})
            self.items.append({'job_id': job, 'attempt_id': attempt, 'status': 'included',
                               'base_commit': 'parent', 'rules': rules, 'kind': claim['job']['kind'],
                               'descriptor_sha256': spec(descriptor)['sha256'], 'support_sha256': spec(support)['sha256']})
        folder = self.root/'.cache/batches/batch'
        folder.mkdir(parents=True)
        manifest = folder/'manifest.json'
        write(manifest, {'id': 'batch', 'base_commit': 'parent', 'commit': 'candidate', 'jobs': self.items})
        validation = {'status': 'failed', 'scope': 'combined-batch', 'reason': 'repository gates failed',
                      'repository_check_returncode': 1, 'candidate': 'candidate', 'source_checkpoint': 'parent',
                      'included_jobs': [item['job_id'] for item in self.items]}
        self.validation = folder/'validation.json'
        write(self.validation, validation)
        self.queue.reject_jobs([item['job_id'] for item in self.items], {'manifest': 'batch', 'validation': validation})
        self.log = folder/'repository-check.log'
        self.log.write_text(''.join(f'{name} {"FAILED" if name == "families" else "passed"} 1.0 s\n'
                                    for name in ('fmt', 'pin', 'hygiene', 'tools', 'clippy', 'build', 'families', 'test')))
        logs = {}
        for name in ('base_log', 'trial_log', 'without_modifier_log', 'probe_log'):
            path = self.root/'.cache'/f'{name}.log'
            path.write_text('recorded evidence')
            logs[name] = spec(path)
        self.isolation = self.root/'.cache/isolation.json'
        write(self.isolation, {'kind': 'isolated-ambiguity-regression', 'manifest': 'batch',
                               'candidate': 'candidate', 'parent': 'parent', 'culprit_job': 'culprit',
                               'family': 'shared-clauses', 'case': 'entry control', 'reason': 'isolated regression',
                               'evidence': logs})
        self.proof = {'kind': 'isolated-batch-family-revalidation', 'manifest': spec(manifest),
                      'validation': spec(self.validation), 'repository_log': spec(self.log),
                      'isolation': spec(self.isolation)}

    def attempt(self, name):
        return next(item['attempt_id'] for item in self.items if item['job_id'] == name)

    def readmit(self, name):
        proof = {**self.proof, 'report': spec(self.reports[name])}
        self.queue.readmit_batch_proposal(name, self.attempt(name), 'parent', proof)

    def test_author_and_mechanical_are_only_provisional_and_preserve_history(self):
        before = {name: self.queue.get_attempt(self.attempt(name)) for name in ('author', 'mechanical')}
        self.readmit('author')
        self.readmit('mechanical')
        coverage = self.queue.coverage()
        self.assertEqual(coverage['counts'], {'provisional-pass': 2, 'integration-rejected': 1})
        self.assertEqual(coverage['accepted_rules'], 0)
        for name, old in before.items():
            self.assertEqual(self.queue.get_attempt(self.attempt(name)), old)
        event = self.queue.audit()[-1]['data']
        self.assertTrue(event['proof']['requires_fresh_full_validation'])
        self.assertEqual(event['previous_adjudication']['status'], 'integration-rejected')
        with self.assertRaisesRegex(ValueError, 'culprit'):
            self.readmit('culprit')
        previous = json.loads((self.root/'.cache/batches/batch/validation.json').read_text())
        self.queue.reject_jobs(['author'], {'manifest': 'batch', 'validation': previous})
        with self.assertRaisesRegex(ValueError, 'already used'):
            self.readmit('author')

    def test_missing_dispositions_or_tampered_individual_report_are_rejected(self):
        attempt = self.attempt('author')
        with closing(self.queue._connect()) as db:
            raw = db.execute('SELECT result FROM attempts WHERE id=?', (attempt,)).fetchone()[0]
            value = json.loads(raw)
            del value['evidence']['dispositions']['a']
            db.execute('UPDATE attempts SET result=? WHERE id=?', (json.dumps(value), attempt))
        with self.assertRaisesRegex(ValueError, 'per-rule dispositions'):
            self.readmit('author')
        with closing(self.queue._connect()) as db:
            db.execute('UPDATE attempts SET result=? WHERE id=?', (raw, attempt))
        self.reports['author'].write_text('{}')
        with self.assertRaisesRegex(ValueError, 'did not pass'):
            self.readmit('author')
        self.assertEqual(self.queue.coverage()['accepted_rules'], 0)

    def test_failure_identity_and_process_guards(self):
        with (self.root/'leases/author').open('w') as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, 'still held'):
                self.readmit('author')
        with patch('retire_queue.os.kill', return_value=None):
            with self.assertRaisesRegex(ValueError, 'still present'):
                self.readmit('author')
        with self.assertRaisesRegex(ValueError, 'stale'):
            self.queue.readmit_batch_proposal('author', self.attempt('author'), 'wrong', self.proof)
        self.log.write_text(self.log.read_text().replace('tools passed', 'tools FAILED'))
        self.proof['repository_log'] = spec(self.log)
        with self.assertRaisesRegex(ValueError, 'not limited to families'):
            self.readmit('author')
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})


    def test_batch_readmission_refuses_initial_foreign_attempt_before_verification(self):
        attempt = self.attempt('author'); events = self.queue.audit()
        with closing(self.queue._connect()) as db:
            db.execute("UPDATE attempts SET job_id='culprit' WHERE id=?", (attempt,))
        with patch.object(queue_module, '_verify_batch_readmission') as verify, \
                self.assertRaisesRegex(ValueError, 'current rejected attempt'):
            self.readmit('author')
        verify.assert_not_called()
        self.assertEqual(self.queue.audit(), events)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})

    def test_batch_verifier_identity_mutations_roll_back_without_readmission(self):
        attempt = self.attempt('author'); original = self.queue.get_attempt(attempt)
        events = self.queue.audit()
        for field, value in (('job_id', 'culprit'), ('worker', 'changed-worker')):
            with self.subTest(field=field), \
                    mutation_during_readmission_verification(self.queue, '_verify_batch_readmission', attempt, field, value), \
                    self.assertRaisesRegex(ValueError, 'attempt changed'):
                self.readmit('author')
            self.assertEqual(self.queue.get_attempt(attempt), original)
            self.assertEqual(self.queue.audit(), events)
            self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})

    def native_fixture(self):
        folder = self.root/'.cache/batches/batch'
        inputs = self.root/'checkpoints/parent/inputs'
        base_grammar = inputs/'frozen/grammar'
        base_grammar.mkdir(parents=True)
        (base_grammar/'test.ebnf').write_text('base grammar')
        base_hash = spec(base_grammar/'test.ebnf')['sha256']
        write(inputs/'manifest.json', {'commit': 'parent', 'population': 1,
                                      'files': {'grammar/test.ebnf': base_hash}})
        write(self.root/'checkpoints/parent.json', {'commit': 'parent', 'inputs': str(inputs)})
        manifest = folder/'manifest.json'
        data = json.loads(manifest.read_text())
        candidate_bytes = b'candidate grammar'
        data['grammar'] = {'test.ebnf': hashlib.sha256(candidate_bytes).hexdigest()}
        write(manifest, data)
        validation = {'status': 'failed', 'scope': 'combined-batch', 'reason': 'full allowed-population checks failed',
                      'repository_check_returncode': 0, 'candidate': 'candidate', 'source_checkpoint': 'parent',
                      'full_checks': {'faces': 1, 'failures': [{'face': 'N', 'lost': ['exact_default']}]},
                      'included_jobs': [item['job_id'] for item in self.items]}
        write(self.validation, validation)
        with closing(self.queue._connect()) as db:
            for item in self.items:
                value = {'status': 'integration-rejected', 'checkpoint': None,
                         'evidence': {'manifest': 'batch', 'validation': validation}}
                db.execute('UPDATE jobs SET adjudication=? WHERE id=?', (json.dumps(value), item['job_id']))
        self.log.write_text(''.join(f'{name} passed 1.0 s\n'
                                    for name in ('fmt', 'pin', 'hygiene', 'tools', 'clippy', 'build', 'families', 'test')))
        controls = {}
        for name, content, indices, failures in (
                ('base', b'base grammar', [], []),
                ('full', candidate_bytes, [0, 1, 2], [{'face': 'N', 'lost': ['exact_default']}]),
                ('remaining', b'remaining grammar', [0, 1], [])):
            root = self.root/'.cache'/('native-'+name)
            (root/'grammar').mkdir(parents=True)
            (root/'grammar/test.ebnf').write_bytes(content)
            result = root/'result.json'
            identity = root/'identity.json'
            population = root/'checks/slice.json'
            write(result, {'faces': 1, 'failures': failures})
            write(identity, {'indices': indices, 'tree': {'test.ebnf': spec(root/'grammar/test.ebnf')['sha256']}})
            write(population, {'cards': ['N']})
            controls[name] = {'result': spec(result), 'identity': spec(identity), 'population': spec(population)}
        isolation = self.root/'.cache/native-isolation.json'
        write(isolation, {'kind': 'isolated-native-regressions', 'manifest': 'batch', 'candidate': 'candidate',
                          'parent': 'parent', 'excluded_jobs': ['culprit'], 'population_count': 1,
                          'controls': controls, 'reason': 'focused full failed and remaining passed'})
        return {'kind': 'isolated-batch-native-revalidation', 'manifest': spec(manifest),
                'validation': spec(self.validation), 'repository_log': spec(self.log), 'isolation': spec(isolation)}

    def library_fixture(self):
        import retire_library_readmission as library
        folder = self.root/'.cache/batches/batch'
        inputs = self.root/'checkpoints/parent/inputs'
        base = inputs/'frozen/grammar'
        base.mkdir(parents=True)
        (base/'test.ebnf').write_bytes(b'base grammar')
        write(inputs/'manifest.json', {'commit': 'parent', 'source': 'a'*64,
                                      'files': {'grammar/test.ebnf': spec(base/'test.ebnf')['sha256']}})
        write(self.root/'checkpoints/parent.json', {'commit': 'parent', 'inputs': str(inputs),
                                                   'cpu_pool': str(self.root/'cpu'), 'cpu_capacity': 32})
        manifest = folder/'manifest.json'
        data = json.loads(manifest.read_text())
        data['grammar'] = {'test.ebnf': hashlib.sha256(b'candidate grammar').hexdigest()}
        write(manifest, data)
        validation_inputs = folder/'validation-inputs'
        (validation_inputs/'frozen/grammar').mkdir(parents=True)
        (validation_inputs/'frozen/grammar/test.ebnf').write_bytes(b'candidate grammar')
        write(validation_inputs/'manifest.json', {'commit': 'candidate', 'source': 'a'*64,
                                                  'files': {'grammar/test.ebnf': data['grammar']['test.ebnf']}})
        validation = {'status': 'failed', 'scope': 'combined-batch', 'reason': 'repository gates failed',
                      'repository_check_returncode': 1, 'candidate': 'candidate', 'source_checkpoint': 'parent',
                      'inputs': str(validation_inputs), 'included_jobs': [item['job_id'] for item in self.items]}
        write(self.validation, validation)
        with closing(self.queue._connect()) as db:
            for item in self.items:
                value = {'status': 'integration-rejected', 'checkpoint': None,
                         'evidence': {'manifest': 'batch', 'validation': validation}}
                db.execute('UPDATE jobs SET adjudication=? WHERE id=?', (json.dumps(value), item['job_id']))
        tests = ['bind::member_focus::tests::end_to_end_native_members_and_both_paid_and_ordinary_goblin_bomb_sources', 'tests::target_bans::a_prohibition_ends_this_turn_after_its_verb']
        self.log.write_text(''.join(f'{name} {"FAILED" if name == "test" else "passed"} 1.0 s\n'
                                    for name in ('fmt', 'pin', 'hygiene', 'tools', 'clippy', 'build', 'families', 'test'))
                            +'\nfailures:\n'+''.join('    '+test+'\n' for test in tests)+'\ntest result: FAILED. 995 passed; 2 failed; 0 ignored;\n')
        binary = self.root/'.cache/test-binary'
        binary.write_bytes(b'compiled test executable')
        controls = {}
        for name, content, indices, returncode in (
                ('base', b'base grammar', [], 0), ('full', b'candidate grammar', [0, 1, 2], 101),
                ('remaining', b'replayed grammar', [0, 1], 0)):
            root = self.root/'.cache'/('library-'+name)
            (root/'grammar').mkdir(parents=True)
            (root/'grammar/test.ebnf').write_bytes(content)
            identity = root/'identity.json'
            write(identity, {'method': 'ordered-proposal-replay-v1', 'indices': indices,
                             'proposals': [{'job_id': self.items[i]['job_id'], 'attempt_id': self.items[i]['attempt_id']}
                                           for i in indices],
                             'tree': {'test.ebnf': spec(root/'grammar/test.ebnf')['sha256']}})
            trial_tests = []
            for index, test in enumerate(tests):
                result = root/f'test-{index:03d}-result.json'
                write(result, {'test': test, 'source_sha256': 'a'*64, 'returncode': returncode,
                               'binary': spec(binary), 'command': [str(binary), '--exact', test, '--nocapture'],
                               'environment': {'ORACLE_GRAMMAR_DIR': str(root/'grammar')}})
                log = root/f'test-{index:03d}.log'
                status, summary = ('FAILED', 'FAILED. 0 passed; 1 failed;') if returncode else ('ok', 'ok. 1 passed; 0 failed;')
                log.write_text(f'running 1 test\ntest {test} ... {status}\ntest result: {summary}\n')
                trial_tests.append({'test': test, 'result': spec(result), 'log': spec(log)})
            controls[name] = {'identity': spec(identity), 'tests': trial_tests}
        isolation = self.root/'.cache/library-isolation.json'
        write(isolation, {'kind': 'isolated-library-test-regression', 'manifest': 'batch', 'candidate': 'candidate',
                          'parent': 'parent', 'excluded_jobs': ['culprit'], 'failed_tests': tests, 'controls': controls, 'reason': 'verified replay'})
        return {'kind': 'isolated-batch-library-revalidation', 'manifest': spec(manifest),
                'validation': spec(self.validation), 'repository_log': spec(self.log), 'isolation': spec(isolation),
                'reports': {name: spec(self.reports[name]) for name in ('author', 'mechanical')}}

    def test_native_readmission_author_and_mechanical_require_fresh_audit(self):
        proof = self.native_fixture()
        for job in ('author', 'mechanical'):
            self.queue.readmit_native_batch_proposal(job, self.attempt(job), 'parent',
                                                     {**proof, 'report': spec(self.reports[job])})
        self.assertEqual(self.queue.coverage()['counts'], {'provisional-pass': 2, 'integration-rejected': 1})
        self.assertEqual(self.queue.coverage()['accepted_rules'], 0)
        event = self.queue.audit()[-1]['data']
        self.assertTrue(event['proof']['requires_fresh_full_validation'])
        self.assertEqual(event['proof']['excluded_jobs'], ['culprit'])
        with self.assertRaisesRegex(ValueError, 'exclude'):
            self.queue.readmit_native_batch_proposal('culprit', self.attempt('culprit'), 'parent',
                                                     {**proof, 'report': spec(self.reports['culprit'])})
        validation = json.loads(self.validation.read_text())
        self.queue.reject_jobs(['author'], {'manifest': 'batch', 'validation': validation})
        with self.assertRaisesRegex(ValueError, 'already used'):
            self.queue.readmit_native_batch_proposal('author', self.attempt('author'), 'parent',
                                                     {**proof, 'report': spec(self.reports['author'])})

    def test_library_batch_readmission_is_atomic_and_requires_fresh_validation(self):
        proof = self.library_fixture()
        with patch('retire_library_readmission._replay') as replay:
            result = self.queue.readmit_library_batch_proposals(['mechanical', 'author'], 'parent', proof)
        self.assertEqual(result['jobs'], ['mechanical', 'author'])
        self.assertTrue(result['proof']['requires_fresh_full_validation'])
        self.assertEqual(replay.call_count, 1)
        self.assertEqual(self.queue.coverage()['counts'], {'provisional-pass': 2, 'integration-rejected': 1})
        self.assertEqual(self.queue.coverage()['accepted_rules'], 0)
        validation = json.loads(self.validation.read_text())
        self.queue.reject_jobs(['author', 'mechanical'], {'manifest': 'batch', 'validation': validation})
        with patch('retire_library_readmission._replay'):
            with self.assertRaisesRegex(ValueError, 'already used'):
                self.queue.readmit_library_batch_proposals(['author', 'mechanical'], 'parent', proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})

    def test_library_batch_rejects_invalid_selection_and_held_lease_without_partial_mutation(self):
        proof = self.library_fixture()
        for jobs in ([], ['author', 'author'], ['author', 'missing'], ['author']*97):
            with self.subTest(jobs=jobs[:2]), self.assertRaises(ValueError):
                self.queue.readmit_library_batch_proposals(jobs, 'parent', proof)
        with (self.root/'leases/mechanical').open('w') as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, 'still held'):
                self.queue.readmit_library_batch_proposals(['author', 'mechanical'], 'parent', proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})

    def test_library_replay_of_a_verified_ancestor_is_still_provisional(self):
        proof = {**self.library_fixture(), 'source_checkpoint':'parent'}
        self.queue.add_checkpoint('later', {'source':'test'}, expected_parent='parent')
        with patch('retire_library_readmission._replay') as replay:
            result = self.queue.readmit_library_batch_proposals(['author', 'mechanical'], 'later', proof)
        self.assertEqual(replay.call_args.args[1], 'parent')
        self.assertEqual(result['checkpoint'], 'later')
        self.assertEqual(result['proof']['source_checkpoint'], 'parent')
        self.assertEqual(self.queue.coverage()['accepted_rules'], 0)

    def test_library_batch_rejects_partial_or_tampered_proof(self):
        proof = self.library_fixture()
        isolation_path = self.root/'.cache/library-isolation.json'
        isolation = json.loads(isolation_path.read_text())
        mutations = [
            lambda value: value.update(excluded_jobs=['author']),
            lambda value: value.update(failed_tests=value['failed_tests'][:1]),
            lambda value: value.update(excluded_jobs=['culprit', 'culprit']),
            lambda value: value['controls']['full'].update(identity=value['controls']['base']['identity']),
            lambda value: value['controls']['remaining']['tests'][1].update(result=value['controls']['full']['tests'][1]['result']),
        ]
        for change in mutations:
            current = json.loads(json.dumps(isolation))
            change(current)
            write(isolation_path, current)
            proof['isolation'] = spec(isolation_path)
            with self.subTest(change=change), self.assertRaises(ValueError), patch('retire_library_readmission._replay'):
                self.queue.readmit_library_batch_proposals(['author', 'mechanical'], 'parent', proof)
        write(isolation_path, isolation)
        proof['isolation'] = spec(isolation_path)
        self.log.write_text(self.log.read_text().replace('families passed', 'families FAILED'))
        proof['repository_log'] = spec(self.log)
        with self.assertRaisesRegex(ValueError, 'not limited'):
            self.queue.readmit_library_batch_proposals(['author', 'mechanical'], 'parent', proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})

    def test_library_batch_two_exclusions_replay_once(self):
        proof = self.library_fixture()
        isolation_path = self.root/'.cache/library-isolation.json'
        isolation = json.loads(isolation_path.read_text())
        isolation['excluded_jobs'] = ['mechanical', 'culprit']
        identity_path = self.root/'.cache/library-remaining/identity.json'
        identity = json.loads(identity_path.read_text())
        identity['indices'] = [0]
        identity['proposals'] = identity['proposals'][:1]
        write(identity_path, identity)
        isolation['controls']['remaining']['identity'] = spec(identity_path)
        write(isolation_path, isolation)
        proof['isolation'] = spec(isolation_path)
        proof['reports'] = {'author': proof['reports']['author']}
        with patch('retire_library_readmission._replay') as replay:
            result = self.queue.readmit_library_batch_proposals(['author'], 'parent', proof)
        self.assertEqual(replay.call_count, 1)
        self.assertEqual(replay.call_args.args[3], [0])
        self.assertEqual(result['proof']['excluded_jobs'], ['mechanical', 'culprit'])
        self.assertEqual(self.queue.coverage()['counts'], {'provisional-pass': 1, 'integration-rejected': 2})

    def test_library_failure_set_requires_complete_bounded_summary(self):
        import retire_library_readmission as library
        name = 'bind::member_focus::tests::end_to_end_native_members_and_both_paid_and_ordinary_goblin_bomb_sources'
        good = f'failures:\n    {name}\n\ntest result: FAILED. 1 passed; 1 failed;\n'
        self.assertEqual(library._failed_tests(good), [name])
        for log in (good.replace('1 failed;', '2 failed;'), good.replace(name, 'bad name'),
                    good.replace('FAILED.', 'ok.'), good.replace('    '+name, '    '+name+'\n    '+name)):
            with self.subTest(log=log), self.assertRaises(ValueError):
                library._failed_tests(log)

    def test_library_batch_rechecks_artifacts_after_replay(self):
        proof = self.library_fixture()
        log = self.root/'.cache/library-remaining/test-001.log'
        def tamper(*args):
            log.write_text('changed during replay')
        with patch('retire_library_readmission._replay', side_effect=tamper):
            with self.assertRaises(ValueError):
                self.queue.readmit_library_batch_proposals(['author', 'mechanical'], 'parent', proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})

    def test_library_batch_rechecks_checkpoint_before_atomic_update(self):
        proof = self.library_fixture()
        def advance(*args):
            self.queue.add_checkpoint('next', 'proof', expected_parent='parent')
        with patch('retire_library_readmission._replay', side_effect=advance):
            with self.assertRaisesRegex(ValueError, 'checkpoint changed'):
                self.queue.readmit_library_batch_proposals(['author', 'mechanical'], 'parent', proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})

    def test_library_replay_reconstructs_ordered_remaining_grammar(self):
        import retire_library_readmission as library
        import retire_queue
        import retire_batch
        import retire_merge
        import mined
        import retire_author
        proof = self.library_fixture()
        for item in self.items:
            if item['kind'] == 'author':
                item['fixture_sha256'] = json.loads(self.reports[item['job_id']].read_text())['fixture_sha256']
        with closing(self.queue._connect()) as db:
            for item in self.items:
                row = db.execute('SELECT result FROM attempts WHERE id=?', (item['attempt_id'],)).fetchone()
                result = json.loads(row[0])
                result['evidence']['run'] = str(self.root/'.cache/attempts'/item['attempt_id'])
                db.execute('UPDATE attempts SET result=? WHERE id=?', (json.dumps(result), item['attempt_id']))
        def proposed(value, *, input_cache=None):
            report = value['report']
            name = report['task']
            item = next(row for row in self.items if row['job_id'] == name)
            return self.root, {'id': name, 'rules': item['rules']}, self.root, []
        def report_author(campaign, job, attempt, base, original, report_spec):
            return json.loads(self.reports[job].read_text()), {}
        def report_mechanical(campaign, job, attempt, base, original, report_spec):
            return json.loads(self.reports[job].read_text())
        calls = []
        def merge(base, current, proposed_dir, rules, hooks):
            calls.append(rules[0])
            return {'grammar/test.ebnf': 'replayed grammar'} if rules == ['a'] else {}
        expected = {'test.ebnf': hashlib.sha256(b'replayed grammar').hexdigest()}
        with patch.object(retire_batch, '_proposal', side_effect=proposed), \
                patch.object(retire_queue, '_verify_original_report', side_effect=report_author), \
                patch.object(retire_queue, '_verify_mechanical_readmission', side_effect=report_mechanical), \
                patch.object(retire_merge, 'merge_grammar', side_effect=merge), \
                patch.object(mined, 'remove', side_effect=lambda root, rules: (rules, {})), \
                patch.object(retire_author, 'apply_hooks'):
            library._replay(self.root, 'parent', self.items, [0, 1], expected, self.root/'.cache', proof['reports'])
            self.assertEqual(calls, ['a', 'b'])
            with self.assertRaisesRegex(ValueError, 'ordered replay differs'):
                library._replay(self.root, 'parent', self.items, [0, 1], {'test.ebnf': 'bad'},
                                self.root/'.cache', proof['reports'])

    def test_native_readmission_refuses_initial_foreign_attempt_before_verification(self):
        proof = self.native_fixture(); attempt = self.attempt('author'); events = self.queue.audit()
        with closing(self.queue._connect()) as db:
            db.execute("UPDATE attempts SET job_id='culprit' WHERE id=?", (attempt,))
        with patch.object(queue_module.retire_native_readmission, 'verify') as verify, \
                self.assertRaisesRegex(ValueError, 'current rejected attempt'):
            self.queue.readmit_native_batch_proposal('author', attempt, 'parent',
                                                     {**proof, 'report': spec(self.reports['author'])})
        verify.assert_not_called(); self.assertEqual(self.queue.audit(), events)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})

    def test_native_verifier_identity_races_refuse_without_readmission(self):
        proof = self.native_fixture(); verify = queue_module.retire_native_readmission.verify
        for job in ('author', 'mechanical'):
            attempt = self.attempt(job); original = self.queue.get_attempt(attempt)
            for field, value in (('job_id', 'culprit'), ('worker', 'changed-worker')):
                query = {'job_id': 'UPDATE attempts SET job_id=? WHERE id=?',
                         'worker': 'UPDATE attempts SET worker=? WHERE id=?'}[field]
                events = self.queue.audit(); jobs = self.queue.get_jobs('integration-rejected')
                def mutate(*args):
                    result = verify(*args)
                    with closing(self.queue._connect()) as db: db.execute(query, (value, attempt))
                    return result
                with self.subTest(job=job, field=field), \
                        patch.object(queue_module.retire_native_readmission, 'verify', side_effect=mutate), \
                        self.assertRaisesRegex(ValueError, 'changed during native proof'):
                    self.queue.readmit_native_batch_proposal(job, attempt, 'parent',
                                                             {**proof, 'report': spec(self.reports[job])})
                self.assertEqual(self.queue.audit(), events)
                self.assertEqual(self.queue.get_jobs('integration-rejected'), jobs)
                self.assertEqual(self.queue.get_attempt(attempt)[field], value)
                with closing(self.queue._connect()) as db: db.execute(query, (original[field], attempt))
                self.assertEqual(self.queue.get_attempt(attempt), original)

    def test_native_readmission_rechecks_checkpoint_after_expensive_proof(self):
        import retire_native_readmission
        proof = self.native_fixture()
        proof['report'] = spec(self.reports['author'])
        verify = retire_native_readmission.verify
        def advance(*args):
            result = verify(*args)
            self.queue.add_checkpoint('next', 'proof', expected_parent='parent')
            return result
        with patch.object(retire_native_readmission, 'verify', side_effect=advance):
            with self.assertRaisesRegex(ValueError, 'changed during'):
                self.queue.readmit_native_batch_proposal('author', self.attempt('author'), 'parent', proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})

    def test_native_isolation_requires_all_control_outcomes_and_exact_complement(self):
        proof = self.native_fixture()
        proof['report'] = spec(self.reports['author'])
        path = self.root/'.cache/native-isolation.json'
        isolation = json.loads(path.read_text())
        remaining = self.root/'.cache/native-remaining/result.json'
        write(remaining, {'faces': 1, 'failures': [{'face': 'N'}]})
        isolation['controls']['remaining']['result'] = spec(remaining)
        write(path, isolation)
        proof['isolation'] = spec(path)
        with self.assertRaisesRegex(ValueError, 'outcome: remaining'):
            self.queue.readmit_native_batch_proposal('author', self.attempt('author'), 'parent', proof)
        write(remaining, {'faces': 1, 'failures': []})
        isolation['controls']['remaining']['result'] = spec(remaining)
        identity = self.root/'.cache/native-remaining/identity.json'
        data = json.loads(identity.read_text())
        data['indices'] = [0]
        write(identity, data)
        isolation['controls']['remaining']['identity'] = spec(identity)
        write(path, isolation)
        proof['isolation'] = spec(path)
        with self.assertRaisesRegex(ValueError, 'outcome: remaining'):
            self.queue.readmit_native_batch_proposal('author', self.attempt('author'), 'parent', proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})

    def test_native_readmission_rejects_failed_repository_gate(self):
        proof = self.native_fixture()
        proof['report'] = spec(self.reports['author'])
        self.log.write_text(self.log.read_text().replace('tools passed', 'tools FAILED'))
        proof['repository_log'] = spec(self.log)
        with self.assertRaisesRegex(ValueError, 'repository gates did not all pass'):
            self.queue.readmit_native_batch_proposal('author', self.attempt('author'), 'parent', proof)
        self.assertEqual(self.queue.coverage()['accepted_rules'], 0)

    def test_native_readmission_requires_pass_controls_intact_hashes_and_dead_owner(self):
        proof = self.native_fixture()
        proof['report'] = spec(self.reports['author'])
        with (self.root/'leases/author').open('w') as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, 'still held'):
                self.queue.readmit_native_batch_proposal('author', self.attempt('author'), 'parent', proof)
        with patch('retire_queue.os.kill', return_value=None):
            with self.assertRaisesRegex(ValueError, 'still present'):
                self.queue.readmit_native_batch_proposal('author', self.attempt('author'), 'parent', proof)
        with self.assertRaisesRegex(ValueError, 'stale'):
            self.queue.readmit_native_batch_proposal('author', self.attempt('author'), 'old', proof)
        (self.root/'.cache/native-remaining/grammar/test.ebnf').write_text('tampered')
        with self.assertRaisesRegex(ValueError, 'grammar hash differs'):
            self.queue.readmit_native_batch_proposal('author', self.attempt('author'), 'parent', proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})



if __name__ == '__main__':
    unittest.main()
