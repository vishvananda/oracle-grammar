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
        task = self.root/'.cache/attempts'/self.attempt/'tasks/parent.json'
        task.parent.mkdir(parents=True)
        task.write_text(json.dumps({'cohort':['A','B']}))
        self.evidence = {'kind':'reviewed-author-split','source_checkpoint':'head','rules':['m_good'],
                         'target_faces':['A'],'review_notes':'Keep the deferred rule.',
                         'original_report':{'path':'old','sha256':'old'}, 'report':{'path':'new','sha256':'new'}}
        self.enterContext(patch.object(self.q,'_require_absent_pid'))
        self.original_report = {'family_returncode':0, 'descriptor_sha256':sha256(task.read_bytes()).hexdigest()}
        self.archive = self.enterContext(patch.object(retire_queue, '_verify_original_report',
                                                      return_value=(self.original_report, {})))
        self.review = self.enterContext(patch.object(retire_queue,'_verify_reviewed_repair',
                                                    return_value=({}, {'cohort':['A','B']})))

    def park(self, *, integration=False):
        proposed = {'m_good':'proposed','m_deferred':'proposed' if integration else 'needs-review'}
        dispositions = {rule:{'rule':rule,'status':status} for rule,status in proposed.items()}
        outcomes = {rule:{'status':'pass' if integration else 'needs-review',
                          'reason':'held','evidence':'archive'} for rule in dispositions}
        self.q.finish(self.attempt,'provisional-pass' if integration else 'needs-review',
                      {'kind':'author','dispositions':dispositions},outcomes)
        if integration:
            self.q.reject_jobs(['parent'],{'reason':'full-population disagreement'})
        self.original = self.q.get_attempt(self.attempt)

    def test_split_preserves_archived_outcomes_and_reviewed_draft(self):
        self.park()
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

    def test_failed_archive_can_be_split_after_a_fresh_passing_repair(self):
        self.park()
        evidence = {**self.evidence, 'kind':'reviewed-author-repair-split',
                    'rules':['m_deferred'], 'target_faces':['B']}
        self.original_report['family_returncode'] = 1
        with self.assertRaisesRegex(ValueError, 'lacks a full author evaluation'):
            self.q.split_reviewed_author('parent', self.attempt, 'head', evidence)
        self.assertEqual(self.q.coverage()['counts'], {'needs-review':2})
        self.original_report['family_returncode'] = 0
        self.review.return_value = ({}, {'cohort':['B']})
        with self.assertRaisesRegex(ValueError, 'does not cover the original cohort'):
            self.q.split_reviewed_author('parent', self.attempt, 'head', evidence)

        self.review.return_value = ({}, {'cohort':['A','B']})
        receipt = self.q.split_reviewed_author('parent', self.attempt, 'head', evidence)
        self.assertEqual(self.archive.call_args.kwargs, {'passed':False})
        self.assertEqual(receipt['rules'], ['m_deferred'])
        self.assertEqual(self.q.get_attempt(self.attempt), self.original)
        self.assertEqual(self.q.coverage()['counts'], {'pending':1,'needs-review':1})

    def test_family_veto_split_preserves_cases_and_requires_separate_dependencies(self):
        self.park()
        old = self.root/'old-evaluation'
        new = self.root/'new-evaluation'
        old.mkdir()
        new.mkdir()
        positive = {'name':'A','text':'A.','expected':['meaning'],'exact':True}
        refusal = {'name':'Refusal','text':'Incomplete.','reject':True}
        family = {'family':'archived','cases':[positive,refusal,{'name':'B','expected':['held']}]}
        self.original_report.update(directory=str(old),family_returncode=1,
                                    failures=['family regression tests failed'])
        self.archive.return_value = self.original_report,family
        gate = old/'family-check.log'
        data = {'family':'archived','cases':3,'passed':2,'compositional':True,
                'failures':[{'name':'B','why':'unexpected refusal: NoParse { furthest: 0, tokens: 1 }'}]}
        gate.write_text(json.dumps(data)+'\n')
        evidence = {**self.evidence,'kind':'reviewed-family-repair-split','scan':{'proof':'accepted'},
                    'family_gate':{'path':str(gate),'sha256':sha256(gate.read_bytes()).hexdigest()}}
        scan = self.root/'scan/rules.json'
        scan.parent.mkdir()
        check = self.enterContext(patch.object(retire_rehabilitation,'verify_scan'))

        def dependencies(value):
            scan.write_text(json.dumps({'needed':value}))
            check.return_value = self.root,set(),'manifest',sha256(scan.read_bytes()).hexdigest()

        def repaired(cases):
            fixture = new/'family.json'
            fixture.write_text(json.dumps({'cases':cases}))
            self.review.return_value = ({'directory':str(new),'fixture_sha256':sha256(fixture.read_bytes()).hexdigest()},
                                        {'cohort':['A','B']})

        before = self.q.audit()
        dependencies({'m_good':['A'],'m_deferred':['A','B']})
        repaired([positive,refusal])
        with self.assertRaisesRegex(ValueError,'does not separate'):
            self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
        dependencies({'m_good':['A'],'m_deferred':['B']})
        repaired([positive])
        with self.assertRaisesRegex(ValueError,'omitted an archived passing case or refusal'):
            self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
        repaired([positive,refusal])
        outside = self.root/'family-check.log'
        outside.write_bytes(gate.read_bytes())
        with self.assertRaisesRegex(ValueError,'outside the original evaluation'):
            self.q.split_reviewed_author('parent',self.attempt,'head',
                                         {**evidence,'family_gate':{**evidence['family_gate'],'path':str(outside)}})
        self.assertEqual(self.q.audit(), before)
        receipt = self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
        self.assertEqual(receipt['rules'], ['m_good'])
        self.assertEqual(self.q.get_attempt(self.attempt), self.original)
        self.assertEqual(self.q.coverage()['counts'], {'pending':1,'needs-review':1})

    def test_unremoved_draft_split_keeps_the_original_cohort_and_requires_separate_rules(self):
        self.park()
        evidence = {**self.evidence,'kind':'reviewed-unremoved-draft-split','scan':{'proof':'accepted'}}
        scan = self.root/'scan/rules.json'
        scan.parent.mkdir()
        with patch.object(retire_rehabilitation,'verify_scan') as accepted:
            def dependencies(value):
                scan.write_text(json.dumps({'needed':value}))
                accepted.return_value = self.root,set(),'manifest',sha256(scan.read_bytes()).hexdigest()
            dependencies({'m_good':['A'],'m_deferred':['A']})
            with self.assertRaisesRegex(ValueError,'does not separate'):
                self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
            dependencies({'m_good':['A'],'m_deferred':['B']})
            self.review.return_value = ({},{'cohort':['A']})
            with self.assertRaisesRegex(ValueError,'does not cover the original cohort'):
                self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
            self.review.return_value = ({},{'cohort':['A','B']})
            receipt = self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
        self.assertEqual(self.archive.call_args.kwargs, {'passed':False,'unremoved':True})
        self.assertEqual(receipt['rules'], ['m_good'])
        self.assertEqual(self.q.get_attempt(self.attempt), self.original)
        self.assertEqual(self.q.coverage()['counts'], {'pending':1,'needs-review':1})

    def test_repair_can_split_an_inherited_review_using_the_original_attempt(self):
        self.park()
        rules = ['m_initial', 'm_repaired', 'm_held']
        self.q.add_jobs([{'id':'chain', 'kind':'author', 'goal':'Retire three rules.',
                          'rules':rules, 'target_faces':['A','B']}])
        attempt = self.q.claim('terra')['attempt_id']
        (self.root/'leases/chain').touch()
        with closing(self.q._connect()) as db:
            db.execute('UPDATE attempts SET started=? WHERE id=?', (json.dumps({'pid':4567}),attempt))
        task = self.root/'.cache/attempts'/attempt/'tasks/chain.json'
        task.parent.mkdir(parents=True)
        task.write_text(json.dumps({'cohort':['A','B']}))
        self.original_report['descriptor_sha256'] = sha256(task.read_bytes()).hexdigest()
        dispositions = {rule:{'rule':rule,'status':'proposed' if rule == 'm_initial' else 'needs-review'}
                        for rule in rules}
        outcomes = {rule:{'status':'needs-review','reason':'held','evidence':'archive'} for rule in rules}
        self.q.finish(attempt, 'needs-review', {'kind':'author','dispositions':dispositions}, outcomes)
        first = self.q.split_reviewed_author('chain', attempt, 'head', {**self.evidence,'rules':['m_initial']})
        parent = first['review_job']
        self.q.add_checkpoint('next', 'accepted newer grammar', expected_parent='head')
        repair = {**self.evidence,'kind':'reviewed-author-repair-split','source_checkpoint':'next',
                  'rules':['m_repaired'],'target_faces':['B']}
        with self.assertRaisesRegex(ValueError,'parked author attempt'):
            self.q.split_reviewed_author(parent, self.attempt, 'next', repair)
        old = self.q.get_attempt(attempt)
        self.review.return_value = ({}, {'cohort':['B']})
        with self.assertRaisesRegex(ValueError,'does not cover the original cohort'):
            self.q.split_reviewed_author(parent, attempt, 'next', repair)
        self.review.return_value = ({}, {'cohort':['A','B']})
        second = self.q.split_reviewed_author(parent, attempt, 'next', repair)
        self.assertEqual(self.archive.call_args.args[1:3], ('chain',attempt))
        self.assertEqual(self.q.get_attempt(attempt), old)
        review = next(job for job in self.q.coverage()['jobs'] if job['id'] == second['review_job'])
        self.assertEqual((review['rules'],review['review_origin']['attempt_id']), (['m_held'],attempt))
        self.assertEqual(self.q.reviewed_draft(second['job_id'])['source_checkpoint'], 'next')

    def test_split_preserves_the_verified_child_when_checkpoint_advances(self):
        self.park()
        self.q.add_checkpoint('new', 'accepted newer grammar', expected_parent='head')
        before = self.q.audit()
        with self.assertRaisesRegex(ValueError, 'outside the accepted checkpoint chain'):
            self.q.split_reviewed_author('parent', self.attempt, 'new',
                                         {**self.evidence, 'source_checkpoint':'unaccepted'})
        with self.assertRaisesRegex(ValueError, 'checkpoint changed'):
            self.q.split_reviewed_author('parent', self.attempt, 'head', self.evidence)
        self.assertEqual(self.q.audit(), before)
        receipt = self.q.split_reviewed_author('parent', self.attempt, 'new', self.evidence)
        child = self.review.call_args.args[2]
        self.assertEqual(self.review.call_args.args[1], 'head')
        self.assertEqual((receipt['checkpoint'], receipt['job_id']), ('new', child['id']))
        self.assertEqual(child['review_checkpoint'], 'head')
        self.assertEqual(self.q.reviewed_draft(child['id']),
                         {'source_checkpoint':'head', 'report':self.evidence['report']})
        self.assertEqual(self.q.get_attempt(self.attempt), self.original)
        self.assertEqual(self.q.claim('terra')['base_commit'], 'new')

    def test_selected_rule_and_coverage_must_match_without_writes(self):
        self.park()
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

    def test_rejected_integration_keeps_the_passing_archive_and_full_cohort(self):
        self.park(integration=True)
        evidence = {**self.evidence,'kind':'reviewed-integration-repair-split'}
        before = self.q.audit()
        with self.assertRaisesRegex(ValueError,'parked author attempt'):
            self.q.split_reviewed_author('parent',self.attempt,'head',self.evidence)
        self.review.return_value = ({},{'cohort':['A']})
        with self.assertRaisesRegex(ValueError,'does not cover the original cohort'):
            self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
        self.assertEqual(self.q.audit(),before)
        self.review.return_value = ({},{'cohort':['A','B']})
        receipt = self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
        self.assertEqual(self.archive.call_args.kwargs,{'passed':True})
        self.assertEqual(receipt['rules'],['m_good'])
        self.assertEqual(self.q.get_attempt(self.attempt),self.original)
        self.assertEqual(self.q.coverage()['counts'],{'pending':1,'needs-review':1})
        held, = self.q.get_jobs('needs-review')
        self.assertEqual(held['job']['review_origin']['status'],'integration-rejected')


if __name__ == '__main__':
    unittest.main()
