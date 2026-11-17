import copy
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import unittest

from retire_queue import Queue
import retire_review


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class CoordinatorReviewTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root/'.cache').mkdir()
        self.queue = Queue(self.root/'queue.sqlite')
        self.queue.add_checkpoint('base', 'proof')
        self.entries = []
        for number in range(5):
            job = f'job{number}'
            rules = [f'r{number}-{i}' for i in range(1 if number == 0 else 3)]
            self.queue.add_jobs([{'id': job, 'kind': 'author', 'rules': rules}])
            claim = self.queue.claim(f'worker{number}', kinds=['author'])
            attempt = claim['attempt_id']
            run = self.root/'.cache/attempts'/attempt
            (run/'tasks').mkdir(parents=True)
            descriptor = run/'tasks'/f'{job}.json'
            descriptor.write_text(json.dumps({'id': job, 'rules': rules}))
            support = run/'support.json'
            support.write_text(json.dumps({'tasks': {job: digest(descriptor)}}))
            self.queue.finish(attempt, 'needs-review', {'error': 'timeout'},
                              {rule: {'status': 'needs-review', 'reason': 'timeout', 'evidence': 'receipt'}
                               for rule in rules})
            with sqlite3.connect(self.root/'queue.sqlite') as db:
                result = db.execute('select result from attempts where id=?', (attempt,)).fetchone()[0]
            folder = self.root/'.cache'/f'evidence{number}'
            folder.mkdir()
            nested = folder/'nested.txt'
            nested.write_text('retained diagnostic')
            nested_spec = {'path': str(nested), 'sha256': digest(nested)}
            if number == 0:
                review = {'kind': 'coordinator-review-supplement', 'acceptance': False, 'rule': rules[0],
                          'removed': rules, 'results': {'baseline': {'native_exit': 0},
                                                        'removed': {'rows': [{'status': 'refused'}]}},
                          'input_hashes': {str(nested): digest(nested)}}
            else:
                review = {'kind': 'coordinator-review-supplement', 'acceptance': False, 'job_id': job,
                          'source_checkpoint': 'base',
                          'attempt_id': attempt, 'untested_gates': ['full audit'],
                          'rules': [{'rule': rule, 'finding': f'explicit {rule}', 'proof': nested_spec}
                                    for rule in rules]}
            rp = folder/'review.json'
            rp.write_text(json.dumps(review))
            index = {'kind': 'coordinator-review-supplement-index', 'acceptance': False,
                     'requires_deeper_review': True, 'queue_changes': False, 'job_id': job,
                     'attempt_id': attempt, 'source_checkpoint': 'base', 'scope': 'bounded cohort',
                     'status': 'coordinator-triaged: seeded parse dependency',
                     'model_disposition': 'unavailable', 'evidence': {'path': str(rp), 'sha256': digest(rp)}}
            index['rule' if number == 0 else 'rules'] = rules[0] if number == 0 else rules
            ip = folder/'index.json'
            ip.write_text(json.dumps(index))
            self.entries.append({'job_id': job, 'attempt_id': attempt, 'rules': rules,
                                 'index': {'path': str(ip), 'sha256': digest(ip)},
                                 'review': {'path': str(rp), 'sha256': digest(rp)},
                                 'attempt_result_sha256': hashlib.sha256(result.encode()).hexdigest(),
                                 'descriptor_sha256': digest(descriptor), 'support_sha256': digest(support)})
        self.manifest = self.root/'.cache/manifest.json'
        self.rewrite_manifest()

    def rewrite_manifest(self, entries=None, roots=None):
        self.manifest.write_text(json.dumps({'kind': 'private-coordinator-triage-join',
                                             'indices': self.entries if entries is None else entries,
                                             'evidence_roots': [] if roots is None else roots}))

    def read(self):
        return retire_review.review(self.root, self.manifest, digest(self.manifest))

    def test_default_unchanged_and_all_thirteen_are_separate(self):
        raw = retire_review.review(self.root)
        joined = self.read()
        self.assertEqual(joined['coordinator_supplements'], {'matched_rules': 13, 'unmatched': []})
        del joined['coordinator_supplements']
        for job in joined['jobs']:
            for rule in job['rules']:
                self.assertIsNone(rule['model_status'])
                self.assertEqual(rule['assessment'], 'unclassified')
                self.assertFalse(rule['coordinator_triage']['acceptance'])
                self.assertTrue(rule['coordinator_triage']['untested_gates'])
                del rule['coordinator_triage']
        self.assertEqual(joined, raw)
        script = Path(retire_review.__file__)
        proc = subprocess.run([sys.executable, str(script), str(self.root), '--coordinator-manifest',
                               str(self.manifest)], capture_output=True)
        self.assertNotEqual(proc.returncode, 0)
        self.assertEqual(proc.stdout, b'')

    def test_fail_closed_tamper_missing_notes_path_and_duplicate(self):
        entry = self.entries[1]
        p = Path(entry['review']['path'])
        original = p.read_text()
        p.write_text(original+' ')
        with self.assertRaises(ValueError):
            self.read()
        p.write_text(original)
        review = json.loads(original)
        del review['rules'][0]['finding']
        p.write_text(json.dumps(review))
        entry['review']['sha256'] = digest(p)
        index_path = Path(entry['index']['path'])
        index = json.loads(index_path.read_text())
        index['evidence'] = entry['review']
        index_path.write_text(json.dumps(index))
        entry['index']['sha256'] = digest(index_path)
        self.rewrite_manifest()
        with self.assertRaises(ValueError):
            self.read()
        self.check_path_and_duplicate()

    def check_path_and_duplicate(self):
        # Path escape and duplicate binding are validated independently of review content.
        entries = copy.deepcopy(self.entries)
        entries[0]['index']['path'] = '/etc/passwd'
        self.rewrite_manifest(entries)
        with self.assertRaises(ValueError):
            self.read()
        entries = [copy.deepcopy(self.entries[0]), copy.deepcopy(self.entries[0])]
        self.rewrite_manifest(entries)
        with self.assertRaises(ValueError):
            self.read()

    def test_nested_tamper_and_facts_only_fail_closed(self):
        entry = self.entries[4]
        review_path = Path(entry['review']['path'])
        review = json.loads(review_path.read_text())
        nested = Path(review['rules'][0]['proof']['path'])
        nested.write_text('changed')
        with self.assertRaises(ValueError):
            self.read()
        nested.write_text('retained diagnostic')
        # A facts-only file does not become a coordinator judgment by linking it.
        review.pop('untested_gates')
        review['rules'] = [{'rule': r, 'proof': review['rules'][0]['proof']}
                           for r in entry['rules']]
        review_path.write_text(json.dumps(review))
        entry['review']['sha256'] = digest(review_path)
        index_path = Path(entry['index']['path'])
        index = json.loads(index_path.read_text())
        index['evidence'] = entry['review']
        index_path.write_text(json.dumps(index))
        entry['index']['sha256'] = digest(index_path)
        self.rewrite_manifest()
        with self.assertRaises(ValueError):
            self.read()

    def test_manifest_hash_symlink_and_duplicate_json_key(self):
        with self.assertRaises(ValueError):
            retire_review.review(self.root, self.manifest, '0' * 64)
        link = self.root/'.cache/linked-index.json'
        link.symlink_to(self.entries[0]['index']['path'])
        entries = copy.deepcopy(self.entries)
        entries[0]['index']['path'] = str(link)
        self.rewrite_manifest(entries)
        with self.assertRaises(ValueError):
            self.read()
        self.manifest.write_text('{"kind":"private-coordinator-triage-join",'
                                 '"kind":"private-coordinator-triage-join","indices":[]}')
        with self.assertRaises(ValueError):
            self.read()

    def test_explicit_external_root_and_root_validation(self):
        with tempfile.TemporaryDirectory() as external_dir:
            external = Path(external_dir)
            entry = self.entries[0]
            old = Path(entry['review']['path'])
            new = external / 'review.json'
            new.write_bytes(old.read_bytes())
            entry['review']['path'] = str(new)
            index = Path(entry['index']['path'])
            value = json.loads(index.read_text())
            value['evidence'] = entry['review']
            index.write_text(json.dumps(value))
            entry['index']['sha256'] = digest(index)
            self.rewrite_manifest()
            with self.assertRaises(ValueError):
                self.read()
            self.rewrite_manifest(roots=[str(external)])
            self.assertEqual(self.read()['coordinator_supplements']['matched_rules'], 13)
            self.rewrite_manifest(roots=['/'])
            with self.assertRaises(ValueError):
                self.read()
            symlink = self.root / '.cache' / 'linked-root'
            symlink.symlink_to(external, target_is_directory=True)
            self.rewrite_manifest(roots=[str(symlink)])
            with self.assertRaises(ValueError):
                self.read()

    def test_unmatched_when_accepted_new_attempt_or_partial_reclassification(self):
        with sqlite3.connect(self.root/'queue.sqlite') as db:
            db.execute("update jobs set status='accepted' where id='job1'")
            db.execute("update jobs set attempt_id='new-attempt' where id='job2'")
            db.execute("insert into attempts(id,job_id,worker,base_commit,result) select 'new-attempt',job_id,worker,base_commit,result from attempts where id=?", (self.entries[2]['attempt_id'],))
            db.execute("insert into jobs(id,payload,status) values('mechanic', ?, 'pending')", (json.dumps({'id': 'mechanic', 'kind': 'mechanical', 'rules': ['r3-2']}),))
            db.execute("update rules set job_id='mechanic' where rule='r3-2'")
        result = self.read()
        self.assertEqual(result['coordinator_supplements']['matched_rules'], 6)
        self.assertEqual(len(result['coordinator_supplements']['unmatched']), 7)
        self.assertEqual({x['reason'] for x in result['coordinator_supplements']['unmatched']},
                         {'outside-review-lanes', 'current-attempt-differs'})

    def test_inherited_origin_requires_verified_link(self):
        entry = self.entries[1]
        with sqlite3.connect(self.root/'queue.sqlite') as db:
            original = db.execute('select result from attempts where id=?', (entry['attempt_id'],)).fetchone()[0]
            origin = {'job_id': 'job1', 'attempt_id': entry['attempt_id'], 'status': 'needs-review',
                      'result_sha256': hashlib.sha256(original.encode()).hexdigest(),
                      'adjudication_sha256': hashlib.sha256(b'null').hexdigest()}
            child = {'id': 'child', 'kind': 'author', 'rules': entry['rules'][:2], 'review_origin': origin}
            db.execute("update jobs set status='superseded' where id='job1'")
            db.execute("insert into jobs(id,payload,status) values('child',?,'needs-review')", (json.dumps(child),))
            db.executemany("update rules set job_id='child' where rule=?", [(r,) for r in child['rules']])
            db.execute("insert into jobs(id,payload,status) values('mechanic',?,'pending')", (json.dumps({'id': 'mechanic', 'rules': entry['rules'][2:]}),))
            db.execute("update rules set job_id='mechanic' where rule=?", (entry['rules'][2],))
            db.execute("insert into events(kind,data) values('rehabilitated-unused',?)", (json.dumps({'review_job':'child','origin_attempt':entry['attempt_id']}),))
        result = self.read()
        child = next(j for j in result['jobs'] if j['job_id'] == 'child')
        self.assertTrue(child['review_origin_valid'])
        self.assertTrue(all('coordinator_triage' in r for r in child['rules']))
        self.assertEqual(len(result['coordinator_supplements']['unmatched']), 1)
        with sqlite3.connect(self.root/'queue.sqlite') as db:
            db.execute("delete from events where kind='rehabilitated-unused'")
        result = self.read()
        child = next(j for j in result['jobs'] if j['job_id'] == 'child')
        self.assertFalse(child['review_origin_valid'])
        self.assertTrue(all('coordinator_triage' not in r for r in child['rules']))
        self.assertEqual(len(result['coordinator_supplements']['unmatched']), 3)


if __name__ == '__main__':
    unittest.main()
