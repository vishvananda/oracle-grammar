"""Canonical removal from ordinary dispatchers preserves authored work."""
from pathlib import Path
import shutil
import tempfile
import unittest

import retire_author as author
import retire_merge


class RemovalTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)/'base'
        self.root = Path(self.temp.name)/'worker'
        syntax = self.base/'grammar/syntax'
        syntax.mkdir(parents=True)
        (syntax/'mined.ebnf').write_text(
            'mined_action ::= m_a | m_b\n/* @sem A */\nm_a ::= "alpha"\n'
            '/* @sem B */\nm_b ::= "beta"\n')
        (syntax/'cost_reminders.ebnf').write_text(
            'cost_choice ::= m_a | m_b\n/* @sem Existing */\nexisting ::= "old"\n')
        (syntax/'card.ebnf').write_text('ability ::= existing\n')
        shutil.copytree(self.base/'grammar', self.root/'grammar')
        self.task = {'rules': ['m_a'], 'write_files': [
            'grammar/syntax/mined.ebnf', 'grammar/syntax/cost_reminders.ebnf']}

    def test_removal_preserves_authored_productions_and_evaluator_accepts(self):
        cost = self.root/'grammar/syntax/cost_reminders.ebnf'
        cost.write_text(cost.read_text()+'/* @sem New */\nnew_phrase ::= "new"\n')
        result = author.regenerate_removals(self.base, self.root, self.task)
        self.assertEqual(result['removed'], ['m_a'])
        self.assertIn('cost_choice ::= m_b', cost.read_text())
        self.assertIn('/* @sem New */\nnew_phrase ::= "new"', cost.read_text())
        self.assertIn('cost_choice ::= m_a | m_b',
                      (Path(result['backups'])/'syntax/cost_reminders.ebnf').read_text())
        directory = Path(self.temp.name)/'evaluation'
        directory.mkdir()
        author.check_seeded_files(self.base, self.root, ['m_a'], directory)
        author.check_root_hooks(self.base, self.root)
        preview = retire_merge.merge_grammar(self.base, self.base, self.root, ['m_a'], [])
        self.assertIn('new_phrase', preview['grammar/syntax/cost_reminders.ebnf'])

    def test_undeclared_dispatcher_change_refused_before_mutation(self):
        self.task['write_files'].remove('grammar/syntax/cost_reminders.ebnf')
        original = (self.root/'grammar/syntax/mined.ebnf').read_bytes()
        with self.assertRaisesRegex(ValueError, 'write set'):
            author.regenerate_removals(self.base, self.root, self.task)
        self.assertEqual(original, (self.root/'grammar/syntax/mined.ebnf').read_bytes())

    def test_root_hook_change_and_retained_semantic_use_refused(self):
        card = self.root/'grammar/syntax/card.ebnf'
        card.write_text('ability ::= existing | m_a\n')
        self.task['write_files'].append('grammar/syntax/card.ebnf')
        with self.assertRaisesRegex(ValueError, 'direct root'):
            author.regenerate_removals(self.base, self.root, self.task)
        card.write_text('ability ::= existing\n')
        cost = self.root/'grammar/syntax/cost_reminders.ebnf'
        cost.write_text(cost.read_text()+'used ::= m_a "cost"\n')
        with self.assertRaisesRegex(ValueError, 'retained users'):
            author.regenerate_removals(self.base, self.root, self.task)
        self.assertIn('m_a ::= ', (self.root/'grammar/syntax/mined.ebnf').read_text())


if __name__ == '__main__':
    unittest.main()
