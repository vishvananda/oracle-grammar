import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import retire_batch as batch


class BatchTests(unittest.TestCase):
    def test_edited_family_gold_is_independently_reconstructed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            frozen, evaluation = root/'inputs/frozen', root/'evaluation'
            frozen.mkdir(parents=True)
            (evaluation/'edited-0').mkdir(parents=True)
            (evaluation/'family.json').write_text('{"family":"test","cases":[]}')
            original = {'faces': [{'name': 'Example', 'characteristics': {'types': ['Instant']}, 'abilities': [{'amount': 1}]}]}
            edited = {'faces': [{'name': 'Example', 'characteristics': {'types': ['Instant']}, 'abilities': [{'amount': 2}]}]}
            (frozen/'gold.jsonl').write_text(json.dumps(original)+'\n')
            (evaluation/'edited-0/gold.jsonl').write_text(json.dumps(edited)+'\n')
            (evaluation/'edited-0/rows.jsonl').write_text(json.dumps({'name': 'Example', 'text': 'two', 'status': 'equal'})+'\n')
            report = {'edited': [{'face': 'Example', 'mutations': [{'path': [0, 'amount'], 'value': 2}], 'flags': [True, False, True, False]}]}
            case = batch._family(evaluation, report, root)['cases'][0]
            self.assertEqual(case['expected'], [{'amount': 2}])
            self.assertFalse(case['exact'])
            self.assertEqual(case['context'], {'types': ['Instant']})
            edited['faces'][0]['abilities'][0]['amount'] = 3
            (evaluation/'edited-0/gold.jsonl').write_text(json.dumps(edited)+'\n')
            with self.assertRaises(ValueError):
                batch._family(evaluation, report, root)

    def test_batch_bound_rejects_empty_and_over_ninety_six(self):
        with tempfile.TemporaryDirectory() as temporary:
            for proposals in ([], [{}] * 97):
                with self.assertRaisesRegex(ValueError, 'ninety-six'):
                    batch.stage(temporary, {}, proposals)
            with patch.object(batch, '_git', side_effect=RuntimeError('past bound')):
                with self.assertRaisesRegex(RuntimeError, 'past bound'):
                    batch.stage(temporary, {'source': temporary, 'commit': 'base'}, [{}] * 96)

    def test_validate_reserves_cpu_and_uses_24_threads(self):
        import os
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root/'workspace'
            (source/'grammar').mkdir(parents=True)
            (source/'grammar/a.ebnf').write_text('root ::= one\n')
            frozen = root/'checkpoint/frozen'
            frozen.mkdir(parents=True)
            (frozen/'gold.jsonl').write_text('gold')
            (root/'checkpoint/manifest.json').write_text(json.dumps({'files': batch.r.tree(frozen)}))
            baseline = root/'baseline.jsonl'
            baseline.write_text('baseline')
            folder = root/'batch'
            folder.mkdir()
            manifest = {'status': 'provisional', 'commit': 'candidate', 'source': str(source),
                        'folder': str(folder), 'grammar': batch.r.tree(source/'grammar'), 'jobs': [{'job_id': 'j1', 'status': 'included'}]}
            cp = {'commit': 'base', 'inputs': str(root/'checkpoint'), 'baseline': str(baseline),
                  'helpers': str(root/'helpers'), 'cpu_pool': str(root/'cpu'), 'cpu_capacity': 32}
            def git(_source, *args):
                return 'candidate' if args[0] == 'rev-parse' else ''
            def checks(run, directory, grammar, names, data, threads):
                self.assertEqual(threads, 24)
                self.assertEqual(len(batch.r._LEASES), 24)
                for fd in batch.r._LEASES:
                    os.fstat(fd)
                (directory/'rows.jsonl').write_text('rows')
                return {'failures': []}
            def gate(*args, **kwargs):
                self.assertEqual(len(kwargs['pass_fds']), 24)
                return SimpleNamespace(returncode=0)
            with patch.object(batch, '_git', side_effect=git), patch.object(batch.r, 'freeze'), patch.object(batch.w, 'payload', return_value=({'0': {'name': 'Example'}}, [], [])), patch.object(batch.w, 'checks', side_effect=checks), patch.object(batch.subprocess, 'run', side_effect=gate):
                result = batch.validate(manifest, cp)
            self.assertEqual(result['status'], 'passed')
            self.assertEqual(result['scope'], 'combined-batch')
            self.assertEqual(result['included_jobs'], ['j1'])
            self.assertEqual(result['inputs'], str(folder/'validation-inputs'))
            self.assertEqual(batch.r._LEASES, [])

    def test_mechanical_stages_without_fixtures_and_reuses_clean_workspace(self):
        import shutil
        import mined
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root/'repo'
            repo.mkdir()
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo/'grammar/syntax').mkdir(parents=True)
            (repo/'tests').mkdir()
            (repo/'tests/families.json').write_text('[]\n')
            (repo/'grammar/syntax/mined.ebnf').write_text('mined_action ::= m_one | m_two | never_action\n/* @sem One */\nm_one ::= "one"\n/* @sem Two */\nm_two ::= "two"\n/* @sem () */\nnever_action ::= "never"\n')
            subprocess.run(['git', 'add', '.'], cwd=repo, check=True)
            subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=t@x', 'commit', '-qm', 'base'], cwd=repo, check=True)
            cp = {'source': str(repo), 'commit': batch._git(repo, 'rev-parse', 'HEAD'), 'integration_workspace': str(root/'workspace')}
            def execute(rule, index):
                run = root/f'run{index}'
                run.mkdir()
                (run/'inputs').mkdir()
                frozen = run/'inputs/frozen'
                frozen.mkdir()
                shutil.copytree((repo if index == 1 else root/'workspace')/'grammar', frozen/'grammar')
                attempt = root/f'attempt{index}'
                attempt.mkdir()
                shutil.copytree(frozen/'grammar', attempt/'grammar')
                mined.remove(attempt, [rule])
                report = {'task': f'task{index}', 'source': cp['commit'], 'descriptor_sha256': 'd', 'support_sha256': 's'}
                job = {'job_id': f'j{index}', 'attempt_id': f'a{index}', 'kind': 'mechanical', 'run': str(run), 'report': report}
                with patch.object(batch, '_proposal', return_value=(run, {'rules': [rule]}, attempt, [])):
                    return batch.stage(root, cp, [job])
            first = execute('m_one', 1)
            self.assertEqual(first['status'], 'provisional')
            self.assertEqual(first['fixtures'], [])
            self.assertEqual(first['jobs'][0]['kind'], 'mechanical')
            self.assertEqual(batch._git(repo, 'rev-parse', 'HEAD'), cp['commit'])
            cp['commit'] = first['commit']
            with self.assertRaisesRegex(ValueError, 'terminal'):
                execute('m_two', 2)
            Path(first['folder'], 'validation.json').write_text('{"status":"passed"}')
            second = execute('m_two', 3)
            self.assertEqual(second['status'], 'provisional')
            self.assertEqual(second['source'], first['source'])
            self.assertNotIn('m_two ::= ', (root/'workspace/grammar/syntax/mined.ebnf').read_text())
            self.assertIn('m_one ::= ', (repo/'grammar/syntax/mined.ebnf').read_text())

    def test_conflicts_are_parked_without_removing_rules_or_overwriting_fixture(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root/'repo'
            source.mkdir()
            subprocess.run(['git', 'init', '-q', str(source)], check=True)
            for folder in ('grammar/syntax', 'tests', 'families'):
                (source/folder).mkdir(parents=True)
            (source/'grammar/syntax/mined.ebnf').write_text('m_old ::= "old"\nm_good ::= "good"\n')
            (source/'tests/families.json').write_text('[]\n')
            (source/'families/existing.json').write_text('protected')
            subprocess.run(['git', 'add', '.'], cwd=source, check=True)
            subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=t@x', 'commit', '-qm', 'base'], cwd=source, check=True)
            commit = batch._git(source, 'rev-parse', 'HEAD')
            report = {'edited': [], 'task': 'task', 'source': commit, 'descriptor_sha256': 'd', 'support_sha256': 's', 'fixture_sha256': 'f'}
            proposals = [{'job_id': n, 'attempt_id': n, 'run': str(root/n), 'report': dict(report)} for n in ('conflict', 'overwrite', 'good')]
            def proposal(p):
                name = p['job_id']
                return root/name, {'fixture': 'families/existing.json' if name == 'overwrite' else f'families/{name}.json',
                                   'rules': ['m_'+('old' if name == 'conflict' else 'good')]}, root/name, []
            def merge(base, current, proposed, rules, hooks):
                if rules == ['m_old']:
                    raise batch.merger.MergeConflict('semantic conflict')
                return {'grammar/syntax/added.ebnf': 'shared ::= "good"\n'}
            def remove(trial, rules):
                path = trial/'grammar/syntax/mined.ebnf'
                path.write_text(path.read_text().replace('m_good ::= "good"\n', ''))
                return rules, {}
            with patch.object(batch, '_proposal', side_effect=proposal), patch.object(batch, '_family', return_value={'family': 'new', 'cases': []}), patch.object(batch.merger, 'merge_grammar', side_effect=merge), patch.object(batch.mined, 'remove', side_effect=remove), patch.object(batch.a, 'apply_hooks'), patch.object(batch, '_helper'):
                result = batch.stage(root, {'source': str(source), 'commit': commit}, proposals)
            self.assertEqual(result['status'], 'provisional')
            self.assertEqual([j['status'] for j in result['jobs']], ['integration-rejected', 'integration-rejected', 'included'])
            candidate = Path(result['source'])
            self.assertIn('m_old', (candidate/'grammar/syntax/mined.ebnf').read_text())
            self.assertEqual((candidate/'families/existing.json').read_text(), 'protected')
            self.assertEqual(json.loads((candidate/'tests/families.json').read_text()), ['families/good.json'])
            self.assertEqual(batch._git(source, 'rev-parse', 'HEAD'), commit)
            self.assertNotIn('shared', (source/'grammar/syntax/mined.ebnf').read_text())


if __name__ == '__main__':
    unittest.main()
