"""Offline tests for the Smart 9Router (FR-RTR-01 / FR-RTR-02)."""

from __future__ import annotations

import email.utils
import json
from datetime import datetime, timedelta, timezone
from pathlib import Path

import httpx
import pytest

from db import get_conn
from smart_9router import RouterError, SmartRouter, parse_retry_after

SCHEMA = (Path(__file__).resolve().parents[1] / "schema.sql").read_text(encoding="utf-8")
TS_FORMAT = "%Y-%m-%d %H:%M:%S"


@pytest.fixture(autouse=True)
def _keys(monkeypatch):
    monkeypatch.setenv("TEST_KEY", "secret-1")
    monkeypatch.setenv("TEST_KEY_2", "secret-2")
    monkeypatch.delenv("TEST_MISSING_1", raising=False)
    monkeypatch.delenv("TEST_MISSING_2", raising=False)


@pytest.fixture()
def db_path(tmp_path):
    path = tmp_path / "router.db"
    conn = get_conn(path)
    conn.executescript(SCHEMA)
    conn.commit()
    conn.close()
    return path


def add_lane(db_path, provider, cluster, model="m-1", env_key="TEST_KEY", rpm=30):
    conn = get_conn(db_path)
    try:
        with conn:
            conn.execute(
                """
                INSERT INTO router_api_pool
                    (provider_name, cluster_type, model_identifier, env_key_name, rpm_limit)
                VALUES (?, ?, ?, ?, ?)
                """,
                (provider, cluster, model, env_key, rpm),
            )
        return int(conn.execute("SELECT last_insert_rowid() AS id").fetchone()["id"])
    finally:
        conn.close()


def lane_row(db_path, lane_id):
    conn = get_conn(db_path)
    try:
        return dict(conn.execute("SELECT * FROM router_api_pool WHERE id = ?", (lane_id,)).fetchone())
    finally:
        conn.close()


def cooldown_seconds(row):
    until = datetime.strptime(row["cooldown_until"], TS_FORMAT).replace(tzinfo=timezone.utc)
    return (until - datetime.now(timezone.utc)).total_seconds()


def ok_response(content="hello", reasoning=None):
    message = {"content": content}
    if reasoning is not None:
        message["reasoning_content"] = reasoning
    return httpx.Response(
        200,
        json={"choices": [{"message": message}]},
        request=httpx.Request("POST", "http://x"),
    )


def error_response(status, headers=None):
    return httpx.Response(
        status,
        json={"error": "nope"},
        headers=headers or {},
        request=httpx.Request("POST", "http://x"),
    )


class FakeTransport:
    def __init__(self, responses):
        self.responses = list(responses)
        self.calls = []

    async def __call__(self, lane, payload, headers, base_url):
        self.calls.append(
            {
                "lane": dict(lane),
                "payload": json.loads(json.dumps(payload)),
                "headers": dict(headers),
                "base_url": base_url,
            }
        )
        item = self.responses.pop(0)
        if isinstance(item, Exception):
            raise item
        return item


MESSAGES = [{"role": "user", "content": "ping"}]


async def test_failover_on_429_sets_cooldown_and_uses_next_lane(db_path):
    lane1 = add_lane(db_path, "groq_1", "CLUSTER_FAST", model="m1")
    add_lane(db_path, "cerebras_1", "CLUSTER_FAST", model="m2")
    router = SmartRouter(db_path=db_path, transport=FakeTransport([error_response(429, {"Retry-After": "2"}), ok_response("from lane2")]))

    result = await router.chat(MESSAGES, "CLUSTER_FAST")

    assert result.provider == "cerebras_1"
    assert result.model == "m2"
    assert result.content == "from lane2"
    assert result.attempts == 2
    row = lane_row(db_path, lane1)
    assert row["is_cooldown"] == 1
    assert 0 < cooldown_seconds(row) <= 3
    await router.aclose()


async def test_503_without_header_uses_default_cooldown(db_path):
    lane1 = add_lane(db_path, "groq_1", "CLUSTER_FAST")
    add_lane(db_path, "cerebras_1", "CLUSTER_FAST")
    router = SmartRouter(db_path=db_path, transport=FakeTransport([error_response(503), ok_response("ok")]))

    result = await router.chat(MESSAGES, "CLUSTER_FAST")

    assert result.provider == "cerebras_1"
    assert 25 <= cooldown_seconds(lane_row(db_path, lane1)) <= 31
    await router.aclose()


async def test_auth_error_locks_lane_for_30_minutes(db_path):
    lane1 = add_lane(db_path, "groq_1", "CLUSTER_FAST")
    add_lane(db_path, "cerebras_1", "CLUSTER_FAST")
    router = SmartRouter(db_path=db_path, transport=FakeTransport([error_response(401), ok_response("ok")]))

    await router.chat(MESSAGES, "CLUSTER_FAST")

    assert 1700 < cooldown_seconds(lane_row(db_path, lane1)) <= 1801
    await router.aclose()


async def test_server_error_does_not_trigger_cooldown(db_path):
    lane1 = add_lane(db_path, "groq_1", "CLUSTER_FAST")
    add_lane(db_path, "cerebras_1", "CLUSTER_FAST")
    router = SmartRouter(db_path=db_path, transport=FakeTransport([error_response(500), ok_response("ok")]))

    result = await router.chat(MESSAGES, "CLUSTER_FAST")

    assert result.provider == "cerebras_1"
    assert lane_row(db_path, lane1)["is_cooldown"] == 0
    await router.aclose()


async def test_transport_error_does_not_trigger_cooldown(db_path):
    lane1 = add_lane(db_path, "groq_1", "CLUSTER_FAST")
    add_lane(db_path, "cerebras_1", "CLUSTER_FAST")
    transport = FakeTransport([httpx.ConnectError("boom"), ok_response("ok")])
    router = SmartRouter(db_path=db_path, transport=transport)

    result = await router.chat(MESSAGES, "CLUSTER_FAST")

    assert result.provider == "cerebras_1"
    assert lane_row(db_path, lane1)["is_cooldown"] == 0
    await router.aclose()


async def test_all_keys_missing_raises_no_keys(db_path):
    add_lane(db_path, "groq_1", "CLUSTER_FAST", env_key="TEST_MISSING_1")
    add_lane(db_path, "groq_2", "CLUSTER_FAST", env_key="TEST_MISSING_2")
    router = SmartRouter(db_path=db_path, transport=FakeTransport([]))

    with pytest.raises(RouterError) as excinfo:
        await router.chat(MESSAGES, "CLUSTER_FAST")

    assert excinfo.value.code == "NO_KEYS"
    await router.aclose()


async def test_all_lanes_in_cooldown_raises_all_cooldown(db_path):
    until = (datetime.now(timezone.utc) + timedelta(minutes=10)).strftime(TS_FORMAT)
    lane1 = add_lane(db_path, "groq_1", "CLUSTER_FAST")
    conn = get_conn(db_path)
    with conn:
        conn.execute("UPDATE router_api_pool SET is_cooldown = 1, cooldown_until = ? WHERE id = ?", (until, lane1))
    conn.close()
    router = SmartRouter(db_path=db_path, transport=FakeTransport([]))

    with pytest.raises(RouterError) as excinfo:
        await router.chat(MESSAGES, "CLUSTER_FAST")

    assert excinfo.value.code == "ALL_COOLDOWN"
    await router.aclose()


async def test_all_attempts_failed_raises_all_failed(db_path):
    add_lane(db_path, "groq_1", "CLUSTER_FAST")
    add_lane(db_path, "cerebras_1", "CLUSTER_FAST")
    router = SmartRouter(db_path=db_path, transport=FakeTransport([error_response(500), error_response(500)]))

    with pytest.raises(RouterError) as excinfo:
        await router.chat(MESSAGES, "CLUSTER_FAST")

    assert excinfo.value.code == "ALL_FAILED"
    await router.aclose()


async def test_unknown_cluster_is_rejected(db_path):
    router = SmartRouter(db_path=db_path, transport=FakeTransport([]))
    with pytest.raises(RouterError) as excinfo:
        await router.chat(MESSAGES, "CLUSTER_NOPE")
    assert excinfo.value.code == "UNKNOWN_CLUSTER"
    await router.aclose()


async def test_expired_cooldown_is_cleared_and_lane_reused(db_path):
    past = (datetime.now(timezone.utc) - timedelta(minutes=5)).strftime(TS_FORMAT)
    lane1 = add_lane(db_path, "groq_1", "CLUSTER_FAST")
    conn = get_conn(db_path)
    with conn:
        conn.execute("UPDATE router_api_pool SET is_cooldown = 1, cooldown_until = ? WHERE id = ?", (past, lane1))
    conn.close()
    router = SmartRouter(db_path=db_path, transport=FakeTransport([ok_response("revived")]))

    result = await router.chat(MESSAGES, "CLUSTER_FAST")

    assert result.provider == "groq_1"
    assert lane_row(db_path, lane1)["is_cooldown"] == 0
    await router.aclose()


async def test_success_increments_daily_counter(db_path):
    lane1 = add_lane(db_path, "groq_1", "CLUSTER_FAST")
    router = SmartRouter(db_path=db_path, transport=FakeTransport([ok_response(), ok_response()]))

    await router.chat(MESSAGES, "CLUSTER_FAST")
    assert lane_row(db_path, lane1)["total_calls_today"] == 1
    await router.chat(MESSAGES, "CLUSTER_FAST")
    assert lane_row(db_path, lane1)["total_calls_today"] == 2
    await router.aclose()


async def test_daily_counter_resets_on_date_change(db_path):
    lane1 = add_lane(db_path, "groq_1", "CLUSTER_FAST")
    conn = get_conn(db_path)
    with conn:
        conn.execute("UPDATE router_api_pool SET total_calls_today = 5 WHERE id = ?", (lane1,))
    conn.close()

    router = SmartRouter(db_path=db_path, transport=FakeTransport([ok_response()]))
    router._last_counter_date = "1970-01-01"

    await router.chat(MESSAGES, "CLUSTER_FAST")

    assert lane_row(db_path, lane1)["total_calls_today"] == 1
    await router.aclose()


async def test_json_mode_sets_response_format(db_path):
    add_lane(db_path, "groq_1", "CLUSTER_FAST")
    transport = FakeTransport([ok_response("{}")])
    router = SmartRouter(db_path=db_path, transport=transport)

    await router.chat(MESSAGES, "CLUSTER_FAST", json_mode=True)

    assert transport.calls[0]["payload"]["response_format"] == {"type": "json_object"}
    await router.aclose()


async def test_reasoning_content_is_used_when_content_empty(db_path):
    add_lane(db_path, "openrouter_1", "CLUSTER_REASONING")
    router = SmartRouter(db_path=db_path, transport=FakeTransport([ok_response(content="", reasoning="deep thought")]))

    result = await router.chat(MESSAGES, "CLUSTER_REASONING")

    assert result.content == "deep thought"
    await router.aclose()


async def test_retry_after_http_date_is_parsed(db_path):
    lane1 = add_lane(db_path, "groq_1", "CLUSTER_FAST")
    add_lane(db_path, "cerebras_1", "CLUSTER_FAST")
    future = datetime.now(timezone.utc) + timedelta(seconds=120)
    header = email.utils.format_datetime(future)
    router = SmartRouter(db_path=db_path, transport=FakeTransport([error_response(429, {"Retry-After": header}), ok_response("ok")]))

    await router.chat(MESSAGES, "CLUSTER_FAST")

    assert 100 <= cooldown_seconds(lane_row(db_path, lane1)) <= 122
    await router.aclose()


async def test_cooldown_status_lists_locked_lanes(db_path):
    add_lane(db_path, "groq_1", "CLUSTER_FAST")
    add_lane(db_path, "cerebras_1", "CLUSTER_FAST")
    router = SmartRouter(db_path=db_path, transport=FakeTransport([error_response(429, {"Retry-After": "5"}), ok_response("ok")]))

    await router.chat(MESSAGES, "CLUSTER_FAST")

    status = router.cooldown_status()
    assert len(status) == 1
    assert status[0]["provider_name"] == "groq_1"
    await router.aclose()


async def test_keyless_lane_needs_no_authorization_header(db_path):
    add_lane(db_path, "ollama_local", "CLUSTER_CODE", model="qwen2.5-coder:7b", env_key="")
    transport = FakeTransport([ok_response("local")])
    router = SmartRouter(db_path=db_path, transport=transport)

    result = await router.chat(MESSAGES, "CLUSTER_CODE")

    assert result.provider == "ollama_local"
    assert "Authorization" not in transport.calls[0]["headers"]
    assert transport.calls[0]["base_url"].endswith("/v1")
    await router.aclose()


def test_parse_retry_after_variants():
    assert parse_retry_after("7", 60) == 7
    assert parse_retry_after(None, 60) == 60
    assert parse_retry_after("garbage", 60) == 60
    past = email.utils.format_datetime(datetime.now(timezone.utc) - timedelta(seconds=30))
    assert parse_retry_after(past, 60) == 1
