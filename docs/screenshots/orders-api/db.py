import queue
import sqlite3
from contextlib import contextmanager


class Pool:
    def __init__(self, path="orders.db", max_connections=10, timeout=5.0):
        self.timeout = timeout
        self.idle = queue.Queue()
        for _ in range(max_connections):
            self.idle.put(sqlite3.connect(path, check_same_thread=False))

    def acquire(self):
        try:
            return self.idle.get(timeout=self.timeout)
        except queue.Empty:
            raise TimeoutError(f"timed out after {self.timeout:.0f}s waiting for a connection")

    def release(self, conn):
        self.idle.put(conn)

    @contextmanager
    def connection(self):
        conn = self.acquire()
        try:
            yield conn
        finally:
            self.release(conn)


pool = Pool()
