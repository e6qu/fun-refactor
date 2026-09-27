"""Serialize a session's observations, reviews, mutations and event identities."""
from contextlib import contextmanager
import fcntl
import os


@contextmanager
def session_lock(session):
    descriptor = os.open(session / "request.lock", os.O_CREAT | os.O_RDWR | getattr(os, "O_NOFOLLOW", 0), 0o600)
    try:
        fcntl.flock(descriptor, fcntl.LOCK_EX)
        yield
    finally:
        os.close(descriptor)
