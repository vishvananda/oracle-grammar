import fcntl
import multiprocessing
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

import retire_cpu


def holder(root, threads, capacity, ready, release):
    with retire_cpu.reserve(root, threads, capacity):
        ready.set()
        release.wait(10)


def waiting_bundle(root, threads, capacity, ready, acquired, release, priority_wait=False):
    ready.set()
    with retire_cpu.reserve(root, threads, capacity, priority_wait=priority_wait):
        acquired.set()
        release.wait(10)


def background_bundle(root, ready, acquired, release):
    ready.set()
    with retire_cpu.reserve(root, 4, 8, background=True):
        acquired.set()
        release.wait(10)


class LeaseTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.children = []

    def tearDown(self):
        for child in self.children:
            if child.is_alive():
                child.terminate()
            child.join(timeout=5)
        self.tmp.cleanup()

    def start(self, target, *args):
        process = multiprocessing.Process(target=target, args=args)
        process.start()
        self.children.append(process)
        return process

    def test_validation(self):
        for threads, capacity in [(0, 2), (3, 2), (1, 0), (1, 49), (True, 2)]:
            with self.subTest(threads=threads, capacity=capacity):
                with self.assertRaises(ValueError):
                    with retire_cpu.reserve(self.root, threads, capacity):
                        pass

    def test_real_contention_never_overcommits(self):
        ready, release = multiprocessing.Event(), multiprocessing.Event()
        self.start(holder, self.root, 2, 3, ready, release)
        self.assertTrue(ready.wait(5))
        with self.assertRaises(BlockingIOError):
            with retire_cpu.reserve(self.root, 2, 3, wait=False):
                pass
        with retire_cpu.reserve(self.root, 1, 3, wait=False) as lease:
            self.assertEqual(len(lease), 1)
            with self.assertRaises(BlockingIOError):
                with retire_cpu.reserve(self.root, 1, 3, wait=False):
                    pass
        release.set()

    def test_scalable_bundle_uses_free_tokens_without_waiting_for_the_requested_count(self):
        ready, release = multiprocessing.Event(), multiprocessing.Event()
        child = self.start(holder, self.root, 16, 32, ready, release)
        self.assertTrue(ready.wait(5))
        with self.assertRaises(BlockingIOError):
            with retire_cpu.reserve(self.root, 24, 32, wait=False):
                pass
        with retire_cpu.reserve(self.root, 24, 32, wait=False, minimum_threads=16) as tokens:
            self.assertEqual(len(tokens), 16)
            with self.assertRaises(BlockingIOError):
                with retire_cpu.reserve(self.root, 1, 32, wait=False):
                    pass
        release.set()
        child.join(timeout=5)
        self.assertEqual(child.exitcode, 0)
        with retire_cpu.reserve(self.root, 24, 32, wait=False, minimum_threads=16) as tokens:
            self.assertEqual(len(tokens), 24)

    def test_wide_integration_lease_takes_its_exclusive_tokens_first(self):
        def held(tokens):
            return sorted(int(Path(os.readlink(f'/proc/self/fd/{fd}')).stem.split('-')[1]) for fd in tokens)
        with retire_cpu.reserve(self.root, 8, 40, wait=False) as wide:
            self.assertEqual(held(wide), list(range(32, 40)))
            with retire_cpu.reserve(self.root, 4, 32, wait=False) as shared:
                self.assertEqual(held(shared), [0, 1, 2, 3])
            with retire_cpu.reserve(self.root, 12, 40, wait=False) as spill:
                self.assertEqual(held(spill), list(range(20, 32)))

    def test_scalable_bundle_enforces_its_minimum(self):
        for minimum in (False, 0, 3):
            with self.subTest(minimum=minimum), self.assertRaises(ValueError):
                with retire_cpu.reserve(self.root, 2, 3, minimum_threads=minimum):
                    pass
        with retire_cpu.reserve(self.root, 2, 3):
            with self.assertRaises(BlockingIOError):
                with retire_cpu.reserve(self.root, 3, 3, wait=False, minimum_threads=2):
                    pass
            with retire_cpu.reserve(self.root, 1, 3, wait=False) as tokens:
                self.assertEqual(len(tokens), 1)

    def wait_for_priority(self):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            try:
                with (self.root/'priority.lock').open('a') as gate:
                    fcntl.flock(gate, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                return
            time.sleep(0.02)
        self.fail('waiter never acquired priority')

    def assert_token_available(self, number):
        # Observe between allocation scans: a waiter may briefly hold a partial
        # bundle while scanning, then releases it before dropping the allocator.
        with (self.root/'allocator.lock').open('a') as allocator:
            fcntl.flock(allocator, fcntl.LOCK_EX)
            with (self.root/f'token-{number:02}.lock').open('a') as token:
                fcntl.flock(token, fcntl.LOCK_EX | fcntl.LOCK_NB)

    def test_waiting_large_bundle_blocks_new_small_bundles_until_acquired(self):
        ready, release = multiprocessing.Event(), multiprocessing.Event()
        self.start(holder, self.root, 5, 8, ready, release)
        self.assertTrue(ready.wait(5))
        waiting, acquired, finish = (multiprocessing.Event() for _ in range(3))
        self.start(waiting_bundle, self.root, 4, 8, waiting, acquired, finish)
        self.assertTrue(waiting.wait(5))
        self.wait_for_priority()
        for _ in range(3):
            with self.assertRaises(BlockingIOError):
                with retire_cpu.reserve(self.root, 2, 8, wait=False):
                    pass
        # The waiter returned partial scans; it does not hold otherwise free tokens.
        self.assert_token_available(5)
        self.assertFalse(acquired.is_set())
        release.set()
        self.assertTrue(acquired.wait(5))
        # Priority is released once the large bundle is acquired; headroom is usable.
        with retire_cpu.reserve(self.root, 2, 8, wait=False) as tokens:
            self.assertEqual(len(tokens), 2)
        finish.set()

    def test_waiting_background_bundle_leaves_capacity_for_foreground_workers(self):
        ready, release = multiprocessing.Event(), multiprocessing.Event()
        self.start(holder, self.root, 5, 8, ready, release)
        self.assertTrue(ready.wait(5))
        waiting, acquired, finish = (multiprocessing.Event() for _ in range(3))
        child = self.start(background_bundle, self.root, waiting, acquired, finish)
        self.assertTrue(waiting.wait(5))
        self.assertFalse(acquired.wait(2*retire_cpu.POLL_SECONDS))
        self.assertTrue(child.is_alive())
        deadline = time.monotonic()+2
        while True:
            try:
                with retire_cpu.reserve(self.root, 2, 8, wait=False) as tokens:
                    self.assertEqual(len(tokens), 2)
                    self.assertFalse(acquired.is_set())
                break
            except BlockingIOError:
                if time.monotonic() >= deadline:
                    self.fail('waiting background trial blocked free foreground tokens')
                time.sleep(0.02)
        release.set()
        self.assertTrue(acquired.wait(5))
        finish.set()

    def test_waiting_priority_replay_claims_one_slot_and_releases_gate(self):
        wide_ready, wide_release = multiprocessing.Event(), multiprocessing.Event()
        small_ready, small_release = multiprocessing.Event(), multiprocessing.Event()
        self.start(holder, self.root, 6, 8, wide_ready, wide_release)
        self.assertTrue(wide_ready.wait(5))
        self.start(holder, self.root, 2, 8, small_ready, small_release)
        self.assertTrue(small_ready.wait(5))
        waiting, acquired, finish = (multiprocessing.Event() for _ in range(3))
        self.start(waiting_bundle, self.root, 1, 8, waiting, acquired, finish, True)
        self.assertTrue(waiting.wait(5))
        self.wait_for_priority()
        self.assertFalse(acquired.is_set())
        with self.assertRaises(BlockingIOError):
            with retire_cpu.reserve(self.root, 1, 8, wait=False):
                pass
        small_release.set()
        self.assertTrue(acquired.wait(5))
        with retire_cpu.reserve(self.root, 1, 8, wait=False) as tokens:
            self.assertEqual(len(tokens), 1)
            with self.assertRaises(BlockingIOError):
                with retire_cpu.reserve(self.root, 1, 8, wait=False):
                    pass
        finish.set()
        wide_release.set()

    def test_dead_large_waiter_releases_priority_gate(self):
        ready, release = multiprocessing.Event(), multiprocessing.Event()
        self.start(holder, self.root, 5, 8, ready, release)
        self.assertTrue(ready.wait(5))
        waiting, acquired, finish = (multiprocessing.Event() for _ in range(3))
        child = self.start(waiting_bundle, self.root, 4, 8, waiting, acquired, finish)
        self.assertTrue(waiting.wait(5))
        self.wait_for_priority()
        child.terminate()
        child.join(5)
        self.assertFalse(child.is_alive())
        with retire_cpu.reserve(self.root, 2, 8, wait=False) as tokens:
            self.assertEqual(len(tokens), 2)
        release.set()

    def test_coordinator_below_half_pool_gets_waiting_priority(self):
        ready, release = multiprocessing.Event(), multiprocessing.Event()
        self.start(holder, self.root, 8, 12, ready, release)
        self.assertTrue(ready.wait(5))
        waiting, acquired, finish = (multiprocessing.Event() for _ in range(3))
        self.start(waiting_bundle, self.root, 5, 12, waiting, acquired, finish)
        self.assertTrue(waiting.wait(5))
        self.wait_for_priority()
        self.assertFalse(acquired.is_set())
        with self.assertRaises(BlockingIOError):
            with retire_cpu.reserve(self.root, 1, 12, wait=False):
                pass
        # A free token exists; only the queued coordinator prevents allocation.
        self.assert_token_available(8)
        release.set()
        self.assertTrue(acquired.wait(5))
        with retire_cpu.reserve(self.root, 1, 12, wait=False) as tokens:
            self.assertEqual(len(tokens), 1)
        finish.set()

    def test_inherited_fd_survives_reserver_exit(self):
        # The intermediate process exits without unlocking; the helper remains
        # alive with the token descriptor inherited via pass_fds.
        marker = self.root / 'helper-ready'
        code = """
import os, subprocess, sys
from pathlib import Path
import retire_cpu
root, marker = map(Path, sys.argv[1:])
with retire_cpu.reserve(root, 1, 1) as fds:
    helper = "import pathlib,sys,time; pathlib.Path(sys.argv[1]).touch(); time.sleep(2)"
    subprocess.Popen([sys.executable, '-c', helper, str(marker)], pass_fds=fds)
    os._exit(0)
"""
        subprocess.run([sys.executable, '-c', code, str(self.root), str(marker)],
                       check=True, env={**os.environ, 'PYTHONPATH': str(Path(retire_cpu.__file__).parent)})
        deadline = time.monotonic() + 5
        while not marker.exists() and time.monotonic() < deadline:
            time.sleep(0.05)
        self.assertTrue(marker.exists())
        with self.assertRaises(BlockingIOError):
            with retire_cpu.reserve(self.root, 1, 1, wait=False):
                pass
        deadline = time.monotonic() + 5
        while True:
            try:
                with retire_cpu.reserve(self.root, 1, 1, wait=False):
                    break
            except BlockingIOError:
                if time.monotonic() >= deadline:
                    self.fail('helper did not release inherited token')
                time.sleep(0.05)


if __name__ == '__main__':
    unittest.main()
