"""Durable, explicitly adjudicated work queue for rule retirement campaigns.

This module never launches workers or decides whether their evidence is sound.
SQLite transactions serialize claims, checkpoint advancement, and adjudication.
"""
import errno
import fcntl
import hashlib
import json
import os
from pathlib import Path
import stat
import sqlite3
import uuid
from contextlib import closing, contextmanager


def _json(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)


def _evidence(value):
    if not value:
        raise ValueError("evidence is required")
    return _json(value)


class Queue:
    def __init__(self, path, config=None):
        self.path = str(path)
        config = dict(config or {})
        maximum = config.get("max_workers", 12)
        if not isinstance(maximum, int) or isinstance(maximum, bool) or not 1 <= maximum <= 12:
            raise ValueError("max_workers must be between 1 and 12")
        with closing(self._connect()) as db:
            db.execute("PRAGMA journal_mode=WAL")
            db.executescript("""
                CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS jobs (
                    id TEXT PRIMARY KEY, payload TEXT NOT NULL, status TEXT NOT NULL,
                    attempt_id TEXT, adjudication TEXT
                );
                CREATE TABLE IF NOT EXISTS rules (
                    rule TEXT PRIMARY KEY, job_id TEXT NOT NULL REFERENCES jobs(id)
                );
                CREATE TABLE IF NOT EXISTS checkpoints (
                    seq INTEGER PRIMARY KEY AUTOINCREMENT, commit_id TEXT UNIQUE NOT NULL,
                    parent TEXT, evidence TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS attempts (
                    id TEXT PRIMARY KEY, job_id TEXT NOT NULL REFERENCES jobs(id),
                    worker TEXT NOT NULL, base_commit TEXT NOT NULL,
                    started TEXT, result TEXT
                );
                CREATE TABLE IF NOT EXISTS events (
                    seq INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL,
                    data TEXT NOT NULL, at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
            """)
            existing = db.execute("SELECT value FROM metadata WHERE key='config'").fetchone()
            if existing and config and existing[0] != _json(config):
                raise ValueError("queue configuration differs from existing configuration")
            if not existing:
                db.execute("INSERT INTO metadata VALUES ('config', ?)", (_json(config),))

    def _connect(self):
        db = sqlite3.connect(self.path, timeout=30, isolation_level=None)
        db.execute("PRAGMA foreign_keys=ON")
        db.row_factory = sqlite3.Row
        return db

    @contextmanager
    def _transaction(self):
        db = self._connect()
        try:
            db.execute("BEGIN IMMEDIATE")
            yield db
            db.commit()
        except BaseException:
            db.rollback()
            raise
        finally:
            db.close()

    @staticmethod
    def _event(db, kind, data):
        db.execute("INSERT INTO events(kind,data) VALUES (?,?)", (kind, _json(data)))

    def add_jobs(self, jobs):
        with self._transaction() as db:
            for job in jobs:
                job = dict(job)
                job_id, rules = job.get("id"), job.get("rules")
                if not isinstance(job_id, str) or not job_id or not isinstance(rules, list) or not rules:
                    raise ValueError("each job needs a nonempty id and rules list")
                if any(not isinstance(rule, str) or not rule for rule in rules) or len(set(rules)) != len(rules):
                    raise ValueError("rules must be distinct nonempty strings")
                try:
                    db.execute("INSERT INTO jobs(id,payload,status) VALUES (?,?,'pending')", (job_id, _json(job)))
                    db.executemany("INSERT INTO rules(rule,job_id) VALUES (?,?)", [(rule, job_id) for rule in rules])
                except sqlite3.IntegrityError as exc:
                    raise ValueError("duplicate job or rule ownership") from exc
                self._event(db, "job-added", job)

    def split_pending(self, jobid, groups, evidence):
        _evidence(evidence)
        groups = [dict(group) for group in groups]
        if not groups:
            raise ValueError("split requires groups")
        with self._transaction() as db:
            row = db.execute("SELECT status FROM jobs WHERE id=?", (jobid,)).fetchone()
            if not row or row[0] != "pending":
                raise ValueError("only pending jobs can be split")
            original = {r[0] for r in db.execute("SELECT rule FROM rules WHERE job_id=?", (jobid,))}
            assigned = []
            ids = []
            for group in groups:
                group_id, rules = group.get("id"), group.get("rules")
                if not isinstance(group_id, str) or not group_id or not isinstance(rules, list) or not rules:
                    raise ValueError("each group needs a nonempty id and rules list")
                if any(not isinstance(rule, str) or not rule for rule in rules):
                    raise ValueError("rules must be nonempty strings")
                ids.append(group_id)
                assigned.extend(rules)
            if len(set(ids)) != len(ids) or len(set(assigned)) != len(assigned) or set(assigned) != original:
                raise ValueError("groups must exactly partition the original rules with distinct ids")
            if db.execute("SELECT 1 FROM jobs WHERE id IN (%s) LIMIT 1" % ",".join("?" for _ in ids), ids).fetchone():
                raise ValueError("group job id already exists")
            for group in groups:
                db.execute("INSERT INTO jobs(id,payload,status) VALUES (?,?,'pending')", (group["id"], _json(group)))
                db.executemany("UPDATE rules SET job_id=? WHERE rule=?", [(group["id"], rule) for rule in group["rules"]])
            db.execute("UPDATE jobs SET status='superseded' WHERE id=?", (jobid,))
            self._event(db, "split", {"job_id": jobid, "groups": groups, "evidence": evidence})

    def add_checkpoint(self, commit, evidence, expected_parent=None):
        if not isinstance(commit, str) or not commit:
            raise ValueError("commit is required")
        encoded = _evidence(evidence)
        with self._transaction() as db:
            row = db.execute("SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1").fetchone()
            parent = row[0] if row else None
            if expected_parent != parent:
                raise ValueError("checkpoint parent conflict")
            try:
                db.execute("INSERT INTO checkpoints(commit_id,parent,evidence) VALUES (?,?,?)", (commit, parent, encoded))
            except sqlite3.IntegrityError as exc:
                raise ValueError("checkpoint already exists") from exc
            self._event(db, "checkpoint", {"commit": commit, "parent": parent, "evidence": evidence})

    def claim(self, worker, checkpoint=None, kinds=None):
        if not isinstance(worker, str) or not worker:
            raise ValueError("worker is required")
        with self._transaction() as db:
            latest = db.execute("SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1").fetchone()
            if not latest:
                raise ValueError("a checkpoint is required before claiming")
            base = latest[0]
            if checkpoint is not None and checkpoint != base:
                raise ValueError("checkpoint is stale")
            config = json.loads(db.execute("SELECT value FROM metadata WHERE key='config'").fetchone()[0])
            active = db.execute("SELECT count(*) FROM jobs WHERE status='running'").fetchone()[0]
            if active >= config.get("max_workers", 12):
                return None
            if db.execute("SELECT 1 FROM attempts a JOIN jobs j ON j.attempt_id=a.id WHERE j.status='running' AND a.worker=?", (worker,)).fetchone():
                raise ValueError("worker already owns a running job")
            if kinds is not None:
                if isinstance(kinds, str):
                    raise ValueError("kinds must be a collection")
                kinds = list(kinds)
                if not kinds or any(not isinstance(kind, str) or not kind for kind in kinds):
                    raise ValueError("kinds must contain nonempty strings")
            candidates = db.execute("SELECT id,payload FROM jobs WHERE status='pending' ORDER BY rowid")
            row = next((candidate for candidate in candidates
                        if kinds is None or json.loads(candidate[1]).get("kind") in kinds), None)
            if not row:
                return None
            attempt = str(uuid.uuid4())
            db.execute("INSERT INTO attempts(id,job_id,worker,base_commit) VALUES (?,?,?,?)", (attempt, row[0], worker, base))
            db.execute("UPDATE jobs SET status='running',attempt_id=? WHERE id=?", (attempt, row[0]))
            result = {"attempt_id": attempt, "job": json.loads(row[1]), "base_commit": base}
            self._event(db, "claimed", {**result, "worker": worker})
            return result

    def mark_started(self, attempt, pid=None, receipt=None):
        if pid is None and receipt is None:
            raise ValueError("pid or receipt is required")
        details = {"pid": pid, "receipt": receipt}
        encoded = _json(details)
        with self._transaction() as db:
            row = db.execute("SELECT a.started,j.status FROM attempts a JOIN jobs j ON j.id=a.job_id WHERE a.id=? AND j.attempt_id=a.id", (attempt,)).fetchone()
            if not row or row[1] != "running":
                raise ValueError("attempt is not running")
            if row[0] is not None:
                if row[0] != encoded:
                    raise ValueError("start receipt is immutable")
                return
            db.execute("UPDATE attempts SET started=? WHERE id=?", (encoded, attempt))
            self._event(db, "started", {"attempt_id": attempt, **details})

    def finish(self, attempt, status, evidence, outcomes):
        if status not in ("provisional-pass", "needs-review", "failed"):
            raise ValueError("invalid finish status")
        _evidence(evidence)
        if not isinstance(outcomes, dict):
            raise ValueError("outcomes must map every rule to its disposition")
        result = {"status": status, "evidence": evidence, "outcomes": outcomes}
        encoded = _json(result)
        with self._transaction() as db:
            row = db.execute("SELECT a.job_id,a.result,j.status FROM attempts a JOIN jobs j ON j.id=a.job_id WHERE a.id=?", (attempt,)).fetchone()
            if not row:
                raise ValueError("unknown attempt")
            if row[1] is not None:
                if row[1] != encoded:
                    raise ValueError("attempt result is immutable")
                return
            if row[2] != "running":
                raise ValueError("attempt is not running")
            rules = {r[0] for r in db.execute("SELECT rule FROM rules WHERE job_id=?", (row[0],))}
            if set(outcomes) != rules:
                raise ValueError("outcomes must cover exactly every assigned rule")
            for outcome in outcomes.values():
                if not isinstance(outcome, dict) or outcome.get("status") not in ("pass", "needs-review") or not outcome.get("evidence"):
                    raise ValueError("each outcome needs a status and evidence")
                if outcome["status"] == "needs-review" and not outcome.get("reason"):
                    raise ValueError("needs-review outcome requires a reason")
            if status == "provisional-pass" and any(o["status"] != "pass" for o in outcomes.values()):
                raise ValueError("provisional pass requires every rule to pass")
            db.execute("UPDATE attempts SET result=? WHERE id=?", (encoded, attempt))
            db.execute("UPDATE jobs SET status=? WHERE id=?", (status, row[0]))
            self._event(db, "finished", {"attempt_id": attempt, **result})

    @contextmanager
    def _retry_lease(self, job_id):
        if (not isinstance(job_id, str) or not job_id or job_id in (".", "..")
                or Path(job_id).name != job_id or "\x00" in job_id):
            raise ValueError("job ID must identify a single lease file")
        lease_path = Path(self.path).parent / "leases" / job_id
        try:
            fd = os.open(lease_path, os.O_RDWR | os.O_NOFOLLOW | os.O_CLOEXEC)
        except OSError as exc:
            raise ValueError("existing attempt lease is unavailable") from exc
        try:
            if not stat.S_ISREG(os.fstat(fd).st_mode):
                raise ValueError("attempt lease is not a regular file")
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as exc:
                raise ValueError("attempt lease is still held") from exc
            yield
        finally:
            os.close(fd)

    def retry_failed(self, job_id, attempt_id, evidence, max_retries):
        """Requeue a finished failure only after proving its process and lease are gone.

        A live or reused PID, unknown start, missing lease or held lease refuses
        recovery. The kernel lock stays held through the transactional update.
        """
        _evidence(evidence)
        if type(max_retries) is not int or max_retries < 0:
            raise ValueError("max_retries must be a nonnegative integer")
        with self._retry_lease(job_id):
            with self._transaction() as db:
                row = db.execute(
                    "SELECT j.status,j.attempt_id,a.started,a.result FROM jobs j "
                    "JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?", (job_id,)).fetchone()
                if not row or row[0] != "failed" or row[1] != attempt_id or not row[3]:
                    raise ValueError("retry requires the current finished failed attempt")
                start = json.loads(row[2]) if row[2] else {}
                pid = start.get("pid")
                if type(pid) is not int or pid <= 0:
                    raise ValueError("attempt process identity is unknown")
                try:
                    os.kill(pid, 0)
                except ProcessLookupError:
                    pass
                except OSError as exc:
                    if exc.errno != errno.ESRCH:
                        raise ValueError("attempt process state is unknown") from exc
                else:
                    raise ValueError("attempt process is still present")
                attempts = db.execute("SELECT count(*) FROM attempts WHERE job_id=?", (job_id,)).fetchone()[0]
                if attempts - 1 >= max_retries:
                    raise ValueError("retry limit reached")
                db.execute("UPDATE jobs SET status='pending',attempt_id=NULL WHERE id=?", (job_id,))
                self._event(db, "retry", {"job_id": job_id, "attempt_id": attempt_id,
                                          "pid": pid, "process": "absent", "lease": "acquired",
                                          "retry_number": attempts, "evidence": evidence})

    def retry_tainted(self, job_id, attempt_id, evidence):
        """Retry only an explicitly verified read-only CPU-pool infrastructure failure.

        The controller must independently verify its terminal invocation and the
        provenance of the cited artifact. An absent recorded PID and an unlocked
        job lease are checked again here while the queue transaction is held.
        """
        _evidence(evidence)
        terminal = evidence.get("controller_terminal") if isinstance(evidence, dict) else None
        failure = evidence.get("infrastructure_failure") if isinstance(evidence, dict) else None
        if (not isinstance(terminal, dict)
                or not isinstance(terminal.get("unit"), str) or not terminal["unit"]
                or not isinstance(terminal.get("invocation"), str) or not terminal["invocation"]
                or not isinstance(terminal.get("verification"), dict) or not terminal["verification"]
                or not isinstance(failure, dict)
                or failure.get("kind") != "sandbox-cpu-pool-read-only"
                or not isinstance(failure.get("artifact"), str) or not failure["artifact"]
                or not isinstance(failure.get("sha256"), str) or len(failure["sha256"]) != 64):
            raise ValueError("terminal controller and infrastructure failure evidence required")
        try:
            path = Path(failure["artifact"])
            if not path.is_file() or path.stat().st_size > 32 * 1024 * 1024:
                raise ValueError("infrastructure artifact is unavailable or too large")
            artifact = path.read_bytes()
        except OSError as exc:
            raise ValueError("infrastructure artifact is unavailable") from exc
        if (hashlib.sha256(artifact).hexdigest() != failure["sha256"]
                or b"Errno 30" not in artifact or b"allocator.lock" not in artifact):
            raise ValueError("infrastructure artifact does not prove the stated failure")
        with self._retry_lease(job_id):
            with self._transaction() as db:
                row = db.execute(
                    "SELECT j.status,j.attempt_id,a.started,a.result FROM jobs j "
                    "JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?", (job_id,)).fetchone()
                if not row or row[0] not in ("needs-review", "failed") or row[1] != attempt_id or not row[3]:
                    raise ValueError("retry requires the current terminal review or failed attempt")
                start = json.loads(row[2]) if row[2] else {}
                pid = start.get("pid")
                if type(pid) is not int or pid <= 0:
                    raise ValueError("attempt process identity is unknown")
                try:
                    os.kill(pid, 0)
                except ProcessLookupError:
                    pass
                except OSError as exc:
                    if exc.errno != errno.ESRCH:
                        raise ValueError("attempt process state is unknown") from exc
                else:
                    raise ValueError("attempt process is still present")
                db.execute("UPDATE jobs SET status='pending',attempt_id=NULL WHERE id=?", (job_id,))
                self._event(db, "retry-tainted", {"job_id": job_id, "attempt_id": attempt_id,
                                                  "pid": pid, "process": "absent", "lease": "acquired",
                                                  "evidence": evidence})

    def retry_unstarted(self, job_id, attempt_id, evidence):
        """Administrative recovery of an explicitly proved pre-launch failure.

        The caller verifies the named controller is terminal and launch never
        occurred; a missing or unknown PID is not sufficient evidence.
        """
        _evidence(evidence)
        terminal = evidence.get("controller_terminal") if isinstance(evidence, dict) else None
        if (not isinstance(evidence, dict) or evidence.get("launch_not_attempted") is not True
                or not isinstance(terminal, dict)
                or not isinstance(terminal.get("unit"), str) or not terminal["unit"]
                or not isinstance(terminal.get("invocation"), str) or not terminal["invocation"]
                or not isinstance(terminal.get("verification"), dict) or not terminal["verification"]):
            raise ValueError("explicit terminal controller and pre-launch evidence required")
        with self._retry_lease(job_id):
            with self._transaction() as db:
                row = db.execute(
                    "SELECT j.status,j.attempt_id,a.started,a.result FROM jobs j "
                    "JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?", (job_id,)).fetchone()
                if not row or row[0] != "failed" or row[1] != attempt_id or not row[3]:
                    raise ValueError("retry requires the current finished failed attempt")
                if row[2] is not None:
                    raise ValueError("attempt has a recorded start")
                db.execute("UPDATE jobs SET status='pending',attempt_id=NULL WHERE id=?", (job_id,))
                self._event(db, "retry-unstarted", {"job_id": job_id, "attempt_id": attempt_id,
                                                    "lease": "acquired", "evidence": evidence})

    def _adjudicate(self, jobids, status, evidence, checkpoint=None):
        _evidence(evidence)
        jobids = list(jobids)
        if not jobids or len(set(jobids)) != len(jobids):
            raise ValueError("distinct jobs are required")
        with self._transaction() as db:
            if status == "accepted":
                latest = db.execute("SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1").fetchone()
                if not latest or checkpoint != latest[0]:
                    raise ValueError("acceptance requires the latest checkpoint")
            detail = {"status": status, "checkpoint": checkpoint, "evidence": evidence}
            for job_id in jobids:
                row = db.execute("SELECT status FROM jobs WHERE id=?", (job_id,)).fetchone()
                if not row or row[0] != "provisional-pass":
                    raise ValueError("only provisional passes can be adjudicated")
                db.execute("UPDATE jobs SET status=?,adjudication=? WHERE id=?", (status, _json(detail), job_id))
            self._event(db, "adjudicated", {"jobs": jobids, **detail})

    def advance_and_accept(self, commit, parent, jobids, evidence):
        """Atomically advance the checkpoint and accept all provisional jobs."""
        if not isinstance(commit, str) or not commit:
            raise ValueError("commit is required")
        encoded = _evidence(evidence)
        jobids = list(jobids)
        if not jobids or len(set(jobids)) != len(jobids):
            raise ValueError("distinct jobs are required")
        with self._transaction() as db:
            latest = db.execute("SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1").fetchone()
            actual_parent = latest[0] if latest else None
            if parent != actual_parent:
                raise ValueError("checkpoint parent conflict")
            for job_id in jobids:
                row = db.execute("SELECT status FROM jobs WHERE id=?", (job_id,)).fetchone()
                if not row or row[0] != "provisional-pass":
                    raise ValueError("only provisional passes can be adjudicated")
            try:
                db.execute("INSERT INTO checkpoints(commit_id,parent,evidence) VALUES (?,?,?)",
                           (commit, parent, encoded))
            except sqlite3.IntegrityError as exc:
                raise ValueError("checkpoint already exists") from exc
            detail = {"status": "accepted", "checkpoint": commit, "evidence": evidence}
            for job_id in jobids:
                db.execute("UPDATE jobs SET status='accepted',adjudication=? WHERE id=?",
                           (_json(detail), job_id))
            self._event(db, "checkpoint", {"commit": commit, "parent": parent, "evidence": evidence})
            self._event(db, "adjudicated", {"jobs": jobids, **detail})

    def accept_jobs(self, jobids, checkpoint, evidence):
        self._adjudicate(jobids, "accepted", evidence, checkpoint)

    def reject_jobs(self, jobids, evidence):
        self._adjudicate(jobids, "integration-rejected", evidence)

    def coverage(self):
        with closing(self._connect()) as db:
            db.execute("BEGIN")
            jobs = []
            counts = {}
            rule_counts = {}
            rules = {}
            for row in db.execute("SELECT id,payload,status,attempt_id,adjudication FROM jobs WHERE status != 'superseded' ORDER BY rowid"):
                job = {"id": row[0], "rules": json.loads(row[1])["rules"], "status": row[2], "attempt_id": row[3], "adjudication": json.loads(row[4]) if row[4] else None}
                counts[row[2]] = counts.get(row[2], 0) + len(job["rules"])
                attempt = db.execute("SELECT result FROM attempts WHERE id=?", (row[3],)).fetchone() if row[3] else None
                result = json.loads(attempt[0]) if attempt and attempt[0] else None
                job["outcomes"] = result["outcomes"] if result else None
                for rule in job["rules"]:
                    outcome = result["outcomes"][rule] if result else None
                    disposition = outcome["status"] if outcome else row[2]
                    if row[2] in ("accepted", "integration-rejected", "failed"):
                        disposition = row[2]
                    rule_counts[disposition] = rule_counts.get(disposition, 0) + 1
                    rules[rule] = {"job_id": row[0], "disposition": disposition, "outcome": outcome}
                jobs.append(job)
            total = db.execute("SELECT count(*) FROM rules").fetchone()[0]
            latest = db.execute("SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1").fetchone()
            return {"total_rules": total, "counts": counts, "accepted_rules": counts.get("accepted", 0), "rule_counts": rule_counts, "rules": rules, "jobs": jobs, "checkpoint": latest[0] if latest else None}

    def get_jobs(self, status=None, limit=None):
        if limit is not None and (not isinstance(limit, int) or isinstance(limit, bool) or limit < 1):
            raise ValueError("limit must be a positive integer")
        query = "SELECT id,payload,status,attempt_id,adjudication FROM jobs"
        args = []
        if status is not None:
            query += " WHERE status=?"
            args.append(status)
        query += " ORDER BY rowid"
        if limit is not None:
            query += " LIMIT ?"
            args.append(limit)
        with closing(self._connect()) as db:
            return [{"id": row[0], "job": json.loads(row[1]), "status": row[2],
                     "attempt_id": row[3], "adjudication": json.loads(row[4]) if row[4] else None}
                    for row in db.execute(query, args)]

    def get_attempt(self, attemptid):
        with closing(self._connect()) as db:
            row = db.execute("SELECT id,job_id,worker,base_commit,started,result FROM attempts WHERE id=?", (attemptid,)).fetchone()
            if not row:
                return None
            return {"attempt_id": row[0], "job_id": row[1], "worker": row[2],
                    "base_commit": row[3], "started": json.loads(row[4]) if row[4] else None,
                    "result": json.loads(row[5]) if row[5] else None}

    def inventory(self):
        return self.coverage()

    def audit(self):
        with closing(self._connect()) as db:
            return [{"seq": r[0], "kind": r[1], "data": json.loads(r[2]), "at": r[3]} for r in db.execute("SELECT seq,kind,data,at FROM events ORDER BY seq")]
