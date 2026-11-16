"""Authoring trials constrain operands, source identities and model isolation."""
import copy
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
