from contextlib import closing
import concurrent.futures
import fcntl
import hashlib
import json
from unittest.mock import patch
import subprocess
import tempfile
import unittest
from pathlib import Path

from retire_queue import Queue, _git_source_identity


class QueueTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / "queue.sqlite"
        self.queue = Queue(self.path)
        self.queue.add_checkpoint("base", {"source": "initial"})

    def add(self, count=1):
        self.queue.add_jobs([{"id": f"job{i}", "rules": [f"r{i}a", f"r{i}b"]} for i in range(count)])

    def passed(self, claim):
        return {rule: {"status": "pass", "evidence": "diagnosed"} for rule in claim["job"]["rules"]}

    def test_concurrent_claims_and_limit(self):
        self.add(30)
        with concurrent.futures.ThreadPoolExecutor(max_workers=30) as pool:
            claims = list(pool.map(lambda i: Queue(self.path).claim(f"worker{i}"), range(30)))
        claims = [claim for claim in claims if claim]
        self.assertEqual(len(claims), 12)
        self.assertEqual(len({c["job"]["id"] for c in claims}), 12)
        self.assertEqual(len({c["attempt_id"] for c in claims}), 12)
        self.assertEqual(self.queue.coverage()["total_rules"], 60)
        self.assertEqual(self.queue.coverage()["counts"], {"running": 24, "pending": 36})

    def test_unique_rules_and_transactional_add(self):
        self.add()
        with self.assertRaises(ValueError):
            self.queue.add_jobs([{"id": "new", "rules": ["unique"]}, {"id": "duplicate", "rules": ["r0a"]}])
        self.assertEqual(self.queue.inventory()["total_rules"], 2)

    def test_finish_coverage_and_immutable_adjudication(self):
        self.add()
        claim = self.queue.claim("worker")
        attempt = claim["attempt_id"]
        self.queue.mark_started(attempt, pid=123)
        self.queue.mark_started(attempt, pid=123)
        with self.assertRaises(ValueError):
            self.queue.finish(attempt, "provisional-pass", "evidence", {"r0a": {"status": "pass", "evidence": "x"}})
        outcomes = self.passed(claim)
        self.queue.finish(attempt, "provisional-pass", "evidence", outcomes)
        self.queue.finish(attempt, "provisional-pass", "evidence", outcomes)
        with self.assertRaises(ValueError):
            self.queue.finish(attempt, "failed", "changed", outcomes)
        self.assertEqual(self.queue.coverage()["accepted_rules"], 0)
        self.queue.accept_jobs(["job0"], "base", {"gates": "passed"})
        self.assertEqual(self.queue.coverage()["accepted_rules"], 2)
        with self.assertRaises(ValueError):
            self.queue.reject_jobs(["job0"], "changed")
        self.queue.finish(attempt, "provisional-pass", "evidence", outcomes)

    def test_checkpoint_snapshot_and_compare_and_swap(self):
        self.add(2)
        first = self.queue.claim("first", "base")
        with self.assertRaises(ValueError):
            self.queue.add_checkpoint("bad", "proof", expected_parent="wrong")
        self.queue.add_checkpoint("next", "proof", expected_parent="base")
        with self.assertRaises(ValueError):
            self.queue.claim("second", "base")
        second = self.queue.claim("second")
        self.assertEqual(first["base_commit"], "base")
        self.assertEqual(second["base_commit"], "next")
        self.assertEqual([e["data"]["base_commit"] for e in self.queue.audit() if e["kind"] == "claimed"], ["base", "next"])

    def test_advance_and_accept_is_atomic_and_preserves_outcomes(self):
        self.add(2)
        first = self.queue.claim("first")
        second = self.queue.claim("second")
        self.queue.finish(first["attempt_id"], "provisional-pass", "proof", self.passed(first))
        with self.assertRaisesRegex(ValueError, "provisional"):
            self.queue.advance_and_accept("next", "base", ["job0", "job1"], "proof")
        self.assertEqual(self.queue.coverage()["checkpoint"], "base")
        self.assertEqual(self.queue.coverage()["counts"], {"provisional-pass": 2, "running": 2})
        self.queue.finish(second["attempt_id"], "provisional-pass", "proof", self.passed(second))
        with self.assertRaisesRegex(ValueError, "parent conflict"):
            self.queue.advance_and_accept("next", "stale", ["job0", "job1"], "proof")
        with self.assertRaises(ValueError):
            self.queue.advance_and_accept("next", "base", ["job0", "job0"], "proof")
        with self.assertRaisesRegex(ValueError, "already exists"):
            self.queue.advance_and_accept("base", "base", ["job0", "job1"], "proof")
        self.assertEqual(self.queue.coverage()["checkpoint"], "base")
        self.queue.advance_and_accept("next", "base", ["job0", "job1"], {"audit": "passed"})
        coverage = self.queue.coverage()
        self.assertEqual(coverage["checkpoint"], "next")
        self.assertEqual(coverage["accepted_rules"], 4)
        self.assertEqual(coverage["rules"]["r0a"]["outcome"]["evidence"], "diagnosed")
        self.assertEqual(coverage["rules"]["r0a"]["disposition"], "accepted")
        self.assertEqual(coverage["jobs"][0]["adjudication"]["checkpoint"], "next")
        self.assertEqual([e["kind"] for e in self.queue.audit()][-2:], ["checkpoint", "adjudicated"])

    def test_no_stealing_and_integration_rejection(self):
        self.add(2)
        first = self.queue.claim("worker")
        with self.assertRaises(ValueError):
            self.queue.claim("worker")
        second = self.queue.claim("other")
        self.assertNotEqual(first["job"]["id"], second["job"]["id"])
        self.queue.finish(first["attempt_id"], "provisional-pass", "proof", self.passed(first))
        self.queue.reject_jobs([first["job"]["id"]], {"gate": "failed"})
        self.assertIsNone(self.queue.claim("another"))
        self.assertEqual(self.queue.coverage()["counts"], {"integration-rejected": 2, "running": 2})

    def test_split_pending_partition_and_coverage(self):
        self.queue.add_jobs([{"id": "old", "kind": "author", "rules": ["a", "b", "c"]}])
        groups = [{"id": "mechanical", "kind": "mechanical", "rules": ["a"]},
                  {"id": "author", "kind": "author", "rules": ["b", "c"]}]
        with self.assertRaises(ValueError):
            self.queue.split_pending("old", [{"id": "bad", "rules": ["a"]}], "proof")
        with self.assertRaises(ValueError):
            self.queue.split_pending("old", [{"id": "old", "rules": ["a", "b", "c"]}], "proof")
        self.assertEqual(len(self.queue.get_jobs(status="pending")), 1)
        self.queue.split_pending("old", groups, {"scan": "checkpoint"})
        coverage = self.queue.coverage()
        self.assertEqual(coverage["total_rules"], 3)
        self.assertEqual(coverage["counts"], {"pending": 3})
        self.assertEqual(set(coverage["rules"]), {"a", "b", "c"})
        self.assertEqual(coverage["rules"]["a"]["job_id"], "mechanical")
        self.assertEqual(len(self.queue.get_jobs(status="superseded")), 1)
        self.assertEqual(len(self.queue.get_jobs(status="pending", limit=1)), 1)
        mechanical = self.queue.claim("worker", kinds=["mechanical"])
        self.assertEqual(mechanical["job"]["id"], "mechanical")
        self.assertEqual(self.queue.get_attempt(mechanical["attempt_id"])["base_commit"], "base")
        with self.assertRaises(ValueError):
            self.queue.split_pending("mechanical", [{"id": "again", "rules": ["a"]}], "proof")
        self.assertIsNone(self.queue.claim("other", kinds=["mechanical"]))
        self.assertEqual(self.queue.claim("other", kinds=["author"])["job"]["id"], "author")
        self.assertEqual([event["kind"] for event in self.queue.audit()].count("split"), 1)

    def test_progress_matches_coverage_through_lifecycle(self):
        def equivalent():
            detailed, progress = self.queue.coverage(), self.queue.progress()
            for key in ("total_rules", "counts", "accepted_rules", "checkpoint"):
                self.assertEqual(progress[key], detailed[key])
            jobs = {}
            for job in detailed["jobs"]:
                jobs[job["status"]] = jobs.get(job["status"], 0) + 1
            self.assertEqual(progress["job_counts"], jobs)
        equivalent()
        self.add(2)
        equivalent()
        self.queue.split_pending("job0", [{"id": "split1", "rules": ["r0a"]},
                                          {"id": "split2", "rules": ["r0b"]}], "proof")
        equivalent()
        first = self.queue.claim("first")
        equivalent()
        self.queue.finish(first["attempt_id"], "provisional-pass", "proof", self.passed(first))
        equivalent()
        self.queue.accept_jobs([first["job"]["id"]], "base", "gates")
        equivalent()
        second = self.queue.claim("second")
        self.queue.finish(second["attempt_id"], "failed", "proof", self.passed(second))
        equivalent()
        lease = self.path.parent / "leases" / second["job"]["id"]
        lease.parent.mkdir()
        lease.touch()
        proof = {"launch_not_attempted": True, "controller_terminal": {
            "unit": "test", "invocation": "id", "verification": {"inactive": True}}}
        self.queue.retry_unstarted(second["job"]["id"], second["attempt_id"], proof)
        equivalent()
        self.queue.add_checkpoint("new", "proof", expected_parent="base")
        equivalent()

    def test_retry_requires_terminal_process_released_lease_and_preserves_history(self):
        self.add()
        claim = self.queue.claim("worker")
        attempt = claim["attempt_id"]
        process = subprocess.Popen(["sleep", "60"])
        self.addCleanup(lambda: process.poll() is None and process.kill())
        self.queue.mark_started(attempt, pid=process.pid)
        lease = self.path.parent / "leases" / "job0"
        lease.parent.mkdir()
        with self.assertRaises(ValueError):
            self.queue.retry_failed("job0", attempt, "proof", 1)
        with lease.open("w") as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.queue.finish(attempt, "failed", {"error": "operational"}, self.passed(claim))
            with self.assertRaisesRegex(ValueError, "lease is still held"):
                self.queue.retry_failed("job0", attempt, "proof", 1)
        with self.assertRaisesRegex(ValueError, "still present"):
            self.queue.retry_failed("job0", attempt, "proof", 1)
        process.terminate()
        process.wait()
        with self.assertRaisesRegex(ValueError, "limit"):
            self.queue.retry_failed("job0", attempt, "proof", 0)
        with self.assertRaises(ValueError):
            self.queue.retry_failed("job0", "wrong", "proof", 1)
        old = self.queue.get_attempt(attempt)
        self.queue.retry_failed("job0", attempt, {"reason": "terminal failure"}, 1)
        self.assertEqual(self.queue.get_attempt(attempt), old)
        self.assertEqual(self.queue.coverage()["counts"], {"pending": 2})
        self.assertIsNone(self.queue.coverage()["jobs"][0]["outcomes"])
        second = self.queue.claim("worker")
        self.assertNotEqual(second["attempt_id"], attempt)
        self.queue.mark_started(second["attempt_id"], pid=process.pid)
        self.queue.finish(second["attempt_id"], "failed", "proof", self.passed(second))
        with self.assertRaisesRegex(ValueError, "limit"):
            self.queue.retry_failed("job0", second["attempt_id"], "proof", 1)
        retry = [event for event in self.queue.audit() if event["kind"] == "retry"]
        self.assertEqual(len(retry), 1)
        self.assertEqual(retry[0]["data"]["attempt_id"], attempt)

    def test_retry_refuses_missing_or_unknown_process_and_nonfailed_status(self):
        self.add()
        claim = self.queue.claim("worker")
        attempt = claim["attempt_id"]
        self.queue.mark_started(attempt, receipt="no pid")
        self.queue.finish(attempt, "failed", "proof", self.passed(claim))
        with self.assertRaisesRegex(ValueError, "lease is unavailable"):
            self.queue.retry_failed("job0", attempt, "proof", 1)
        lease = self.path.parent / "leases" / "job0"
        lease.parent.mkdir()
        lease.touch()
        with self.assertRaisesRegex(ValueError, "identity is unknown"):
            self.queue.retry_failed("job0", attempt, "proof", 1)
        with self.assertRaises(ValueError):
            self.queue.retry_failed("../job0", attempt, "proof", 1)
        with self.assertRaises(ValueError):
            self.queue.retry_failed("job0", attempt, {}, 1)
        self.assertEqual(self.queue.coverage()["counts"], {"failed": 2})

    def test_retry_refuses_running_and_review(self):
        self.add()
        claim = self.queue.claim("worker")
        attempt = claim["attempt_id"]
        process = subprocess.run(["true"], check=True)
        self.assertEqual(process.returncode, 0)
        # Reaped child supplies an actual absent process identity.
        child = subprocess.Popen(["true"])
        child.wait()
        self.queue.mark_started(attempt, pid=child.pid)
        lease = self.path.parent / "leases" / "job0"
        lease.parent.mkdir()
        lease.touch()
        with self.assertRaisesRegex(ValueError, "current finished failed"):
            self.queue.retry_failed("job0", attempt, "proof", 1)
        self.queue.finish(attempt, "needs-review", "proof", self.passed(claim))
        with self.assertRaisesRegex(ValueError, "current finished failed"):
            self.queue.retry_failed("job0", attempt, "proof", 1)

    def test_tainted_retry_requires_terminal_pid_and_matching_infrastructure_artifact(self):
        self.add()
        claim = self.queue.claim("worker")
        attempt = claim["attempt_id"]
        lease = self.path.parent / "leases" / "job0"
        lease.parent.mkdir()
        lease.touch()
        artifact = self.path.parent / "error.log"
        artifact.write_text("OSError: [Errno 30] Read-only file system: cpu/allocator.lock\n")
        proof = {"controller_terminal": {"unit": "controller.service", "invocation": "abc",
                                          "verification": {"inactive": True}},
                 "infrastructure_failure": {"kind": "sandbox-cpu-pool-read-only", "artifact": str(artifact),
                                            "sha256": hashlib.sha256(artifact.read_bytes()).hexdigest()}}
        child = subprocess.Popen(["true"])
        child.wait()
        self.queue.mark_started(attempt, pid=child.pid)
        with self.assertRaisesRegex(ValueError, "terminal review"):
            self.queue.retry_tainted("job0", attempt, proof)
        self.queue.finish(attempt, "needs-review", "diagnostic interrupted", self.passed(claim))
        old = self.queue.get_attempt(attempt)
        with lease.open("w") as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, "still held"):
                self.queue.retry_tainted("job0", attempt, proof)
        with self.assertRaises(ValueError):
            self.queue.retry_tainted("job0", attempt, {**proof, "infrastructure_failure": {**proof["infrastructure_failure"], "sha256": "0" * 64}})
        self.queue.retry_tainted("job0", attempt, proof)
        self.assertEqual(self.queue.get_attempt(attempt), old)
        self.assertEqual(self.queue.coverage()["counts"], {"pending": 2})
        self.assertEqual(self.queue.audit()[-1]["kind"], "retry-tainted")
        self.assertEqual(self.queue.audit()[-1]["data"]["evidence"], proof)

    def test_tainted_retry_rejects_unknown_and_live_pid(self):
        self.add(2)
        lease_dir = self.path.parent / "leases"
        lease_dir.mkdir()
        (lease_dir / "job0").touch()
        (lease_dir / "job1").touch()
        artifact = self.path.parent / "error.log"
        artifact.write_text("[Errno 30] cpu/allocator.lock")
        proof = {"controller_terminal": {"unit": "u", "invocation": "i", "verification": {"inactive": True}},
                 "infrastructure_failure": {"kind": "sandbox-cpu-pool-read-only", "artifact": str(artifact),
                                            "sha256": hashlib.sha256(artifact.read_bytes()).hexdigest()}}
        first = self.queue.claim("one")
        self.queue.mark_started(first["attempt_id"], receipt="unknown pid")
        self.queue.finish(first["attempt_id"], "failed", "proof", self.passed(first))
        with self.assertRaisesRegex(ValueError, "identity is unknown"):
            self.queue.retry_tainted("job0", first["attempt_id"], proof)
        second = self.queue.claim("two")
        child = subprocess.Popen(["sleep", "60"])
        self.addCleanup(lambda: (child.kill(), child.wait()) if child.poll() is None else None)
        self.queue.mark_started(second["attempt_id"], pid=child.pid)
        self.queue.finish(second["attempt_id"], "failed", "proof", self.passed(second))
        with self.assertRaisesRegex(ValueError, "still present"):
            self.queue.retry_tainted("job1", second["attempt_id"], proof)

    def test_administrative_unstarted_retry_requires_explicit_evidence_and_lease(self):
        self.add()
        claim = self.queue.claim("worker")
        attempt = claim["attempt_id"]
        proof = {"launch_not_attempted": True, "controller_terminal": {
            "unit": "controller.service", "invocation": "abc", "verification": {"inactive": True}}}
        lease = self.path.parent / "leases" / "job0"
        lease.parent.mkdir()
        lease.touch()
        with self.assertRaisesRegex(ValueError, "current finished failed"):
            self.queue.retry_unstarted("job0", attempt, proof)
        self.queue.finish(attempt, "failed", "preparation failed", self.passed(claim))
        for invalid in ({}, {"launch_not_attempted": True},
                        {"launch_not_attempted": False, "controller_terminal": proof["controller_terminal"]},
                        {"launch_not_attempted": True, "controller_terminal": {"unit": "x", "invocation": "y"}}):
            with self.assertRaises(ValueError):
                self.queue.retry_unstarted("job0", attempt, invalid)
        with lease.open("w") as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, "still held"):
                self.queue.retry_unstarted("job0", attempt, proof)
        with self.assertRaises(ValueError):
            self.queue.retry_unstarted("job0", "wrong", proof)
        old = self.queue.get_attempt(attempt)
        self.queue.retry_unstarted("job0", attempt, proof)
        self.assertEqual(self.queue.get_attempt(attempt), old)
        self.assertEqual(self.queue.coverage()["counts"], {"pending": 2})
        self.assertEqual(self.queue.audit()[-1]["data"]["evidence"], proof)
        second = self.queue.claim("worker")
        self.queue.mark_started(second["attempt_id"], receipt="pid unknown")
        self.queue.finish(second["attempt_id"], "failed", "proof", self.passed(second))
        with self.assertRaisesRegex(ValueError, "recorded start"):
            self.queue.retry_unstarted("job0", second["attempt_id"], proof)

    def rejected_with_family_evidence(self):
        self.add()
        claim = self.queue.claim('worker')
        attempt = claim['attempt_id']
        child = subprocess.Popen(['true'])
        child.wait()
        self.queue.mark_started(attempt, pid=child.pid)
        lease = self.path.parent/'leases/job0'
        lease.parent.mkdir()
        lease.touch()
        run = self.path.parent/'.cache/attempts'/attempt
        report_dir = run/'evaluations/job0/attempt-1'
        report_dir.mkdir(parents=True)
        (run/'tasks').mkdir()
        descriptor = run/'tasks/job0.json'
        descriptor.write_text('{}')
        support = run/'support.json'
        support.write_text(json.dumps({'tasks': {'job0': hashlib.sha256(descriptor.read_bytes()).hexdigest()}}))
        grammar = report_dir/'grammar'
        grammar.mkdir()
        (grammar/'example.ebnf').write_text('example ::= \"x\"')
        family = report_dir/'family.json'
        family.write_text(json.dumps({'family': 'assigned', 'cases': [{}, {}, {}, {}]}))
        digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
        report = report_dir/'report.json'
        report.write_text(json.dumps({'passed': True, 'failures': [], 'task': 'job0',
                                      'source': 'base', 'directory': str(report_dir),
                                      'descriptor_sha256': digest(descriptor), 'support_sha256': digest(support),
                                      'fixture_sha256': digest(family), 'grammar': {'example.ebnf': digest(grammar/'example.ebnf')},
                                      'edited': [{'face': 'Target'}]}))
        dispositions = {name: {'rule': name, 'status': 'proposed'} for name in claim['job']['rules']}
        original = {'report': str(report), 'task': 'job0', 'base_commit': 'base',
                    'kind': 'author', 'dispositions': dispositions}
        self.queue.finish(attempt, 'provisional-pass', original, self.passed(claim))
        self.queue.reject_jobs(['job0'], {'manifest': 'batch1', 'reason': 'family gate'})
        manifest_dir = self.path.parent/'.cache/batches/batch1'
        logs = manifest_dir/'proposal-0'
        logs.mkdir(parents=True)
        manifest = manifest_dir/'manifest.json'
        manifest.write_text(json.dumps({'id': 'batch1', 'jobs': [{'job_id': 'job0', 'attempt_id': attempt,
            'status': 'integration-rejected', 'reason': 'family check failed: test', 'family_logs': str(logs)}]}))
        gate = logs/'family-0.log'
        gate.write_text(json.dumps({'family': 'assigned', 'cases': 5, 'compositional': True,
                                    'failures': [{'name': 'Target', 'why': 'no parse'}]}) + '\n')
        proof = {'kind': 'family-case-mode-revalidation',
                 'previous_manifest': {'path': str(manifest), 'sha256': digest(manifest)},
                 'gate_proof': {'path': str(gate), 'sha256': digest(gate)},
                 'report': {'path': str(report), 'sha256': digest(report)}}
        return claim, proof, lease, report

    def test_readmit_preserves_rejection_and_reuses_exact_passed_attempt(self):
        claim, proof, lease, report = self.rejected_with_family_evidence()
        attempt = claim['attempt_id']
        old_attempt = self.queue.get_attempt(attempt)
        with self.assertRaisesRegex(ValueError, 'case modes'):
            self.queue.readmit_proposal('job0', attempt, 'base', proof)
        self.queue.add_checkpoint('migrated', {'family_case_modes': True}, expected_parent='base')
        with self.assertRaisesRegex(ValueError, 'stale'):
            self.queue.readmit_proposal('job0', attempt, 'base', proof)
        with lease.open('w') as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, 'still held'):
                self.queue.readmit_proposal('job0', attempt, 'migrated', proof)
        with self.assertRaises(ValueError):
            self.queue.readmit_proposal('job0', attempt, 'migrated',
                {**proof, 'gate_proof': {**proof['gate_proof'], 'sha256': '0'*64}})
        self.queue.readmit_proposal('job0', attempt, 'migrated', proof)
        self.assertEqual(self.queue.get_attempt(attempt), old_attempt)
        job = self.queue.get_jobs('provisional-pass')[0]
        self.assertEqual(job['attempt_id'], attempt)
        self.assertIsNone(job['adjudication'])
        self.assertEqual([e['kind'] for e in self.queue.audit()][-1], 'readmitted')
        event = self.queue.audit()[-1]['data']
        self.assertEqual(event['previous_adjudication']['status'], 'integration-rejected')
        self.assertEqual(event['evidence'], proof)
        self.assertTrue(any(e['kind'] == 'adjudicated' for e in self.queue.audit()))
        with self.assertRaises(ValueError):
            self.queue.readmit_proposal('job0', attempt, 'migrated', proof)

    def test_readmit_rejects_bad_report_dispositions_or_live_pid(self):
        claim, proof, lease, report = self.rejected_with_family_evidence()
        attempt = claim['attempt_id']
        self.queue.add_checkpoint('migrated', {'family_case_modes': True}, expected_parent='base')
        with patch('retire_queue.os.kill', return_value=None):
            with self.assertRaisesRegex(ValueError, 'still present'):
                self.queue.readmit_proposal('job0', attempt, 'migrated', proof)
        changed = json.loads(report.read_text())
        changed['passed'] = False
        report.write_text(json.dumps(changed))
        proof['report']['sha256'] = hashlib.sha256(report.read_bytes()).hexdigest()
        with self.assertRaisesRegex(ValueError, 'did not pass'):
            self.queue.readmit_proposal('job0', attempt, 'migrated', proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 2})

    def test_readmit_refuses_nonproposed_or_incomplete_original_dispositions(self):
        claim, proof, lease, report = self.rejected_with_family_evidence()
        attempt = claim['attempt_id']
        self.queue.add_checkpoint('migrated', {'family_case_modes': True}, expected_parent='base')
        original = self.queue.get_attempt(attempt)['result']
        for mutation in ('missing', 'review', 'outcome'):
            changed = json.loads(json.dumps(original))
            if mutation == 'missing':
                del changed['evidence']['dispositions']['r0a']
            elif mutation == 'review':
                changed['evidence']['dispositions']['r0a']['status'] = 'needs-review'
            else:
                changed['outcomes']['r0a']['status'] = 'needs-review'
            with closing(self.queue._connect()) as db:
                db.execute('UPDATE attempts SET result=? WHERE id=?', (json.dumps(changed), attempt))
            with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, 'dispositions'):
                self.queue.readmit_proposal('job0', attempt, 'migrated', proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 2})

    def test_review_requires_reason_and_pass_cannot_hide_review(self):
        self.add()
        claim = self.queue.claim("worker")
        outcomes = self.passed(claim)
        outcomes["r0a"] = {"status": "needs-review", "evidence": "proof"}
        with self.assertRaises(ValueError):
            self.queue.finish(claim["attempt_id"], "needs-review", "proof", outcomes)
        outcomes["r0a"]["reason"] = "semantic contract"
        with self.assertRaises(ValueError):
            self.queue.finish(claim["attempt_id"], "provisional-pass", "proof", outcomes)
        self.queue.finish(claim["attempt_id"], "needs-review", "proof", outcomes)
        self.assertEqual(self.queue.coverage()["counts"], {"needs-review": 2})


class DiagnosticRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root/'repo'
        self.repo.mkdir()
        self.git('init', '-q')
        self.git('config', 'user.name', 'Test')
        self.git('config', 'user.email', 'test@invalid')
        for folder in ('grammar', 'src/bin', 'src/tests', 'tools', 'tests'):
            (self.repo/folder).mkdir(parents=True)
        (self.repo/'grammar/syntax.ebnf').write_text('base')
        (self.repo/'tests/families.json').write_text(json.dumps([f'families/{i}.json' for i in range(67)]))
        (self.repo/'src/lib.rs').write_text('native')
        (self.repo/'tools/dev.py').write_text('dev')
        (self.repo/'tools/pin.py').write_text('pin')
        (self.repo/'src/bin/family-check.rs').write_text('old cli')
        (self.repo/'src/tests/family_check_modes.rs').write_text('old test')
        self.git('add', '.')
        self.git('commit', '-qm', 'base')
        self.parent = self.git('rev-parse', 'HEAD')
        (self.repo/'grammar/syntax.ebnf').write_text('audited')
        self.git('add', '.')
        self.git('commit', '-qm', 'audited')
        self.audited = self.git('rev-parse', 'HEAD')
        (self.repo/'src/bin/family-check.rs').write_text('fixed cli')
        (self.repo/'src/tests/family_check_modes.rs').write_text('fixed test')
        self.git('add', '.')
        self.git('commit', '-qm', 'repaired')
        self.commit = self.git('rev-parse', 'HEAD')
        (self.root/'config.json').write_text(json.dumps({'source_repo': str(self.repo)}))
        self.queue = Queue(self.root/'queue.sqlite')
        self.queue.add_checkpoint(self.parent, {'proof': 'base'})
        self.queue.add_jobs([{'id': 'job0', 'rules': ['a']}, {'id': 'job1', 'rules': ['b']}])
        self.jobs = ['job0', 'job1']
        claims = [self.queue.claim('worker0'), self.queue.claim('worker1')]
        (self.root/'leases').mkdir()
        self.items = []
        for claim in claims:
            job = claim['job']['id']
            attempt = claim['attempt_id']
            (self.root/'leases'/job).touch()
            child = subprocess.Popen(['true'])
            child.wait()
            self.queue.mark_started(attempt, pid=child.pid)
            directory = self.root/'.cache/attempts'/attempt/'evaluations'/job/'attempt-a'
            directory.mkdir(parents=True)
            run = self.root/'.cache/attempts'/attempt
            (run/'tasks').mkdir()
            descriptor = run/'tasks'/f'{job}.json'
            descriptor.write_text('{}')
            digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
            support = run/'support.json'
            support.write_text(json.dumps({'tasks': {job: digest(descriptor)}}))
            family = directory/'family.json'
            family.write_text(json.dumps({'family': 'test', 'cases': []}))
            grammar = directory/'grammar'
            grammar.mkdir()
            (grammar/'example.ebnf').write_text('grammar')
            report = directory/'report.json'
            report.write_text(json.dumps({'passed': True, 'failures': [], 'task': job,
                 'source': self.parent, 'directory': str(directory), 'fixture_sha256': digest(family),
                 'descriptor_sha256': digest(descriptor), 'support_sha256': digest(support),
                 'grammar': {'example.ebnf': digest(grammar/'example.ebnf')}}))
            outcomes = {name: {'status': 'pass', 'evidence': 'native'} for name in claim['job']['rules']}
            decisions = {name: {'rule': name, 'status': 'proposed'} for name in claim['job']['rules']}
            original = {'task': job, 'base_commit': self.parent, 'report': str(report),
                        'kind': 'author', 'dispositions': decisions}
            self.queue.finish(attempt, 'provisional-pass', original, outcomes)
            self.items.append({'job_id': job, 'attempt_id': attempt, 'status': 'included',
                               'base_commit': self.parent, 'rules': claim['job']['rules'],
                               'descriptor_sha256': digest(descriptor), 'support_sha256': digest(support)})
        folder = self.root/'.cache/batches/batch1'
        folder.mkdir(parents=True)
        self.manifest = folder/'manifest.json'
        self.manifest.write_text(json.dumps({'id': 'batch1', 'base_commit': self.parent,
                                             'commit': self.audited, 'jobs': self.items}))
        validation = {'status': 'failed', 'reason': 'repository gates failed', 'candidate': self.audited,
                      'source_checkpoint': self.parent, 'included_jobs': self.jobs,
                      'full_checks': {'faces': 42, 'failures': []}, 'repository_check_returncode': 1}
        self.validation = folder/'validation.json'
        self.validation.write_text(json.dumps(validation))
        self.queue.reject_jobs(self.jobs, {'manifest': 'batch1', 'validation': validation})
        self.old_log = folder/'repository-check.log'
        self.old_log.write_text(self.gates('FAILED') + 'Error: family-check at most 64\\n')
        self.new_log = self.root/'new-gates.log'
        self.new_log.write_text(self.gates('passed'))
        self.identity = self.root/'identity.json'
        source_identity = _git_source_identity(self.repo, self.audited, self.commit)
        self.identity.write_text(json.dumps({**source_identity, 'parent_checkpoint': self.parent}))
        self.proof = {'kind': 'diagnostic-only-family-limit', 'manifest': self.spec(self.manifest),
            'validation': self.spec(self.validation), 'previous_repo_log': self.spec(self.old_log),
            'new_repo_log': self.spec(self.new_log), 'identity_proof': self.spec(self.identity)}

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.repo, text=True).strip()

    @staticmethod
    def spec(path):
        return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}

    @staticmethod
    def gates(families):
        return ''.join(f'{gate} {families if gate == "families" else "passed"} 1.0 s\n'
                       for gate in ('fmt', 'pin', 'hygiene', 'tools', 'clippy', 'build', 'families', 'test'))

    def test_recovery_atomically_advances_and_retains_rejection_history(self):
        self.queue.advance_recovered_diagnostics(self.commit, self.parent, self.jobs, self.proof)
        coverage = self.queue.coverage()
        self.assertEqual(coverage['checkpoint'], self.commit)
        self.assertEqual(coverage['counts'], {'accepted': 2})
        self.assertTrue(all(job['adjudication']['kind'] == 'diagnostic-only' for job in coverage['jobs']))
        recovery = self.queue.audit()[-1]
        self.assertEqual(recovery['kind'], 'diagnostic-recovery')
        self.assertEqual(set(recovery['data']['previous_adjudications']), set(self.jobs))
        self.assertTrue(any(event['kind'] == 'adjudicated' for event in self.queue.audit()))

    def test_recovery_rejects_stale_partial_bad_gate_and_held_lease(self):
        with self.assertRaisesRegex(ValueError, 'parent conflict'):
            self.queue.advance_recovered_diagnostics(self.commit, 'stale', self.jobs, self.proof)
        with self.assertRaisesRegex(ValueError, 'specified native-pass'):
            self.queue.advance_recovered_diagnostics(self.commit, self.parent, self.jobs[:1], self.proof)
        with (self.root/'leases/job0').open('w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, 'still held'):
                self.queue.advance_recovered_diagnostics(self.commit, self.parent, self.jobs, self.proof)
        self.new_log.write_text(self.gates('FAILED'))
        self.proof['new_repo_log'] = self.spec(self.new_log)
        with self.assertRaisesRegex(ValueError, 'repository gates'):
            self.queue.advance_recovered_diagnostics(self.commit, self.parent, self.jobs, self.proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 2})
        self.assertEqual(self.queue.coverage()['checkpoint'], self.parent)

    def test_recovery_refuses_native_or_grammar_changes(self):
        (self.repo/'src/lib.rs').write_text('changed native')
        self.git('add', '.')
        self.git('commit', '-qm', 'bad native')
        bad = self.git('rev-parse', 'HEAD')
        identity = json.loads(self.identity.read_text())
        identity['new_commit'] = bad
        self.identity.write_text(json.dumps(identity))
        self.proof['identity_proof'] = self.spec(self.identity)
        with self.assertRaisesRegex(ValueError, 'native implementation changed'):
            self.queue.advance_recovered_diagnostics(bad, self.parent, self.jobs, self.proof)
        self.assertEqual(self.queue.coverage()['checkpoint'], self.parent)

    def test_recovery_rejects_failed_rule_or_changed_original_evidence(self):
        with closing(self.queue._connect()) as db:
            attempt, result = db.execute('SELECT id,result FROM attempts WHERE job_id=?', ('job0',)).fetchone()
            value = json.loads(result)
            value['evidence']['dispositions']['a']['status'] = 'needs-review'
            db.execute('UPDATE attempts SET result=? WHERE id=?', (json.dumps(value), attempt))
        with self.assertRaisesRegex(ValueError, 'per-rule pass'):
            self.queue.advance_recovered_diagnostics(self.commit, self.parent, self.jobs, self.proof)
        self.assertEqual(self.queue.coverage()['checkpoint'], self.parent)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 2})

    def test_recovery_refuses_other_original_failures_and_grammar_drift(self):
        self.old_log.write_text(self.gates('FAILED').replace('tools passed', 'tools FAILED')
                                + 'Error: family-check at most 64\\n')
        self.proof['previous_repo_log'] = self.spec(self.old_log)
        with self.assertRaisesRegex(ValueError, 'repository gates'):
            self.queue.advance_recovered_diagnostics(self.commit, self.parent, self.jobs, self.proof)
        self.old_log.write_text(self.gates('FAILED') + 'Error: family-check at most 64\\n')
        self.proof['previous_repo_log'] = self.spec(self.old_log)
        (self.repo/'grammar/syntax.ebnf').write_text('different grammar')
        self.git('add', '.')
        self.git('commit', '-qm', 'bad grammar')
        bad = self.git('rev-parse', 'HEAD')
        identity = json.loads(self.identity.read_text())
        identity['new_commit'] = bad
        self.identity.write_text(json.dumps(identity))
        self.proof['identity_proof'] = self.spec(self.identity)
        with self.assertRaisesRegex(ValueError, 'audited grammar changed'):
            self.queue.advance_recovered_diagnostics(bad, self.parent, self.jobs, self.proof)

    def test_recovery_refuses_compiled_tool_input_changes(self):
        for name in ('tools/dev.py', 'tools/pin.py'):
            with self.subTest(path=name):
                self.git('reset', '--hard', self.commit)
                (self.repo/name).write_text('changed')
                self.git('add', '.')
                self.git('commit', '-qm', 'changed compiled tool')
                bad = self.git('rev-parse', 'HEAD')
                identity = json.loads(self.identity.read_text())
                identity['new_commit'] = bad
                self.identity.write_text(json.dumps(identity))
                self.proof['identity_proof'] = self.spec(self.identity)
                with self.assertRaisesRegex(ValueError, 'native implementation changed'):
                    self.queue.advance_recovered_diagnostics(bad, self.parent, self.jobs, self.proof)
        self.assertEqual(self.queue.coverage()['checkpoint'], self.parent)

    def test_recovery_refuses_self_consistent_false_claimed_identity_hashes(self):
        original = json.loads(self.identity.read_text())
        for scope in ('native', 'grammar'):
            identity = dict(original)
            identity[scope+'_before_sha256'] = identity[scope+'_after_sha256'] = 'c'*64
            self.identity.write_text(json.dumps(identity))
            self.proof['identity_proof'] = self.spec(self.identity)
            with self.subTest(scope=scope), self.assertRaisesRegex(ValueError, 'verified Git source'):
                self.queue.advance_recovered_diagnostics(self.commit, self.parent, self.jobs, self.proof)
        self.assertEqual(self.queue.coverage()['checkpoint'], self.parent)

    def family_limit_proof(self, item=None):
        item = item or self.items[0]
        report = self.root/'.cache/attempts'/item['attempt_id']/'evaluations'/item['job_id']/'attempt-a/report.json'
        return {'kind': 'family-limit-revalidation', 'previous_manifest': self.spec(self.manifest),
                'validation': self.spec(self.validation), 'previous_repo_log': self.spec(self.old_log),
                'report': self.spec(report)}

    def test_family_limit_readmission_restages_without_reusing_audit_as_acceptance(self):
        item, proof = self.items[0], self.family_limit_proof()
        attempt = item['attempt_id']
        original = self.queue.get_attempt(attempt)
        self.queue.add_checkpoint('new', {'family_case_modes': True, 'family_registry_limit': 67},
                                  expected_parent=self.parent)
        self.queue.readmit_family_limit_proposal('job0', attempt, 'new', proof)
        self.assertEqual(self.queue.get_attempt(attempt), original)
        job = self.queue.get_jobs('provisional-pass')[0]
        self.assertEqual(job['id'], 'job0')
        self.assertEqual(job['attempt_id'], attempt)
        self.assertIsNone(job['adjudication'])
        self.assertEqual(self.queue.coverage()['checkpoint'], 'new')
        self.assertEqual(self.queue.coverage()['accepted_rules'], 0)
        event = self.queue.audit()[-1]
        self.assertEqual(event['kind'], 'readmitted-family-limit')
        self.assertEqual(event['data']['old_registry_count'], 67)
        self.assertEqual(event['data']['previous_adjudication']['status'], 'integration-rejected')
        self.assertTrue(any(e['kind'] == 'adjudicated' for e in self.queue.audit()))

    def test_family_limit_readmission_rejects_stale_missing_capabilities_and_lease(self):
        item, proof = self.items[0], self.family_limit_proof()
        attempt = item['attempt_id']
        with self.assertRaisesRegex(ValueError, 'stale'):
            self.queue.readmit_family_limit_proposal('job0', attempt, 'stale', proof)
        self.queue.add_checkpoint('old-cli', {'family_case_modes': True, 'family_registry_limit': 66},
                                  expected_parent=self.parent)
        with self.assertRaisesRegex(ValueError, 'registry limit'):
            self.queue.readmit_family_limit_proposal('job0', attempt, 'old-cli', proof)
        self.queue.add_checkpoint('new', {'family_case_modes': True, 'family_registry_limit': 67},
                                  expected_parent='old-cli')
        with (self.root/'leases/job0').open('w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, 'still held'):
                self.queue.readmit_family_limit_proposal('job0', attempt, 'new', proof)
        with patch('retire_queue.os.kill', return_value=None):
            with self.assertRaisesRegex(ValueError, 'still present'):
                self.queue.readmit_family_limit_proposal('job0', attempt, 'new', proof)
        wrong = dict(proof, report={**proof['report'], 'sha256': '0'*64})
        with self.assertRaisesRegex(ValueError, 'hash differs'):
            self.queue.readmit_family_limit_proposal('job0', attempt, 'new', wrong)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 2})

    def test_family_limit_readmission_refuses_nonproposed_and_false_case_mode(self):
        item, proof = self.items[0], self.family_limit_proof()
        attempt = item['attempt_id']
        self.queue.add_checkpoint('no-mode', {'family_case_modes': False, 'family_registry_limit': 4096},
                                  expected_parent=self.parent)
        with self.assertRaisesRegex(ValueError, 'registry limit'):
            self.queue.readmit_family_limit_proposal('job0', attempt, 'no-mode', proof)
        self.queue.add_checkpoint('enabled', {'family_case_modes': True, 'family_registry_limit': 4096},
                                  expected_parent='no-mode')
        with self.queue._connect() as db:
            result = json.loads(db.execute('SELECT result FROM attempts WHERE id=?', (attempt,)).fetchone()[0])
            result['evidence']['dispositions']['a']['status'] = 'needs-review'
            db.execute('UPDATE attempts SET result=? WHERE id=?', (json.dumps(result), attempt))
        with self.assertRaisesRegex(ValueError, 'dispositions'):
            self.queue.readmit_family_limit_proposal('job0', attempt, 'enabled', proof)
        self.assertEqual(self.queue.coverage()['accepted_rules'], 0)

    def test_family_limit_readmission_refuses_nonincluded_or_other_gate_failures(self):
        item, proof = self.items[0], self.family_limit_proof()
        attempt = item['attempt_id']
        self.queue.add_checkpoint('new', {'family_case_modes': True, 'family_registry_limit': 4096},
                                  expected_parent=self.parent)
        manifest = json.loads(self.manifest.read_text())
        manifest['jobs'][0]['status'] = 'integration-rejected'
        self.manifest.write_text(json.dumps(manifest))
        proof['previous_manifest'] = self.spec(self.manifest)
        with self.assertRaisesRegex(ValueError, 'included native-pass'):
            self.queue.readmit_family_limit_proposal('job0', attempt, 'new', proof)
        manifest['jobs'][0]['status'] = 'included'
        self.manifest.write_text(json.dumps(manifest))
        proof['previous_manifest'] = self.spec(self.manifest)
        self.old_log.write_text(self.gates('FAILED').replace('tools passed', 'tools FAILED')
                                + 'Error: family-check at most 64\n')
        proof['previous_repo_log'] = self.spec(self.old_log)
        with self.assertRaisesRegex(ValueError, 'not limited'):
            self.queue.readmit_family_limit_proposal('job0', attempt, 'new', proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 2})


if __name__ == "__main__":
    unittest.main()
