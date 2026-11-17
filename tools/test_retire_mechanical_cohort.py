"""Mechanical cohorts lex complete linear bodies without changing Oracle input."""
from contextlib import redirect_stdout
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import retire as r
import retire_campaign
import retire_dispatch as dispatch
import retire_mechanical_cohort as mechanical
import retire_tasks
import retire_worker as legacy
from retire_queue import Queue


class LinearWordsTests(unittest.TestCase):
    def test_both_quote_styles_keep_references_outside_literal_words(self):
        body = '''"you" "get" "an" "emblem" "with" '"' affected static_changes "." '"' "search"'''
        self.assertEqual(mechanical.linear_words(body), ['you', 'get', 'an', 'emblem', 'with', 'search'])
        self.assertEqual(mechanical.linear_words("'draw' counter_amount 'cards'"), ['draw', 'cards'])
        self.assertEqual(mechanical.linear_words('"don\'t" object_ref "control"'), ["don", "t", 'control'])

    def test_delimiters_are_not_backslash_escaped_in_w3c_literals(self):
        # W3C EBNF has no escapes; literal backslashes remain literal characters.
        self.assertEqual(mechanical.linear_words(r'"alpha\nbeta"'), ['alpha', 'nbeta'])
        self.assertEqual(mechanical.linear_words("'can\"t'"), ['can', 't'])
        for body in (r'"say \"hello\""', r"'can\'t'", '"unterminated', "'unterminated",
                     '""', "''", '"line\nbreak"', '"nul\x00byte"'):
            with self.subTest(body=body):
                self.assertIsNone(mechanical.linear_words(body))

    def test_branch_repeat_optional_group_and_unknown_syntax_refuse(self):
        for body in ('"a" | "b"', '"a"?', '"a"*', '"a"+', '("a")', '["a"]', '{"a"}',
                     '"a" - "b"', '"a", "b"', '"a";', '"a" #x41', '"a" $name',
                     '"a" name.with.dots', '"a" name-with-dashes', '"a" 12', '"a" /regex/'):
            with self.subTest(body=body):
                self.assertIsNone(mechanical.linear_words(body))

    def test_comments_are_not_stripped_to_manufacture_a_linear_body(self):
        for body in ('"a" /* "b" */ "c"', '"a" /* unterminated', '"a" // "b"',
                     '"a" (* "b" *)'):
            with self.subTest(body=body):
                self.assertIsNone(mechanical.linear_words(body))
        self.assertEqual(mechanical.linear_words('"/*" "words" "*/"'), ['words'])
        self.assertEqual(mechanical.linear_words('"(" ")" "|" "?" "*" "+"'), [])

    def test_complete_suffix_and_bounds_are_checked(self):
        self.assertIsNone(mechanical.linear_words('"first" unknown @'))
        with patch.object(mechanical, 'MAX_BODY_BYTES', 4):
            self.assertIsNone(mechanical.linear_words('"long"'))
        self.assertIsNone(mechanical.linear_words(None))
        self.assertEqual(mechanical.linear_words('names_only'), [])


class CohortTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.run = Path(self.tmp.name)
        self.syntax = self.run/'frozen/grammar/syntax'
        self.syntax.mkdir(parents=True)
        self.grammar = self.syntax/'mined.ebnf'

    def write(self, body):
        self.grammar.write_text('/* @sem example */\nm_rule ::= '+body+'\n')

    def test_emblem_quote_terminals_never_become_phantom_identifier_words(self):
        self.write('''"you" "get" "an" "emblem" "with" '"' affected static_changes "." '"' "search"''')
        oracle = 'You get an emblem with "Lands you control have indestructible." Search your library.'
        index = {'0': {'name': 'Nissa', 'text': oracle},
                 '1': {'name': 'Other', 'text': 'You get an emblem with flying.'},
                 '2': {'name': 'Nissa', 'text': oracle}}
        before = dict(index['0'])
        self.assertEqual(legacy.cohort(self.run, 'm_rule', index), [])
        self.assertEqual(mechanical.cohort(self.run, 'm_rule', index), ['Nissa'])
        self.assertEqual(index['0'], before)

    def test_ordinary_linear_cohort_matches_legacy_and_returns_all_names(self):
        self.write('"untap" counter_amount filter_phrase')
        index = {str(i): {'name': f'name-{i:03d}', 'text': 'Untap a creature.'} for i in range(501)}
        index['extra'] = {'name': 'unrelated', 'text': 'Tap a creature.'}
        before = self.grammar.read_bytes()
        self.assertEqual(mechanical.cohort(self.run, 'm_rule', index),
                         legacy.cohort(self.run, 'm_rule', index))
        self.assertEqual(len(mechanical.cohort(self.run, 'm_rule', index)), 501)
        self.assertEqual(self.grammar.read_bytes(), before)

    def test_wrapped_comment_cannot_disappear_and_branch_is_uncertain(self):
        index = {'0': {'name': 'one', 'text': 'Draw cards.'}}
        for body in ('"draw"\n /* hidden branch | */\n "cards"', '"draw"\n | "cards"',
                     '"draw" /* hidden */ "cards"', '"draw"\n @unknown'):
            with self.subTest(body=body):
                self.write(body)
                self.assertEqual(mechanical.cohort(self.run, 'm_rule', index), [])

    def test_missing_duplicate_symlink_and_file_bounds_refuse(self):
        self.write('"one"')
        index = {'0': {'name': 'one', 'text': 'one'}}
        with self.assertRaisesRegex(ValueError, 'exactly one'):
            mechanical.cohort(self.run, 'missing', index)
        self.grammar.write_text('m_rule ::= "one"\nm_rule ::= "two"\n')
        with self.assertRaisesRegex(ValueError, 'exactly one'):
            mechanical.cohort(self.run, 'm_rule', index)
        self.write('"one"')
        with patch.object(mechanical, 'MAX_FILE_BYTES', 1), self.assertRaisesRegex(ValueError, 'too large'):
            mechanical.cohort(self.run, 'm_rule', index)
        with patch.object(mechanical, 'MAX_GRAMMAR_BYTES', 1), self.assertRaisesRegex(ValueError, 'byte bound'):
            mechanical.cohort(self.run, 'm_rule', index)
        target = self.syntax/'copy.ebnf'
        target.symlink_to(self.grammar)
        with self.assertRaisesRegex(ValueError, 'unavailable'):
            mechanical.cohort(self.run, 'm_rule', index)

    def test_preparation_uses_complete_quote_aware_cohort(self):
        self.write('''"you" "get" "an" "emblem" "with" '"' affected static_changes '"' "search"''')
        campaign = self.run/'campaign'
        campaign.mkdir()
        queue = Queue(campaign/'queue.sqlite')
        queue.add_checkpoint('base', 'proof')
        queue.add_jobs([{'id': 'mechanical', 'kind': 'mechanical', 'rules': ['m_rule']}])
        claim = queue.claim('worker')
        index = {'0': {'name': 'Nissa', 'text': 'You get an emblem with "lands". Search your library.'}}
        class Contexts:
            def get(self, checkpoint):
                return ((index, [], []), {}, {})
        with patch.object(dispatch.campaign_api, 'checkpoint', return_value={'inputs': str(self.run)}), \
                patch.object(dispatch, 'close_cohort'), \
                patch.object(legacy, 'cohort', side_effect=AssertionError('legacy selector used')), \
                patch.object(dispatch, 'prepare_task', side_effect=ValueError('stop after preparation')) as prepare:
            dispatch.perform(campaign, claim, Contexts())
        job, detail = prepare.call_args.args[3:5]
        self.assertEqual(job['target_faces'], ['Nissa'])
        self.assertEqual(detail['names'], ['Nissa'])

    def test_worker_independently_uses_the_same_real_selector(self):
        self.write('''"you" "get" "an" "emblem" "with" '"' affected static_changes '"' "search"''')
        (self.run/'scan').mkdir()
        r.write(self.run/'scan/rules.json', {'unused': ['m_rule']})
        working = self.run/'working'
        working.mkdir()
        # The diagnostic uses the original frozen input root, not a narrowed packet.
        (working/'inputs').symlink_to(self.run, target_is_directory=True)
        index = {'0': {'name': 'Nissa', 'text': 'You get an emblem with "lands". Search your library.'}}
        task = {'rules': ['m_rule'], 'target_faces': ['Nissa'], 'cohort': ['Nissa'],
                'cpu_pool': str(self.run/'cpu'), 'cpu_capacity': 32}
        (working/'tasks').mkdir()
        r.write(working/'tasks/mechanical.json', task)
        r.write(working/'support.json', {})
        with patch.object(dispatch.author, 'verify', return_value=(task, self.run, {'commit': 'base'})), \
                patch.object(legacy, 'cohort', side_effect=AssertionError('legacy selector used')), \
                patch.object(legacy, 'payload', return_value=(index, [], [])), \
                patch.object(legacy, 'checks', return_value={'faces': 1, 'failures': []}) as checks, \
                patch.object(dispatch.r, 'verify'), redirect_stdout(io.StringIO()):
            dispatch.mechanical(working, 'mechanical')
        self.assertEqual(checks.call_args.args[3], {'Nissa'})
        report = r.read(next((working/'evaluations/mechanical').glob('*/report.json')))
        self.assertTrue(report['passed'])

    def test_dispatch_and_frozen_bundles_use_the_new_mechanical_selector(self):
        self.assertIs(dispatch.mechanical_cohort, mechanical)
        self.assertIn('retire_mechanical_cohort.py', retire_campaign.RUNNERS)
        self.assertIn('retire_mechanical_cohort.py', retire_tasks.RUNNERS)


if __name__ == '__main__':
    unittest.main()
