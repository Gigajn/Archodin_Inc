"""Smart 9Router — free-tier LLM routing with cooldown-aware failover.

Implements FR-RTR-01 (task classification into four model clusters) and
FR-RTR-02 (zero-downtime failover: a 429/503 moves the request to the next lane
without sleeping, and locks the limited lane into cooldown per ``Retry-After``).

Every provider in the seeded pool speaks the OpenAI-compatible
``/chat/completions`` protocol, so one transport covers all of them.
"""

from __future__ import annotations

import datetime as dt
import email.utils
import logging
import os
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Awaitable, Callable

import httpx

from db import get_conn

logger = logging.getLogger("aahc.router")

VALID_CLUSTERS = ("CLUSTER_CODE", "CLUSTER_FAST", "CLUSTER_REASONING", "CLUSTER_VISION")

PROVIDER_BASE_URLS: dict[str, str] = {
    "groq": "https://api.groq.com/openai/v1",
    "openrouter": "https://openrouter.ai/api/v1",
    "cerebras": "https://api.cerebras.ai/v1",
    "gemini": "https://generativelanguage.googleapis.com/v1beta/openai",
}

OLLAMA_DEFAULT_BASE = "http://localhost:11434"

TS_FORMAT = "%Y-%m-%d %H:%M:%S"

DEFAULT_COOLDOWN_429 = 60
DEFAULT_COOLDOWN_503 = 30
COOLDOWN_AUTH = 1800

# Injected in tests: (lane, payload, headers, base_url) -> httpx.Response
Transport = Callable[[dict[str, Any], dict[str, Any], dict[str, str], str], Awaitable[httpx.Response]]


@dataclass
class RouterResult:
    content: str
    provider: str
    model: str
    cluster: str
    latency_ms: int
    attempts: int


class RouterError(RuntimeError):
    """Typed routing failure. ``code`` is one of NO_KEYS / ALL_COOLDOWN / ALL_FAILED /
    UNKNOWN_CLUSTER / UNKNOWN_PROVIDER."""

    def __init__(self, code: str, message: str = "") -> None:
        super().__init__(f"{code}: {message}" if message else code)
        self.code = code


def _now() -> dt.datetime:
    return dt.datetime.now(dt.timezone.utc)


def _format_ts(moment: dt.datetime) -> str:
    return moment.strftime(TS_FORMAT)


def _parse_ts(value: str | None) -> dt.datetime | None:
    if not value:
        return None
    try:
        return dt.datetime.strptime(value, TS_FORMAT).replace(tzinfo=dt.timezone.utc)
    except ValueError:
        return None


def parse_retry_after(value: str | None, default_seconds: int) -> int:
    """Parse a ``Retry-After`` header: integer seconds or an HTTP-date."""
    if not value:
        return default_seconds
    text = value.strip()
    if text.isdigit():
        return max(1, int(text))
    try:
        parsed = email.utils.parsedate_to_datetime(text)
    except (TypeError, ValueError):
        return default_seconds
    if parsed is None:
        return default_seconds
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=dt.timezone.utc)
    delta = (parsed - _now()).total_seconds()
    return max(1, int(delta)) if delta > 0 else 1


class SmartRouter:
    """Routes chat completions across the free-tier pool stored in ``router_api_pool``."""

    PROVIDER_BASE_URLS = PROVIDER_BASE_URLS

    def __init__(
        self,
        db_path: str | Path | None = None,
        transport: Transport | None = None,
        client: httpx.AsyncClient | None = None,
    ) -> None:
        self.db_path = db_path
        self._transport = transport
        self._client = client
        self._last_counter_date: str | None = None

    # ------------------------------------------------------------------ internal

    def _conn(self):
        return get_conn(self.db_path)

    def _ensure_client(self) -> httpx.AsyncClient:
        if self._client is None:
            self._client = httpx.AsyncClient(timeout=httpx.Timeout(60.0, connect=5.0))
        return self._client

    @staticmethod
    def _base_url(provider_name: str) -> str:
        for family, base in PROVIDER_BASE_URLS.items():
            if provider_name == family or provider_name.startswith(f"{family}_"):
                return base
        if provider_name == "ollama" or provider_name.startswith("ollama_"):
            base = os.environ.get("OLLAMA_BASE_URL", OLLAMA_DEFAULT_BASE)
            return f"{base.rstrip('/')}/v1"
        raise RouterError("UNKNOWN_PROVIDER", provider_name)

    def _maybe_reset_daily_counters(self) -> None:
        today = _now().strftime("%Y-%m-%d")
        if self._last_counter_date == today:
            return
        conn = self._conn()
        try:
            conn.execute("UPDATE router_api_pool SET total_calls_today = 0")
            conn.commit()
        finally:
            conn.close()
        self._last_counter_date = today
        logger.info("router: daily call counters reset for %s", today)

    def _load_lanes(self, cluster: str) -> tuple[list[dict[str, Any]], int, int]:
        """Return (usable lanes in priority order, skipped-by-cooldown, total rows)."""
        now = _now()
        conn = self._conn()
        try:
            rows = conn.execute(
                "SELECT * FROM router_api_pool WHERE cluster_type = ? ORDER BY id",
                (cluster,),
            ).fetchall()
            lanes: list[dict[str, Any]] = []
            skipped_cooldown = 0
            stale_ids: list[int] = []
            for row in rows:
                lane = dict(row)
                if lane.get("is_cooldown") and lane.get("cooldown_until"):
                    until = _parse_ts(lane["cooldown_until"])
                    if until is not None and until > now:
                        skipped_cooldown += 1
                        continue
                    stale_ids.append(int(lane["id"]))
                lanes.append(lane)
            for lane_id in stale_ids:
                conn.execute(
                    "UPDATE router_api_pool SET is_cooldown = 0, cooldown_until = NULL WHERE id = ?",
                    (lane_id,),
                )
            if stale_ids:
                conn.commit()
            return lanes, skipped_cooldown, len(rows)
        finally:
            conn.close()

    @staticmethod
    def _resolve_key(lane: dict[str, Any]) -> tuple[str | None, bool]:
        """Return (key, usable). ``(None, True)`` means a keyless lane."""
        name = (lane.get("env_key_name") or "").strip()
        if not name:
            return None, True
        value = os.environ.get(name, "").strip()
        if not value:
            return None, False
        return value, True

    async def _call_lane(
        self,
        lane: dict[str, Any],
        payload: dict[str, Any],
        headers: dict[str, str],
        base_url: str,
    ) -> httpx.Response:
        if self._transport is not None:
            return await self._transport(lane, payload, headers, base_url)
        client = self._ensure_client()
        return await client.post(f"{base_url}/chat/completions", json=payload, headers=headers)

    def _mark_cooldown(self, lane_id: int, seconds: int) -> None:
        until = _format_ts(_now() + dt.timedelta(seconds=max(1, int(seconds))))
        conn = self._conn()
        try:
            conn.execute(
                "UPDATE router_api_pool SET is_cooldown = 1, cooldown_until = ? WHERE id = ?",
                (until, lane_id),
            )
            conn.commit()
        finally:
            conn.close()

    def _bump_calls(self, lane_id: int) -> None:
        conn = self._conn()
        try:
            conn.execute(
                "UPDATE router_api_pool SET total_calls_today = total_calls_today + 1 WHERE id = ?",
                (lane_id,),
            )
            conn.commit()
        finally:
            conn.close()

    @staticmethod
    def _extract_content(response: httpx.Response) -> str:
        try:
            data = response.json()
        except ValueError:
            return ""
        choices = data.get("choices") or []
        if not choices:
            return ""
        message = choices[0].get("message") or {}
        content = message.get("content")
        if content:
            return str(content)
        reasoning = message.get("reasoning_content")
        return str(reasoning) if reasoning else ""

    # -------------------------------------------------------------------- public

    async def chat(
        self,
        messages: list[dict[str, Any]],
        cluster: str,
        *,
        temperature: float = 0.3,
        max_tokens: int = 2048,
        json_mode: bool = False,
        max_attempts: int = 10,
    ) -> RouterResult:
        if cluster not in VALID_CLUSTERS:
            raise RouterError("UNKNOWN_CLUSTER", cluster)

        started = time.perf_counter()
        self._maybe_reset_daily_counters()
        lanes, skipped_cooldown, total_rows = self._load_lanes(cluster)

        payload: dict[str, Any] = {
            "model": None,  # filled per lane
            "messages": messages,
            "temperature": temperature,
            "max_tokens": max_tokens,
        }
        if json_mode:
            payload["response_format"] = {"type": "json_object"}

        attempts = 0
        skipped_no_key = 0
        last_error = ""

        for lane in lanes:
            if attempts >= max_attempts:
                break
            key, usable = self._resolve_key(lane)
            if not usable:
                skipped_no_key += 1
                logger.warning(
                    "router: skipping %s/%s — env %s is not set",
                    lane["provider_name"],
                    lane["model_identifier"],
                    lane["env_key_name"],
                )
                continue

            headers = {"Content-Type": "application/json"}
            if key is not None:
                headers["Authorization"] = f"Bearer {key}"
            lane_payload = dict(payload)
            lane_payload["model"] = lane["model_identifier"]

            attempts += 1
            try:
                base_url = self._base_url(lane["provider_name"])
                response = await self._call_lane(lane, lane_payload, headers, base_url)
            except httpx.HTTPError as exc:
                last_error = f"{type(exc).__name__}: {exc}"
                logger.warning("router: lane %s transport error: %s", lane["provider_name"], last_error)
                continue

            status = response.status_code
            if status == 200:
                self._bump_calls(int(lane["id"]))
                latency_ms = int((time.perf_counter() - started) * 1000)
                return RouterResult(
                    content=self._extract_content(response),
                    provider=str(lane["provider_name"]),
                    model=str(lane["model_identifier"]),
                    cluster=cluster,
                    latency_ms=latency_ms,
                    attempts=attempts,
                )

            if status in (429, 503):
                default = DEFAULT_COOLDOWN_429 if status == 429 else DEFAULT_COOLDOWN_503
                seconds = parse_retry_after(response.headers.get("Retry-After"), default)
                self._mark_cooldown(int(lane["id"]), seconds)
                last_error = f"HTTP {status} on {lane['provider_name']} (cooldown {seconds}s)"
                logger.warning("router: %s", last_error)
                continue

            if status in (401, 403):
                self._mark_cooldown(int(lane["id"]), COOLDOWN_AUTH)
                last_error = f"HTTP {status} on {lane['provider_name']} (auth, cooldown {COOLDOWN_AUTH}s)"
                logger.warning("router: %s", last_error)
                continue

            last_error = f"HTTP {status} on {lane['provider_name']}"
            logger.warning("router: %s", last_error)

        if attempts == 0:
            if total_rows == 0:
                raise RouterError("ALL_FAILED", f"no lanes configured for {cluster}")
            if skipped_no_key > 0:
                raise RouterError("NO_KEYS", f"no API keys available for {cluster}")
            if skipped_cooldown > 0:
                raise RouterError("ALL_COOLDOWN", f"all {skipped_cooldown} lanes of {cluster} are cooling down")
            raise RouterError("ALL_FAILED", f"no usable lane for {cluster}")

        raise RouterError("ALL_FAILED", f"{attempts} lane(s) failed for {cluster}; last: {last_error}")

    def cooldown_status(self) -> list[dict[str, Any]]:
        conn = self._conn()
        try:
            rows = conn.execute(
                """
                SELECT provider_name, cluster_type, model_identifier, cooldown_until
                FROM router_api_pool
                WHERE is_cooldown = 1
                ORDER BY id
                """
            ).fetchall()
            return [dict(row) for row in rows]
        finally:
            conn.close()

    async def aclose(self) -> None:
        if self._client is not None:
            await self._client.aclose()
            self._client = None
