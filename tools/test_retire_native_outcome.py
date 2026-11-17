import json
from pathlib import Path
import subprocess
import tempfile
import unittest

import retire as r
import retire_native_outcome as outcome


class PreservationOutcomeTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.run = Path(self.tmp.name)/'run'
        self.directory = Path(self.tmp.name)/'checks'
        self.directory.mkdir()
        self.index = {'0': {'name': 'A', 'text': 'A', 'parse': {'readings': 1}, 'flags': [True]*4},
                      '1': {'name': 'B', 'text': 'B', 'parse': {'readings': 1}, 'flags': [True]*4}}
        self.data = (self.index, [], [])
        r.write(self.directory/'slice.json', {'cards': ['A', 'B']})
        r.jsonl(self.directory/'before.jsonl', [x['parse'] for x in self.index.values()])
        r.jsonl(self.directory/'rows.jsonl', [{'name': n, 'text': n, 'status': 'equal', 'readings': 1,
                                                'exact': True, 'round_trip': [1, 1]} for n in ('A','B')])
        (self.directory/'native-failures.jsonl').write_text('')
        r.jsonl(self.directory/'commands.jsonl', [{'program': 'score', 'returncode': 0},
                                                   {'program': 'validate-readings', 'returncode': 1}])
        self.native = {'source_gold': {'faces': 2, 'valid': 2},
                       'canonical_gold': {'faces': 2, 'valid': 2, 'idempotent': 2},
                       'readings': {'total': 2, 'valid': 2, 'canonical': 2},
                       'reading_preservation': {'passed': False, 'occurrences': 2, 'old_readings': 2,
                           'merged_faces': 0, 'missing': [{'name': 'A', 'reading': 0}],
                           'additions': [{'name': 'B', 'occurrence': 1, 'added_current_indices': [0]}]}}
        self.write()
        cmd = [str((self.run/'frozen/bin/validate-readings').resolve()), str(self.directory/'gold.jsonl'),
               str(self.directory/'rows.jsonl'), str(self.directory/'native.json'), str(self.directory/'native-failures.jsonl'),
               '--preserve-from', str(self.directory/'before.jsonl')]
        self.error = subprocess.CalledProcessError(1, cmd)

    def write(self):
        r.write(self.directory/'native.json', self.native)

    def test_same_total_missing_and_added_reading_is_semantic_failure(self):
        result = outcome.preservation_failure(self.error, self.run, self.directory, self.data)
        self.assertEqual(result['faces'], 2)
        self.assertEqual(result['failures'], ['native readings changed or failed validation'])

    def test_unknown_command_or_exit_is_never_classified(self):
        for error in (subprocess.CalledProcessError(2, self.error.cmd),
                      subprocess.CalledProcessError(1, ['/not/the/native/helper'])):
            with self.subTest(error=error), self.assertRaises(subprocess.CalledProcessError):
                outcome.preservation_failure(error, self.run, self.directory, self.data)

    def test_incomplete_or_passing_native_reports_fail_closed(self):
        for mutate in (lambda x: x['reading_preservation'].update(passed=True),
                       lambda x: x['reading_preservation'].update(missing=[], additions=[], merged_faces=0),
                       lambda x: x['readings'].update(valid=1),
                       lambda x: x['source_gold'].update(valid=1),
                       lambda x: x['reading_preservation'].update(old_readings=1),
                       lambda x: x['reading_preservation'].update(occurrences=1),
                       lambda x: x['reading_preservation'].update(missing=[{'name': 'outside'}])):
            with self.subTest(mutate=mutate):
                original=json.loads(json.dumps(self.native));mutate(self.native);self.write()
                with self.assertRaises(ValueError):outcome.preservation_failure(self.error,self.run,self.directory,self.data)
                self.native=original
        self.write();(self.directory/'rows.jsonl').write_text('{}\n')
        with self.assertRaises((ValueError, KeyError)):outcome.preservation_failure(self.error,self.run,self.directory,self.data)

    def test_command_receipt_and_details_must_be_complete(self):
        r.jsonl(self.directory/'commands.jsonl',[{'program':'validate-readings','returncode':1}])
        with self.assertRaises(ValueError):outcome.preservation_failure(self.error,self.run,self.directory,self.data)
        r.jsonl(self.directory/'commands.jsonl',[{'program':'score','returncode':0},{'program':'validate-readings','returncode':1}])
        (self.directory/'native-failures.jsonl').write_text('{invalid\n')
        with self.assertRaises(ValueError):outcome.preservation_failure(self.error,self.run,self.directory,self.data)
        (self.directory/'native-failures.jsonl').write_text('')
        r.jsonl(self.directory/'before.jsonl',[{'readings': 9}, {'readings': 1}])
        with self.assertRaises(ValueError):outcome.preservation_failure(self.error,self.run,self.directory,self.data)


if __name__=='__main__':unittest.main()
