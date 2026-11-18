"""A reviewed split preserves distinct ownership and the original review."""
from contextlib import closing
from hashlib import sha256
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import retire_queue
import retire_rehabilitation
from retire_queue import Queue


class ReviewedAuthorSplitTests(unittest.TestCase):
    def setUp(self):
        temporary = self.enterContext(tempfile.TemporaryDirectory())
        self.root = Path(temporary)
        self.q = Queue(self.root/'queue.sqlite')
        self.q.add_checkpoint('head', 'accepted')
        parent = {'id':'parent', 'kind':'author', 'goal':'Retire two rules.',
                  'rules':['m_good','m_deferred'], 'target_faces':['A','B'],
                  'source_fragments':[{'name':'m_good'},{'name':'m_deferred'}]}
        self.q.add_jobs([parent])
        self.attempt = self.q.claim('terra')['attempt_id']
        leases = self.root/'leases'
        leases.mkdir(exist_ok=True)
        (leases/'parent').touch()
        with closing(self.q._connect()) as db:
            db.execute('UPDATE attempts SET started=? WHERE id=?', (json.dumps({'pid':4567}),self.attempt))
        self.q.finish(self.attempt,'needs-review',{'kind':'author','dispositions':{
            'm_good':{'rule':'m_good','status':'proposed'},
            'm_deferred':{'rule':'m_deferred','status':'needs-review'}}},{
                rule:{'status':'needs-review','reason':'held','evidence':'archive'} for rule in parent['rules']})
        task = self.root/'.cache/attempts'/self.attempt/'tasks/parent.json'
        task.parent.mkdir(parents=True)
        task.write_text(json.dumps({'cohort':['A','B']}))
        self.original = self.q.get_attempt(self.attempt)
        self.evidence = {'kind':'reviewed-author-split','source_checkpoint':'head','rules':['m_good'],
                         'target_faces':['A'],'review_notes':'Keep the deferred rule.',
                         'original_report':{'path':'old','sha256':'old'}, 'report':{'path':'new','sha256':'new'}}
        self.enterContext(patch.object(self.q,'_require_absent_pid'))
        original_report = {'family_returncode':0, 'descriptor_sha256':sha256(task.read_bytes()).hexdigest()}
        self.enterContext(patch.object(retire_queue,'_verify_original_report',return_value=(original_report,{})))
        self.review = self.enterContext(patch.object(retire_queue,'_verify_reviewed_repair',
                                                    return_value=({}, {'cohort':['A','B']})))

    def test_split_preserves_archived_outcomes_and_reviewed_draft(self):
        receipt = self.q.split_reviewed_author('parent',self.attempt,'head',self.evidence)
        self.assertEqual(self.q.coverage()['counts'],{'pending':1,'needs-review':1})
        self.assertEqual(self.q.coverage()['accepted_rules'],0)
        self.assertEqual(self.q.get_attempt(self.attempt),self.original)
        selected, = self.q.get_jobs('pending')
        deferred, = self.q.get_jobs('needs-review')
        self.assertEqual((selected['id'], deferred['id']), (receipt['job_id'], receipt['review_job']))
        self.assertEqual((selected['job']['rules'],deferred['job']['rules']),(['m_good'],['m_deferred']))
        self.assertEqual(selected['job']['source_fragments'],[{'name':'m_good'}])
        self.assertEqual(self.q.reviewed_draft(selected['id']),{'source_checkpoint':'head','report':self.evidence['report']})
        self.assertIsNone(self.q.reviewed_draft(deferred['id']))
        for job in (selected,deferred):
            with closing(self.q._connect()) as db:
                row = db.execute('SELECT status,attempt_id,payload,adjudication FROM jobs WHERE id=?',(job['id'],)).fetchone()
                resolved = retire_rehabilitation.resolve_origin(db,job['id'],row)
            self.assertEqual(resolved[1]['attempt_id'],self.attempt)
        with self.assertRaisesRegex(ValueError,'parked author attempt'):
            self.q.split_reviewed_author('parent',self.attempt,'head',self.evidence)

    def test_selected_rule_and_coverage_must_match_without_writes(self):
        before = self.q.audit()
        tests = (({'rules':['m_deferred']},'was not proposed'),
                 ({'rules':['m_good','m_deferred']},'proper rule subset'),
                 ({'target_faces':['outside']},'assigned target faces'))
        for changes,reason in tests:
            with self.subTest(changes=changes), self.assertRaisesRegex(ValueError,reason):
                self.q.split_reviewed_author('parent',self.attempt,'head',{**self.evidence,**changes})
        self.review.return_value = ({},{'cohort':['A']})
        with self.assertRaisesRegex(ValueError,'does not cover the original cohort'):
            self.q.split_reviewed_author('parent',self.attempt,'head',self.evidence)
        self.assertEqual(self.q.audit(),before)
        self.assertEqual(self.q.get_attempt(self.attempt),self.original)
        self.assertEqual(self.q.coverage()['counts'],{'needs-review':2})


if __name__ == '__main__':
    unittest.main()
