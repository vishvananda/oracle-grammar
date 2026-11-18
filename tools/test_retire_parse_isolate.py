"""The isolation planner needs a passing final union before proposing survivors."""
from contextlib import nullcontext
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch

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

    def test_retry_reuses_independent_witness_and_checks_the_surviving_union(self):
        with TemporaryDirectory() as temporary:
            campaign = Path(temporary)
            inputs = campaign/'inputs'
            grammar = inputs/'frozen/grammar'
            grammar.mkdir(parents=True)
            (grammar/'card.ebnf').write_text('card ::= "fixture"\n')
            folder = campaign/'batch'
            folder.mkdir()
            for name in ('manifest.json', 'validation.json', 'repository-check.log'):
                (folder/name).write_text('{}\n')
            jobs = [{'job_id': str(i), 'attempt_id': str(i), 'kind': 'author', 'rules': [str(i)]}
                    for i in range(4)]
            cp = {'commit': 'accepted', 'cpu_pool': str(campaign/'cpu'), 'cpu_capacity': 4}
            manifest = {'commit': 'rejected', 'grammar': isolate.r.tree(grammar)}
            archived = (folder, manifest, jobs, cp, inputs, inputs, ['Fixture'], {})
            scratch = campaign/'.cache/proof'
            trials = []

            def checked(_, _control, root):
                indices = isolate.r.read(root/'identity.json')['indices']
                trials.append(indices)
                return {'failures': ['regression'] if 1 in indices else []}

            with (
                patch.object(isolate, '_archived', return_value=archived),
                patch.object(isolate, '_proposals', return_value=[(i,) for i in range(4)]),
                patch.object(isolate, '_apply'),
                patch.object(isolate.batch, '_control_setup', return_value={'occurrences': 1}),
                patch.object(isolate.cpu, 'reserve', side_effect=lambda *a, **k: nullcontext()),
                patch.object(isolate, '_check', side_effect=checked),
                patch.object(isolate, '_controls', side_effect=lambda roots: {k: str(v) for k, v in roots.items()}),
            ):
                result = isolate.isolate(campaign, 'batch', scratch, 5, witness_retry=3)
            self.assertEqual(result['jobs'], ['0', '2', '3'])
            self.assertEqual(trials, [[], [0, 1, 2, 3], [0, 1], [0], [1], [0, 2, 3]])
            proof = isolate.r.read(scratch/'isolation.json')
            self.assertEqual(proof['excluded_jobs'], ['1'])
            self.assertEqual(proof['controls']['base'], str(scratch/'base'))
            self.assertEqual(proof['controls']['full'], str(scratch/'full'))
