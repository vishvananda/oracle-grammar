"""Only complete and bounded rejection groups qualify for recovery."""
import json
from pathlib import Path
import sqlite3
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch

import retire as r
from retire_library_readmission import LIBRARY_REJECTION
from retire_parse_readmission import PARSE_SCREEN_REJECTION, PRINT_ONLY_REJECTION, awaiting_print_recovery
import retire_recover_full_print as recover


class PendingFullPrintTests(unittest.TestCase):
    def setUp(self):
        self.temporary = TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.campaign = Path(self.temporary.name)
        self.output = self.campaign/'.cache/output'
        self.output.mkdir(parents=True)
        self.batch = '0123456789abcdef'*2
        self.folder = self.campaign/'.cache/batches'/self.batch
        self.folder.mkdir(parents=True)
        self.manifest = {'id': self.batch, 'jobs': [
            {'job_id': 'a', 'attempt_id': 'old-a', 'status': 'included'},
            {'job_id': 'b', 'attempt_id': 'old-b', 'status': 'included'},
            {'job_id': 'c', 'attempt_id': 'old-c', 'status': 'excluded'},
        ]}
        self.validation = {'reason': PRINT_ONLY_REJECTION, 'scope': 'combined-batch', 'status': 'failed',
                           'full_checks': {'failures': [{'face': 'One', 'lost': ['exact_default']}]}}
        r.write(self.folder/'manifest.json', self.manifest)
        r.write(self.folder/'validation.json', self.validation)
        self.db = sqlite3.connect(self.campaign/'queue.sqlite')
        self.addCleanup(self.db.close)
        self.db.execute('CREATE TABLE jobs (id, attempt_id, adjudication, status)')
        for job in ('a', 'b'):
            self.db.execute('INSERT INTO jobs VALUES (?, ?, ?, ?)', (job, 'old-'+job,
                json.dumps({'evidence': {'manifest': self.batch, 'validation': self.validation}}),
                'integration-rejected'))
        self.db.commit()

    def pending(self):
        return [batch for batch, _ in recover.pending_failed_batches(self.campaign, self.output)]

    def test_complete_current_group_can_be_recovered_once(self):
        self.assertEqual(self.pending(), [self.batch])
        (self.output/self.batch).mkdir()
        self.assertEqual(self.pending(), [])

    def test_readmitted_or_changed_attempt_is_not_replayed(self):
        self.db.execute("UPDATE jobs SET attempt_id='new-b' WHERE id='b'")
        self.db.commit()
        self.assertEqual(self.pending(), [])
        self.db.execute("UPDATE jobs SET attempt_id='old-b', status='provisional-pass' WHERE id='b'")
        self.db.commit()
        self.assertEqual(self.pending(), [])

    def test_managed_watcher_holds_completion_until_terminal_recovery(self):
        r.write(self.campaign/'full-print-recovery.json', {'output': str(self.output)})
        self.assertFalse(awaiting_print_recovery(self.campaign))
        with r.locked(self.campaign/'full-print-recovery.lock'):
            self.assertTrue(awaiting_print_recovery(self.campaign))
            with r.locked(self.output/f'{self.batch}.lock'):
                (self.output/self.batch).mkdir()
                self.assertTrue(awaiting_print_recovery(self.campaign))
            self.assertFalse(awaiting_print_recovery(self.campaign))
        self.assertFalse(awaiting_print_recovery(self.campaign))

    def test_mixed_and_large_failures_are_left_for_review(self):
        for failures in ([{'face': 'One', 'lost': ['exact_default', 'exact']}],
                         [{'face': 'One', 'gate': 'native'}],
                         [{'face': str(i), 'lost': ['exact_default']} for i in range(17)]):
            with self.subTest(failures=failures):
                self.validation['full_checks']['failures'] = failures
                r.write(self.folder/'validation.json', self.validation)
                self.assertEqual(self.pending(), [])

    def test_recovery_requires_the_existing_readmission_verifier(self):
        result = {'rules': 7, 'jobs': ['a']}
        with patch.object(recover.isolation, 'isolate', return_value=result) as isolate, \
                patch.object(recover.isolation, 'readmit_result', side_effect=ValueError('stale evidence')) as verify:
            receipt = recover.recover(self.campaign, self.output, self.batch, 32, apply=True)
        proof = self.output/self.batch/'proof'
        isolate.assert_called_once_with(self.campaign, self.batch, proof, 32, full_check=True)
        verify.assert_called_once_with(self.campaign, proof, result, full_check=True)
        self.assertEqual(receipt, {'status': 'needs-review', 'reason': 'stale evidence'})
        self.assertEqual(self.pending(), [])

    def test_exhausted_diagnosis_rechecks_witnesses_before_readmission(self):
        result = {'rules': 1, 'jobs': ['a']}
        attempts = [recover.isolation.FocusedTrialLimit(['b']), result]
        with patch.object(recover.isolation, 'isolate', side_effect=attempts) as isolate, \
                patch.object(recover.isolation, 'readmit_result') as readmit:
            receipt = recover.recover(self.campaign, self.output, self.batch, 52, apply=True, mode='parse')
        folder = self.output/self.batch
        self.assertEqual(isolate.call_args_list[0].args, (self.campaign, self.batch, folder/'proof', 52))
        self.assertEqual(isolate.call_args.args, (self.campaign, self.batch, folder/'proof-with-witnesses', 44))
        self.assertEqual(isolate.call_args.kwargs, {'suspects': ('b',), 'full_check': False})
        readmit.assert_called_once_with(self.campaign, folder/'proof-with-witnesses', result, full_check=False)
        self.assertEqual(receipt['proof'], str(folder/'proof-with-witnesses/result.json'))

    def test_recovery_extends_only_after_verifying_the_added_baseline_faces(self):
        folder = self.output/self.batch
        folder.mkdir()
        source, inputs = self.campaign/'source', self.campaign/'inputs'
        for path in (source/'grammar', inputs/'frozen/grammar'):
            path.mkdir(parents=True)
            (path/'root.ebnf').write_text('root ::= "accepted"\n')
        (self.campaign/'checkpoints').mkdir()
        r.write(self.campaign/'checkpoints/head.json', {'commit': 'head', 'source': str(source), 'inputs': str(inputs)})
        config = self.campaign/'config.json'
        prior, rejected = folder/'before.json', folder/'failed-faces.json'
        r.write(prior, {'cards': ['Existing']})
        r.write(rejected, {'cards': ['Existing', 'Regressed']})
        before = {'native_regression_controls': recover.isolation._spec(prior)}
        r.write(config, before)
        proof = folder/'isolation.json'
        r.write(proof, {'profile': recover.isolation._spec(rejected)})
        result = {'evidence': {'isolation': recover.isolation._spec(proof)}}
        with (
            patch.object(recover, 'Queue') as queue,
            patch.object(recover.tasks, 'close_cohort') as closure,
            patch.object(recover.batches, '_control_setup') as setup,
            patch.object(recover.batches, '_control_check') as check,
        ):
            queue.return_value.progress.return_value = {'checkpoint': 'head'}
            check.return_value = {'failures': ['invalid baseline']}
            with self.assertRaisesRegex(ValueError, 'accepted baseline'):
                recover._retain_recovery_faces(self.campaign, folder, result)
            self.assertEqual(r.read(config), before)
            check.return_value = {'failures': []}
            extended = recover._retain_recovery_faces(self.campaign, folder, result)
        self.assertEqual(extended['added'], ['Regressed'])
        self.assertEqual(r.read(config)['native_regression_controls'], extended['profile'])
        self.assertEqual(r.read(Path(extended['profile']['path']))['cards'], ['Existing', 'Regressed'])
        closure.assert_called_with(inputs, {'names': ['Existing', 'Regressed']})
        focused = setup.call_args.args[0]['native_regression_controls']['path']
        self.assertEqual(r.read(Path(focused))['cards'], ['Regressed'])

    def test_live_parse_recovery_requires_enabled_complete_additions(self):
        self.validation.update(reason=PARSE_SCREEN_REJECTION,
                               parse_screen={'status': 'failed', 'native_exit': 0,
                                             'failures': ['native readings changed or failed validation',
                                                          {'face': 'One', 'gate': 'parse/reading-count/text'}]})
        r.write(self.folder/'validation.json', self.validation)
        self.db.execute("UPDATE jobs SET adjudication=json_set(adjudication,'$.evidence.validation.reason',?)",
                        (PARSE_SCREEN_REJECTION,))
        self.db.commit()
        self.assertEqual(self.pending(), [])
        self.assertEqual(recover.pending_failed_batches(self.campaign, self.output, include_parse=True),
                         [(self.batch, 'parse')])
        r.write(self.campaign/'full-print-recovery.json', {'output': str(self.output), 'include_parse': True})
        with r.locked(self.campaign/'full-print-recovery.lock'):
            self.assertTrue(awaiting_print_recovery(self.campaign))
            self.validation['parse_screen']['native_exit'] = 1
            r.write(self.folder/'validation.json', self.validation)
            self.assertFalse(awaiting_print_recovery(self.campaign))
            native = self.folder/'parse-screen/native.json'
            native.parent.mkdir()
            native_report = {'reading_preservation': {'passed': False, 'merged_faces': 0,
                                                       'missing': [{'name': 'One', 'reading': 0}],
                                                       'additions': [{'name': 'Two', 'added_current_indices': [0]}]}}
            r.write(native, native_report)
            screen = self.validation['parse_screen']
            screen['failures'].append({'face': 'Two', 'gate': 'parse/reading-count/text'})
            screen['artifacts'] = {'native.json': {'path': str(native), 'sha256': r.digest(native)}}
            r.write(self.folder/'validation.json', self.validation)
            self.assertTrue(awaiting_print_recovery(self.campaign))

            native_report['reading_preservation']['missing'][0]['name'] = 'Another'
            r.write(native, native_report)
            self.assertFalse(awaiting_print_recovery(self.campaign))
            screen['artifacts']['native.json']['sha256'] = r.digest(native)
            r.write(self.folder/'validation.json', self.validation)
            self.assertFalse(awaiting_print_recovery(self.campaign))

    def test_parse_recovery_uses_the_parse_proof_for_readmission(self):
        result = {'rules': 7, 'jobs': ['a']}
        with patch.object(recover.isolation, 'isolate', return_value=result) as isolate, \
                patch.object(recover.isolation, 'readmit_result') as readmit:
            receipt = recover.recover(self.campaign, self.output, self.batch, 32, apply=True, mode='parse')
        proof = self.output/self.batch/'proof'
        isolate.assert_called_once_with(self.campaign, self.batch, proof, 32, full_check=False)
        readmit.assert_called_once_with(self.campaign, proof, result, full_check=False)
        self.assertEqual(receipt['status'], 'readmitted')

    def test_live_library_recovery_requires_exact_library_only_failure(self):
        origin = 'a'*40
        self.validation = {'status':'failed', 'reason':LIBRARY_REJECTION, 'scope':'combined-batch',
                           'source_checkpoint':origin, 'repository_check_returncode':1}
        r.write(self.folder/'validation.json', self.validation)
        (self.campaign/'checkpoints').mkdir()
        r.write(self.campaign/'checkpoints'/f'{origin}.json', {'library_regression_controls':{'path':'profile'}})
        self.db.execute("UPDATE jobs SET adjudication=json_set(adjudication,'$.evidence.validation.reason',?)",
                        (LIBRARY_REJECTION,))
        self.db.commit()
        log = self.folder/'repository-check.log'
        states = ''.join(f'{gate} passed 1.0 s\n' for gate in ('fmt','pin','hygiene','tools','clippy','build','families'))
        summary = 'test FAILED 2.0 s\n\nfailures:\n    tests::old\n\ntest result: FAILED. 1 passed; 1 failed;\n'
        log.write_text(states+summary)
        self.assertEqual(self.pending(), [])
        self.assertEqual(recover.pending_failed_batches(self.campaign, self.output, include_library=True),
                         [(self.batch, 'library')])
        r.write(self.campaign/'full-print-recovery.json', {'output':str(self.output), 'include_library':True})
        with r.locked(self.campaign/'full-print-recovery.lock'):
            self.assertTrue(awaiting_print_recovery(self.campaign))
            for broken in (states.replace('pin passed', 'pin FAILED')+summary, states+'test FAILED 2.0 s\n'):
                log.write_text(broken)
                self.assertFalse(awaiting_print_recovery(self.campaign))

    def test_library_recovery_uses_the_library_proof_for_readmission(self):
        result = {'rules':7, 'jobs':['a']}
        with patch.object(recover.library, 'isolate', return_value=result) as isolate, \
                patch.object(recover.library, 'readmit_result') as readmit:
            receipt = recover.recover(self.campaign, self.output, self.batch, 32, apply=True, mode='library')
        path = self.output/self.batch/'proof'
        isolate.assert_called_once_with(self.campaign, self.batch, path, 32)
        readmit.assert_called_once_with(self.campaign, path, result)
        self.assertEqual(receipt['status'], 'readmitted')
