"""Mechanical staging accepts exactly the complete canonical assigned removal."""
from pathlib import Path
import json
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import mined
import retire as r
import retire_batch as batch


class MechanicalCanonicalTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.repo = self.root/'repo'
        (self.repo/'grammar/syntax').mkdir(parents=True)
        (self.repo/'tests').mkdir()
        (self.repo/'tests/families.json').write_text('[]\n')
        (self.repo/'grammar/lexical.ebnf').write_text('word ::= "word"\n')
        (self.repo/'grammar/print-model.txt').write_text('unchanged compiled input\n')
        (self.repo/'grammar/syntax/mined.ebnf').write_text(
            'mined_action ::= m_rule | never_action\n'
            '/* @sem Original */\nm_rule ::= "untap" counter_amount filter_phrase\n'
            'never_action ::= "never"\n')
        (self.repo/'grammar/syntax/cost_reminders.ebnf').write_text(
            'keyword_nonmana_mined_cost ::= m_rule | other\nother ::= "other"\n')
        self.run = self.root/'attempt'
        (self.run/'inputs/frozen').mkdir(parents=True)
        shutil.copytree(self.repo/'grammar', self.run/'inputs/frozen/grammar')
        self.directory = self.run/'evaluated'
        self.directory.mkdir()
        shutil.copytree(self.repo/'grammar', self.directory/'grammar')
        removed, kept = mined.remove(self.directory, ['m_rule'])
        self.assertEqual((removed, kept), (['m_rule'], {}))

    def verify(self):
        batch._mechanical_removal(self.run, self.directory, ['m_rule'], self.root)

    def test_ordinary_dispatcher_change_is_part_of_canonical_removal(self):
        self.verify()
        self.assertEqual((self.directory/'grammar/syntax/cost_reminders.ebnf').read_text(),
                         'keyword_nonmana_mined_cost ::= other\nother ::= "other"\n')
        self.assertNotIn('m_rule', (self.directory/'grammar/syntax/mined.ebnf').read_text())

    def test_all_grammar_files_and_exact_wording_are_verified(self):
        for mode in ('ordinary-semantic', 'ordinary-wording', 'compiled', 'extra', 'missing', 'seeded'):
            with self.subTest(mode=mode):
                shutil.rmtree(self.directory/'grammar')
                shutil.copytree(self.run/'inputs/frozen/grammar', self.directory/'grammar')
                mined.remove(self.directory, ['m_rule'])
                path = self.directory/'grammar/syntax/cost_reminders.ebnf'
                if mode == 'ordinary-semantic': path.write_text('/* @sem Wrong */\n'+path.read_text())
                elif mode == 'ordinary-wording': path.write_text(path.read_text().replace('"other"', '"changed"'))
                elif mode == 'compiled': (self.directory/'grammar/lexical.ebnf').write_text('word ::= "changed"\n')
                elif mode == 'extra': (self.directory/'grammar/syntax/extra.ebnf').write_text('extra ::= "x"\n')
                elif mode == 'missing': path.unlink()
                else: (self.directory/'grammar/syntax/mined.ebnf').write_text('never_action ::= "changed"\n')
                with self.assertRaisesRegex(ValueError, 'complete canonical removal'):
                    self.verify()

    def test_symlinks_and_duplicate_or_retained_assignments_refuse(self):
        path = self.directory/'grammar/syntax/link.ebnf'; path.symlink_to('cost_reminders.ebnf')
        with self.assertRaisesRegex(ValueError, 'symlink'):
            self.verify()
        path.unlink()
        with self.assertRaisesRegex(ValueError, 'distinct assigned'):
            batch._mechanical_removal(self.run, self.directory, ['m_rule', 'm_rule'], self.root)
        base = self.run/'inputs/frozen/grammar/syntax/cost_reminders.ebnf'
        base.write_text('retained ::= m_rule "again"\n')
        with self.assertRaisesRegex(ValueError, 'retained users'):
            self.verify()

    def prepare_proposal(self, hooks=None):
        support = self.run/'support'; support.mkdir()
        (support/'proof.txt').write_text('fixed helper identity\n')
        descriptor = self.run/'tasks/task.json'; descriptor.parent.mkdir()
        r.write(descriptor, {'rules': ['m_rule']})
        r.write(self.run/'support.json', {'files': r.tree(support),
                                         'tasks': {'task': r.digest(descriptor)}})
        r.write(self.run/'inputs/manifest.json', {'files': r.tree(self.run/'inputs/frozen'),
                                                 'commit': 'claimed-base'})
        r.write(self.directory/'hooks.json', [] if hooks is None else hooks)
        report = {'passed': True, 'failures': [], 'kind': 'mechanical', 'task': 'task',
                  'source': 'claimed-base', 'descriptor_sha256': r.digest(descriptor),
                  'support_sha256': r.digest(self.run/'support.json'), 'directory': str(self.directory),
                  'grammar': r.tree(self.directory/'grammar'), 'hooks': [] if hooks is None else hooks,
                  'faces': 1057, 'chunks_expected': 3, 'chunks_completed': 3, 'coverage_complete': True}
        return {'job_id': 'job', 'attempt_id': 'attempt', 'kind': 'mechanical',
                'run': str(self.run), 'report': report}

    def stage(self, proposal):
        subprocess.run(['git', 'init', '-q', str(self.repo)], check=True)
        subprocess.run(['git', 'add', '.'], cwd=self.repo, check=True)
        subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=t@x',
                        'commit', '-qm', 'current prefix'], cwd=self.repo, check=True)
        commit = batch._git(self.repo, 'rev-parse', 'HEAD')
        with patch.object(batch.merger, 'merge_grammar', side_effect=AssertionError('old grammar must not merge')):
            return batch.stage(self.root, {'source': str(self.repo), 'commit': commit}, [proposal])

    def test_staging_preserves_new_current_shared_alternatives(self):
        proposal = self.prepare_proposal()
        current = self.repo/'grammar/syntax/cost_reminders.ebnf'
        current.write_text('keyword_nonmana_mined_cost ::= m_rule | other | new_current\n'
                           'other ::= "other"\n/* @sem New */\nnew_current ::= "new"\n')
        result = self.stage(proposal)
        self.assertEqual(result['status'], 'provisional')
        self.assertEqual(result['rules'], ['m_rule'])
        grammar = Path(result['source'])/'grammar'
        self.assertEqual((grammar/'syntax/cost_reminders.ebnf').read_text(),
                         'keyword_nonmana_mined_cost ::= other | new_current\n'
                         'other ::= "other"\n/* @sem New */\nnew_current ::= "new"\n')
        self.assertEqual((grammar/'lexical.ebnf').read_text(), 'word ::= "word"\n')
        self.assertEqual(json.loads((Path(result['source'])/'tests/families.json').read_text()), [])

    def test_mechanical_hooks_remain_forbidden(self):
        result = self.stage(self.prepare_proposal([{'root': 'ability', 'production': 'wrong'}]))
        self.assertEqual(result['status'], 'no-proposals')
        self.assertEqual(result['rules'], [])
        self.assertIn('cannot request hooks', result['jobs'][0]['reason'])

    def test_repeat_trials_cache_the_expected_tree_and_verify_the_candidate(self):
        self.prepare_proposal()
        cache = batch._ProposalInputCache()
        original = batch._canonical_mechanical_tree
        with patch.object(batch, '_canonical_mechanical_tree', wraps=original) as canonical:
            for _ in range(2):
                trial = cache.trial()
                trial.read(self.run/'inputs')
                batch._mechanical_removal(self.run, self.directory, ['m_rule'], self.root, input_cache=trial)
                self.assertTrue(trial.unchanged())
            canonical.assert_called_once()

            (self.directory/'grammar/lexical.ebnf').write_text('word ::= "changed"\n')
            trial = cache.trial()
            trial.read(self.run/'inputs')
            with self.assertRaisesRegex(ValueError, 'complete canonical removal'):
                batch._mechanical_removal(self.run, self.directory, ['m_rule'], self.root, input_cache=trial)
            canonical.assert_called_once()

            (self.run/'inputs/frozen/grammar/lexical.ebnf').write_text('word ::= "stale"\n')
            with self.assertRaisesRegex(ValueError, 'frozen inputs changed'):
                cache.trial().read(self.run/'inputs')


if __name__ == '__main__':
    unittest.main()
