"""The isolation planner needs a passing final union before proposing survivors."""
import unittest

import retire_parse_isolate as isolate


class SurvivorTests(unittest.TestCase):
    def test_retry_hints_require_a_failing_singleton_with_the_pinned_prefix(self):
        jobs = [{'job_id': str(i)} for i in range(6)]
        failure = {'result': {'failures': ['regression']}}
        passed = {'result': {'failures': []}}
        seen = {(0, 1): passed, (0, 1, 2, 3): failure, (0, 1, 2): failure,
                (0, 1, 4): passed, (1, 5): failure}
        self.assertEqual(isolate._witnessed_suspects(seen, [0, 1], jobs), ['2'])

    def test_suspect_skips_bisection_when_the_remaining_union_passes(self):
        trials = []

        def trial(indices):
            trials.append(indices)
            return {'failures': ['regression'] if 2 in indices else []}

        self.assertEqual(isolate._survivors([0, 1, 2, 3], trial, [0], suspect=2), [0, 1, 3])
        self.assertEqual(trials, [[0, 1, 3]])

    def test_incorrect_suspect_falls_back_to_the_actual_contributor(self):
        def trial(indices):
            return {'failures': ['regression'] if 2 in indices else []}

        self.assertEqual(isolate._survivors([0, 1, 2, 3], trial, [0], suspect=1), [0, 1, 3])

    def test_multiple_suspects_exclude_only_reproduced_failures(self):
        trials = []

        def trial(indices):
            trials.append(indices)
            return {'failures': ['regression'] if {2, 4}.intersection(indices) else []}

        self.assertEqual(isolate._survivors([0, 1, 2, 3, 4], trial, [0], suspects=[1, 2, 4]), [0, 1, 3])
        self.assertEqual(trials, [[0, 1], [0, 2], [0, 4], [0, 1, 3]])

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
