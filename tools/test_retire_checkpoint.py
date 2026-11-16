"""A private checkpoint requires complete gates and exact audit identities."""
from pathlib import Path
import tempfile
import unittest

import retire as r
import retire_checkpoint as c


class CheckpointTests(unittest.TestCase):
    def test_every_repository_gate_is_required(self):
        lines = [f'{gate} passed 1.0 s' for gate in sorted(c.GATES)]
        self.assertTrue(c.gate_results('\n'.join(lines)+'\n')['passed'])
        self.assertFalse(c.gate_results('\n'.join(lines[:-1])+'\n')['complete'])
        self.assertFalse(c.gate_results('\n'.join(lines).replace('clippy passed',
                                                               'clippy FAILED')+'\n')['passed'])
        self.assertFalse(c.gate_results('\n'.join([*lines, lines[0]])+'\n')['passed'])

    def test_unchanged_gate_reuse_is_evidence_but_missing_output_is_not(self):
        lines = [f'{gate} reused 0.0 s' for gate in c.GATES]
        self.assertTrue(c.gate_results('\n'.join(lines)+'\n')['passed'])
        self.assertFalse(c.gate_results('Started check process\n')['complete'])

    def test_an_audit_for_another_snapshot_patch_or_grammar_never_accepts(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = Path(tmp)
            (run/'combined').mkdir()
            r.write(run/'manifest.json', {'id': 1})
            (run/'combined/retirement.patch').write_text('patch\n')
            grammar = {'syntax/test.ebnf': 'digest'}
            validation = {'status': 'validated', 'failures': [], 'grammar': grammar,
                          'snapshot': r.digest(run/'manifest.json'),
                          'patch': r.digest(run/'combined/retirement.patch')}
            self.assertTrue(c.accepted(validation, run, {'grammar': grammar}))
            for key in ('snapshot', 'patch', 'grammar'):
                altered = dict(validation, **{key: 'different'})
                self.assertFalse(c.accepted(altered, run, {'grammar': grammar}))
            self.assertFalse(c.accepted(dict(validation, failures=['lost reading']),
                                        run, {'grammar': grammar}))


if __name__ == '__main__':
    unittest.main()
