import tempfile
import unittest
from pathlib import Path

from retire_merge import MergeConflict, canonical_seeded_removal, merge_grammar, merge_text


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

    def test_identical_additions_keep_published_prose_and_reject_directive_changes(self):
        base = 'root ::= one\n'
        declaration = '/* @sem Two */\ntwo ::= "two"\n'
        current = base + '\n/* Current prose. */\n' + declaration
        proposed = base + '\n/* Different prose. */\n' + declaration
        self.assertEqual(merge_text(base, current, proposed), current)
        self.assertEqual(merge_text(base, current, base+'\n'+declaration), current)
        for patch in (proposed.replace('@sem Two', '@sem Other'),
                      proposed.replace('Different prose.', '@prefer 2'),
                      proposed.replace('"two"', '"another"')):
            with self.subTest(patch=patch), self.assertRaisesRegex(MergeConflict, 'conflicting addition'):
                merge_text(base, current, patch)

    def test_repeated_verbatim_base_definition_is_collapsed(self):
        base_block = '/* @sem X */\nx ::= "x"\n'
        extra = '/* @sem Y */\ny ::= "y"\n'
        proposed = base_block + base_block + extra
        self.assertEqual(merge_text(base_block, base_block, proposed), base_block + extra)
        current = base_block.replace('@sem X', '@sem Rebound')
        self.assertEqual(merge_text(base_block, current, proposed), current + '\n' + extra)

    def test_duplicate_removal_preserves_separate_leading_comments(self):
        base = '/* @sem X */\nx ::= "x"\n'
        extra = '/* A note about the family. */\n'
        next_production = '/* @sem Y */\ny ::= "y"\n'
        proposed = base + extra + base + next_production
        self.assertEqual(merge_text(base, base, proposed), base + extra + next_production)
        with self.assertRaisesRegex(MergeConflict, 'duplicate production x'):
            merge_text(base, base, base + '/* @sem SeparateMeaning */\n' + base)

    def test_duplicate_names_with_new_or_changed_declarations_still_fail(self):
        base_block = '/* @sem X */\nx ::= "x"\n'
        for base, proposed in (('', base_block * 2),
                               (base_block, base_block + base_block.replace('@sem X', '@sem Other')),
                               (base_block, base_block.replace('@sem X', '@sem Other') * 2)):
            with self.subTest(base=base, proposed=proposed), self.assertRaisesRegex(MergeConflict, 'duplicate production x'):
                merge_text(base, base, proposed)

    def test_removal_does_not_erase_concurrent_additions(self):
        base = 'root ::= one | two\n'
        self.assertEqual(merge_text(base, 'root ::= one | two | three\n', 'root ::= one\n'),
                         'root ::= one | three\n')

    def test_bare_dispatcher_merges_an_independent_prose_edit(self):
        base = '/* Conditions on triggers and activations. */\ncondition ::= one | two\n'
        current = base.replace('one | two', 'one | two | three')
        proposed = 'condition ::= one | two\n'
        expected = 'condition ::= one | two | three\n'
        self.assertEqual(merge_text(base, current, proposed), expected)
        self.assertEqual(merge_text(base, proposed, current), expected)

    def test_bare_dispatcher_rejects_divergent_prose_or_preference_changes(self):
        base = '/* Original. */\nroot ::= one\n'
        current = '/* Current. */\nroot ::= one | two\n'
        proposed = '/* Proposed. */\nroot ::= one | three\n'
        with self.assertRaisesRegex(MergeConflict, 'conflicting comments on root'):
            merge_text(base, current, proposed)
        with self.assertRaisesRegex(MergeConflict, 'conflicting comments on root'):
            merge_text(base, base.replace('Original.', '@prefer 1'), base.replace('one', 'one | two'))

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
            expected = canonical_seeded_removal(root/'base', ['m_old'])
            (root/'proposed/grammar/syntax/mined.ebnf').write_text('bad\n')
            with self.assertRaises(MergeConflict):
                merge_grammar(root/'base', root/'current', root/'proposed', ['m_old'], [])
            with self.assertRaisesRegex(MergeConflict, 'noncanonical seeded file'):
                merge_grammar(root/'base', root/'current', root/'proposed', ['m_old'], [],
                              canonical_removal=lambda *_: expected)


if __name__ == '__main__':
    unittest.main()
