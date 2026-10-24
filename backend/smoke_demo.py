#!/usr/bin/env python3
"""Offline end-to-end smoke demo for the AAHC Phase 1 foundation.

Runs with no API keys and no network access:

    python3 smoke_demo.py
    python3 smoke_demo.py --db /tmp/aahc_demo.db
"""

from __future__ import annotations

import argparse
from pathlib import Path

from db import get_conn
from hr_architect import clone_trading_desk
from init_db import init_db
from skill_hunter import sync_vault_to_registry, verify_skill


def main() -> int:
    parser = argparse.ArgumentParser(description="AAHC Phase 1 offline smoke demo")
    parser.add_argument("--db", default=str(Path(__file__).resolve().parent / "data" / "demo.db"))
    args = parser.parse_args()

    summary = init_db(args.db)
    print("1. database ready:", {k: v for k, v in summary.items() if not k.startswith("_")})

    clone = clone_trading_desk(["BTC/USDT", "ETH/USDT", "SOL/USDT"], db_path=args.db)
    print(f"2. trading floor {clone['division_id']}: {len(clone['traders'])} traders, CRO = {clone['risk_manager_id']}")

    sync_vault_to_registry(db_path=args.db)
    passed = verify_skill("web_search", db_path=args.db)
    print(f"3. web_search sandbox verification: {'PASSED' if passed else 'FAILED'}")

    conn = get_conn(args.db)
    try:
        rows = conn.execute(
            """
            SELECT e.name, e.role_title, e.assigned_pair, e.model_cluster, e.desk_coord_x, e.desk_coord_y
            FROM employees e
            JOIN divisions d ON d.id = e.division_id
            ORDER BY e.id
            """
        ).fetchall()
        print(f"4. office roster ({len(rows)} desks):")
        for row in rows:
            pair = row["assigned_pair"] or "-"
            print(
                f"   ({row['desk_coord_x']:>2},{row['desk_coord_y']:>2}) "
                f"{row['name']:<14} {row['role_title']:<24} {pair:<10} {row['model_cluster']}"
            )
        lanes = conn.execute(
            "SELECT cluster_type, COUNT(*) AS n FROM router_api_pool GROUP BY cluster_type ORDER BY cluster_type"
        ).fetchall()
        print("5. free-tier router lanes:", {r["cluster_type"]: r["n"] for r in lanes})
    finally:
        conn.close()

    print("\nSmoke demo finished: no API keys and no network were used.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
