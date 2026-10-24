"""AI HRD Architect — turns a business brief into a working division of agents.

Implements FR-HRD-01 (auto-provision a division from a text brief), FR-HRD-02
(auto-layout of 2D desks inside the division's room bounds) and FR-HRD-03 (fast
cloning of trading desks, one employee per pair, supervised by a CRO).
"""

from __future__ import annotations

import json
import logging
import re
from typing import TYPE_CHECKING, Any, Literal

from pydantic import BaseModel, Field, ValidationError

from db import get_conn

if TYPE_CHECKING:  # pragma: no cover - importing is only needed for typing
    from smart_9router import SmartRouter

logger = logging.getLogger("aahc.hr")

ALLOWED_TOOLS: list[str] = [
    "github_search",
    "crawl_web",
    "pypi_install",
    "docker_bash",
    "ccxt_market_data",
    "risk_formulas",
    "rag_search",
    "web_search",
    "dataset_hub",
    "file_write",
]

SPRITE_PALETTE: list[str] = [
    "char_brown_hair_white_shirt",
    "char_black_hair_blue_shirt",
    "char_blonde_hair_red_shirt",
    "char_dark_hair_green_shirt",
    "char_white_hair_purple_shirt",
]

DEFAULT_ROOM_BOUNDS: dict[str, int] = {"x": 0, "y": 0, "w": 12, "h": 10}

TRADING_DIVISION_NAME = "Quant Trading Floor"
RISK_MANAGER_NAME = "CRO Agent"

_CLUSTER_RULES: tuple[tuple[tuple[str, ...], str], ...] = (
    (("vision", "image", "design"), "CLUSTER_VISION"),
    (("developer", "engineer", "programmer", "qa", "tester", "ml", "data"), "CLUSTER_CODE"),
    (("trader", "risk", "architect", "hr", "analyst", "quant"), "CLUSTER_REASONING"),
    (("researcher", "market", "writer", "copywriter", "cs", "support", "affiliate", "media"), "CLUSTER_FAST"),
)

_SYSTEM_INSTRUCTION = """You are the AI HRD Architect of an autonomous AI holding company.
Given a business goal, design the optimal division: the agents, their prompts, the
SOP chain between them, and the execution mode.

Return ONLY one JSON object. No prose, no markdown fences. Exact shape:
{{
  "name": "<division name>",
  "floor_number": <integer>,
  "execution_mode": "DAG_DEV" | "STREAM_TRADE" | "WEBHOOK_CS" | "CRON_BATCH",
  "cron_schedule": "<cron or null>",
  "room_grid_bounds": {{"x": 0, "y": 0, "w": 12, "h": 10}},
  "employees": [
    {{
      "name": "<short human name>",
      "role_title": "<role title>",
      "system_prompt": "<detailed operating prompt for this agent>",
      "model_cluster": "CLUSTER_CODE" | "CLUSTER_FAST" | "CLUSTER_REASONING" | "CLUSTER_VISION",
      "allowed_tools": ["<tool>", ...],
      "assigned_pair": "<pair or null>"
    }}
  ],
  "workflow_steps": [
    {{"step_order": 1, "from_name": "<employee name>", "to_name": "<employee name>", "validation_rule": "<rule or null>"}}
  ]
}}

Rules:
- allowed_tools may ONLY contain values from this list: {tools}
- every workflow step's from_name/to_name MUST match an employee "name" in this same payload
- use assigned_pair only for trading desks (one pair per agent), otherwise null
- return at least one employee
""".format(tools=", ".join(ALLOWED_TOOLS))


class EmployeeBlueprint(BaseModel):
    name: str
    role_title: str
    system_prompt: str
    model_cluster: Literal["CLUSTER_CODE", "CLUSTER_FAST", "CLUSTER_REASONING", "CLUSTER_VISION"]
    allowed_tools: list[str] = Field(default_factory=list)
    assigned_pair: str | None = None


class WorkflowStepBlueprint(BaseModel):
    step_order: int
    from_name: str | None = None
    to_name: str | None = None
    validation_rule: str | None = None


class DivisionBlueprint(BaseModel):
    name: str
    floor_number: int
    execution_mode: Literal["DAG_DEV", "STREAM_TRADE", "WEBHOOK_CS", "CRON_BATCH"]
    cron_schedule: str | None = None
    room_grid_bounds: dict[str, int] = Field(default_factory=lambda: dict(DEFAULT_ROOM_BOUNDS))
    employees: list[EmployeeBlueprint] = Field(min_length=1)
    workflow_steps: list[WorkflowStepBlueprint] = Field(default_factory=list)


# --------------------------------------------------------------------------- helpers


def slugify(value: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "_", value.strip().lower()).strip("_")
    return slug or "division"


def _strip_fences(text: str) -> str:
    cleaned = (text or "").strip()
    match = re.search(r"```(?:json)?\s*(.*?)```", cleaned, re.DOTALL)
    if match:
        cleaned = match.group(1).strip()
    return cleaned


def _infer_cluster(*texts: str) -> str:
    haystack = " ".join(t for t in texts if t).lower()
    for keywords, cluster in _CLUSTER_RULES:
        if any(keyword in haystack for keyword in keywords):
            return cluster
    return "CLUSTER_FAST"


def _normalise_payload(data: dict[str, Any]) -> dict[str, Any]:
    """Fill gaps the LLM may leave so validation has a fair chance."""
    payload = dict(data)
    bounds = payload.get("room_grid_bounds")
    if not isinstance(bounds, dict):
        payload["room_grid_bounds"] = dict(DEFAULT_ROOM_BOUNDS)

    mode = payload.get("execution_mode")
    if isinstance(mode, str) and mode.strip():
        payload["execution_mode"] = mode.strip().upper()

    employees: list[dict[str, Any]] = []
    for raw in payload.get("employees") or []:
        if not isinstance(raw, dict):
            continue
        employee = dict(raw)
        if not employee.get("model_cluster"):
            employee["model_cluster"] = _infer_cluster(
                str(employee.get("role_title") or ""), str(employee.get("name") or "")
            )
        else:
            cluster = str(employee["model_cluster"]).strip().upper()
            if not cluster.startswith("CLUSTER_"):
                cluster = f"CLUSTER_{cluster}"
            employee["model_cluster"] = cluster
        tools = employee.get("allowed_tools") or []
        kept: list[str] = []
        for tool in tools:
            if tool in ALLOWED_TOOLS:
                if tool not in kept:
                    kept.append(tool)
            else:
                logger.warning("hr: dropping unknown tool %r from %s", tool, employee.get("name"))
        employee["allowed_tools"] = kept
        employee.setdefault("assigned_pair", None)
        employees.append(employee)
    payload["employees"] = employees

    steps: list[dict[str, Any]] = []
    for raw in payload.get("workflow_steps") or []:
        if not isinstance(raw, dict):
            continue
        step = dict(raw)
        step.setdefault("from_name", None)
        step.setdefault("to_name", None)
        step.setdefault("validation_rule", None)
        steps.append(step)
    payload["workflow_steps"] = steps
    return payload


def _layout_cells(bounds: dict[str, Any], count: int) -> list[tuple[int, int]]:
    """Pick ``count`` non-overlapping desk cells inside ``bounds``.

    Desks sit on a coarse grid: rows every 2 cells starting at ``y+2``, columns
    every 3 cells starting at ``x+2``. Columns ``x`` and ``x+1`` stay free as the
    walking aisle. If the room fills up the stored height grows instead of
    overlapping two employees.
    """
    x0 = int(bounds.get("x", 0))
    y0 = int(bounds.get("y", 0))
    width = int(bounds.get("w", 12))
    height = int(bounds.get("h", 10))

    cells: list[tuple[int, int]] = []
    y = y0 + 2
    while len(cells) < count:
        if y > y0 + height - 1:
            height += 2
            bounds["h"] = height
        x = x0 + 2
        while x <= x0 + width - 1 and len(cells) < count:
            cells.append((x, y))
            x += 3
        y += 2
    return cells


def _occupied_cells(conn: Any, division_id: str) -> set[tuple[int, int]]:
    rows = conn.execute(
        "SELECT desk_coord_x, desk_coord_y FROM employees WHERE division_id = ?",
        (division_id,),
    ).fetchall()
    return {(int(row["desk_coord_x"]), int(row["desk_coord_y"])) for row in rows}


def _unique_division_id(conn: Any, base: str) -> str:
    candidate = f"div_{base}"
    suffix = 1
    while conn.execute("SELECT 1 FROM divisions WHERE id = ?", (candidate,)).fetchone():
        suffix += 1
        candidate = f"div_{base}_{suffix}"
    return candidate


def _next_floor_number(conn: Any) -> int:
    row = conn.execute("SELECT COALESCE(MAX(floor_number), 0) AS top FROM divisions").fetchone()
    return int(row["top"]) + 1


def _trader_prompt(pair: str, name: str) -> str:
    return (
        f"You are {name}, the dedicated desk trader for {pair} at the AAHC Quant Trading Floor.\n"
        f"- Your market analysis is scoped to {pair} ONLY. Never mix in other instruments.\n"
        f"- You may NOT place orders. Your only output is a JSON OrderProposal with the fields "
        f'"pair", "side", "entry", "stop_loss", "take_profit", "confidence", "reason".\n'
        f"- Every proposal goes to the Risk Manager (CRO), who alone is authorised to execute on the exchange.\n"
        f"- If your confidence is below the desk threshold, output no proposal and explain why."
    )


def _risk_manager_prompt() -> str:
    return (
        f"You are {RISK_MANAGER_NAME}, the Chief Risk Officer of the AAHC Quant Trading Floor.\n"
        "- You receive JSON OrderProposals from the desk traders.\n"
        "- You validate each proposal with the position sizing formula "
        "Position Size = (Total Equity * Max Risk Per Trade %) / |Entry Price - Stop Loss Price|.\n"
        "- You alone may call the exchange execution tool, and only after the proposal passes validation.\n"
        "- Any proposal risking more than the configured max risk per trade must be rejected with a reason."
    )


# -------------------------------------------------------------------------- generate


async def generate_division_blueprint(business_brief: str, router: "SmartRouter") -> DivisionBlueprint:
    """Ask the reasoning cluster for a division blueprint and validate it."""
    messages: list[dict[str, Any]] = [
        {"role": "system", "content": _SYSTEM_INSTRUCTION},
        {"role": "user", "content": business_brief},
    ]
    last_error: Exception | None = None

    for _ in range(2):
        result = await router.chat(
            messages=messages,
            cluster="CLUSTER_REASONING",
            temperature=0.2,
            json_mode=True,
        )
        raw = _strip_fences(result.content)
        try:
            data = json.loads(raw)
            if not isinstance(data, dict):
                raise ValueError("blueprint is not a JSON object")
            blueprint = DivisionBlueprint.model_validate(_normalise_payload(data))
            return blueprint
        except (json.JSONDecodeError, ValidationError, ValueError) as exc:
            last_error = exc
            logger.warning("hr: invalid blueprint from LLM: %s", exc)
            messages = messages + [
                {"role": "assistant", "content": result.content},
                {
                    "role": "user",
                    "content": f"Your previous answer was invalid: {exc}. Return ONLY valid JSON in the required shape.",
                },
            ]

    raise ValueError(f"LLM returned invalid blueprint: {last_error}")


# ---------------------------------------------------------------------------- deploy


def deploy_division(
    blueprint: DivisionBlueprint,
    db_path: str | None = None,
    conn: Any | None = None,
) -> dict[str, Any]:
    """Persist a blueprint: division row, employees with desk cells, workflow steps."""
    owns_conn = conn is None
    if owns_conn:
        conn = get_conn(db_path)
    try:
        division_id = _unique_division_id(conn, slugify(blueprint.name))
        staff = list(blueprint.employees)

        # Layout: takes a copy so the caller's blueprint keeps its own bounds unless it grows.
        bounds = dict(blueprint.room_grid_bounds or DEFAULT_ROOM_BOUNDS)
        for key, value in DEFAULT_ROOM_BOUNDS.items():
            bounds.setdefault(key, value)
        cells = _layout_cells(bounds, len(staff))

        division_slug = division_id.removeprefix("div_")
        employee_ids: dict[str, str] = {}
        desks: dict[str, list[int]] = {}

        with conn:
            conn.execute(
                """
                INSERT INTO divisions (id, name, floor_number, execution_mode, cron_schedule, is_active, room_grid_bounds)
                VALUES (?, ?, ?, ?, ?, 1, ?)
                """,
                (
                    division_id,
                    blueprint.name,
                    blueprint.floor_number,
                    blueprint.execution_mode,
                    blueprint.cron_schedule,
                    json.dumps(bounds),
                ),
            )

            for index, (employee, (x, y)) in enumerate(zip(staff, cells), start=1):
                employee_id = f"emp_{division_slug}_{index:02d}"
                employee_ids[employee.name] = employee_id
                desks[employee.name] = [x, y]
                conn.execute(
                    """
                    INSERT INTO employees
                        (id, division_id, name, role_title, system_prompt, model_cluster,
                         allowed_tools, assigned_pair, sprite_asset, desk_coord_x, desk_coord_y, current_state)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'IDLE')
                    """,
                    (
                        employee_id,
                        division_id,
                        employee.name,
                        employee.role_title,
                        employee.system_prompt,
                        employee.model_cluster,
                        json.dumps(employee.allowed_tools),
                        employee.assigned_pair,
                        SPRITE_PALETTE[(index - 1) % len(SPRITE_PALETTE)],
                        x,
                        y,
                    ),
                )

            for step in sorted(blueprint.workflow_steps, key=lambda s: s.step_order):
                from_id = employee_ids.get(step.from_name) if step.from_name else None
                to_id = employee_ids.get(step.to_name) if step.to_name else None
                if step.from_name and from_id is None:
                    logger.warning("hr: workflow step %s references unknown employee %r", step.step_order, step.from_name)
                    continue
                if step.to_name and to_id is None:
                    logger.warning("hr: workflow step %s references unknown employee %r", step.step_order, step.to_name)
                    continue
                conn.execute(
                    """
                    INSERT INTO workflow_steps (division_id, step_order, from_employee_id, to_employee_id, validation_rule)
                    VALUES (?, ?, ?, ?, ?)
                    """,
                    (division_id, step.step_order, from_id, to_id, step.validation_rule),
                )

        return {"division_id": division_id, "employee_ids": employee_ids, "desks": desks}
    finally:
        if owns_conn:
            conn.close()


# --------------------------------------------------------------------- trading clone


def clone_trading_desk(
    pairs: list[str],
    db_path: str | None = None,
    conn: Any | None = None,
) -> dict[str, Any]:
    """Create one desk trader per pair plus the supervising CRO (idempotent)."""
    owns_conn = conn is None
    if owns_conn:
        conn = get_conn(db_path)
    try:
        row = conn.execute("SELECT * FROM divisions WHERE name = ?", (TRADING_DIVISION_NAME,)).fetchone()
        if row is None:
            division_id = _unique_division_id(conn, slugify(TRADING_DIVISION_NAME))
            bounds = dict(DEFAULT_ROOM_BOUNDS)
            with conn:
                conn.execute(
                    """
                    INSERT INTO divisions (id, name, floor_number, execution_mode, cron_schedule, is_active, room_grid_bounds)
                    VALUES (?, ?, ?, 'STREAM_TRADE', NULL, 1, ?)
                    """,
                    (division_id, TRADING_DIVISION_NAME, _next_floor_number(conn), json.dumps(bounds)),
                )
        else:
            division_id = str(row["id"])
            bounds = json.loads(row["room_grid_bounds"] or json.dumps(DEFAULT_ROOM_BOUNDS))
            for key, value in DEFAULT_ROOM_BOUNDS.items():
                bounds.setdefault(key, value)

        existing = conn.execute(
            "SELECT id, name, assigned_pair, desk_coord_x, desk_coord_y FROM employees WHERE division_id = ?",
            (division_id,),
        ).fetchall()
        occupied = {(int(r["desk_coord_x"]), int(r["desk_coord_y"])) for r in existing}
        by_pair = {str(r["assigned_pair"]): str(r["id"]) for r in existing if r["assigned_pair"]}
        cro_row = next((r for r in existing if r["name"] == RISK_MANAGER_NAME), None)

        def next_cell() -> tuple[int, int]:
            candidates = _layout_cells(bounds, len(occupied) + 1)
            for cell in candidates:
                if cell not in occupied:
                    occupied.add(cell)
                    return cell
            raise RuntimeError("trading floor layout could not find a free desk cell")

        division_slug = division_id.removeprefix("div_")
        next_index = len(existing) + 1
        traders: dict[str, str] = {}
        skipped: list[str] = []

        with conn:
            if cro_row is None:
                risk_manager_id = f"emp_{division_slug}_{next_index:02d}"
                x, y = next_cell()
                conn.execute(
                    """
                    INSERT INTO employees
                        (id, division_id, name, role_title, system_prompt, model_cluster,
                         allowed_tools, assigned_pair, sprite_asset, desk_coord_x, desk_coord_y, current_state)
                    VALUES (?, ?, ?, 'Chief Risk Officer', ?, 'CLUSTER_REASONING', ?, NULL, ?, ?, ?, 'IDLE')
                    """,
                    (
                        risk_manager_id,
                        division_id,
                        RISK_MANAGER_NAME,
                        _risk_manager_prompt(),
                        json.dumps(["risk_formulas", "ccxt_market_data"]),
                        SPRITE_PALETTE[(next_index - 1) % len(SPRITE_PALETTE)],
                        x,
                        y,
                    ),
                )
                next_index += 1
            else:
                risk_manager_id = str(cro_row["id"])

            for pair in pairs:
                if not pair:
                    continue
                if pair in by_pair:
                    skipped.append(pair)
                    continue
                base = pair.split("/")[0]
                name = f"Trader {base}"
                employee_id = f"emp_{division_slug}_{next_index:02d}"
                x, y = next_cell()
                conn.execute(
                    """
                    INSERT INTO employees
                        (id, division_id, name, role_title, system_prompt, model_cluster,
                         allowed_tools, assigned_pair, sprite_asset, desk_coord_x, desk_coord_y, current_state)
                    VALUES (?, ?, ?, ?, ?, 'CLUSTER_FAST', ?, ?, ?, ?, ?, 'IDLE')
                    """,
                    (
                        employee_id,
                        division_id,
                        name,
                        f"Desk Trader - {pair}",
                        _trader_prompt(pair, name),
                        json.dumps(["ccxt_market_data", "risk_formulas"]),
                        pair,
                        SPRITE_PALETTE[(next_index - 1) % len(SPRITE_PALETTE)],
                        x,
                        y,
                    ),
                )
                conn.execute(
                    """
                    INSERT INTO workflow_steps (division_id, step_order, from_employee_id, to_employee_id, validation_rule)
                    VALUES (?, ?, ?, ?, 'risk_manager_approved')
                    """,
                    (division_id, next_index, employee_id, risk_manager_id),
                )
                traders[pair] = employee_id
                by_pair[pair] = employee_id
                next_index += 1

            conn.execute(
                "UPDATE divisions SET room_grid_bounds = ? WHERE id = ?",
                (json.dumps(bounds), division_id),
            )

        return {
            "division_id": division_id,
            "risk_manager_id": risk_manager_id,
            "traders": traders,
            "skipped": skipped,
        }
    finally:
        if owns_conn:
            conn.close()
