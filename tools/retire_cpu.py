"""Shared, crash-safe native CPU token leases for retirement campaigns."""
from contextlib import contextmanager
import fcntl
import os
from pathlib import Path
import time

MAX_CAPACITY = 32
POLL_SECONDS = 0.2


@contextmanager
def reserve(root: Path, threads: int, capacity: int = MAX_CAPACITY, wait: bool = True,
            *, priority_wait: bool = False, background: bool = False, minimum_threads: int | None = None):
    """Reserve all requested tokens together and yield their open file descriptors.

    A caller that can scale its work to the granted count may supply a minimum.
    It gets the largest available bundle between that minimum and its request.
    Every process using the same root participates in the same pool. A smaller
    capacity uses its first tokens, so temporary restrictions remain compatible
    with larger reservations. Pass the yielded descriptors in subprocess
    ``pass_fds`` to keep the lease alive after the reserving process exits.
    Tokens are released solely by closing every copy of each descriptor. A
    waiting bundle above two tokens, at least half the pool, or explicitly
    marked priority gates new allocations until it acquires all tokens. Existing
    reservations continue and unused capacity is available afterward.
    Background requests sample for a complete bundle without keeping that gate
    while they wait. Foreground requests keep their existing priority.
    """
    if (isinstance(capacity, bool) or not isinstance(capacity, int)
            or not 1 <= capacity <= MAX_CAPACITY):
        raise ValueError(f'capacity must be an integer from 1 to {MAX_CAPACITY}')
    if isinstance(threads, bool) or not isinstance(threads, int) or not 1 <= threads <= capacity:
        raise ValueError('threads must be an integer between 1 and capacity')
    minimum = threads if minimum_threads is None else minimum_threads
    if type(minimum) is not int or not 1 <= minimum <= threads:
        raise ValueError('minimum_threads must be an integer between 1 and threads')
    if type(background) is not bool or (background and priority_wait):
        raise ValueError('background reservations cannot request waiting priority')
    root = Path(root)
    root.mkdir(parents=True, exist_ok=True)
    allocator = os.open(root / 'allocator.lock', os.O_CREAT | os.O_RDWR, 0o600)
    try:
        priority = os.open(root / 'priority.lock', os.O_CREAT | os.O_RDWR, 0o600)
    except BaseException:
        os.close(allocator)
        raise
    held = []
    exclusive_wait = not background and (priority_wait or threads > 2 or threads >= capacity // 2)
    priority_held = False
    try:
        while True:
            # A priority waiter holds this gate until the full bundle is free.
            # Other reservations take it for each allocation scan.
            if not priority_held:
                try:
                    fcntl.flock(priority, fcntl.LOCK_EX | (0 if wait and not background else fcntl.LOCK_NB))
                except BlockingIOError:
                    if not wait:
                        raise
                    time.sleep(POLL_SECONDS)
                    continue
                priority_held = True
            # Serialize scans so no waiter can hold part of a competing bundle.
            fcntl.flock(allocator, fcntl.LOCK_EX)
            try:
                for number in range(capacity):
                    fd = os.open(root / f'token-{number:02}.lock', os.O_CREAT | os.O_RDWR, 0o600)
                    try:
                        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    except BlockingIOError:
                        os.close(fd)
                        continue
                    except BaseException:
                        os.close(fd)
                        raise
                    held.append(fd)
                    if len(held) == threads:
                        break
                if len(held) < minimum:
                    for fd in held:
                        os.close(fd)
                    held.clear()
            finally:
                fcntl.flock(allocator, fcntl.LOCK_UN)
            if held or not exclusive_wait:
                fcntl.flock(priority, fcntl.LOCK_UN)
                priority_held = False
            if held:
                break
            if not wait:
                raise BlockingIOError('not enough CPU lease tokens available')
            time.sleep(POLL_SECONDS)
        yield tuple(held)
    finally:
        for fd in held:
            # Never LOCK_UN: a helper may share this open file description.
            os.close(fd)
        os.close(priority)
        os.close(allocator)
