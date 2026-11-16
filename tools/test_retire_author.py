"""Authoring trials constrain operands, source identities and model isolation."""
import copy
import unittest
from unittest.mock import patch
from pathlib import Path
import tempfile
import threading

import retire_author as a
import retire_author_pool as pool
import retire as r


class AuthoringTests(unittest.TestCase):
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
