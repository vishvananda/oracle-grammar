"""Seeded-rule removal keeps comments, hooks and kept users consistent."""
from pathlib import Path
import tempfile
import unittest

import mined

SYNTAX = '''/* Rules mined from the corpus. */
mined_ability ::= m_a | m_b | m_c
mined_clause ::= m_d | m_a
/* @sem () */
never_ability ::= "never"

/* @sem A */
m_a ::= "a"

/* @sem B */
/* second comment */
m_b ::= "b" m_d
/* @sem C,
   on two lines */
m_c ::= "c"

/* @sem D */
m_d ::= "d"
'''

HAND = '''/* m_a in a comment stays. */
uses ::= m_a | m_b
whole ::= "x"
    m_c
'''


class MinedTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        (self.root/'grammar/syntax').mkdir(parents=True)
        (self.root/'grammar/tail').mkdir()
        (self.root/'grammar/syntax/mined.ebnf').write_text(SYNTAX)
        (self.root/'grammar/syntax/hand.ebnf').write_text(HAND)

    def read(self, name):
        return (self.root/'grammar/syntax'/name).read_text()

    def test_removes_definitions_comments_and_alternatives(self):
        removed, kept = mined.remove(self.root, ['m_a'])
        self.assertEqual((removed, kept), (['m_a'], {}))
        text = self.read('mined.ebnf')
        self.assertNotIn('@sem A', text)
        self.assertIn('mined_ability ::= m_b | m_c\nmined_clause ::= m_d\n', text)
        self.assertNotIn('\n\n\n', text)
        self.assertIn('uses ::= m_b\n', self.read('hand.ebnf'))
        self.assertIn('m_a in a comment', self.read('hand.ebnf'))

    def test_keeps_rules_a_kept_rule_uses_in_a_sequence(self):
        removed, kept = mined.remove(self.root, ['m_c', 'm_d'])
        self.assertEqual((removed, kept), ([], {'m_c': 'whole', 'm_d': 'm_b'}))
        self.assertEqual(self.read('mined.ebnf'), SYNTAX)

    def test_a_user_removed_with_its_rule_frees_it(self):
        self.assertEqual(mined.remove(self.root, ['m_b', 'm_d']), (['m_b', 'm_d'], {}))
        text = self.read('mined.ebnf')
        self.assertIn('mined_ability ::= m_a | m_c\nmined_clause ::= m_a\n', text)
        for gone in ('@sem B', 'second comment', '@sem D'):
            self.assertNotIn(gone, text)
        self.assertIn('/* @sem C,\n   on two lines */\nm_c', text)
        self.assertIn('uses ::= m_a\n', self.read('hand.ebnf'))

    def test_a_kept_list_never_loses_its_last_alternative(self):
        self.assertEqual(mined.remove(self.root, ['m_a', 'm_b']),
                         ([], {'m_a': 'uses', 'm_b': 'uses'}))

    def test_an_emptied_hook_needs_its_never_rule(self):
        (self.root/'grammar/syntax/hand.ebnf').write_text('')
        with self.assertRaisesRegex(ValueError, 'mined_clause would lose'):
            mined.remove(self.root, ['m_a', 'm_b', 'm_d'])
        mined.remove(self.root, ['m_a', 'm_b', 'm_c'])
        self.assertIn('mined_ability ::= never_ability\n', self.read('mined.ebnf'))

    def test_a_continued_list_loses_names_across_its_lines(self):
        (self.root/'grammar/syntax/hand.ebnf').write_text(
            'uses ::= m_a | m_b\n    | m_c | m_d\n    | m_a\nwhole ::= "x"\n')
        self.assertEqual(mined.remove(self.root, ['m_a', 'm_c']), (['m_a', 'm_c'], {}))
        self.assertIn('uses ::= m_b\n    | m_d\nwhole', self.read('hand.ebnf'))
        mined.remove(self.root, ['m_b'])
        self.assertIn('uses ::= m_d\nwhole', self.read('hand.ebnf'))

    def test_a_wrapped_sequence_is_never_split(self):
        (self.root/'grammar/syntax/hand.ebnf').write_text('uses ::= m_a | m_b\n    m_c\n')
        self.assertEqual(mined.remove(self.root, ['m_a']), ([], {'m_a': 'uses'}))

    def test_adoption_copies_a_rule_into_its_module_and_parent(self):
        (self.root/'grammar/syntax/hand.ebnf').write_text('uses ::= m_a\n    | m_b\n')
        plan = [{'rule': 'm_c', 'name': 'c_word', 'module': 'hand', 'parent': 'uses',
                 'comment': 'The word c.'}]
        self.assertEqual(mined.adopt(self.root, plan), ['c_word'])
        self.assertEqual(self.read('mined.ebnf'), SYNTAX)
        self.assertEqual(self.read('hand.ebnf'), 'uses ::= m_a\n    | m_b\n    | c_word\n\n'
                         '/* The word c. */\n/* @sem C,\n   on two lines */\nc_word ::= "c"\n')

    def test_adoption_refuses_seeded_users_and_taken_names(self):
        plan = [{'rule': 'm_b', 'name': 'b_word', 'module': 'hand', 'parent': 'uses',
                 'comment': 'B.'}]
        with self.assertRaisesRegex(ValueError, 'm_b uses seeded rules'):
            mined.adopt(self.root, plan)
        plan = [{'rule': 'm_a', 'name': 'whole', 'module': 'hand', 'parent': 'uses',
                 'comment': 'A.'}]
        with self.assertRaisesRegex(ValueError, 'not a free production name'):
            mined.adopt(self.root, plan)
        plan = [{'rule': 'm_a', 'name': 'a_word', 'module': 'hand', 'parent': 'whole',
                 'comment': 'A.'}]
        with self.assertRaisesRegex(ValueError, 'not a hand-written list'):
            mined.adopt(self.root, plan)

    def test_refuses_names_that_are_not_seeded_rules(self):
        with self.assertRaisesRegex(ValueError, 'not seeded'):
            mined.remove(self.root, ['m_z', 'never_ability'])


if __name__ == '__main__':
    unittest.main()
