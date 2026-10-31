import csv

from db import pool

CHUNK = 10_000


def count_orders():
    with pool.connection() as conn:
        return conn.execute("SELECT COUNT(*) FROM orders").fetchone()[0]


def export_orders(out):
    writer = csv.writer(out)
    for offset in range(0, count_orders(), CHUNK):
        conn = pool.acquire()
        rows = conn.execute(
            "SELECT id, customer, total, created_at FROM orders LIMIT ? OFFSET ?",
            (CHUNK, offset),
        )
        writer.writerows(rows)
