"""Shared SQLite helpers for the AAHC backend.

Every backend module (``smart_9router``, ``hr_architect``, ``skill_hunter``) opens
its own connection through :func:`get_conn` so the modules stay importable and
testable in isolation while sharing the same WAL pragmas the 24/7 service needs.
"""

from __future__ import annotations

import datetime as _dt
import os
import sqlite3
from pathlib import Path

DEFAULT_DB_PATH = Path(__file__).resolve().parent / "data" / "aahc.db"


def resolve_db_path(db_path: str | Path | None = None) -> Path:
    """Return the database path, honouring ``AAHC_DB_PATH`` when none is passed."""
    if db_path is not None:
        return Path(db_path)
    env_path = os.environ.get("AAHC_DB_PATH")
    return Path(env_path) if env_path else DEFAULT_DB_PATH


def get_conn(db_path: str | Path | None = None) -> sqlite3.Connection:
    """Open a SQLite connection configured for concurrent local use."""
    path = resolve_db_path(db_path)
    path.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(str(path), timeout=30)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA foreign_keys=ON")
    conn.execute("PRAGMA busy_timeout=5000")
    return conn


def utcnow_iso() -> str:
    """Current UTC timestamp in the storage format used across the schema."""
    return _dt.datetime.now(_dt.timezone.utc).strftime("%Y-%m-%d %H:%M:%S")
