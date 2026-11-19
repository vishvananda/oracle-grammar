"""Authoring trials constrain operands, source identities and model isolation."""
import copy
from concurrent.futures import ThreadPoolExecutor
from contextlib import redirect_stdout
import fcntl
import io
import unittest
from unittest.mock import patch
from pathlib import Path
import tempfile
import threading
import shutil

import retire_author as a
import retire_author_pool as pool
import retire as r


class AuthoringTests(unittest.TestCase):
    def test_coordinator_waits_for_the_existing_diagnostic_before_reserving_cpu(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            lock = root/'.cache/author-check.lock'
            lock.parent.mkdir()
            verified = threading.Event()
            prepared = ({'cpu_pool': str(root/'cpu')}, root, {'commit': 'base'})

            def verify(run, task):
                verified.set()
                return prepared

            with lock.open('w') as active, ThreadPoolExecutor(max_workers=1) as executor, \
                 patch.object(a, 'verify', side_effect=verify), \
                 patch.object(a, '_check_with_cpu', return_value={'passed': True}) as check:
                fcntl.flock(active, fcntl.LOCK_EX)
                later = executor.submit(a.check, root, 'task', coordinator=True)
                try:
                    self.assertTrue(verified.wait(2))
                    with self.assertRaisesRegex(ValueError, 'diagnostic helper is still running'):
                        a.check(root, 'task')
                    check.assert_not_called()
                    self.assertFalse(later.done())
                finally:
                    fcntl.flock(active, fcntl.LOCK_UN)
                self.assertEqual(later.result(timeout=2), {'passed': True})
                check.assert_called_once_with(root, 'task', True, False, prepared)

    def test_preview_keeps_duplicate_targets_and_companion_faces(self):
        records = [{'faces': [{'name': 'Target'}, {'name': 'Companion'}]},
                   {'faces': [{'name': 'Target'}]}, {'faces': [{'name': 'Near match'}]}]
        index = {'0': {'name': 'Target', 'record': 0, 'face': 0},
                 '1': {'name': 'Companion', 'record': 0, 'face': 1},
                 '2': {'name': 'Target', 'record': 1, 'face': 0},
                 '3': {'name': 'Near match', 'record': 2, 'face': 0}}
        task = {'target_faces': ['Target'], 'cohort': ['Companion', 'Near match', 'Target']}
        data = (index, records, [])
        run = Path('/private/task')
        for failures in ([], [{'face': 'Target', 'lost': ['exact_default']}]):
            with self.subTest(failures=failures), \
                 patch.object(a.w, 'payload', return_value=data), \
                 patch.object(a.w, 'checks', return_value={'faces': 3, 'failures': failures}) as checks, \
                 patch.object(a, 'edited_checks') as edited:
                report = a._preview_checks(run, run/'logs', run/'grammar', task)
                checks.assert_called_once_with(run/'inputs', run/'logs', run/'grammar',
                                               ['Companion', 'Target'], data, 2)
                self.assertEqual(report['failures'], failures)
                self.assertEqual(report['deferred_checks'], ['full assigned cohort', 'edited operand checks'])
                edited.assert_not_called()
        with self.assertRaisesRegex(ValueError, 'full assigned cohort'):
            a._check(run, 'task', coordinator=True, preview=True)

    def test_edited_pair_preserves_independent_face_order_and_refuses_ambiguity(self):
        gold = [{'faces': [{'name': 'Back'}, {'name': 'Front'}]}]
        oracle = [{'cards': [{'name': 'Front', 'text': 'front'}, {'name': 'Back', 'text': 'back'}]}]
        paired_gold, paired_oracle = a.edited_record_pair(gold, oracle, 'Back')
        self.assertEqual([f['name'] for f in paired_gold['faces']], ['Back', 'Front'])
        self.assertEqual([f['name'] for f in paired_oracle['cards']], ['Front', 'Back'])
        paired_oracle['cards'][1]['text'] = 'changed'
        self.assertEqual(oracle[0]['cards'][1]['text'], 'back')
        with self.assertRaises(ValueError):
            a.edited_record_pair(gold + gold, oracle, 'Back')
        with self.assertRaises(ValueError):
            a.edited_record_pair(gold, oracle + oracle, 'Back')
        with self.assertRaises(ValueError):
            a.edited_record_pair(gold, [{'cards': [{'name': 'Back'}, {'name': 'Other'}]}], 'Back')

    def test_author_family_cannot_disable_compositional_checks(self):
        family = {'cases': [{'expected': []}, {'reject': True}, {}, {'compositional': True}]}
        a.validate_author_family(family)
        family['cases'][2]['compositional'] = None
        a.validate_author_family(family)
        for invalid in (False, 0, 'false'):
            family['cases'][2]['compositional'] = invalid
            with self.subTest(invalid=invalid), self.assertRaisesRegex(ValueError, 'compositional'):
                a.validate_author_family(family)
        with self.assertRaises(ValueError):
            a.validate_author_family({'cases': [{'expected': []}, {'reject': True}]})

    def test_edited_operands_require_the_contextual_family_after_score_and_native(self):
        for allow_seeded, family_exit in ((True, 1), (False, 0)):
            with self.subTest(allow_seeded=allow_seeded), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                directory = root/'evaluation'
                directory.mkdir()
                task = {'edits': 'edits.json', 'target_faces': ['Example'], 'family_case_modes': allow_seeded}
                face = {'name': 'Example', 'abilities': [{'amount': 1}], 'characteristics': {'types': ['Creature']}}
                gold = [{'faces': [face]}]
                oracle = [{'cards': [{'name': 'Example', 'text': 'Original'}]}]
                r.write(root/task['edits'], [
                    {'face': 'Example', 'set': [{'path': [0, 'amount'], 'value': value}], 'text': f'Value {value}'}
                    for value in (2, 3)])
                calls = []

                def execute(run, folder, grammar, program, arguments, threads):
                    calls.append(program)
                    if program == 'score':
                        text = r.read(folder/'oracle.json')[0]['cards'][0]['text']
                        r.jsonl(folder/'rows.jsonl', [{'name': 'Example', 'text': text,
                                                       'status': 'equal', 'exact': True}])
                    else:
                        r.write(folder/'native.json', {'readings': {'total': 1, 'valid': 1, 'canonical': 1},
                                                       'source_gold': {'valid': 1}})

                def check_family(run, folder, grammar, program, arguments):
                    calls.append(program)
                    self.assertEqual(arguments, [folder/'family.json', '--compositional'])
                    cases = r.read(arguments[0])['cases']
                    self.assertEqual([case['expected'] for case in cases], [[{'amount': 2}], [{'amount': 3}]])
                    self.assertEqual([case['context'] for case in cases], [face['characteristics']]*2)
                    self.assertTrue(all(case['exact'] and case['default'] for case in cases))
                    self.assertEqual(['compositional' in case for case in cases], [allow_seeded]*2)
                    self.assertEqual([case.get('compositional') for case in cases], [False if allow_seeded else None]*2)
                    return family_exit

                with patch.object(r, 'execute', side_effect=execute), patch.object(a, 'helper', side_effect=check_family):
                    args = (root, directory, root/'grammar', task, root, ({}, gold, oracle))
                    if family_exit:
                        with self.assertRaisesRegex(ValueError, 'edited operand family checks failed'):
                            a.edited_checks(*args)
                    else:
                        self.assertEqual(len(a.edited_checks(*args)), 2)
                self.assertEqual(calls, ['score', 'validate-readings']*2+['family-check'])

    def test_family_failure_skips_cohort_and_edited_but_success_runs_all(self):
        for family_code in (1, 0):
            with self.subTest(family_code=family_code), tempfile.TemporaryDirectory() as tmp:
                run, root = Path(tmp)/'run', Path(tmp)/'worker'
                for folder in (run/'tasks', root/'.cache', root/'grammar/syntax', root/'families', root/'tests/fixtures'):
                    folder.mkdir(parents=True)
                (root/'grammar/syntax/example.ebnf').write_text('example ::= "example"\n')
                r.write(run/'tasks/task.json', {'task': 'test'})
                r.write(run/'support.json', {'files': {}})
                fixture = root/'families/case.json'
                r.write(fixture, {'cases': [{'expected': []}, {'reject': True}, {}, {}]})
                r.write(root/'tests/fixtures/edits.json', [])
                task = {'rules': ['m_seed'], 'fixture': 'families/case.json',
                        'edits': 'tests/fixtures/edits.json', 'cohort': ['Example']}
                verification = (task, root, {'commit': 'base'})
                with patch.object(a, 'verify', return_value=verification) as verify, \
                     patch.object(a, 'seeded', side_effect=[{'m_seed'}, set()]), \
                     patch.object(a, 'check_seeded_files'), \
                     patch.object(a, 'apply_hooks', return_value=[]), \
                     patch.object(a, 'helper', return_value=family_code) as helper, \
                     patch.object(a.w, 'payload', return_value='data') as payload, \
                     patch.object(a.w, 'checks', return_value={'faces': 3, 'failures': []}) as checks, \
                     patch.object(a, 'edited_checks', return_value=[{'face': 'Example'}]) as edited, \
                     redirect_stdout(io.StringIO()):
                    report = a._check(run, 'task', coordinator=True)
                self.assertEqual(verify.call_count, 2)
                helper.assert_called_once()
                self.assertEqual(r.read(Path(report['directory'])/'report.json'), report)
                edits = Path(report['directory'])/'edits.json'
                self.assertEqual(edits.read_bytes(), (root/task['edits']).read_bytes())
                self.assertEqual(r.digest(edits), report['edits_sha256'])
                if family_code:
                    self.assertFalse(report['passed'])
                    self.assertEqual(report['failures'], ['family regression tests failed'])
                    self.assertEqual(report['checks_skipped']['reason'],
                                     'authored compositional family regression failed')
                    self.assertEqual(set(report['checks_skipped']['checks']),
                                     {'cohort score', 'cohort native validation', 'edited operand checks'})
                    self.assertNotIn('faces', report)
                    self.assertNotIn('edited', report)
                    payload.assert_not_called()
                    checks.assert_not_called()
                    edited.assert_not_called()
                else:
                    self.assertTrue(report['passed'])
                    self.assertEqual(report['faces'], 3)
                    self.assertEqual(report['edited'], [{'face': 'Example'}])
                    self.assertNotIn('checks_skipped', report)
                    payload.assert_called_once()
                    checks.assert_called_once()
                    edited.assert_called_once()

    def test_operand_edits_change_a_leaf_without_replacing_programs(self):
        source = [{'args': {'amount': 1}}]
        changed = copy.deepcopy(source)
        a.edit_leaf(changed, [0, 'args', 'amount'], 2)
        self.assertEqual(source[0]['args']['amount'], 1)
        self.assertEqual(changed[0]['args']['amount'], 2)
        for path, value in [([0, 'args'], {}), ([0, 'args', 'amount'], 2)]:
            with self.assertRaises(ValueError):
                a.edit_leaf(changed, path, value)

    def test_identifier_renames_do_not_count_as_edited_operand_tests(self):
        for field in ('result', 'slot', 'binding', 'id'):
            with self.assertRaisesRegex(ValueError, 'identifier'):
                a.edit_leaf([{field: 0}], [0, field], 1)

    def test_mechanical_removal_repairs_hook_damage_and_preserves_authored_modules(self):
        with tempfile.TemporaryDirectory() as tmp:
            base, root = Path(tmp)/'base', Path(tmp)/'worker'
            for folder in (base, root):
                (folder/'grammar/syntax').mkdir(parents=True)
                (folder/'grammar/syntax/mined.ebnf').write_text(
                    'mined_action ::= m_a | m_b\n'
                    '/* @sem A */\nm_a ::= "alpha"\n'
                    '/* @sem B */\nm_b ::= "beta"\n')
            (root/'grammar/syntax/mined.ebnf').write_text('broken dangling hook\n')
            (root/'grammar/syntax/custom.ebnf').write_text('authored phrase\n')
            result = a.regenerate_removals(base, root, {'rules': ['m_a'],
                'write_files': ['grammar/syntax/mined.ebnf', 'grammar/syntax/custom.ebnf']})
            self.assertEqual(result['removed'], ['m_a'])
            self.assertEqual(a.seeded(root), {'m_b'})
            text = (root/'grammar/syntax/mined.ebnf').read_text()
            self.assertIn('mined_action ::= m_b', text)
            self.assertIn('/* @sem B */\nm_b', text)
            self.assertEqual((root/'grammar/syntax/custom.ebnf').read_text(), 'authored phrase\n')
            self.assertEqual((Path(result['backups'])/'syntax/mined.ebnf').read_text(),
                             'broken dangling hook\n')

    def test_root_hooks_reject_direct_edits_even_in_an_allowed_syntax_module(self):
        with tempfile.TemporaryDirectory() as tmp:
            run, root = Path(tmp)/'run', Path(tmp)/'worker'
            base = run/'inputs/frozen'
            for directory in (base/'grammar/syntax', root/'grammar/syntax',
                              run/'tasks', run/'support/runner'):
                directory.mkdir(parents=True)
            original = ('/* @sem Ability::$existing */\nability ::= existing\n'
                        'line ::= existing_line\nevent_clause ::= existing_event\n')
            for folder in (base, root):
                (folder/'grammar/syntax/card.ebnf').write_text(original)
                (folder/'grammar/syntax/events.ebnf').write_text(
                    '/* @sem Ability::Triggered{} */\nshared_trigger ::= "at"\n')
            descriptor = run/'tasks/task.json'
            r.write(descriptor, {'checkout': str(root), 'write_files': ['grammar/syntax/card.ebnf']})
            shutil.copy2(Path(a.__file__), run/'support/runner/retire_author.py')
            r.write(run/'support.json', {'files': r.tree(run/'support'),
                                        'tasks': {'task': r.digest(descriptor)}})
            mock_manifest = {'commit': 'base', 'source': 'compiled'}
            def command(args, **kwargs):
                return 'base\n' if 'rev-parse' in args else ''
            with patch.object(r, 'verify', return_value=mock_manifest), \
                 patch.object(a.dev, 'source', return_value='compiled'), \
                 patch.object(a.subprocess, 'check_output', side_effect=command), \
                 patch.object(a, 'changed_files', return_value=['grammar/syntax/card.ebnf']):
                a.verify(run, 'task')
                with (root/'grammar/syntax/events.ebnf').open('a') as stream:
                    stream.write('/* @sem EventSpec::Attack */\nnew_event ::= "attacks"\n')
                a.check_root_hooks(base, root)
                (root/'grammar/syntax/card.ebnf').write_text(original.replace(
                    'ability ::= existing', 'ability ::= existing | shared_trigger'))
                with self.assertRaisesRegex(ValueError, 'direct root'):
                    a.verify(run, 'task')
                (root/'grammar/syntax/card.ebnf').write_text(original.replace(
                    'ability ::= existing', 'ability ::=   existing'))
                a.verify(run, 'task')
            # The same addition is accepted through the typed diagnostic hook.
            a.check_root_hooks(base, root)
            a.apply_hooks(root/'grammar', [{'root': 'ability', 'production': 'shared_trigger'}])
            self.assertIn('existing | shared_trigger', (root/'grammar/syntax/card.ebnf').read_text())

    def test_root_hook_semantics_and_locations_cannot_change(self):
        with tempfile.TemporaryDirectory() as tmp:
            base, root = Path(tmp)/'base', Path(tmp)/'worker'
            for folder in (base, root):
                (folder/'grammar/syntax').mkdir(parents=True)
                (folder/'grammar/syntax/card.ebnf').write_text(
                    '/* @sem Ability::$original */\nability ::= original\n')
            path = root/'grammar/syntax/card.ebnf'
            path.write_text(path.read_text().replace('$original', '$different'))
            with self.assertRaisesRegex(ValueError, 'direct root'):
                a.check_root_hooks(base, root)
            path.write_text('')
            (root/'grammar/syntax/moved.ebnf').write_text(
                '/* @sem Ability::$original */\nability ::= original\n')
            with self.assertRaisesRegex(ValueError, 'direct root'):
                a.check_root_hooks(base, root)

    def test_ordinary_hook_registration_is_typed_and_rejects_mined_roots(self):
        with tempfile.TemporaryDirectory() as tmp:
            grammar = Path(tmp)/'grammar'
            (grammar/'syntax').mkdir(parents=True)
            (grammar/'syntax/card.ebnf').write_text('ability ::= existing\nevent_clause ::= event_when\n')
            (grammar/'syntax/events.ebnf').write_text(
                '/* @sem Ability::Triggered{} */\nshared_trigger ::= "at"\n')
            requests = [{'root': 'ability', 'production': 'shared_trigger'}]
            a.apply_hooks(grammar, requests)
            self.assertIn('ability ::= existing | shared_trigger',
                          (grammar/'syntax/card.ebnf').read_text())
            for root in ('mined_ability', 'event_clause'):
                with self.assertRaises(ValueError):
                    a.apply_hooks(grammar, [{'root': root, 'production': 'shared_trigger'}])

    def test_seeded_dispatcher_cannot_hide_a_renamed_handwritten_root(self):
        with tempfile.TemporaryDirectory() as tmp:
            base, root, output = Path(tmp)/'base', Path(tmp)/'worker', Path(tmp)/'output'
            (base/'grammar/syntax').mkdir(parents=True)
            (base/'grammar/syntax/mined.ebnf').write_text(
                'mined_action ::= m_a | m_b\n'
                '/* @sem A */\nm_a ::= "alpha"\n'
                '/* @sem B */\nm_b ::= "beta"\n')
            shutil.copytree(base/'grammar', root/'grammar')
            a.regenerate_removals(base, root, {'rules': ['m_a'],
                'write_files': ['grammar/syntax/mined.ebnf']})
            output.mkdir()
            a.check_seeded_files(base, root, ['m_a'], output)
            p = root/'grammar/syntax/mined.ebnf'
            p.write_text(p.read_text().replace('mined_action ::= m_b',
                                               'mined_action ::= m_b | renamed_seed'))
            second = Path(tmp)/'second'; second.mkdir()
            with self.assertRaisesRegex(ValueError, 'non-removal edits'):
                a.check_seeded_files(base, root, ['m_a'], second)

    def test_descriptor_and_support_hashes_fail_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = Path(tmp)
            (run/'support').mkdir()
            (run/'tasks').mkdir()
            r.write(run/'tasks/task.json', {'checkout': '/unused'})
            r.write(run/'support.json', {'files': {}, 'tasks': {'task': 'wrong'}})
            with patch.object(r, 'verify', return_value={}):
                with self.assertRaisesRegex(ValueError, 'descriptor'):
                    a.verify(run, 'task')
            (run/'support/extra').write_text('changed')
            with self.assertRaisesRegex(ValueError, 'tooling'):
                a.verify(run, 'task')

    def test_pair_prompt_is_identical_apart_from_assignment_address(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = Path(tmp)
            (run/'tasks').mkdir()
            task = {'goal': 'share a phrase', 'rules': ['m_example'],
                    'write_files': ['grammar/syntax/events.ebnf', 'families/pilot.json'],
                    'fixture': 'families/pilot.json', 'edits': 'tests/fixtures/edits.json'}
            r.write(run/'tasks/luna-1.json', task)
            r.write(run/'tasks/terra-1.json', task)
            left = pool.prompt(run, 'luna-1').replace('luna-1', 'PAIR')
            right = pool.prompt(run, 'terra-1').replace('terra-1', 'PAIR')
            self.assertEqual(left, right)
            self.assertIn('No Cargo', left)
            self.assertIn('baseline successes', left)

    def test_fast_pair_reuses_a_slot_without_waiting_for_unrelated_tasks(self):
        next_started = threading.Event()
        ordering = []
        lock = threading.Lock()
        def launch(run, task):
            with lock:
                ordering.append(('start', task))
            if task in ('luna-1', 'luna-3'):
                self.assertTrue(next_started.wait(3), 'fast pair waited on the entire first wave')
            if task == 'luna-2':
                next_started.set()
            with lock:
                ordering.append(('end', task))
            return {'task': task}
        plan = {'waves': [['luna-1', 'terra-2', 'luna-3'], ['terra-1', 'luna-2', 'terra-3']]}
        with patch.object(pool, 'launch', side_effect=launch):
            results = pool.run_pairs(Path('/unused'), plan)
        self.assertEqual(len(results), 6)
        self.assertEqual(len({r['task'] for r in results}), 6)
        for first, second in [('luna-1', 'terra-1'), ('terra-2', 'luna-2'), ('luna-3', 'terra-3')]:
            self.assertLess(ordering.index(('end', first)), ordering.index(('start', second)))
        self.assertLess(ordering.index(('start', 'luna-2')), ordering.index(('end', 'luna-1')))


if __name__ == '__main__':
    unittest.main()
