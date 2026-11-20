from pathlib import Path
import tempfile
import unittest

import retire as r
import retire_batch as batch


class ReusePlanTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.campaign = Path(temporary.name)
        previous = self.campaign/'.cache/batches/previous'
        self.rows = previous/'full-checks/rows.jsonl'
        self.traces = self.rows.parent/'traces.jsonl'
        self.inputs = previous/'validation-inputs'
        grammar = self.inputs/'frozen/grammar'
        (grammar/'syntax').mkdir(parents=True)
        (grammar/'syntax/card.ebnf').write_text('card ::= "a"\n')
        self.rows.parent.mkdir(parents=True)
        self.rows.write_text('{"name":"A"}\n')
        self.traces.write_text('{"format":1}\n{"name":"A"}\n')
        r.write(self.inputs/'manifest.json', {'files': {'grammar/syntax/card.ebnf': r.digest(grammar/'syntax/card.ebnf'),
                                                        'gold.jsonl': 'x'}})
        self.record({'streak': 0})
        self.checks = self.campaign/'.cache/batches/next/full-checks'
        self.checks.mkdir(parents=True)
        self.checkpoint = {'baseline': str(self.rows), 'inputs': str(self.inputs), 'full_check_round_trip': 'gated',
                           'full_check_reuse': 'on', 'full_check_reuse_audit_every': 3}

    def record(self, reuse, **extra):
        r.write(self.rows.parent.parent/'validation.json',
                {'rows': str(self.rows), 'full_check_traces_sha256': r.digest(self.traces), 'print_reuse': reuse,
                 **extra})

    def plan(self):
        return batch._reuse_arguments(self.campaign, self.checkpoint, self.checks)

    def test_settings_default_off_and_need_the_gated_round_trip(self):
        self.assertEqual(batch._reuse_settings({}), ('off', 4))
        with self.assertRaisesRegex(ValueError, 'gated round trip'):
            batch._reuse_settings({'full_check_reuse': 'on'})
        for bad in ({'full_check_reuse': True}, {'full_check_reuse_audit_every': 0},
                    {'full_check_reuse_audit_every': 4.0}):
            with self.assertRaisesRegex(ValueError, 'off, shadow or on'):
                batch._reuse_settings(bad)

    def test_off_keeps_the_plain_print(self):
        self.checkpoint['full_check_reuse'] = 'off'
        self.assertEqual(self.plan(), ([], None))

    def test_reuse_counts_batches_since_the_last_full_audit(self):
        arguments, plan = self.plan()
        self.assertEqual((plan['mode'], plan['streak']), ('on', 1))
        self.assertIn(f'--reuse-rows={self.rows}', arguments)
        self.assertIn(f'--reuse-grammar={self.inputs/"frozen/grammar"}', arguments)
        self.assertIn(f'--trace-out={self.checks/"traces.jsonl"}', arguments)
        self.record({'streak': 2})
        arguments, plan = self.plan()
        self.assertEqual((plan['mode'], plan['streak']), ('shadow', 0))
        self.assertIn('--reuse-mode=shadow', arguments)

    def test_a_contradiction_audits_every_batch_until_cleared(self):
        r.write(self.campaign/batch.REUSE_CONTRADICTED, {'batch': 'x', 'faces': ['A']})
        self.assertEqual(self.plan()[1]['mode'], 'shadow')
        self.checkpoint['full_check_reuse'] = 'shadow'
        (self.campaign/batch.REUSE_CONTRADICTED).unlink()
        self.assertEqual(self.plan()[1]['mode'], 'shadow')

    def test_untraced_or_altered_previous_prints_only_trace(self):
        for damage in (lambda: self.traces.write_text('{"format":1}\n'),
                       lambda: (self.inputs/'frozen/grammar/syntax/card.ebnf').write_text('card ::= "b"\n'),
                       lambda: self.record({'streak': 0}, rows=str(self.traces))):
            with self.subTest(damage=damage):
                damage()
                arguments, plan = self.plan()
                self.assertEqual(plan, {'mode': 'trace', 'streak': 0, 'previous': None})
                self.assertEqual(arguments, [f'--trace-out={self.checks/"traces.jsonl"}'])
                self.setUp()

    def test_evidence_marks_contradictions_and_unusable_runs(self):
        (self.checks/'traces.jsonl').write_text('{}\n')
        plan = {'mode': 'shadow', 'streak': 0, 'previous': str(self.rows)}
        r.write(self.checks/'reuse.json', {'reused': 0, 'printed': 3, 'unseen': 0, 'predicted': 2,
                                           'reasons': {'shadow': 2}, 'contradicted': ['B']})
        evidence = batch._reuse_evidence(self.campaign, self.checks, plan, 'next')
        self.assertEqual((evidence['contradicted'], evidence['predicted']), (1, 2))
        self.assertEqual(r.read(self.campaign/batch.REUSE_CONTRADICTED), {'batch': 'next', 'faces': ['B']})
        r.write(self.checks/'reuse.json', {'unavailable': 'another build', 'reused': 0})
        evidence = batch._reuse_evidence(self.campaign, self.checks, dict(plan, mode='on', streak=2), 'next')
        self.assertEqual((evidence['mode'], evidence['streak'], evidence['unavailable']),
                         ('trace', 0, 'another build'))


if __name__ == '__main__':
    unittest.main()
