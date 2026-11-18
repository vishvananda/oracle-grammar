"""The isolation planner needs a passing final union before proposing survivors."""
import unittest

import retire_parse_isolate as isolate


class SurvivorTests(unittest.TestCase):
    def test_second_failure_crosses_halves_after_first_culprit_is_removed(self):
        checked = []

        def trial(indices):
            self.assertIn(0, indices)
            self.assertEqual(indices, sorted(set(indices)))
            checked.append(indices)
            failed = 1 in indices or {2, 4} <= set(indices)
            return {'failures': ['regression'] if failed else []}

        remaining = isolate._survivors([0, 1, 2, 3, 4], trial, [0])
        self.assertEqual(remaining, [0, 3, 4])
        self.assertIn([0, 2, 3, 4], checked)
        self.assertEqual(trial(remaining)['failures'], [])

    def test_one_side_of_an_interaction_can_be_readmitted(self):
        def trial(indices):
            return {'failures': ['regression'] if {2, 3} <= set(indices) else []}

        self.assertEqual(isolate._survivors([0, 1, 2, 3], trial, [0]), [0, 1, 2])
