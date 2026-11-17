"""Parse-screen readmission verifies complete rejection and focused preservation."""
from contextlib import closing
import copy
import json
import shutil
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

import retire as r
import retire_batch_driver as driver
import retire_parse_readmission as parsed
import retire_parse_screen as screen
import retire_worker as worker
from test_retire_batch_readmission import BatchReadmissionTests, spec, write


def native_report(rows, old_count, additions):
    count = len(rows); total = sum(row['readings'] for row in rows)
    return {'compiled_schema': 192, 'gold_source_schemas': {'192': count},
            'source_gold': {'faces': count, 'valid': count, 'invalid_reasons': {}},
            'canonical_gold': {'faces': count, 'valid': count, 'idempotent': count, 'invalid_reasons': {}},
            'recognized_faces': count, 'faces_with_any_valid_reading': count, 'faces_with_all_readings_valid': count,
            'readings': {'total': total, 'valid': total, 'canonical': total, 'invalid_reasons': {}},
            'reading_preservation': {'passed': True, 'occurrences': count, 'old_readings': old_count,
                'explicit_readings': 0, 'implicit_equal_singletons': old_count,
                'native_normalized_distinct_readings': old_count, 'missing': [], 'additions': additions, 'merged_faces': 0}}


class ParseReadmissionTests(BatchReadmissionTests):
    def fixture(self):
        folder = self.root/'.cache/batches/batch'
        self.old = [{'name': n, 'text': n.lower(), 'status': 'equal', 'readings': 1, 'exact': True}
                    for n in ('C', 'A', 'B')]
        self.data = ({str(i): dict(name=row['name'], text=row['text'], parse=row,
                                  flags=r.flags(row), record=0 if i == 0 else 1, face=0 if i < 2 else 1)
                      for i, row in enumerate(self.old)},
                     [{'faces': [{'name': 'C'}]}, {'faces': [{'name': 'A'}, {'name': 'B'}]}],
                     [{'cards': [{'name': 'C'}]}, {'cards': [{'name': 'A'}, {'name': 'B'}]}])
        self.inputs = self.root/'checkpoints/parent/inputs'
        self.run = folder/'validation-inputs'
        trees = []
        for root, commit, grammar in ((self.inputs, 'parent', 'base'), (self.run, 'candidate', 'full')):
            frozen = root/'frozen'; (frozen/'grammar').mkdir(parents=True); (frozen/'bin').mkdir()
            (frozen/'grammar/test.ebnf').write_text(grammar)
            r.write(frozen/'allowed.json', {'cards': ['A', 'B', 'C']})
            r.jsonl(frozen/'gold.jsonl', self.data[1]); r.write(frozen/'oracle.json', self.data[2])
            r.write(frozen/'index.json', self.data[0]); r.jsonl(frozen/'parse.jsonl', self.old)
            binaries = {}
            for name in r.HELPERS:
                (frozen/'bin'/name).write_text(name); binaries[name] = r.digest(frozen/'bin'/name)
            r.write(frozen/'build.json', {'source_sha256': '0'*64, 'compiler': {'rustc': 'test'}, 'binaries': binaries})
            r.write(root/'manifest.json', {'format': 3, 'commit': commit, 'population': 3, 'source': '0'*64,
                                          'tooling': r.tooling(), 'files': r.tree(frozen)})
            trees.append(r.tree(frozen/'grammar'))
        self.baseline = self.root/'checkpoints/parent/baseline.jsonl'; r.jsonl(self.baseline, self.old)
        write(self.root/'checkpoints/parent.json', {'commit': 'parent', 'inputs': str(self.inputs), 'baseline': str(self.baseline)})
        manifest_path = folder/'manifest.json'; manifest = r.read(manifest_path); manifest['grammar'] = trees[1]; r.write(manifest_path, manifest)
        addition = {'name': 'A', 'occurrence': 1, 'previous_status': 'equal',
                    'added_current_indices': [1], 'retained_current_indices': [0]}
        self.original = folder/'parse-screen'; self.original.mkdir()
        self.make_checks(self.original, {'A', 'B', 'C'}, addition)
        original_screen = screen.assess(self.run, self.original, self.data, 0)
        original_screen['artifacts'] = {name: spec(self.original/name) for name in parsed.ARTIFACTS}
        write(self.original/'result.json', original_screen)
        validation = {'status': 'failed', 'scope': 'combined-batch', 'candidate': 'candidate', 'source_checkpoint': 'parent',
            'reason': 'full allowed-population parse screen failed', 'included_jobs': [x['job_id'] for x in self.items],
            'repository_check_returncode': 0, 'inputs': str(self.run), 'baseline_sha256': r.digest(self.baseline),
            'checkpoint_manifest_sha256': r.digest(self.inputs/'manifest.json'),
            'validation_manifest_sha256': r.digest(self.run/'manifest.json'),
            'parse_screen': original_screen, 'parse_screen_result_sha256': r.digest(self.original/'result.json')}
        write(self.validation, validation)
        with closing(self.queue._connect()) as db:
            for item in self.items:
                db.execute('UPDATE jobs SET adjudication=? WHERE id=?',
                           (json.dumps({'status': 'integration-rejected', 'evidence': {'manifest': 'batch', 'validation': validation}}), item['job_id']))
        self.log.write_text(''.join(f'{name} passed 1.0 s\n' for name in ('fmt','pin','hygiene','tools','clippy','build','families','test')))
        controls = {}
        for label, indices, tree in (('base', [], trees[0]), ('full', [0,1,2], trees[1]), ('remaining', [0,1], {'test.ebnf': None})):
            root = self.root/'.cache'/f'focus-{label}'; (root/'grammar').mkdir(parents=True)
            (root/'grammar/test.ebnf').write_text(label if label != 'remaining' else 'remaining')
            identity = {'method':'ordered-proposal-replay-v1','indices':indices,'tree':r.tree(root/'grammar'),
                        'proposals':[{'job_id':self.items[i]['job_id'],'attempt_id':self.items[i]['attempt_id']}for i in indices]}
            write(root/'identity.json', identity); checks = root/'checks'; checks.mkdir()
            focused_addition = dict(addition, occurrence=0) if label == 'full' else None
            result = self.make_checks(checks, {'A', 'B'}, focused_addition)
            write(root/'result.json', result)
            controls[label] = {'identity':spec(root/'identity.json'),'result':spec(root/'result.json'),
                'population':spec(checks/'slice.json'),'artifacts':{name:spec(checks/name)for name in parsed.ARTIFACTS}}
        profile = self.root/'.cache/profile.json'; write(profile, {'cards':['A']})
        originals = self.root/'.cache/original-reports.json'; write(originals, {name:spec(path)for name,path in self.reports.items()})
        write(self.isolation, {'kind':'isolated-parse-screen-regressions','manifest':'batch','candidate':'candidate',
            'parent':'parent','excluded_jobs':['culprit'],'reason':'explicit isolated addition','profile':spec(profile),
            'population_count':2,'controls':controls,'original_reports':spec(originals)})
        return {'kind':'isolated-parse-screen-revalidation','manifest':spec(manifest_path), 'validation':spec(self.validation),
                'repository_log':spec(self.log),'isolation':spec(self.isolation),
                'reports':{name:spec(self.reports[name])for name in ('author','mechanical')}}

    def make_checks(self, directory, names, addition):
        worker.subset(directory, names, self.data)
        rows = [dict(entry['parse'])for entry in self.data[0].values()if entry['name'] in names]
        if addition: rows[addition['occurrence']]['readings'] = 2
        r.jsonl(directory/'rows.jsonl', rows)
        r.write(directory/'native.json', native_report(rows,len(rows),[addition]if addition else []))
        (directory/'native-failures.jsonl').write_text('')
        r.jsonl(directory/'commands.jsonl',[{'program':'score','returncode':0},{'program':'validate-readings','returncode':0}])
        for name in ('score.log','validate-readings.log'): (directory/name).write_text('complete\n')
        return {'faces':len(rows),'failures':worker.native_failures(r.read(directory/'native.json'),len(rows))
                +worker.compare(rows,self.data[0],set(names))}

    def invoke(self, proof):
        return self.queue.readmit_parse_screen_batch_proposals(['author','mechanical'],'parent',proof)

    def test_parse_readmission_is_atomic_nonacceptance_and_replays_both_trees(self):
        proof = self.fixture(); originals = [self.queue.get_attempt(self.attempt(name))for name in ('author','mechanical')]
        with patch.object(parsed.library,'_replay')as replay:
            self.invoke(proof)
        self.assertEqual(replay.call_count,2)
        self.assertEqual([call.args[3]for call in replay.call_args_list],[[0,1,2],[0,1]])
        self.assertTrue(all(call.kwargs == {'canonical_mechanical':True}for call in replay.call_args_list))
        self.assertEqual(self.queue.coverage()['counts'],{'provisional-pass':2,'integration-rejected':1})
        self.assertEqual(self.queue.coverage()['accepted_rules'],0)
        self.assertEqual([self.queue.get_attempt(self.attempt(name))for name in ('author','mechanical')],originals)
        self.assertEqual(self.queue.audit()[-1]['kind'],'readmitted-parse-screen-batch')
        self.assertEqual(driver.recovered_cohort(self.queue,'parent')['jobs'],['author','mechanical'])
        self.queue.reject_jobs(['author','mechanical'],{'manifest':'batch','validation':r.read(self.validation)})
        with patch.object(parsed.library,'_replay'),self.assertRaisesRegex(ValueError,'already used'):
            self.invoke(proof)

    def test_missing_extra_duplicate_or_stale_selection_refuses_without_changes(self):
        proof = self.fixture()
        for jobs, cp in ((['author'],'parent'),(['author','mechanical','culprit'],'parent'),
                         (['author','author'],'parent'),(['author','mechanical'],'stale')):
            with self.subTest(jobs=jobs,cp=cp),patch.object(parsed.library,'_replay'),self.assertRaises(ValueError):
                self.queue.readmit_parse_screen_batch_proposals(jobs,cp,proof)
        self.assertEqual(self.queue.coverage()['counts'],{'integration-rejected':3})

    def test_focused_baseline_print_loss_and_wrong_population_refuse(self):
        proof = self.fixture(); iso = r.read(self.isolation)
        for mutation in ('print-loss','population','baseline','native','refused'):
            with self.subTest(mutation=mutation):
                control = iso['controls']['remaining']; checks = Path(control['identity']['path']).parent/'checks'
                saved = {p:p.read_bytes()for p in checks.iterdir()if p.is_file()}
                if mutation == 'print-loss':
                    rows = r.rows(checks/'rows.jsonl'); rows[0]['exact'] = False; r.jsonl(checks/'rows.jsonl',rows)
                elif mutation == 'population': write(checks/'slice.json',{'cards':['A']})
                elif mutation == 'baseline': (checks/'before.jsonl').write_text('[]\n')
                elif mutation == 'native':
                    native=r.read(checks/'native.json');native['readings']['valid']=0;write(checks/'native.json',native)
                else:
                    rows=r.rows(checks/'rows.jsonl');rows[0].update(status='refused',readings=0);r.jsonl(checks/'rows.jsonl',rows)
                control['artifacts']={name:spec(checks/name)for name in parsed.ARTIFACTS};control['population']=spec(checks/'slice.json')
                write(self.isolation,iso);proof['isolation']=spec(self.isolation)
                with patch.object(parsed.library,'_replay'),self.assertRaises(ValueError):self.invoke(proof)
                for path,data in saved.items():path.write_bytes(data)
        self.assertEqual(self.queue.coverage()['counts'],{'integration-rejected':3})

    def test_original_fullscreen_cannot_be_replaced_with_full_render_contract(self):
        proof=self.fixture();v=r.read(self.validation);v['reason']='full allowed-population checks failed';write(self.validation,v);proof['validation']=spec(self.validation)
        with patch.object(parsed.library,'_replay'),self.assertRaisesRegex(ValueError,'original additions-only'):
            self.invoke(proof)

    def test_artifact_change_after_replay_and_checkpoint_or_row_race_refuse_atomic(self):
        proof=self.fixture()
        def change(*args,**kwargs):
            (self.root/'.cache/focus-remaining/checks/score.log').write_text('changed')
        with patch.object(parsed.library,'_replay',side_effect=change),self.assertRaisesRegex(ValueError,'changed'):
            self.invoke(proof)
        self.assertEqual(self.queue.coverage()['counts'],{'integration-rejected':3})

    def test_missing_original_report_or_incorrect_exact_added_indices_refuse(self):
        proof=self.fixture();iso=r.read(self.isolation)
        report_path=Path(iso['original_reports']['path']);reports=r.read(report_path);del reports['culprit'];write(report_path,reports)
        iso['original_reports']=spec(report_path);write(self.isolation,iso);proof['isolation']=spec(self.isolation)
        with patch.object(parsed.library,'_replay'),self.assertRaisesRegex(ValueError,'all original included'):
            self.invoke(proof)


    def test_exact_added_reading_indices_cannot_be_swapped(self):
        proof=self.fixture();iso=r.read(self.isolation);control=iso['controls']['full']
        checks=Path(control['identity']['path']).parent/'checks';native=r.read(checks/'native.json')
        native['reading_preservation']['additions'][0].update(added_current_indices=[0],retained_current_indices=[1])
        write(checks/'native.json',native);control['artifacts']['native.json']=spec(checks/'native.json')
        write(self.isolation,iso);proof['isolation']=spec(self.isolation)
        with patch.object(parsed.library,'_replay'),self.assertRaisesRegex(ValueError,'exactly reproduce'):
            self.invoke(proof)

    def test_artifact_recheck_and_current_row_checkpoint_ownership_races_are_atomic(self):
        for mode in ('artifact','frozen','row','checkpoint','ownership'):
            with self.subTest(mode=mode):
                if mode != 'artifact': self.setUp()
                proof=self.fixture();verify=parsed.verify
                def verify_then_race(*args,**kwargs):
                    value=verify(*args,**kwargs)
                    if mode=='artifact':
                        (self.root/'.cache/focus-remaining/checks/score.log').write_text('late mutation')
                    elif mode=='frozen':
                        (self.inputs/'frozen/bin'/r.HELPERS[0]).write_text('late frozen mutation')
                    else:
                        with closing(self.queue._connect())as db:
                            if mode=='row':
                                db.execute("UPDATE attempts SET result=? WHERE id=?", ('{}',self.attempt('mechanical')))
                            elif mode=='checkpoint':
                                db.execute("INSERT INTO checkpoints(commit_id,parent,evidence)VALUES('later','parent','{}')")
                            else:db.execute("UPDATE rules SET job_id='culprit' WHERE rule='a'")
                    return value
                with patch.object(parsed.library,'_replay'),patch.object(parsed,'verify',side_effect=verify_then_race),self.assertRaises(ValueError):
                    self.invoke(proof)
                self.assertTrue(all(j['status']=='integration-rejected'for j in self.queue.get_jobs()))
                self.assertFalse(any(e['kind']=='readmitted-parse-screen-batch'for e in self.queue.audit()))

    def test_cross_mode_readmission_event_blocks_repeat(self):
        proof=self.fixture()
        with closing(self.queue._connect())as db:
            self.queue._event(db,'readmitted-library-batch',{'job_id':'mechanical','manifest':'batch'})
        with patch.object(parsed.library,'_replay'),self.assertRaisesRegex(ValueError,'already used'):
            self.invoke(proof)
        self.assertEqual(self.queue.coverage()['counts'],{'integration-rejected':3})

    def test_replayed_remaining_tree_must_match_and_original_reports_are_pinned(self):
        proof=self.fixture()
        with patch.object(parsed.library,'_replay',side_effect=ValueError('independent ordered replay differs')),self.assertRaisesRegex(ValueError,'ordered replay'):
            self.invoke(proof)
        self.reports['mechanical'].write_text('{}')
        with patch.object(parsed.library,'_replay'),self.assertRaisesRegex(ValueError,'hash differs'):
            self.invoke(proof)


    def test_derivation_key_bridge_is_top_level_only_and_shape_bounded(self):
        old={'name':'A','text':'a','flags':[True]*4,'parse':{'status':'equal','readings':1,'key':['mined.2'],
              'ir':{'actor':'you','amount':2,'key':['semantic-key']}}}
        current=copy.deepcopy(old);current['parse']['key']=['mined.1']
        self.assertEqual(parsed._baseline_bridge([old],[(9,current)]),
                         [{'occurrence':9,'face':'A','control_key':['mined.2'],'screen_key':['mined.1']}])
        for field,value in (('actor','opponent'),('amount',3),('key',['changed-semantic-key'])):
            changed=copy.deepcopy(current);changed['parse']['ir'][field]=value
            with self.subTest(field=field),self.assertRaisesRegex(ValueError,'meanings, wording or print flags'):
                parsed._baseline_bridge([old],[(9,changed)])
        for change in (lambda x:x.update(text='new'),lambda x:x.update(flags=[False]*4),
                       lambda x:x['parse'].update(readings=2),lambda x:x['parse'].update(key={'not':'metadata'}),
                       lambda x:x['parse'].update(key=[3])):
            changed=copy.deepcopy(current);change(changed)
            with self.assertRaises(ValueError):parsed._baseline_bridge([old],[(9,changed)])

    def test_unlisted_or_forged_control_derivation_key_bridge_refuses(self):
        proof=self.fixture();iso=r.read(self.isolation)
        iso['derivation_key_changes']=[{'occurrence':99,'face':'foreign','control_key':['old'],'screen_key':['new']}]
        write(self.isolation,iso);proof['isolation']=spec(self.isolation)
        with patch.object(parsed.library,'_replay'),self.assertRaisesRegex(ValueError,'declared derivation-key bridge'):
            self.invoke(proof)
        del iso['derivation_key_changes'];control=iso['controls']['base'];checks=Path(control['identity']['path']).parent/'checks'
        rows=r.rows(checks/'before.jsonl');rows[0]['key']=['forged'];r.jsonl(checks/'before.jsonl',rows)
        control['artifacts']['before.jsonl']=spec(checks/'before.jsonl');write(self.isolation,iso);proof['isolation']=spec(self.isolation)
        with patch.object(parsed.library,'_replay'),self.assertRaisesRegex(ValueError,'baseline differs from frozen subset'):
            self.invoke(proof)


class CanonicalReplayModeTests(unittest.TestCase):
    def test_new_mode_uses_full_canonical_removal_and_legacy_mode_keeps_its_contract(self):
        import mined
        import retire_batch as batch
        import retire_library_readmission as library
        import retire_queue as queue_module
        with tempfile.TemporaryDirectory()as temporary:
            root=Path(temporary);inputs=root/'inputs';(inputs/'frozen/grammar/syntax').mkdir(parents=True)
            (inputs/'frozen/grammar/syntax/mined.ebnf').write_text('mined_action ::= m_a | never_action\nm_a ::= "a"\nnever_action ::= "never"\n')
            (inputs/'frozen/grammar/syntax/cost.ebnf').write_text('dispatcher ::= m_a | other\nother ::= "other"\n')
            attempt_dir=root/'attempt';(attempt_dir/'inputs').mkdir(parents=True)
            shutil.copytree(inputs/'frozen',attempt_dir/'inputs/frozen')
            evaluated=root/'evaluated';evaluated.mkdir();shutil.copytree(inputs/'frozen/grammar',evaluated/'grammar')
            mined.remove(evaluated,['m_a'])
            from retire_queue import Queue
            queue=Queue(root/'queue.sqlite');queue.add_checkpoint('parent','proof')
            queue.add_jobs([{'id':'mechanical','kind':'mechanical','rules':['m_a']}]);claim=queue.claim('worker')
            queue.finish(claim['attempt_id'],'provisional-pass',{'kind':'mechanical','run':str(attempt_dir),
                'report':str(evaluated/'report.json'),'dispositions':{'m_a':{'status':'proposed','reason':'checked','evidence':'proof'}}},
                {'m_a':{'status':'pass','evidence':'proof'}})
            write(root/'checkpoints/parent.json',{'inputs':str(inputs),'cpu_pool':str(root/'cpu'),'cpu_capacity':32})
            report={'task':'mechanical','source':'parent','descriptor_sha256':'D','support_sha256':'S','faces':1}
            write(evaluated/'report.json',report)
            included=[{'job_id':'mechanical','attempt_id':claim['attempt_id'],'base_commit':'parent','kind':'mechanical',
                       'rules':['m_a'],'descriptor_sha256':'D','support_sha256':'S'}]
            proposal=(attempt_dir,{'id':'mechanical','rules':['m_a']},evaluated,[])
            with patch.object(queue_module,'_verify_mechanical_readmission',return_value=report), \
                    patch.object(batch,'_proposal',return_value=proposal), \
                    patch.object(library, '_grammar_tree',wraps=library._grammar_tree), \
                    patch.object(batch.merger,'merge_grammar',return_value={'grammar/syntax/cost.ebnf':'canonical ordinary delta'})as merge:
                library._replay(root,'parent',included,[0],r.tree(evaluated/'grammar'),root,{},canonical_mechanical=True)
                merge.assert_not_called()
                with self.assertRaisesRegex(ValueError,'mechanical proposal contains additional edits'):
                    library._replay(root,'parent',included,[0],r.tree(evaluated/'grammar'),root,{})


if __name__=='__main__':unittest.main()
