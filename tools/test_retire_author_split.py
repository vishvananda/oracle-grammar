"""A reviewed split preserves distinct ownership and the original review."""
from contextlib import closing
from hashlib import sha256
import json
from pathlib import Path
import subprocess
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
        with closing(self.q._connect()) as db, db:
            row = db.execute("SELECT payload FROM jobs WHERE id='parent'").fetchone()
            job = json.loads(row[0])
            job['target_faces'].extend(['C','D'])
            db.execute("UPDATE jobs SET payload=? WHERE id='parent'", (json.dumps(job),))
        self.park()
        old = self.root/'old-evaluation'
        new = self.root/'new-evaluation'
        old.mkdir()
        new.mkdir()
        positive = {'name':'A','text':'A.','expected':['meaning'],'exact':True,'default':True}
        refusal = {'name':'Refusal','text':'Incomplete.','reject':True}
        family = {'family':'archived','cases':[positive,refusal,{'name':'B','expected':['held'],'exact':True},
                                             {'name':'C','expected':['held']}, {'name':'D','expected':['held']}]}
        self.original_report.update(directory=str(old),family_returncode=1,
                                    failures=['family regression tests failed'])
        self.archive.return_value = self.original_report,family
        gate = old/'family-check.log'
        data = {'family':'archived','cases':5,'passed':2,'compositional':True,
                'failures':[{'name':'B','why':'a card reading has no exact keyed print'},
                            {'name':'C','why':'unexpected refusal: Binding("missing event receipt")'},
                            {'name':'D','why':'unexpected refusal: NoParse { furthest: 0, tokens: 1 }'}]}

        def gate_evidence():
            gate.write_text(json.dumps(data)+'\n')
            return {**self.evidence,'kind':'reviewed-family-repair-split','scan':{'proof':'accepted'},
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
        data['failures'][1]['why'] = 'report could not validate its test configuration'
        with self.assertRaisesRegex(ValueError,'does not isolate'):
            self.q.split_reviewed_author('parent',self.attempt,'head',gate_evidence())
        data['failures'][1]['why'] = 'unexpected refusal: Binding("missing event receipt")'
        evidence = gate_evidence()
        dependencies({'m_good':['A'],'m_deferred':['A','B']})
        repaired([positive,refusal])
        with self.assertRaisesRegex(ValueError,'does not separate'):
            self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
        dependencies({'m_good':['A'],'m_deferred':['B']})
        repaired([positive])
        with self.assertRaisesRegex(ValueError,'omitted an archived positive case or refusal'):
            self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
        repaired([positive,refusal])
        outside = self.root/'family-check.log'
        outside.write_bytes(gate.read_bytes())
        with self.assertRaisesRegex(ValueError,'outside the original evaluation'):
            self.q.split_reviewed_author('parent',self.attempt,'head',
                                         {**evidence,'family_gate':{**evidence['family_gate'],'path':str(outside)}})
        data['passed'] -= 1
        data['failures'].append({'name':'A','why':'a card reading has no verified default print'})
        positive['default'] = False
        with self.assertRaisesRegex(ValueError,'does not isolate'):
            self.q.split_reviewed_author('parent',self.attempt,'head',gate_evidence())
        positive['default'] = True
        evidence = gate_evidence()
        repaired([refusal])
        with self.assertRaisesRegex(ValueError,'omitted an archived positive case or refusal'):
            self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
        repaired([dict(positive, default=False),refusal])
        with self.assertRaisesRegex(ValueError,'omitted an archived positive case or refusal'):
            self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
        self.assertEqual(self.q.audit(), before)
        repaired([positive,refusal])
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

    def test_fresh_family_split_requires_source_edits_and_preserves_resolved_cases(self):
        self.park()
        old, new = self.root/'old', self.root/'new'
        for directory in (old, new, self.root/'scan'):
            directory.mkdir()
        refusal = {'name':'Bad wording','text':'Draw invalid.','reject':True}
        alias = {'name':'Original example','text':'Draw a card.','expected':['meaning']}
        synthetic = {'name':'Synthetic sibling','text':'Do a sibling action.','expected':['sibling']}
        family = {'family':'archive','cases':[synthetic, refusal, alias]}
        self.original_report.update(directory=str(old),family_returncode=1,
                                    failures=['family regression tests failed'])
        self.archive.return_value = self.original_report,family
        gate = old/'family-check.log'
        gate.write_text(json.dumps({'family':'archive','cases':3,'passed':2,'compositional':True,
                                    'failures':[{'name':'Synthetic sibling','why':'unexpected refusal: NoParse { x }'}]}))
        evidence = {**self.evidence,'kind':'reviewed-fresh-family-split','scan':{},
                    'family_gate':{'path':str(gate),'sha256':sha256(gate.read_bytes()).hexdigest()}}
        scan = self.root/'scan/rules.json'
        scan.write_text(json.dumps({'needed':{'m_good':['A'],'m_deferred':['B']}}))
        pin = 'a' * 64
        with closing(self.q._connect()) as db, db:
            db.execute("UPDATE checkpoints SET evidence=? WHERE commit_id='head'",
                       (json.dumps({'validation':{'validation_manifest_sha256':pin}}),))
        self.enterContext(patch.object(retire_rehabilitation,'verify_scan',
                                        return_value=(self.root,set(),pin,sha256(scan.read_bytes()).hexdigest())))
        self.enterContext(patch.object(retire_rehabilitation,'approved_family_texts',
                                        return_value={'A':{'Draw a card.'},'B':{'Draw two cards.'}}))
        fixture = new/'family.json'
        edits = new/'edits.json'
        report = {'directory':str(new)}

        def repair(cases, values=(2, 3), flags=(True,True,True,False)):
            fixture.write_text(json.dumps({'cases':cases}))
            mutations = [[{'path':[0,'count'], 'value':value}] for value in values]
            edits.write_text(json.dumps([{'face':'A','set':change} for change in mutations]))
            report.update(fixture_sha256=sha256(fixture.read_bytes()).hexdigest(),
                          edits_sha256=sha256(edits.read_bytes()).hexdigest(),
                          edited=[{'face':'A','mutations':change,'flags':list(flags)} for change in mutations])
            self.review.return_value = (report, {'cohort':['A','B']})

        repair([refusal])
        with self.assertRaisesRegex(ValueError,'omitted an archived positive case'):
            self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
        repair([refusal, alias], values=(2,2))
        with self.assertRaisesRegex(ValueError,'distinct edits covering every selected face'):
            self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
        repair([refusal, alias], flags=(True,False,True,False))
        with self.assertRaisesRegex(ValueError,'exact, verified operand edits'):
            self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
        self.assertEqual(self.q.coverage()['counts'],{'needs-review':2})
        repair([refusal, alias])
        receipt = self.q.split_reviewed_author('parent',self.attempt,'head',evidence)
        self.assertEqual(receipt['rules'], ['m_good'])
        self.assertEqual(self.q.get_attempt(self.attempt), self.original)
        self.assertEqual(self.q.coverage()['counts'],{'pending':1,'needs-review':1})

    def test_fresh_split_can_defer_only_anchored_sibling_semantic_failures(self):
        gate = self.root/'family-check.log'
        archived = {'directory':str(self.root),'family_returncode':1,
                    'failures':['family regression tests failed']}
        positive = {'name':'A','text':'Draw a card.','expected':['meaning']}
        refusal = {'name':'Incomplete','text':'Draw.','reject':True}
        sibling = {'name':'B','text':'Draw two cards.','expected':['held'],'readings':1}
        family = {'family':'archive','cases':[positive,refusal,sibling]}
        selected, deferred = {'target_faces':['A']}, {'target_faces':['A','B']}
        approved = {'A':{'Draw a card.'}, 'B':{'Draw two cards.'}}

        def replay(label, why, *, fresh=True, sources=approved):
            gate.write_text(json.dumps({'family':'archive','compositional':True,'cases':3,'passed':2,
                                        'failures':[{'name':label,'why':why}]}))
            evidence = {'family_gate':{'path':str(gate),'sha256':sha256(gate.read_bytes()).hexdigest()}}
            return retire_queue._family_split_cases(evidence,archived,family,selected,deferred,
                                                     approved=sources,fresh=fresh)

        mismatch = 'no reading equals the expected IR'
        self.assertEqual(replay('B',mismatch), ({'A'},[positive,refusal]))
        self.assertEqual(replay('B','expected Some(1) readings, got 2'), ({'A'},[positive,refusal]))
        for label, why, fresh, sources in (
            ('A',mismatch,True,approved),
            ('B',mismatch,False,approved),
            ('B',mismatch,True,{**approved,'B':{'Discard two cards.'}}),
            ('B','expected Some(2) readings, got 3',True,approved),
            ('B','expected Some(1) readings, got 1',True,approved),
            ('B','work limit: test',True,approved),
        ):
            with self.subTest(label=label, why=why, fresh=fresh, sources=sources):
                with self.assertRaisesRegex(ValueError,'does not isolate replayable cases'):
                    replay(label,why,fresh=fresh,sources=sources)

    def test_numbered_family_cases_require_pinned_oracle_words_and_preserve_labels(self):
        self.park()
        old = self.root/'old'
        new = self.root/'new'
        frozen = self.root/'frozen'
        scan = self.root/'scan'
        for directory in (old, new, frozen, scan):
            directory.mkdir()
        oracle = frozen/'oracle.json'
        data = json.dumps([{'cards': [
            {'name':'A','text':'{3}: Do the first thing. Then draw a card.\nDo the second thing.'},
            {'name':'B','text':'Do the held thing.'}]}]).encode()
        oracle.write_bytes(data)
        manifest = self.root/'manifest.json'
        manifest.write_text(json.dumps({'files':{'oracle.json':sha256(data).hexdigest()}}))
        pinned = sha256(manifest.read_bytes()).hexdigest()
        with closing(self.q._connect()) as db, db:
            db.execute("UPDATE checkpoints SET evidence=? WHERE commit_id='head'",
                       (json.dumps({'validation':{'validation_manifest_sha256':pinned}}),))
        dependencies = scan/'rules.json'
        dependencies.write_text(json.dumps({'needed':{'m_good':['A'],'m_deferred':['B']}}))
        self.enterContext(patch.object(retire_rehabilitation, 'verify_scan',
                                       return_value=(self.root,set(),pinned,sha256(dependencies.read_bytes()).hexdigest())))
        first = {'name':'A 0','text':'Do the first thing.','expected':['first'],'exact':True,'default':True}
        second = {'name':'A 1','text':'Do the second thing.','expected':['second'],'exact':True,'default':True}
        held = {'name':'B 0','text':'Unrelated words.','expected':['held']}
        refusal = {'name':'Synthetic refusal','text':'Do the.','reject':True}
        family = {'family':'archive','cases':[first,second,held,refusal]}
        self.original_report.update(directory=str(old), family_returncode=1, failures=['family regression tests failed'])
        self.archive.return_value = self.original_report, family
        gate = old/'family-check.log'
        gate.write_text(json.dumps({'family':'archive','cases':4,'passed':3,'compositional':True,
                                    'failures':[{'name':'B 0','why':'unexpected refusal: NoParse { furthest: 0 }'}]}))
        evidence = {**self.evidence,'kind':'reviewed-family-repair-split','numbered_family_cases':True,
                    'family_gate':{'path':str(gate),'sha256':sha256(gate.read_bytes()).hexdigest()}}
        fixture = new/'family.json'

        def repaired(cases):
            fixture.write_text(json.dumps({'cases':cases}))
            self.review.return_value = ({'directory':str(new),'fixture_sha256':sha256(fixture.read_bytes()).hexdigest()},
                                        {'cohort':['A','B']})

        repaired([first,second,refusal])
        with self.assertRaisesRegex(ValueError,'does not isolate'):
            self.q.split_reviewed_author('parent', self.attempt, 'head', evidence)
        held['text'] = 'Do the held thing.'
        with self.assertRaisesRegex(ValueError,'does not isolate'):
            self.q.split_reviewed_author('parent', self.attempt, 'head', {**evidence,'numbered_family_cases':False})
        oracle.write_bytes(data+b' ')
        with self.assertRaisesRegex(ValueError,'rehabilitation input hash differs'):
            self.q.split_reviewed_author('parent', self.attempt, 'head', evidence)
        oracle.write_bytes(data)
        repaired([dict(first, name='A'),second,refusal])
        with self.assertRaisesRegex(ValueError,'omitted an archived positive case or refusal'):
            self.q.split_reviewed_author('parent', self.attempt, 'head', evidence)
        repaired([first,second,refusal])
        receipt = self.q.split_reviewed_author('parent', self.attempt, 'head', evidence)
        self.assertEqual(receipt['rules'], ['m_good'])
        self.assertEqual(self.q.coverage()['counts'], {'pending':1,'needs-review':1})

    def test_legacy_native_veto_must_agree_with_residual_family_refusals(self):
        directory = self.root/'evaluation'
        directory.mkdir()
        args = ['/frozen/bin/validate-readings', *(str(directory/name) for name in (
            'gold.jsonl','rows.jsonl','native.json','native-failures.jsonl')),
            '--preserve-from',str(directory/'before.jsonl')]
        archived = {'directory':str(directory),'family_returncode':1,
                    'failures':['family regression tests failed',str(subprocess.CalledProcessError(1,args))]}
        positive = {'name':'A','expected':['meaning']}
        refusal = {'name':'Incomplete','reject':True}
        family = {'family':'archived','cases':[positive,refusal,{'name':'B','expected':['held']}]}
        selected, recovered = {'target_faces':['A']}, {'target_faces':['A','B']}
        gate = {'family':'archived','compositional':True,'cases':3,'passed':2,
                'failures':[{'name':'B','why':'unexpected refusal: NoParse { furthest: 0 }'}]}
        gold = {'faces':2,'valid':2,'idempotent':2}
        native = {'reading_preservation':{'occurrences':2,'old_readings':2,'merged_faces':0,'passed':False,
                                         'missing':[{'name':'B','reading':0}],'additions':[]},
                  'source_gold':gold,'canonical_gold':gold,'readings':{'total':1,'valid':1,'canonical':1}}

        def artifact(name, content):
            path = directory/name
            path.write_text(content)
            return {'path':str(path),'sha256':sha256(path.read_bytes()).hexdigest()}

        commands = '\n'.join(json.dumps({'program':name,'returncode':code}) for name,code in (
            ('family-check',1),('score',0),('validate-readings',1)))
        evidence = {'family_gate':artifact('family-check.log',json.dumps(gate)),
                    'legacy_native_gate':{'commands.jsonl':artifact('commands.jsonl',commands)}}
        preserve = native['reading_preservation']
        for changes in ({'missing':[{'name':'A','reading':0}]}, {'additions':[{'name':'B','reading':1}]},
                        {'old_readings':3}):
            evidence['legacy_native_gate']['native.json'] = artifact(
                'native.json',json.dumps(dict(native,reading_preservation={**preserve,**changes})))
            with self.assertRaisesRegex(ValueError,'does not isolate residual family refusals'):
                retire_queue._family_split_cases(evidence,archived,family,selected,recovered)
        evidence['legacy_native_gate']['native.json'] = artifact('native.json',json.dumps(native))
        positives, preserved = retire_queue._family_split_cases(evidence,archived,family,selected,recovered)
        self.assertEqual((positives,preserved), ({'A'},[positive,refusal]))

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
