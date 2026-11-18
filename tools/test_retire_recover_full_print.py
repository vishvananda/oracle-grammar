"""Automatically recover only intact, small full-print rejection groups."""
import json
from pathlib import Path
import sqlite3
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch

import retire as r
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
                         [(self.batch, False)])
        r.write(self.campaign/'full-print-recovery.json', {'output': str(self.output), 'include_parse': True})
        with r.locked(self.campaign/'full-print-recovery.lock'):
            self.assertTrue(awaiting_print_recovery(self.campaign))
            self.validation['parse_screen']['native_exit'] = 1
            r.write(self.folder/'validation.json', self.validation)
            self.assertFalse(awaiting_print_recovery(self.campaign))

    def test_parse_recovery_uses_the_parse_proof_for_readmission(self):
        result = {'rules': 7, 'jobs': ['a']}
        with patch.object(recover.isolation, 'isolate', return_value=result) as isolate, \
                patch.object(recover.isolation, 'readmit_result') as readmit:
            receipt = recover.recover(self.campaign, self.output, self.batch, 32, apply=True, full_check=False)
        proof = self.output/self.batch/'proof'
        isolate.assert_called_once_with(self.campaign, self.batch, proof, 32, full_check=False)
        readmit.assert_called_once_with(self.campaign, proof, result, full_check=False)
        self.assertEqual(receipt['status'], 'readmitted')
