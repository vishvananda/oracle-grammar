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
    def fixture(self, *, refusal=False, mixed=False):
        folder = self.root/'.cache/batches/batch'
        self.old = [{'name': n, 'text': n.lower(), 'status': 'equal', 'readings': 1, 'exact': True}
                    for n in ('C', 'A', 'B')]
        if mixed:
            self.old[2]['readings'] = 2
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
        self.make_checks(self.original, {'A', 'B', 'C'}, None if refusal else addition,
                         refusal=1 if refusal else None, loss=2 if mixed else None)
        original_screen = screen.assess(self.run, self.original, self.data, int(refusal or mixed))
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
            focused_addition = dict(addition, occurrence=0) if label == 'full' and not refusal else None
            result = self.make_checks(checks, {'A', 'B'}, focused_addition,
                                      refusal=0 if label == 'full' and refusal else None,
                                      loss=1 if label == 'full' and mixed else None)
            write(root/'result.json', result)
            controls[label] = {'identity':spec(root/'identity.json'),'result':spec(root/'result.json'),
                'population':spec(checks/'slice.json'),'artifacts':{name:spec(checks/name)for name in parsed.ARTIFACTS}}
        profile = self.root/'.cache/profile.json'; write(profile, {'cards':['A','B'] if mixed else ['A']})
        originals = self.root/'.cache/original-reports.json'; write(originals, {name:spec(path)for name,path in self.reports.items()})
        write(self.isolation, {'kind':'isolated-parse-screen-regressions','manifest':'batch','candidate':'candidate',
            'parent':'parent','excluded_jobs':['culprit'],'reason':'explicit isolated addition','profile':spec(profile),
            'population_count':2,'controls':controls,'original_reports':spec(originals)})
        return {'kind':'isolated-parse-screen-revalidation','manifest':spec(manifest_path), 'validation':spec(self.validation),
                'repository_log':spec(self.log),'isolation':spec(self.isolation),
                'reports':{name:spec(self.reports[name])for name in ('author','mechanical')}}

    def make_checks(self, directory, names, addition, *, refusal=None, loss=None):
        worker.subset(directory, names, self.data)
        rows = [dict(entry['parse'])for entry in self.data[0].values()if entry['name'] in names]
        old_count = sum(row['readings'] for row in rows)
        if addition: rows[addition['occurrence']]['readings'] = 2
        if refusal is not None:
            rows[refusal].update(status='refused', readings=0)
        if loss is not None:
            rows[loss]['readings'] -= 1
        r.jsonl(directory/'rows.jsonl', rows)
        native = native_report(rows,old_count,[addition]if addition else [])
        if refusal is not None:
            for count in ('recognized_faces', 'faces_with_any_valid_reading', 'faces_with_all_readings_valid'):
                native[count] -= 1
            native['reading_preservation'].update(passed=False, missing=[{'name':rows[refusal]['name'],'reading':0}])
        if loss is not None:
            native['reading_preservation'].update(passed=False, missing=[{'name':rows[loss]['name'],'reading':0}])
        r.write(directory/'native.json', native)
        (directory/'native-failures.jsonl').write_text('')
        r.jsonl(directory/'commands.jsonl',[{'program':'score','returncode':0},
                                          {'program':'validate-readings','returncode':int(refusal is not None or loss is not None)}])
        for name in ('score.log','validate-readings.log'): (directory/name).write_text('complete\n')
        return {'faces':len(rows),'failures':worker.native_failures(r.read(directory/'native.json'),len(rows))
                +worker.compare(rows,self.data[0],set(names))}

    def invoke(self, proof):
        return self.queue.readmit_parse_screen_batch_proposals(['author','mechanical'],'parent',proof)

    def test_readmits_survivors_after_reproducing_the_original_refusal(self):
        proof = self.fixture(refusal=True)
        with patch.object(parsed.library, '_replay'):
            result = self.invoke(proof)
        self.assertEqual(result['proof']['proof_scope'], 'refusal-only parse-screen isolation')
        self.assertTrue(result['proof']['requires_fresh_full_validation'])
        self.assertEqual(self.queue.coverage()['counts'], {'provisional-pass': 2, 'integration-rejected': 1})

    def test_refusal_cannot_explain_a_missing_reading_on_a_recognized_face(self):
        self.fixture(refusal=True)
        rows = r.rows(self.original/'rows.jsonl')
        rows[1].update(status='equal', readings=1)
        with self.assertRaisesRegex(ValueError, 'match the missing native readings'):
            parsed._refusals(enumerate(self.data[0].values()), rows, [{'name':'A','reading':0}])

    def test_readmits_survivors_after_reproducing_the_exact_mixed_delta(self):
        proof = self.fixture(mixed=True)
        with patch.object(parsed.library, '_replay'):
            result = self.invoke(proof)
        self.assertEqual(result['proof']['proof_scope'], 'mixed reading losses and additions in parse-screen isolation')
        self.assertTrue(result['proof']['requires_fresh_full_validation'])
        self.assertEqual(self.queue.coverage()['counts'], {'provisional-pass': 2, 'integration-rejected': 1})

    def test_mixed_delta_rejects_a_different_missing_reading_of_the_same_face(self):
        proof = self.fixture(mixed=True)
        isolation = r.read(self.isolation)
        control = isolation['controls']['full']
        native_path = Path(control['artifacts']['native.json']['path'])
        native = r.read(native_path)
        native['reading_preservation']['missing'][0]['reading'] = 1
        write(native_path, native)
        control['artifacts']['native.json'] = spec(native_path)
        write(self.isolation, isolation)
        proof['isolation'] = spec(self.isolation)
        with self.assertRaisesRegex(ValueError, 'same reading losses and additions'):
            self.invoke(proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})

    def full_fixture(self):
        proof = self.fixture()
        folder = self.original.parent/'full-checks'
        self.original.rename(folder)
        isolation = r.read(self.isolation)
        full = isolation['controls']['full']
        focus = Path(full['identity']['path']).parent/'checks'
        for checks in (folder, focus):
            rows = r.rows(checks/'rows.jsonl')
            next(row for row in rows if row['name'] == 'A')['default'] = 'a'
            r.jsonl(checks/'rows.jsonl', rows)
        focused_report = r.read(Path(full['result']['path']))
        focused_report['failures'].append({'face':'A','lost':['exact_default']})
        write(Path(full['result']['path']), focused_report)
        full['result'] = spec(Path(full['result']['path']))
        full['artifacts']['rows.jsonl'] = spec(focus/'rows.jsonl')
        isolation['kind'] = 'isolated-full-check-regressions'
        write(self.isolation, isolation)
        validation = r.read(self.validation)
        for key in ('parse_screen', 'parse_screen_result_sha256',
                    'checkpoint_manifest_sha256', 'validation_manifest_sha256'):
            del validation[key]
        validation['reason'] = 'full allowed-population checks failed'
        validation['full_checks'] = {**focused_report, 'faces':3}
        write(self.validation, validation)
        with closing(self.queue._connect()) as db:
            for item in self.items:
                db.execute('UPDATE jobs SET adjudication=? WHERE id=?',
                           (json.dumps({'status':'integration-rejected',
                                       'evidence':{'manifest':'batch','validation':validation}}), item['job_id']))
        return dict(proof, kind='isolated-full-check-revalidation', source_checkpoint='parent',
                    validation=spec(self.validation), isolation=spec(self.isolation),
                    archive_manifests={'checkpoint_manifest_sha256':spec(self.inputs/'manifest.json'),
                                       'validation_manifest_sha256':spec(self.run/'manifest.json')},
                    full_artifacts={name:spec(folder/name) for name in parsed.ARTIFACTS})

    def test_archived_full_check_uses_ancestor_and_queues_exact_survivors(self):
        proof = self.full_fixture()
        self.queue.add_checkpoint('current', 'evidence', expected_parent='parent')
        with patch.object(parsed.library, '_replay') as replay:
            result = self.queue.readmit_full_check_batch_proposals(['author','mechanical'], 'current', proof)
        self.assertEqual([call.args[1] for call in replay.call_args_list], ['parent','parent'])
        self.assertEqual(result['proof']['source_checkpoint'], 'parent')
        self.assertEqual(result['proof']['proof_scope'], 'archived full-check isolation')
        self.assertEqual(self.queue.coverage()['accepted_rules'], 0)
        self.assertEqual(driver.recovered_cohort(self.queue,'current'),
                         {'manifest':'batch','checkpoint':'current','jobs':['author','mechanical']})

    def _downgrade_checkpoint_print_rows(self):
        frozen = self.inputs/'frozen'
        index = r.read(frozen/'index.json')
        earlier = next(entry for entry in index.values() if entry['name'] == 'A')
        earlier['parse']['default'] = 'a'
        earlier['flags'] = r.flags(earlier['parse'])
        self.assertFalse(earlier['flags'][3])
        r.write(frozen/'index.json', index)
        r.jsonl(frozen/'parse.jsonl', [entry['parse'] for entry in index.values()])
        manifest_path = self.inputs/'manifest.json'
        manifest = r.read(manifest_path)
        manifest['files'] = r.tree(frozen)
        r.write(manifest_path, manifest)
        return manifest_path

    def test_parse_controls_use_frozen_validation_rows_with_new_baseline(self):
        proof = self.fixture()
        manifest_path = self._downgrade_checkpoint_print_rows()
        validation = r.read(self.validation)
        validation['checkpoint_manifest_sha256'] = r.digest(manifest_path)
        write(self.validation, validation)
        proof['validation'] = spec(self.validation)
        with closing(self.queue._connect()) as db:
            for item in self.items:
                db.execute('UPDATE jobs SET adjudication=? WHERE id=?',
                           (json.dumps({'status': 'integration-rejected', 'evidence': {'manifest': 'batch',
                                         'validation': validation}}), item['job_id']))
        for baseline, reason in ((None, 'baseline meanings'), ('other', 'control baseline is invalid')):
            with self.subTest(baseline=baseline):
                isolation = r.read(self.isolation)
                if baseline is not None:
                    isolation['control_baseline'] = baseline
                write(self.isolation, isolation)
                proof['isolation'] = spec(self.isolation)
                with self.assertRaisesRegex(ValueError, reason):
                    self.invoke(proof)
        isolation['control_baseline'] = parsed.VALIDATION_INPUTS
        write(self.isolation, isolation)
        proof['isolation'] = spec(self.isolation)
        with patch.object(parsed.library, '_replay'):
            receipt = self.invoke(proof)
        self.assertEqual(receipt['jobs'], ['author', 'mechanical'])
        self.assertEqual(receipt['proof']['derivation_key_changes'], [])

    def test_archived_full_controls_use_accepted_print_rows(self):
        proof = self.full_fixture()
        manifest_path = self._downgrade_checkpoint_print_rows()
        proof['archive_manifests']['checkpoint_manifest_sha256'] = spec(manifest_path)
        with patch.object(parsed.library, '_replay'):
            result = self.queue.readmit_full_check_batch_proposals(['author','mechanical'], 'parent', proof)
        self.assertEqual(result['proof']['proof_scope'], 'archived full-check isolation')
        self.assertEqual(result['proof']['derivation_key_changes'], [])

    def test_archived_full_requires_frozen_original_population(self):
        proof = self.full_fixture()
        path = Path(proof['full_artifacts']['oracle.json']['path'])
        path.write_text('[]')
        proof['full_artifacts']['oracle.json'] = spec(path)
        with patch.object(parsed.library, '_replay'), self.assertRaisesRegex(ValueError,'frozen population'):
            self.queue.readmit_full_check_batch_proposals(['author','mechanical'], 'parent', proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected':3})

    def test_archived_full_control_must_reproduce_print_loss_as_well_as_addition(self):
        proof = self.full_fixture()
        isolation = r.read(self.isolation)
        control = isolation['controls']['full']
        rows_path = Path(control['artifacts']['rows.jsonl']['path'])
        rows = r.rows(rows_path)
        del rows[0]['default']
        r.jsonl(rows_path, rows)
        control['artifacts']['rows.jsonl'] = spec(rows_path)
        result_path = Path(control['result']['path'])
        result = r.read(result_path)
        result['failures'].pop()
        write(result_path, result)
        control['result'] = spec(result_path)
        write(self.isolation, isolation)
        proof['isolation'] = spec(self.isolation)
        with patch.object(parsed.library, '_replay'), self.assertRaisesRegex(ValueError,'exact archived failures'):
            self.queue.readmit_full_check_batch_proposals(['author','mechanical'], 'parent', proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected':3})

    def test_parse_readmission_is_atomic_nonacceptance_and_replays_both_trees(self):
        proof = self.fixture(); originals = [self.queue.get_attempt(self.attempt(name))for name in ('author','mechanical')]
        with patch.object(parsed.library,'_replay')as replay:
            self.invoke(proof)
        self.assertEqual(replay.call_count,2)
        self.assertEqual([call.args[3]for call in replay.call_args_list],[[0,1,2],[0,1]])
        self.assertTrue(all(call.kwargs['canonical_mechanical'] is True for call in replay.call_args_list))
        full, remaining = (call.kwargs['input_cache'] for call in replay.call_args_list)
        self.assertIs(full.snapshots, remaining.snapshots)
        self.assertIs(full.removals, remaining.removals)
        self.assertIsNot(full.locations, remaining.locations)
        self.assertEqual(self.queue.coverage()['counts'],{'provisional-pass':2,'integration-rejected':1})
        self.assertEqual(self.queue.coverage()['accepted_rules'],0)
        self.assertEqual([self.queue.get_attempt(self.attempt(name))for name in ('author','mechanical')],originals)
        self.assertEqual(self.queue.audit()[-1]['kind'],'readmitted-parse-screen-batch')
        self.assertEqual(driver.recovered_cohort(self.queue,'parent')['jobs'],['author','mechanical'])
        self.queue.reject_jobs(['author','mechanical'],{'manifest':'batch','validation':r.read(self.validation)})
        with patch.object(parsed.library,'_replay'),self.assertRaisesRegex(ValueError,'already used'):
            self.invoke(proof)

    def test_historical_origin_replays_original_proof_and_queues_against_current_checkpoint(self):
        proof = dict(self.fixture(), source_checkpoint='parent')
        self.queue.add_checkpoint('middle', 'evidence', expected_parent='parent')
        self.queue.add_checkpoint('current', 'evidence', expected_parent='middle')
        with patch.object(parsed.library, '_replay') as replay:
            result = self.queue.readmit_parse_screen_batch_proposals(['author', 'mechanical'], 'current', proof)
        self.assertEqual([call.args[1] for call in replay.call_args_list], ['parent', 'parent'])
        self.assertEqual(result['checkpoint'], 'current')
        self.assertEqual(result['proof']['source_checkpoint'], 'parent')
        self.assertTrue(result['proof']['requires_fresh_full_validation'])
        self.assertEqual(self.queue.coverage()['accepted_rules'], 0)
        cohort = driver.recovered_cohort(self.queue, 'current')
        self.assertEqual(cohort, {'manifest': 'batch', 'checkpoint': 'current', 'jobs': ['author', 'mechanical']})

    def test_historical_source_requires_accepted_ancestor_and_matching_rejection(self):
        proof = self.fixture()
        self.queue.add_checkpoint('middle', 'evidence', expected_parent='parent')
        self.queue.add_checkpoint('current', 'evidence', expected_parent='middle')
        for source, error in (('unknown', 'accepted checkpoint chain'), ('middle', 'original additions-only'),
                              ('../parent', 'invalid batch readmission origin')):
            with self.subTest(source=source), self.assertRaisesRegex(ValueError, error):
                self.queue.readmit_parse_screen_batch_proposals(
                    ['author', 'mechanical'], 'current', dict(proof, source_checkpoint=source))
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})

    def test_historical_readmission_refuses_if_checkpoint_chain_changes_during_replay(self):
        proof = dict(self.fixture(), source_checkpoint='parent')
        self.queue.add_checkpoint('middle', 'evidence', expected_parent='parent')
        self.queue.add_checkpoint('current', 'evidence', expected_parent='middle')

        def break_chain(*args, **kwargs):
            with closing(self.queue._connect()) as db:
                db.execute("UPDATE checkpoints SET parent=NULL WHERE commit_id='middle'")

        with patch.object(parsed.library, '_replay', side_effect=break_chain):
            with self.assertRaisesRegex(ValueError, 'accepted checkpoint chain'):
                self.queue.readmit_parse_screen_batch_proposals(['author', 'mechanical'], 'current', proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})

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
        for mode in ('artifact','frozen','row','checkpoint','ownership','attempt-job','attempt-worker'):
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
                            elif mode=='attempt-job':
                                db.execute("UPDATE attempts SET job_id='culprit' WHERE id=?",(self.attempt('mechanical'),))
                            elif mode=='attempt-worker':
                                db.execute("UPDATE attempts SET worker='changed-worker' WHERE id=?",(self.attempt('mechanical'),))
                            elif mode=='checkpoint':
                                db.execute("INSERT INTO checkpoints(commit_id,parent,evidence)VALUES('later','parent','{}')")
                            else:db.execute("UPDATE rules SET job_id='culprit' WHERE rule='a'")
                    return value
                with patch.object(parsed.library,'_replay'),patch.object(parsed,'verify',side_effect=verify_then_race),self.assertRaises(ValueError):
                    self.invoke(proof)
                self.assertTrue(all(j['status']=='integration-rejected'for j in self.queue.get_jobs()))
                self.assertFalse(any(e['kind']=='readmitted-parse-screen-batch'for e in self.queue.audit()))

    def test_initial_attempt_must_belong_to_selected_job(self):
        proof=self.fixture()
        with closing(self.queue._connect())as db:
            db.execute("UPDATE attempts SET job_id='culprit' WHERE id=?",(self.attempt('mechanical'),))
        with patch.object(parsed.library,'_replay')as replay,self.assertRaisesRegex(ValueError,'current rejected attempts'):
            self.invoke(proof)
        replay.assert_not_called()
        self.assertEqual(self.queue.coverage()['counts'],{'integration-rejected':3})
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
