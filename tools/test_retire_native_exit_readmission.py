import json
from contextlib import closing
from pathlib import Path
import subprocess
from unittest.mock import patch

import retire_native_exit_readmission as native
from retire_queue import _proof_file, _gates
from test_retire_batch_readmission import BatchReadmissionTests, spec, write


class NativeExitReadmissionTests(BatchReadmissionTests):
    def exit_fixture(self):
        proof = self.native_fixture()
        folder = self.root/'.cache/batches/batch'
        manifest = json.loads((folder/'manifest.json').read_text())
        inputs = self.root/'checkpoints/parent/inputs'
        base = json.loads((inputs/'manifest.json').read_text());base['source']='source'
        for name in ('allowed.json','gold.jsonl','oracle.json'):
            path=inputs/'frozen'/name;path.write_text('same '+name);base['files'][name]=spec(path)['sha256']
        write(inputs/'frozen/build.json',{'source_sha256':'source','compiler':{'rustc':'test'}})
        write(inputs/'manifest.json',base)
        baseline=self.root/'checkpoints/parent/baseline.jsonl';row={'name':'N','text':'N','status':'equal','readings':1,'exact':True}
        baseline.write_text(json.dumps(row)+'\n')
        write(self.root/'checkpoints/parent.json',{'commit':'parent','inputs':str(inputs),'baseline':str(baseline)})
        self.native_data=({'0': {'name':'N','text':'N','parse':row,'flags':[True,True,True,True]}},[],[])
        run = folder/'validation-inputs';grammar=run/'frozen/grammar';grammar.mkdir(parents=True)
        (grammar/'test.ebnf').write_text('candidate grammar')
        candidate_files={'grammar/test.ebnf':spec(grammar/'test.ebnf')['sha256']}
        for name in ('allowed.json','gold.jsonl','oracle.json'):
            path=run/'frozen'/name;path.write_text('same '+name);candidate_files[name]=spec(path)['sha256']
        (run/'frozen/parse.jsonl').write_text(baseline.read_text())
        for name in ('score','validate-readings','rule-audit'):
            candidate_files['bin/'+name]='hash-'+name
        write(run/'frozen/build.json',{'source_sha256':'source','compiler':{'rustc':'test'},
                                       'binaries':{name:'hash-'+name for name in ('score','validate-readings','rule-audit')}})
        write(run/'manifest.json',{'commit':'candidate','population':1,'source':'source','files':candidate_files})
        checks=folder/'full-checks';checks.mkdir()
        for name in ('commands.jsonl','native.json','native-failures.jsonl','slice.json','rows.jsonl','before.jsonl','gold.jsonl','oracle.json'):
            (checks/name).write_text('historical '+name)
        cmd=[str((run/'frozen/bin/validate-readings').resolve()),str(checks/'gold.jsonl'),str(checks/'rows.jsonl'),str(checks/'native.json'),str(checks/'native-failures.jsonl'),'--preserve-from',str(checks/'before.jsonl')]
        validation={'status':'failed','scope':'combined-batch','reason':str(subprocess.CalledProcessError(1,cmd)),
                    'repository_check_returncode':0,'candidate':'candidate','source_checkpoint':'parent',
                    'included_jobs':[x['job_id'] for x in self.items],'inputs':str(run),'baseline_sha256':spec(baseline)['sha256']}
        write(self.validation,validation)
        with closing(self.queue._connect()) as db:
            for item in self.items:
                db.execute('update jobs set adjudication=? where id=?',(json.dumps({'status':'integration-rejected','evidence':{'manifest':'batch','validation':validation}}),item['job_id']))
        files=[folder/'manifest.json',folder/'validation.json',run/'manifest.json',*[checks/name for name in ('commands.jsonl','native.json','native-failures.jsonl','slice.json','rows.jsonl','before.jsonl','gold.jsonl','oracle.json')]]
        supplement=self.root/'.cache/native-supplement.json';full={'faces':1,'failures':[{'face':'N','lost':['exact_default']}]}
        write(supplement,{'kind':'historical-native-exit-one-supplement','batch':'batch','candidate':'candidate',
                          'checkpoint':'parent','recorded_reason':validation['reason'],'full_checks':full,
                          'input_sha256':{str(p):spec(p)['sha256'] for p in files}})
        isolation=self.root/'.cache/native-isolation.json';value=json.loads(isolation.read_text())
        value.update(kind='isolated-native-regressions-exit-one', historical_supplement=spec(supplement))
        for name in ('full_replay','original_reports','control_provenance'):
            path=self.root/'.cache'/(name+'.json');write(path,{});value[name]=spec(path)
        write(isolation,value)
        proof.update(kind='isolated-native-exit-one-revalidation',validation=spec(self.validation),
                     isolation=spec(isolation),supplement=spec(supplement),
                     reports={name:spec(self.reports[name]) for name in ('author','mechanical')})
        return proof,full

    def verified(self,proof,full):
        self.stack.enter_context(patch.object(native.r,'verify'))
        self.stack.enter_context(patch.object(native.worker,'payload',return_value=self.native_data))
        self.stack.enter_context(patch.object(native.retire_native_outcome,'preservation_failure',return_value=full))
        return self.stack.enter_context(patch.object(native.library,'_replay'))

    def test_atomic_native_exit_revalidation_and_once(self):
        from contextlib import ExitStack
        self.stack=ExitStack()
        with self.stack:
            proof,full=self.exit_fixture();replay=self.verified(proof,full)
            result=self.queue.readmit_native_exit_one_batch_proposals(['author','mechanical'],'parent',proof)
            self.assertEqual(result['jobs'],['author','mechanical']);self.assertEqual(replay.call_count,1)
            self.assertEqual(self.queue.coverage()['accepted_rules'],0)
            validation=json.loads(self.validation.read_text());self.queue.reject_jobs(['author','mechanical'],{'manifest':'batch','validation':validation})
            with self.assertRaisesRegex(ValueError,'already used'):
                self.queue.readmit_native_exit_one_batch_proposals(['author','mechanical'],'parent',proof)
            self.assertEqual(self.queue.coverage()['counts'],{'integration-rejected':3})

    def test_atomic_failures_wrong_command_population_and_tamper(self):
        from contextlib import ExitStack
        proof,full=self.exit_fixture()
        with ExitStack() as stack:
            self.stack=stack;self.verified(proof,full)
            with self.assertRaises(ValueError):self.queue.readmit_native_exit_one_batch_proposals(['author','author'],'parent',proof)
            with self.assertRaises(ValueError):self.queue.readmit_native_exit_one_batch_proposals(['author','mechanical'],'stale',proof)
            isolation=self.root/'.cache/native-isolation.json';original=json.loads(isolation.read_text())
            for mutate in (lambda x:x.update(excluded_jobs=['author']),lambda x:x.update(population_count=2),
                           lambda x:x['controls']['remaining'].update(identity=x['controls']['full']['identity'])):
                value=json.loads(json.dumps(original));mutate(value);write(isolation,value);proof['isolation']=spec(isolation)
                with self.subTest(mutate=mutate),self.assertRaises(ValueError):
                    self.queue.readmit_native_exit_one_batch_proposals(['author','mechanical'],'parent',proof)
            write(isolation,original);proof['isolation']=spec(isolation)
            (self.root/'.cache/native-remaining/grammar/test.ebnf').write_text('tampered')
            with self.assertRaises(ValueError):self.queue.readmit_native_exit_one_batch_proposals(['author','mechanical'],'parent',proof)
        self.assertEqual(self.queue.coverage()['counts'],{'integration-rejected':3})

    def test_population_and_baseline_swap_fail_closed(self):
        from contextlib import ExitStack
        proof,full=self.exit_fixture()
        with ExitStack() as stack:
            self.stack=stack;self.verified(proof,full)
            frozen=self.root/'.cache/batches/batch/validation-inputs/manifest.json'
            original=json.loads(frozen.read_text());changed=json.loads(json.dumps(original))
            changed['files']['gold.jsonl']='wrong';write(frozen,changed)
            with self.assertRaisesRegex(ValueError,'population differs'):
                self.queue.readmit_native_exit_one_batch_proposals(['author','mechanical'],'parent',proof)
            write(frozen,original)
            build=self.root/'.cache/batches/batch/validation-inputs/frozen/build.json'
            old_build=json.loads(build.read_text());changed_build=json.loads(json.dumps(old_build));changed_build['binaries']['score']='wrong';write(build,changed_build)
            with self.assertRaisesRegex(ValueError,'helper identity differs'):
                self.queue.readmit_native_exit_one_batch_proposals(['author','mechanical'],'parent',proof)
            write(build,old_build)
            baseline=self.root/'checkpoints/parent/baseline.jsonl';baseline.write_text('changed')
            with self.assertRaisesRegex(ValueError,'baseline hash differs'):
                self.queue.readmit_native_exit_one_batch_proposals(['author','mechanical'],'parent',proof)
        self.assertEqual(self.queue.coverage()['counts'],{'integration-rejected':3})

    def test_wrong_original_command_and_stale_adjudication_fail(self):
        from contextlib import ExitStack
        proof,full=self.exit_fixture()
        with ExitStack() as stack:
            self.stack=stack;self.verified(proof,full)
            original=json.loads(self.validation.read_text());changed=dict(original,reason='exit 2 wrong command');write(self.validation,changed)
            proof['validation']=spec(self.validation)
            with closing(self.queue._connect()) as db:
                for name in ('author','mechanical'):
                    db.execute('update jobs set adjudication=? where id=?',(json.dumps({'status':'integration-rejected','evidence':{'manifest':'batch','validation':changed}}),name))
            with self.assertRaisesRegex(ValueError,'command differs'):
                self.queue.readmit_native_exit_one_batch_proposals(['author','mechanical'],'parent',proof)
            write(self.validation,original);proof['validation']=spec(self.validation)
            with self.assertRaisesRegex(ValueError,'original rejected batch'):
                self.queue.readmit_native_exit_one_batch_proposals(['author','mechanical'],'parent',proof)
        self.assertEqual(self.queue.coverage()['counts'],{'integration-rejected':3})

    def test_forty_five_jobs_are_one_atomic_operation(self):
        names=[f'sim-{i:02}' for i in range(45)]
        self.queue.add_jobs([{'id':n,'kind':'author','rules':[f'sim_rule_{i}']} for i,n in enumerate(names)])
        child=subprocess.Popen(['true']);child.wait()
        for i,name in enumerate(names):
            claim=self.queue.claim('worker-'+name,kinds=['author']);self.assertEqual(claim['job']['id'],name)
            attempt=claim['attempt_id'];self.queue.mark_started(attempt,pid=child.pid)
            rule=f'sim_rule_{i}'
            self.queue.finish(attempt,'provisional-pass',{'kind':'author','dispositions':{rule:{'rule':rule,'status':'proposed','reason':'reviewed','evidence':'proof'}}},{rule:{'status':'pass','evidence':'proof'}})
            (self.root/'leases'/name).touch()
        self.queue.reject_jobs(names,{'manifest':'sim'})
        evidence={'kind':'isolated-native-exit-one-revalidation','reports':{name:{} for name in names}}
        with patch('retire_native_exit_readmission.verify',return_value={'manifest':'sim','requires_fresh_full_validation':True}) as verifier:
            result=self.queue.readmit_native_exit_one_batch_proposals(names,'parent',evidence)
        self.assertEqual(result['jobs'],names);self.assertEqual(verifier.call_count,1)
        self.assertEqual(len(self.queue.get_jobs('provisional-pass')),45)

    def test_checkpoint_race_and_post_replay_tamper_are_atomic(self):
        from contextlib import ExitStack
        proof,full=self.exit_fixture()
        with ExitStack() as stack:
            self.stack=stack;replay=self.verified(proof,full)
            replay.side_effect=lambda *args:self.queue.add_checkpoint('next','proof',expected_parent='parent')
            with self.assertRaisesRegex(ValueError,'checkpoint changed'):
                self.queue.readmit_native_exit_one_batch_proposals(['author','mechanical'],'parent',proof)
        self.assertEqual(self.queue.coverage()['counts'],{'integration-rejected':3})

    def test_replay_tamper_fails_before_mutation(self):
        from contextlib import ExitStack
        proof,full=self.exit_fixture()
        with ExitStack() as stack:
            self.stack=stack;replay=self.verified(proof,full)
            replay.side_effect=lambda *args:(self.root/'.cache/native-remaining/grammar/test.ebnf').write_text('tampered')
            with self.assertRaisesRegex(ValueError,'changed during replay'):
                self.queue.readmit_native_exit_one_batch_proposals(['author','mechanical'],'parent',proof)
        self.assertEqual(self.queue.coverage()['counts'],{'integration-rejected':3})
