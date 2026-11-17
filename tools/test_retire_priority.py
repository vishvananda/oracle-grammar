import hashlib
import json
from contextlib import closing
from pathlib import Path
import tempfile
import unittest

import retire_priority as priority
from retire_queue import Queue


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value)+'\n')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class PriorityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        inputs = self.root/'checkpoints/base/inputs'
        write(self.root/'checkpoints/base.json', {'commit': 'base', 'inputs': str(inputs)})
        manifest = inputs/'manifest.json'
        write(manifest, {'format': 3, 'commit': 'base', 'population': 6})
        scan = inputs/'scan/rules.json'
        write(scan, {'summary': {'faces': 6, 'equal': 6, 'at_risk': 0, 'unused_rules': 1},
                     'unused': ['m_mech'], 'needed': {f'm_{n}': [f'Face{n}'] for n in range(6)}})
        write(inputs/'scan-complete.json', {'report': sha(scan)})
        write(inputs/'scan/commands.jsonl', {'program': 'rule-audit', 'returncode': 0})
        self.queue = Queue(self.root/'queue.sqlite')
        self.queue.add_checkpoint('base', {'inputs': str(inputs)})
        jobs = []
        for n in range(6):
            rule = f'm_{n}'
            jobs.append({'id': f'job{n}', 'kind': 'author', 'rules': [rule],
                         'requires_diagnosis': n == 5, 'syntax_neighbor_faces': [],
                         'source_fragments': [{'name': rule, 'sem': 'Rule::Thing($x)', 'body': 'word'*(n+1)}]})
        jobs.append({'id': 'mechanical', 'kind': 'mechanical', 'rules': ['m_mech']})
        self.queue.add_jobs(jobs)

    def reversed_plan(self):
        result = priority.plan(self.root)
        by_name = {row['id']: row for row in result['rows']}
        for rank, n in enumerate(reversed(range(6))):
            by_name[f'job{n}']['rank'] = rank
        return result

    def test_alpha_shapes_preserve_repeated_identity_constants_and_quoted_text(self):
        self.assertEqual(priority.semantic_shape('X($a,$a,$b,1)'), priority.semantic_shape('X($x,$x,$y,1)'))
        self.assertNotEqual(priority.semantic_shape('X($a,$a,$b,1)'), priority.semantic_shape('X($a,$b,$a,1)'))
        self.assertNotEqual(priority.semantic_shape('X($a,1)'), priority.semantic_shape('X($a,2)'))
        self.assertEqual(priority.semantic_shape(r'X("escaped \\\"$literal\\\"",$a)'), r'X("escaped \\\"$literal\\\"",$0)')
        self.assertNotEqual(priority.semantic_shape('Name("a  b",$x)'), priority.semantic_shape('Name("a b",$x)'))
        self.assertEqual(priority.semantic_shape('Name( "$x",  $y )'), 'Name( "$x", $0 )')

    def test_plan_read_only_deterministic_includes_source_complexity(self):
        first, second = priority.plan(self.root), priority.plan(self.root)
        self.assertEqual(first['rows'], second['rows'])
        self.assertEqual(first['pending_snapshot_sha256'], second['pending_snapshot_sha256'])
        self.assertEqual(len(first['rows']), 6)
        self.assertEqual(first['mechanical_jobs'], 1)
        row = next(row for row in first['rows'] if row['id'] == 'job5')
        self.assertTrue(row['evidence']['requires_diagnosis'])
        self.assertGreater(row['evidence']['semantic_rhs_bytes'], 0)
        self.assertEqual(row['evidence']['same_shape_sibling_rules'], 5)
        with closing(self.queue._connect()) as db:
            self.assertEqual(db.execute('SELECT count(*) FROM priority_snapshots').fetchone()[0], 0)

    def test_mechanical_first_and_every_fourth_author_is_fifo(self):
        published = self.queue.publish_priorities('base', self.reversed_plan())
        self.assertEqual((published['inserted'], published['skipped']), (6, 0))
        self.assertEqual(self.queue.claim('mechanical')['job']['id'], 'mechanical')
        self.assertEqual([self.queue.claim(f'w{i}', kinds=['author'])['job']['id'] for i in range(4)],
                         ['job5', 'job4', 'job3', 'job0'])

    def test_publish_skips_claimed_and_changed_payload_and_is_immutable(self):
        plan = self.reversed_plan()
        self.queue.claim('running', kinds=['author'])
        with self.queue._transaction() as db:
            row = db.execute("SELECT payload FROM jobs WHERE id='job1'").fetchone()[0]
            payload = json.loads(row)
            payload['goal'] = 'changed'
            db.execute("UPDATE jobs SET payload=? WHERE id='job1'", (json.dumps(payload),))
        published = self.queue.publish_priorities('base', plan)
        self.assertEqual((published['inserted'], published['skipped']), (4, 2))
        with self.assertRaisesRegex(ValueError, 'already exists'):
            self.queue.publish_priorities('base', plan)
        self.assertEqual(self.queue.coverage()['total_rules'], 7)

    def test_new_checkpoint_ignores_old_ranks(self):
        plan = self.reversed_plan()
        self.queue.publish_priorities('base', plan)
        self.queue.add_checkpoint('new', {'inputs': 'unused'}, expected_parent='base')
        self.assertEqual(self.queue.claim('worker', kinds=['author'])['job']['id'], 'job0')
        with self.assertRaisesRegex(ValueError, 'stale'):
            self.queue.publish_priorities('base', plan)

    def test_unranked_jobs_fall_back_to_fifo_and_are_not_starved(self):
        plan = self.reversed_plan()
        plan['rows'] = [row for row in plan['rows'] if row['id'] in ('job4', 'job5')]
        self.queue.publish_priorities('base', plan)
        self.assertEqual([self.queue.claim(f'w{i}', kinds=['author'])['job']['id'] for i in range(4)],
                         ['job5', 'job4', 'job0', 'job1'])


if __name__ == '__main__':
    unittest.main()
