#!/usr/bin/env python3
"""AAHC Phase 1 bootstrap: create the company database and seed the free-tier pool.

Usage:
    python3 init_db.py                 # writes backend/data/aahc.db
    python3 init_db.py --db /tmp/x.db  # explicit path
"""

from __future__ import annotations

import argparse
from pathlib import Path

from db import get_conn

SCHEMA_FILE = Path(__file__).resolve().parent / "schema.sql"
VAULT_DIR = Path(__file__).resolve().parent / "skills_vault"

# Default zero-cost routing lanes. Order within a cluster = attempt order.
DEFAULT_ROUTER_POOL: list[tuple[str, str, str, str, int]] = [
    # provider_name, cluster_type, model_identifier, env_key_name, rpm_limit
    ("openrouter_1", "CLUSTER_CODE", "qwen/qwen-2.5-coder-32b-instruct:free", "OPENROUTER_API_KEY", 20),
    ("gemini_1", "CLUSTER_CODE", "gemini-2.5-flash", "GEMINI_API_KEY", 15),
    ("ollama_local", "CLUSTER_CODE", "qwen2.5-coder:7b", "", 0),
    ("groq_1", "CLUSTER_FAST", "llama-3.3-70b-versatile", "GROQ_API_KEY_1", 30),
    ("cerebras_1", "CLUSTER_FAST", "llama-3.3-70b", "CEREBRAS_API_KEY", 30),
    ("groq_2", "CLUSTER_FAST", "llama-3.3-70b-versatile", "GROQ_API_KEY_2", 30),
    ("openrouter_1", "CLUSTER_REASONING", "deepseek/deepseek-r1:free", "OPENROUTER_API_KEY", 20),
    ("groq_1", "CLUSTER_REASONING", "deepseek-r1-distill-llama-70b", "GROQ_API_KEY_1", 30),
    ("gemini_1", "CLUSTER_REASONING", "gemini-2.5-flash", "GEMINI_API_KEY", 15),
    ("gemini_1", "CLUSTER_VISION", "gemini-2.5-flash", "GEMINI_API_KEY", 15),
    ("ollama_local", "CLUSTER_VISION", "llama3.2-vision:11b", "", 0),
]


def seed_router_pool(conn) -> int:
    inserted = 0
    for row in DEFAULT_ROUTER_POOL:
        cur = conn.execute(
            """
            INSERT OR IGNORE INTO router_api_pool
                (provider_name, cluster_type, model_identifier, env_key_name, rpm_limit)
            VALUES (?, ?, ?, ?, ?)
            """,
            row,
        )
        inserted += cur.rowcount if cur.rowcount > 0 else 0
    return inserted


def seed_skill_registry(conn) -> int:
    """Register every folder in skills_vault/ that carries a SKILL.md (unverified)."""
    if not VAULT_DIR.is_dir():
        return 0
    inserted = 0
    for skill_dir in sorted(p for p in VAULT_DIR.iterdir() if p.is_dir()):
        skill_md = skill_dir / "SKILL.md"
        if not skill_md.is_file():
            continue
        cur = conn.execute(
            """
            INSERT OR IGNORE INTO skills_registry
                (skill_id, skill_name, category, source_type, install_command, skill_md_path, is_verified, created_by)
            VALUES (?, ?, ?, ?, ?, ?, 0, 'SYSTEM')
            """,
            (
                skill_dir.name,
                skill_dir.name.replace("_", " ").title(),
                "CORE",
                "LOCAL_PYTHON",
                f"pip install -r {skill_dir.name}/requirements.txt",
                str(skill_md),
            ),
        )
        inserted += cur.rowcount if cur.rowcount > 0 else 0
    return inserted


def init_db(db_path: str | None = None) -> dict[str, int]:
    """Create/upgrade the schema and seed defaults. Returns per-table row counts."""
    conn = get_conn(db_path)
    try:
        conn.executescript(SCHEMA_FILE.read_text(encoding="utf-8"))
        lanes = seed_router_pool(conn)
        skills = seed_skill_registry(conn)
        conn.commit()
        tables = [
            r[0]
            for r in conn.execute(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
            )
        ]
        counts = {t: conn.execute(f"SELECT COUNT(*) FROM {t}").fetchone()[0] for t in tables}
        counts["_new_router_lanes"] = lanes
        counts["_new_skills"] = skills
        return counts
    finally:
        conn.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Initialise the AAHC SQLite database.")
    parser.add_argument("--db", default=None, help="Database path (default: backend/data/aahc.db)")
    args = parser.parse_args()
    summary = init_db(args.db)
    print("AAHC database ready.")
    for key, value in summary.items():
        print(f"  {key}: {value}")
