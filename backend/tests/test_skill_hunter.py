"""Offline tests for the Skill Hunter & Toolsmith."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from types import SimpleNamespace

import httpx
import pytest

from db import get_conn
from skill_hunter import (
    VAULT_DIR,
    hunt_skill,
    read_skill_meta,
    search_local,
    search_mcp_registry,
    search_pypi,
    slugify,
    sync_vault_to_registry,
    toolsmith_create,
    verify_skill,
)

SCHEMA = (Path(__file__).resolve().parents[1] / "schema.sql").read_text(encoding="utf-8")

EXECUTOR_WITH_SELFTEST = (
    "import argparse, sys\n"
    "parser = argparse.ArgumentParser()\n"
    "parser.add_argument('--selftest', action='store_true')\n"
    "parser.add_argument('--query', default=None)\n"
    "args = parser.parse_args()\n"
    "if args.selftest:\n"
    "    print('OK')\n"
)


@pytest.fixture()
def db_path(tmp_path):
    path = tmp_path / "skills.db"
    conn = get_conn(path)
    conn.executescript(SCHEMA)
    conn.commit()
    conn.close()
    return path


@pytest.fixture()
def vault(tmp_path):
    directory = tmp_path / "skills_vault"
    directory.mkdir()
    return directory


class Proc:
    def __init__(self, returncode=0, stdout="OK", stderr=""):
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr


def fake_runner(returncode=0, stderr=""):
    def run(command, **kwargs):
        return Proc(returncode=returncode, stderr=stderr)

    return run


class FakeRouter:
    def __init__(self, content):
        self.content = content
        self.calls = []

    async def chat(self, messages, cluster, **kwargs):
        self.calls.append({"messages": messages, "cluster": cluster, "kwargs": kwargs})
        return SimpleNamespace(content=self.content)


def pypi_response(name="crawl4ai", summary="Web crawler for LLMs"):
    return httpx.Response(
        200,
        json={"info": {"name": name, "summary": summary}},
        request=httpx.Request("GET", f"https://pypi.org/pypi/{name}/json"),
    )


def mcp_response(qualified="@mcp/foo", description="MCP server"):
    return httpx.Response(
        200,
        json={"servers": [{"qualifiedName": qualified, "description": description, "homepage": "https://example.com"}]},
        request=httpx.Request("GET", "https://registry.smithery.ai/servers"),
    )


def make_transport(pypi=None, github=None, mcp=None, record=None):
    async def transport(request):
        if record is not None:
            record.append(str(request.url))
        host = request.url.host
        if "pypi.org" in host:
            return pypi or httpx.Response(404, request=request)
        if "api.github.com" in host:
            return github or httpx.Response(403, request=request)
        if "smithery.ai" in host:
            return mcp or httpx.Response(500, request=request)
        return httpx.Response(404, request=request)

    return transport


def registry_row(db_path, skill_id):
    conn = get_conn(db_path)
    try:
        row = conn.execute("SELECT * FROM skills_registry WHERE skill_id = ?", (skill_id,)).fetchone()
        return dict(row) if row else None
    finally:
        conn.close()


# --------------------------------------------------------------------- local vault


def test_search_local_finds_the_seeded_web_search_skill():
    hits = search_local("web_search", VAULT_DIR)

    assert len(hits) == 1
    assert hits[0].source == "LOCAL"
    assert hits[0].name == "web_search"


def test_read_skill_meta_reads_the_seeded_header():
    meta = read_skill_meta(VAULT_DIR / "web_search" / "SKILL.md")

    assert meta["name"] == "web_search"
    assert meta["category"] == "CORE"
    assert meta["source_type"] == "API_WRAPPER"
    assert meta["install_command"] == "pip install httpx"


def test_metadata_outside_the_header_window_is_ignored(vault):
    skill_md = vault / "late_meta" / "SKILL.md"
    skill_md.parent.mkdir()
    skill_md.write_text("\n".join([f"line {i}" for i in range(20)] + ["name: late_meta"]), encoding="utf-8")

    assert read_skill_meta(skill_md) == {}


async def test_hunt_skill_reports_already_local(tmp_path):
    db = tmp_path / "a.db"
    conn = get_conn(db)
    conn.executescript(SCHEMA)
    conn.commit()
    conn.close()

    report = await hunt_skill("web_search", vault_dir=VAULT_DIR, db_path=db)

    assert report.status == "ALREADY_LOCAL"
    assert report.source == "LOCAL"
    assert report.verified is False


def test_sync_vault_to_registry_is_idempotent(db_path):
    first = sync_vault_to_registry(VAULT_DIR, db_path)
    second = sync_vault_to_registry(VAULT_DIR, db_path)

    assert first >= 1
    assert second == 0
    row = registry_row(db_path, "web_search")
    assert row is not None
    assert row["is_verified"] == 0
    assert row["created_by"] == "SYSTEM"


# ------------------------------------------------------------- external registries


def test_slugify_collapses_punctuation():
    assert slugify("Crawl4AI Scraper!") == "crawl4ai_scraper"


async def test_search_pypi_returns_a_hit():
    hits = await search_pypi("crawl4ai", transport=make_transport(pypi=pypi_response()))

    assert len(hits) == 1
    assert hits[0].source == "PYPI"
    assert hits[0].install_command == "pip install crawl4ai"


async def test_search_pypi_404_returns_empty():
    hits = await search_pypi("nope", transport=make_transport())
    assert hits == []


async def test_search_mcp_registry_non_200_returns_empty():
    hits = await search_mcp_registry("foo", transport=make_transport())
    assert hits == []


async def test_search_mcp_registry_parses_qualified_name():
    hits = await search_mcp_registry("foo", transport=make_transport(mcp=mcp_response()))

    assert len(hits) == 1
    assert hits[0].source == "MCP_REGISTRY"
    assert hits[0].install_command == "npx -y @mcp/foo"


# ------------------------------------------------------------------ hunting + verify


async def test_hunt_skill_installs_from_pypi_and_verifies(db_path, vault):
    transport = make_transport(pypi=pypi_response())

    report = await hunt_skill("crawl4ai", transport=transport, vault_dir=vault, db_path=db_path)

    assert report.status == "INSTALLED"
    assert report.source == "PYPI"
    assert report.verified is True

    folder = vault / "crawl4ai"
    assert (folder / "SKILL.md").is_file()
    assert (folder / "tool_executor.py").is_file()
    assert (folder / "requirements.txt").is_file()

    row = registry_row(db_path, "crawl4ai")
    assert row is not None
    assert row["is_verified"] == 1
    assert row["created_by"] == "SKILL_HUNTER_AGENT"
    assert row["install_command"] == "pip install crawl4ai"


async def test_hunt_skill_not_found_without_router(db_path, vault):
    report = await hunt_skill("totally_unknown_skill_xyz", transport=make_transport(), vault_dir=vault, db_path=db_path)

    assert report.status == "NOT_FOUND"
    assert report.vault_path == ""
    assert list(vault.iterdir()) == []


async def test_github_rate_limit_falls_through_to_pypi(db_path, vault):
    transport = make_transport(pypi=pypi_response(), github=httpx.Response(403, request=httpx.Request("GET", "https://api.github.com")))

    report = await hunt_skill("crawl4ai", transport=transport, vault_dir=vault, db_path=db_path)

    assert report.source == "PYPI"
    assert report.status == "INSTALLED"


async def test_mcp_registry_takes_precedence_over_pypi(db_path, vault):
    record: list[str] = []
    transport = make_transport(pypi=pypi_response(), mcp=mcp_response(), record=record)

    report = await hunt_skill("foo", transport=transport, vault_dir=vault, db_path=db_path)

    assert report.source == "MCP_REGISTRY"
    assert not any("pypi.org" in url for url in record), "PyPI must not be queried once MCP answered"


def test_verify_skill_marks_verified_on_success(vault, db_path):
    folder = vault / "good_skill"
    folder.mkdir()
    (folder / "tool_executor.py").write_text(EXECUTOR_WITH_SELFTEST, encoding="utf-8")
    (folder / "SKILL.md").write_text("name: good_skill\n", encoding="utf-8")
    sync_vault_to_registry(vault, db_path)

    assert verify_skill("good_skill", vault_dir=vault, db_path=db_path, runner=fake_runner(0)) is True
    assert registry_row(db_path, "good_skill")["is_verified"] == 1


def test_verify_skill_keeps_unverified_on_failure(vault, db_path):
    folder = vault / "broken_skill"
    folder.mkdir()
    (folder / "tool_executor.py").write_text("import sys; sys.exit(1)\n", encoding="utf-8")
    (folder / "SKILL.md").write_text("name: broken_skill\n", encoding="utf-8")
    sync_vault_to_registry(vault, db_path)

    assert verify_skill("broken_skill", vault_dir=vault, db_path=db_path, runner=fake_runner(1, "boom")) is False
    assert registry_row(db_path, "broken_skill")["is_verified"] == 0


def test_verify_skill_missing_executor_returns_false(vault, db_path):
    assert verify_skill("ghost_skill", vault_dir=vault, db_path=db_path) is False


# ---------------------------------------------------------------------- toolsmith


async def test_toolsmith_creates_a_new_skill(db_path, vault):
    payload = json.dumps(
        {
            "skill_id": "olx_scraper",
            "skill_name": "OLX Scraper",
            "category": "MARKETING",
            "source_type": "API_WRAPPER",
            "purpose": "Scrape cheap property listings from OLX.",
            "executor_code": EXECUTOR_WITH_SELFTEST,
            "requirements": ["httpx"],
        }
    )
    router = FakeRouter(payload)

    report = await toolsmith_create("olx_scraper", "find cheap properties", router, vault_dir=vault, db_path=db_path)

    assert report.status == "INSTALLED"
    assert report.source == "TOOLSMITH"
    assert router.calls[0]["cluster"] == "CLUSTER_CODE"
    folder = vault / "olx_scraper"
    assert (folder / "tool_executor.py").is_file()
    assert "httpx" in (folder / "requirements.txt").read_text(encoding="utf-8")
    assert registry_row(db_path, "olx_scraper")["created_by"] == "SKILL_HUNTER_AGENT"


async def test_toolsmith_falls_back_to_template_when_executor_lacks_selftest(db_path, vault):
    payload = json.dumps(
        {
            "skill_id": "no_selftest",
            "skill_name": "No Selftest",
            "category": "CORE",
            "source_type": "LOCAL_PYTHON",
            "purpose": "demo",
            "executor_code": "print('hi')\n",
            "requirements": [],
        }
    )
    router = FakeRouter(payload)

    report = await toolsmith_create("no_selftest", "demo", router, vault_dir=vault, db_path=db_path)

    assert report.status == "INSTALLED"
    assert any("template" in note for note in report.notes)


# ----------------------------------------------------------------- real subprocess


def test_seeded_web_search_selftest_runs_without_network():
    executor = VAULT_DIR / "web_search" / "tool_executor.py"
    proc = subprocess.run(
        [sys.executable, str(executor), "--selftest"],
        capture_output=True,
        text=True,
        timeout=60,
    )

    assert proc.returncode == 0
    assert "OK" in proc.stdout
