import concurrent.futures
import fcntl
import hashlib
import subprocess
import tempfile
import unittest
from pathlib import Path

from retire_queue import Queue


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


if __name__ == "__main__":
    unittest.main()
