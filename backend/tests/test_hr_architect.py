"""Offline tests for the AI HRD Architect (FR-HRD-01/02/03)."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

import pytest

from db import get_conn
from hr_architect import (
    RISK_MANAGER_NAME,
    SPRITE_PALETTE,
    TRADING_DIVISION_NAME,
    clone_trading_desk,
    deploy_division,
    generate_division_blueprint,
    slugify,
)

SCHEMA = (Path(__file__).resolve().parents[1] / "schema.sql").read_text(encoding="utf-8")


@dataclass
class FakeReply:
    content: str


class FakeRouter:
    def __init__(self, replies):
        self.replies = list(replies)
        self.calls = []

    async def chat(self, messages, cluster, **kwargs):
        self.calls.append({"messages": messages, "cluster": cluster, "kwargs": kwargs})
        reply = self.replies.pop(0)
        if isinstance(reply, Exception):
            raise reply
        return reply


@pytest.fixture()
def db_path(tmp_path):
    path = tmp_path / "hr.db"
    conn = get_conn(path)
    conn.executescript(SCHEMA)
    conn.commit()
    conn.close()
    return path


def employees_spec(count=2, with_cluster=True):
    roles = [
        ("Dev One", "Backend Developer"),
        ("QA One", "QA Tester"),
    ]
    if count > 2:
        roles = roles + [(f"Agent {i}", "Researcher") for i in range(3, count + 1)]
    spec = []
    for name, role in roles[:count]:
        item = {
            "name": name,
            "role_title": role,
            "system_prompt": f"prompt for {name}",
            "allowed_tools": ["docker_bash", "bogus_tool"],
        }
        if with_cluster:
            item["model_cluster"] = "CLUSTER_CODE"
        spec.append(item)
    return spec


def blueprint_payload(name="Web App Squad", employees=None, steps=None, bounds=None):
    return {
        "name": name,
        "floor_number": 1,
        "execution_mode": "DAG_DEV",
        "cron_schedule": None,
        "room_grid_bounds": bounds or {"x": 0, "y": 0, "w": 12, "h": 10},
        "employees": employees if employees is not None else employees_spec(),
        "workflow_steps": steps
        if steps is not None
        else [{"step_order": 1, "from_name": "Dev One", "to_name": "QA One", "validation_rule": "tests_pass"}],
    }


def blueprint_json(fenced=True, **kwargs):
    text = json.dumps(blueprint_payload(**kwargs))
    return f"```json\n{text}\n```" if fenced else text


def division_rows(db_path):
    conn = get_conn(db_path)
    try:
        return [dict(r) for r in conn.execute("SELECT * FROM divisions ORDER BY id").fetchall()]
    finally:
        conn.close()


def employee_rows(db_path, division_id):
    conn = get_conn(db_path)
    try:
        return [
            dict(r)
            for r in conn.execute(
                "SELECT * FROM employees WHERE division_id = ? ORDER BY id", (division_id,)
            ).fetchall()
        ]
    finally:
        conn.close()


def workflow_rows(db_path, division_id):
    conn = get_conn(db_path)
    try:
        return [
            dict(r)
            for r in conn.execute(
                "SELECT * FROM workflow_steps WHERE division_id = ? ORDER BY step_order", (division_id,)
            ).fetchall()
        ]
    finally:
        conn.close()


# ------------------------------------------------------------------------ generate


async def test_generate_success_strips_fences_and_filters_tools():
    router = FakeRouter([FakeReply(blueprint_json())])

    blueprint = await generate_division_blueprint("Build a web app team", router)

    assert len(blueprint.employees) == 2
    assert blueprint.employees[0].allowed_tools == ["docker_bash"]  # bogus_tool dropped
    assert len(router.calls) == 1
    assert router.calls[0]["cluster"] == "CLUSTER_REASONING"
    assert router.calls[0]["kwargs"]["json_mode"] is True
    assert router.calls[0]["kwargs"]["temperature"] == 0.2


async def test_generate_retries_once_then_succeeds():
    router = FakeRouter([FakeReply("this is not json"), FakeReply(blueprint_json())])

    blueprint = await generate_division_blueprint("Build a web app team", router)

    assert len(router.calls) == 2
    assert len(blueprint.employees) == 2
    # the repair turn was appended to the conversation
    assert len(router.calls[1]["messages"]) == 4


async def test_generate_raises_after_two_invalid_replies():
    router = FakeRouter([FakeReply("nope"), FakeReply("still nope")])

    with pytest.raises(ValueError) as excinfo:
        await generate_division_blueprint("Build a web app team", router)

    assert "invalid blueprint" in str(excinfo.value)


async def test_generate_raises_when_no_employees():
    router = FakeRouter([FakeReply(blueprint_json(employees=[])), FakeReply(blueprint_json(employees=[]))])

    with pytest.raises(ValueError):
        await generate_division_blueprint("Build nothing", router)


async def test_missing_model_cluster_is_inferred_from_role():
    employees = [
        {"name": "Data Diver", "role_title": "Data Scientist", "system_prompt": "x", "allowed_tools": []},
    ]
    router = FakeRouter([FakeReply(blueprint_json(employees=employees, steps=[]))])

    blueprint = await generate_division_blueprint("Build an ML team", router)

    assert blueprint.employees[0].model_cluster == "CLUSTER_CODE"


async def test_lowercase_cluster_is_normalised():
    employees = [
        {
            "name": "Vision Guy",
            "role_title": "Image Generator",
            "system_prompt": "x",
            "allowed_tools": [],
            "model_cluster": "vision",
        }
    ]
    router = FakeRouter([FakeReply(blueprint_json(employees=employees, steps=[]))])

    blueprint = await generate_division_blueprint("Build a vision team", router)

    assert blueprint.employees[0].model_cluster == "CLUSTER_VISION"


# -------------------------------------------------------------------------- deploy


async def test_deploy_writes_division_employees_and_steps(db_path):
    router = FakeRouter([FakeReply(blueprint_json())])
    blueprint = await generate_division_blueprint("Build a web app team", router)

    result = deploy_division(blueprint, db_path=db_path)

    divisions = division_rows(db_path)
    assert len(divisions) == 1
    assert divisions[0]["id"] == result["division_id"]
    assert divisions[0]["execution_mode"] == "DAG_DEV"

    staff = employee_rows(db_path, result["division_id"])
    assert len(staff) == 2
    coords = {(r["desk_coord_x"], r["desk_coord_y"]) for r in staff}
    assert len(coords) == 2
    for row in staff:
        assert row["sprite_asset"] in SPRITE_PALETTE
        assert isinstance(json.loads(row["allowed_tools"]), list)
        assert row["current_state"] == "IDLE"
    assert all(x >= 2 for x, _ in coords)  # aisle columns stay free

    steps = workflow_rows(db_path, result["division_id"])
    assert len(steps) == 1
    assert steps[0]["validation_rule"] == "tests_pass"
    assert steps[0]["from_employee_id"] and steps[0]["to_employee_id"]
    assert result["employee_ids"]["Dev One"] == steps[0]["from_employee_id"]


async def test_deploy_uses_distinct_division_ids_on_name_collision(db_path):
    router = FakeRouter([FakeReply(blueprint_json(name="Same Name")), FakeReply(blueprint_json(name="Same Name"))])
    first = deploy_division(await generate_division_blueprint("Same Name", router), db_path=db_path)
    second = deploy_division(await generate_division_blueprint("Same Name", router), db_path=db_path)

    assert first["division_id"] == f"div_{slugify('Same Name')}"
    assert second["division_id"] == f"{first['division_id']}_2"


async def test_layout_keeps_desks_unique_and_grows_the_room(db_path):
    employees = employees_spec(count=60)
    router = FakeRouter([FakeReply(blueprint_json(employees=employees, steps=[]))])
    blueprint = await generate_division_blueprint("Big Team", router)

    result = deploy_division(blueprint, db_path=db_path)

    staff = employee_rows(db_path, result["division_id"])
    coords = {(r["desk_coord_x"], r["desk_coord_y"]) for r in staff}
    assert len(staff) == 60
    assert len(coords) == 60
    bounds = json.loads(division_rows(db_path)[0]["room_grid_bounds"])
    assert bounds["h"] > 10, "room height must grow instead of overlapping desks"


async def test_layout_is_division_local(db_path):
    router = FakeRouter([FakeReply(blueprint_json(name="Floor A")), FakeReply(blueprint_json(name="Floor B"))])
    first = deploy_division(await generate_division_blueprint("Floor A", router), db_path=db_path)
    second = deploy_division(await generate_division_blueprint("Floor B", router), db_path=db_path)

    # Two rooms draw their own grid, so identical local cells are expected.
    assert first["desks"] == second["desks"]


async def test_unknown_workflow_step_is_skipped(db_path):
    steps = [
        {"step_order": 1, "from_name": "Dev One", "to_name": "Ghost Agent", "validation_rule": None},
        {"step_order": 2, "from_name": "Dev One", "to_name": "QA One", "validation_rule": "ok"},
    ]
    router = FakeRouter([FakeReply(blueprint_json(steps=steps))])
    blueprint = await generate_division_blueprint("Team", router)

    result = deploy_division(blueprint, db_path=db_path)

    steps_rows = workflow_rows(db_path, result["division_id"])
    assert len(steps_rows) == 1
    assert steps_rows[0]["step_order"] == 2


# --------------------------------------------------------------------- trading clone


def test_clone_trading_desk_creates_traders_and_one_cro(db_path):
    result = clone_trading_desk(["BTC/USDT", "ETH/USDT"], db_path=db_path)

    assert set(result["traders"]) == {"BTC/USDT", "ETH/USDT"}
    staff = employee_rows(db_path, result["division_id"])
    assert len(staff) == 3  # 2 traders + CRO
    cro = [r for r in staff if r["name"] == RISK_MANAGER_NAME]
    assert len(cro) == 1
    assert cro[0]["id"] == result["risk_manager_id"]
    assert cro[0]["model_cluster"] == "CLUSTER_REASONING"

    traders = [r for r in staff if r["assigned_pair"]]
    assert {r["assigned_pair"] for r in traders} == {"BTC/USDT", "ETH/USDT"}
    assert all("risk_formulas" in r["allowed_tools"] for r in traders)

    steps = workflow_rows(db_path, result["division_id"])
    assert len(steps) == 2
    assert all(step["to_employee_id"] == result["risk_manager_id"] for step in steps)
    assert all(step["validation_rule"] == "risk_manager_approved" for step in steps)


def test_clone_trading_desk_scopes_prompt_to_the_pair(db_path):
    result = clone_trading_desk(["BTC/USDT"], db_path=db_path)

    staff = employee_rows(db_path, result["division_id"])
    trader = next(r for r in staff if r["assigned_pair"] == "BTC/USDT")
    assert "BTC/USDT" in trader["system_prompt"]
    assert "OrderProposal" in trader["system_prompt"]
    assert "may NOT place orders" in trader["system_prompt"]


def test_clone_trading_desk_is_idempotent(db_path):
    clone_trading_desk(["BTC/USDT", "ETH/USDT"], db_path=db_path)
    second = clone_trading_desk(["BTC/USDT", "ETH/USDT"], db_path=db_path)

    assert second["traders"] == {}
    assert second["skipped"] == ["BTC/USDT", "ETH/USDT"]
    staff = employee_rows(db_path, second["division_id"])
    assert len(staff) == 3
    assert len([r for r in staff if r["name"] == RISK_MANAGER_NAME]) == 1


def test_clone_trading_desk_adds_only_the_new_pair(db_path):
    clone_trading_desk(["BTC/USDT"], db_path=db_path)
    result = clone_trading_desk(["BTC/USDT", "SOL/USDT"], db_path=db_path)

    assert set(result["traders"]) == {"SOL/USDT"}
    staff = employee_rows(db_path, result["division_id"])
    assert len(staff) == 3  # CRO + 2 traders
    assert len(division_rows(db_path)) == 1


def test_clone_trading_desk_reuses_existing_floor(db_path):
    first = clone_trading_desk(["BTC/USDT"], db_path=db_path)
    second = clone_trading_desk(["ETH/USDT"], db_path=db_path)

    assert first["division_id"] == second["division_id"]
    assert division_rows(db_path)[0]["name"] == TRADING_DIVISION_NAME


def test_clone_trading_desk_desk_cells_do_not_overlap(db_path):
    result = clone_trading_desk(["BTC/USDT", "ETH/USDT", "SOL/USDT", "XRP/USDT"], db_path=db_path)

    staff = employee_rows(db_path, result["division_id"])
    coords = {(r["desk_coord_x"], r["desk_coord_y"]) for r in staff}
    assert len(coords) == len(staff) == 5
