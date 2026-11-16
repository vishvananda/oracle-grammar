import tempfile
import unittest
from pathlib import Path

from retire_merge import MergeConflict, merge_grammar, merge_text


class MergeTests(unittest.TestCase):
    def test_parallel_additions_and_dispatcher(self):
        base = 'root ::= one\n\n/* @sem One */\none ::= "one"\n'
        current = base.replace('root ::= one', 'root ::= one | two') + '\n/* @sem Two */\ntwo ::= "two"\n'
        proposed = base.replace('root ::= one', 'root ::= one | three') + '\n/* @sem Three */\nthree ::= "three"\n'
        merged = merge_text(base, current, proposed)
        self.assertIn('root ::= one | two | three', merged)
        self.assertIn('/* @sem Two */\ntwo ::= "two"', merged)
        self.assertIn('/* @sem Three */\nthree ::= "three"', merged)
        self.assertEqual(merge_text(base, merged, proposed), merged)

    def test_divergent_semantics_or_new_name_conflict(self):
        base = '/* @sem A */\nx ::= "x"\n'
        with self.assertRaises(MergeConflict):
            merge_text(base, base.replace('@sem A', '@sem B'), base.replace('@sem A', '@sem C'))
        with self.assertRaises(MergeConflict):
            merge_text('', '/* @sem A */\nx ::= "a"\n', '/* @sem B */\nx ::= "b"\n')

    def test_removal_does_not_erase_concurrent_additions(self):
        base = 'root ::= one | two\n'
        self.assertEqual(merge_text(base, 'root ::= one | two | three\n', 'root ::= one\n'),
                         'root ::= one | three\n')

    def test_preview_and_canonical_seeded(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name in ('base', 'current', 'proposed'):
                syntax = root / name / 'grammar' / 'syntax'
                syntax.mkdir(parents=True)
                (syntax / 'mined.ebnf').write_text('mined_action ::= m_old | never_action\n/* @sem X */\nm_old ::= "old"\n/* @sem () */\nnever_action ::= "never"\n')
                (syntax / 'actions.ebnf').write_text('root ::= one\n/* @sem One */\none ::= "one"\n')
            (root/'current/grammar/syntax/actions.ebnf').write_text('root ::= one | two\n/* @sem One */\none ::= "one"\n/* @sem Two */\ntwo ::= "two"\n')
            (root/'proposed/grammar/syntax/actions.ebnf').write_text('root ::= one | three\n/* @sem One */\none ::= "one"\n/* @sem Three */\nthree ::= "three"\n')
            # Canonical removal is performed only in the private proposal.
            import mined
            mined.remove(root/'proposed', ['m_old'])
            changes = merge_grammar(root/'base', root/'current', root/'proposed', ['m_old'], [])
            self.assertEqual(list(changes), ['grammar/syntax/actions.ebnf'])
            self.assertIn('root ::= one | two | three', changes['grammar/syntax/actions.ebnf'])
            self.assertIn('m_old', (root/'current/grammar/syntax/mined.ebnf').read_text())
            (root/'proposed/grammar/syntax/mined.ebnf').write_text('bad\n')
            with self.assertRaises(MergeConflict):
                merge_grammar(root/'base', root/'current', root/'proposed', ['m_old'], [])


if __name__ == '__main__':
    unittest.main()
