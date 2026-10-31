import io

from flask import Flask, Response

from db import pool
from export import export_orders

app = Flask(__name__)


@app.get("/orders")
def orders():
    with pool.connection() as conn:
        rows = conn.execute("SELECT id, customer, total FROM orders ORDER BY id DESC LIMIT 50")
        return {"orders": [dict(zip(("id", "customer", "total"), r)) for r in rows]}


@app.get("/orders/export")
def export():
    out = io.StringIO()
    export_orders(out)
    return Response(out.getvalue(), mimetype="text/csv")
